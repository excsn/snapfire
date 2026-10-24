use std::sync::Arc;

use aes_gcm::aead::Aead;
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;

use crate::keyring::Keyring;
use crate::store::SessionRecord;
use crate::SessionId;

/// The most a browser keeps of one cookie, its name and value together (RFC 6265 section 6.1).
pub const COOKIE_LIMIT: usize = 4096;

const LABEL: &[u8] = b"snapfire session cookie";
const VERSION: &str = "v1.";
const NONCE: usize = 12;

/// A session's id and record in the cookie itself, AES-256-GCM under a key derived from each key
/// of a [`Keyring`]: sealed under the current one, opened under any.
pub struct SealedCookie {
  ring: Arc<Keyring>,
}

impl SealedCookie {
  pub fn over(ring: Arc<Keyring>) -> Self {
    Self { ring }
  }

  pub fn keyring(&self) -> &Arc<Keyring> {
    &self.ring
  }

  /// `v1.` and the nonce and ciphertext in unpadded base64url; empty for an empty ring.
  pub fn seal(&self, id: &SessionId, record: &SessionRecord) -> String {
    let Some(key) = self.ring.derive(LABEL).into_iter().next() else {
      return String::new();
    };
    let plain = format!("{}\n{}", id.0, record.encode());
    let nonce: [u8; NONCE] = rand::random();
    let sealed = cipher(&key).encrypt(&Nonce::from(nonce), plain.as_bytes()).expect("AES-GCM seals any length a cookie holds");
    let mut bytes = nonce.to_vec();
    bytes.extend(sealed);
    format!("{VERSION}{}", URL_SAFE_NO_PAD.encode(bytes))
  }

  /// The id, the record and the position of the key that opened it, `0` for the current one;
  /// `None` for a value no key opens.
  pub fn open(&self, value: &str) -> Option<(SessionId, SessionRecord, usize)> {
    let bytes = URL_SAFE_NO_PAD.decode(value.strip_prefix(VERSION)?).ok()?;
    if bytes.len() < NONCE {
      return None;
    }
    let (nonce, sealed) = bytes.split_at(NONCE);
    let nonce = Nonce::from(<[u8; NONCE]>::try_from(nonce).ok()?);
    let (position, plain) = self.ring.derive(LABEL).iter().enumerate().find_map(|(position, key)| cipher(key).decrypt(&nonce, sealed).ok().map(|plain| (position, plain)))?;
    let text = String::from_utf8(plain).ok()?;
    let (id, record) = text.rsplit_once('\n')?;
    Some((SessionId(id.to_owned()), SessionRecord::decode(record, 0)?, position))
  }
}

fn cipher(key: &[u8; 32]) -> Aes256Gcm {
  Aes256Gcm::new_from_slice(key).expect("a 32 byte key")
}
