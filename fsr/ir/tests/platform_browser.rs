//! The Rust half of `btoa`, `atob`, `TextEncoder` and `TextDecoder`, against what Chrome answers. `fixtures/platform_chrome.json` was recorded with `browserc eval` in HeadlessChrome 153; a case Chrome throws on is `{ "throws": name }`.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Ambient, Extensions};

fn to_value(json: &serde_json::Value) -> Value {
  match json {
    serde_json::Value::Number(n) => Value::int(n.as_i64().unwrap()),
    serde_json::Value::String(s) => Value::str(s.clone()),
    serde_json::Value::Array(items) => Value::Seq(items.iter().map(to_value).collect()),
    other => panic!("{other}"),
  }
}

fn to_json(value: &Value) -> serde_json::Value {
  match value {
    Value::Str(s) => serde_json::Value::String(s.to_string()),
    Value::Int(n) => serde_json::json!(*n as i64),
    Value::Seq(items) => serde_json::Value::Array(items.iter().map(to_json).collect()),
    other => panic!("{other:?}"),
  }
}

#[test]
fn base64_and_utf8_answer_as_chrome_does() {
  let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/platform_chrome.json")).unwrap();
  let registry = Extensions::standard();
  let mut wrong = Vec::new();
  for case in fixture["cases"].as_array().unwrap() {
    let name = format!("encoding.{}", case["k"].as_str().unwrap());
    let args: Vec<Value> = case["a"].as_array().unwrap().iter().map(to_value).collect();
    let got = match registry.call(&name, &Ambient::default(), &args) {
      Ok(out) => to_json(&out),
      Err(_) => serde_json::json!({ "throws": "InvalidCharacterError" }),
    };
    if got != case["out"] {
      wrong.push(format!("{name} {}: {got}, Chrome {}", case["a"], case["out"]));
    }
  }
  assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}
