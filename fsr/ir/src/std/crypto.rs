//! `crypto`: SHA-256 as lowercase hex, its verification and random bytes.

use sha2::{Digest, Sha256};
use snapfire_fsr_core::Value;

use crate::ext::{number, text, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("crypto.hash", Reach::Render, |_, args| Ok(Value::str(hash(text("crypto.hash", args, 0)?))));
  extensions.register("crypto.verify", Reach::Render, verify);
  extensions.register("crypto.random", Reach::Body, random);
  extensions.register("crypto.uuid", Reach::Body, |_, _| Ok(Value::str(uuid::Uuid::new_v4().to_string())));
  extensions.register("crypto.digest", Reach::Render, digest);
}

/// `crypto.digest(algorithm, encoding, parts…)`: the hash of the parts' bytes in order. `sha256` is the algorithm provided; the encoding is `hex` or `base64`.
fn digest(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "crypto.digest";
  let algorithm = text(what, args, 0)?;
  if algorithm.to_ascii_lowercase().replace('-', "") != "sha256" {
    return Err(Fail::internal(format!("{what}: `{algorithm}` is not provided; `sha256` is")));
  }
  let encoding = text(what, args, 1)?.to_owned();
  let mut hasher = Sha256::new();
  for part in args.iter().skip(2) {
    match part {
      Value::Bytes(bytes) => hasher.update(bytes),
      other => hasher.update(crate::interp::stringify(other)?.as_bytes()),
    }
  }
  encode(what, &hasher.finalize(), &encoding)
}

/// `bytes` as `encoding` names them: `hex`, `base64` or `utf8`.
pub fn encode(what: &str, bytes: &[u8], encoding: &str) -> Result<Value, Fail> {
  Ok(match encoding.to_ascii_lowercase().as_str() {
    "hex" => Value::str(hex(bytes)),
    "base64" => Value::str(base64(bytes)),
    "utf8" | "utf-8" => Value::str(String::from_utf8_lossy(bytes).into_owned()),
    other => return Err(Fail::internal(format!("{what}: an encoding of `{other}`; `hex`, `base64` and `utf8` are read"))),
  })
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
  const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
  let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
  for chunk in bytes.chunks(3) {
    let n = chunk.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b)) << (8 * (3 - chunk.len()));
    for i in 0..4 {
      if i <= chunk.len() {
        out.push(TABLE[((n >> (18 - 6 * i)) & 63) as usize] as char);
      } else {
        out.push('=');
      }
    }
  }
  out
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

/// `crypto.random(bytes, encoding?)`: that many random bytes as `hex`, the default, or `base64`.
fn random(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "crypto.random";
  let n = number(what, args, 0)?.clamp(0.0, 1024.0) as usize;
  let mut buf = vec![0u8; n];
  getrandom::fill(&mut buf).map_err(|e| Fail::internal(format!("{what}: {e}")))?;
  match args.get(1) {
    None | Some(Value::Null | Value::Undefined) => Ok(Value::str(hex(&buf))),
    Some(_) => match text(what, args, 1)? {
      encoding @ ("hex" | "base64") => encode(what, &buf, encoding),
      other => Err(Fail::internal(format!("{what}: an encoding of `{other}`; `hex` and `base64` are read"))),
    },
  }
}
