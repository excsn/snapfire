//! The wire under generated input. Everything here crosses a network: a
//! browser posts an action payload and reads a page's rows, so a decoder that
//! panics is a request that kills the worker rather than answering 400.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use serde_json::json;
use snapfire_fsr_core::{Html, ModuleId, Node, SlotId, TypedArray, Value, ValueMap, ValueSeq};
use snapfire_fsr_payload::{
  html_serialize, json_to_value, node_to_row_json, row_json_to_node, serialize_page, value_to_json,
};

fn text() -> BoxedStrategy<String> {
  prop_oneof![
    3 => "[a-zA-Z0-9 ._-]{0,12}",
    2 => "(?s).{0,10}",
    2 => prop_oneof![
      Just(String::new()),
      Just("<script>".to_owned()),
      Just("</script>".to_owned()),
      Just("\"".to_owned()),
      Just("'".to_owned()),
      Just("&".to_owned()),
      Just("&amp;".to_owned()),
      Just("<!--".to_owned()),
      Just("\u{0}".to_owned()),
      Just("\u{2028}".to_owned()),
      Just("é🌍".to_owned()),
      Just("\\u003c".to_owned()),
    ],
  ]
  .boxed()
}

fn typed_array() -> BoxedStrategy<TypedArray> {
  prop_oneof![
    prop::collection::vec(any::<i8>(), 0..4).prop_map(TypedArray::I8),
    prop::collection::vec(any::<u8>(), 0..4).prop_map(TypedArray::U8),
    prop::collection::vec(any::<i16>(), 0..4).prop_map(TypedArray::I16),
    prop::collection::vec(any::<u16>(), 0..4).prop_map(TypedArray::U16),
    prop::collection::vec(any::<i32>(), 0..4).prop_map(TypedArray::I32),
    prop::collection::vec(any::<u32>(), 0..4).prop_map(TypedArray::U32),
    prop::collection::vec(any::<i64>(), 0..4).prop_map(TypedArray::I64),
    prop::collection::vec(any::<u64>(), 0..4).prop_map(TypedArray::U64),
    prop::collection::vec(any::<f32>().prop_filter("finite", |f| f.is_finite()), 0..4).prop_map(TypedArray::F32),
    prop::collection::vec(any::<f64>().prop_filter("finite", |f| f.is_finite()), 0..4).prop_map(TypedArray::F64),
  ]
  .boxed()
}

fn value() -> BoxedStrategy<Value> {
  let leaf = prop_oneof![
    Just(Value::Null),
    any::<bool>().prop_map(Value::Bool),
    any::<i128>().prop_map(Value::Int),
    (i128::MAX as u128 + 1..=u128::MAX).prop_map(Value::UInt),
    any::<f32>().prop_filter("finite", |f| f.is_finite()).prop_map(Value::F32),
    any::<f64>().prop_filter("finite", |f| f.is_finite()).prop_map(Value::F64),
    text().prop_map(Value::str),
    prop::collection::vec(any::<u8>(), 0..8).prop_map(Value::Bytes),
    typed_array().prop_map(Value::TypedArray),
    (text(), prop::option::of(text())).prop_map(|(tag, payload)| Value::Variant {
      tag,
      payload: payload.map(|p| Box::new(Value::str(p))),
    }),
  ];
  leaf
    .prop_recursive(4, 32, 4, |inner| {
      prop_oneof![
        prop::collection::vec(inner.clone(), 0..4).prop_map(|v| Value::Seq(ValueSeq::from(v))),
        prop::collection::vec((text(), inner), 0..4).prop_map(|pairs| {
          let mut map = ValueMap::default();
          for (k, v) in pairs {
            map.insert(k, v);
          }
          Value::Map(map)
        }),
      ]
    })
    .boxed()
}

fn node() -> BoxedStrategy<Node> {
  let leaf = prop_oneof![
    text().prop_map(Node::text),
    text().prop_map(|h| Node::Raw(Html(h))),
  ];
  leaf
    .prop_recursive(3, 20, 3, |inner| {
      prop_oneof![
        prop::collection::vec(inner.clone(), 0..3).prop_map(Node::Seq),
        (prop::collection::vec((text(), value()), 0..3), prop::collection::vec(inner.clone(), 0..2), prop::option::of(inner.clone()))
          .prop_map(|(props, children, ssr)| {
            let mut map = ValueMap::default();
            for (k, v) in props {
              map.insert(k, v);
            }
            Node::Client {
              module: ModuleId::new("m.tsx", "default"),
              props: map,
              children,
              ssr: ssr.map(Box::new),
            }
          }),
        (any::<u32>(), inner).prop_map(|(slot, fallback)| Node::Pending {
          slot: SlotId(slot),
          fallback: Box::new(fallback),
        }),
      ]
    })
    .boxed()
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// The tagged encoding is lossless: this is what the browser and the server
  /// each rebuild the other's values from.
  #[test]
  fn a_value_survives_the_json(v in value()) {
    let json = value_to_json(&v);
    let back = json_to_value(&json).map_err(|e| TestCaseError::fail(format!("{e}\n{json}")))?;
    prop_assert_eq!(back, v);
  }

  /// The same through the text, which is what actually crosses the wire.
  #[test]
  fn a_value_survives_the_text(v in value()) {
    let text = value_to_json(&v).to_string();
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(json_to_value(&json).map_err(|e| TestCaseError::fail(e.to_string()))?, v);
  }

  /// A node tree survives the row encoding the streaming client reads.
  #[test]
  fn a_node_survives_the_row(n in node()) {
    let row = node_to_row_json(&n);
    let back = row_json_to_node(&row).map_err(|e| TestCaseError::fail(format!("{e}\n{row}")))?;
    prop_assert_eq!(back, n);
  }

  /// Whatever a client sends, a decoder answers or refuses. It may not panic:
  /// this input is a request body.
  #[test]
  fn decoding_arbitrary_json_never_panics(text in "(?s).{0,200}") {
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
      let _ = json_to_value(&json);
      let _ = row_json_to_node(&json);
    }
  }

  /// The same over JSON that is well formed but shaped like the protocol,
  /// which is where a decoder actually indexes and unwraps.
  #[test]
  fn decoding_protocol_shaped_json_never_panics(
    tag in prop_oneof![Just("t"), Just("r"), Just("q"), Just("c"), Just("p"), Just("int"), Just("bytes"), Just("map"), Just("seq"), Just("nope")],
    body in prop_oneof![
      Just(json!(null)), Just(json!(0)), Just(json!("x")), Just(json!([])),
      Just(json!({})), Just(json!([1, 2, 3])), Just(json!({"m": 1})),
    ],
  ) {
    for candidate in [json!([tag, body]), json!({tag: body}), json!([tag]), json!([tag, body, body])] {
      let _ = json_to_value(&candidate);
      let _ = row_json_to_node(&candidate);
    }
  }

  /// Text a page carries is escaped: no value a loader returns may close a tag
  /// or open one. `Node::Raw` is the deliberate exception and is not generated
  /// into text positions here.
  #[test]
  fn rendered_text_cannot_escape_its_element(parts in prop::collection::vec(text(), 1..4)) {
    let tree = Node::Seq(parts.iter().map(|p| Node::text(p.clone())).collect());
    let html = html_serialize(&tree);
    prop_assert!(!html.contains('<'), "unescaped `<` in {html:?}");
    prop_assert!(!html.contains('>'), "unescaped `>` in {html:?}");
  }

  /// A page serializes to rows that read back as the same tree.
  #[test]
  fn a_page_survives_serialize(n in node()) {
    let text = serialize_page(&n);
    let row = text.lines().find_map(|l| l.strip_prefix("N ")).ok_or_else(|| TestCaseError::fail("no N row"))?;
    let json: serde_json::Value = serde_json::from_str(row).map_err(|e| TestCaseError::fail(e.to_string()))?;
    prop_assert_eq!(row_json_to_node(&json).map_err(|e| TestCaseError::fail(e.to_string()))?, n);
  }
}

/// serde_json's fast float parser is not exact: it wrote this value correctly
/// and read it back one ulp away, so a price a loader returned arrived at the
/// browser as a different number. `float_roundtrip` is the feature that fixes
/// it and this holds the crate to it.
#[test]
fn a_float_crosses_the_wire_unchanged() {
  for v in [
    -1.163_001_882_891_140_9e30,
    2.088_002_797_351_528_5e-173,
    0.1 + 0.2,
    f64::MIN_POSITIVE,
    f64::MAX,
    1e308,
    5e-324,
  ] {
    let text = value_to_json(&Value::F64(v)).to_string();
    let json: serde_json::Value = serde_json::from_str(&text).expect("reparses");
    match json_to_value(&json).expect("decodes") {
      Value::F64(back) => assert_eq!(back.to_bits(), v.to_bits(), "{v:?} came back as {back:?} via {text}"),
      other => panic!("{other:?}"),
    }
  }
}

/// A payload is a request body, so its nesting is the client's choice. serde_json
/// caps its own recursion; this holds the rest of the path to answering rather
/// than overflowing, which aborts.
#[test]
fn a_deeply_nested_payload_is_refused_rather_than_fatal() {
  for depth in [64usize, 200, 5_000, 100_000] {
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));
    match serde_json::from_str::<serde_json::Value>(&text) {
      Err(_) => {}
      Ok(json) => {
        let _ = json_to_value(&json);
        let _ = row_json_to_node(&json);
      }
    }
    let object = format!("{}{}", r#"{"a":"#.repeat(depth), format!("1{}", "}".repeat(depth)));
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&object) {
      let _ = json_to_value(&json);
    }
  }
}
