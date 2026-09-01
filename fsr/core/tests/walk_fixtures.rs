//! The two hand-walked pages from the design docs as literal vocabulary values.

use snapfire_fsr_core::{
  DataSourceId, Fingerprint, ModuleId, Node, NodeId, PlanNode, SlotId, SlotName, TypedArray, Value,
};

fn chart_island(series: Vec<f64>) -> Node {
  let mut props = indexmap::IndexMap::new();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(series)));
  Node::Client {
    module: ModuleId::new("components/ServerChart.tsx", "default"),
    props,
    children: Vec::new(),
    ssr: None,
  }
}

fn walked_page() -> Node {
  Node::Seq(vec![
    Node::raw("<html><head></head><body><nav></nav><main>"),
    Node::Seq(vec![
      Node::raw("<section><h1>Servers</h1><table></table>"),
      chart_island(vec![1.0, 2.5, 3.0]),
      Node::raw("</section>"),
    ]),
    Node::raw("</main></body></html>"),
  ])
}

fn single_page_plan() -> PlanNode {
  let mut page = PlanNode::new(NodeId(1), ModuleId::new("routes/dash/servers/page.tera", "default"));
  page.data_source = Some(DataSourceId("servers_loader".into()));

  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("routes/dash/layout.tera", "default"));
  layout.data_source = Some(DataSourceId("layout_loader".into()));
  layout.children.push((SlotName("content".into()), page));
  layout
}

fn streaming_plan() -> PlanNode {
  let mut chart = PlanNode::new(NodeId(2), ModuleId::new("routes/dash/servers/chart_section.tera", "default"));
  chart.data_source = Some(DataSourceId("chart_loader".into()));
  chart.deferred = true;
  chart.fallback = Some(ModuleId::new("routes/dash/servers/chart_loading.tera", "default"));

  let mut plan = single_page_plan();
  plan.children[0].1.children.push((SlotName("chart".into()), chart));
  plan
}

#[test]
fn single_page_plan_holds_what_evaluation_needs() {
  let plan = single_page_plan();
  assert_eq!(plan.children.len(), 1);
  let (slot, page) = &plan.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(page.module.to_string(), "routes/dash/servers/page.tera#default");
  assert!(!page.deferred);
  assert!(page.fallback.is_none());
}

#[test]
fn deferral_is_declared_in_the_plan() {
  let plan = streaming_plan();
  let chart = &plan.children[0].1.children[0].1;
  assert!(chart.deferred);
  assert!(chart.fallback.is_some());
  assert!(chart.data_source.is_some());
}

#[test]
fn island_props_survive_as_structure() {
  let page = walked_page();
  let Node::Seq(top) = &page else { panic!("walked page is a Seq") };
  let Node::Seq(content) = &top[1] else { panic!("content is a Seq") };
  let Node::Client { module, props, ssr, .. } = &content[1] else { panic!("island is a Client node") };
  assert_eq!(module.to_string(), "components/ServerChart.tsx#default");
  assert!(matches!(props["series"], Value::TypedArray(TypedArray::F64(_))));
  assert!(ssr.is_none(), "null evaluator leaves ssr empty");
}

#[test]
fn node_fingerprint_tracks_island_props() {
  let a = walked_page();
  let b = walked_page();
  assert_eq!(a.fingerprint(), b.fingerprint());

  let mut different = walked_page();
  if let Node::Seq(top) = &mut different {
    if let Node::Seq(content) = &mut top[1] {
      content[1] = chart_island(vec![9.0]);
    }
  }
  assert_ne!(a.fingerprint(), different.fingerprint());
}

#[test]
fn plan_fingerprint_tracks_deferral() {
  let eager = single_page_plan();
  let streaming = streaming_plan();
  assert_eq!(eager.fingerprint(), single_page_plan().fingerprint());
  assert_ne!(eager.fingerprint(), streaming.fingerprint());
}

#[test]
fn pending_carries_its_fallback_inline() {
  let pending = Node::Pending {
    slot: SlotId(1),
    fallback: Box::new(Node::raw("<div class=skl></div>")),
  };
  let Node::Pending { slot, fallback } = &pending else { unreachable!() };
  assert_eq!(*slot, SlotId(1));
  assert!(matches!(**fallback, Node::Raw(_)));
}
