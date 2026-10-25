use snapfire_fsr_core::{Fingerprint, ModuleId, Node, Place, RefKind, SlotId, TypedArray, Value, ValueMap};
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
  let mut plain = ValueMap::default();
  plain.insert("name".to_owned(), Value::str("web-1"));
  plain.insert("id".to_owned(), Value::int(7i64));
  assert_value_round_trips(Value::Map(plain));

  let mut tricky = ValueMap::default();
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
  let mut props = ValueMap::default();
  props.insert("series".to_owned(), Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
  props.insert("onSave".to_owned(), Value::action_ref("saveServer"));
  let page = Node::Seq(vec![
    Node::raw("<main>"),
    Node::Client {
      module: ModuleId::new("components/ServerChart.tsx", "default"),
      props,
      children: vec![Node::raw("<svg></svg>")],
      rendered: true,
    },
    Node::Pending { slot: SlotId(1), fallback: Box::new(Node::raw("<div class=skl></div>")) },
    Node::Placed { target: Place::Body, nodes: vec![Node::raw("<dialog>"), Node::text("a < b"), Node::raw("</dialog>")] },
    Node::Placed { target: Place::Named("modals".to_owned()), nodes: vec![Node::raw("<p>m</p>")] },
    Node::raw("<div id=\"modals\">"),
    Node::Outlet("modals".to_owned()),
    Node::raw("</div></main>"),
  ]);

  let row = node_to_row_json(&page);
  let reparsed: serde_json::Value = serde_json::from_str(&row.to_string()).unwrap();
  let decoded = row_json_to_node(&reparsed).unwrap();
  assert_eq!(page.fingerprint(), decoded.fingerprint());
}

#[test]
fn a_typed_variant_decodes_to_the_plain_value_its_payload_stands_for() {
  let decode = |text: &str| json_to_value(&serde_json::from_str::<serde_json::Value>(text).unwrap());
  let mut map = ValueMap::default();
  map.insert("alice".to_owned(), Value::int(100i64));
  map.insert("1".to_owned(), Value::str("one"));
  assert_eq!(decode(r#"{"$":"var","t":"$map","p":[["alice",100],[1,"one"]]}"#).unwrap(), Value::Map(map));
  assert_eq!(decode(r#"{"$":"var","t":"$set","p":["react","next"]}"#).unwrap(), Value::seq(vec![Value::str("react"), Value::str("next")]));
  assert_eq!(decode(r#"{"$":"var","t":"$date","p":1750000000000}"#).unwrap(), Value::int(1_750_000_000_000i64));
  assert_eq!(decode(r#"{"$":"var","t":"$bigint","p":{"$":"i","v":"9007199254740993"}}"#).unwrap(), Value::int(9_007_199_254_740_993i64));
  assert_eq!(decode(r#"{"$":"var","t":"$bigint","p":"12"}"#).unwrap(), Value::int(12i64));
  assert!(decode(r#"{"$":"var","t":"$map","p":"pairs"}"#).is_err());
  assert!(decode(r#"{"$":"var","t":"$date"}"#).is_err());
  assert_eq!(decode(r#"{"$":"var","t":"Down"}"#).unwrap(), Value::Variant { tag: "Down".to_owned(), payload: None });
}

#[test]
fn undefined_is_left_out_of_an_object_and_tagged_anywhere_else() {
  let mut props = ValueMap::default();
  props.insert("policy".to_owned(), Value::Undefined);
  props.insert("none".to_owned(), Value::Null);
  let json = value_to_json(&Value::Map(props));
  assert_eq!(json.to_string(), r#"{"none":null}"#);
  let decoded = json_to_value(&json).unwrap();
  let Value::Map(decoded) = decoded else { panic!("{decoded:?}") };
  assert_eq!(decoded.get("policy"), None);
  assert_eq!(decoded.get("none"), Some(&Value::Null));
  assert_value_round_trips(Value::Undefined);
  assert_value_round_trips(Value::seq(vec![Value::Undefined, Value::Null]));
  assert_eq!(value_to_json(&Value::seq(vec![Value::Undefined, Value::Null])).to_string(), r#"[{"$":"undef"},null]"#);
}
