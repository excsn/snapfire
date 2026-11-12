//! `encoding`: what the lowerer writes for `btoa`, `atob`, `new TextEncoder().encode(s)` and `new TextDecoder().decode(bytes)`. `encode` answers a `Uint8Array`; `decode` takes one or an array of numbers from 0 to 255.

use snapfire_fsr_core::Value;

use crate::ext::{text, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("encoding.btoa", Reach::Render, |_, args| btoa(&text("btoa", args, 0)?).map(Value::str));
  extensions.register("encoding.atob", Reach::Render, |_, args| atob(&text("atob", args, 0)?).map(Value::str));
  extensions.register("encoding.encode", Reach::Render, |_, args| {
    let bytes: Vec<Value> = text("TextEncoder.encode", args, 0)?.bytes().map(|b| Value::F64(f64::from(b))).collect();
    crate::interp::typed_from("Uint8", &bytes)
  });
  extensions.register("encoding.decode", Reach::Render, |_, args| decode(args.first()));
  extensions.register("encoding.toBase64", Reach::Render, |_, args| {
    let bytes = byte_list("Uint8Array.prototype.toBase64", args.first())?;
    let (url, bare) = (option(args.get(1), "alphabet").as_deref() == Some("base64url"), args.get(1).is_some_and(|o| matches!(o, Value::Map(m) if matches!(m.get("omitPadding"), Some(Value::Bool(true))))));
    let mut text = super::crypto::base64(&bytes);
    if url {
      text = text.replace('+', "-").replace('/', "_");
    }
    if bare {
      text = text.trim_end_matches('=').to_owned();
    }
    Ok(Value::str(text))
  });
  extensions.register("encoding.toHex", Reach::Render, |_, args| {
    let bytes = byte_list("Uint8Array.prototype.toHex", args.first())?;
    Ok(Value::str(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()))
  });
  extensions.register("encoding.fromBase64", Reach::Render, |_, args| {
    let given = text("Uint8Array.fromBase64", args, 0)?;
    let standard = if option(args.get(1), "alphabet").as_deref() == Some("base64url") { given.replace('-', "+").replace('_', "/") } else { given.to_owned() };
    let decoded = atob(&standard).map_err(|_| Fail::internal("SyntaxError: Uint8Array.fromBase64: not base64"))?;
    crate::interp::typed_from("Uint8", &decoded.chars().map(|c| Value::F64(f64::from(u32::from(c)))).collect::<Vec<_>>())
  });
  extensions.register("encoding.fromHex", Reach::Render, |_, args| {
    let given = text("Uint8Array.fromHex", args, 0)?;
    let invalid = || Fail::internal("SyntaxError: Uint8Array.fromHex: not hex");
    if given.len() % 2 != 0 {
      return Err(invalid());
    }
    let bytes = (0..given.len()).step_by(2).map(|at| given.get(at..at + 2).and_then(|pair| u8::from_str_radix(pair, 16).ok()).map(|b| Value::F64(f64::from(b))).ok_or_else(invalid)).collect::<Result<Vec<_>, _>>()?;
    crate::interp::typed_from("Uint8", &bytes)
  });
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

/// A string option of `options`.
fn option(options: Option<&Value>, key: &str) -> Option<String> {
  match options {
    Some(Value::Map(map)) => match map.get(key) {
      Some(Value::Str(s)) => Some(s.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// The bytes of a `Uint8Array` or of an array of numbers from 0 to 255.
fn byte_list(what: &str, bytes: Option<&Value>) -> Result<Vec<u8>, Fail> {
  match bytes.cloned().map(crate::interp::as_items) {
    Some(Value::Seq(items)) => items.iter().map(|b| match b {
      Value::F64(n) => Ok(*n as u8),
      Value::Int(n) => Ok(*n as u8),
      other => Err(Fail::internal(format!("TypeError: {what}: a byte wants a number, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    }).collect(),
    other => Err(Fail::internal(format!("TypeError: {what}: wants a Uint8Array, got {}", other.as_ref().map_or("nothing", snapfire_fsr_core::ext::kind_name)))),
  }
}

/// The UTF-8 text of `bytes`, a malformed sequence as U+FFFD and a leading byte order mark dropped, as `new TextDecoder().decode(bytes)` answers.
fn decode(bytes: Option<&Value>) -> Result<Value, Fail> {
  let what = "TextDecoder.decode";
  let raw: Vec<u8> = match bytes {
    None | Some(Value::Null | Value::Undefined) => Vec::new(),
    Some(Value::TypedArray(snapfire_fsr_core::TypedArray::U8(bytes)) | Value::Bytes(bytes)) => bytes.clone(),
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
