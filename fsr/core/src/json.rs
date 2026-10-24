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
  if n.is_nan() {
    return "NaN".to_owned();
  }
  if n == 0.0 {
    return "0".to_owned();
  }
  if n.is_infinite() {
    return if n > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
  }
  let sign = if n < 0.0 { "-" } else { "" };
  let shortest = format!("{:e}", n.abs());
  let length = shortest.split_once('e').expect("`{:e}` writes an exponent").0.replace('.', "").len();
  let closest = format!("{:.*e}", length - 1, n.abs());
  let scientific = if closest.parse::<f64>() == Ok(n.abs()) { closest } else { shortest };
  let (mantissa, exponent) = scientific.split_once('e').expect("`{:e}` writes an exponent");
  let digits = mantissa.replace('.', "");
  let k = digits.len() as i64;
  let point = exponent.parse::<i64>().expect("an integer exponent") + 1;
  let body = if k <= point && point <= 21 {
    format!("{digits}{}", "0".repeat((point - k) as usize))
  } else if 0 < point && point <= 21 {
    format!("{}.{}", &digits[..point as usize], &digits[point as usize..])
  } else if -6 < point && point <= 0 {
    format!("0.{}{digits}", "0".repeat((-point) as usize))
  } else {
    let e = point - 1;
    let e = if e < 0 { format!("-{}", -e) } else { format!("+{e}") };
    match digits.split_at(1) {
      (first, "") => format!("{first}e{e}"),
      (first, rest) => format!("{first}.{rest}e{e}"),
    }
  };
  format!("{sign}{body}")
}

/// Whether a value is left out of an object, as `undefined` and a function are.
fn skipped(value: &Value) -> bool {
  matches!(value, Value::Undefined | Value::Ref { .. })
}

fn write(out: &mut String, value: &Value, indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  match value {
    Value::Null | Value::Undefined | Value::Ref { .. } => out.push_str("null"),
    Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
    Value::F64(f) => float(out, *f),
    Value::F32(f) => float(out, f64::from(*f)),
    Value::Int(n) => integer(out, &n.to_string(), integers)?,
    Value::UInt(n) => integer(out, &n.to_string(), integers)?,
    Value::Str(s) => string(out, s),
    Value::Seq(items) => {
      let items: Vec<&Value> = items.iter().collect();
      array(out, &items, indent, depth, integers)?;
    }
    Value::Map(map) => {
      let fields: Vec<(String, &Value)> = map.iter().filter(|(_, v)| !skipped(v)).map(|(k, v)| (k.clone(), v)).collect();
      object(out, &fields, indent, depth, integers)?;
    }
    Value::Bytes(bytes) => indexed(out, bytes.iter().map(|b| Value::Int((*b).into())).collect(), indent, depth, integers)?,
    Value::TypedArray(array) => indexed(out, elements(array), indent, depth, integers)?,
    Value::Variant { tag, payload } => match (tag.as_str(), payload.as_deref()) {
      (typed::DATE, Some(ms)) => match millis(ms).and_then(crate::time::iso_string) {
        Some(iso) => string(out, &iso),
        None => out.push_str("null"),
      },
      (typed::MAP | typed::SET, _) => out.push_str("{}"),
      (typed::URL, Some(href)) => write(out, href, indent, depth, integers)?,
      (typed::BIGINT, Some(n)) => match n {
        Value::Str(digits) => integer(out, digits, integers)?,
        other => write(out, other, indent, depth, integers)?,
      },
      (_, payload) => {
        let tag_value = Value::str(tag.as_str());
        let mut fields: Vec<(String, &Value)> = vec![("tag".to_owned(), &tag_value)];
        if let Some(payload) = payload.filter(|p| !skipped(p)) {
          fields.push(("payload".to_owned(), payload));
        }
        object(out, &fields, indent, depth, integers)?;
      }
    },
  }
  Ok(())
}

fn float(out: &mut String, f: f64) {
  if f.is_finite() {
    out.push_str(&number_text(f));
  } else {
    out.push_str("null");
  }
}

fn integer(out: &mut String, digits: &str, integers: Integers) -> Result<(), Unwritable> {
  match integers {
    Integers::Digits => {
      out.push_str(digits);
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
  let fields: Vec<(String, &Value)> = items.iter().enumerate().map(|(i, v)| (i.to_string(), v)).collect();
  object(out, &fields, indent, depth, integers)
}

fn newline(out: &mut String, indent: &str, depth: usize) {
  if !indent.is_empty() {
    out.push('\n');
    for _ in 0..depth {
      out.push_str(indent);
    }
  }
}

fn array(out: &mut String, items: &[&Value], indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  if items.is_empty() {
    out.push_str("[]");
    return Ok(());
  }
  out.push('[');
  for (i, item) in items.iter().enumerate() {
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

fn object(out: &mut String, fields: &[(String, &Value)], indent: &str, depth: usize, integers: Integers) -> Result<(), Unwritable> {
  if fields.is_empty() {
    out.push_str("{}");
    return Ok(());
  }
  out.push('{');
  for (i, (key, item)) in fields.iter().enumerate() {
    if i > 0 {
      out.push(',');
    }
    newline(out, indent, depth + 1);
    string(out, key);
    out.push(':');
    if !indent.is_empty() {
      out.push(' ');
    }
    write(out, item, indent, depth + 1, integers)?;
  }
  newline(out, indent, depth);
  out.push('}');
  Ok(())
}

fn string(out: &mut String, s: &str) {
  out.push('"');
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
