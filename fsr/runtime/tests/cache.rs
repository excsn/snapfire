use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream;
use snapfire_fsr_core::{
  CacheKey, Data, DataSourceId, Fingerprint, ModuleId, Node, NodeId, Params, PlanNode, SlotName,
  Value, ValueMap,
};
use snapfire_fsr_runtime::{
  assemble, Chunk, DataSources, Evaluator, Evaluators, Identity, MemoryCache, NodeChunks,
  RequestCtx, Runtime, SessionCell,
};

struct CountingEval(Arc<AtomicU32>);

impl Evaluator for CountingEval {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    self.0.fetch_add(1, Ordering::Relaxed);
    let text = match props.get("version") {
      Some(Value::Int(v)) => format!("<v{v}>"),
      _ => "<page>".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(text)))]))
  }
}

struct HeadShell(Arc<AtomicU32>);

impl Evaluator for HeadShell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    self.0.fetch_add(1, Ordering::Relaxed);
    Box::pin(stream::iter([
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Node(Node::raw("<body>"))),
    ]))
  }
}

fn cached_leaf(source: Option<&str>) -> PlanNode {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("page.tera", "default"));
  plan.cache_key = Some(CacheKey("page".into()));
  plan.data_source = source.map(|s| DataSourceId(s.into()));
  plan
}

fn runtime(evals: Arc<AtomicU32>, sources: DataSources) -> Arc<Runtime> {
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(CountingEval(evals)));
  Runtime::builder()
    .sources(sources)
    .evaluators(evaluators)
    .cache(Arc::new(MemoryCache::new()))
    .build()
}

fn versioned_sources(version: Arc<AtomicU32>) -> DataSources {
  let mut sources = DataSources::new();
  sources.insert_fn("ver", move |_p| {
    let v = version.load(Ordering::Relaxed);
    async move {
      let mut data = ValueMap::new();
      data.insert("version".to_owned(), Value::int(v as i64));
      Ok(data)
    }
  });
  sources
}

#[test]
fn a_hit_skips_evaluation_and_preserves_output() {
  let evals = Arc::new(AtomicU32::new(0));
  let version = Arc::new(AtomicU32::new(1));
  let rt = runtime(Arc::clone(&evals), versioned_sources(version));
  let plan = cached_leaf(Some("ver"));

  let first = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let second = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 1, "second render is a cache hit");
  assert_eq!(first.tree.fingerprint(), second.tree.fingerprint());
}

#[test]
fn changed_data_is_a_miss_never_a_stale_hit() {
  let evals = Arc::new(AtomicU32::new(0));
  let version = Arc::new(AtomicU32::new(1));
  let rt = runtime(Arc::clone(&evals), versioned_sources(Arc::clone(&version)));
  let plan = cached_leaf(Some("ver"));

  let first = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  version.store(2, Ordering::Relaxed);
  let second = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 2);
  assert_ne!(first.tree.fingerprint(), second.tree.fingerprint());
  assert_eq!(second.tree, Node::raw("<v2>"));
}

#[test]
fn params_are_part_of_the_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);

  let mut a = Params::new();
  a.insert("section".to_owned(), "servers".to_owned());
  let mut b = Params::new();
  b.insert("section".to_owned(), "network".to_owned());

  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(a.clone()), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(b.clone()), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(a.clone()), &Node::raw(""))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 2, "distinct params evaluate, repeats hit");
}

#[test]
fn invalidation_by_plan_cache_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);

  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  block_on(rt.cache.invalidate("page"));
  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 2);
}

#[test]
fn a_subtree_that_used_the_head_slot_is_never_cached() {
  let evals = Arc::new(AtomicU32::new(0));
  let mut evaluators = Evaluators::new();
  evaluators.register(
    |m: &ModuleId| m.path == "shell.tera",
    Arc::new(HeadShell(Arc::clone(&evals))),
  );
  let rt = Runtime::builder()
    .evaluators(evaluators)
    .cache(Arc::new(MemoryCache::new()))
    .build();

  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.cache_key = Some(CacheKey("shell".into()));

  let a = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw("<title>a</title>"))).unwrap();
  let b = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw("<title>b</title>"))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 2, "head content must never bake into a cache entry");
  assert_ne!(a.tree.fingerprint(), b.tree.fingerprint());
}

#[test]
fn a_deferred_descendant_bypasses_the_cache() {
  let evals = Arc::new(AtomicU32::new(0));
  struct SlotShell(Arc<AtomicU32>);
  impl Evaluator for SlotShell {
    fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
      self.0.fetch_add(1, Ordering::Relaxed);
      Box::pin(stream::iter([Ok(Chunk::Slot(SlotName("late".into())))]))
    }
  }
  struct Leaf;
  impl Evaluator for Leaf {
    fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
      Box::pin(stream::iter([Ok(Chunk::Node(Node::raw("<late>")))]))
    }
  }

  let mut evaluators = Evaluators::new();
  evaluators.register(
    |m: &ModuleId| m.path == "shell.tera",
    Arc::new(SlotShell(Arc::clone(&evals))),
  );
  evaluators.register(|m: &ModuleId| m.path == "late.tera", Arc::new(Leaf));
  let rt = Runtime::builder()
    .evaluators(evaluators)
    .cache(Arc::new(MemoryCache::new()))
    .build();

  let mut late = PlanNode::new(NodeId(1), ModuleId::new("late.tera", "default"));
  late.deferred = true;
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.cache_key = Some(CacheKey("shell".into()));
  plan.children.push((SlotName("late".into()), late));

  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  assert_eq!(evals.load(Ordering::Relaxed), 2, "slot ids are per response, so no caching around Pending");
}

#[test]
fn identity_is_part_of_the_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);

  let user = |subject: &str| {
    let cell = SessionCell::default();
    cell.set_identity(Some(Identity { subject: subject.to_owned(), claims: ValueMap::new() }));
    RequestCtx { params: Params::new(), session: cell, csrf: None }
  };

  block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("bob"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 3, "anon, alice and bob each evaluate once; alice repeats hit");
}
