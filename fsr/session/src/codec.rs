use std::sync::Arc;

use crate::keyring::Keyring;
use crate::SessionId;

pub trait CookieCodec: Send + Sync {
  fn encode(&self, id: &SessionId) -> String;
  fn decode(&self, value: &str) -> Option<SessionId>;
  /// Whether `value`, which `decode` accepted, is what `encode` writes now.
  /// `false` for a value signed under a key that is no longer current, which
  /// has the layer set the cookie again so it moves before that key is
  /// retired. A codec that never rotates leaves the default.
  fn current(&self, value: &str) -> bool {
    let _ = value;
    true
  }
}

/// `{id}.{hex hmac}` under a `Keyring`: signed by its current key, verified
/// by any of them.
pub struct HmacCodec {
  ring: Arc<Keyring>,
}

impl HmacCodec {
  /// A ring of one key.
  pub fn new(key: &[u8]) -> Self {
    Self::over(Arc::new(Keyring::new(key)))
  }

  /// Over a ring the caller holds and rotates.
  pub fn over(ring: Arc<Keyring>) -> Self {
    Self { ring }
  }

  pub fn keyring(&self) -> &Arc<Keyring> {
    &self.ring
  }

  pub(crate) fn sign(&self, input: &[u8]) -> String {
    self.ring.sign(input)
  }

  pub(crate) fn verify(&self, input: &[u8], signature_hex: &str) -> bool {
    self.ring.verify(input, signature_hex).is_some()
  }

  fn split(value: &str) -> Option<(&str, &str)> {
    // The signature is hex and carries no `.`, so the last one separates
    // them: an id that holds a dot still reads back.
    let (id, signature) = value.rsplit_once('.')?;
    (!id.is_empty()).then_some((id, signature))
  }
}

impl CookieCodec for HmacCodec {
  fn encode(&self, id: &SessionId) -> String {
    format!("{}.{}", id.0, self.sign(id.0.as_bytes()))
  }

  fn decode(&self, value: &str) -> Option<SessionId> {
    let (id, signature) = Self::split(value)?;
    self.verify(id.as_bytes(), signature).then(|| SessionId(id.to_owned()))
  }

  fn current(&self, value: &str) -> bool {
    Self::split(value).and_then(|(id, signature)| self.ring.verify(id.as_bytes(), signature)) == Some(0)
  }
}
