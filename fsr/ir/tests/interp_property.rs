//! The interpreter over hostile values. Everything it evaluates came from a
//! loader, a service or a request, so an extreme number must be an error or an
//! answer rather than a panic in the middle of a render.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Component, Expr, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Interpreter;

/// Evaluates `expr` with `$props` bound, through the only public path.
fn eval(expr: Expr, props: ValueMap) -> Result<String, String> {
  let component = Component::new(Vec::new(), Tmpl::Expr(expr));
  Interpreter::default()
    .render(&component, &props, &Components::new())
    .map(|r| r.html)
    .map_err(|e| e.to_string())
}

fn prop(name: &str) -> Expr {
  Expr::Field(Box::new(Expr::var("$props")), name.to_owned())
}

/// The numbers that break arithmetic, plus the ordinary ones.
fn value() -> BoxedStrategy<Value> {
  prop_oneof![
    2 => any::<i128>().prop_map(Value::Int),
    2 => any::<f64>().prop_map(Value::F64),
    1 => "[a-z ]{0,6}".prop_map(Value::str),
    2 => prop_oneof![
      Just(Value::Int(i128::MAX)),
      Just(Value::Int(i128::MIN)),
      Just(Value::Int(0)),
      Just(Value::Int(-1)),
      Just(Value::F64(f64::NAN)),
      Just(Value::F64(f64::INFINITY)),
      Just(Value::F64(f64::NEG_INFINITY)),
      Just(Value::F64(0.0)),
      Just(Value::F64(-0.0)),
      Just(Value::F64(f64::MAX)),
      Just(Value::F64(f64::MIN_POSITIVE)),
      Just(Value::Null),
      Just(Value::Bool(true)),
      Just(Value::str("")),
    ],
  ]
  .boxed()
}

fn pair(a: Value, b: Value) -> ValueMap {
  let mut map = ValueMap::default();
  map.insert("a".to_owned(), a);
  map.insert("b".to_owned(), b);
  map
}

fn arith_ops() -> BoxedStrategy<ArithOp> {
  prop_oneof![Just(ArithOp::Add), Just(ArithOp::Sub), Just(ArithOp::Mul), Just(ArithOp::Div), Just(ArithOp::Rem)].boxed()
}

fn compare_ops() -> BoxedStrategy<CompareOp> {
  prop_oneof![
    Just(CompareOp::Eq), Just(CompareOp::Ne), Just(CompareOp::Lt),
    Just(CompareOp::Le), Just(CompareOp::Gt), Just(CompareOp::Ge)
  ]
  .boxed()
}

fn builtins() -> BoxedStrategy<Builtin> {
  prop_oneof![
    Just(Builtin::Round), Just(Builtin::Floor), Just(Builtin::Ceil), Just(Builtin::Abs),
    Just(Builtin::Min), Just(Builtin::Max), Just(Builtin::ToFixed), Just(Builtin::Repeat),
    Just(Builtin::Join), Just(Builtin::Trim), Just(Builtin::Upper), Just(Builtin::Lower),
    Just(Builtin::Includes), Just(Builtin::StartsWith), Just(Builtin::EndsWith),
    Just(Builtin::Split), Just(Builtin::Replace),
    Just(Builtin::EncodeUriComponent), Just(Builtin::LocaleNumber),
    Just(Builtin::Range), Just(Builtin::Omit)
  ]
  .boxed()
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// Arithmetic over any two values answers or fails; it does not panic, and
  /// an integer that leaves the range is a failure rather than a wrap.
  #[test]
  fn arithmetic_over_any_values_never_panics(op in arith_ops(), a in value(), b in value()) {
    let _ = eval(Expr::Arith(op, Box::new(prop("a")), Box::new(prop("b"))), pair(a, b));
  }

  #[test]
  fn comparison_over_any_values_never_panics(op in compare_ops(), a in value(), b in value()) {
    let _ = eval(Expr::Compare(op, Box::new(prop("a")), Box::new(prop("b"))), pair(a, b));
  }

  /// Every builtin with one and two arguments of any shape.
  #[test]
  fn a_builtin_over_any_values_never_panics(name in builtins(), a in value(), b in value()) {
    let _ = eval(Expr::Builtin { name, args: vec![prop("a")] }, pair(a.clone(), b.clone()));
    let _ = eval(Expr::Builtin { name, args: vec![prop("a"), prop("b")] }, pair(a.clone(), b.clone()));
    let _ = eval(Expr::Builtin { name, args: Vec::new() }, pair(a, b));
  }

  /// The conversions, which is where a number becomes text and back.
  #[test]
  fn a_conversion_over_any_value_never_panics(v in value()) {
    for build in [
      Expr::Str as fn(Box<Expr>) -> Expr,
      Expr::Num,
      Expr::BigInt,
      Expr::Length,
      Expr::Keys,
      Expr::Values,
      Expr::Entries,
      Expr::Not,
    ] {
      let mut map = ValueMap::default();
      map.insert("a".to_owned(), v.clone());
      let _ = eval(build(Box::new(prop("a"))), map);
    }
  }

  /// An integer that leaves the range fails rather than wrapping: a price
  /// multiplied by a quantity must not come out negative.
  #[test]
  fn integer_arithmetic_does_not_wrap(a in any::<i128>(), b in any::<i128>()) {
    let out = eval(Expr::Arith(ArithOp::Mul, Box::new(prop("a")), Box::new(prop("b"))), pair(Value::Int(a), Value::Int(b)));
    match a.checked_mul(b) {
      Some(want) => prop_assert_eq!(out, Ok(want.to_string())),
      None => prop_assert!(out.is_err(), "{a} * {b} gave {:?}", out),
    }
  }
}

/// The three builtins that took a size from data. Each used to end the process
/// rather than the request: `toFixed` panicked in the formatter, `repeat` and
/// `range` asked for an allocation that aborts.
#[test]
fn a_size_from_data_is_refused_rather_than_fatal() {
  let big = |v: Value| {
    let mut map = ValueMap::default();
    map.insert("a".to_owned(), Value::F64(1.0));
    map.insert("b".to_owned(), v);
    map
  };
  for v in [Value::F64(1e18), Value::F64(f64::INFINITY), Value::Int(i128::MAX), Value::F64(1e9)] {
    let out = eval(Expr::Builtin { name: Builtin::ToFixed, args: vec![prop("a"), prop("b")] }, big(v.clone()));
    assert!(out.is_err(), "toFixed accepted {v:?}: {out:?}");
    let out = eval(Expr::Builtin { name: Builtin::Range, args: vec![prop("b")] }, big(v.clone()));
    assert!(out.is_err(), "range accepted {v:?}: {out:?}");
  }
  let mut map = ValueMap::default();
  map.insert("a".to_owned(), Value::str("xy"));
  map.insert("b".to_owned(), Value::F64(1e18));
  let out = eval(Expr::Builtin { name: Builtin::Repeat, args: vec![prop("a"), prop("b")] }, map);
  assert!(out.is_err(), "repeat accepted 1e18: {out:?}");
}

/// What still works, so the bounds did not take the builtins with them.
#[test]
fn ordinary_sizes_still_work() {
  let mut map = ValueMap::default();
  map.insert("a".to_owned(), Value::F64(1.5));
  map.insert("b".to_owned(), Value::F64(2.0));
  assert_eq!(eval(Expr::Builtin { name: Builtin::ToFixed, args: vec![prop("a"), prop("b")] }, map.clone()), Ok("1.50".to_owned()));
  assert_eq!(eval(Expr::Builtin { name: Builtin::Range, args: vec![prop("b")] }, map.clone()), Ok("0<!-- -->1".to_owned()));
  let mut text = ValueMap::default();
  text.insert("a".to_owned(), Value::str("ab"));
  text.insert("b".to_owned(), Value::F64(3.0));
  assert_eq!(eval(Expr::Builtin { name: Builtin::Repeat, args: vec![prop("a"), prop("b")] }, text), Ok("ababab".to_owned()));
}
