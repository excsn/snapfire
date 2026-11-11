//! Plain JSON, the JSON anything outside FSR reads and writes: what JavaScript's `JSON.stringify` writes for a value and what a service's JSON reads as. Nothing is tagged, so a key named `$` is a key. The payload encoding, which round-trips every value between the host and its own client, is `snapfire_fsr_payload`.

use crate::value::typed;
use crate::{TypedArray, Value, ValueMap};

/// What an integer, JavaScript's `BigInt`, is written as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Integers {
  /// Refused, as `JSON.stringify` refuses a `BigInt` inside a body.
  Refuse,
  /// Its digits, at a boundary where a service or a client reads the number.
  Digits,
}

/// A value plain JSON has no form for under the rule given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unwritable(pub String);

impl std::fmt::Display for Unwritable {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for Unwritable {}

/// `value` as `JSON.stringify(value, null, indent)` writes it, an integer as `integers` says. `undefined` and a function are left out of an object and written `null` in an array or alone; a non-finite number is `null`; a `Date` is its ISO string, an invalid one `null`; a `Map` and a `Set` are `{}`; a `URL` is its href; bytes and a typed array are objects keyed by index.
pub fn stringify(value: &Value, indent: &str, integers: Integers) -> Result<String, Unwritable> {
  let mut out = String::new();
  write(&mut out, value, indent, 0, integers)?;
  Ok(out)
}

/// `stringify` with no indent and integers as their digits, parsed back for an API that takes `serde_json::Value`.
pub fn to_json(value: &Value) -> Result<serde_json::Value, Unwritable> {
  serde_json::from_str(&stringify(value, "", Integers::Digits)?).map_err(|e| Unwritable(e.to_string()))
}

/// A JSON document a service or a client sent as a value: an integral number an integer, any other number a double, an object a map whatever its keys.
pub fn to_value(json: &serde_json::Value) -> Value {
  match json {
    serde_json::Value::Null => Value::Null,
    serde_json::Value::Bool(b) => Value::Bool(*b),
    serde_json::Value::Number(n) => match (n.as_i64(), n.as_u64()) {
      (Some(i), _) => Value::Int(i.into()),
      (None, Some(u)) => Value::Int(u.into()),
      _ => Value::F64(n.as_f64().unwrap_or(f64::NAN)),
    },
    serde_json::Value::String(s) => Value::str(s.as_str()),
    serde_json::Value::Array(items) => Value::seq(items.iter().map(to_value).collect::<Vec<Value>>()),
    serde_json::Value::Object(fields) => Value::Map(fields.iter().map(|(k, v)| (k.clone(), to_value(v))).collect::<ValueMap>()),
  }
}

/// `Number.prototype.toString()` of a double: the shortest digits that read back as it, fixed between 1e-7 and 1e21 and exponential outside.
pub fn number_text(n: f64) -> String {
  let mut out = String::new();
  write_number(&mut out, n);
  out
}

/// [`number_text`] appended to `out`.
pub fn write_number(out: &mut String, n: f64) {
  use std::fmt::Write as _;
  if n.is_nan() {
    return out.push_str("NaN");
  }
  if n == 0.0 {
    return out.push('0');
  }
  if n.is_infinite() {
    return out.push_str(if n > 0.0 { "Infinity" } else { "-Infinity" });
  }
  // Below 2^53 every whole double is exact and its shortest digits are its own.
  if n.fract() == 0.0 && n.abs() < 9_007_199_254_740_992.0 {
    let _ = write!(out, "{}", n as i64);
    return;
  }
  if n < 0.0 {
    out.push('-');
  }
  let a = n.abs();
  let mut shortest = Digits::default();
  let _ = write!(shortest, "{a:e}");
  let length = shortest.mantissa().bytes().filter(|b| *b != b'.').count();
  let mut closest = Digits::default();
  let _ = write!(closest, "{:.*e}", length - 1, a);
  let scientific = if closest.as_str().parse::<f64>() == Ok(a) { &closest } else { &shortest };
  let mut digits = Digits::default();
  for c in scientific.mantissa().chars().filter(|c| *c != '.') {
    let _ = digits.write_char(c);
  }
  let digits = digits.as_str();
  let k = digits.len() as i64;
  let point = scientific.exponent() + 1;
  let zeros = |out: &mut String, count: i64| out.extend(std::iter::repeat_n('0', count as usize));
  if k <= point && point <= 21 {
    out.push_str(digits);
    zeros(out, point - k);
  } else if 0 < point && point <= 21 {
    out.push_str(&digits[..point as usize]);
    out.push('.');
    out.push_str(&digits[point as usize..]);
  } else if -6 < point && point <= 0 {
    out.push_str("0.");
    zeros(out, -point);
    out.push_str(digits);
  } else {
    out.push_str(&digits[..1]);
    if k > 1 {
      out.push('.');
      out.push_str(&digits[1..]);
    }
    let e = point - 1;
    let _ = write!(out, "e{}{}", if e < 0 { '-' } else { '+' }, e.abs());
  }
}

/// A double's `{:e}` text on the stack: at most 17 digits, a point, a sign and a four-digit exponent.
struct Digits {
  bytes: [u8; 40],
  len: usize,
}

impl Default for Digits {
  fn default() -> Self {
    Self { bytes: [0; 40], len: 0 }
  }
}

impl Digits {
  fn as_str(&self) -> &str {
    std::str::from_utf8(&self.bytes[..self.len]).expect("formatting writes UTF-8")
  }

  fn mantissa(&self) -> &str {
    self.as_str().split_once('e').expect("`{:e}` writes an exponent").0
  }

  fn exponent(&self) -> i64 {
    self.as_str().split_once('e').expect("`{:e}` writes an exponent").1.parse().expect("an integer exponent")
  }
}

impl std::fmt::Write for Digits {
  fn write_str(&mut self, s: &str) -> std::fmt::Result {
    let end = self.len + s.len();
    if end > self.bytes.len() {
      return Err(std::fmt::Error);
    }
    self.bytes[self.len..end].copy_from_slice(s.as_bytes());
    self.len = end;
    Ok(())
  }
}

/// Whether a value is left out of an object, as `undefined` and a function are.
fn skipped(value: &Value) -> bool {
  matches!(value, Value::Undefined | Value::Ref { .. }) || matches!(value, Value::Variant { tag, .. } if tag == typed::SYMBOL)
}

fn write(out: &mut String, value: &Value, indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  match value {
    Value::Null | Value::Undefined | Value::Ref { .. } => out.push_str("null"),
    Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
    Value::F64(f) => float(out, *f),
    Value::F32(f) => float(out, f64::from(*f)),
    Value::Int(n) => integer(out, n, integers)?,
    Value::UInt(n) => integer(out, n, integers)?,
    Value::Str(s) => string(out, s),
    Value::Seq(items) if items.kind() == crate::value::Collection::Set => out.push_str("{}"),
    Value::Map(map) if map.kind() == crate::value::Collection::Map => out.push_str("{}"),
    Value::Map(map) if map.get(typed::REGEX).is_some() => out.push_str("{}"),
    Value::Seq(items) => crate::on_enough_stack(|| array(out, items.iter(), indent, depth, integers))?,
    Value::Map(map) => crate::on_enough_stack(|| object(out, map.iter().map(|(k, v)| (k.as_str(), v)), indent, depth, integers))?,
    Value::Bytes(bytes) => indexed(out, bytes.iter().map(|b| Value::Int((*b).into())).collect(), indent, depth, integers)?,
    Value::TypedArray(array) => indexed(out, elements(array), indent, depth, integers)?,
    Value::Variant { tag, payload } => match (tag.as_str(), payload.as_deref()) {
      (typed::DATE, Some(ms)) => match millis(ms).and_then(crate::time::iso_string) {
        Some(iso) => string(out, &iso),
        None => out.push_str("null"),
      },
      (typed::MAP | typed::SET, _) => out.push_str("{}"),
      (typed::SYMBOL, _) => out.push_str("null"),
      (typed::RAW_JSON, Some(Value::Str(text))) => out.push_str(text),
      (typed::URL, Some(href)) => write(out, href, indent, depth, integers)?,
      (typed::TEMPORAL, _) if typed::temporal(value).is_some() => string(out, typed::temporal(value).map_or("", |(_, text)| text)),
      (typed::BIGINT, Some(n)) => match n {
        Value::Str(digits) => integer(out, digits.as_str(), integers)?,
        other => write(out, other, indent, depth, integers)?,
      },
      (_, payload) => {
        let tag_value = Value::str(tag.as_str());
        object(out, std::iter::once(("tag", &tag_value)).chain(payload.map(|payload| ("payload", payload))), indent, depth, integers)?;
      }
    },
  }
  Ok(())
}

fn float(out: &mut String, f: f64) {
  if f.is_finite() {
    write_number(out, f);
  } else {
    out.push_str("null");
  }
}

fn integer(out: &mut String, digits: impl std::fmt::Display, integers: Integers) -> Result<(), Unwritable> {
  use std::fmt::Write as _;
  match integers {
    Integers::Digits => {
      let _ = write!(out, "{digits}");
      Ok(())
    }
    Integers::Refuse => Err(Unwritable("JSON.stringify of a BigInt, which JavaScript throws on".to_owned())),
  }
}

fn millis(value: &Value) -> Option<f64> {
  match value {
    Value::F64(f) => Some(*f),
    Value::F32(f) => Some(f64::from(*f)),
    Value::Int(n) => Some(*n as f64),
    Value::UInt(n) => Some(*n as f64),
    _ => None,
  }
}

fn elements(array: &TypedArray) -> Vec<Value> {
  match array {
    TypedArray::I8(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::U8(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::I16(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::U16(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::I32(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::U32(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::I64(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::U64(v) => v.iter().map(|n| Value::Int((*n).into())).collect(),
    TypedArray::F32(v) => v.iter().map(|n| Value::F64((*n).into())).collect(),
    TypedArray::F64(v) => v.iter().map(|n| Value::F64(*n)).collect(),
  }
}

fn indexed(out: &mut String, items: Vec<Value>, indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  let keys: Vec<String> = (0..items.len()).map(|i| i.to_string()).collect();
  object(out, keys.iter().map(String::as_str).zip(&items), indent, depth, integers)
}

fn newline(out: &mut String, indent: &str, depth: usize) {
  if !indent.is_empty() {
    out.push('\n');
    for _ in 0..depth {
      out.push_str(indent);
    }
  }
}

fn array<'v>(out: &mut String, items: impl ExactSizeIterator<Item = &'v Value>, indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  if items.len() == 0 {
    out.push_str("[]");
    return Ok(());
  }
  out.push('[');
  for (i, item) in items.enumerate() {
    if i > 0 {
      out.push(',');
    }
    newline(out, indent, depth + 1);
    write(out, item, indent, depth + 1, integers)?;
  }
  newline(out, indent, depth);
  out.push(']');
  Ok(())
}

/// The fields `skipped` leaves in, as `{}` when none is.
fn object<'v>(out: &mut String, fields: impl Iterator<Item = (&'v str, &'v Value)>, indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  let mut written = 0;
  for (key, item) in fields.filter(|(_, v)| !skipped(v)) {
    out.push(if written == 0 { '{' } else { ',' });
    written += 1;
    newline(out, indent, depth + 1);
    string(out, key);
    out.push(':');
    if !indent.is_empty() {
      out.push(' ');
    }
    write(out, item, indent, depth + 1, integers)?;
  }
  if written == 0 {
    out.push_str("{}");
    return Ok(());
  }
  newline(out, indent, depth);
  out.push('}');
  Ok(())
}

fn string(out: &mut String, s: &str) {
  out.push('"');
  if !s.bytes().any(|b| b < 0x20 || b == b'"' || b == b'\\') {
    out.push_str(s);
    out.push('"');
    return;
  }
  for c in s.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\u{8}' => out.push_str("\\b"),
      '\u{c}' => out.push_str("\\f"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
      c => out.push(c),
    }
  }
  out.push('"');
}
