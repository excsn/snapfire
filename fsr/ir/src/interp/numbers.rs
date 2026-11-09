use super::*;

pub(super) fn float(op: ArithOp, a: f64, b: f64) -> f64 {
  float_inlined(op, a, b)
}

/// [`float`] for a generated body, which inlines it. The interpreter calls [`float`]: inlining this into [`arith`] slows its every call.
#[inline]
pub(crate) fn float_inlined(op: ArithOp, a: f64, b: f64) -> f64 {
  match op {
    ArithOp::Add => a + b,
    ArithOp::Sub => a - b,
    ArithOp::Mul => a * b,
    ArithOp::Div => a / b,
    ArithOp::Rem => a % b,
    ArithOp::Pow => js_pow(a, b),
    ArithOp::Quot => (a / b).trunc(),
    ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor | ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => bitwise(op, a, b) as f64,
  }
}

/// A bitwise operator on two numbers as JavaScript applies it: `ToInt32` on each side, the shift count taken modulo 32 and `>>>` reading its left side unsigned.
pub(super) fn bitwise(op: ArithOp, a: f64, b: f64) -> i128 {
  let (a, b) = (int32(a), int32(b));
  let shift = (b as u32) & 31;
  match op {
    ArithOp::BitAnd => i128::from(a & b),
    ArithOp::BitOr => i128::from(a | b),
    ArithOp::BitXor => i128::from(a ^ b),
    ArithOp::Shl => i128::from(a.wrapping_shl(shift)),
    ArithOp::Shr => i128::from(a >> shift),
    ArithOp::UShr => i128::from((a as u32) >> shift),
    _ => unreachable!("bitwise is called for the bitwise operators"),
  }
}

pub(super) fn arith(op: ArithOp, l: Value, r: Value) -> Result<Value, Fail> {
  if matches!(op, ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor | ArithOp::Shl | ArithOp::Shr | ArithOp::UShr) {
    return match (to_number(&l), to_number(&r)) {
      (Some(a), Some(b)) => Ok(Value::Int(bitwise(op, a, b))),
      _ => Err(Fail::internal(format!("{:?} wants numbers, strings, booleans or null, got {} and {}", op, kind_name(&l), kind_name(&r)))),
    };
  }
  match (l, r) {
    (Value::Int(a), Value::Int(b)) if op == ArithOp::Div => Ok(match a.checked_rem(b).filter(|rem| *rem == 0).and_then(|_| a.checked_div(b)) {
      Some(quotient) => Value::Int(quotient),
      None => Value::F64(a as f64 / b as f64),
    }),
    (Value::Int(a), Value::Int(b)) => Ok(Value::Int(match op {
      ArithOp::Add => a.checked_add(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Sub => a.checked_sub(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Mul => a.checked_mul(b).ok_or_else(|| Fail::internal("integer overflow"))?,
      ArithOp::Div | ArithOp::Quot => a.checked_div(b).ok_or_else(|| Fail::internal("a bigint divided by zero"))?,
      ArithOp::Rem => a.checked_rem(b).ok_or_else(|| Fail::internal("division by zero"))?,
      ArithOp::Pow => u32::try_from(b).ok().and_then(|b| a.checked_pow(b)).ok_or_else(|| Fail::internal("integer power out of range"))?,
      ArithOp::BitAnd | ArithOp::BitOr | ArithOp::BitXor | ArithOp::Shl | ArithOp::Shr | ArithOp::UShr => bitwise(op, a as f64, b as f64),
    })),
    (Value::F64(a), Value::F64(b)) => Ok(Value::F64(float(op, a, b))),
    // A number literal lowers as a float and a record's width is an integer,
    // so `width / 2` meets here; JavaScript has one number type and so does this.
    (Value::Int(a), Value::F64(b)) => Ok(Value::F64(float(op, a as f64, b))),
    (Value::F64(a), Value::Int(b)) => Ok(Value::F64(float(op, a, b as f64))),
    (Value::UInt(a), Value::F64(b)) => Ok(Value::F64(float(op, a as f64, b))),
    (Value::F64(a), Value::UInt(b)) => Ok(Value::F64(float(op, a, b as f64))),
    (Value::Str(a), Value::Str(b)) if op == ArithOp::Add => spelled(|out| {
      out.push_str(a.as_str());
      out.push_str(b.as_str());
      Ok(())
    }),
    (l, r) if op == ArithOp::Add && (matches!(l, Value::Seq(_) | Value::Map(_)) || matches!(r, Value::Seq(_) | Value::Map(_))) => spelled(|out| {
      stringify_into(&l, out)?;
      stringify_into(&r, out)
    }),
    // `+` with a string on either side concatenates the other as `String(x)` does.
    (Value::Str(a), other) if op == ArithOp::Add && !matches!(other, Value::Seq(_) | Value::Map(_)) => spelled(|out| {
      out.push_str(a.as_str());
      stringify_into(&other, out)
    }),
    (other, Value::Str(b)) if op == ArithOp::Add && !matches!(other, Value::Seq(_) | Value::Map(_)) => spelled(|out| {
      stringify_into(&other, out)?;
      out.push_str(b.as_str());
      Ok(())
    }),
    (l, r) => match (to_number(&l), to_number(&r)) {
      (Some(a), Some(b)) => Ok(Value::F64(float(op, a, b))),
      _ => Err(Fail::internal(format!("{:?} wants numbers, strings, booleans or null, got {} and {}", op, kind_name(&l), kind_name(&r)))),
    },
  }
}

pub(super) fn is_number(value: &Value) -> bool {
  matches!(value, Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_))
}

/// JavaScript's `ToNumber`, through [`stringify`] for an array or an object: null is 0, undefined NaN, a boolean 0 or 1 and a string its numeric literal or NaN. `None` for anything else.
pub(super) fn to_number(value: &Value) -> Option<f64> {
  match value {
    Value::Null => Some(0.0),
    Value::Undefined => Some(f64::NAN),
    Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
    Value::Int(n) => Some(*n as f64),
    Value::UInt(n) => Some(*n as f64),
    Value::F32(f) => Some(*f as f64),
    Value::F64(f) => Some(*f),
    Value::Str(s) => Some(string_number(s)),
    Value::Seq(_) | Value::Map(_) => stringify(value).ok().map(|text| string_number(&text)),
    _ => None,
  }
}

pub(super) fn map_key(key: &Value) -> Result<String, Fail> {
  match key {
    Value::Seq(_) | Value::Map(_) => Err(type_error("a `Map` key", "a scalar", key)),
    other => stringify(other),
  }
}

/// JavaScript's `==`. Two objects compare as `===` does.
pub(super) fn loose_equal(l: &Value, r: &Value) -> Result<bool, Fail> {
  let object = |v: &Value| matches!(v, Value::Seq(_) | Value::Map(_));
  match (l, r) {
    (Value::Null | Value::Undefined, Value::Null | Value::Undefined) => Ok(true),
    (Value::Null | Value::Undefined, _) | (_, Value::Null | Value::Undefined) => Ok(false),
    (l, r) if object(l) && object(r) => compare(CompareOp::Eq, l, r),
    (l, r) if object(l) => loose_equal(&Value::str(stringify(l)?), r),
    (l, r) if object(r) => loose_equal(l, &Value::str(stringify(r)?)),
    (Value::Str(a), Value::Str(b)) => Ok(a == b),
    (l, r) if matches!(l, Value::Bool(_) | Value::Str(_)) || matches!(r, Value::Bool(_) | Value::Str(_)) => match (to_number(l), to_number(r)) {
      (Some(a), Some(b)) => Ok(a == b),
      _ => compare(CompareOp::Eq, l, r),
    },
    (l, r) => compare(CompareOp::Eq, l, r),
  }
}

/// JavaScript's `ToInt32`.
pub(super) fn int32(n: f64) -> i32 {
  if !n.is_finite() {
    return 0;
  }
  n.trunc().rem_euclid(4_294_967_296.0) as u32 as i32
}

/// `parseInt(s, radix)` once `s` is a string and `radix` an `int32`.
pub(super) fn parse_int(s: &str, radix: i32) -> f64 {
  let s = s.trim_start_matches(js_whitespace);
  let (negative, s) = match s.as_bytes().first() {
    Some(b'-') => (true, &s[1..]),
    Some(b'+') => (false, &s[1..]),
    _ => (false, s),
  };
  let (radix, s) = match radix {
    0 | 16 => match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
      Some(rest) => (16, rest),
      None => (if radix == 0 { 10 } else { 16 }, s),
    },
    2..=36 => (radix as u32, s),
    _ => return f64::NAN,
  };
  let digits = &s[..s.find(|c: char| !c.is_digit(radix)).unwrap_or(s.len())];
  if digits.is_empty() {
    return f64::NAN;
  }
  let magnitude = if radix == 10 {
    digits.parse::<f64>().unwrap_or(f64::NAN)
  } else {
    let mut exact: u128 = 0;
    let mut rest = digits;
    while let Some(c) = rest.chars().next() {
      match exact.checked_mul(u128::from(radix)).and_then(|n| n.checked_add(u128::from(c.to_digit(radix).expect("a digit")))) {
        Some(n) => exact = n,
        None => break,
      }
      rest = &rest[1..];
    }
    exact as f64 * f64::from(radix).powi(rest.len() as i32)
  };
  if negative { -magnitude } else { magnitude }
}

/// The digits of an integer `n` in `radix`, as `Number.prototype.toString` writes them; `None` for a finite number with a fraction and for one past `Number.MAX_SAFE_INTEGER` in a radix that is not a power of 2, whose digits engines round differently.
pub(super) fn radix_digits(n: f64, radix: u32) -> Option<String> {
  if n.is_nan() {
    return Some("NaN".to_owned());
  }
  if n.is_infinite() {
    return Some(if n < 0.0 { "-Infinity" } else { "Infinity" }.to_owned());
  }
  if n.fract() != 0.0 || (n.abs() > SAFE_INTEGER && !radix.is_power_of_two()) {
    return None;
  }
  let bits = n.abs().to_bits();
  let exponent = ((bits >> 52) & 0x7ff) as i64;
  let mantissa = if exponent == 0 { (bits & ((1 << 52) - 1)) << 1 } else { (bits & ((1 << 52) - 1)) | (1 << 52) };
  let shift = exponent - 1075;
  let mut limbs: Vec<u32> = vec![mantissa as u32, (mantissa >> 32) as u32];
  if shift > 0 {
    let (words, bits) = ((shift / 32) as usize, (shift % 32) as u32);
    let mut shifted = vec![0u32; words];
    let mut carry = 0u32;
    for limb in limbs {
      shifted.push(if bits == 0 { limb } else { (limb << bits) | carry });
      carry = if bits == 0 { 0 } else { limb >> (32 - bits) };
    }
    shifted.push(carry);
    limbs = shifted;
  } else if shift < 0 {
    let value = mantissa >> (-shift).min(63);
    limbs = vec![value as u32, (value >> 32) as u32];
  }
  let mut out = Vec::new();
  while limbs.iter().any(|&limb| limb != 0) {
    let mut remainder = 0u64;
    for limb in limbs.iter_mut().rev() {
      let current = (remainder << 32) | u64::from(*limb);
      *limb = (current / u64::from(radix)) as u32;
      remainder = current % u64::from(radix);
    }
    out.push(char::from_digit(remainder as u32, radix).expect("a digit"));
  }
  if out.is_empty() {
    out.push('0');
  }
  if n < 0.0 {
    out.push('-');
  }
  Some(out.into_iter().rev().collect())
}

/// JavaScript's `StringToNumber`: surrounding whitespace is dropped, empty is 0, `0x`, `0o` and `0b` read unsigned in their base, `Infinity` may carry a sign and anything else that is not a decimal literal is NaN.
pub(super) fn string_number(s: &str) -> f64 {
  let s = s.trim_matches(js_whitespace);
  if s.is_empty() {
    return 0.0;
  }
  for (prefix, radix) in [("0x", 16), ("0X", 16), ("0o", 8), ("0O", 8), ("0b", 2), ("0B", 2)] {
    if let Some(digits) = s.strip_prefix(prefix) {
      if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return f64::NAN;
      }
      return digits.chars().fold(0.0, |n, c| n * f64::from(radix) + f64::from(c.to_digit(radix).expect("checked digit")));
    }
  }
  let unsigned = s.strip_prefix(['+', '-']).unwrap_or(s);
  if unsigned == "Infinity" {
    return if s.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY };
  }
  if !unsigned.starts_with(|c: char| c.is_ascii_digit() || c == '.') || !unsigned.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-')) {
    return f64::NAN;
  }
  s.parse().unwrap_or(f64::NAN)
}

pub(super) fn compare(op: CompareOp, l: &Value, r: &Value) -> Result<bool, Fail> {
  use std::cmp::Ordering;
  if !matches!(op, CompareOp::Eq | CompareOp::Ne) && (matches!(l, Value::Seq(_) | Value::Map(_)) || matches!(r, Value::Seq(_) | Value::Map(_))) {
    let primitive = |v: &Value| match v {
      Value::Seq(_) | Value::Map(_) => stringify(v).map(Value::str),
      other => Ok(other.clone()),
    };
    return compare(op, &primitive(l)?, &primitive(r)?);
  }
  if matches!(op, CompareOp::Eq | CompareOp::Ne) {
    let same = match (l, r) {
      (Value::Map(a), Value::Map(b)) => Some(a.same(b)),
      (Value::Seq(a), Value::Seq(b)) => Some(a.same(b)),
      _ => None,
    };
    if let Some(same) = same {
      return Ok(same == (op == CompareOp::Eq));
    }
  }
  let ordering = match (l, r) {
    (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
    (Value::F64(a), Value::F64(b)) => a.partial_cmp(b),
    (Value::Str(a), Value::Str(b)) => Some(a.cmp(b)),
    (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
    (Value::Null, Value::Null) | (Value::Undefined, Value::Undefined) => Some(Ordering::Equal),
    (l, r) if is_number(l) && is_number(r) => to_number(l).zip(to_number(r)).and_then(|(a, b)| a.partial_cmp(&b)),
    (l, r) if matches!(op, CompareOp::Eq | CompareOp::Ne) && kind_name(l) != kind_name(r) => return Ok(op == CompareOp::Ne),
    (l, r) => match (to_number(l), to_number(r)) {
      (Some(a), Some(b)) if op != CompareOp::Eq && op != CompareOp::Ne => a.partial_cmp(&b),
      _ => return Err(Fail::internal(format!("comparing {} with {} is not defined", kind_name(l), kind_name(r)))),
    },
  };
  let Some(ordering) = ordering else {
    return Ok(matches!(op, CompareOp::Ne));
  };
  Ok(match op {
    CompareOp::Eq => ordering == Ordering::Equal,
    CompareOp::Ne => ordering != Ordering::Equal,
    CompareOp::Lt => ordering == Ordering::Less,
    CompareOp::Le => ordering != Ordering::Greater,
    CompareOp::Gt => ordering == Ordering::Greater,
    CompareOp::Ge => ordering != Ordering::Less,
  })
}

/// The matches `s.replace(pattern, f)` replaces, each with its byte range and the arguments `f` receives: the match, each group, the UTF-16 offset and the subject.
pub(super) fn replacements<'v>(subject: &'v Value, pattern: &Value) -> Result<(&'v str, Vec<(usize, usize, Vec<Value>)>), Fail> {
  let Value::Str(s) = subject else { return Err(type_error("replace", "a string", subject)) };
  let s: &str = s;
  let whole = Value::str(s);
  let mut found = Vec::new();
  match crate::jsregex::of(pattern) {
    Some((source, flags)) => {
      let re = crate::jsregex::compiled(source, flags).map_err(Fail::internal)?;
      let all = if flags.contains('g') { re.all(s) } else { re.first(s).map(Vec::from_iter) };
      for m in all.map_err(Fail::internal)? {
        let at = m.whole();
        let mut args: Vec<Value> = m.texts(s).map(|g| g.map_or(Value::Null, Value::str)).collect();
        args.push(Value::F64(crate::jsregex::utf16_index(s, at.start) as f64));
        args.push(whole.clone());
        found.push((at.start, at.end, args));
      }
    }
    None => {
      let from = stringify(pattern)?;
      if let Some(at) = s.find(from.as_str()) {
        found.push((at, at + from.len(), vec![Value::str(from.clone()), Value::F64(crate::jsregex::utf16_index(s, at) as f64), whole.clone()]));
      }
    }
  }
  Ok((s, found))
}

/// The array a callback receives last, `(item, index, array)` or `(acc, item, index, array)`, when its lambda names more than two parameters; an argument past the last parameter is dropped.
pub(super) fn array_arg(f: &Expr, items: &[Value]) -> Option<Value> {
  match f {
    Expr::Lambda { params, .. } if params.len() > 2 => Some(Value::seq(items.to_vec())),
    _ => None,
  }
}

pub(super) fn with_array(mut args: Vec<Value>, whole: &Option<Value>) -> Vec<Value> {
  if let Some(array) = whole {
    args.push(array.clone());
  }
  args
}
