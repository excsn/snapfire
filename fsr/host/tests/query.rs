//! The query and path decoders under generated input. A query string and a
//! path segment are whatever the client sent, so nothing here may panic. A
//! query must mean what the form-urlencoded grammar says it means however the
//! client chose to spell it.

use bolero::check;
use proptest::prelude::*;
use snapfire_fsr_host::{fragment_of, payload_of, percent_decoded};
use snapfire_fsr_runtime::parse_query;

/// One `key[=value]` pair as meant: `None` is the bare key.
type Pair = (String, Option<String>);

fn keys() -> impl Strategy<Value = String> {
  prop_oneof![
    3 => prop_oneof![Just("__fragment"), Just("__payload"), Just("n"), Just("by"), Just("enc")].prop_map(str::to_owned),
    1 => prop_oneof![Just("__fragments"), Just("_fragment"), Just("__"), Just("a b"), Just("é"), Just("🌍"), Just("")].prop_map(str::to_owned),
    1 => "(?s).{0,6}",
  ]
}

fn values() -> impl Strategy<Value = Option<String>> {
  prop_oneof![
    1 => Just(None),
    1 => Just(Some(String::new())),
    2 => prop_oneof![Just("side"), Just("a b"), Just("a+b"), Just("50%"), Just("x=y"), Just("&"), Just("é"), Just("🌍")]
      .prop_map(|v| Some(v.to_owned())),
    2 => "(?s).{0,8}".prop_map(Some),
  ]
}

/// A character spelled any way the grammar allows: as itself when that is
/// unambiguous, else as `%XX` of each UTF-8 byte in either case, a space also
/// as `+`.
fn spell(text: &str, choices: &[u8]) -> String {
  let mut out = String::new();
  for (i, c) in text.chars().enumerate() {
    let choice = choices.get(i % choices.len().max(1)).copied().unwrap_or(0);
    let reserved = matches!(c, '&' | '=' | '+' | '%' | '#');
    match (c, choice % 3) {
      (' ', 0) => out.push('+'),
      (_, 0 | 1) if !reserved && c != ' ' => out.push(c),
      _ => {
        let mut buf = [0u8; 4];
        for b in c.encode_utf8(&mut buf).bytes() {
          if choice % 2 == 0 {
            out.push_str(&format!("%{b:02X}"));
          } else {
            out.push_str(&format!("%{b:02x}"));
          }
        }
      }
    }
  }
  out
}

/// Pairs as meant and one spelling of them, with empty segments between.
fn query() -> impl Strategy<Value = (Vec<Pair>, String)> {
  (
    prop::collection::vec((keys(), values(), prop::collection::vec(any::<u8>(), 1..8), 0..3usize), 0..6),
    0..3usize,
  )
    .prop_map(|(pairs, lead)| {
      let mut text = "&".repeat(lead);
      let mut meant = Vec::new();
      for (key, value, choices, gap) in pairs {
        if key.is_empty() && value.is_none() {
          continue;
        }
        if !text.is_empty() && !text.ends_with('&') {
          text.push('&');
        }
        text.push_str(&spell(&key, &choices));
        if let Some(v) = &value {
          text.push('=');
          text.push_str(&spell(v, &choices));
        }
        text.push_str(&"&".repeat(gap));
        meant.push((key, value));
      }
      (meant, text)
    })
}

fn fragment_meant(pairs: &[Pair]) -> Option<Option<String>> {
  pairs
    .iter()
    .find(|(k, _)| k == "__fragment")
    .map(|(_, v)| v.clone().filter(|v| !v.is_empty()))
}

/// The segment as meant and one spelling of it. `+` is a plus in a path. A
/// `%` that two hex digits do not follow is a percent. Each stray `%` ends on
/// a character that is not a hex digit, so the part after it cannot complete it.
fn segment() -> impl Strategy<Value = (String, String)> {
  prop::collection::vec(
    prop_oneof![
      3 => ("(?s).", any::<bool>()).prop_map(|(c, encode)| {
        let raw = if encode || c == "%" { c.bytes().map(|b| format!("%{b:02X}")).collect() } else { c.clone() };
        (c, raw)
      }),
      1 => prop_oneof![Just("%z"), Just("%g"), Just("%+1"), Just("%-1"), Just("% 1"), Just("%1z"), Just("%a\u{e9}"), Just("+")]
        .prop_map(|s| (s.to_owned(), s.to_owned())),
    ],
    0..8,
  )
  .prop_map(|parts| parts.into_iter().fold((String::new(), String::new()), |(mut m, mut r), (a, b)| {
    m.push_str(&a);
    r.push_str(&b);
    (m, r)
  }))
}

/// Each byte of `text` dropped, doubled and flipped, then `text` cut at every length.
fn mutations(text: &str) -> Vec<String> {
  let bytes = text.as_bytes();
  let mut out = Vec::new();
  for i in 0..bytes.len() {
    let mut dropped = bytes.to_vec();
    dropped.remove(i);
    let mut doubled = bytes.to_vec();
    doubled.insert(i, bytes[i]);
    let mut flipped = bytes.to_vec();
    flipped[i] ^= 0x20;
    for b in [dropped, doubled, flipped] {
      out.push(String::from_utf8_lossy(&b).into_owned());
    }
    out.push(String::from_utf8_lossy(&bytes[..i]).into_owned());
  }
  out
}

proptest! {
  #![proptest_config(ProptestConfig::with_cases(2000))]

  #[test]
  fn a_query_asks_for_the_fragment_its_first_fragment_key_names((meant, text) in query()) {
    prop_assert_eq!(fragment_of(&text), fragment_meant(&meant), "{}", text);
  }

  #[test]
  fn a_query_asks_for_the_payload_when_any_key_is_the_payload_key((meant, text) in query()) {
    prop_assert_eq!(payload_of(&text), meant.iter().any(|(k, _)| k == "__payload"), "{}", text);
  }

  #[test]
  fn a_query_reads_as_its_last_value_per_key_without_internal_keys((meant, text) in query()) {
    let mut expected = std::collections::BTreeMap::new();
    for (k, v) in &meant {
      if !k.is_empty() && !k.starts_with("__") {
        expected.insert(k.clone(), v.clone().unwrap_or_default());
      }
    }
    let read: std::collections::BTreeMap<_, _> = parse_query(&text).into_iter().collect();
    prop_assert_eq!(read, expected, "{}", text);
  }

  #[test]
  fn a_path_segment_decodes_to_what_it_spells((meant, raw) in segment()) {
    prop_assert_eq!(percent_decoded(&raw), meant, "{}", raw);
  }

  #[test]
  fn a_path_segment_that_decodes_to_invalid_utf8_is_kept_as_sent(prefix in "[a-z]{0,3}", byte in 0x80u8..=0xff) {
    let raw = format!("{prefix}%{byte:02X}");
    prop_assert_eq!(percent_decoded(&raw), raw.clone());
  }

  #[test]
  fn a_mutated_or_cut_query_or_segment_never_panics((_, text) in query(), (_, raw) in segment()) {
    for m in mutations(&text).into_iter().chain(mutations(&raw)) {
      let _ = (fragment_of(&m), payload_of(&m), parse_query(&m), percent_decoded(&m));
    }
  }
}

/// Any text at all: every decoder answers.
#[test]
fn fuzz_any_text_is_decoded() {
  check!().with_type::<String>().for_each(|text| {
    let _ = (fragment_of(text), payload_of(text), parse_query(text), percent_decoded(text));
  });
}

/// Text drawn from the grammar's own alphabet, where the edge cases live.
#[test]
fn fuzz_text_from_the_query_alphabet_is_decoded() {
  const ALPHABET: &[u8] = b"%&=+_afAF09g \xc3\xa9";
  check!().with_type::<Vec<u8>>().for_each(|picks| {
    let bytes: Vec<u8> = picks.iter().map(|p| ALPHABET[*p as usize % ALPHABET.len()]).collect();
    let text = String::from_utf8_lossy(&bytes);
    let _ = (fragment_of(&text), payload_of(&text), parse_query(&text), percent_decoded(&text));
  });
}
