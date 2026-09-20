use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use futures::executor::block_on;
use futures_util::stream;
use snapfire_fsr_core::{
  CacheKey, Data, DataSourceId, Fingerprint, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap,
};
use snapfire_fsr_runtime::{CsrfHandle, 
  CacheEntry, Chunk, DataSources, Evaluator, Evaluators, FibreCache, Identity, MemoryCache, NodeCache, NodeChunks,
  RequestCtx, Runtime, SessionCell, assemble,
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
      let mut data = ValueMap::default();
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

  let first = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  let second = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();

  assert_eq!(evals.load(Ordering::Relaxed), 1, "second render is a cache hit");
  assert_eq!(first.tree.fingerprint(), second.tree.fingerprint());
}

#[test]
fn changed_data_is_a_miss_never_a_stale_hit() {
  let evals = Arc::new(AtomicU32::new(0));
  let version = Arc::new(AtomicU32::new(1));
  let rt = runtime(Arc::clone(&evals), versioned_sources(Arc::clone(&version)));
  let plan = cached_leaf(Some("ver"));

  let first = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  version.store(2, Ordering::Relaxed);
  let second = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();

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

  assert_eq!(
    evals.load(Ordering::Relaxed),
    2,
    "distinct params evaluate, repeats hit"
  );
}

#[test]
fn invalidation_by_plan_cache_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);

  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  block_on(rt.cache.invalidate("page"));
  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();

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

  let a = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw("<title>a</title>"),
  ))
  .unwrap();
  let b = block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw("<title>b</title>"),
  ))
  .unwrap();

  assert_eq!(
    evals.load(Ordering::Relaxed),
    2,
    "head content must never bake into a cache entry"
  );
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

  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  assert_eq!(
    evals.load(Ordering::Relaxed),
    2,
    "slot ids are per response, so no caching around Pending"
  );
}

#[test]
fn identity_is_part_of_the_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);

  let user = |subject: &str| {
    let cell = SessionCell::default();
    cell.set_identity(Some(Identity {
      subject: subject.to_owned(),
      claims: ValueMap::default(),
    }));
    RequestCtx {
      params: Params::new(),
      session: cell,
      ..Default::default()
    }
  };

  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("bob"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();

  assert_eq!(
    evals.load(Ordering::Relaxed),
    3,
    "anon, alice and bob each evaluate once; alice repeats hit"
  );
}

#[test]
fn a_tuned_shard_count_changes_nothing_a_caller_can_observe() {
  let entry = CacheEntry {
    node: Node::raw("<p>one</p>"),
    segments: Vec::new(),
    digest: 0,
  };
  let tuned = FibreCache::bounded_sharded(64, Duration::from_secs(60), 4);
  let custom = FibreCache::new(
    fibre_cache::CacheBuilder::default()
      .capacity(64)
      .time_to_live(Duration::from_secs(60))
      .shards(2)
      .build()
      .unwrap(),
  );

  for cache in [&tuned, &custom] {
    block_on(cache.put("page|a".to_owned(), entry.clone()));
    assert_eq!(block_on(cache.get("page|a")), Some(entry.clone()));
    block_on(cache.invalidate("page"));
    assert_eq!(block_on(cache.get("page|a")), None);
  }
}

#[test]
fn the_csrf_token_is_part_of_the_key() {
  let evals = Arc::new(AtomicU32::new(0));
  let rt = runtime(Arc::clone(&evals), DataSources::new());
  let plan = cached_leaf(None);
  let with = |token: &str| RequestCtx {
    csrf: CsrfHandle::fixed(token),
    ..Default::default()
  };

  block_on(assemble(
    &rt,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  block_on(assemble(&rt, &plan, &with("t1"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &with("t2"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &with("t1"), &Node::raw(""))).unwrap();

  assert_eq!(
    evals.load(Ordering::Relaxed),
    3,
    "a token is injected into props, so an entry never serves another session's"
  );
}

#[test]
fn invalidation_says_how_many_entries_went() {
  let entry = CacheEntry {
    node: Node::raw("<p>one</p>"),
    segments: Vec::new(),
    digest: 0,
  };
  let memory = MemoryCache::new();
  let fibre = FibreCache::bounded(64, Duration::from_secs(60));
  let caches: [&dyn NodeCache; 2] = [&memory, &fibre];
  for cache in caches {
    block_on(cache.put("page|a".to_owned(), entry.clone()));
    block_on(cache.put("page|b".to_owned(), entry.clone()));
    block_on(cache.put("other|a".to_owned(), entry.clone()));
    assert_eq!(block_on(cache.invalidate("page")), 2);
    assert_eq!(block_on(cache.invalidate("page")), 0);
    assert_eq!(block_on(cache.get("other|a")), Some(entry.clone()));
  }
}

struct PropsEval(Arc<AtomicU32>);

impl Evaluator for PropsEval {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    self.0.fetch_add(1, Ordering::Relaxed);
    let who = match props.get("identity") {
      Some(Value::Map(m)) => match m.get("subject") {
        Some(Value::Str(s)) => format!("<{s}>"),
        _ => "<?>".to_owned(),
      },
      _ => "<nobody>".to_owned(),
    };
    let token = if props.contains_key("csrf_token") { "<token>" } else { "" };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("{who}{token}"))))]))
  }
}

fn user(subject: &str) -> RequestCtx {
  let cell = SessionCell::default();
  cell.set_identity(Some(Identity { subject: subject.to_owned(), claims: ValueMap::default() }));
  RequestCtx { params: Params::new(), session: cell, csrf: CsrfHandle::fixed("t0k"), ..Default::default() }
}

#[test]
fn a_fixed_subtree_is_memoized_once_for_everyone_and_carries_no_visitor() {
  use snapfire_fsr_runtime::{subtree_shape, Reads, Static, SubtreeReads};
  let evals = Arc::new(AtomicU32::new(0));
  let plan = cached_leaf(None);
  let mut reads = Reads::new();
  reads.insert(subtree_shape(&plan), SubtreeReads { class: Static::Fixed, store_keys: Vec::new(), path: false, csrf: false });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(PropsEval(Arc::clone(&evals))));
  let rt = Runtime::builder().evaluators(evaluators).cache(Arc::new(MemoryCache::new())).reads(reads).build();

  let first = block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();
  let second = block_on(assemble(&rt, &plan, &user("bob"), &Node::raw(""))).unwrap();
  let third = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  assert_eq!(evals.load(Ordering::Relaxed), 1, "alice's render serves bob and the anonymous visitor");
  assert_eq!(first.tree, Node::raw("<nobody>"), "a fixed subtree's props name no identity and no token");
  assert_eq!(second.tree, first.tree);
  assert_eq!(third.tree, first.tree);
}

struct PathEval(Arc<AtomicU32>);

impl Evaluator for PathEval {
  fn evaluate(&self, _module: &ModuleId, props: &snapfire_fsr_core::Data) -> snapfire_fsr_runtime::evaluator::NodeChunks {
    self.0.fetch_add(1, Ordering::Relaxed);
    let at = match props.get(snapfire_fsr_runtime::PATH_PROP) {
      Some(Value::Str(s)) => s.to_string(),
      _ => "-".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(at)))]))
  }
}

fn at(path: &str) -> RequestCtx {
  RequestCtx { params: Params::new(), path: path.to_owned(), ..Default::default() }
}

#[test]
fn a_subtree_rendering_the_path_is_memoized_per_path() {
  use snapfire_fsr_runtime::{subtree_shape, Reads, Static, SubtreeReads};
  let evals = Arc::new(AtomicU32::new(0));
  let plan = cached_leaf(None);
  let mut reads = Reads::new();
  reads.insert(subtree_shape(&plan), SubtreeReads { class: Static::Fixed, store_keys: Vec::new(), path: true, csrf: false });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(PathEval(Arc::clone(&evals))));
  let rt = Runtime::builder().evaluators(evaluators).cache(Arc::new(MemoryCache::new())).reads(reads).build();

  let first = block_on(assemble(&rt, &plan, &at("/billing"), &Node::raw(""))).unwrap();
  let second = block_on(assemble(&rt, &plan, &at("/billing/overdue"), &Node::raw(""))).unwrap();
  let again = block_on(assemble(&rt, &plan, &at("/billing"), &Node::raw(""))).unwrap();
  assert_eq!(first.tree, Node::raw("/billing"));
  assert_eq!(second.tree, Node::raw("/billing/overdue"), "the second path is rendered rather than served the first one's marks");
  assert_eq!(again.tree, first.tree);
  assert_eq!(evals.load(Ordering::Relaxed), 2, "one render per path; the repeat is a hit");
}

#[test]
fn a_subtree_that_renders_no_path_is_memoized_across_paths() {
  use snapfire_fsr_runtime::{subtree_shape, Reads, Static, SubtreeReads};
  let evals = Arc::new(AtomicU32::new(0));
  let plan = cached_leaf(None);
  let mut reads = Reads::new();
  reads.insert(subtree_shape(&plan), SubtreeReads { class: Static::Fixed, store_keys: Vec::new(), path: false, csrf: false });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(PathEval(Arc::clone(&evals))));
  let rt = Runtime::builder().evaluators(evaluators).cache(Arc::new(MemoryCache::new())).reads(reads).build();

  let first = block_on(assemble(&rt, &plan, &at("/billing"), &Node::raw(""))).unwrap();
  let second = block_on(assemble(&rt, &plan, &at("/billing/overdue"), &Node::raw(""))).unwrap();
  assert_eq!(first.tree, Node::raw("-"), "the path is left out of the props of a subtree that renders none");
  assert_eq!(second.tree, first.tree);
  assert_eq!(evals.load(Ordering::Relaxed), 1, "one render serves both paths");
}

#[test]
fn an_anonymous_subtree_is_memoized_per_subject_without_the_token() {
  use snapfire_fsr_runtime::{subtree_shape, Reads, Static, SubtreeReads};
  let evals = Arc::new(AtomicU32::new(0));
  let plan = cached_leaf(None);
  let mut reads = Reads::new();
  reads.insert(subtree_shape(&plan), SubtreeReads { class: Static::Anonymous, store_keys: Vec::new(), path: false, csrf: false });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(PropsEval(Arc::clone(&evals))));
  let rt = Runtime::builder().evaluators(evaluators).cache(Arc::new(MemoryCache::new())).reads(reads).build();

  let alice = block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("bob"), &Node::raw(""))).unwrap();
  block_on(assemble(&rt, &plan, &user("alice"), &Node::raw(""))).unwrap();
  assert_eq!(evals.load(Ordering::Relaxed), 2, "alice and bob each once; alice repeats hit");
  assert_eq!(alice.tree, Node::raw("<alice>"), "the identity is a prop, the token is not");

  let dynamic = block_on(assemble(&runtime(Arc::new(AtomicU32::new(0)), DataSources::new()), &plan, &user("alice"), &Node::raw(""))).unwrap();
  assert_eq!(dynamic.tree, Node::raw("<page>"), "the counting evaluator ignores props");
}

struct PassThrough;

impl snapfire_fsr_runtime::Seeds for PassThrough {
  fn seed(&self, _ctx: &RequestCtx, data: &Data) -> futures_util::future::BoxFuture<'static, Result<Data, snapfire_fsr_runtime::LoadError>> {
    let data = data.clone();
    Box::pin(async move { Ok(data) })
  }
}

struct ContentShell;

impl Evaluator for ContentShell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([Ok(Chunk::Slot(SlotName("content".into())))]))
  }
}

struct StoreEval(Arc<AtomicU32>);

impl Evaluator for StoreEval {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    self.0.fetch_add(1, Ordering::Relaxed);
    let text = match props.get("$store") {
      Some(Value::Map(store)) => format!("<{:?}>", store.keys().collect::<Vec<_>>()),
      _ => "<no store>".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(text)))]))
  }
}

#[test]
fn a_fixed_subtree_is_keyed_by_the_store_keys_it_reads_and_sees_only_those() {
  use snapfire_fsr_runtime::{subtree_shape, Reads, Static, SubtreeReads};
  let evals = Arc::new(AtomicU32::new(0));
  let count = Arc::new(AtomicU32::new(1));
  let other = Arc::new(AtomicU32::new(1));
  let mut sources = DataSources::new();
  let (c, o) = (Arc::clone(&count), Arc::clone(&other));
  sources.insert_fn("seeds", move |_p| {
    let (c, o) = (c.load(Ordering::Relaxed), o.load(Ordering::Relaxed));
    async move {
      let mut data = ValueMap::default();
      data.insert("cart/count".to_owned(), Value::int(c as i64));
      data.insert("other".to_owned(), Value::int(o as i64));
      Ok(data)
    }
  });
  let mut root = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  root.data_source = Some(DataSourceId("seeds".into()));
  let mut leaf = cached_leaf(None);
  leaf.id = NodeId(1);
  let shape = subtree_shape(&leaf);
  root.children.push((SlotName("content".into()), leaf));
  let mut reads = Reads::new();
  reads.insert(shape, SubtreeReads { class: Static::Fixed, store_keys: vec!["cart/count".to_owned()], path: false, csrf: false });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "page.tera", Arc::new(StoreEval(Arc::clone(&evals))));
  evaluators.register(|m: &ModuleId| m.path == "layout.tera", Arc::new(ContentShell));
  let rt = Runtime::builder().sources(sources).evaluators(evaluators).cache(Arc::new(MemoryCache::new())).store("seeds", Arc::new(PassThrough)).reads(reads).build();

  let render = |rt: &Arc<Runtime>| block_on(assemble(rt, &root, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let first = render(&rt);
  assert!(format!("{:?}", first.tree).contains("[\\\"cart/count\\\"]"), "the page sees the key it reads and not `other`: {:?}", first.tree);
  other.store(2, Ordering::Relaxed);
  render(&rt);
  assert_eq!(evals.load(Ordering::Relaxed), 1, "a key the page does not read changing is a hit");
  count.store(2, Ordering::Relaxed);
  render(&rt);
  assert_eq!(evals.load(Ordering::Relaxed), 2, "the key it reads changing is a miss");
}

#[test]
fn warm_renders_record_a_build_and_answer_before_the_live_cache() {
  use snapfire_fsr_runtime::WarmRenders;
  let live = Arc::new(MemoryCache::new());
  let warm = WarmRenders::new(std::collections::HashMap::new(), live.clone());
  let entry = |text: &str| CacheEntry { node: Node::raw(text), segments: Vec::new(), digest: 1 };

  block_on(live.put("k|a".to_owned(), entry("live")));
  assert_eq!(block_on(warm.get("k|a")).map(|e| e.node), Some(Node::raw("live")), "nothing warm, the live cache answers");

  warm.record(true);
  assert_eq!(block_on(warm.get("k|a")), None, "while recording every lookup misses");
  block_on(warm.put("k|a".to_owned(), entry("built")));
  warm.record(false);
  assert_eq!(block_on(warm.get("k|a")).map(|e| e.node), Some(Node::raw("built")), "the build's entry answers before the live one");
  assert_eq!(warm.len(), 1);

  block_on(warm.put("k|b".to_owned(), entry("request")));
  assert_eq!(warm.len(), 1, "a request's put reaches the live cache alone");
  assert_eq!(block_on(live.get("k|b")).map(|e| e.node), Some(Node::raw("request")));
  assert_eq!(block_on(warm.invalidate("k")), 2, "invalidation reaches the live cache alone");
  assert_eq!(block_on(warm.get("k|a")).map(|e| e.node), Some(Node::raw("built")));
}
