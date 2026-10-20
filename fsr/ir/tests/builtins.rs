use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::ast::{Component, Tmpl};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Owner;
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

#[tokio::test]
async fn query_strings_encode_the_way_url_search_params_does() {
  use snapfire_fsr_ir::ast::Builtin;
  let pairs = Expr::Object(vec![Entry::Field("q".to_owned(), str("tea & cake")), Entry::Field("page".to_owned(), Expr::Lit(Lit::Float(2.0))), Entry::Field("é".to_owned(), str("a+b/c*~"))]);
  assert_eq!(run(Expr::Builtin { name: Builtin::FormEncode, args: vec![pairs] }).await, Value::str("q=tea+%26+cake&page=2&%C3%A9=a%2Bb%2Fc*%7E"));
  assert_eq!(run(Expr::Builtin { name: Builtin::FormEncode, args: vec![str("?a=1+2&b=%41%zz")] }).await, Value::str("a=1+2&b=A%25zz"), "a query string is read and written again, a malformed escape kept");
  let get = |key: &str| Expr::Builtin { name: Builtin::FormGet, args: vec![str("a=1+2&b=x&a=3&%C3%A9=%26"), str(key)] };
  assert_eq!(run(get("a")).await, Value::str("1 2"), "the first value under a key, decoded");
  assert_eq!(run(get("é")).await, Value::str("&"), "a key is matched decoded");
  assert_eq!(run(get("c")).await, Value::Null, "a key with no value is null");
  assert_eq!(run(Expr::Builtin { name: Builtin::Unique, args: vec![over(&["a", "b", "a"])] }).await, seq(&["a", "b"]));
  let map = Expr::Builtin { name: Builtin::FromEntries, args: vec![Expr::Array(vec![Entry::Item(Expr::Array(vec![Entry::Item(Expr::Lit(Lit::Float(1.0))), Entry::Item(str("one"))]))])] };
  assert_eq!(run(Expr::Builtin { name: Builtin::HasKey, args: vec![map, Expr::Lit(Lit::Float(1.0))] }).await, Value::Bool(true), "a number key is held as its text");
  let has = |at: f64| Expr::Builtin { name: Builtin::HasKey, args: vec![over(&["a", "b"]), Expr::Lit(Lit::Float(at))] };
  assert_eq!(run(has(1.0)).await, Value::Bool(true), "an array holds its indices");
  assert_eq!(run(has(2.0)).await, Value::Bool(false));
  let parts = Expr::Array(vec![
    Entry::Item(str("a")),
    Entry::Item(str("")),
    Entry::Item(Expr::Lit(Lit::Float(0.0))),
    Entry::Item(Expr::Lit(Lit::Float(2.0))),
    Entry::Item(Expr::Lit(Lit::Bool(true))),
    Entry::Item(Expr::Lit(Lit::Null)),
    Entry::Item(Expr::Array(vec![Entry::Item(str("b")), Entry::Item(Expr::Array(vec![Entry::Item(str("c"))]))])),
    Entry::Item(Expr::Object(vec![Entry::Field("d".to_owned(), Expr::Lit(Lit::Bool(true))), Entry::Field("e".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("f".to_owned(), str("x"))])),
  ]);
  assert_eq!(run(Expr::Builtin { name: Builtin::ClassNames, args: vec![parts] }).await, Value::str("a 2 b c d f"), "clsx's rules: falsy and booleans skipped, arrays flattened, an object's truthy keys");
  let type_of = |e: Expr| Expr::Builtin { name: Builtin::TypeOf, args: vec![e] };
  for (value, want) in [
    (str("a"), "string"),
    (Expr::Lit(Lit::Float(1.5)), "number"),
    (Expr::Lit(Lit::Bool(false)), "boolean"),
    (Expr::Array(Vec::new()), "object"),
    (Expr::Object(Vec::new()), "object"),
    (Expr::Lit(Lit::Null), "undefined"),
  ] {
    assert_eq!(run(type_of(value)).await, Value::str(want), "{want}");
  }
  let is_array = |e: Expr| Expr::Builtin { name: Builtin::IsArray, args: vec![e] };
  assert_eq!(run(is_array(Expr::Array(Vec::new()))).await, Value::Bool(true));
  assert_eq!(run(is_array(Expr::Object(Vec::new()))).await, Value::Bool(false));
  assert_eq!(run(is_array(Expr::Lit(Lit::Null))).await, Value::Bool(false));
}

#[tokio::test]
async fn an_invalid_date_has_no_parts_and_no_iso_string() {
  use snapfire_fsr_ir::ast::Builtin;
  let invalid = Expr::Builtin { name: Builtin::DateMs, args: vec![str("next tuesday")] };
  let year = run(Expr::Builtin { name: Builtin::DatePart, args: vec![invalid.clone(), str("year")] }).await;
  assert!(matches!(year, Value::F64(f) if f.is_nan()), "{year:?}");
  let err = Interpreter::default().evaluate(&Expr::Builtin { name: Builtin::IsoString, args: vec![invalid] }, Vec::new()).await.unwrap_err();
  assert!(err.message.contains("invalid date"), "{}", err.message);
}

#[tokio::test]
async fn nan_is_falsy() {
  let interp = Interpreter::default();
  let not = |name: &str| Expr::Not(Box::new(Expr::Var(name.to_owned())));
  let scope = vec![("wide".to_owned(), Value::F64(f64::NAN)), ("narrow".to_owned(), Value::F32(f32::NAN))];
  assert_eq!(interp.evaluate(&not("wide"), scope.clone()).await.unwrap(), Value::Bool(true));
  assert_eq!(interp.evaluate(&not("narrow"), scope.clone()).await.unwrap(), Value::Bool(true));
  assert_eq!(interp.evaluate(&Expr::Not(Box::new(Expr::Lit(Lit::Float(f64::NAN)))), Vec::new()).await.unwrap(), Value::Bool(true));
  assert_eq!(interp.evaluate(&Expr::Not(Box::new(not("wide"))), scope).await.unwrap(), Value::Bool(false));
}

fn int(n: i128) -> Expr {
  Expr::Lit(Lit::Int(n))
}

fn divide(a: i128, b: i128) -> Expr {
  Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Div, Box::new(int(a)), Box::new(int(b)))
}

/// `/` divides as JavaScript numbers do: an integer that divides evenly stays one and anything else is a fraction.
#[tokio::test]
async fn dividing_two_integers_follows_javascript_numbers() {
  let interp = Interpreter::default();
  assert_eq!(interp.evaluate(&divide(120, 272), Vec::new()).await.unwrap(), Value::F64(120.0 / 272.0));
  assert_eq!(interp.evaluate(&divide(10, 2), Vec::new()).await.unwrap(), Value::Int(5));
  assert_eq!(interp.evaluate(&divide(-7, 2), Vec::new()).await.unwrap(), Value::F64(-3.5));
  assert_eq!(interp.evaluate(&divide(1, 0), Vec::new()).await.unwrap(), Value::F64(f64::INFINITY));
  let nan = interp.evaluate(&divide(0, 0), Vec::new()).await.unwrap();
  assert!(matches!(nan, Value::F64(n) if n.is_nan()), "{nan:?}");
}

/// A `bigint` division truncates toward zero and refuses zero, as JavaScript's does.
#[tokio::test]
async fn a_bigint_division_truncates_toward_zero() {
  let interp = Interpreter::default();
  let quot = |a, b| Expr::Arith(snapfire_fsr_ir::ast::ArithOp::Quot, Box::new(int(a)), Box::new(int(b)));
  assert_eq!(interp.evaluate(&quot(7, 2), Vec::new()).await.unwrap(), Value::Int(3));
  assert_eq!(interp.evaluate(&quot(-7, 2), Vec::new()).await.unwrap(), Value::Int(-3));
  assert!(interp.evaluate(&quot(1, 0), Vec::new()).await.is_err());
}

#[tokio::test]
async fn a_strings_length_counts_utf16_code_units() {
  let interp = Interpreter::default();
  for (text, units) in [("", 0.0), ("Zoë", 3.0), ("🙂", 2.0), ("a🙂b", 4.0)] {
    let length = Expr::Length(Box::new(str(text)));
    assert_eq!(interp.evaluate(&length, Vec::new()).await.unwrap(), Value::F64(units), "{text}");
    let component = Component::new(Owner::React, Vec::new(), Tmpl::Expr(length));
    let html = interp.render(&component, &ValueMap::default(), &Components::new()).unwrap().html;
    assert_eq!(html, units.to_string(), "{text}");
  }
}

#[tokio::test]
async fn trim_strips_what_javascript_strips() {
  let interp = Interpreter::default();
  for (text, trimmed) in [(" a ", "a"), ("\u{feff}a\u{feff}", "a"), ("\u{85}a\u{85}", "\u{85}a\u{85}"), ("\u{2028}\u{3000}a\t\n", "a")] {
    assert_eq!(interp.evaluate(&call(Builtin::Trim, &[text]), Vec::new()).await.unwrap(), Value::str(trimmed), "{text:?}");
  }
}

#[tokio::test]
async fn starts_with_takes_a_position_in_utf16_code_units() {
  let interp = Interpreter::default();
  let at = |s: &str, search: &str, position: i128| Expr::Builtin { name: Builtin::StartsWith, args: vec![str(s), str(search), int(position)] };
  for (s, search, position, holds) in [("abc", "b", 1, true), ("abc", "a", 1, false), ("🙂;", ";", 2, true), ("🙂;", ";", 1, false), ("abc", "", 9, true), ("abc", "a", -4, true)] {
    assert_eq!(interp.evaluate(&at(s, search, position), Vec::new()).await.unwrap(), Value::Bool(holds), "{s} {search} {position}");
  }
}

#[tokio::test]
async fn includes_finds_a_number_whatever_its_representation_and_finds_nan() {
  let interp = Interpreter::default();
  let has = |items: Vec<Lit>, wanted: Lit| Expr::Builtin { name: Builtin::Includes, args: vec![Expr::Array(items.into_iter().map(|l| Entry::Item(Expr::Lit(l))).collect()), Expr::Lit(wanted)] };
  assert_eq!(interp.evaluate(&has(vec![Lit::Int(0), Lit::Int(3)], Lit::Float(3.0)), Vec::new()).await.unwrap(), Value::Bool(true));
  assert_eq!(interp.evaluate(&has(vec![Lit::Float(f64::NAN)], Lit::Float(f64::NAN)), Vec::new()).await.unwrap(), Value::Bool(true));
  assert_eq!(interp.evaluate(&has(vec![Lit::Int(1)], Lit::Str("1".to_owned())), Vec::new()).await.unwrap(), Value::Bool(false));
}

#[tokio::test]
async fn graphemes_split_as_intl_segmenter_does_with_utf16_indexes() {
  let interp = Interpreter::default();
  let text = "a👍🏽e\u{301}";
  let all = Expr::Builtin { name: Builtin::Graphemes, args: vec![str(text)] };
  let segment = |s: &str, index: f64| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("segment".to_owned(), Value::str(s));
    map.insert("index".to_owned(), Value::F64(index));
    map.insert("input".to_owned(), Value::str(text));
    Value::Map(map)
  };
  assert_eq!(interp.evaluate(&all, Vec::new()).await.unwrap(), Value::Seq(vec![segment("a", 0.0), segment("👍🏽", 1.0), segment("e\u{301}", 5.0)].into()));
  let at = |i: f64| Expr::Builtin { name: Builtin::Graphemes, args: vec![str(text), Expr::Lit(Lit::Float(i))] };
  assert_eq!(interp.evaluate(&at(3.0), Vec::new()).await.unwrap(), segment("👍🏽", 1.0), "an index inside a cluster finds the cluster");
  assert_eq!(interp.evaluate(&at(6.0), Vec::new()).await.unwrap(), segment("e\u{301}", 5.0));
  assert_eq!(interp.evaluate(&at(7.0), Vec::new()).await.unwrap(), Value::Null, "past the end is undefined");
  assert_eq!(interp.evaluate(&at(-1.0), Vec::new()).await.unwrap(), Value::Null);
}

#[tokio::test]
async fn a_string_spreads_into_an_array_as_its_code_points() {
  let interp = Interpreter::default();
  let spread = Expr::Array(vec![Entry::Spread(str("a👍🏽"))]);
  assert_eq!(interp.evaluate(&spread, Vec::new()).await.unwrap(), Value::Seq(vec![Value::str("a"), Value::str("👍"), Value::str("🏽")].into()));
}
