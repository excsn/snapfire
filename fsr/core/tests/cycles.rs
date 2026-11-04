//! `collect_cycles` frees a cycle nothing outside it holds and leaves every object something outside still reaches as it was.

use snapfire_fsr_core::{collect_cycles, Value, ValueMap};

fn object(fields: &[(&str, Value)]) -> Value {
  let mut map = ValueMap::default();
  for (k, v) in fields {
    map.insert(*k, v.clone());
  }
  Value::Map(map)
}

fn link(from: &Value, key: &str, to: &Value) {
  let Value::Map(map) = from else { panic!() };
  map.through().unwrap().insert(key, to.clone());
}

fn holders(value: &Value) -> usize {
  match value {
    Value::Map(map) => map.holders(),
    Value::Seq(items) => items.holders(),
    _ => panic!(),
  }
}

#[test]
fn a_cycle_nothing_else_holds_is_freed() {
  let c = object(&[("n", Value::int(1))]);
  let a = object(&[("c", c.clone())]);
  link(&a, "self", &a);
  collect_cycles(vec![a]);
  assert_eq!(holders(&c), 1);
}

#[test]
fn a_cycle_held_from_outside_keeps_every_field() {
  let c = object(&[("n", Value::int(1))]);
  let a = object(&[("c", c.clone())]);
  link(&a, "self", &a);
  let kept = a.clone();
  collect_cycles(vec![a]);
  let Value::Map(map) = &kept else { panic!() };
  assert!(map.get("self").is_some() && map.get("c").is_some());
  assert_eq!(holders(&c), 2);
}

#[test]
fn a_cycle_a_live_object_reaches_keeps_every_field() {
  let c = object(&[("n", Value::int(1))]);
  let a = object(&[("c", c.clone())]);
  let b = object(&[("a", a.clone())]);
  link(&a, "b", &b);
  let root = object(&[("a", a.clone())]);
  collect_cycles(vec![a, b]);
  let Value::Map(map) = &root else { panic!() };
  let Some(Value::Map(a)) = map.get("a") else { panic!() };
  assert!(a.get("b").is_some() && a.get("c").is_some());
  assert_eq!(holders(&c), 2);
}

#[test]
fn a_frozen_value_is_never_touched() {
  let frozen = object(&[("n", Value::int(1))]).frozen();
  let a = object(&[("f", frozen.clone())]);
  link(&a, "self", &a);
  collect_cycles(vec![a]);
  let Value::Map(map) = &frozen else { panic!() };
  assert_eq!(map.get("n"), Some(&Value::int(1)));
}
