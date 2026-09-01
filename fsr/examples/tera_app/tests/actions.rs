use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{ActionErrorKind, RequestCtx};
use tera_app::{build_app, call_action, negotiate_encoding, render, AppError, RenderMode};

fn input(pairs: &[(&str, Value)]) -> Value {
  let mut map = ValueMap::new();
  for (k, v) in pairs {
    map.insert((*k).to_owned(), v.clone());
  }
  Value::Map(map)
}

#[test]
fn the_write_loop_closes() {
  let app = build_app(Duration::ZERO);

  let before = block_on(render(&app, "/dash/servers", RenderMode::Html)).unwrap();
  assert!(!before.contains("web-3"));

  let result = block_on(call_action(
    &app,
    "add_server",
    RequestCtx::default(),
    input(&[("name", Value::str("web-3")), ("load", Value::F64(0.12))]),
  ))
  .unwrap();
  let Value::Map(out) = result else { panic!() };
  assert_eq!(out["count"], Value::Int(3));

  let after = block_on(render(&app, "/dash/servers", RenderMode::Html)).unwrap();
  assert!(after.contains("<td>web-3</td>"), "the mutation is visible on the next render");
  assert!(after.contains("(3 servers)"), "metadata is data, so the title tracks the mutation too");
}

#[test]
fn form_shaped_string_input_coerces() {
  let app = build_app(Duration::ZERO);
  block_on(call_action(
    &app,
    "add_server",
    RequestCtx::default(),
    input(&[("name", Value::str("web-9")), ("load", Value::str("0.5"))]),
  ))
  .unwrap();
  let html = block_on(render(&app, "/dash/servers", RenderMode::Html)).unwrap();
  assert!(html.contains("<td>web-9</td>"));
}

#[test]
fn action_failures_are_typed() {
  let app = build_app(Duration::ZERO);

  let missing = block_on(call_action(&app, "nope", RequestCtx::default(), Value::Map(ValueMap::new()))).unwrap_err();
  assert_eq!(missing.kind, ActionErrorKind::NotFound);

  let invalid = block_on(call_action(&app, "add_server", RequestCtx::default(), Value::Map(ValueMap::new()))).unwrap_err();
  assert_eq!(invalid.kind, ActionErrorKind::Invalid);

  block_on(call_action(
    &app,
    "add_server",
    RequestCtx::default(),
    input(&[("name", Value::str("dup")), ("load", Value::F64(0.1))]),
  ))
  .unwrap();
  let conflict = block_on(call_action(
    &app,
    "add_server",
    RequestCtx::default(),
    input(&[("name", Value::str("dup")), ("load", Value::F64(0.2))]),
  ))
  .unwrap_err();
  assert_eq!(conflict.kind, ActionErrorKind::Conflict);
}

#[test]
fn encoding_negotiation_only_admits_what_exists() {
  assert_eq!(negotiate_encoding(None).unwrap(), "json");
  assert_eq!(negotiate_encoding(Some("json")).unwrap(), "json");
  let err = negotiate_encoding(Some("cbor")).unwrap_err();
  assert!(matches!(err, AppError::UnsupportedEncoding(e) if e == "cbor"));
}
