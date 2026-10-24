//! Plain JSON as `JSON.stringify` writes it and as a service's JSON reads.

use proptest::prelude::*;
use snapfire_fsr_core::json::{stringify, to_value, Integers};
use snapfire_fsr_core::value::typed;
use snapfire_fsr_core::{Value, ValueMap};

fn map(pairs: &[(&str, Value)]) -> Value {
  let mut out = ValueMap::default();
  for (k, v) in pairs {
    out.insert((*k).to_owned(), v.clone());
  }
  Value::Map(out)
}

fn plain(value: &Value) -> String {
  stringify(value, "", Integers::Digits).unwrap()
}

#[test]
fn numbers_are_written_as_javascript_writes_them() {
  assert_eq!(plain(&Value::F64(0.0)), "0");
  assert_eq!(plain(&Value::F64(7.0)), "7");
  assert_eq!(plain(&Value::F64(0.5)), "0.5");
  assert_eq!(plain(&Value::F64(1e21)), "1e+21");
  assert_eq!(plain(&Value::F64(f64::NAN)), "null");
  assert_eq!(plain(&Value::F64(f64::NEG_INFINITY)), "null");
  assert_eq!(plain(&Value::Int(9_007_199_254_740_993)), "9007199254740993");
  assert_eq!(stringify(&Value::Int(1), "", Integers::Refuse).unwrap_err().0, "JSON.stringify of a BigInt, which JavaScript throws on");
}

#[test]
fn undefined_and_functions_leave_an_object_and_are_null_in_an_array() {
  let action = Value::Ref { kind: snapfire_fsr_core::RefKind::Action, id: "a".to_owned() };
  assert_eq!(plain(&map(&[("a", Value::Undefined), ("b", Value::Bool(true)), ("f", action.clone())])), r#"{"b":true}"#);
  assert_eq!(plain(&Value::seq(vec![Value::Undefined, action])), "[null,null]");
  assert_eq!(plain(&map(&[("$", Value::str("m")), ("v", Value::Null)])), r#"{"$":"m","v":null}"#, "a `$` key is a key");
}

#[test]
fn the_values_javascript_gives_a_json_form_are_written_that_way() {
  let variant = |tag: &str, payload: Value| Value::Variant { tag: tag.to_owned(), payload: Some(Box::new(payload)) };
  assert_eq!(plain(&variant(typed::DATE, Value::F64(0.0))), r#""1970-01-01T00:00:00.000Z""#);
  assert_eq!(plain(&variant(typed::DATE, Value::F64(f64::NAN))), "null", "an invalid date's toJSON is null");
  assert_eq!(plain(&variant(typed::MAP, Value::seq(Vec::new()))), "{}");
  assert_eq!(plain(&variant(typed::SET, Value::seq(vec![Value::Int(1)]))), "{}");
  assert_eq!(plain(&variant(typed::URL, Value::str("https://a.example/"))), r#""https://a.example/""#);
  assert_eq!(plain(&Value::Bytes(vec![7, 8])), r#"{"0":7,"1":8}"#);
  assert_eq!(plain(&variant("Shipped", Value::str("today"))), r#"{"tag":"Shipped","payload":"today"}"#);
}

#[test]
fn an_indent_lays_the_document_out_as_javascript_does() {
  let value = map(&[("a", Value::seq(vec![Value::F64(1.0), Value::F64(2.0)])), ("b", map(&[]))]);
  assert_eq!(stringify(&value, "  ", Integers::Digits).unwrap(), "{\n  \"a\": [\n    1,\n    2\n  ],\n  \"b\": {}\n}");
}

#[test]
fn a_document_reads_with_no_tag_and_integers_as_integers() {
  let json: serde_json::Value = serde_json::from_str(r#"{"$":"f","v":1,"w":1.5,"big":18446744073709551615}"#).unwrap();
  assert_eq!(to_value(&json), map(&[("$", Value::str("f")), ("v", Value::Int(1)), ("w", Value::F64(1.5)), ("big", Value::Int(18_446_744_073_709_551_615))]));
}

fn json_value() -> impl Strategy<Value = Value> {
  let leaf = prop_oneof![
    Just(Value::Null),
    any::<bool>().prop_map(Value::Bool),
    any::<i64>().prop_map(|n| Value::Int(n.into())),
    any::<f64>().prop_map(Value::F64),
    ".*".prop_map(|s: String| Value::str(s.as_str())),
  ];
  leaf.prop_recursive(4, 32, 6, |inner| {
    prop_oneof![
      prop::collection::vec(inner.clone(), 0..6).prop_map(Value::seq),
      prop::collection::vec((".*", inner), 0..6).prop_map(|pairs: Vec<(String, Value)>| Value::Map(pairs.into_iter().collect())),
    ]
  })
}

proptest! {
  /// Whatever the writer produces is a JSON document. Reading it back and writing it again gives the same text.
  #[test]
  fn every_written_value_is_json_and_reads_back(value in json_value()) {
    let text = plain(&value);
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| TestCaseError::fail(format!("{e}: {text}")))?;
    let back = to_value(&json);
    prop_assert_eq!(plain(&back), text);
  }
}
