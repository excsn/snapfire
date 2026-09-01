use std::time::Duration;

use advanced_tera_app::{build_app, render, services, RenderMode};
use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;

#[test]
fn the_applications_contract_is_internally_valid() {
  services::contract().validate().unwrap();
}

#[test]
fn a_loader_reaches_the_backend_through_the_bound_handle() {
  let app = build_app(Duration::ZERO);
  let handle = app.services().bind_anonymous();

  let mut args = ValueMap::new();
  args.insert("section".to_owned(), Value::str("servers"));
  let Value::Seq(servers) = block_on(handle.call(services::FLEET, "list", args)).unwrap() else {
    panic!("list returns a sequence")
  };
  assert_eq!(servers.len(), 2);
}

#[test]
fn a_call_outside_the_contract_never_reaches_the_backend() {
  let app = build_app(Duration::ZERO);
  let handle = app.services().bind_anonymous();

  let err = block_on(handle.call(services::FLEET, "purge", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::NotFound);

  let mut wrong = ValueMap::new();
  wrong.insert("section".to_owned(), Value::Int(1));
  let err = block_on(handle.call(services::FLEET, "list", wrong)).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
}

#[test]
fn a_failing_backend_still_only_costs_its_own_segment() {
  let app = build_app(Duration::ZERO);
  let html = block_on(render(&app, "/dash/down", RenderMode::Html)).unwrap();
  assert!(html.contains("Backend unavailable"));
  assert!(html.contains("(2 servers)"), "a different capability still answers");
}
