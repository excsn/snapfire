mod common;

use common::{app, csrf_in, get, location, post_form, render, session_cookie, text};
use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, SessionCell};

fn input(pairs: &[(&str, Value)]) -> Value {
  let mut map = ValueMap::new();
  for (k, v) in pairs {
    map.insert((*k).to_owned(), v.clone());
  }
  Value::Map(map)
}

#[test]
fn the_write_loop_closes() {
  let host = app();

  let before = render(&host, "/dash/servers");
  assert!(!before.contains("web-3"));

  let result = block_on(host.call_action("add_server", SessionCell::default(), input(&[("name", Value::str("web-3")), ("load", Value::F64(0.12))]))).unwrap();
  let Value::Map(out) = result else { panic!() };
  assert_eq!(out["count"], Value::Int(3));

  let after = render(&host, "/dash/servers");
  assert!(after.contains("<td>web-3</td>"), "the mutation is visible on the next render");
  assert!(after.contains("(3 servers)"), "metadata is data, so the title tracks the mutation too");
}

#[test]
fn form_shaped_string_input_coerces() {
  let host = app();
  block_on(host.call_action("add_server", SessionCell::default(), input(&[("name", Value::str("web-9")), ("load", Value::str("0.5"))]))).unwrap();
  let html = render(&host, "/dash/servers");
  assert!(html.contains("<td>web-9</td>"));
}

#[test]
fn action_failures_are_typed() {
  let host = app();

  let missing = block_on(host.call_action("nope", SessionCell::default(), Value::Map(ValueMap::new()))).unwrap_err();
  assert_eq!(missing.kind, FailureKind::NotFound);

  let invalid = block_on(host.call_action("add_server", SessionCell::default(), Value::Map(ValueMap::new()))).unwrap_err();
  assert_eq!(invalid.kind, FailureKind::Invalid);

  block_on(host.call_action("add_server", SessionCell::default(), input(&[("name", Value::str("dup")), ("load", Value::F64(0.1))]))).unwrap();
  let conflict = block_on(host.call_action("add_server", SessionCell::default(), input(&[("name", Value::str("dup")), ("load", Value::F64(0.2))]))).unwrap_err();
  assert_eq!(conflict.kind, FailureKind::Conflict);
}

#[test]
fn encoding_negotiation_only_admits_what_exists() {
  let host = app();
  let response = get(&host, "/dash/servers?__payload&enc=json", None);
  assert_eq!(response.status(), 200);
  assert!(text(response).starts_with("V {\"fmt\":1,\"enc\":\"json\"}"));
  let response = get(&host, "/dash/servers?__payload&enc=cbor", None);
  assert_eq!(response.status(), 406);
  assert!(text(response).contains("unsupported payload encoding `cbor`"));
}

#[test]
fn the_page_form_posts_the_action_and_lands_back_on_the_page() {
  let host = app();
  let response = get(&host, "/dash/servers", None);
  let cookie = session_cookie(&response);
  let token = csrf_in(&text(response));

  let response = post_form(&host, "/_sf/action/add_server", Some(&cookie), "name=web-4&load=0.9&_csrf=wrong", Some("http://localhost/dash/servers"));
  assert_eq!(response.status(), 403);

  let response = post_form(&host, "/_sf/action/add_server", Some(&cookie), &format!("name=web-4&load=0.9&_csrf={token}"), Some("http://localhost/dash/servers"));
  assert_eq!(response.status(), 303);
  assert_eq!(location(&response), "/dash/servers");

  let html = text(get(&host, "/dash/servers", Some(&cookie)));
  assert!(html.contains("<td>web-4</td>") && html.contains("(3 servers)"), "{html}");
}
