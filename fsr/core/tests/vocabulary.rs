use indexmap::IndexMap;
use snapfire_fsr_core::{Fingerprint, ModuleId, TypedArray, Value};

#[test]
fn module_id_round_trips() {
  let id: ModuleId = "components/ServerChart.tsx#default".parse().unwrap();
  assert_eq!(id, ModuleId::new("components/ServerChart.tsx", "default"));
  assert_eq!(id.to_string(), "components/ServerChart.tsx#default");
}

#[test]
fn module_id_rejects_missing_export() {
  assert!("components/ServerChart.tsx".parse::<ModuleId>().is_err());
  assert!("#default".parse::<ModuleId>().is_err());
  assert!("components/x.tsx#".parse::<ModuleId>().is_err());
}

#[test]
fn every_nan_fingerprints_identically() {
  let quiet = Value::F64(f64::NAN);
  let payload = Value::F64(f64::from_bits(0x7ff8_0000_0000_0001));
  let negative = Value::F64(f64::from_bits(0xfff8_0000_0000_0000));
  assert_eq!(quiet.fingerprint(), payload.fingerprint());
  assert_eq!(quiet.fingerprint(), negative.fingerprint());
  assert_ne!(quiet.fingerprint(), Value::F64(0.0).fingerprint());
}

#[test]
fn nan_inside_typed_array_is_canonical() {
  let a = Value::TypedArray(TypedArray::F64(vec![1.0, f64::NAN]));
  let b = Value::TypedArray(TypedArray::F64(vec![1.0, f64::from_bits(0x7ff8_0000_0000_0001)]));
  assert_eq!(a.fingerprint(), b.fingerprint());
}

#[test]
fn map_fingerprint_ignores_insertion_order() {
  let mut ab = IndexMap::new();
  ab.insert("a".to_owned(), Value::int(1));
  ab.insert("b".to_owned(), Value::int(2));
  let mut ba = IndexMap::new();
  ba.insert("b".to_owned(), Value::int(2));
  ba.insert("a".to_owned(), Value::int(1));

  let ab_order: Vec<&String> = ab.keys().collect();
  let ba_order: Vec<&String> = ba.keys().collect();
  assert_ne!(ab_order, ba_order, "insertion order is preserved for serialization");

  assert_eq!(Value::Map(ab.clone()), Value::Map(ba.clone()), "equality ignores order, agreeing with the fingerprint");
  assert_eq!(Value::Map(ab).fingerprint(), Value::Map(ba).fingerprint());
}

#[test]
fn uint_normalizes_into_int_range() {
  assert_eq!(Value::uint(42), Value::Int(42));
  assert_eq!(Value::uint(42).fingerprint(), Value::int(42i64).fingerprint());

  let wide = Value::uint(u128::MAX);
  assert_eq!(wide, Value::UInt(u128::MAX));
  assert_ne!(wide.fingerprint(), Value::Int(i128::MAX).fingerprint());
}

#[test]
fn unnormalized_uint_still_matches_int_fingerprint() {
  assert_eq!(Value::UInt(42).fingerprint(), Value::Int(42).fingerprint());
}

#[test]
fn typed_array_is_not_a_seq_of_scalars() {
  let arr = Value::TypedArray(TypedArray::F64(vec![1.0, 2.0]));
  let seq = Value::Seq(vec![Value::F64(1.0), Value::F64(2.0)]);
  assert_ne!(arr.fingerprint(), seq.fingerprint());
}

#[test]
fn element_kind_distinguishes_typed_arrays() {
  let f32s = Value::TypedArray(TypedArray::F32(vec![1.0]));
  let f64s = Value::TypedArray(TypedArray::F64(vec![1.0]));
  assert_ne!(f32s.fingerprint(), f64s.fingerprint());
}

#[test]
fn strings_and_bytes_do_not_collide() {
  let s = Value::str("ab");
  let b = Value::Bytes(b"ab".to_vec());
  assert_ne!(s.fingerprint(), b.fingerprint());
}

#[test]
fn variant_payload_shapes_are_distinct() {
  let unit = Value::Variant { tag: "Down".into(), payload: None };
  let with_null = Value::Variant { tag: "Down".into(), payload: Some(Box::new(Value::Null)) };
  assert_ne!(unit.fingerprint(), with_null.fingerprint());
}

#[test]
fn ref_kinds_are_distinct() {
  let action = Value::Ref { kind: snapfire_fsr_core::RefKind::Action, id: "save".into() };
  let module = Value::Ref { kind: snapfire_fsr_core::RefKind::Module, id: "save".into() };
  assert_ne!(action.fingerprint(), module.fingerprint());
}

#[test]
fn seq_length_prefix_prevents_boundary_shifts() {
  let a = Value::Seq(vec![Value::str("ab"), Value::str("c")]);
  let b = Value::Seq(vec![Value::str("a"), Value::str("bc")]);
  assert_ne!(a.fingerprint(), b.fingerprint());
}
