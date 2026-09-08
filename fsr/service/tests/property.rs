//! The contract check under generated values. An action's input is a request
//! body, so this is the gate: a value it wrongly accepts reaches a body that
//! was written against the declared type.

use proptest::prelude::*;
use snapfire_fsr_core::{Value, ValueMap, ValueSeq};
use snapfire_fsr_service::{Contract, Type};

fn contract() -> Contract {
  Contract::default()
}

fn check(ty: &Type, v: &Value) -> bool {
  contract().check_value(ty, v, "$").is_ok()
}

/// The integer widths, at their edges. One off here lets a value through that
/// the body will treat as the narrower type.
#[test]
fn an_integer_width_is_exact_at_its_edges() {
  let cases: &[(Type, i128, i128)] = &[
    (Type::I32, i32::MIN as i128, i32::MAX as i128),
    (Type::I64, i64::MIN as i128, i64::MAX as i128),
    (Type::U32, 0, u32::MAX as i128),
    (Type::U64, 0, u64::MAX as i128),
  ];
  for (ty, low, high) in cases {
    assert!(check(ty, &Value::Int(*low)), "{ty:?} refused its lowest {low}");
    assert!(check(ty, &Value::Int(*high)), "{ty:?} refused its highest {high}");
    assert!(!check(ty, &Value::Int(low - 1)), "{ty:?} took {} which is below it", low - 1);
    assert!(!check(ty, &Value::Int(high + 1)), "{ty:?} took {} which is above it", high + 1);
  }
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// An integer is accepted exactly when it is in range, whatever the width.
  #[test]
  fn an_integer_is_accepted_exactly_in_range(n in any::<i128>()) {
    prop_assert_eq!(check(&Type::I32, &Value::Int(n)), i32::try_from(n).is_ok(), "i32 {}", n);
    prop_assert_eq!(check(&Type::I64, &Value::Int(n)), i64::try_from(n).is_ok(), "i64 {}", n);
    prop_assert_eq!(check(&Type::U32, &Value::Int(n)), u32::try_from(n).is_ok(), "u32 {}", n);
    prop_assert_eq!(check(&Type::U64, &Value::Int(n)), u64::try_from(n).is_ok(), "u64 {}", n);
    prop_assert!(check(&Type::I128, &Value::Int(n)), "i128 {}", n);
  }

  /// A scalar type takes its own kind and nothing else, so nothing is coerced
  /// silently across the gate.
  #[test]
  fn a_scalar_type_takes_only_its_own_kind(n in any::<i64>(), f in any::<f64>(), s in "[a-z]{0,6}") {
    let values = [
      Value::Null,
      Value::Bool(true),
      Value::Int(n as i128),
      Value::F64(f),
      Value::F32(f as f32),
      Value::str(s),
      Value::Bytes(vec![1]),
    ];
    for v in &values {
      prop_assert_eq!(check(&Type::Null, v), matches!(v, Value::Null));
      prop_assert_eq!(check(&Type::Bool, v), matches!(v, Value::Bool(_)));
      prop_assert_eq!(check(&Type::Str, v), matches!(v, Value::Str(_)));
      prop_assert_eq!(check(&Type::Bytes, v), matches!(v, Value::Bytes(_)));
      prop_assert_eq!(check(&Type::F64, v), matches!(v, Value::F64(_)));
      prop_assert_eq!(check(&Type::F32, v), matches!(v, Value::F32(_)));
    }
  }

  /// An optional takes null or the inner type, never anything else.
  #[test]
  fn an_optional_takes_null_or_its_inner(n in any::<i32>(), s in "[a-z]{0,4}") {
    let ty = Type::optional(Type::I32);
    prop_assert!(check(&ty, &Value::Null));
    prop_assert!(check(&ty, &Value::Int(n as i128)));
    prop_assert!(!check(&ty, &Value::str(s)));
  }

  /// A list checks every item, so one bad element anywhere refuses the whole.
  #[test]
  fn a_list_refuses_one_bad_item(
    good in prop::collection::vec(any::<i32>(), 0..6),
    at in 0usize..8,
  ) {
    let ty = Type::List(Box::new(Type::I32));
    let items: Vec<Value> = good.iter().map(|n| Value::Int(*n as i128)).collect();
    prop_assert!(check(&ty, &Value::Seq(ValueSeq::from(items.clone()))));
    if !items.is_empty() {
      let mut bad = items.clone();
      let at = at % bad.len();
      bad[at] = Value::str("no");
      prop_assert!(!check(&ty, &Value::Seq(ValueSeq::from(bad))));
    }
  }

  /// Checking any value against any type answers; it does not panic.
  #[test]
  fn checking_never_panics(n in any::<i128>(), s in "(?s).{0,8}") {
    let mut map = ValueMap::default();
    map.insert(s.clone(), Value::Int(n));
    let values = [
      Value::Null,
      Value::Int(n),
      Value::UInt(u128::MAX),
      Value::F64(f64::NAN),
      Value::str(s.clone()),
      Value::Seq(ValueSeq::from(vec![Value::Int(n)])),
      Value::Map(map),
    ];
    let types = [
      Type::Null, Type::Bool, Type::I32, Type::I64, Type::I128, Type::U32, Type::U64,
      Type::U128, Type::F32, Type::F64, Type::Str, Type::Bytes,
      Type::optional(Type::I32),
      Type::List(Box::new(Type::Str)),
      Type::Map(Box::new(Type::I32)),
      Type::Named("missing".to_owned()),
    ];
    for ty in &types {
      for v in &values {
        let _ = contract().check_value(ty, v, "$");
      }
    }
  }
}
