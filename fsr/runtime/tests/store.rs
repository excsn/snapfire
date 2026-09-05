use std::sync::Arc;

use futures::executor::block_on;
use futures_util::future::BoxFuture;
use futures_util::{stream, StreamExt};
use snapfire_fsr_core::{Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap};
use snapfire_fsr_runtime::{
  assemble, html_stream, wire_stream, Chunk, DataSources, Evaluator, Evaluators, Head, LoadError, NodeChunks, RequestCtx, Runtime, Seeds,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<body>"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</body>"))),
    ]))
  }
}

/// Renders whatever `$store` reached it, which is what a lowered `useStore` reads.
struct Page;

impl Evaluator for Page {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let seen = match props.get("$store") {
      Some(Value::Map(store)) => format!("{:?}", store.iter().map(|(k, v)| (k.clone(), v.clone())).collect::<Vec<_>>()),
      _ => "none".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<p>{seen}</p>"))))]))
  }
}

/// Seeds one key from a field of its segment's data.
struct FieldSeed(&'static str, &'static str);

impl Seeds for FieldSeed {
  fn seed(&self, _ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    let (key, field) = (self.0.to_owned(), self.1.to_owned());
    let value = data.get(&field).cloned().unwrap_or(Value::Null);
    Box::pin(async move {
      let mut out = Data::new();
      out.insert(key, value);
      Ok(out)
    })
  }
}

struct Failing;

impl Seeds for Failing {
  fn seed(&self, _ctx: &RequestCtx, _data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    Box::pin(async move { Err(LoadError { source_id: "page".to_owned(), message: "no".to_owned() }) })
  }
}

fn runtime(page_seeds: Option<Arc<dyn Seeds>>) -> Arc<Runtime> {
  let mut sources = DataSources::new();
  sources.insert_fn("layout", |_p| async move {
    let mut data = ValueMap::new();
    data.insert("count".to_owned(), Value::Int(2));
    data.insert("where".to_owned(), Value::str("layout"));
    Ok(data)
  });
  sources.insert_fn("page", |_p| async move {
    let mut data = ValueMap::new();
    data.insert("where".to_owned(), Value::str("page"));
    Ok(data)
  });
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell", Arc::new(Shell));
  evaluators.register(|m: &ModuleId| m.path == "page", Arc::new(Page));
  let mut runtime = Runtime::builder().sources(sources).evaluators(evaluators).store("layout", Arc::new(LayoutSeed));
  if let Some(seeds) = page_seeds {
    runtime = runtime.store("page", seeds);
  }
  runtime.build()
}

/// Two keys, so an inner segment can win one and leave the other.
struct LayoutSeed;

impl Seeds for LayoutSeed {
  fn seed(&self, _ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    let count = data.get("count").cloned().unwrap_or(Value::Null);
    let owner = data.get("where").cloned().unwrap_or(Value::Null);
    Box::pin(async move {
      let mut out = Data::new();
      out.insert("cart/count".to_owned(), count);
      out.insert("owner".to_owned(), owner);
      Ok(out)
    })
  }
}

fn plan(deferred: bool) -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page", "default"));
  page.data_source = Some(DataSourceId("page".into()));
  page.deferred = deferred;
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.data_source = Some(DataSourceId("layout".into()));
  shell.children.push((SlotName("content".into()), page));
  shell
}

fn head() -> Head {
  Head::new("Shop", Node::raw(""))
}

fn assembled(page_seeds: Option<Arc<dyn Seeds>>, deferred: bool) -> snapfire_fsr_runtime::Assembly {
  block_on(assemble(&runtime(page_seeds), &plan(deferred), &RequestCtx::anonymous(Params::new()), head())).unwrap()
}

#[test]
fn a_seeding_segment_reaches_the_wire_and_the_document() {
  let assembly = assembled(None, false);
  assert_eq!(assembly.store.get("cart/count"), Some(&Value::Int(2)));
  let wire: String = block_on(wire_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(wire.contains("\nT {\"cart/count\":2,\"owner\":\"layout\"}\n"), "{wire}");

  let html: String = block_on(html_stream(assembled(None, false)).collect::<Vec<_>>()).concat();
  assert!(html.contains("<script type=\"application/json\" data-sf-store>{\"cart/count\":2,\"owner\":\"layout\"}</script>"), "{html}");
}

#[test]
fn an_inner_segment_wins_the_key_it_shares_with_an_outer_one() {
  let assembly = assembled(Some(Arc::new(FieldSeed("owner", "where"))), false);
  assert_eq!(assembly.store.get("owner"), Some(&Value::str("page")));
  assert_eq!(assembly.store.get("cart/count"), Some(&Value::Int(2)), "the key only the layout sets survives");
}

#[test]
fn every_node_renders_with_the_seed_as_a_prop() {
  let html: String = block_on(html_stream(assembled(None, false)).collect::<Vec<_>>()).concat();
  assert!(html.contains("(\"cart/count\", Int(2))"), "{html}");
  assert!(html.contains("(\"owner\", Str(\"layout\"))"), "{html}");
}

#[test]
fn a_failing_seed_costs_its_keys_and_not_the_page() {
  let assembly = assembled(Some(Arc::new(Failing)), false);
  assert_eq!(assembly.store.get("cart/count"), Some(&Value::Int(2)));
  let html: String = block_on(html_stream(assembly).collect::<Vec<_>>()).concat();
  assert!(html.contains("<p>"), "{html}");
}

#[test]
fn a_deferred_segment_seeds_when_it_resolves() {
  let wire: Vec<String> = block_on(wire_stream(assembled(Some(Arc::new(FieldSeed("owner", "where"))), true)).collect());
  assert!(wire[0].contains("\nT {\"cart/count\":2,\"owner\":\"layout\"}\n"), "{}", wire[0]);
  assert!(wire[1].starts_with("S 1 "), "{}", wire[1]);
  assert!(wire[1].ends_with("\nT {\"owner\":\"page\"}\n"), "{}", wire[1]);

  let html: Vec<String> = block_on(html_stream(assembled(Some(Arc::new(FieldSeed("owner", "where"))), true)).collect());
  assert!(html[1].ends_with("<script>__sfFill(1);__sfStore({\"owner\":\"page\"})</script>"), "{}", html[1]);
}
