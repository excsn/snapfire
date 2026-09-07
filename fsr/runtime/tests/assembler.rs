use std::sync::Arc;

use futures::executor::block_on;
use futures_util::stream;
use snapfire_fsr_core::{Data, DataSourceId, ModuleId, Node, NodeId, Params, PlanNode, SlotName, Value, ValueMap};
use snapfire_fsr_runtime::{
  AssembleError, Chunk, DataSources, Evaluator, Evaluators, NodeChunks, RequestCtx, Runtime, assemble,
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

  let assembly = block_on(assemble(&runtime, &plan, &RequestCtx::anonymous(Params::new()), &head)).unwrap();
  assert!(assembly.pending.is_empty());

  let Node::Seq(parts) = assembly.tree else {
    panic!("shell output is a Seq")
  };
  assert_eq!(parts[0], Node::raw("<before>"));
  assert_eq!(parts[1], Node::raw("<title>t</title>"));
  let Node::Client { module, props, ssr, .. } = &parts[2] else {
    panic!("the .tsx child fell through to the null evaluator")
  };
  assert_eq!(module.path, "components/App.tsx");
  assert!(ssr.is_none());
  assert!(
    matches!(props["params"], Value::Map(_)),
    "null evaluator receives the merged props"
  );
  assert_eq!(parts[3], Node::raw("<after>"));
}

#[test]
fn a_slot_with_no_child_is_an_error() {
  let plan = shell_plan(Vec::new());
  let runtime = shell_runtime(DataSources::new());
  let err = block_on(assemble(
    &runtime,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap_err();
  assert!(matches!(err, AssembleError::MissingSlot { slot, .. } if slot == "content"));
}

#[test]
fn a_missing_data_source_is_an_error() {
  let mut plan = shell_plan(Vec::new());
  plan.data_source = Some(DataSourceId("nowhere".into()));
  let runtime = shell_runtime(DataSources::new());
  let err = block_on(assemble(
    &runtime,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap_err();
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

  let assembly = block_on(assemble(
    &runtime,
    &plan,
    &RequestCtx::anonymous(params),
    &Node::raw(""),
  ))
  .unwrap();
  assert_eq!(assembly.tree, Node::text("hello servers"));
}

struct NamedShell;

impl Evaluator for NamedShell {
  fn evaluate(&self, _module: &ModuleId, _props: &Data) -> NodeChunks {
    Box::pin(stream::iter([
      Ok(Chunk::Node(Node::raw("<a>"))),
      Ok(Chunk::Slot(SlotName("content".into()))),
      Ok(Chunk::Node(Node::raw("<b>"))),
      Ok(Chunk::Slot(SlotName("modal".into()))),
      Ok(Chunk::Node(Node::raw("<c>"))),
    ]))
  }
}

fn named_runtime() -> Arc<Runtime> {
  let mut evaluators = Evaluators::new();
  evaluators.register(|m: &ModuleId| m.path == "layout.tera", Arc::new(NamedShell));
  Runtime::new(DataSources::new(), evaluators)
}

fn layout_plan(children: Vec<(SlotName, PlanNode)>, keep: Vec<&str>) -> PlanNode {
  let mut plan = PlanNode::new(NodeId(0), ModuleId::new("layout.tera", "default"));
  plan.children = children;
  plan.keep = keep.into_iter().map(|k| SlotName(k.into())).collect();
  plan
}

#[test]
fn a_named_slot_the_plan_leaves_unfilled_renders_nothing_and_names_the_segments_it_fills() {
  let plan = layout_plan(vec![(SlotName("content".into()), leaf(1, "page.tsx"))], Vec::new());
  let assembly = block_on(assemble(
    &named_runtime(),
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  let Node::Seq(parts) = &assembly.tree else {
    panic!("{:?}", assembly.tree)
  };
  assert_eq!(parts.len(), 4, "the empty modal slot contributes nothing: {parts:?}");
  assert_eq!(parts[2], Node::raw("<b>"));
  assert_eq!(parts[3], Node::raw("<c>"));
  let sidecar = snapfire_fsr_runtime::segments_to_json(&assembly.segments);
  assert_eq!(sidecar["c"].as_array().unwrap().len(), 1);
  assert_eq!(sidecar["c"][0]["n"], "content");
  assert!(sidecar.get("keep").is_none());
}

#[test]
fn a_kept_slot_renders_nothing_and_the_sidecar_says_which() {
  let plan = layout_plan(
    vec![(SlotName("modal".into()), leaf(1, "page.modal.tsx"))],
    vec!["content"],
  );
  let assembly = block_on(assemble(
    &named_runtime(),
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  let Node::Seq(parts) = &assembly.tree else {
    panic!("{:?}", assembly.tree)
  };
  assert_eq!(parts.len(), 4);
  assert!(
    matches!(&parts[2], Node::Client { module, .. } if module.path == "page.modal.tsx"),
    "{:?}",
    parts[2]
  );
  let sidecar = snapfire_fsr_runtime::segments_to_json(&assembly.segments);
  assert_eq!(sidecar["keep"], serde_json::json!(["content"]));
  assert_eq!(sidecar["c"][0]["n"], "modal");
}

#[test]
fn a_node_with_children_learns_which_slots_the_plan_fills_or_keeps() {
  use parking_lot::Mutex;
  struct Recording(Arc<Mutex<Vec<Data>>>);
  impl Evaluator for Recording {
    fn evaluate(&self, _module: &ModuleId, props: &Data) -> NodeChunks {
      self.0.lock().push(props.clone());
      Box::pin(stream::iter([Ok(Chunk::Node(Node::raw("<x>")))]))
    }
  }
  let seen = Arc::new(Mutex::new(Vec::new()));
  let mut evaluators = Evaluators::new();
  evaluators.register(|_: &ModuleId| true, Arc::new(Recording(Arc::clone(&seen))));
  let runtime = Runtime::new(DataSources::new(), evaluators);

  let plan = layout_plan(
    vec![(SlotName("modal".into()), leaf(1, "page.modal.tsx"))],
    vec!["content", "promo"],
  );
  block_on(assemble(
    &runtime,
    &plan,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();
  let props = seen.lock();
  assert_eq!(
    props[0].get("$slots"),
    Some(&Value::Seq(vec![
      Value::str("modal"),
      Value::str("content"),
      Value::str("promo")
    ])),
    "children first, then the kept ones"
  );
  assert!(
    props.iter().skip(1).all(|p| p.get("$slots").is_none()),
    "a leaf has no slots to name: {:?}",
    props
  );
}

/// A layout with one pane that reads the query and one that never does, which
/// is the shape GAPS 9.56 was found on.
struct Panes;

impl Evaluator for Panes {
  fn evaluate(&self, module: &ModuleId, props: &Data) -> NodeChunks {
    let view = match props.get("view") {
      Some(Value::Str(v)) => v.clone(),
      _ => String::new(),
    };
    Box::pin(stream::iter(match module.path.as_str() {
      "rail.tsx" => vec![Ok(Chunk::Node(Node::text(format!("rail:{view}"))))],
      "contacts.tsx" => vec![Ok(Chunk::Node(Node::text("contacts")))],
      _ => vec![
        Ok(Chunk::Node(Node::raw("<main>"))),
        Ok(Chunk::Slot(SlotName("rail".into()))),
        Ok(Chunk::Slot(SlotName("contacts".into()))),
        Ok(Chunk::Node(Node::raw("</main>"))),
      ],
    }))
  }
}

#[test]
fn a_segment_digest_says_what_came_out_the_same_when_the_key_did_not() {
  let render = |view: &str| {
    let mut sources = DataSources::new();
    let held = view.to_owned();
    sources.insert_fn("view", move |_p| {
      let held = held.clone();
      async move {
        let mut data = ValueMap::new();
        data.insert("view".to_owned(), Value::str(held));
        Ok(data)
      }
    });
    let mut evaluators = Evaluators::new();
    evaluators.register(|m: &ModuleId| m.path.ends_with(".tsx"), Arc::new(Panes));
    let runtime = Runtime::new(sources, evaluators);

    let mut rail = leaf(1, "rail.tsx");
    rail.data_source = Some(DataSourceId("view".into()));
    let mut plan = PlanNode::new(NodeId(0), ModuleId::new("wave.tsx", "default"));
    plan.children = vec![
      (SlotName("rail".into()), rail),
      (SlotName("contacts".into()), leaf(2, "contacts.tsx")),
    ];

    let mut ctx = RequestCtx::anonymous(Params::new());
    ctx.query.insert("view".to_owned(), view.to_owned());
    let assembly = block_on(assemble(&runtime, &plan, &ctx, &Node::raw(""))).unwrap();
    snapfire_fsr_runtime::segments_to_json(&assembly.segments)
  };

  let inbox = render("inbox");
  let mine = render("mine");

  let pane = |side: &serde_json::Value, name: &str| {
    let found = side["c"]
      .as_array()
      .unwrap()
      .iter()
      .find(|c| c["n"] == name)
      .unwrap_or_else(|| panic!("no {name} pane in {side}"));
    (found["k"].as_str().unwrap().to_owned(), found["d"].as_str().unwrap().to_owned())
  };

  let (inbox_rail, inbox_rail_d) = pane(&inbox, "rail");
  let (mine_rail, mine_rail_d) = pane(&mine, "rail");
  assert_ne!(inbox_rail, mine_rail, "the whole query is in every key, so every key moved");
  assert_ne!(inbox_rail_d, mine_rail_d, "the rail reads the view, so it rendered something else");

  let (inbox_contacts, inbox_contacts_d) = pane(&inbox, "contacts");
  let (mine_contacts, mine_contacts_d) = pane(&mine, "contacts");
  assert_ne!(inbox_contacts, mine_contacts, "its key moved with the query all the same");
  assert_eq!(inbox_contacts_d, mine_contacts_d, "and its digest did not, because it renders the same either way");

  assert_eq!(
    inbox["d"], mine["d"],
    "the layout's own output holds no pane, so a pane changing leaves it alone"
  );
}

/// The engineless configuration: no evaluator renders anything, so every node
/// is the browser's. A layout is then a `Client` node whose page must still
/// reach the document.
#[test]
fn a_module_the_browser_owns_still_offers_its_plan_children_a_region() {
  let mut layout = PlanNode::new(NodeId(0), ModuleId::new("routes/layout.tsx", "default"));
  layout.children = vec![
    (SlotName("content".into()), leaf(1, "routes/page.tsx")),
    (SlotName("modal".into()), leaf(2, "routes/slots/modal/page.tsx")),
  ];
  let runtime = Runtime::new(DataSources::new(), Evaluators::new());
  let assembly = block_on(assemble(
    &runtime,
    &layout,
    &RequestCtx::anonymous(Params::new()),
    &Node::raw(""),
  ))
  .unwrap();

  let Node::Client { module, children, .. } = &assembly.tree else {
    panic!("the null evaluator hands the module to the browser: {:?}", assembly.tree)
  };
  assert_eq!(module.path, "routes/layout.tsx");
  assert_eq!(children.len(), 2, "one region per slot the plan fills: {children:?}");

  let region = |i: usize| {
    let Node::Seq(parts) = &children[i] else { panic!("{:?}", children[i]) };
    let (Node::Raw(open), Node::Raw(close)) = (&parts[0], &parts[2]) else { panic!("{parts:?}") };
    (open.0.clone(), parts[1].clone(), close.0.clone())
  };
  let (open, page, close) = region(0);
  assert_eq!(open, "<sf-s>", "the page's region is bare, which is what the mounter reads as children");
  assert_eq!(close, "</sf-s>");
  assert!(
    matches!(&page, Node::Client { module, .. } if module.path == "routes/page.tsx"),
    "the page is inside the region rather than dropped: {page:?}"
  );
  let (open, modal, _) = region(1);
  assert_eq!(open, "<sf-s data-sf-name=\"modal\">", "a parallel segment's region is named, which the mounter reads as a prop");
  assert!(matches!(&modal, Node::Client { module, .. } if module.path == "routes/slots/modal/page.tsx"), "{modal:?}");

  let sidecar = snapfire_fsr_runtime::segments_to_json(&assembly.segments);
  let children = sidecar["c"].as_array().unwrap();
  assert_eq!(children.len(), 2, "each one is a segment a navigation can diff: {sidecar}");
  assert_eq!(children[0]["n"], "content");
  assert_eq!(children[0]["p"], serde_json::json!([0, 1]), "its path walks the island's children and then the region's parts");
  assert_eq!(children[1]["n"], "modal");
  assert_eq!(children[1]["p"], serde_json::json!([1, 1]));
}
