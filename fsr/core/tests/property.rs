//! The value model under generated input: the copy-on-write map that every
//! render clones, and the parsers a configuration goes through.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use snapfire_fsr_core::{parse_duration, Value, ValueMap, ValueSeq};

fn text() -> BoxedStrategy<String> {
  prop_oneof![
    3 => "[a-zA-Z0-9 _-]{0,10}",
    1 => "(?s).{0,8}",
    1 => prop_oneof![Just(String::new()), Just("é🌍".to_owned()), Just("\u{0}".to_owned())],
  ]
  .boxed()
}

fn value() -> BoxedStrategy<Value> {
  let leaf = prop_oneof![
    Just(Value::Null),
    any::<bool>().prop_map(Value::Bool),
    any::<i128>().prop_map(Value::Int),
    text().prop_map(Value::str),
  ];
  leaf
    .prop_recursive(3, 24, 3, |inner| {
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

fn map_of(pairs: Vec<(String, Value)>) -> ValueMap {
  let mut map = ValueMap::default();
  for (k, v) in pairs {
    map.insert(k, v);
  }
  map
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// A clone shares until it is written. Writing to either side must leave the
  /// other exactly as it was: a render clones a props map at every component
  /// and every loop iteration, so a leak here is one request's data appearing
  /// in another's markup.
  #[test]
  fn writing_a_clone_leaves_the_original(
    pairs in prop::collection::vec((text(), value()), 0..6),
    key in text(),
    v in value(),
  ) {
    let original = map_of(pairs);
    let before = original.clone();
    let mut copy = original.clone();
    copy.insert(key.clone(), v);
    prop_assert_eq!(&original, &before, "the original moved when its clone was written");

    let mut second = original.clone();
    second.shift_remove(&key);
    prop_assert_eq!(&original, &before, "the original moved when a clone removed a key");
  }

  /// And the other way: writing the original leaves a clone taken earlier.
  #[test]
  fn writing_an_original_leaves_its_clone(
    pairs in prop::collection::vec((text(), value()), 0..6),
    key in text(),
    v in value(),
  ) {
    let mut original = map_of(pairs);
    let snapshot = original.clone();
    let before = snapshot.clone();
    original.insert(key, v);
    prop_assert_eq!(&snapshot, &before, "a clone moved when the original was written");
  }

  /// Nesting is where sharing is easiest to get wrong: a map inside a map
  /// inside a sequence must copy the whole path on a write.
  #[test]
  fn writing_a_nested_clone_leaves_the_original(
    inner in prop::collection::vec((text(), value()), 0..4),
    key in text(),
    v in value(),
  ) {
    let mut original = ValueMap::default();
    original.insert("nested".to_owned(), Value::Map(map_of(inner)));
    let before = original.clone();

    let mut copy = original.clone();
    if let Some(Value::Map(map)) = copy.get_mut("nested") {
      map.insert(key, v);
    }
    prop_assert_eq!(&original, &before, "a nested write reached the original");
  }

  /// Every value equals itself through a clone, whatever it holds.
  #[test]
  fn a_value_equals_its_clone(v in value()) {
    prop_assert_eq!(v.clone(), v);
  }

  /// A duration is answered or refused, never a panic and never a wrap. A
  /// configuration is a file someone typed.
  #[test]
  fn parsing_a_duration_never_panics(raw in "(?s).{0,20}") {
    let _ = parse_duration(&raw);
  }

  #[test]
  fn parsing_a_duration_shaped_string_never_panics(
    n in "[0-9]{0,25}",
    unit in prop_oneof![Just(""), Just("s"), Just("m"), Just("h"), Just("d"), Just("x"), Just(" "), Just("ms")],
    pad in prop_oneof![Just(""), Just(" "), Just("\t")],
  ) {
    let _ = parse_duration(&format!("{pad}{n}{unit}{pad}"));
  }

  /// The units are what they say they are, wherever the arithmetic does not
  /// leave the range.
  #[test]
  fn a_duration_means_its_unit(n in 0u64..100_000_000) {
    prop_assert_eq!(parse_duration(&n.to_string()).map(|d| d.as_secs()), Some(n));
    prop_assert_eq!(parse_duration(&format!("{n}s")).map(|d| d.as_secs()), Some(n));
    prop_assert_eq!(parse_duration(&format!("{n}m")).map(|d| d.as_secs()), Some(n * 60));
    prop_assert_eq!(parse_duration(&format!("{n}h")).map(|d| d.as_secs()), Some(n * 3600));
    prop_assert_eq!(parse_duration(&format!("{n}d")).map(|d| d.as_secs()), Some(n * 86_400));
  }
}

/// A span too large for the range is refused rather than wrapping. In a debug
/// build the multiply panicked; in a release build `9999999999999999d` became
/// a few hours.
#[test]
fn a_duration_beyond_the_range_is_refused() {
  for raw in ["9999999999999999d", "999999999999999999h", "18446744073709551615d", "307445734561825861m"] {
    assert_eq!(parse_duration(raw), None, "{raw}");
  }
  assert_eq!(parse_duration("18446744073709551615").map(|d| d.as_secs()), Some(u64::MAX));
  assert_eq!(parse_duration("18446744073709551616"), None, "beyond u64");
}
