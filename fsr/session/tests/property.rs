//! The cookie under generated input. Its value is whatever a browser sends,
//! so a decoder that panics is a request that kills the worker, and one that
//! accepts a forged value is a session anyone can take.

use proptest::prelude::*;
use snapfire_fsr_session::{CookieCodec, HmacCodec, SessionId};

fn ids() -> impl Strategy<Value = String> {
  prop_oneof![
    3 => "[a-zA-Z0-9_-]{1,40}",
    1 => "(?s).{1,20}",
    1 => prop_oneof![
      Just("a.b".to_owned()),
      Just(".".to_owned()),
      Just("..".to_owned()),
      Just("é🌍".to_owned()),
      Just(" ".to_owned()),
      Just("\u{0}".to_owned()),
    ],
  ]
}

proptest! {
  #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

  /// A cookie this key wrote reads back as the id it names.
  #[test]
  fn a_signed_cookie_reads_back(id in ids(), key in prop::collection::vec(any::<u8>(), 0..40)) {
    let codec = HmacCodec::new(&key);
    let cookie = codec.encode(&SessionId(id.clone()));
    prop_assert_eq!(codec.decode(&cookie), Some(SessionId(id)), "cookie: {}", cookie);
  }

  /// Whatever a browser sends, decoding answers or refuses.
  #[test]
  fn decoding_arbitrary_text_never_panics(value in "(?s).{0,120}", key in prop::collection::vec(any::<u8>(), 0..40)) {
    let _ = HmacCodec::new(&key).decode(&value);
  }

  /// One byte changed anywhere in the cookie makes it unusable. Hex is
  /// case-insensitive, so a flip of case is the same signature rather than a
  /// forgery and is not a case here.
  #[test]
  fn a_tampered_cookie_is_refused(
    id in "[a-zA-Z0-9_-]{1,32}",
    key in prop::collection::vec(any::<u8>(), 1..40),
    at in 0usize..4096,
    byte in any::<u8>(),
  ) {
    let codec = HmacCodec::new(&key);
    let cookie = codec.encode(&SessionId(id.clone()));
    let mut bytes = cookie.clone().into_bytes();
    let at = at % bytes.len();
    prop_assume!(bytes[at] != byte);
    bytes[at] = byte;
    let Ok(tampered) = String::from_utf8(bytes) else { return Ok(()) };
    prop_assume!(tampered.to_ascii_lowercase() != cookie.to_ascii_lowercase());
    prop_assert_ne!(codec.decode(&tampered), Some(SessionId(id)), "forged: {}", tampered);
  }

  /// A cookie one key wrote is worthless under another.
  #[test]
  fn a_cookie_does_not_cross_keys(
    id in "[a-zA-Z0-9_-]{1,32}",
    a in prop::collection::vec(any::<u8>(), 1..32),
    b in prop::collection::vec(any::<u8>(), 1..32),
  ) {
    prop_assume!(a != b);
    let cookie = HmacCodec::new(&a).encode(&SessionId(id));
    prop_assert_eq!(HmacCodec::new(&b).decode(&cookie), None, "crossed: {}", cookie);
  }
}

/// The exact values that used to crash. `from_hex` sliced the string by byte
/// index, so any multi-byte character in a cookie or a CSRF token split a
/// character and panicked the worker answering the request.
#[test]
fn a_cookie_holding_a_multibyte_character_is_refused_not_fatal() {
  let codec = HmacCodec::new(b"k");
  for value in [" .𐀀", "a.é", "a.🌍🌍", "a.\u{0}\u{1}", "..", "a.ÿÿ"] {
    assert_eq!(codec.decode(value), None, "{value:?}");
  }
}

/// An id holding a dot still reads back: the signature is hex and carries none,
/// so the last dot is the separator.
#[test]
fn an_id_holding_a_dot_reads_back() {
  let codec = HmacCodec::new(b"k");
  for id in ["a.b", ".", "a.b.c", "..."] {
    let cookie = codec.encode(&SessionId(id.to_owned()));
    assert_eq!(codec.decode(&cookie), Some(SessionId(id.to_owned())), "{cookie}");
  }
}
