use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::{from_hex, to_hex, SessionId};

type HmacSha256 = Hmac<Sha256>;

pub trait CookieCodec: Send + Sync {
  fn encode(&self, id: &SessionId) -> String;
  fn decode(&self, value: &str) -> Option<SessionId>;
}

/// `{id}.{hex hmac}`. Verification is constant-time through the mac itself.
pub struct HmacCodec {
  key: Vec<u8>,
}

impl HmacCodec {
  pub fn new(key: &[u8]) -> Self {
    Self { key: key.to_vec() }
  }

  fn mac_for(&self, input: &[u8]) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(&self.key).expect("hmac accepts any key length");
    mac.update(input);
    mac
  }

  pub(crate) fn sign(&self, input: &[u8]) -> String {
    to_hex(&self.mac_for(input).finalize().into_bytes())
  }

  pub(crate) fn verify(&self, input: &[u8], signature_hex: &str) -> bool {
    let Some(signature) = from_hex(signature_hex) else { return false };
    self.mac_for(input).verify_slice(&signature).is_ok()
  }
}

impl CookieCodec for HmacCodec {
  fn encode(&self, id: &SessionId) -> String {
    format!("{}.{}", id.0, self.sign(id.0.as_bytes()))
  }

  fn decode(&self, value: &str) -> Option<SessionId> {
    let (id, signature) = value.split_once('.')?;
    if id.is_empty() || !self.verify(id.as_bytes(), signature) {
      return None;
    }
    Some(SessionId(id.to_owned()))
  }
}
