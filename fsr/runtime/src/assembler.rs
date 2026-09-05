use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use futures_util::future::{try_join_all, BoxFuture};
use futures_util::TryStreamExt;
use snapfire_fsr_core::{Data, ModuleId, Node, Params, PlanNode, SlotId, SlotName, Value, ValueMap};

use snapfire_fsr_core::Fingerprint;

use crate::cache::{CacheEntry, NoCache, NodeCache};
use crate::ctx::RequestCtx;
use crate::data::{DataSources, LoadError};
use crate::evaluator::{Chunk, EvalError, Evaluator, NullEvaluator};
use crate::meta::{Head, Meta, Metadata};
use crate::store::Seeds;
use crate::segments::{DefaultKeyer, SegmentInfo, SegmentKeyer};

#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
  #[error("no data source registered for `{0}`")]
  MissingDataSource(String),
  #[error(transparent)]
  Load(#[from] LoadError),
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
    Self { rules: Vec::new(), null: NullEvaluator }
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
}

pub struct Runtime {
  pub sources: DataSources,
  pub evaluators: Evaluators,
  pub keyer: Arc<dyn SegmentKeyer>,
  pub cache: Arc<dyn NodeCache>,
  /// By data source id: how a segment describes the document from its data.
  pub metas: HashMap<String, Arc<dyn Metadata>>,
  /// By data source id: what a segment seeds the store with from its data.
  pub stores: HashMap<String, Arc<dyn Seeds>>,
}

pub struct RuntimeBuilder {
  sources: DataSources,
  evaluators: Evaluators,
  keyer: Arc<dyn SegmentKeyer>,
  cache: Arc<dyn NodeCache>,
  metas: HashMap<String, Arc<dyn Metadata>>,
  stores: HashMap<String, Arc<dyn Seeds>>,
}

impl RuntimeBuilder {
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

  pub fn meta(mut self, source_id: impl Into<String>, meta: Arc<dyn Metadata>) -> Self {
    self.metas.insert(source_id.into(), meta);
    self
  }

  pub fn store(mut self, source_id: impl Into<String>, seeds: Arc<dyn Seeds>) -> Self {
    self.stores.insert(source_id.into(), seeds);
    self
  }

  pub fn build(self) -> Arc<Runtime> {
    Arc::new(Runtime {
      sources: self.sources,
      evaluators: self.evaluators,
      keyer: self.keyer,
      cache: self.cache,
      metas: self.metas,
      stores: self.stores,
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
      metas: HashMap::new(),
      stores: HashMap::new(),
    }
  }

  pub fn new(sources: DataSources, evaluators: Evaluators) -> Arc<Self> {
    Self::builder().sources(sources).evaluators(evaluators).build()
  }

  pub fn with_keyer(
    sources: DataSources,
    evaluators: Evaluators,
    keyer: Arc<dyn SegmentKeyer>,
  ) -> Arc<Self> {
    Self::builder().sources(sources).evaluators(evaluators).keyer(keyer).build()
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
  /// Nested deferral: a resolution may introduce new pending slots.
  pub pending: Vec<PendingResolution>,
  /// What the resolved subtree says about the document, when a segment in
  /// it has metadata; the streams patch the title and description with it.
  pub meta: Meta,
  /// The store keys the resolved subtree seeds; the streams write them.
  pub store: Data,
}

pub struct Assembly {
  pub tree: Node,
  pub pending: Vec<PendingResolution>,
  pub segments: SegmentInfo,
  /// The title and description the eager wave settled on, defaults included.
  pub meta: Meta,
  /// The store keys the eager wave settled on, outermost segment first.
  pub store: Data,
  pub locale: crate::ctx::Locale,
  /// The head's `entry`: a module the browser loads for this response's islands.
  pub entry: Option<String>,
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

/// A node's props carry the route's store seed as `$store`, which a lowered
/// component's `Expr::Store` reads and the IR evaluator strips again before
/// the props reach the browser.
fn inject_store(props: &mut Data, store: &Data) {
  if !store.is_empty() {
    props.insert("$store".to_owned(), Value::Map(store.clone()));
  }
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
  let mut map = ValueMap::new();
  for (k, v) in params {
    map.insert(k.clone(), Value::Str(v.clone()));
  }
  Value::Map(map)
}

struct Session {
  runtime: Arc<Runtime>,
  ctx: RequestCtx,
  head: Head,
  next_slot: AtomicU32,
}

/// The innermost node of `plan` whose loaded data has metadata registered,
/// deferred children excluded since their data is not in this wave.
fn describing_node<'p>(runtime: &Runtime, plan: &'p PlanNode, loaded: &Loaded, is_root: bool) -> Option<&'p PlanNode> {
  if plan.deferred && !is_root {
    return None;
  }
  for (_, child) in &plan.children {
    if let Some(found) = describing_node(runtime, child, loaded, false) {
      return Some(found);
    }
  }
  let source = plan.data_source.as_ref()?;
  (runtime.metas.contains_key(&source.0) && loaded.data.contains_key(&plan.id.0)).then_some(plan)
}

/// Every node of `plan` whose loaded data seeds the store, outermost first,
/// deferred children excluded since their data is not in this wave.
fn seeding_nodes<'p>(runtime: &Runtime, plan: &'p PlanNode, loaded: &Loaded, is_root: bool, out: &mut Vec<&'p PlanNode>) {
  if plan.deferred && !is_root {
    return;
  }
  if let Some(source) = &plan.data_source {
    if runtime.stores.contains_key(&source.0) && loaded.data.contains_key(&plan.id.0) {
      out.push(plan);
    }
  }
  for (_, child) in &plan.children {
    seeding_nodes(runtime, child, loaded, false, out);
  }
}

fn collect_loads<'p>(
  node: &'p PlanNode,
  is_root: bool,
  out: &mut Vec<(u32, &'p snapfire_fsr_core::DataSourceId)>,
) {
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
  node.children.iter().any(|(_, c)| c.deferred || has_deferred_descendant(c))
}

fn subtree_has_failure(node: &PlanNode, failed: &HashMap<u32, LoadError>) -> bool {
  failed.contains_key(&node.id.0) || node.children.iter().any(|(_, c)| subtree_has_failure(c, failed))
}

/// Every module and slot beneath a node, so two routes sharing a layout node
/// with no data of its own still key their subtrees apart.
fn subtree_shape(node: &PlanNode, h: &mut xxhash_rust::xxh3::Xxh3) {
  h.update(node.module.to_string().as_bytes());
  for (slot, child) in &node.children {
    h.update(slot.0.as_bytes());
    subtree_shape(child, h);
  }
}

fn subtree_data_fingerprint(node: &PlanNode, data: &HashMap<u32, Data>) -> u64 {
  fn walk(node: &PlanNode, data: &HashMap<u32, Data>, h: &mut xxhash_rust::xxh3::Xxh3) {
    h.update(&node.id.0.to_le_bytes());
    if let Some(d) = data.get(&node.id.0) {
      h.update(&d.fingerprint().to_le_bytes());
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
      let ctx = &self.ctx;
      async move {
        let source = source.ok_or(AssembleError::MissingDataSource(source_name))?;
        Ok::<_, AssembleError>((node_id, source.load(ctx).await))
      }
    });

    let mut loaded = Loaded { data: HashMap::new(), failed: HashMap::new() };
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

  /// The degraded rendering of a segment whose loader failed: the plan's error
  /// module with params plus the message, or the built-in error node.
  async fn error_segment(&self, node: &PlanNode, failure: &LoadError) -> Result<Node, AssembleError> {
    let Some(module) = &node.error else { return Ok(error_node(&failure.to_string())) };
    let mut props = ValueMap::new();
    self.inject_ctx_props(&mut props);
    props.insert("error".to_owned(), Value::Str(failure.to_string()));
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
    Ok(if parts.len() == 1 { parts.pop().unwrap() } else { Node::Seq(parts) })
  }

  async fn fallback_node(&self, child: &PlanNode, store: &Data) -> Result<Node, AssembleError> {
    let Some(module) = &child.fallback else { return Ok(Node::raw("")) };
    let mut props = ValueMap::new();
    self.inject_ctx_props(&mut props);
    inject_store(&mut props, store);
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
    Ok(if parts.len() == 1 { parts.pop().unwrap() } else { Node::Seq(parts) })
  }

  fn defer(self: &Arc<Self>, child: PlanNode, slot: SlotId, key: String) -> PendingResolution {
    let session = Arc::clone(self);
    let resolved_key = key.clone();
    PendingResolution {
      slot,
      key,
      future: Box::pin(async move {
        match session.resolve_subtree(&child).await {
          Ok((node, pending, _segments, meta, store)) => Resolved { slot, key: resolved_key, node, pending, meta, store },
          Err(e) => Resolved { slot, key: resolved_key, node: error_node(&e.to_string()), pending: Vec::new(), meta: Meta::default(), store: Data::new() },
        }
      }),
    }
  }

  async fn resolve_subtree(
    self: &Arc<Self>,
    plan: &PlanNode,
  ) -> Result<(Node, Vec<PendingResolution>, Vec<SegmentInfo>, Meta, Data), AssembleError> {
    let loaded = self.load_eager(plan).await?;
    let meta = self.describe(plan, &loaded).await;
    let store = self.seed(plan, &loaded).await;
    let mut pending = Vec::new();
    let (node, children, _used_head) = self.build(plan, &loaded, &mut pending, &meta, &store).await?;
    Ok((node, pending, children, meta, store))
  }

  /// The store keys every seeding segment of `plan` settled on, an inner
  /// segment winning a key an outer one also sets. A failing seed costs its
  /// keys rather than the page.
  async fn seed(&self, plan: &PlanNode, loaded: &Loaded) -> Data {
    let mut nodes = Vec::new();
    seeding_nodes(&self.runtime, plan, loaded, true, &mut nodes);
    let mut out = Data::new();
    for node in nodes {
      let source = node.data_source.as_ref().expect("a seeding node has a source");
      match self.runtime.stores[&source.0].seed(&self.ctx, &loaded.data[&node.id.0]).await {
        Ok(seeded) => out.extend(seeded),
        Err(e) => tracing::warn!(target: "fsr::load", node = node.id.0, error = %e, "segment store failed"),
      }
    }
    out
  }

  /// The metadata of the innermost described segment of `plan`, or none. A
  /// failing `describe` degrades to the defaults rather than the page.
  async fn describe(&self, plan: &PlanNode, loaded: &Loaded) -> Meta {
    let Some(node) = describing_node(&self.runtime, plan, loaded, true) else { return Meta::default() };
    let source = node.data_source.as_ref().expect("a describing node has a source");
    let describer = &self.runtime.metas[&source.0];
    match describer.describe(&self.ctx, &loaded.data[&node.id.0]).await {
      Ok(meta) => meta,
      Err(e) => {
        tracing::warn!(target: "fsr::load", node = node.id.0, error = %e, "segment metadata failed");
        Meta::default()
      }
    }
  }

  fn cache_key_for(&self, node: &PlanNode, loaded: &Loaded, store: &Data) -> Option<String> {
    let plan_key = node.cache_key.as_ref()?;
    if has_deferred_descendant(node) || subtree_has_failure(node, &loaded.failed) {
      return None;
    }
    let data = &loaded.data;
    let mut pairs: Vec<String> = self.ctx.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort_unstable();
    let subject = self.ctx.session.identity().map(|i| i.subject).unwrap_or_else(|| "-".to_owned());
    let csrf = self.ctx.csrf.as_deref().unwrap_or("-");
    let mut shape = xxhash_rust::xxh3::Xxh3::new();
    subtree_shape(node, &mut shape);
    Some(format!(
      "{}|{}|ident={}|csrf={}|locale={}|{:016x}|{:016x}|{:016x}",
      plan_key.0,
      pairs.join("&"),
      subject,
      csrf,
      self.ctx.locale.tag,
      shape.digest(),
      subtree_data_fingerprint(node, data),
      store.fingerprint()
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
      return Err(AssembleError::MissingSlot { node: plan.id.0, slot: slot.0.clone() });
    }
    Ok(None)
  }

  /// The keyer's key for a segment, marked with the locale when it is not
  /// the default one, so a locale switch swaps every segment.
  fn segment_key(&self, plan: &PlanNode) -> String {
    let mut key = self.runtime.keyer.key(plan, &self.ctx.params, &self.ctx.query);
    key.push_str(&self.ctx.locale.key_suffix());
    key
  }

  fn inject_ctx_props(&self, props: &mut Data) {
    props.insert("params".to_owned(), params_value(&self.ctx.params));
    if !self.ctx.locale.tag.is_empty() {
      props.insert("locale".to_owned(), Value::Str(self.ctx.locale.tag.clone()));
    }
    if let Some(identity) = self.ctx.identity_value() {
      props.insert("identity".to_owned(), identity);
    }
    if let Some(csrf) = &self.ctx.csrf {
      props.insert("csrf_token".to_owned(), Value::Str(csrf.clone()));
    }
  }

  /// Replaces every `Node::Slot` inside `node` with the plan child of that
  /// name, the way a `Chunk::Slot` is answered, recording each child segment
  /// with its path inside `node`.
  fn fill_slots<'a>(
    self: &'a Arc<Self>,
    node: Node,
    plan: &'a PlanNode,
    loaded: &'a Loaded,
    out_pending: &'a mut Vec<PendingResolution>,
    segments: &'a mut Vec<SegmentInfo>,
    path: &'a mut Vec<u32>,
    meta: &'a Meta,
    store: &'a Data,
  ) -> BoxFuture<'a, Result<(Node, bool), AssembleError>> {
    Box::pin(async move {
      let mut used_head = false;
      match node {
        Node::Slot(slot) => {
          let Some(child) = self.child_for(plan, &slot)? else { return Ok((Node::raw(""), false)) };
          let key = self.segment_key(child);
          let keep = SegmentInfo::keep_of(child);
          if child.deferred {
            let slot_id = SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed));
            let fallback = self.fallback_node(child, store).await?;
            out_pending.push(self.defer(child.clone(), slot_id, key.clone()));
            segments.push(SegmentInfo { key, name: slot.0, path: Vec::new(), slot: Some(slot_id.0), children: Vec::new(), keep });
            Ok((Node::Pending { slot: slot_id, fallback: Box::new(fallback) }, false))
          } else {
            let (child_node, grandchildren, child_used_head) = self.build(child, loaded, out_pending, meta, store).await?;
            segments.push(SegmentInfo { key, name: slot.0, path: path.clone(), slot: None, children: grandchildren, keep });
            Ok((child_node, child_used_head))
          }
        }
        Node::Seq(items) => {
          let mut out = Vec::with_capacity(items.len());
          for (i, item) in items.into_iter().enumerate() {
            path.push(i as u32);
            let (filled, head) = self.fill_slots(item, plan, loaded, out_pending, segments, path, meta, store).await?;
            path.pop();
            used_head |= head;
            out.push(filled);
          }
          Ok((Node::Seq(out), used_head))
        }
        Node::Client { module, props, children, ssr } => {
          let mut out = Vec::with_capacity(children.len());
          for (i, item) in children.into_iter().enumerate() {
            path.push(i as u32);
            let (filled, head) = self.fill_slots(item, plan, loaded, out_pending, segments, path, meta, store).await?;
            path.pop();
            used_head |= head;
            out.push(filled);
          }
          Ok((Node::Client { module, props, children: out, ssr }, used_head))
        }
        other => Ok((other, false)),
      }
    })
  }

  fn build<'a>(
    self: &'a Arc<Self>,
    node: &'a PlanNode,
    loaded: &'a Loaded,
    out_pending: &'a mut Vec<PendingResolution>,
    meta: &'a Meta,
    store: &'a Data,
  ) -> BoxFuture<'a, Result<(Node, Vec<SegmentInfo>, bool), AssembleError>> {
    Box::pin(async move {
      if let Some(failure) = loaded.failed.get(&node.id.0) {
        return Ok((self.error_segment(node, failure).await?, Vec::new(), false));
      }
      let data = &loaded.data;
      let cache_key = self.cache_key_for(node, loaded, store);
      if let Some(key) = &cache_key {
        if let Some(entry) = self.runtime.cache.get(key).await {
          tracing::debug!(target: "fsr::cache", key = %key, "hit");
          return Ok((entry.node, entry.segments, false));
        }
        tracing::debug!(target: "fsr::cache", key = %key, "miss");
      }

      let mut props = data.get(&node.id.0).cloned().unwrap_or_default();
      self.inject_ctx_props(&mut props);
      inject_store(&mut props, store);
      if !node.children.is_empty() || !node.keep.is_empty() {
        let slots = node.children.iter().map(|(name, _)| name).chain(&node.keep).map(|name| Value::Str(name.0.clone())).collect();
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
      for chunk in chunks {
        match chunk {
          Chunk::Node(n) if has_slot(&n) => {
            let idx = parts.len();
            let mut inner: Vec<SegmentInfo> = Vec::new();
            let (filled, child_used_head) = self.fill_slots(n, node, loaded, out_pending, &mut inner, &mut Vec::new(), meta, store).await?;
            used_head |= child_used_head;
            parts.push(filled);
            for info in inner {
              segments.push((idx, info));
            }
          }
          Chunk::Node(n) => parts.push(n),
          Chunk::Slot(slot) if slot.0 == "head" => {
            used_head = true;
            parts.push(self.head.node(meta));
          }
          Chunk::Slot(slot) => {
            let Some(child) = self.child_for(node, &slot)? else { continue };
            let key = self.segment_key(child);
            let keep = SegmentInfo::keep_of(child);
            if child.deferred {
              let slot_id = SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed));
              let fallback = self.fallback_node(child, store).await?;
              parts.push(Node::Pending { slot: slot_id, fallback: Box::new(fallback) });
              out_pending.push(self.defer(child.clone(), slot_id, key.clone()));
              segments.push((usize::MAX, SegmentInfo { key, name: slot.0, path: Vec::new(), slot: Some(slot_id.0), children: Vec::new(), keep }));
            } else {
              let (child_node, grandchildren, child_used_head) =
                self.build(child, loaded, out_pending, meta, store).await?;
              used_head |= child_used_head;
              let idx = parts.len();
              parts.push(child_node);
              segments.push((idx, SegmentInfo { key, name: slot.0, path: Vec::new(), slot: None, children: grandchildren, keep }));
            }
          }
        }
      }
      let collapsed = parts.len() == 1;
      let out = if collapsed { parts.pop().unwrap() } else { Node::Seq(parts) };
      let segments: Vec<SegmentInfo> = segments
        .into_iter()
        .map(|(idx, mut info)| {
          if info.slot.is_none() && !collapsed {
            info.path.insert(0, idx as u32);
          }
          info
        })
        .collect();
      if let Some(key) = cache_key {
        if !used_head {
          self
            .runtime
            .cache
            .put(key, CacheEntry { node: out.clone(), segments: segments.clone() })
            .await;
        }
      }
      Ok((out, segments, used_head))
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
  let head: Head = head.into();
  let session = Arc::new(Session {
    runtime: Arc::clone(runtime),
    ctx: ctx.clone(),
    head: head.clone(),
    next_slot: AtomicU32::new(1),
  });
  let (tree, pending, children, meta, store) = session.resolve_subtree(plan).await?;
  let segments = SegmentInfo {
    key: session.segment_key(plan),
    name: String::new(),
    path: Vec::new(),
    slot: None,
    children,
    keep: SegmentInfo::keep_of(plan),
  };
  let meta = Meta { title: meta.title.or_else(|| (!head.title.is_empty()).then(|| head.title.clone())), description: meta.description.or_else(|| head.description.clone()) };
  Ok(Assembly { tree, pending, segments, meta, store, locale: ctx.locale.clone(), entry: head.entry.clone() })
}
