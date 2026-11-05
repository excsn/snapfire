//! The bodies `fixtures/native_bodies.rs` is written from, shared by the test that keeps the file current and the test that runs it.

use snapfire_fsr_ir::Body;
use snapfire_fsr_ir::ast::{ArithOp, Builtin, CompareOp, Entry, Expr, Lit, Stmt};

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

fn var(name: &str) -> Box<Expr> {
  Box::new(Expr::Var(name.to_owned()))
}

fn num(n: f64) -> Box<Expr> {
  Box::new(Expr::Lit(Lit::Float(n)))
}

fn arith(op: ArithOp, a: Box<Expr>, b: Box<Expr>) -> Box<Expr> {
  Box::new(Expr::Arith(op, a, b))
}

fn compare(op: CompareOp, a: Box<Expr>, b: Box<Expr>) -> Box<Expr> {
  Box::new(Expr::Compare(op, a, b))
}

fn range(count: Box<Expr>) -> Expr {
  Expr::Builtin { name: Builtin::Range, args: vec![*count] }
}

fn items(items: Vec<Box<Expr>>) -> Expr {
  Expr::Array(items.into_iter().map(|e| Entry::Item(*e)).collect())
}

pub fn counted_numbers() -> Body {
  let set = |name: &str, expr: Box<Expr>| Stmt::Set { name: name.to_owned(), expr: *expr };
  vec![
    Stmt::Let { name: "n".to_owned(), expr: Expr::Lit(Lit::Float(10.0)) },
    Stmt::Let { name: "sum".to_owned(), expr: Expr::Lit(Lit::Float(0.0)) },
    Stmt::Let { name: "count".to_owned(), expr: Expr::Lit(Lit::Float(0.0)) },
    Stmt::ForOf {
      name: "k".to_owned(),
      over: range(var("n")),
      body: vec![
        Stmt::Let { name: "i".to_owned(), expr: *arith(ArithOp::Add, num(2.0), arith(ArithOp::Mul, var("k"), num(3.0))) },
        Stmt::If { cond: *compare(CompareOp::Eq, arith(ArithOp::Rem, var("i"), num(4.0)), num(2.0)), then: vec![Stmt::Continue], r#else: vec![] },
        set("sum", arith(ArithOp::Rem, arith(ArithOp::Add, var("sum"), var("i")), num(7.0))),
        set("count", arith(ArithOp::Add, var("count"), num(1.0))),
      ],
    },
    Stmt::Return(items(vec![
      var("sum"),
      var("count"),
      arith(ArithOp::Div, var("sum"), num(0.0)),
      arith(ArithOp::Rem, num(-7.0), num(3.0)),
      compare(CompareOp::Lt, num(f64::NAN), num(1.0)),
      compare(CompareOp::Ne, num(f64::NAN), num(f64::NAN)),
      arith(ArithOp::Pow, num(2.0), num(10.0)),
      Box::new(Expr::Ternary(compare(CompareOp::Ge, var("count"), num(3.0)), var("count"), num(-1.0))),
      arith(ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Floor, args: vec![*arith(ArithOp::Div, var("sum"), num(2.0))] }), num(1.0)),
    ])),
  ]
}

pub fn counted_fraction() -> Body {
  let pushed = items(vec![arith(ArithOp::Mul, var("k"), num(2.0))]);
  let Expr::Array(mut entries) = pushed else { unreachable!("an array") };
  entries.insert(0, Entry::Spread(Expr::Var("out".to_owned())));
  vec![
    Stmt::Let { name: "out".to_owned(), expr: Expr::Array(vec![]) },
    Stmt::ForOf { name: "k".to_owned(), over: range(num(2.5)), body: vec![Stmt::Expr(Expr::Builtin { name: Builtin::Overwrite, args: vec![Expr::Var("out".to_owned()), Expr::Array(entries)] })] },
    Stmt::Return(Expr::Var("out".to_owned())),
  ]
}

pub fn numbers_beside_an_integer() -> Body {
  vec![
    Stmt::Let { name: "a".to_owned(), expr: Expr::Lit(Lit::Int(2)) },
    Stmt::Let { name: "b".to_owned(), expr: Expr::Lit(Lit::Float(0.5)) },
    Stmt::Let { name: "c".to_owned(), expr: *arith(ArithOp::Add, var("a"), var("b")) },
    Stmt::Let { name: "d".to_owned(), expr: *arith(ArithOp::Mul, var("b"), num(4.0)) },
    Stmt::Return(items(vec![var("c"), var("d")])),
  ]
}

pub fn counted_while() -> Body {
  vec![
    Stmt::Let { name: "i".to_owned(), expr: Expr::Lit(Lit::Float(0.0)) },
    Stmt::Let { name: "s".to_owned(), expr: Expr::Lit(Lit::Float(0.0)) },
    Stmt::While {
      cond: *compare(CompareOp::Lt, var("i"), num(5.0)),
      body: vec![Stmt::Set { name: "s".to_owned(), expr: *arith(ArithOp::Add, var("s"), var("i")) }],
      step: vec![Stmt::Set { name: "i".to_owned(), expr: *arith(ArithOp::Add, var("i"), num(1.0)) }],
    },
    Stmt::Return(items(vec![var("i"), var("s")])),
  ]
}

pub fn written_bodies() -> Vec<(&'static str, Body)> {
  vec![
    ("sorted_by_n", sorted_copy(by_n())),
    ("sorted_plainly", sorted_plainly()),
    ("sorted_in_place", sorted_in_place()),
    ("object_from_a_block", object_from_a_block()),
    ("local_from_a_block", local_from_a_block()),
    ("counted_numbers", counted_numbers()),
    ("counted_fraction", counted_fraction()),
    ("numbers_beside_an_integer", numbers_beside_an_integer()),
    ("counted_while", counted_while()),
  ]
}

pub fn sorted_in_place() -> Body {
  let list = || Expr::Var("list".to_owned());
  vec![
    listed(),
    Stmt::Expr(Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::Overwrite, args: vec![list(), Expr::Sort(Box::new(list()), Box::new(by_n()))] }),
    Stmt::Return(list()),
  ]
}
