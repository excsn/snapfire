use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream;
use futures_util::StreamExt;
use snapfire_fsr_core::{
  Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap,
};
use snapfire_fsr_runtime::{
  RequestCtx,
  assemble, html_stream, wire_stream, Chunk, DataSources, Evaluator, Evaluators, NodeChunks,
  Runtime,
};

struct Shell(&'static str);

impl Evaluator for Shell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<shell>"))),
      Ok(Chunk::Slot(SlotName(self.0.into()))),
      Ok(Chunk::Node(Node::raw("</shell>"))),
    ]))
  }
}

struct Leafy(&'static str);

impl Evaluator for Leafy {
  fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
    let suffix = match props.get("late") {
      Some(Value::Str(s)) => format!("<late>{s}</late>"),
      _ => String::new(),
    };
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw(format!("{}{suffix}", self.0))))]))
  }
}

struct Skeleton;

impl Evaluator for Skeleton {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([Ok(Chunk::Node(Node::raw("<skl></skl>")))]))
  }
}

fn deferred_child(id: u32, module: &str, source: Option<&str>) -> PlanNode {
  let mut child = PlanNode::new(NodeId(id), ModuleId::new(module, "default"));
  child.deferred = true;
  child.fallback = Some(ModuleId::new("loading.tera", "default"));
  child.data_source = source.map(|s| DataSourceId(s.into()));
  child
}

fn runtime_with(evaluators: Vec<(&'static str, Arc<dyn Evaluator>)>, sources: DataSources) -> Arc<Runtime> {
  let mut evs = Evaluators::new();
  for (path, ev) in evaluators {
    let path = path.to_owned();
    evs.register(move |m: &ModuleId| m.path == path, ev);
  }
  Runtime::new(sources, evs)
}

fn collect<S: futures_util::Stream<Item = String> + Send>(s: S) -> Vec<String> {
  block_on(s.collect::<Vec<_>>())
}

#[test]
fn a_deferred_slot_streams_its_resolution_row() {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("chart".into()), deferred_child(1, "chart.tera", Some("chart_loader"))));

  let mut sources = DataSources::new();
  sources.insert_fn("chart_loader", |_p| async {
    let mut data = ValueMap::new();
    data.insert("late".to_owned(), Value::str("ready"));
    Ok(data)
  });

  let runtime = runtime_with(
    vec![
      ("shell.tera", Arc::new(Shell("chart"))),
      ("chart.tera", Arc::new(Leafy("<chart>"))),
      ("loading.tera", Arc::new(Skeleton)),
    ],
    sources,
  );

  let assembly = block_on(assemble(&runtime, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  assert_eq!(assembly.pending.len(), 1);

  let rows = collect(wire_stream(assembly));
  assert_eq!(rows.len(), 2);
  assert!(rows[0].starts_with("V {\"fmt\":1,\"enc\":\"json\"}\nN "));
  assert!(rows[0].contains("[\"p\",1,[\"r\",\"<skl></skl>\"]]"), "fallback rides inside Pending: {}", rows[0]);
  assert!(rows[0].contains("\nG {"), "segment sidecar row: {}", rows[0]);
  assert!(rows[0].contains("\"s\":1"), "deferred segment is slot-addressed: {}", rows[0]);
  assert!(rows[1].starts_with("S 1 "), "resolution row: {}", rows[1]);
  assert!(rows[1].contains("<late>ready</late>"), "deferred loader data reached the late render: {}", rows[1]);
}

#[test]
fn nested_deferral_introduces_new_slots_from_a_resolution() {
  let mut inner_host = PlanNode::new(NodeId(1), ModuleId::new("outer.tera", "default"));
  inner_host.deferred = true;
  inner_host.fallback = Some(ModuleId::new("loading.tera", "default"));
  inner_host.children.push((SlotName("inner".into()), deferred_child(2, "inner.tera", None)));

  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("outer".into()), inner_host));

  let runtime = runtime_with(
    vec![
      ("shell.tera", Arc::new(Shell("outer"))),
      ("outer.tera", Arc::new(Shell("inner"))),
      ("inner.tera", Arc::new(Leafy("<deep>"))),
      ("loading.tera", Arc::new(Skeleton)),
    ],
    DataSources::new(),
  );

  let assembly = block_on(assemble(&runtime, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let rows = collect(wire_stream(assembly));

  assert_eq!(rows.len(), 3, "tree, outer resolution, inner resolution: {rows:?}");
  assert!(rows[1].starts_with("S 1 "));
  assert!(rows[1].contains("[\"p\",2,"), "outer resolution contains the inner Pending: {}", rows[1]);
  assert!(rows[2].starts_with("S 2 "));
  assert!(rows[2].contains("<deep>"));
}

#[test]
fn a_failed_deferred_loader_resolves_to_its_error_node() {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("chart".into()), deferred_child(1, "chart.tera", Some("broken"))));

  let mut sources = DataSources::new();
  sources.insert_fn("broken", |_p| async {
    Err(snapfire_fsr_runtime::LoadError { source_id: "broken".into(), message: "backend down".into() })
  });

  let runtime = runtime_with(
    vec![
      ("shell.tera", Arc::new(Shell("chart"))),
      ("chart.tera", Arc::new(Leafy("<chart>"))),
      ("loading.tera", Arc::new(Skeleton)),
    ],
    sources,
  );

  let assembly = block_on(assemble(&runtime, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let rows = collect(wire_stream(assembly));
  assert_eq!(rows.len(), 2);
  assert!(rows[1].contains("data-sf-error"), "segment error node, not a dead response: {}", rows[1]);
  assert!(rows[1].contains("backend down"));
}

#[test]
fn html_stream_fills_late_slots_and_keeps_island_ids_unique() {
  struct IslandLeaf;
  impl Evaluator for IslandLeaf {
    fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
      Box::pin(stream::iter([Ok(Chunk::Node(Node::Client {
        module: ModuleId::new("components/Chart.tsx", "default"),
        props: ValueMap::new(),
        children: Vec::new(),
        ssr: None,
      }))]))
    }
  }

  struct ShellWithIsland;
  impl Evaluator for ShellWithIsland {
    fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
      Box::pin(stream::iter([
        Ok(Chunk::Node(Node::Client {
          module: ModuleId::new("components/Nav.tsx", "default"),
          props: ValueMap::new(),
          children: Vec::new(),
          ssr: None,
        })),
        Ok(Chunk::Slot(SlotName("chart".into()))),
      ]))
    }
  }

  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("shell.tera", "default"));
  plan.children.push((SlotName("chart".into()), deferred_child(1, "chart.tera", None)));

  let runtime = runtime_with(
    vec![
      ("shell.tera", Arc::new(ShellWithIsland)),
      ("chart.tera", Arc::new(IslandLeaf)),
      ("loading.tera", Arc::new(Skeleton)),
    ],
    DataSources::new(),
  );

  let assembly = block_on(assemble(&runtime, &plan, &RequestCtx::anonymous(Params::new()), &Node::raw(""))).unwrap();
  let chunks = collect(html_stream(assembly));

  assert_eq!(chunks.len(), 2);
  assert!(chunks[0].contains("<div data-sf-slot=\"1\"><skl></skl></div>"));
  assert!(chunks[0].contains("function __sfFill"), "fill script ships with the first chunk");
  assert!(chunks[0].contains("sf-i0"), "initial island takes id 0");
  assert!(chunks[1].starts_with("<template data-sf-fill=\"1\">"));
  assert!(chunks[1].ends_with("<script>__sfFill(1)</script>"));
  assert!(chunks[1].contains("sf-i1"), "late island continues the id sequence: {}", chunks[1]);
  assert!(!chunks[1].contains("sf-i0\""), "no id collision across chunks");
}
