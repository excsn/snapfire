use std::sync::Arc;

use snapfire_fsr::{App, BindError, Owner, Routes};
use snapfire_fsr_core::{ModuleId, NodeId, PlanNode, SlotName, ValueMap};
use snapfire_fsr_runtime::NullEvaluator;

const MANIFEST: &str = r#"{
  "version": 1,
  "routes": [
    { "pattern": "/", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "app#Catalog", "source": "catalog" } } ] } },
    { "pattern": "/cart", "plan": { "id": 0, "module": "shell#document", "children": [
      { "slot": "content", "node": { "id": 1, "module": "app#Cart", "source": "cart" } } ] } }
  ]
}"#;

fn plan(module: &str) -> PlanNode {
  let content = PlanNode::new(NodeId(1), ModuleId::new("app", module));
  let mut shell = PlanNode::new(NodeId(0), ModuleId::new("shell", "document"));
  shell.children.push((SlotName("content".into()), content));
  shell
}

fn bound() -> snapfire_fsr::AppBuilder {
  App::from_manifest(MANIFEST)
    .unwrap()
    .source("catalog", |_ctx| async { Ok(ValueMap::new()) })
    .source("cart", |_ctx| async { Ok(ValueMap::new()) })
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
}

#[test]
fn a_plan_naming_a_source_nothing_answers_refuses_to_start() {
  let err = App::from_manifest(MANIFEST)
    .unwrap()
    .source("catalog", |_ctx| async { Ok(ValueMap::new()) })
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::Unbound { name: "cart".into() });
  assert!(err.to_string().contains("which nothing answers"));
}

#[test]
fn an_override_that_names_nothing_refuses_to_start() {
  let err = bound()
    .source_override("pricing", |_ctx| async { Ok(ValueMap::new()) })
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::OverridesNothing { name: "pricing".into() });
}

#[test]
fn the_report_says_who_answers_what() {
  let app = bound()
    .source_override("cart", |_ctx| async { Ok(ValueMap::new()) })
    .route("/about", plan("About"))
    .action("checkout", |_ctx, input| async move { Ok(input) })
    .build()
    .unwrap();

  assert_eq!(
    app.report.routes,
    vec![
      ("/".to_owned(), Owner::PlanFile),
      ("/about".to_owned(), Owner::Rust),
      ("/cart".to_owned(), Owner::PlanFile),
    ]
  );
  assert_eq!(
    app.report.sources,
    vec![("catalog".to_owned(), Owner::Rust), ("cart".to_owned(), Owner::RustOverride)]
  );
  assert_eq!(app.report.actions, vec!["checkout".to_owned()]);

  let printed = app.report.to_string();
  assert!(printed.contains("cart"), "{printed}");
  assert!(printed.contains("rust override"), "{printed}");
}

#[test]
fn a_route_the_plan_file_claims_cannot_be_added_twice() {
  let err = bound().route("/", plan("Other")).build().unwrap_err();
  assert_eq!(err, BindError::Claimed("/".into()));
  assert!(err.to_string().contains("mark the Rust one as an override"));
}

#[test]
fn replacing_a_route_is_allowed_and_reported() {
  let app = bound().route_override("/", plan("Other")).build().unwrap();
  assert_eq!(app.report.routes[0], ("/".to_owned(), Owner::RustOverride));
  assert_eq!(app.report.routes.len(), 2, "the replacement did not add a route");
}

#[test]
fn a_pattern_the_matcher_refuses_names_itself() {
  let err = bound().route("/{unclosed", plan("Other")).build().unwrap_err();
  assert!(matches!(err, BindError::Pattern { .. }), "{err}");
}

#[test]
fn routes_alone_build_without_a_plan_file() {
  let app = App::builder(Routes::new().add("/only", plan("Only")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();

  assert_eq!(app.report.routes, vec![("/only".to_owned(), Owner::Rust)]);
  assert!(app.report.sources.is_empty());
}

#[test]
fn a_bad_manifest_reports_itself_rather_than_panicking() {
  assert!(matches!(App::from_manifest("nonsense"), Err(BindError::Plan(_))));
  assert!(matches!(App::from_manifest(r#"{"version":99,"routes":[]}"#), Err(BindError::Plan(_))));
}

#[test]
fn the_plan_builder_numbers_nodes_so_the_caller_never_does() {
  use snapfire_fsr::{IntoPlan, Plan};

  let built = Plan::of("shell#document")
    .slot(
      "content",
      Plan::of("app#Catalog")
        .source("catalog")
        .error("app#Failed")
        .cache_key("catalog_page")
        .slot("aside", Plan::of("app#Recommended").source("aside").deferred().fallback("app#Loading")),
    )
    .into_plan()
    .unwrap();

  assert_eq!(built.id, NodeId(0));
  let (slot, content) = &built.children[0];
  assert_eq!(slot.0, "content");
  assert_eq!(content.id, NodeId(1), "ids run in tree order");
  assert_eq!(content.data_source.as_ref().unwrap().0, "catalog");
  assert_eq!(content.error, Some(ModuleId::new("app", "Failed")));

  let (aside_slot, aside) = &content.children[0];
  assert_eq!(aside_slot.0, "aside");
  assert_eq!(aside.id, NodeId(2));
  assert!(aside.deferred);
  assert_eq!(aside.fallback, Some(ModuleId::new("app", "Loading")));
}

#[test]
fn a_module_the_builder_cannot_parse_fails_the_build() {
  use snapfire_fsr::Plan;

  let err = App::builder(Routes::new().add("/x", Plan::of("no-hash")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap_err();

  assert_eq!(err, BindError::Module { module: "no-hash".into() });
}

#[test]
fn a_plan_node_built_by_hand_is_still_accepted() {
  let app = App::builder(Routes::new().add("/raw", plan("Raw")))
    .evaluator(|_: &ModuleId| true, Arc::new(NullEvaluator))
    .build()
    .unwrap();
  assert_eq!(app.report.routes, vec![("/raw".to_owned(), Owner::Rust)]);
}
