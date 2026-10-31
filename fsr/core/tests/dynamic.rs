use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use snapfire_fsr_core::{Dynamic, Value};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inner {
  low_edge: f64,
  upper: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
enum Choice {
  Plain,
  One(u32),
  Pair(u32, String),
  Named { a: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
enum Either {
  Number(u64),
  Text(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct Whole {
  text: String,
  flag: bool,
  small: i8,
  big: u64,
  negative: i64,
  wide: u128,
  ratio: f64,
  missing: Option<String>,
  present: Option<Inner>,
  list: Vec<u32>,
  pair: (u32, Option<f64>),
  map: BTreeMap<String, String>,
  choices: Vec<Choice>,
  either: Vec<Either>,
  unit: (),
}

fn whole() -> Whole {
  Whole {
    text: "t".to_string(),
    flag: true,
    small: -3,
    big: u64::MAX,
    negative: i64::MIN,
    wide: u128::MAX,
    ratio: 1.5,
    missing: None,
    present: Some(Inner { low_edge: 0.5, upper: None }),
    list: vec![1, 2],
    pair: (7, None),
    map: BTreeMap::from([("k".to_string(), "v".to_string())]),
    choices: vec![Choice::Plain, Choice::One(1), Choice::Pair(2, "x".to_string()), Choice::Named { a: true }],
    either: vec![Either::Number(5), Either::Text("five".to_string())],
    unit: (),
  }
}

#[test]
fn a_serde_type_round_trips_without_going_through_json() {
  let original = whole();
  let dynamic = Dynamic::from_serde(&original).unwrap();
  assert_eq!(dynamic.to_serde::<Whole>().unwrap(), original);
}

#[test]
fn field_names_and_shapes_are_the_ones_serde_asks_for() {
  let Value::Map(fields) = Dynamic::from_serde(&Inner { low_edge: 1.0, upper: None }).unwrap().0 else { panic!("a map") };
  assert_eq!(fields.get("lowEdge"), Some(&Value::F64(1.0)));
  assert_eq!(fields.get("upper"), Some(&Value::Null));
  assert_eq!(Dynamic::from_serde(&Choice::Plain).unwrap().0, Value::str("Plain"));
  let Value::Map(one) = Dynamic::from_serde(&Choice::One(1)).unwrap().0 else { panic!("a map") };
  assert_eq!(one.get("One"), Some(&Value::Int(1)));
}

#[test]
fn integers_keep_their_full_range() {
  assert_eq!(Dynamic::from_serde(&u64::MAX).unwrap().0, Value::Int(u64::MAX as i128));
  assert_eq!(Dynamic(Value::Int(u64::MAX as i128)).to_serde::<u64>().unwrap(), u64::MAX);
  assert_eq!(Dynamic::from_serde(&u128::MAX).unwrap().to_serde::<u128>().unwrap(), u128::MAX);
  assert_eq!(Dynamic::from_serde(&i128::MIN).unwrap().to_serde::<i128>().unwrap(), i128::MIN);
}

#[test]
fn a_whole_number_a_script_wrote_as_a_float_reads_as_an_integer() {
  assert_eq!(Dynamic(Value::F64(5.0)).to_serde::<u64>().unwrap(), 5);
  assert_eq!(Dynamic(Value::F64(-5.0)).to_serde::<i32>().unwrap(), -5);
  assert_eq!(Dynamic(Value::F64(5.0)).to_serde::<Either>().unwrap(), Either::Number(5));
  assert_eq!(Dynamic(Value::F64(5.0)).to_serde::<f64>().unwrap(), 5.0);
  assert_eq!(Dynamic(Value::Int(5)).to_serde::<f64>().unwrap(), 5.0);
  assert!(Dynamic(Value::F64(5.5)).to_serde::<u64>().is_err());
  assert_eq!(Dynamic(Value::F64(5.5)).to_serde::<f64>().unwrap(), 5.5);
}

#[test]
fn undefined_reads_as_absent() {
  #[derive(Debug, PartialEq, Deserialize)]
  struct Partial {
    #[serde(default)]
    gone: Option<String>,
    #[serde(default)]
    defaulted: u32,
    kept: u32,
  }
  let fields = [("gone".to_string(), Value::Undefined), ("defaulted".to_string(), Value::Undefined), ("kept".to_string(), Value::Int(1))];
  let read: Partial = Dynamic(Value::Map(fields.into_iter().collect())).to_serde().unwrap();
  assert_eq!(read, Partial { gone: None, defaulted: 0, kept: 1 });
  assert_eq!(Dynamic(Value::Undefined).to_serde::<Option<u32>>().unwrap(), None);
  assert_eq!(Dynamic(Value::Null).to_serde::<Option<u32>>().unwrap(), None);
}

#[test]
fn flatten_works_both_ways() {
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  struct Line {
    msg: String,
    #[serde(flatten)]
    fields: BTreeMap<String, Either>,
  }
  let line = Line { msg: "m".to_string(), fields: BTreeMap::from([("code".to_string(), Either::Number(500)), ("host".to_string(), Either::Text("h".to_string()))]) };
  let dynamic = Dynamic::from_serde(&line).unwrap();
  let Value::Map(fields) = &dynamic.0 else { panic!("a map") };
  assert_eq!(fields.get("code"), Some(&Value::Int(500)));
  assert_eq!(dynamic.to_serde::<Line>().unwrap(), line);
}

#[test]
fn bytes_stay_bytes() {
  struct Raw;
  impl Serialize for Raw {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
      serializer.serialize_bytes(b"ab")
    }
  }
  assert_eq!(Dynamic::from_serde(&Raw).unwrap().0, Value::Bytes(b"ab".to_vec()));
}

#[test]
fn a_map_key_that_is_not_text_writes_as_its_digits_and_reads_back() {
  let map = BTreeMap::from([(7u32, "v".to_string())]);
  let dynamic = Dynamic::from_serde(&map).unwrap();
  let Value::Map(fields) = &dynamic.0 else { panic!("a map") };
  assert_eq!(fields.get("7"), Some(&Value::str("v")));
  assert!(Dynamic::from_serde(&BTreeMap::from([(vec![1u32], 1u32)])).is_err());
}

#[test]
fn a_wrong_shape_is_an_error_naming_what_was_wrong() {
  let error = Dynamic(Value::str("text")).to_serde::<u64>().unwrap_err().to_string();
  assert!(error.contains("text") || error.contains("string"), "{error}");
  assert!(Dynamic(Value::Int(1)).to_serde::<Inner>().is_err());
  assert!(Dynamic(Value::str("Unknown")).to_serde::<Choice>().is_err());
  let two = [("One".to_string(), Value::Int(1)), ("Plain".to_string(), Value::Null)];
  assert!(Dynamic(Value::Map(two.into_iter().collect())).to_serde::<Choice>().is_err());
}

#[test]
fn plain_json_converts_at_the_edge() {
  let json = serde_json::json!({ "a": [1, 2.5, "x", null, true], "b": { "n": -3 } });
  let dynamic = Dynamic::from_json(&json);
  assert_eq!(dynamic.to_json().unwrap(), json);
}
