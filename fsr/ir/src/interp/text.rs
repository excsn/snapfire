use super::*;

pub(crate) fn scalar_str(value: &Value) -> Result<std::borrow::Cow<'_, str>, Fail> {
  use std::borrow::Cow;
  Ok(match value {
    Value::Str(s) => Cow::Borrowed(s.as_str()),
    Value::Null => Cow::Borrowed("null"),
    Value::Undefined => Cow::Borrowed("undefined"),
    Value::Bool(true) => Cow::Borrowed("true"),
    Value::Bool(false) => Cow::Borrowed("false"),
    Value::F64(f) if *f == 0.0 => Cow::Borrowed("0"),
    Value::F32(f) if *f == 0.0 => Cow::Borrowed("0"),
    other => Cow::Owned(stringify(other)?),
  })
}

/// JavaScript's `Number::toString` in radix 10: the fewest digits that read back as `n`, the closest of those to `n` with a tie to the even one, in exponent form from 1e21 up and below 1e-6.
/// `split`'s `limit` as `ToUint32` reads it; no limit when it is undefined.
pub(super) fn split_limit(args: &[Value]) -> usize {
  match args.get(2) {
    None | Some(Value::Null | Value::Undefined) => usize::MAX,
    Some(limit) => to_number(limit).filter(|n| n.is_finite()).map_or(0.0, |n| n.trunc().rem_euclid(4_294_967_296.0)) as usize,
  }
}

pub(crate) use snapfire_fsr_core::json::number_text;

/// An array as JavaScript's `+` reads it, `Array.prototype.join(",")`: an item that is null or undefined is empty and a nested array is joined the same way.
pub(super) fn array_text_into(items: &[Value], out: &mut String) -> Result<bool, Fail> {
  for (at, item) in items.iter().enumerate() {
    if at > 0 {
      out.push(',');
    }
    match item {
      Value::Null | Value::Undefined => {}
      other => {
        if !plain_text_into(other, out)? {
          return Ok(false);
        }
      }
    }
  }
  Ok(true)
}

pub(crate) fn stringify(value: &Value) -> Result<String, Fail> {
  let mut out = String::new();
  stringify_into(value, &mut out)?;
  Ok(out)
}

/// `String(value)` written onto the end of `out`.
pub(crate) fn stringify_into(value: &Value, out: &mut String) -> Result<(), Fail> {
  match plain_text_into(value, out)? {
    true => Ok(()),
    false => Err(type_error("String", "a scalar, an array or a plain object", value)),
  }
}

/// `String(value)` onto the end of `out` or `false` once it meets an object that converts itself, which takes the interpreter to call.
pub(super) fn plain_text_into(value: &Value, out: &mut String) -> Result<bool, Fail> {
  match value {
    Value::Null => out.push_str("null"),
    Value::Undefined => out.push_str("undefined"),
    Value::Bool(true) => out.push_str("true"),
    Value::Bool(false) => out.push_str("false"),
    Value::Int(n) => out.push_str(itoa::Buffer::new().format(*n)),
    Value::UInt(n) => out.push_str(itoa::Buffer::new().format(*n)),
    Value::F64(f) => snapfire_fsr_core::json::write_number(out, *f),
    Value::F32(f) => snapfire_fsr_core::json::write_number(out, f64::from(*f)),
    Value::Str(s) => out.push_str(s.as_str()),
    Value::Bytes(bytes) => out.push_str(&String::from_utf8_lossy(bytes)),
    Value::Seq(items) if items.kind() == Collection::Set => out.push_str("[object Set]"),
    Value::Map(map) if map.kind() == Collection::Map => out.push_str("[object Map]"),
    Value::Seq(items) => return on_enough_stack(|| array_text_into(items, out)),
    Value::Map(_) if converts_itself(value) => return Ok(false),
    Value::Map(_) => out.push_str("[object Object]"),
    temporal @ Value::Variant { .. } if snapfire_fsr_core::value::typed::temporal(temporal).is_some() => out.push_str(snapfire_fsr_core::value::typed::temporal(temporal).map_or("", |(_, text)| text)),
    other => return Err(type_error("String", "a scalar, an array or a plain object", other)),
  }
  Ok(true)
}

std::thread_local! {
  static SPELLING: std::cell::Cell<String> = const { std::cell::Cell::new(String::new()) };
}

/// The string `write` spells, built in a buffer the thread reuses, so the result is the only allocation and a short one has none.
/// `Number(value)`.
pub(crate) fn number_of(value: Value) -> Result<Value, Fail> {
  match value {
    v @ Value::F32(_) => Ok(v),
    v => to_number(&v).map(Value::F64).ok_or_else(|| type_error("Number", "a number, a string, a boolean or null", &v)),
  }
}

/// `BigInt(value)`.
pub(crate) fn bigint_of(value: Value) -> Result<Value, Fail> {
  match value {
    v @ Value::Int(_) => Ok(v),
    Value::UInt(n) => Ok(Value::Int(n as i128)),
    Value::F64(f) if f.fract() == 0.0 && f.abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
    Value::F32(f) if f.fract() == 0.0 && (f as f64).abs() <= SAFE_INTEGER => Ok(Value::Int(f as i128)),
    Value::Bool(b) => Ok(Value::Int(i128::from(b))),
    Value::Str(s) => s
      .trim()
      .parse::<i128>()
      .map(Value::Int)
      .map_err(|_| Fail::new(FailureKind::Invalid, format!("`{s}` is not an integer"))),
    other => Err(type_error("BigInt", "an integer, an integral number or a string", &other)),
  }
}

pub(crate) fn spelled(write: impl FnOnce(&mut String) -> Result<(), Fail>) -> Result<Value, Fail> {
  let mut out = SPELLING.take();
  out.clear();
  let spelled = write(&mut out).map(|()| Value::str(out.as_str()));
  if out.capacity() <= 1 << 16 {
    SPELLING.set(out);
  }
  spelled
}

pub(super) fn set_path(root: &mut Value, steps: &[Value], value: Value) -> Result<(), Fail> {
  let Some((first, rest)) = steps.split_first() else {
    *root = value;
    return Ok(());
  };
  if matches!(root, Value::Null | Value::Undefined) {
    *root = Value::Map(ValueMap::default());
  }
  match (root, first) {
    (Value::Map(map), Value::Str(key)) => {
      let child = map.entry(key.to_string()).or_insert(Value::Null);
      set_path(child, rest, value)
    }
    (Value::Seq(items), Value::Int(i)) => {
      let i = usize::try_from(*i).map_err(|_| Fail::internal("negative index"))?;
      let child = items.get_mut(i).ok_or_else(|| Fail::internal("index past the end"))?;
      set_path(child, rest, value)
    }
    (Value::Seq(items), Value::F64(f)) if f.fract() == 0.0 && *f >= 0.0 => {
      let i = *f as usize;
      let child = items.get_mut(i).ok_or_else(|| Fail::internal("index past the end"))?;
      set_path(child, rest, value)
    }
    (root, key) => Err(type_error("session write", "an object with a string key or an array with an index", if matches!(root, Value::Map(_) | Value::Seq(_)) { key } else { root })),
  }
}

pub(super) fn delete_path(root: &mut Value, steps: &[Value]) -> Result<(), Fail> {
  let Some((first, rest)) = steps.split_first() else {
    return Ok(());
  };
  match (root, first) {
    (Value::Map(map), Value::Str(key)) => {
      if rest.is_empty() {
        map.shift_remove(key.as_str());
        Ok(())
      } else if let Some(child) = map.get_mut(key.as_str()) {
        delete_path(child, rest)
      } else {
        Ok(())
      }
    }
    (Value::Null | Value::Undefined, _) => Ok(()),
    (root, key) => Err(type_error("session delete", "an object with a string key", if matches!(root, Value::Map(_)) { key } else { root })),
  }
}

/// JavaScript's `WhiteSpace` and `LineTerminator`, what `String.prototype.trim` strips.
pub(super) fn js_whitespace(c: char) -> bool {
  matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
