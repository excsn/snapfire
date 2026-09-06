use snapfire_fsr_core::Value;
use snapfire_fsr_ir::ast::{CompareOp, Entry, Lit};
use snapfire_fsr_ir::{Expr, Interpreter};

fn over(items: &[&str]) -> Expr {
  Expr::Array(items.iter().map(|s| Entry::Item(Expr::Lit(Lit::Str((*s).to_owned())))).collect())
}

fn matching(slug: &str) -> Expr {
  Expr::Lambda {
    params: vec!["s".to_owned()],
    body: Box::new(Expr::Compare(
      CompareOp::Eq,
      Box::new(Expr::Var("s".to_owned())),
      Box::new(Expr::Lit(Lit::Str(slug.to_owned()))),
    )),
  }
}

#[tokio::test]
async fn find_index_answers_the_position_and_minus_one_for_a_miss() {
  let interp = Interpreter::default();
  let at = |slug: &str| Expr::FindIndex(Box::new(over(&["a", "b", "c"])), Box::new(matching(slug)));

  assert_eq!(interp.evaluate(&at("a"), Vec::new()).await.unwrap(), Value::F64(0.0));
  assert_eq!(interp.evaluate(&at("c"), Vec::new()).await.unwrap(), Value::F64(2.0));
  assert_eq!(interp.evaluate(&at("z"), Vec::new()).await.unwrap(), Value::F64(-1.0));
}

#[tokio::test]
async fn find_index_passes_the_position_to_the_predicate() {
  let interp = Interpreter::default();
  let after_first = Expr::FindIndex(
    Box::new(over(&["a", "b", "a"])),
    Box::new(Expr::Lambda {
      params: vec!["s".to_owned(), "i".to_owned()],
      body: Box::new(Expr::Logic(
        snapfire_fsr_ir::ast::LogicOp::And,
        Box::new(Expr::Compare(CompareOp::Eq, Box::new(Expr::Var("s".to_owned())), Box::new(Expr::Lit(Lit::Str("a".to_owned()))))),
        Box::new(Expr::Compare(CompareOp::Gt, Box::new(Expr::Var("i".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0))))),
      )),
    }),
  );
  assert_eq!(interp.evaluate(&after_first, Vec::new()).await.unwrap(), Value::F64(2.0));
}
