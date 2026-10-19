use snapfire_fsr_core::Value;
use snapfire_fsr_ir::ast::{Builtin, Entry, Lit};
use snapfire_fsr_ir::jsregex;
use snapfire_fsr_ir::{Expr, Interpreter};

fn str(s: &str) -> Expr {
  Expr::Lit(Lit::Str(s.to_owned()))
}

fn re(pattern: &str, flags: &str) -> Expr {
  Expr::Object(vec![Entry::Field(jsregex::PATTERN.to_owned(), str(pattern)), Entry::Field(jsregex::FLAGS.to_owned(), str(flags))])
}

fn call(name: Builtin, args: Vec<Expr>) -> Expr {
  Expr::Builtin { name, args }
}

fn strs(items: &[&str]) -> Value {
  Value::Seq(items.iter().map(|s| Value::str(*s)).collect())
}

async fn run(e: Expr) -> Value {
  Interpreter::default().evaluate(&e, Vec::new()).await.unwrap()
}

async fn fails(e: Expr) -> String {
  Interpreter::default().evaluate(&e, Vec::new()).await.unwrap_err().to_string()
}

/// Each expected value is what Chrome 153 answered for the same call.
#[tokio::test]
async fn lookaround_and_backreferences_match_as_v8_does() {
  assert_eq!(run(call(Builtin::Replace, vec![str("1234567"), re(r"\d(?=(\d{3})+$)", "g"), str("$&,")])).await, Value::str("1,234,567"));
  assert_eq!(run(call(Builtin::Match, vec![str("cost $42 and $7"), re(r"(?<=\$)\d+", "g")])).await, strs(&["42", "7"]));
  assert_eq!(run(call(Builtin::Match, vec![str("hello aabb"), re(r"(\w)\1", "g")])).await, strs(&["ll", "aa", "bb"]));
  assert_eq!(run(call(Builtin::Replace, vec![str("aXbX"), re("(?<=a)X", ""), str("-")])).await, Value::str("a-bX"));
}

#[tokio::test]
async fn named_groups_and_split_captures_match_as_v8_does() {
  assert_eq!(run(call(Builtin::Replace, vec![str("2026-10 and 1999-01"), re(r"(?<y>\d{4})-(?<m>\d\d)", "g"), str("$<m>/$<y>")])).await, Value::str("10/2026 and 01/1999"));
  assert_eq!(run(call(Builtin::Split, vec![str("a1b22c"), re(r"(\d)+", "")])).await, strs(&["a", "1", "b", "2", "c"]));
  assert_eq!(run(call(Builtin::RegexTest, vec![re(r"\p{L}+", "v"), str("é")])).await, Value::Bool(true));
}

#[tokio::test]
async fn without_u_a_pattern_sees_utf16_code_units() {
  assert_eq!(run(call(Builtin::RegexTest, vec![re("^.$", ""), str("😀")])).await, Value::Bool(false));
  assert_eq!(run(call(Builtin::RegexTest, vec![re("^.$", "u"), str("😀")])).await, Value::Bool(true));
  assert_eq!(run(call(Builtin::RegexTest, vec![re("^..$", ""), str("😀")])).await, Value::Bool(true));
  assert_eq!(run(call(Builtin::Search, vec![str("a😀b"), re("b", "")])).await, Value::F64(3.0));
  assert_eq!(run(call(Builtin::Replace, vec![str("x😀y😀"), re("y", ""), str("[$`]")])).await, Value::str("x😀[x😀]😀"));
  let err = fails(call(Builtin::Match, vec![str("😀"), re(".", "")])).await;
  assert!(err.contains("surrogate"), "a match that would split a surrogate pair is refused: {err}");
}

#[test]
fn a_nested_unbounded_quantifier_is_refused_at_compile_time() {
  for pattern in ["(a+)+$", "(a*)*b", r"(?:\s*\w+)*$", "(a|b+)+c", "((a+))+"] {
    let err = jsregex::compiled(pattern, "").unwrap_err();
    assert!(err.contains("a nested quantifier"), "{pattern}: {err}");
  }
  for pattern in [r"(\d+,)*\d+", r"(\d{2})+", "(ab)+", r"(?:\s*,\s*\w)+", "(a?)+", r"[(+]+"] {
    assert!(jsregex::compiled(pattern, "").is_ok(), "{pattern} has a fixed atom in every repetition");
  }
}

#[tokio::test]
async fn a_pattern_or_a_subject_past_its_cap_is_refused() {
  let long = "a".repeat(jsregex::MAX_PATTERN + 1);
  assert!(jsregex::compiled(&long, "").unwrap_err().contains("longer than"));
  let subject = "a".repeat(jsregex::MAX_INPUT + 1);
  let err = fails(call(Builtin::RegexTest, vec![re("a", ""), str(&subject)])).await;
  assert!(err.contains("longer than"), "{err}");
}

#[test]
fn the_sticky_flag_and_an_unknown_flag_are_refused_by_name() {
  assert_eq!(jsregex::compiled("a", "y").unwrap_err(), "the sticky flag `y`");
  assert_eq!(jsregex::compiled("a", "q").unwrap_err(), "the flag `q`");
}
