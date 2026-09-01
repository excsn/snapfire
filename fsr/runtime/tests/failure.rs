use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream;
use snapfire_fsr_core::{
  CacheKey, Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value,
  ValueMap,
};
use snapfire_fsr_runtime::{
  RequestCtx,
  assemble, AssembleError, Chunk, DataSources, Evaluator, Evaluators, LoadError, MemoryCache,
  NodeChunks, Runtime,
};

struct Shell;

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<layout>"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("</layout>"))),
    ]))
  }
}

struct Page;

impl Evaluator for Page {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let body = match props.get("body") {
      Some(Value::Str(s)) => s.clone(),
      _ => "?".to_owned(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<page>{body}</page>"))))]))
  }
}

struct ErrorPartial;

impl Evaluator for ErrorPartial {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let message = match props.get("error") {
      Some(Value::Str(s)) => s.clone(),
      _ => panic!("error module receives the failure message"),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("<oops>{message}</oops>"))))]))
  }
}

fn evaluators() -> Evaluators {
  let mut evs = Evaluators::new();
  evs.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(Shell));
  evs.register(|m: &ModuleId| m.path == "page.tera", Arc::new(Page));
  evs.register(|m: &ModuleId| m.path == "error.tera", Arc::new(ErrorPartial));
  evs
}

fn plan_with_page(error_module: bool) -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("page.tera", "default"));
  page.data_source = Some(DataSourceId("page_loader".into()));
  if error_module {
    page.error = Some(ModuleId::new("error.tera", "default"));
  }
  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  layout.children.push((SlotName("content".into()), page));
  layout
}

fn failing_sources() -> DataSources {
  let mut sources = DataSources::new();
  sources.insert_fn("page_loader", |_p| async {
    Err(LoadError { source_id: "page_loader".into(), message: "backend down".into() })
  });
  sources
}

#[test]
fn an_eager_loader_failure_degrades_one_segment_never_the_page() {
  let rt = Runtime::new(failing_sources(), evaluators());
  let assembly = block_on(assemble(&rt, &plan_with_page(false), &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();

  let Node::Seq(parts) = &assembly.tree else { panic!() };
  assert_eq!(parts[0], Node::raw("<layout>"), "the layout still renders");
  assert_eq!(parts[2], Node::raw("</layout>"));
  let rendered = format!("{:?}", parts[1]);
  assert!(rendered.contains("data-sf-error"), "the failed segment became its error node: {rendered}");
  assert!(rendered.contains("backend down"));
}

#[test]
fn the_plan_error_module_renders_with_the_failure_message() {
  let rt = Runtime::new(failing_sources(), evaluators());
  let assembly = block_on(assemble(&rt, &plan_with_page(true), &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let Node::Seq(parts) = &assembly.tree else { panic!() };
  let rendered = format!("{:?}", parts[1]);
  assert!(rendered.contains("<oops>"), "custom error partial rendered: {rendered}");
  assert!(rendered.contains("backend down"));
}

#[test]
fn a_missing_data_source_stays_a_hard_error() {
  let rt = Runtime::new(DataSources::new(), evaluators());
  let err = block_on(assemble(&rt, &plan_with_page(false), &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap_err();
  assert!(matches!(err, AssembleError::MissingDataSource(_)), "misconfiguration is not a runtime degrade");
}

#[test]
fn a_failed_subtree_is_never_cached() {
  let healthy = Arc::new(AtomicBool::new(false));
  let mut sources = DataSources::new();
  let flag = Arc::clone(&healthy);
  sources.insert_fn("page_loader", move |_p| {
    let ok = flag.load(Ordering::Relaxed);
    async move {
      if ok {
        let mut data = ValueMap::new();
        data.insert("body".to_owned(), Value::str("recovered"));
        Ok(data)
      } else {
        Err(LoadError { source_id: "page_loader".into(), message: "backend down".into() })
      }
    }
  });

  let mut plan = plan_with_page(false);
  plan.children[0].1.cache_key = Some(CacheKey("page".into()));

  let rt = Runtime::builder()
    .sources(sources)
    .evaluators(evaluators())
    .cache(Arc::new(MemoryCache::new()))
    .build();

  let broken = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  assert!(format!("{:?}", broken.tree).contains("data-sf-error"));

  healthy.store(true, Ordering::Relaxed);
  let recovered = block_on(assemble(&rt, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  assert!(
    format!("{:?}", recovered.tree).contains("recovered"),
    "no poisoned cache entry survives the failure: {:?}",
    recovered.tree
  );
}
