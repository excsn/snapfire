//! `crypto`: SHA-256 as lowercase hex, its verification and random bytes.

use sha2::{Digest, Sha256};
use snapfire_fsr_core::Value;

use crate::ext::{number, text, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("crypto.hash", Reach::Render, |_, args| Ok(Value::Str(hash(text("crypto.hash", args, 0)?))));
  extensions.register("crypto.verify", Reach::Render, verify);
  extensions.register("crypto.random", Reach::Body, random);
}

pub fn hash(s: &str) -> String {
  hex(&Sha256::digest(s.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
  let mut out = String::with_capacity(bytes.len() * 2);
  for b in bytes {
    out.push_str(&format!("{b:02x}"));
  }
  out
}

/// `crypto.verify(text, hash)`: whether `hash` is the hash of `text`, compared
/// in constant time over the hash's length.
fn verify(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "crypto.verify";
  let computed = hash(text(what, args, 0)?);
  let given = text(what, args, 1)?.to_ascii_lowercase();
  let mut diff = computed.len() ^ given.len();
  for (a, b) in computed.bytes().zip(given.bytes()) {
    diff |= (a ^ b) as usize;
  }
  Ok(Value::Bool(diff == 0))
}

/// `crypto.random(bytes)`: that many random bytes as hex.
fn random(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "crypto.random";
  let n = number(what, args, 0)?.clamp(0.0, 1024.0) as usize;
  let mut buf = vec![0u8; n];
  getrandom::fill(&mut buf).map_err(|e| Fail::internal(format!("{what}: {e}")))?;
  Ok(Value::Str(hex(&buf)))
}
