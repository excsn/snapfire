use snapfire_fsr_core::Value;
use snapfire_fsr_ir::ast::{Builtin, CompareOp, Entry, Lit};
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

fn str(s: &str) -> Expr {
  Expr::Lit(Lit::Str(s.to_owned()))
}

fn call(name: Builtin, args: &[&str]) -> Expr {
  Expr::Builtin { name, args: args.iter().map(|s| str(s)).collect() }
}

fn seq(items: &[&str]) -> Value {
  Value::Seq(items.iter().map(|s| Value::str(*s)).collect())
}

async fn run(e: Expr) -> Value {
  Interpreter::default().evaluate(&e, Vec::new()).await.unwrap()
}

async fn fails(e: Expr) -> String {
  Interpreter::default().evaluate(&e, Vec::new()).await.unwrap_err().to_string()
}

#[tokio::test]
async fn split_is_joins_inverse() {
  assert_eq!(run(call(Builtin::Split, &["a,b,c", ","])).await, seq(&["a", "b", "c"]));
  assert_eq!(run(call(Builtin::Split, &["a", ","])).await, seq(&["a"]), "a separator the subject lacks is one piece");
  assert_eq!(run(call(Builtin::Split, &["", ","])).await, seq(&[""]), "an empty subject is one empty piece");
  assert_eq!(run(call(Builtin::Split, &[",a,", ","])).await, seq(&["", "a", ""]), "a leading and a trailing separator each keep their empty");
  assert_eq!(run(call(Builtin::Split, &["a-b", "-b"])).await, seq(&["a", ""]), "a multi-character separator");
}

#[tokio::test]
async fn split_refuses_an_empty_separator() {
  let err = fails(call(Builtin::Split, &["ab", ""])).await;
  assert!(err.contains("separator that is not empty"), "{err}");
}

#[tokio::test]
async fn split_refuses_more_pieces_than_it_will_build() {
  let subject = "a".repeat(2_000_001);
  let err = fails(Expr::Builtin { name: Builtin::Split, args: vec![str(&subject), str("a")] }).await;
  assert!(err.contains("pieces"), "{err}");
}

#[tokio::test]
async fn starts_with_and_ends_with_answer_the_edges() {
  assert_eq!(run(call(Builtin::StartsWith, &["/fr_FR/agents", "/fr_FR"])).await, Value::Bool(true));
  assert_eq!(run(call(Builtin::StartsWith, &["/agents", "/fr_FR"])).await, Value::Bool(false));
  assert_eq!(run(call(Builtin::StartsWith, &["ab", ""])).await, Value::Bool(true), "every string starts with the empty one");
  assert_eq!(run(call(Builtin::EndsWith, &["page.tsx", ".tsx"])).await, Value::Bool(true));
  assert_eq!(run(call(Builtin::EndsWith, &["page.ts", ".tsx"])).await, Value::Bool(false));
  assert_eq!(run(call(Builtin::EndsWith, &["ab", ""])).await, Value::Bool(true));
}

#[tokio::test]
async fn replace_takes_the_first_occurrence_only() {
  assert_eq!(run(call(Builtin::Replace, &["on hold", " ", "-"])).await, Value::str("on-hold"));
  assert_eq!(run(call(Builtin::Replace, &["a a a", " ", "-"])).await, Value::str("a-a a"), "the first only, as `replace` does and `replaceAll` does not");
  assert_eq!(run(call(Builtin::Replace, &["abc", "z", "-"])).await, Value::str("abc"), "no match leaves the subject alone");
  assert_eq!(run(call(Builtin::Replace, &["abc", "", "-"])).await, Value::str("-abc"), "an empty pattern matches at the front");
}

#[tokio::test]
async fn replace_expands_the_dollar_sequences_javascript_expands() {
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "$&$&"])).await, Value::str("a--b"), "$& is the match");
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "$$"])).await, Value::str("a$b"), "$$ is one dollar");
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "$`"])).await, Value::str("aab"), "$` is the text before");
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "$'"])).await, Value::str("abb"), "$' is the text after");
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "$1"])).await, Value::str("a$1b"), "a string pattern has no captures, so $1 is literal");
  assert_eq!(run(call(Builtin::Replace, &["a-b", "-", "x$"])).await, Value::str("ax$b"), "a trailing dollar is literal");
}
