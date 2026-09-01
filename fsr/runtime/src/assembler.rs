use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use futures_util::future::{try_join_all, BoxFuture};
use futures_util::TryStreamExt;
use snapfire_fsr_core::{Data, ModuleId, Node, Params, PlanNode, SlotId, Value, ValueMap};

use snapfire_fsr_core::Fingerprint;

use crate::cache::{CacheEntry, NoCache, NodeCache};
use crate::ctx::RequestCtx;
use crate::data::{DataSources, LoadError};
use crate::evaluator::{Chunk, EvalError, Evaluator, NullEvaluator};
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
}

pub struct RuntimeBuilder {
  sources: DataSources,
  evaluators: Evaluators,
  keyer: Arc<dyn SegmentKeyer>,
  cache: Arc<dyn NodeCache>,
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

  pub fn build(self) -> Arc<Runtime> {
    Arc::new(Runtime {
      sources: self.sources,
      evaluators: self.evaluators,
      keyer: self.keyer,
      cache: self.cache,
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
  pub future: BoxFuture<'static, Resolved>,
}

pub struct Resolved {
  pub slot: SlotId,
  pub node: Node,
  /// Nested deferral: a resolution may introduce new pending slots.
  pub pending: Vec<PendingResolution>,
}

pub struct Assembly {
  pub tree: Node,
  pub pending: Vec<PendingResolution>,
  pub segments: SegmentInfo,
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
  head: Node,
  next_slot: AtomicU32,
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

fn has_deferred_descendant(node: &PlanNode) -> bool {
  node.children.iter().any(|(_, c)| c.deferred || has_deferred_descendant(c))
}

fn subtree_has_failure(node: &PlanNode, failed: &HashMap<u32, LoadError>) -> bool {
  failed.contains_key(&node.id.0) || node.children.iter().any(|(_, c)| subtree_has_failure(c, failed))
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

  async fn fallback_node(&self, child: &PlanNode) -> Result<Node, AssembleError> {
    let Some(module) = &child.fallback else { return Ok(Node::raw("")) };
    let mut props = ValueMap::new();
    self.inject_ctx_props(&mut props);
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

  fn defer(self: &Arc<Self>, child: PlanNode, slot: SlotId) -> PendingResolution {
    let session = Arc::clone(self);
    PendingResolution {
      slot,
      future: Box::pin(async move {
        match session.resolve_subtree(&child).await {
          Ok((node, pending, _segments)) => Resolved { slot, node, pending },
          Err(e) => Resolved { slot, node: error_node(&e.to_string()), pending: Vec::new() },
        }
      }),
    }
  }

  async fn resolve_subtree(
    self: &Arc<Self>,
    plan: &PlanNode,
  ) -> Result<(Node, Vec<PendingResolution>, Vec<SegmentInfo>), AssembleError> {
    let loaded = self.load_eager(plan).await?;
    let mut pending = Vec::new();
    let (node, children, _used_head) = self.build(plan, &loaded, &mut pending).await?;
    Ok((node, pending, children))
  }

  fn cache_key_for(&self, node: &PlanNode, loaded: &Loaded) -> Option<String> {
    let plan_key = node.cache_key.as_ref()?;
    if has_deferred_descendant(node) || subtree_has_failure(node, &loaded.failed) {
      return None;
    }
    let data = &loaded.data;
    let mut pairs: Vec<String> = self.ctx.params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort_unstable();
    let subject = self.ctx.session.identity().map(|i| i.subject).unwrap_or_else(|| "-".to_owned());
    Some(format!(
      "{}|{}|ident={}|{:016x}",
      plan_key.0,
      pairs.join("&"),
      subject,
      subtree_data_fingerprint(node, data)
    ))
  }

  fn inject_ctx_props(&self, props: &mut Data) {
    props.insert("params".to_owned(), params_value(&self.ctx.params));
    if let Some(identity) = self.ctx.identity_value() {
      props.insert("identity".to_owned(), identity);
    }
    if let Some(csrf) = &self.ctx.csrf {
      props.insert("csrf_token".to_owned(), Value::Str(csrf.clone()));
    }
  }

  fn build<'a>(
    self: &'a Arc<Self>,
    node: &'a PlanNode,
    loaded: &'a Loaded,
    out_pending: &'a mut Vec<PendingResolution>,
  ) -> BoxFuture<'a, Result<(Node, Vec<SegmentInfo>, bool), AssembleError>> {
    Box::pin(async move {
      if let Some(failure) = loaded.failed.get(&node.id.0) {
        return Ok((self.error_segment(node, failure).await?, Vec::new(), false));
      }
      let data = &loaded.data;
      let cache_key = self.cache_key_for(node, loaded);
      if let Some(key) = &cache_key {
        if let Some(entry) = self.runtime.cache.get(key).await {
          tracing::debug!(target: "fsr::cache", key = %key, "hit");
          return Ok((entry.node, entry.segments, false));
        }
        tracing::debug!(target: "fsr::cache", key = %key, "miss");
      }

      let mut props = data.get(&node.id.0).cloned().unwrap_or_default();
      self.inject_ctx_props(&mut props);

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
          Chunk::Node(n) => parts.push(n),
          Chunk::Slot(slot) if slot.0 == "head" => {
            used_head = true;
            parts.push(self.head.clone());
          }
          Chunk::Slot(slot) => {
            let child = node
              .children
              .iter()
              .find(|(name, _)| *name == slot)
              .map(|(_, child)| child)
              .ok_or_else(|| AssembleError::MissingSlot { node: node.id.0, slot: slot.0.clone() })?;
            let key = self.runtime.keyer.key(child, &self.ctx.params);
            if child.deferred {
              let slot_id = SlotId(self.next_slot.fetch_add(1, Ordering::Relaxed));
              let fallback = self.fallback_node(child).await?;
              parts.push(Node::Pending { slot: slot_id, fallback: Box::new(fallback) });
              out_pending.push(self.defer(child.clone(), slot_id));
              segments.push((usize::MAX, SegmentInfo { key, path: Vec::new(), slot: Some(slot_id.0), children: Vec::new() }));
            } else {
              let (child_node, grandchildren, child_used_head) =
                self.build(child, loaded, out_pending).await?;
              used_head |= child_used_head;
              let idx = parts.len();
              parts.push(child_node);
              segments.push((idx, SegmentInfo { key, path: Vec::new(), slot: None, children: grandchildren }));
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
            info.path = vec![idx as u32];
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
  head: &Node,
) -> Result<Assembly, AssembleError> {
  let session = Arc::new(Session {
    runtime: Arc::clone(runtime),
    ctx: ctx.clone(),
    head: head.clone(),
    next_slot: AtomicU32::new(1),
  });
  let (tree, pending, children) = session.resolve_subtree(plan).await?;
  let segments = SegmentInfo {
    key: runtime.keyer.key(plan, &ctx.params),
    path: Vec::new(),
    slot: None,
    children,
  };
  Ok(Assembly { tree, pending, segments })
}
