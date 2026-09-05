mod common;

use advanced_tera_app::services::{self, fleet};
use advanced_tera_app::state::Fleet;
use common::{app, render};
use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::FailureKind;

#[test]
fn the_applications_contract_is_internally_valid() {
  services::contract().validate().unwrap();
}

#[test]
fn a_loader_reaches_the_backend_through_the_bound_handle() {
  let handle = services::build(Fleet::seed()).bind_anonymous();

  let mut args = ValueMap::new();
  args.insert("section".to_owned(), Value::str("servers"));
  let Value::Seq(servers) = block_on(handle.call(fleet::NAME, fleet::LIST, args)).unwrap() else {
    panic!("list returns a sequence")
  };
  assert_eq!(servers.len(), 2);
}

#[test]
fn a_call_outside_the_contract_never_reaches_the_backend() {
  let handle = services::build(Fleet::seed()).bind_anonymous();

  let err = block_on(handle.call(fleet::NAME, "purge", ValueMap::new())).unwrap_err();
  assert_eq!(err.kind, FailureKind::NotFound);

  let mut wrong = ValueMap::new();
  wrong.insert("section".to_owned(), Value::Int(1));
  let err = block_on(handle.call(fleet::NAME, fleet::LIST, wrong)).unwrap_err();
  assert_eq!(err.kind, FailureKind::Invalid);
}

#[test]
fn a_failing_backend_still_only_costs_its_own_segment() {
  let host = app();
  let html = render(&host, "/dash/down");
  assert!(html.contains("Backend unavailable"));
  assert!(html.contains("(2 servers)"), "a different capability still answers");
}
