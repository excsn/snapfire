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

  let back = Manifest::from_json(&manifest.to_json()).unwrap();
  assert_eq!(back, manifest);

  let routes = back.routes().unwrap();
  assert_eq!(routes.len(), 1);
  assert_eq!(routes[0].0, "/");
  assert_eq!(routes[0].1, sample(), "the runtime gets back exactly what was written");
}

#[test]
fn absent_is_absent_rather_than_null() {
  let leaf = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  let json = Manifest::new(vec![RouteEntry { pattern: "/".into(), plan: Node::from_plan(&leaf) }]).to_json();

  assert!(!json.contains("null"), "{json}");
  assert!(!json.contains("deferred"), "a false flag is not written: {json}");
  assert!(!json.contains("children"), "an empty child list is not written: {json}");
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
  let json = r#"{"version":99,"routes":[]}"#;
  assert_eq!(Manifest::from_json(json).unwrap_err(), PlanError::Version { found: 99 });
  assert_eq!(FORMAT_VERSION, 1);
}

#[test]
fn a_malformed_module_id_names_itself_and_where_it_is() {
  let json = r#"{"version":1,"routes":[{"pattern":"/x","plan":{"id":0,"module":"no-hash"}}]}"#;
  let err = Manifest::from_json(json).unwrap().routes().unwrap_err();
  assert_eq!(err, PlanError::Module { at: "/x".into(), module: "no-hash".into() });
  assert!(err.to_string().contains("`path#export`"));
}

#[test]
fn a_duplicate_node_id_within_one_route_is_refused() {
  let json = r#"{"version":1,"routes":[{"pattern":"/x","plan":{"id":0,"module":"a#b",
    "children":[{"slot":"content","node":{"id":0,"module":"c#d"}}]}}]}"#;
  let err = Manifest::from_json(json).unwrap().routes().unwrap_err();
  assert!(matches!(err, PlanError::DuplicateNode { id: 0, .. }), "{err}");
}

#[test]
fn the_same_node_id_in_two_routes_is_fine() {
  let json = r#"{"version":1,"routes":[
    {"pattern":"/a","plan":{"id":0,"module":"a#b"}},
    {"pattern":"/b","plan":{"id":0,"module":"c#d"}}]}"#;
  assert_eq!(Manifest::from_json(json).unwrap().routes().unwrap().len(), 2);
}

#[test]
fn two_children_in_one_slot_are_refused() {
  let json = r#"{"version":1,"routes":[{"pattern":"/x","plan":{"id":0,"module":"a#b","children":[
    {"slot":"content","node":{"id":1,"module":"c#d"}},
    {"slot":"content","node":{"id":2,"module":"e#f"}}]}}]}"#;
  let err = Manifest::from_json(json).unwrap().routes().unwrap_err();
  assert!(matches!(err, PlanError::DuplicateSlot { .. }), "{err}");
}

#[test]
fn a_hand_written_file_reads_the_way_it_looks() {
  let json = r#"{
    "version": 1,
    "routes": [
      { "pattern": "/product/{id}",
        "plan": { "id": 0, "module": "shell#document", "children": [
          { "slot": "content",
            "node": { "id": 1, "module": "app/main.tsx#Product", "source": "product" } }
        ] } }
    ]
  }"#;
  let routes = Manifest::from_json(json).unwrap().routes().unwrap();
  let (pattern, plan) = &routes[0];

  assert_eq!(pattern, "/product/{id}");
  assert_eq!(plan.module, ModuleId::new("shell", "document"));
  let (slot, child) = &plan.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(child.data_source, Some(DataSourceId("product".into())));
}
