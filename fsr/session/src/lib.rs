mod codec;
mod sessions;
mod store;
mod tokens;

pub use codec::{CookieCodec, HmacCodec};
pub use sessions::{Opened, SessionConfig, Sessions};
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
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn from_hex(s: &str) -> Option<Vec<u8>> {
  // Over bytes rather than `&s[i..i + 2]`: the value is whatever a browser
  // sent, and slicing a string by byte index panics when the index lands
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
