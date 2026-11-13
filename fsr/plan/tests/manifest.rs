use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};
use snapfire_fsr_plan::{Manifest, Node, PlanError, RouteEntry, FORMAT_VERSION};

fn sample() -> PlanNode {
  let mut content = PlanNode::new(NodeId(1), ModuleId::new("app/main.tsx", "Catalog"));
  content.data_source = Some(DataSourceId("catalog".into()));
  content.error = Some(ModuleId::new("app/main.tsx", "Failed"));
  content.cache_key = Some(CacheKey("catalog_page".into()));

  let mut deferred = PlanNode::new(NodeId(2), ModuleId::new("app/main.tsx", "Recommended"));
  deferred.data_source = Some(DataSourceId("recommended".into()));
  deferred.deferred = true;
  deferred.fallback = Some(ModuleId::new("app/main.tsx", "Loading"));
  content.children.push((SlotName("aside".into()), deferred));

  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.children.push((SlotName("content".into()), content));
  shell
}

#[test]
fn a_plan_round_trips_through_the_file() {
  let manifest = Manifest::new(vec![RouteEntry {
    pattern: "/".to_owned(),
    plan: Node::from_plan(&sample()),
  }]);

  let back = Manifest::from_text(&manifest.to_sexpr()).unwrap();
  assert_eq!(back, manifest);

  let routes = back.routes().unwrap();
  assert_eq!(routes.len(), 1);
  assert_eq!(routes[0].0, "/");
  assert_eq!(routes[0].1, sample(), "the runtime gets back exactly what was written");
}

#[test]
fn absent_is_absent_rather_than_null() {
  let leaf = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  let text = Manifest::new(vec![RouteEntry { pattern: "/".into(), plan: Node::from_plan(&leaf) }]).to_sexpr();

  assert!(!text.contains("nil"), "{text}");
  assert!(!text.contains("deferred"), "a false flag is not written: {text}");
  assert!(!text.contains("slot"), "an empty child list is not written: {text}");
}

#[test]
fn the_file_names_every_source_and_module_a_host_must_bind() {
  let manifest = Manifest::new(vec![RouteEntry { pattern: "/".into(), plan: Node::from_plan(&sample()) }]);

  assert_eq!(manifest.sources(), vec!["catalog", "recommended"]);
  assert_eq!(
    manifest.modules(),
    vec![
      "shell#document",
      "app/main.tsx#Catalog",
      "app/main.tsx#Failed",
      "app/main.tsx#Recommended",
      "app/main.tsx#Loading",
    ],
    "fallback and error modules count, since an evaluator has to cover them"
  );
}

#[test]
fn a_version_the_runtime_does_not_know_is_refused() {
  assert_eq!(Manifest::from_text("(plan 99)\n").unwrap_err(), PlanError::Version { found: 99 });
  assert_eq!(FORMAT_VERSION, 7);
}

#[test]
fn a_malformed_module_id_names_itself_and_where_it_is() {
  let err = Manifest::from_text("(plan 2)\n(route /x (node 0 no-hash))\n").unwrap().routes().unwrap_err();
  assert_eq!(err, PlanError::Module { at: "/x".into(), module: "no-hash".into() });
  assert!(err.to_string().contains("`path#export`"));
}

#[test]
fn a_duplicate_node_id_within_one_route_is_refused() {
  let text = "(plan 2)\n(route /x (node 0 a#b (slot content (node 0 c#d))))\n";
  let err = Manifest::from_text(text).unwrap().routes().unwrap_err();
  assert!(matches!(err, PlanError::DuplicateNode { id: 0, .. }), "{err}");
}

#[test]
fn the_same_node_id_in_two_routes_is_fine() {
  let text = "(plan 2)\n(route /a (node 0 a#b))\n(route /b (node 0 c#d))\n";
  assert_eq!(Manifest::from_text(text).unwrap().routes().unwrap().len(), 2);
}

#[test]
fn two_children_in_one_slot_are_refused() {
  let text = "(plan 2)\n(route /x (node 0 a#b (slot content (node 1 c#d)) (slot content (node 2 e#f))))\n";
  let err = Manifest::from_text(text).unwrap().routes().unwrap_err();
  assert!(matches!(err, PlanError::DuplicateSlot { .. }), "{err}");
}

#[test]
fn a_hand_written_file_reads_the_way_it_looks() {
  let text = "(plan 2)
(route |/product/{id}|
  (node 0 shell#document
    (slot content
      (node 1 app/main.tsx#Product (source product)))))
";
  let routes = Manifest::from_text(text).unwrap().routes().unwrap();
  let (pattern, plan) = &routes[0];

  assert_eq!(pattern, "/product/{id}");
  assert_eq!(plan.module, ModuleId::new("shell", "document"));
  let (slot, child) = &plan.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(child.data_source, Some(DataSourceId("product".into())));
}

#[test]
fn lowered_rows_round_trip_with_their_bodies() {
  use snapfire_fsr_ir::{Expr, Stmt};
  use snapfire_fsr_plan::{ActionEntry, SourceEntry};

  let body = vec![Stmt::Return(Expr::object(vec![("products", Expr::Array(vec![]))]))];
  let manifest = Manifest::new(vec![])
    .with_sources(vec![SourceEntry::lowered("catalog", "app/routes/index/page.loader.ts", body.clone()), SourceEntry::rust("pricing")])
    .with_actions(vec![ActionEntry::lowered("checkout", "app/routes/cart/actions.ts", body).with_input("Empty")]);

  let text = manifest.to_sexpr();
  assert_eq!(manifest.version, FORMAT_VERSION);
  let back = Manifest::from_text(&text).unwrap();
  assert_eq!(back, manifest);
  assert_eq!(back.lowered_sources().count(), 1);
  assert_eq!(back.lowered_actions().count(), 1);
  assert_eq!(back.action_ids(), vec!["checkout"]);
}

#[test]
fn a_lowered_row_without_a_body_is_refused() {
  let err = Manifest::from_text("(plan 2)\n(source catalog lowered)\n").unwrap_err();
  assert_eq!(err, PlanError::NoBody { id: "catalog".into() });
}

#[test]
fn a_site_s_reads_carry_its_name_like_every_other_module() {
  let mut manifest = Manifest::new(Vec::new());
  manifest.reads.insert("src/ui/Box.vue#default".to_owned(), vec!["cart/count".to_owned()]);
  let site = manifest.namespaced("shop", "/shop", "shell#document");
  assert_eq!(site.reads.get("shop:src/ui/Box.vue#default"), Some(&vec!["cart/count".to_owned()]), "{:?}", site.reads);
  assert_eq!(site.reads.len(), 1);
}
