mod codec;
mod sessions;
mod store;

pub use codec::{CookieCodec, HmacCodec};
pub use sessions::{Opened, SessionConfig, Sessions};
pub use store::{MemorySessionStore, SessionRecord, SessionStore};

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
  if s.len() % 2 != 0 {
    return None;
  }
  (0..s.len())
    .step_by(2)
    .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
    .collect()
}
