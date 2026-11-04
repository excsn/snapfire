//! The bodies `fixtures/native_bodies.rs` is written from, shared by the test that keeps the file current and the test that runs it.

use snapfire_fsr_ir::Body;
use snapfire_fsr_ir::ast::{Entry, Expr, Lit, Stmt};

pub fn item(n: f64, name: &str) -> Expr {
  Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Lit(Lit::Float(n))), Entry::Field("name".to_owned(), Expr::Lit(Lit::Str(name.into())))])
}

pub fn by_n() -> Expr {
  let field = |v: &str| Box::new(Expr::Field(Box::new(Expr::Var(v.to_owned())), "n".to_owned()));
  Expr::Lambda { params: vec!["a".to_owned(), "b".to_owned()], body: Box::new(Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Sub, field("a"), field("b"))) }
}

pub fn listed() -> Stmt {
  Stmt::Let { name: "list".to_owned(), expr: Expr::Array(vec![Entry::Item(item(3.0, "c")), Entry::Item(item(1.0, "a")), Entry::Item(item(3.0, "d")), Entry::Item(item(2.0, "b"))]) }
}

pub fn sorted_copy(comparator: Expr) -> Body {
  vec![listed(), Stmt::Return(Expr::Sort(Box::new(Expr::Var("list".to_owned())), Box::new(comparator)))]
}

pub fn sorted_plainly() -> Body {
  let items = [Expr::Lit(Lit::Int(10)), Expr::Lit(Lit::Int(9)), Expr::Lit(Lit::Int(1)), Expr::Lit(Lit::Str("b".into())), Expr::Lit(Lit::Str("a".into()))];
  vec![
    Stmt::Let { name: "list".to_owned(), expr: Expr::Array(items.into_iter().map(Entry::Item).collect()) },
    Stmt::Return(Expr::Sort(Box::new(Expr::Var("list".to_owned())), Box::new(Expr::Lit(Lit::Null)))),
  ]
}

pub fn object_from_a_block() -> Body {
  vec![
    Stmt::Let { name: "found".to_owned(), expr: Expr::Block(vec![Stmt::Return(item(1.0, "a"))]) },
    Stmt::Return(Expr::Var("found".to_owned())),
  ]
}

pub fn local_from_a_block() -> Body {
  vec![
    Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Int(2)) },
    Stmt::Let { name: "found".to_owned(), expr: Expr::Block(vec![Stmt::Return(Expr::Var("n".to_owned()))]) },
    Stmt::Return(Expr::Var("found".to_owned())),
  ]
}

pub fn written_bodies() -> Vec<(&'static str, Body)> {
  vec![("sorted_by_n", sorted_copy(by_n())), ("sorted_plainly", sorted_plainly()), ("sorted_in_place", sorted_in_place()), ("object_from_a_block", object_from_a_block()), ("local_from_a_block", local_from_a_block())]
}

pub fn sorted_in_place() -> Body {
  let list = || Expr::Var("list".to_owned());
  vec![
    listed(),
    Stmt::Expr(Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::Overwrite, args: vec![list(), Expr::Sort(Box::new(list()), Box::new(by_n()))] }),
    Stmt::Return(list()),
  ]
}
