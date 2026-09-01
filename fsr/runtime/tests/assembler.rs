use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream;
use snapfire_fsr_core::{
  Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap,
};
use snapfire_fsr_runtime::{
  assemble, AssembleError, Chunk, DataSources, Evaluator, Evaluators, NodeChunks, Runtime,
};

struct SlotShell;

impl Evaluator for SlotShell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<before>"))),
      Ok(Chunk::Slot(SlotName("head".into()))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("<after>"))),
    ]))
  }
}

fn shell_plan(children: Vec<(SlotName, PlanNode)>) -> PlanNode {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children = children;
  plan
}

fn leaf(id: u32, module: &str) -> PlanNode {
  PlanNode::new(NodeId(id), ModuleId::new(module, "default"))
}

fn shell_runtime(sources: DataSources) -> Arc<Runtime> {
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "shell.tera", Arc::new(SlotShell));
  Runtime::new(sources, evaluators)
}

#[test]
fn head_slot_fills_from_the_runtime_and_unknown_modules_fall_to_null() {
  let plan = shell_plan(vec![(SlotName("content".into()), leaf(1, "components/App.tsx"))]);
  let head = Node::raw("<title>t</title>");
  let runtime = shell_runtime(DataSources::new());

  let assembly = block_on(assemble(&runtime, &plan, &Params::new(), &head)).unwrap();
  assert!(assembly.pending.is_empty());

  let Node::Seq(parts) = assembly.tree else { panic!("shell output is a Seq") };
  assert_eq!(parts[0], Node::raw("<before>"));
  assert_eq!(parts[1], Node::raw("<title>t</title>"));
  let Node::Client { module, props, ssr, .. } = &parts[2] else {
    panic!("the .tsx child fell through to the null evaluator")
  };
  assert_eq!(module.path, "components/App.tsx");
  assert!(ssr.is_none());
  assert!(matches!(props["params"], Value::Map(_)), "null evaluator receives the merged props");
  assert_eq!(parts[3], Node::raw("<after>"));
}

#[test]
fn a_slot_with_no_child_is_an_error() {
  let plan = shell_plan(Vec::new());
  let runtime = shell_runtime(DataSources::new());
  let err = block_on(assemble(&runtime, &plan, &Params::new(), &Node::raw(""))).unwrap_err();
  assert!(matches!(err, AssembleError::MissingSlot { slot, .. } if slot == "content"));
}

#[test]
fn a_missing_data_source_is_an_error() {
  let mut plan = shell_plan(Vec::new());
  plan.data_source = Some(DataSourceId("nowhere".into()));
  let runtime = shell_runtime(DataSources::new());
  let err = block_on(assemble(&runtime, &plan, &Params::new(), &Node::raw(""))).unwrap_err();
  assert!(matches!(err, AssembleError::MissingDataSource(id) if id == "nowhere"));
}

#[test]
fn loader_data_reaches_props_and_params_ride_along() {
  struct Echo;
  impl Evaluator for Echo {
    fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
      let text = match (&props["greeting"], &props["params"]) {
        (Value::Str(g), Value::Map(p)) => match &p["section"] {
          Value::Str(s) => format!("{g} {s}"),
          _ => panic!(),
        },
        _ => panic!("loader data and params both present"),
      };
      Box::pin(stream::iter([Ok(Chunk::Node(Node::text(text)))]))
    }
  }

  let mut plan = leaf(0, "echo.tera");
  plan.data_source = Some(DataSourceId("greeting".into()));

  let mut sources = DataSources::new();
  sources.insert_fn("greeting", |_p| async {
    let mut data = ValueMap::new();
    data.insert("greeting".to_owned(), Value::str("hello"));
    Ok(data)
  });

  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "echo.tera", Arc::new(Echo));
  let runtime = Runtime::new(sources, evaluators);

  let mut params = Params::new();
  params.insert("section".to_owned(), "servers".to_owned());

  let assembly = block_on(assemble(&runtime, &plan, &params, &Node::raw(""))).unwrap();
  assert_eq!(assembly.tree, Node::text("hello servers"));
}
