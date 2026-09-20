use hmac::{Hmac, Mac};
use parking_lot::RwLock;
use sha2::Sha256;

use crate::{from_hex, to_hex};

type HmacSha256 = Hmac<Sha256>;

/// An ordered set of signing keys: the first signs, every one verifies. A
/// rotation puts a new key in front and keeps the old ones verifying until
/// they are retired, so a value signed before the rotation is still good
/// through the grace period. Shared and changed in place, so nothing that
/// holds it is rebuilt by a rotation.
pub struct Keyring {
  keys: RwLock<Vec<Vec<u8>>>,
}

impl Keyring {
  pub fn new(key: &[u8]) -> Self {
    Self { keys: RwLock::new(vec![key.to_vec()]) }
  }

  /// The first key is current; the rest verify only. An empty list is a ring
  /// that signs nothing and verifies nothing.
  pub fn from_keys<I, K>(keys: I) -> Self
  where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
  {
    Self { keys: RwLock::new(keys.into_iter().map(|k| k.as_ref().to_vec()).collect()) }
  }

  /// A new current key; the one it replaces stays until `retire`. A key
  /// already in the ring is moved to the front rather than held twice.
  pub fn rotate(&self, key: &[u8]) {
    let mut keys = self.keys.write();
    keys.retain(|k| k != key);
    keys.insert(0, key.to_vec());
  }

  /// Drops a key; a value signed under it no longer verifies. Retiring the
  /// current key leaves the next one signing.
  pub fn retire(&self, key: &[u8]) {
    self.keys.write().retain(|k| k != key);
  }

  /// The whole list as `from_keys` would take it, for a configuration read
  /// again.
  pub fn replace<I, K>(&self, keys: I)
  where
    I: IntoIterator<Item = K>,
    K: AsRef<[u8]>,
  {
    *self.keys.write() = keys.into_iter().map(|k| k.as_ref().to_vec()).collect();
  }

  pub fn len(&self) -> usize {
    self.keys.read().len()
  }

  pub fn is_empty(&self) -> bool {
    self.keys.read().is_empty()
  }

  /// Whether `key` is the one signing now.
  pub fn is_current(&self, key: &[u8]) -> bool {
    self.keys.read().first().is_some_and(|k| k == key)
  }

  /// The hex hmac of `input` under the current key; empty for an empty ring.
  pub fn sign(&self, input: &[u8]) -> String {
    match self.keys.read().first() {
      Some(key) => to_hex(&mac(key, input).finalize().into_bytes()),
      None => String::new(),
    }
  }

  /// The position of the key `signature_hex` verifies under, `0` for the
  /// current one; `None` when no key does. Constant time through the mac.
  pub fn verify(&self, input: &[u8], signature_hex: &str) -> Option<usize> {
    let signature = from_hex(signature_hex)?;
    self.keys.read().iter().position(|key| mac(key, input).verify_slice(&signature).is_ok())
  }
}

impl std::fmt::Debug for Keyring {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Keyring").field("keys", &self.len()).finish()
  }
}

fn mac(key: &[u8], input: &[u8]) -> HmacSha256 {
  let mut mac = HmacSha256::new_from_slice(key).expect("hmac accepts any key length");
  mac.update(input);
  mac
}
