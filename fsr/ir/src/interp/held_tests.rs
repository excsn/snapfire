use std::sync::atomic::{AtomicI64, Ordering};

use super::*;
use crate::ast::Lit;

struct Ticking(AtomicI64);

impl Clock for Ticking {
  fn now(&self) -> i128 {
    self.0.fetch_add(1, Ordering::SeqCst) as i128
  }
}

fn int(n: i128) -> Expr {
  Expr::Lit(Lit::Int(n))
}

fn chapters() -> Expr {
  let lambda = Expr::Lambda { params: vec!["i".to_owned()], body: Box::new(Expr::object(vec![("n", Expr::var("i"))])) };
  Expr::Map(Box::new(Expr::Builtin { name: Builtin::Range, args: vec![int(3)] }), Box::new(lambda))
}

fn interpreter(consts: Vec<(&str, Expr)>) -> Interpreter {
  let consts: Consts = consts.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
  Interpreter::with_clock(Arc::new(Ticking(AtomicI64::new(0)))).with_consts(Some(Arc::new(consts)))
}

#[test]
fn a_pure_constant_is_held_after_its_first_read() {
  let interpreter = interpreter(vec![("a#CHAPTERS", chapters())]);
  assert!(interpreter.held.get("a#CHAPTERS").is_none());
  let mut env = Env::detached(&interpreter, Vec::new());
  let read = env.eval_sync(&Expr::Const("a#CHAPTERS".to_owned())).unwrap();
  assert_eq!(Some(&read), interpreter.held.get("a#CHAPTERS"));
  let mut fresh = Env::detached(&interpreter, Vec::new());
  assert_eq!(read, fresh.eval_sync(&chapters()).unwrap());
}

#[test]
fn a_held_constant_is_indexed_in_place() {
  let interpreter = interpreter(vec![("a#CHAPTERS", chapters())]);
  let mut env = Env::detached(&interpreter, vec![(Name::new_static("i"), Value::Int(2))]);
  let read = Expr::Const("a#CHAPTERS".to_owned()).index(Expr::var("i")).field("n");
  assert_eq!(env.eval_sync(&read).unwrap(), Value::F64(2.0));
  assert_eq!(env.eval_sync(&read).unwrap(), Value::F64(2.0));
}

#[test]
fn a_parameter_the_call_leaves_out_is_undefined_and_takes_its_default() {
  let pick = Expr::Lambda {
    params: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
    body: Box::new(Expr::Coalesce(Box::new(Expr::var("c")), Box::new(Expr::lit_str("fallback")))),
  };
  let interpreter = interpreter(Vec::new());
  let mut env = Env::detached(&interpreter, Vec::new());
  let call = Expr::Apply { f: Box::new(pick), args: vec![int(1), int(2)] };
  assert_eq!(env.eval_sync(&call).unwrap(), Value::str("fallback"));
}

/// `p => { p.x = 2; }` as the lowerer writes it: the value and the parameter's last value, then the parameter's position.
fn sets_x(to: i128) -> Expr {
  let last = Expr::Object(vec![Entry::Spread(Expr::var("p")), Entry::Field("x".to_owned(), int(to))]);
  Expr::Lambda { params: vec!["p".to_owned()], body: Box::new(Expr::Builtin { name: Builtin::WriteBack, args: vec![Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Undefined)), Entry::Item(last)]), Expr::Array(vec![Entry::Item(int(0))])] }) }
}

#[test]
fn a_call_writes_what_the_function_changed_back_to_the_name_it_was_passed() {
  let interpreter = interpreter(Vec::new());
  let mut env = Env::detached(&interpreter, vec![(Name::new_static("f"), Value::Null)]);
  let o = Expr::Object(vec![Entry::Field("x".to_owned(), int(1)), Entry::Field("y".to_owned(), int(5))]);
  let body = Expr::Block(vec![
    Stmt::Let { name: "o".to_owned(), expr: o.clone() },
    Stmt::Expr(Expr::Apply { f: Box::new(sets_x(2)), args: vec![Expr::var("o")] }),
    Stmt::Return(Expr::var("o")),
  ]);
  let Value::Map(out) = env.eval_sync(&body).unwrap() else { panic!("a map") };
  assert_eq!((out.get("x"), out.get("y")), (Some(&Value::Int(2)), Some(&Value::Int(5))));
  let nested = Expr::Block(vec![
    Stmt::Let { name: "w".to_owned(), expr: Expr::Object(vec![Entry::Field("inner".to_owned(), o)]) },
    Stmt::Expr(Expr::Apply { f: Box::new(sets_x(3)), args: vec![Expr::var("w").field("inner")] }),
    Stmt::Return(Expr::var("w").field("inner").field("x")),
  ]);
  assert_eq!(env.eval_sync(&nested).unwrap(), Value::Int(3));
  let unheld = Expr::Apply { f: Box::new(sets_x(4)), args: vec![Expr::Object(Vec::new())] };
  assert_eq!(env.eval_sync(&unheld).unwrap(), Value::Undefined, "a value no name holds takes the change nowhere");
}

#[test]
fn a_callback_of_an_array_method_that_changes_its_item_fails() {
  let interpreter = interpreter(Vec::new());
  let mut env = Env::detached(&interpreter, Vec::new());
  let items = Expr::Array(vec![Entry::Item(Expr::Object(vec![Entry::Field("x".to_owned(), int(1))]))]);
  let mapped = Expr::Map(Box::new(items), Box::new(sets_x(2)));
  assert!(env.eval_sync(&mapped).unwrap_err().message.contains("callback of an array method"));
}

#[test]
fn one_name_passed_for_two_parameters_the_function_changes_fails() {
  let both = Expr::Lambda {
    params: vec!["p".to_owned(), "q".to_owned()],
    body: Box::new(Expr::Builtin { name: Builtin::WriteBack, args: vec![Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Undefined)), Entry::Item(Expr::var("p")), Entry::Item(Expr::var("q"))]), Expr::Array(vec![Entry::Item(int(0)), Entry::Item(int(1))])] }),
  };
  let interpreter = interpreter(Vec::new());
  let mut env = Env::detached(&interpreter, Vec::new());
  let body = Expr::Block(vec![Stmt::Let { name: "o".to_owned(), expr: Expr::Object(Vec::new()) }, Stmt::Expr(Expr::Apply { f: Box::new(both), args: vec![Expr::var("o"), Expr::var("o")] })]);
  assert!(env.eval_sync(&body).unwrap_err().message.contains("`o`"));
}

#[test]
fn a_pure_application_is_held() {
  let double = Expr::Lambda { params: vec!["x".to_owned()], body: Box::new(Expr::Arith(ArithOp::Mul, Box::new(Expr::var("x")), Box::new(int(2)))) };
  let interpreter = interpreter(vec![("a#SIX", Expr::Apply { f: Box::new(double), args: vec![int(3)] })]);
  let mut env = Env::detached(&interpreter, Vec::new());
  env.eval_sync(&Expr::Const("a#SIX".to_owned())).unwrap();
  assert!(interpreter.held.get("a#SIX").is_some());
}

#[test]
fn a_constant_that_reads_the_request_is_evaluated_on_every_read() {
  let interpreter = interpreter(vec![("a#NOW", Expr::Now), ("a#LOCALE", Expr::Locale), ("a#LATER", Expr::Arith(ArithOp::Add, Box::new(Expr::Const("a#NOW".to_owned())), Box::new(int(1))))]);
  let mut env = Env::detached(&interpreter, Vec::new());
  let first = env.eval_sync(&Expr::Const("a#LATER".to_owned())).unwrap();
  let second = env.eval_sync(&Expr::Const("a#LATER".to_owned())).unwrap();
  assert_ne!(first, second);
  env.eval_sync(&Expr::Const("a#LOCALE".to_owned())).unwrap();
  assert!(interpreter.held.get("a#NOW").is_none());
  assert!(interpreter.held.get("a#LATER").is_none());
  assert!(interpreter.held.get("a#LOCALE").is_none());
}
