use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use futures_util::TryStreamExt;
use futures_util::future::{BoxFuture, try_join_all};
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_core::{Data, ModuleId, Node, Params, PlanNode, SlotId, SlotName, Value, ValueMap};

use snapfire_fsr_core::Fingerprint;

use crate::cache::{CacheEntry, LoadCache, NoCache, NoLoadCache, NodeCache};
use crate::ctx::RequestCtx;
use crate::data::{DataSources, LoadError, LoadKeyer, NoLoadKey};
use crate::evaluator::{Chunk, EvalError, Evaluator, NullEvaluator};
use crate::meta::{Head, Meta, Metadata};
use crate::reads::{subtree_shape, Reads, Static, SubtreeReads, DOCUMENT_PROP, PATH_PROP};
use crate::segments::{DefaultKeyer, SegmentInfo, SegmentKeyer};
use crate::store::{Contribution, Seeds, merge_contributions, pending_keys};

#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
  #[error("no data source registered for `{0}`")]
  MissingDataSource(String),
  #[error(transparent)]
  Eval(#[from] EvalError),
  #[error("evaluator asked for slot `{slot}` and plan node {node} has no child there")]
  MissingSlot { node: u32, slot: String },
  #[error("fallback module `{0}` may not contain slots")]
  SlotInFallback(String),
}

/// Module-to-evaluator dispatch. The null evaluator is the fallback, reached
/// through the same trait as every registered one.
pub struct Evaluators {
  rules: Vec<(Box<dyn Fn(&ModuleId) -> bool + Send + Sync>, Arc<dyn Evaluator>)>,
  null: NullEvaluator,
}

impl Default for Evaluators {
  fn default() -> Self {
    Self {
      rules: Vec::new(),
      null: NullEvaluator,
    }
  }
}

impl Evaluators {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn register(
    &mut self,
    applies: impl Fn(&ModuleId) -> bool + Send + Sync + 'static,
    evaluator: Arc<dyn Evaluator>,
  ) {
    self.rules.push((Box::new(applies), evaluator));
  }

  pub fn select(&self, module: &ModuleId) -> &dyn Evaluator {
    for (applies, evaluator) in &self.rules {
      if applies(module) {
        return evaluator.as_ref();
      }
    }
    &self.null
  }

  /// Whether a registered evaluator, rather than the null one, answers `module`.
  pub fn covers(&self, module: &ModuleId) -> bool {
    self.rules.iter().any(|(applies, _)| applies(module))
  }
}

pub struct Runtime {
  pub sources: DataSources,
  pub evaluators: Evaluators,
  pub keyer: Arc<dyn SegmentKeyer>,
  pub cache: Arc<dyn NodeCache>,
  /// What a source reads of the request, so a load can be answered from the
  /// memo below rather than run.
  pub load_keyer: Arc<dyn LoadKeyer>,
  pub loads: Arc<dyn LoadCache>,
  /// Plan nodes whose markup was seen to place the head slot. Such a subtree
  /// is never written to the cache and whether it places the head is a
  /// property of its module rather than of the request, so once one build has
  /// seen it the lookup is known to be dead and is skipped.
  head_users: parking_lot::Mutex<std::collections::HashSet<u32>>,
  /// By data source id: how a segment describes the document from its data.
  pub metas: HashMap<String, Arc<dyn Metadata>>,
  /// The application's say over which parallel slot wins a store key both
  /// seed, as `contribution_order` reads it.
  pub slot_order: Vec<String>,
  /// By module id: the head elements a page rendering that module carries,
  /// which the build settled, a priority image's preload among them.
  pub heads: HashMap<String, Vec<crate::meta::HeadEl>>,
  /// By data source id: what a segment seeds the store with from its data.
  pub stores: HashMap<String, Arc<dyn Seeds>>,
  /// What each subtree reads of the request, by its shape. A subtree with no
  /// entry is taken to read everything.
  pub reads: Reads,
}

pub struct RuntimeBuilder {
  sources: DataSources,
  evaluators: Evaluators,
  keyer: Arc<dyn SegmentKeyer>,
  cache: Arc<dyn NodeCache>,
  load_keyer: Arc<dyn LoadKeyer>,
  loads: Arc<dyn LoadCache>,
  metas: HashMap<String, Arc<dyn Metadata>>,
  heads: HashMap<String, Vec<crate::meta::HeadEl>>,
  stores: HashMap<String, Arc<dyn Seeds>>,
  reads: Reads,
  slot_order: Vec<String>,
}

impl RuntimeBuilder {
  /// The slot names that settle a store key two parallel slots both seed, the later winning; see `contribution_order`.
  pub fn slot_order(mut self, slot_order: Vec<String>) -> Self {
    self.slot_order = slot_order;
    self
  }

  pub fn sources(mut self, sources: DataSources) -> Self {
    self.sources = sources;
    self
  }

  pub fn evaluators(mut self, evaluators: Evaluators) -> Self {
    self.evaluators = evaluators;
    self
  }

  pub fn keyer(mut self, keyer: Arc<dyn SegmentKeyer>) -> Self {
    self.keyer = keyer;
    self
  }

  pub fn cache(mut self, cache: Arc<dyn NodeCache>) -> Self {
    self.cache = cache;
    self
  }

  pub fn load_keyer(mut self, keyer: Arc<dyn LoadKeyer>) -> Self {
    self.load_keyer = keyer;
    self
  }

  pub fn loads(mut self, loads: Arc<dyn LoadCache>) -> Self {
    self.loads = loads;
    self
  }

  pub fn meta(mut self, source_id: impl Into<String>, meta: Arc<dyn Metadata>) -> Self {
    self.metas.insert(source_id.into(), meta);
    self
  }

  /// The head elements a module asks for, by module id.
  pub fn heads(mut self, heads: HashMap<String, Vec<crate::meta::HeadEl>>) -> Self {
    self.heads = heads;
    self
  }

  pub fn store(mut self, source_id: impl Into<String>, seeds: Arc<dyn Seeds>) -> Self {
    self.stores.insert(source_id.into(), seeds);
    self
  }

  /// What each subtree reads of the request, keyed by `subtree_shape`. A
  /// `Fixed` subtree is memoized once for everyone and its props carry no
  /// identity and no token; an `Anonymous` one is memoized per subject.
  pub fn reads(mut self, reads: Reads) -> Self {
    self.reads = reads;
    self
  }

  pub fn build(self) -> Arc<Runtime> {
    Arc::new(Runtime {
      sources: self.sources,
      evaluators: self.evaluators,
      keyer: self.keyer,
      cache: self.cache,
      load_keyer: self.load_keyer,
      loads: self.loads,
      metas: self.metas,
      heads: self.heads,
      stores: self.stores,
      reads: self.reads,
      slot_order: self.slot_order,
      head_users: parking_lot::Mutex::new(std::collections::HashSet::new()),
    })
  }
}

impl Runtime {
  pub fn builder() -> RuntimeBuilder {
    RuntimeBuilder {
      sources: DataSources::new(),
      evaluators: Evaluators::new(),
      keyer: Arc::new(DefaultKeyer),
      cache: Arc::new(NoCache),
      load_keyer: Arc::new(NoLoadKey),
      loads: Arc::new(NoLoadCache),
      metas: HashMap::new(),
      heads: HashMap::new(),
      stores: HashMap::new(),
      reads: Reads::new(),
      slot_order: Vec::new(),
    }
  }

  pub fn new(sources: DataSources, evaluators: Evaluators) -> Arc<Self> {
    Self::builder().sources(sources).evaluators(evaluators).build()
  }

  pub fn with_keyer(sources: DataSources, evaluators: Evaluators, keyer: Arc<dyn SegmentKeyer>) -> Arc<Self> {
    Self::builder()
      .sources(sources)
      .evaluators(evaluators)
      .keyer(keyer)
      .build()
  }
}

/// A deferred slot's eventual content. The future never fails: a failed loader
/// or evaluation resolves to the segment's error node instead.
pub struct PendingResolution {
  pub slot: SlotId,
  /// The deferred segment's key, so its fill is delimited like any region.
  pub key: String,
  pub future: BoxFuture<'static, Resolved>,
}

pub struct Resolved {
  pub slot: SlotId,
  pub key: String,
  pub node: Node,
  /// The child segments of the resolved subtree, positioned in `node`, which
  /// the eager sidecar could not name.
  pub segments: Vec<SegmentInfo>,
  /// Nested deferral: a resolution may introduce new pending slots.
  pub pending: Vec<PendingResolution>,
  /// What the resolved subtree says about the document, when a segment in
  /// it has metadata; the streams patch the title and description with it.
  pub meta: Meta,
  /// What the resolved subtree's own segments seeded; the streams write them
  /// ahead of its markup, so the browser holds them before it mounts anything
  /// rendered from them.
  pub contributions: Vec<Contribution>,
}

pub struct Assembly {
  pub tree: Node,
  pub pending: Vec<PendingResolution>,
  pub segments: SegmentInfo,
  /// The title and description the eager wave settled on, defaults included.
  pub meta: Meta,
  /// What each eager segment seeded, one entry per seeding segment.
  pub contributions: Vec<Contribution>,
  /// The store the eager wave rendered from: `contributions` merged by position.
  pub store: Data,
  pub locale: crate::ctx::Locale,
  /// The head's `entry`: a module the browser loads for this response's islands.
  pub entry: Option<String>,
  /// The head's `styles`: stylesheets this response needs beyond the document's.
  pub styles: Vec<String>,
  /// The head's `catalog`: the locale's message table as JSON.
  pub catalog: Option<String>,
  /// The failure of the route's own page, when its loader failed: the node
  /// reached from the root along the `content` slots. A layout or a slot
  /// failing degrades its segment and leaves this `None`.
  pub failed: Option<FailureKind>,
}

impl std::fmt::Debug for Assembly {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Assembly")
      .field("tree", &self.tree)
      .field("pending", &self.pending.len())
      .field("segments", &self.segments)
      .finish()
  }
}

/// The prop a node's pending store keys ride in beside `$store`, stripped with it.
pub const PENDING_PROP: &str = "$pending";

/// Of the pending keys, the ones a subtree reads; all of them when what it reads is not known.
fn pending_read(pending: &BTreeSet<String>, reads: Option<&SubtreeReads>) -> Vec<String> {
  match reads {
    Some(reads) if reads.class != Static::Dynamic => pending.iter().filter(|k| reads.store_keys.contains(k)).cloned().collect(),
    _ => pending.iter().cloned().collect(),
  }
}

/// A node's props carry the route's store seed as `$store`, which a lowered
/// component's `Expr::Store` reads and the IR evaluator strips again before
/// the props reach the browser.
/// The whole store for a subtree that reads everything, the keys it was seen
/// to read otherwise, so a memoized render depends on what its key names.
fn inject_store(props: &mut Data, store: &Data, pending: &BTreeSet<String>, reads: Option<&SubtreeReads>) {
  let pending = pending_read(pending, reads);
  if !pending.is_empty() {
    props.insert(PENDING_PROP.to_owned(), Value::Seq(pending.into_iter().map(Value::str).collect()));
  }
  let store = match reads {
    Some(reads) if reads.class != Static::Dynamic => store_read(store, &reads.store_keys),
    _ => store.clone(),
  };
  if !store.is_empty() {
    props.insert("$store".to_owned(), Value::Map(store));
  }
}

fn store_read(store: &Data, keys: &[String]) -> Data {
  let mut out = Data::default();
  for key in keys {
    if let Some(value) = store.get(key) {
      out.insert(key.clone(), value.clone());
    }
  }
  out
}

fn error_node(message: &str) -> Node {
  Node::Seq(vec![
    Node::raw("<div data-sf-error>"),
    Node::text(message.to_owned()),
    Node::raw("</div>"),
  ])
}

/// Everything the eager wave produced: resolved data per node and, separately,
/// the loads that failed. A failure never aborts the wave; the segment it
/// belongs to degrades to its error node instead.
struct Loaded {
  data: HashMap<u32, Data>,
  failed: HashMap<u32, LoadError>,
}

fn params_value(params: &Params) -> Value {
  let mut map = ValueMap::default();
  for (k, v) in params {
    map.insert(k.clone(), Value::str(v.clone()));
  }
  Value::Map(map)
}

/// The document's request and the plan nodes that load and are keyed under
/// it: the layouts an intercept keeps, whose data belongs to the page beneath
/// the overlay while their markup is still marked by the address.
pub struct Origin {
  pub ctx: RequestCtx,
  pub nodes: Vec<u32>,
}

struct Session {
  runtime: Arc<Runtime>,
  ctx: RequestCtx,
  origin: Option<Origin>,
  head: Head,
  next_slot: AtomicU32,
}

impl Session {
  /// The request node `id` loads and is keyed under: the document's for a node
  /// the origin names, the navigation's own otherwise.
  fn ctx_of(&self, id: u32) -> &RequestCtx {
    match &self.origin {
      Some(origin) if origin.nodes.contains(&id) => &origin.ctx,
      _ => &self.ctx,
    }
  }
}

/// Every node of `plan` whose loaded data has metadata registered, outermost
/// first so an inner segment folds over an outer one; deferred children
/// excluded since their data is not in this wave.
fn describing_nodes<'p>(
  runtime: &Runtime,
  plan: &'p PlanNode,
  loaded: &Loaded,
  is_root: bool,
  out: &mut Vec<&'p PlanNode>,
) {
  if plan.deferred && !is_root {
    return;
  }
  if let Some(source) = &plan.data_source {
    if runtime.metas.contains_key(&source.0) && loaded.data.contains_key(&plan.id.0) {
      out.push(plan);
    }
  }
  for (_, child) in &plan.children {
    describing_nodes(runtime, child, loaded, false, out);
  }
}

/// Every node of `plan` whose source seeds the store, outermost first, with
/// its slot path. Deferred children are excluded unless `deferred` is set,
/// since their data is not in this wave.
fn seeding_nodes<'p>(
  runtime: &Runtime,
  plan: &'p PlanNode,
  is_root: bool,
  deferred: bool,
  path: &mut Vec<String>,
  out: &mut Vec<(&'p PlanNode, Vec<String>)>,
) {
  if plan.deferred && !is_root && !deferred {
    return;
  }
  if let Some(source) = &plan.data_source {
    if runtime.stores.contains_key(&source.0) {
      out.push((plan, path.clone()));
    }
  }
  for (slot, child) in &plan.children {
    path.push(slot.0.clone());
    seeding_nodes(runtime, child, false, deferred, path, out);
    path.pop();
  }
}

/// Every seeding node beneath a deferred child of `plan`, at any depth, with
/// its slot path: the segments this wave renders before their seeds land.
fn promising_nodes<'p>(
  runtime: &Runtime,
  plan: &'p PlanNode,
  is_root: bool,
  path: &mut Vec<String>,
  out: &mut Vec<(&'p PlanNode, Vec<String>)>,
) {
  if plan.deferred && !is_root {
    seeding_nodes(runtime, plan, true, true, path, out);
    return;
  }
  for (slot, child) in &plan.children {
    path.push(slot.0.clone());
    promising_nodes(runtime, child, false, path, out);
    path.pop();
  }
}

/// The store a subtree renders from: what the segments around it already
/// seeded plus its own, merged by position.
struct Seeded {
  contributions: Vec<Contribution>,
  merged: Data,
  /// The keys a deferred segment's seed will replace: see [`pending_keys`].
  pending: BTreeSet<String>,
}

impl Seeded {
  fn new(contributions: Vec<Contribution>, slot_order: &[String]) -> Self {
    let merged = merge_contributions(&contributions, slot_order);
    let pending = pending_keys(&contributions, slot_order);
    Self { contributions, merged, pending }
  }
}

fn collect_loads<'p>(node: &'p PlanNode, is_root: bool, out: &mut Vec<(u32, &'p snapfire_fsr_core::DataSourceId)>) {
  if node.deferred && !is_root {
    return;
  }
  if let Some(source) = &node.data_source {
    out.push((node.id.0, source));
  }
  for (_, child) in &node.children {
    collect_loads(child, false, out);
  }
}

fn has_slot(node: &Node) -> bool {
  match node {
    Node::Slot(_) => true,
    Node::Seq(items) => items.iter().any(has_slot),
    Node::Client { children, .. } => children.iter().any(has_slot),
    _ => false,
  }
}

fn has_deferred_descendant(node: &PlanNode) -> bool {
  node
    .children
    .iter()
    .any(|(_, c)| c.deferred || has_deferred_descendant(c))
}

fn subtree_has_failure(node: &PlanNode, failed: &HashMap<u32, LoadError>) -> bool {
  failed.contains_key(&node.id.0) || node.children.iter().any(|(_, c)| subtree_has_failure(c, failed))
}

/// Every module and slot beneath a node, so two routes sharing a layout node
/// with no data of its own still key their subtrees apart.
fn subtree_data_fingerprint(node: &PlanNode, data: &HashMap<u32, Data>) -> u64 {
  fn walk(node: &PlanNode, data: &HashMap<u32, Data>, h: &mut xxhash_rust::xxh3::Xxh3) {
    // A presence marker rather than the node id: the walk is already in plan
    // order, so the shape is carried without depending on how the nodes are
    // numbered.
    match data.get(&node.id.0) {
      None => h.update(&[0]),
      Some(d) => {
        h.update(&[1]);
        h.update(&d.fingerprint().to_le_bytes());
      }
    }
    for (_, child) in &node.children {
      walk(child, data, h);
    }
  }
  let mut h = xxhash_rust::xxh3::Xxh3::new();
  walk(node, data, &mut h);
  h.digest()
}

impl Session {
  async fn load_eager(&self, plan: &PlanNode) -> Result<Loaded, AssembleError> {
    let mut wanted = Vec::new();
    collect_loads(plan, true, &mut wanted);

    let loads = wanted.iter().map(|(node_id, source_id)| {
      let node_id = *node_id;
      let source = self.runtime.sources.get(source_id);
      let source_name = source_id.0.clone();
      let runtime = &self.runtime;
      let ctx = self.ctx_of(node_id);
      let memo = runtime.load_keyer.key(source_id, ctx);
      let span = tracing::info_span!(target: "fsr::trace", "source", id = %source_id.0, node = node_id, memo = tracing::field::Empty, fibre.outcome = tracing::field::Empty);
      let loading = async move {
        let source = source.ok_or(AssembleError::MissingDataSource(source_name))?;
        if let Some(key) = &memo {
          if let Some(data) = runtime.loads.get(key).await {
            tracing::Span::current().record("memo", "hit");
            tracing::Span::current().record("fibre.outcome", "ok");
            tracing::debug!(target: "fsr::load", key = %key, "memo hit");
            return Ok::<_, AssembleError>((node_id, Ok(data)));
          }
          tracing::Span::current().record("memo", "miss");
        }
        let loaded = source.load(ctx).await;
        tracing::Span::current().record("fibre.outcome", if loaded.is_ok() { "ok" } else { "failed" });
        if let (Some(key), Ok(data)) = (&memo, &loaded) {
          runtime.loads.put(key.clone(), data.clone()).await;
        }
        Ok::<_, AssembleError>((node_id, loaded))
      };
      tracing::Instrument::instrument(loading, span)
    });

    let mut loaded = Loaded {
      data: HashMap::new(),
      failed: HashMap::new(),
    };
    for (node_id, result) in try_join_all(loads).await? {
      match result {
        Ok(data) => {
          loaded.data.insert(node_id, data);
        }
        Err(e) => {
          tracing::warn!(target: "fsr::load", node = node_id, error = %e, "segment loader failed");
          loaded.failed.insert(node_id, e);
        }
      }
    }
    Ok(loaded)
  }

  /// The degraded rendering of a segment whose loader failed: the module the
  /// plan names for that failure kind, else its error module, else the
  /// built-in error node. Rendered with params plus the message and the kind.
  async fn error_segment(&self, node: &PlanNode, failure: &LoadError) -> Result<Node, AssembleError> {
    let kind = failure.kind.as_str();
    let for_kind = node.error_kinds.iter().find(|(named, _)| named == kind).map(|(_, module)| module);
    let Some(module) = for_kind.or(node.error.as_ref()) else {
      return Ok(error_node(&failure.to_string()));
    };
    let mut props = ValueMap::default();
    self.inject_ctx_props(&mut props, node.id.0, Static::Dynamic, true, true);
    props.insert("error".to_owned(), Value::str(failure.to_string()));
    props.insert("kind".to_owned(), Value::str(kind));
    let chunks: Vec<Chunk> = self
      .runtime
      .evaluators
      .select(module)
      .evaluate(module, &props)
      .try_collect()
      .await?;
    let mut parts = Vec::with_capacity(chunks.len());
    for chunk in chunks {
      match chunk {
        Chunk::Node(n) => parts.push(n),
        Chunk::Slot(_) => return Err(AssembleError::SlotInFallback(module.to_string())),
      }
    }
    Ok(if parts.len() == 1 {
      parts.pop().unwrap()
    } else {
      Node::Seq(parts)
    })
  }

  async fn fallback_node(&self, child: &PlanNode, store: &Data, pending: &BTreeSet<String>) -> Result<Node, AssembleError> {
    let Some(module) = &child.fallback else {
      return Ok(Node::raw(""));
    };
    let mut props = ValueMap::default();
    self.inject_ctx_props(&mut props, child.id.0, Static::Dynamic, true, true);
    inject_store(&mut props, store, pending, None);
    let chunks: Vec<Chunk> = self
      .runtime
      .evaluators
      .select(module)
      .evaluate(module, &props)
      .try_collect()
      .await?;
    let mut parts = Vec::with_capacity(chunks.len());
    for chunk in chunks {
      match chunk {
        Chunk::Node(n) => parts.push(n),
        Chunk::Slot(_) => return Err(AssembleError::SlotInFallback(module.to_string())),
      }
    }
    Ok(if parts.len() == 1 {
      parts.pop().unwrap()
    } else {
      Node::Seq(parts)
    })
  }

  /// A deferred child's resolution. It renders from what the wave around it
  /// seeded, `around`, plus its own segments' seeds, so a component in it
  /// reads a layout's key the way it would had the segment not been deferred.
  fn defer(self: &Arc<Self>, child: PlanNode, slot: SlotId, key: String, path: Vec<String>, around: Vec<Contribution>) -> PendingResolution {
    let session = Arc::clone(self);
    let resolved_key = key.clone();
    let mut promised = Vec::new();
    seeding_nodes(&self.runtime, &child, true, true, &mut path.clone(), &mut promised);
    let kept: Vec<Contribution> = promised.into_iter().map(|(node, path)| Contribution::seeded(self.segment_key(node), path, Data::default())).collect();
    PendingResolution {
      slot,
      key,
      future: Box::pin(async move {
        match session.resolve_subtree(&child, path, around).await {
          Ok((node, pending, segments, meta, contributions, _digest, _failed)) => Resolved {
            slot,
            key: resolved_key,
            node,
            segments,
            pending,
            meta,
            contributions,
          },
          Err(e) => Resolved {
            slot,
            key: resolved_key,
            node: error_node(&e.to_string()),
            segments: Vec::new(),
            pending: Vec::new(),
            meta: Meta::default(),
            contributions: kept,
          },
        }
      }),
    }
  }

  /// Renders `plan`, at `path` from the route's root, from `around` plus what
  /// its own segments seed. The contributions returned are the subtree's own.
  async fn resolve_subtree(
    self: &Arc<Self>,
    plan: &PlanNode,
    path: Vec<String>,
    around: Vec<Contribution>,
  ) -> Result<(Node, Vec<PendingResolution>, Vec<SegmentInfo>, Meta, Vec<Contribution>, u64, Option<FailureKind>), AssembleError> {
    let loaded = self.load_eager(plan).await?;
    let meta = self.describe(plan, &loaded).await;
    let mut own = self.seed(plan, path.clone(), &loaded).await;
    own.extend(self.promise(plan, path.clone()));
    let mut all = around;
    all.extend(own.iter().cloned());
    let seeded = Seeded::new(all, &self.runtime.slot_order);
    let mut pending = Vec::new();
    let (node, children, _used_head, digest) = self.build(plan, &path, &loaded, &mut pending, &meta, &seeded).await?;
    let failed = loaded.failed.get(&page_of(plan).id.0).map(|e| e.kind);
    Ok((node, pending, children, meta, own, digest, failed))
  }

  /// What every seeding segment of `plan` seeds, each with the slot path
  /// that places it. A segment whose loader or seed failed seeds nothing, so
  /// its keys go, a promise for it is kept and the browser drops what it
  /// seeded before, rather than the page failing.
  async fn seed(&self, plan: &PlanNode, mut path: Vec<String>, loaded: &Loaded) -> Vec<Contribution> {
    let mut nodes = Vec::new();
    seeding_nodes(&self.runtime, plan, true, false, &mut path, &mut nodes);
    let mut out = Vec::new();
    for (node, path) in nodes {
      let source = node.data_source.as_ref().expect("a seeding node has a source");
      let values = match loaded.data.get(&node.id.0) {
        Some(data) => match self.runtime.stores[&source.0].seed(self.ctx_of(node.id.0), data).await {
          Ok(values) => values,
          Err(e) => {
            tracing::warn!(target: "fsr::load", node = node.id.0, error = %e, "segment store failed");
            Data::default()
          }
        },
        None => Data::default(),
      };
      out.push(Contribution::seeded(self.segment_key(node), path, values));
    }
    out
  }

  /// A promise per seeding segment under a deferred child of `plan` whose
  /// keys are known before its data: what this wave tells the browser is
  /// still coming.
  fn promise(&self, plan: &PlanNode, mut path: Vec<String>) -> Vec<Contribution> {
    let mut nodes = Vec::new();
    promising_nodes(&self.runtime, plan, true, &mut path, &mut nodes);
    nodes
      .into_iter()
      .filter_map(|(node, path)| {
        let source = node.data_source.as_ref()?;
        let keys = self.runtime.stores[&source.0].keys()?;
        (!keys.is_empty()).then(|| Contribution::promised(self.segment_key(node), path, keys))
      })
      .collect()
  }

  /// Every described segment of `plan` folded outermost first, so a layout
  /// states what the whole site says and a route overrides only what differs.
  /// A failing `describe` degrades to the defaults rather than the page.
  async fn describe(&self, plan: &PlanNode, loaded: &Loaded) -> Meta {
    let mut nodes = Vec::new();
    describing_nodes(&self.runtime, plan, loaded, true, &mut nodes);
    let mut meta = Meta::default();
    for node in nodes {
      let source = node.data_source.as_ref().expect("a describing node has a source");
      let describer = &self.runtime.metas[&source.0];
      match describer.describe(self.ctx_of(node.id.0), &loaded.data[&node.id.0]).await {
        Ok(described) => meta.merge(described),
        Err(e) => tracing::warn!(target: "fsr::load", node = node.id.0, error = %e, "segment metadata failed"),
      }
    }
    if !self.runtime.heads.is_empty() {
      let mut rows = Vec::new();
      module_heads(&self.runtime.heads, plan, &mut rows);
      meta.merge(Meta { head: rows, ..Meta::default() });
    }
    meta
  }

  /// The memo key of a subtree: its plan key, the parameters, what it reads
  /// of the request, the locale, the path when a component in it renders one,
  /// its shape, its own sources' data and the store it reads. A `Fixed` subtree's key names no visitor, so one entry
  /// serves everyone; an `Anonymous` one names the subject; a `Dynamic` one
  /// names the subject, the token and the whole store.
  fn cache_key_for(&self, node: &PlanNode, loaded: &Loaded, seeded: &Seeded, shape: u64, reads: Option<&SubtreeReads>) -> Option<String> {
    let store = &seeded.merged;
    let plan_key = node.cache_key.as_ref()?;
    if has_deferred_descendant(node) || subtree_has_failure(node, &loaded.failed) {
      return None;
    }
    let data = &loaded.data;
    let class = reads.map(|r| r.class).unwrap_or(Static::Dynamic);
    let mut pairs: Vec<String> = self.ctx_of(node.id.0).params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort_unstable();
    let subject = match class {
      Static::Fixed => "-".to_owned(),
      _ => self.ctx.session.identity().map(|i| i.subject).unwrap_or_else(|| "-".to_owned()),
    };
    let csrf = match class {
      Static::Dynamic if reads.is_none_or(|r| r.csrf) => self.ctx.csrf.memo_key()?,
      _ => "-".to_owned(),
    };
    let store_fp = match (class, reads) {
      (Static::Dynamic, _) | (_, None) => store.fingerprint(),
      (_, Some(reads)) => store_read(store, &reads.store_keys).fingerprint(),
    };
    let path = match reads {
      Some(reads) if !reads.path => "-".to_owned(),
      _ => format!("{}|doc={}", self.ctx.path, self.ctx.document.as_deref().unwrap_or("-")),
    };
    Some(format!(
      "{}|{}|ident={}|csrf={}|locale={}|path={}|{:016x}|{:016x}|{:016x}|pending={}",
      plan_key.0,
      pairs.join("&"),
      subject,
      csrf,
      self.ctx.locale.tag,
      path,
      shape,
      subtree_data_fingerprint(node, data),
      store_fp,
      pending_read(&seeded.pending, reads).join(",")
    ))
  }

  /// The plan child a slot names. A named slot the plan leaves unfilled
  /// renders nothing; `content` unfilled is a broken plan unless the node
  /// keeps it for the browser.
  fn child_for<'p>(&self, plan: &'p PlanNode, slot: &SlotName) -> Result<Option<&'p PlanNode>, AssembleError> {
    if let Some((_, child)) = plan.children.iter().find(|(name, _)| name == slot) {
      return Ok(Some(child));
    }
    if slot.0 == "content" && !plan.keep.contains(slot) {
      return Err(AssembleError::MissingSlot {
        node: plan.id.0,
        slot: slot.0.clone(),
      });
    }
    Ok(None)
  }

  /// The keyer's key for a segment, marked with the locale when it is not
  /// the default one, so a locale switch swaps every segment.
  fn segment_key(&self, plan: &PlanNode) -> String {
    let ctx = self.ctx_of(plan.id.0);
    let mut key = self.runtime.keyer.key(plan, &ctx.params, &ctx.query);
    key.push_str(&self.ctx.locale.key_suffix());
    key
  }

  /// The request as props: the parameters and the locale always, the
  /// identity unless the subtree is `Fixed`, the token only when it is
  /// `Dynamic`, so a render the memo shares carries nothing of the visitor.
  fn inject_ctx_props(&self, props: &mut Data, node: u32, class: Static, path: bool, csrf: bool) {
    props.insert("params".to_owned(), params_value(&self.ctx_of(node).params));
    if path {
      props.insert(PATH_PROP.to_owned(), Value::str(self.ctx.path.clone()));
      props.insert(DOCUMENT_PROP.to_owned(), Value::str(self.ctx.document.clone().unwrap_or_else(|| self.ctx.path.clone())));
    }
    if !self.ctx.locale.tag.is_empty() {
      props.insert("locale".to_owned(), Value::str(self.ctx.locale.tag.clone()));
    }
    if class != Static::Fixed {
      if let Some(identity) = self.ctx.identity_value() {
        props.insert("identity".to_owned(), identity);
      }
    }
    if class == Static::Dynamic && csrf {
      if let Some(csrf) = self.ctx.csrf.get() {
        props.insert("csrf_token".to_owned(), Value::str(csrf));
      }
    }
    if class == Static::Dynamic {
      if let Some(failure) = &self.ctx.failure {
        props.insert("action_failure".to_owned(), failure.clone());
      }
    }
  }

  /// Replaces every `Node::Slot` inside `node` with the plan child of that
  /// name, the way a `Chunk::Slot` is answered, recording each child segment
  /// with its path inside `node`.
  fn fill_slots<'a>(
    self: &'a Arc<Self>,
    node: Node,
    plan: &'a PlanNode,
    plan_path: &'a [String],
    loaded: &'a Loaded,
    out_pending: &'a mut Vec<PendingResolution>,
    segments: &'a mut Vec<SegmentInfo>,
    path: &'a mut Vec<u32>,
    meta: &'a Meta,
    seeded: &'a Seeded,
  ) -> BoxFuture<'a, Result<(Node, bool), AssembleError>> {
    Box::pin(async move {
      let mut used_head = false;
      match node {
        Node::Slot(slot) => {
          let Some(child) = self.child_for(plan, &slot)? else {
            return Ok((Node::raw(""), false));
          };
          let key = self.segment_key(child);
          let keep = SegmentInfo::keep_of(child);
          let mut child_path = plan_path.to_vec();
          child_path.push(slot.0.clone());
          if child.deferred {
            let slot_id = SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed));
            let fallback = self.fallback_node(child, &seeded.merged, &seeded.pending).await?;
            out_pending.push(self.defer(child.clone(), slot_id, key.clone(), child_path, seeded.contributions.clone()));
            segments.push(SegmentInfo {
              key,
              digest: 0,
              name: slot.0,
              path: Vec::new(),
              slot: Some(slot_id.0),
              children: Vec::new(),
              keep,
            });
            Ok((
              Node::Pending {
                slot: slot_id,
                fallback: Box::new(fallback),
              },
              false,
            ))
          } else {
            let (child_node, grandchildren, child_used_head, digest) =
              self.build(child, &child_path, loaded, out_pending, meta, seeded).await?;
            segments.push(SegmentInfo {
              key,
              digest,
              name: slot.0,
              path: path.clone(),
              slot: None,
              children: grandchildren,
              keep,
            });
            Ok((child_node, child_used_head))
          }
        }
        Node::Seq(items) => {
          let mut out = Vec::with_capacity(items.len());
          for (i, item) in items.into_iter().enumerate() {
            path.push(i as u32);
            let (filled, head) = self
              .fill_slots(item, plan, plan_path, loaded, out_pending, segments, path, meta, seeded)
              .await?;
            path.pop();
            used_head |= head;
            out.push(filled);
          }
          Ok((Node::Seq(out), used_head))
        }
        Node::Client {
          module,
          props,
          children,
          ssr,
        } => {
          let mut out = Vec::with_capacity(children.len());
          for (i, item) in children.into_iter().enumerate() {
            path.push(i as u32);
            let (filled, head) = self
              .fill_slots(item, plan, plan_path, loaded, out_pending, segments, path, meta, seeded)
              .await?;
            path.pop();
            used_head |= head;
            out.push(filled);
          }
          Ok((
            Node::Client {
              module,
              props,
              children: out,
              ssr,
            },
            used_head,
          ))
        }
        other => Ok((other, false)),
      }
    })
  }

  fn build<'a>(
    self: &'a Arc<Self>,
    node: &'a PlanNode,
    plan_path: &'a [String],
    loaded: &'a Loaded,
    out_pending: &'a mut Vec<PendingResolution>,
    meta: &'a Meta,
    seeded: &'a Seeded,
  ) -> BoxFuture<'a, Result<(Node, Vec<SegmentInfo>, bool, u64), AssembleError>> {
    Box::pin(async move {
      if let Some(failure) = loaded.failed.get(&node.id.0) {
        let node = self.error_segment(node, failure).await?;
        let digest = node.fingerprint();
        return Ok((node, Vec::new(), false, digest));
      }
      let data = &loaded.data;
      let shape = subtree_shape(node);
      let reads = self.runtime.reads.get(&shape);
      let class = reads.map(|r| r.class).unwrap_or(Static::Dynamic);
      let cache_key = match self.runtime.head_users.lock().contains(&node.id.0) {
        true => None,
        false => self.cache_key_for(node, loaded, seeded, shape, reads),
      };
      let render = tracing::info_span!(target: "fsr::trace", "render", module = %node.module, cache = tracing::field::Empty);
      let _rendering = render.enter();
      if let Some(key) = &cache_key {
        if let Some(entry) = self.runtime.cache.get(key).await {
          render.record("cache", "hit");
          tracing::debug!(target: "fsr::cache", key = %key, "hit");
          return Ok((entry.node, entry.segments, false, entry.digest));
        }
        render.record("cache", "miss");
        tracing::debug!(target: "fsr::cache", key = %key, "miss");
      }

      let mut props = data.get(&node.id.0).cloned().unwrap_or_default();
      self.inject_ctx_props(&mut props, node.id.0, class, reads.is_none_or(|r| r.path), reads.is_none_or(|r| r.csrf));
      inject_store(&mut props, &seeded.merged, &seeded.pending, reads);
      if !node.children.is_empty() || !node.keep.is_empty() {
        let slots = node
          .children
          .iter()
          .map(|(name, _)| name)
          .chain(&node.keep)
          .map(|name| Value::str(name.0.clone()))
          .collect();
        props.insert("$slots".to_owned(), Value::Seq(slots));
      }

      let chunks: Vec<Chunk> = self
        .runtime
        .evaluators
        .select(&node.module)
        .evaluate(&node.module, &props)
        .try_collect()
        .await?;

      let mut parts = Vec::with_capacity(chunks.len());
      let mut segments: Vec<(usize, SegmentInfo)> = Vec::new();
      let mut used_head = false;
      let mut own = xxhash_rust::xxh3::Xxh3::new();
      for chunk in chunks {
        own.update(&[match &chunk {
          Chunk::Node(_) => 0,
          Chunk::Slot(_) => 1,
        }]);
        match chunk {
          Chunk::Node(n) if has_slot(&n) => {
            n.write_canonical(&mut own);
            let idx = parts.len();
            let mut inner: Vec<SegmentInfo> = Vec::new();
            let (filled, child_used_head) = self
              .fill_slots(n, node, plan_path, loaded, out_pending, &mut inner, &mut Vec::new(), meta, seeded)
              .await?;
            used_head |= child_used_head;
            parts.push(filled);
            for info in inner {
              segments.push((idx, info));
            }
          }
          Chunk::Node(n) => {
            n.write_canonical(&mut own);
            parts.push(n);
          }
          Chunk::Slot(slot) if slot.0 == "head" => {
            used_head = true;
            let head = self.head.node(meta);
            head.write_canonical(&mut own);
            parts.push(head);
          }
          Chunk::Slot(slot) => {
            own.update(slot.0.as_bytes());
            let Some(child) = self.child_for(node, &slot)? else {
              continue;
            };
            let key = self.segment_key(child);
            let keep = SegmentInfo::keep_of(child);
            let mut child_path = plan_path.to_vec();
            child_path.push(slot.0.clone());
            if child.deferred {
              let slot_id = SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed));
              let fallback = self.fallback_node(child, &seeded.merged, &seeded.pending).await?;
              parts.push(Node::Pending {
                slot: slot_id,
                fallback: Box::new(fallback),
              });
              out_pending.push(self.defer(child.clone(), slot_id, key.clone(), child_path, seeded.contributions.clone()));
              segments.push((
                usize::MAX,
                SegmentInfo {
                  key,
                  digest: 0,
                  name: slot.0,
                  path: Vec::new(),
                  slot: Some(slot_id.0),
                  children: Vec::new(),
                  keep,
                },
              ));
            } else {
              let (child_node, grandchildren, child_used_head, child_digest) =
                self.build(child, &child_path, loaded, out_pending, meta, seeded).await?;
              used_head |= child_used_head;
              let idx = parts.len();
              parts.push(child_node);
              segments.push((
                idx,
                SegmentInfo {
                  key,
                  digest: child_digest,
                  name: slot.0,
                  path: Vec::new(),
                  slot: None,
                  children: grandchildren,
                  keep,
                },
              ));
            }
          }
        }
      }
      let collapsed = parts.len() == 1;
      let out = if collapsed {
        parts.pop().unwrap()
      } else {
        Node::Seq(parts)
      };
      let segments: Vec<SegmentInfo> = segments
        .into_iter()
        .map(|(idx, mut info)| {
          if info.slot.is_none() && !collapsed {
            info.path.insert(0, idx as u32);
          }
          info
        })
        .collect();
      let digest = own.digest();
      if used_head {
        self.runtime.head_users.lock().insert(node.id.0);
      }
      if let Some(key) = cache_key {
        if !used_head {
          self
            .runtime
            .cache
            .put(
              key,
              CacheEntry {
                node: out.clone(),
                segments: segments.clone(),
                digest,
              },
            )
            .await;
        }
      }
      Ok((out, segments, used_head, digest))
    })
  }
}

/// Data resolves fully before any evaluation begins, per plan node: every
/// non-deferred node's source fires in parallel, deferred nodes get a
/// `Pending` slot with their fallback and resolve through `Assembly::pending`.
pub async fn assemble(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: impl Into<Head>,
) -> Result<Assembly, AssembleError> {
  assemble_in(runtime, plan, ctx, head.into(), None).await
}

/// `assemble` for an intercepted render: the nodes `origin` names load, seed,
/// describe and take their segment key and `params` prop from the document's
/// request, while `$path` and `$document` come from `ctx` as on every node.
pub async fn assemble_under(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: impl Into<Head>,
  origin: Origin,
) -> Result<Assembly, AssembleError> {
  assemble_in(runtime, plan, ctx, head.into(), Some(origin)).await
}

async fn assemble_in(
  runtime: &Arc<Runtime>,
  plan: &PlanNode,
  ctx: &RequestCtx,
  head: Head,
  origin: Option<Origin>,
) -> Result<Assembly, AssembleError> {
  let session = Arc::new(Session {
    runtime: Arc::clone(runtime),
    ctx: ctx.clone(),
    origin,
    head: head.clone(),
    next_slot: AtomicU32::new(1),
  });
  let (tree, pending, children, meta, contributions, digest, failed) = session.resolve_subtree(plan, Vec::new(), Vec::new()).await?;
  let store = merge_contributions(&contributions, &runtime.slot_order);
  let segments = SegmentInfo {
    key: session.segment_key(plan),
    digest,
    name: String::new(),
    path: Vec::new(),
    slot: None,
    children,
    keep: SegmentInfo::keep_of(plan),
  };
  let meta = Meta {
    title: meta
      .title
      .or_else(|| (!head.title.is_empty()).then(|| head.title.clone())),
    description: meta.description.or_else(|| head.description.clone()),
    head: meta.head,
  };
  Ok(Assembly {
    tree,
    pending,
    segments,
    meta,
    contributions,
    store,
    locale: ctx.locale.clone(),
    entry: head.entry.clone(),
    styles: head.styles.clone(),
    catalog: head.catalog.clone(),
    failed,
  })
}

/// The route's own page: the node reached from the root along the `content`
/// slots, past the document and every layout.
fn page_of(plan: &PlanNode) -> &PlanNode {
  let mut node = plan;
  while let Some((_, child)) = node.children.iter().find(|(slot, _)| slot.0 == "content") {
    node = child;
  }
  node
}

/// The head rows of every module in the subtree, outermost first.
fn module_heads(heads: &HashMap<String, Vec<crate::meta::HeadEl>>, plan: &PlanNode, out: &mut Vec<crate::meta::HeadEl>) {
  if let Some(rows) = heads.get(&plan.module.to_string()) {
    out.extend(rows.iter().cloned());
  }
  for (_, child) in &plan.children {
    module_heads(heads, child, out);
  }
}
