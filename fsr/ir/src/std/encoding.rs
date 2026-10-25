//! `encoding`: what the lowerer writes for `btoa`, `atob`, `new TextEncoder().encode(s)` and `new TextDecoder().decode(bytes)`. Bytes are an array of numbers from 0 to 255.

use snapfire_fsr_core::Value;

use crate::ext::{text, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("encoding.btoa", Reach::Render, |_, args| btoa(&text("btoa", args, 0)?).map(Value::str));
  extensions.register("encoding.atob", Reach::Render, |_, args| atob(&text("atob", args, 0)?).map(Value::str));
  extensions.register("encoding.encode", Reach::Render, |_, args| Ok(Value::Seq(text("TextEncoder.encode", args, 0)?.bytes().map(Value::int).collect())));
  extensions.register("encoding.decode", Reach::Render, |_, args| decode(args.first()));
}

/// The base64 of `s`'s code units, each of which must be at most U+00FF, as the browser's `btoa` answers.
pub fn btoa(s: &str) -> Result<String, Fail> {
  let mut bytes = Vec::with_capacity(s.len());
  for c in s.chars() {
    let unit = u32::from(c);
    if unit > 0xFF {
      return Err(Fail::internal(format!("btoa: `{c}` is outside Latin-1; encode the text as UTF-8 with `new TextEncoder().encode(s)` first")));
    }
    bytes.push(unit as u8);
  }
  Ok(super::crypto::base64(&bytes))
}

/// The forgiving base64 decode of the WHATWG infra standard, each byte as one character, as the browser's `atob` answers.
pub fn atob(s: &str) -> Result<String, Fail> {
  let mut data: Vec<u8> = s.bytes().filter(|b| !matches!(b, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')).collect();
  if data.len() % 4 == 0 {
    for _ in 0..2 {
      if data.last() == Some(&b'=') {
        data.pop();
      }
    }
  }
  let invalid = || Fail::internal(format!("atob: `{s}` is not base64"));
  if data.len() % 4 == 1 {
    return Err(invalid());
  }
  let mut out = String::with_capacity(data.len() * 3 / 4);
  let (mut buffer, mut bits) = (0u32, 0u32);
  for b in data {
    let value = match b {
      b'A'..=b'Z' => b - b'A',
      b'a'..=b'z' => b - b'a' + 26,
      b'0'..=b'9' => b - b'0' + 52,
      b'+' => 62,
      b'/' => 63,
      _ => return Err(invalid()),
    };
    buffer = (buffer << 6) | u32::from(value);
    bits += 6;
    if bits >= 8 {
      bits -= 8;
      out.push(char::from(((buffer >> bits) & 0xFF) as u8));
    }
  }
  Ok(out)
}

/// The UTF-8 text of `bytes`, a malformed sequence as U+FFFD and a leading byte order mark dropped, as `new TextDecoder().decode(bytes)` answers.
fn decode(bytes: Option<&Value>) -> Result<Value, Fail> {
  let what = "TextDecoder.decode";
  let raw: Vec<u8> = match bytes {
    None | Some(Value::Null | Value::Undefined) => Vec::new(),
    Some(Value::Seq(items)) => items
      .iter()
      .map(|item| match item {
        Value::Int(n) => Ok(*n as u8),
        Value::UInt(n) => Ok(*n as u8),
        Value::F64(f) => Ok(*f as u8),
        other => Err(Fail::internal(format!("{what}: each byte wants a number, got {}", snapfire_fsr_core::ext::kind_name(other)))),
      })
      .collect::<Result<_, _>>()?,
    Some(other) => return Err(Fail::internal(format!("{what}: wants the bytes `TextEncoder.encode` made, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  };
  let text = String::from_utf8_lossy(&raw);
  Ok(Value::str(text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()))
}
