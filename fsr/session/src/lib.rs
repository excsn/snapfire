mod codec;
mod csrf;
mod keyring;
mod sealed;
mod sessions;
mod store;
mod tokens;

pub use codec::{CookieCodec, HmacCodec};
pub use csrf::{constant_time_eq, random_token, CsrfScheme, Derived, PerSession, SingleUse};
pub use keyring::Keyring;
pub use sealed::{SealedCookie, COOKIE_LIMIT};
pub use sessions::{DEFAULT_MAX_BYTES, Opened, STATE_COOKIE, SessionConfig, Sessions};
pub use store::{MemorySessionStore, SessionRecord, SessionStore, StoreError};
pub use tokens::TokenCell;

use std::fmt;

/// Random, opaque and meaningless off-box. The cookie carries this signed,
/// never session data and never a backend credential.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
  pub fn generate() -> Self {
    let bytes: [u8; 16] = rand::random();
    Self(to_hex(&bytes))
  }
}

impl fmt::Display for SessionId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.0)
  }
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
  const DIGITS: &[u8; 16] = b"0123456789abcdef";
  let mut out = String::with_capacity(bytes.len() * 2);
  for b in bytes {
    out.push(DIGITS[usize::from(b >> 4)] as char);
    out.push(DIGITS[usize::from(b & 0x0f)] as char);
  }
  out
}

pub(crate) fn from_hex(s: &str) -> Option<Vec<u8>> {
  // Over bytes rather than `&s[i..i + 2]`: the value is whatever a browser
  // sent and slicing a string by byte index panics when the index lands
  // inside a character.
  let bytes = s.as_bytes();
  if bytes.len() % 2 != 0 {
    return None;
  }
  bytes
    .chunks_exact(2)
    .map(|pair| {
      let hi = (pair[0] as char).to_digit(16)?;
      let lo = (pair[1] as char).to_digit(16)?;
      Some((hi * 16 + lo) as u8)
    })
    .collect()
}

#[cfg(test)]
mod hex_tests {
  use super::{from_hex, to_hex};

  #[test]
  fn bytes_are_written_as_lowercase_hex_two_digits_each_and_read_back() {
    let bytes = [0x00, 0x0f, 0xa5, 0xff, 0x10];
    assert_eq!(to_hex(&bytes), "000fa5ff10");
    assert_eq!(from_hex(&to_hex(&bytes)).as_deref(), Some(&bytes[..]));
    assert_eq!(to_hex(&[]), "");
  }
}
