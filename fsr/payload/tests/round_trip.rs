use snapfire_fsr_core::{Fingerprint, ModuleId, Node, RefKind, SlotId, TypedArray, Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, node_to_row_json, row_json_to_node, value_to_json};

fn assert_value_round_trips(value: Value) {
  let json = value_to_json(&value);
  let text = json.to_string();
  let reparsed: serde_json::Value = serde_json::from_str(&text).unwrap();
  let decoded = json_to_value(&reparsed).unwrap();
  assert_eq!(
    value.fingerprint(),
    decoded.fingerprint(),
    "lossless round trip failed for {value:?} via {text}"
  );
}

#[test]
fn scalars_round_trip() {
  assert_value_round_trips(Value::Null);
  assert_value_round_trips(Value::Bool(true));
  assert_value_round_trips(Value::int(0i64));
  assert_value_round_trips(Value::int(-42i64));
  assert_value_round_trips(Value::str("hello"));
  assert_value_round_trips(Value::str("$looks-like-a-tag"));
}

#[test]
fn wide_integers_round_trip_tagged() {
  assert_value_round_trips(Value::Int(i128::MAX));
  assert_value_round_trips(Value::Int(i128::MIN));
  assert_value_round_trips(Value::uint(u128::MAX));
  assert_value_round_trips(Value::int(9_007_199_254_740_991i64));
  assert_value_round_trips(Value::int(9_007_199_254_740_992i64));
}

#[test]
fn floats_round_trip_including_non_finite() {
  assert_value_round_trips(Value::F64(1.5));
  assert_value_round_trips(Value::F64(1.0));
  assert_value_round_trips(Value::F64(-0.0));
  assert_value_round_trips(Value::F64(f64::NAN));
  assert_value_round_trips(Value::F64(f64::INFINITY));
  assert_value_round_trips(Value::F64(f64::NEG_INFINITY));
  assert_value_round_trips(Value::F32(2.5));
  assert_value_round_trips(Value::F32(f32::NAN));
}

#[test]
fn integral_f64_stays_a_float() {
  let json = value_to_json(&Value::F64(1.0));
  let decoded = json_to_value(&json).unwrap();
  assert_eq!(decoded, Value::F64(1.0));
  assert!(matches!(decoded, Value::F64(_)), "1.0 must not collapse into Int(1)");
}

#[test]
fn bytes_and_typed_arrays_round_trip() {
  assert_value_round_trips(Value::Bytes(vec![0, 1, 2, 255]));
  assert_value_round_trips(Value::TypedArray(TypedArray::I8(vec![-1, 0, 1])));
  assert_value_round_trips(Value::TypedArray(TypedArray::U16(vec![0, 65535])));
  assert_value_round_trips(Value::TypedArray(TypedArray::I64(vec![i64::MIN, i64::MAX])));
  assert_value_round_trips(Value::TypedArray(TypedArray::F32(vec![1.0, f32::NAN])));
  assert_value_round_trips(Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, f64::NAN])));
}

#[test]
fn variants_and_refs_round_trip() {
  assert_value_round_trips(Value::Variant { tag: "Down".into(), payload: None });
  assert_value_round_trips(Value::Variant {
    tag: "Retrying".into(),
    payload: Some(Box::new(Value::int(3i64))),
  });
  assert_value_round_trips(Value::action_ref("saveServer"));
  assert_value_round_trips(Value::Ref { kind: RefKind::Module, id: "components/Star.tsx#default".into() });
}

#[test]
fn maps_round_trip_including_dollar_keys() {
  let mut plain = ValueMap::new();
  plain.insert("name".to_owned(), Value::str("web-1"));
  plain.insert("id".to_owned(), Value::int(7i64));
  assert_value_round_trips(Value::Map(plain));

  let mut tricky = ValueMap::new();
  tricky.insert("$".to_owned(), Value::str("not a tag"));
  tricky.insert("other".to_owned(), Value::int(1i64));
  assert_value_round_trips(Value::Map(tricky));
}

#[test]
fn foreign_plain_json_decodes() {
  let foreign: serde_json::Value = serde_json::from_str(r#"{"a":[1,2.5,null,"x",true]}"#).unwrap();
  let decoded = json_to_value(&foreign).unwrap();
  let Value::Map(map) = decoded else { panic!() };
  let Value::Seq(items) = &map["a"] else { panic!() };
  assert_eq!(items[0], Value::Int(1));
  assert_eq!(items[1], Value::F64(2.5));
  assert_eq!(items[2], Value::Null);
}

#[test]
fn node_rows_round_trip_the_walked_page() {
  let mut props = ValueMap::new();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
  props.insert("onSave".to_owned(), Value::action_ref("saveServer"));
  let page = Node::Seq(vec![
    Node::raw("<main>"),
    Node::Client {
      module: ModuleId::new("components/ServerChart.tsx", "default"),
      props,
      children: vec![Node::text("chart caption")],
      ssr: Some(Box::new(Node::raw("<svg></svg>"))),
    },
    Node::Pending { slot: SlotId(1), fallback: Box::new(Node::raw("<div class=skl></div>")) },
    Node::raw("</main>"),
  ]);

  let row = node_to_row_json(&page);
  let reparsed: serde_json::Value = serde_json::from_str(&row.to_string()).unwrap();
  let decoded = row_json_to_node(&reparsed).unwrap();
  assert_eq!(page.fingerprint(), decoded.fingerprint());
}
