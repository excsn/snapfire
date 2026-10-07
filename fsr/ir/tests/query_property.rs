//! `new URLSearchParams(text).toString()` under generated query strings. The
//! oracle is the WHATWG serializer in `form_urlencoded` over the pairs the text
//! was built from, so however a pair is spelled it reads back as that pair.

use bolero::check;
use proptest::prelude::*;
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::ast::{Builtin, Lit};
use snapfire_fsr_ir::{Expr, Interpreter};

fn encoded(text: &str) -> Result<Value, String> {
  let expr = Expr::Builtin { name: Builtin::FormEncode, args: vec![Expr::Lit(Lit::Str(text.to_owned()))] };
  tokio::runtime::Builder::new_current_thread()
    .build()
    .unwrap()
    .block_on(Interpreter::default().evaluate(&expr, Vec::new()))
    .map_err(|e| e.to_string())
}

/// A character spelled any way the grammar allows: as itself when that is
/// unambiguous, else as `%XX` of each UTF-8 byte in either case, a space also
/// as `+`.
fn spell(text: &str, choices: &[u8]) -> String {
  let mut out = String::new();
  for (i, c) in text.chars().enumerate() {
    let choice = choices[i % choices.len()];
    let reserved = matches!(c, '&' | '=' | '+' | '%' | '#' | '?');
    match (c, choice % 3) {
      (' ', 0) => out.push('+'),
      (_, 0 | 1) if !reserved && c != ' ' => out.push(c),
      _ => {
        let mut buf = [0u8; 4];
        for b in c.encode_utf8(&mut buf).bytes() {
          out.push_str(&if choice % 2 == 0 { format!("%{b:02X}") } else { format!("%{b:02x}") });
        }
      }
    }
  }
  out
}

fn query() -> impl Strategy<Value = (Vec<(String, String)>, String)> {
  (
    prop::collection::vec(
      ("(?s).{0,6}", prop::option::of("(?s).{0,8}"), prop::collection::vec(any::<u8>(), 1..8), 0..3usize),
      0..6,
    ),
    any::<bool>(),
  )
    .prop_map(|(pairs, question)| {
      let mut text = if question { "?".to_owned() } else { String::new() };
      let mut meant = Vec::new();
      for (key, value, choices, gap) in pairs {
        if key.is_empty() && value.is_none() {
          continue;
        }
        if !text.is_empty() && !text.ends_with(['&', '?']) {
          text.push('&');
        }
        text.push_str(&spell(&key, &choices));
        if let Some(v) = &value {
          text.push('=');
          text.push_str(&spell(v, &choices));
        }
        text.push_str(&"&".repeat(gap));
        meant.push((key, value.unwrap_or_default()));
      }
      (meant, text)
    })
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(1000))]

  #[test]
  fn a_query_string_reads_back_as_the_pairs_it_spells((meant, text) in query()) {
    let expected = form_urlencoded::Serializer::new(String::new()).extend_pairs(&meant).finish();
    prop_assert_eq!(encoded(&text), Ok(Value::str(expected)), "{}", text);
  }
}

/// Any text at all is read as a query string and written again.
#[test]
fn fuzz_any_text_is_a_query_string() {
  check!().with_type::<String>().for_each(|text| {
    assert!(encoded(text).is_ok(), "{text:?}");
  });
}
