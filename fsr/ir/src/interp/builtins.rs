use super::*;

/// The response `Response`, `ResponseJson`, `ResponseRedirect` and `RedirectTo` make, failing where the Fetch standard throws.
pub(super) fn response_builtin(name: Builtin, args: &[Value]) -> Result<Value, Fail> {
  use snapfire_fsr_runtime::{HttpResponse, REDIRECT_STATUSES};
  let status_of = |value: &Value| -> Result<u16, Fail> {
    match value {
      Value::Int(n) => u16::try_from(*n).map_err(|_| Fail::internal(format!("RangeError: {n} is not a status"))),
      Value::F64(n) if n.fract() == 0.0 && (0.0..=f64::from(u16::MAX)).contains(n) => Ok(*n as u16),
      other => Err(type_error("a response status", "a whole number", other)),
    }
  };
  let init = |response: &mut HttpResponse, init: Option<&Value>| -> Result<(), Fail> {
    let Some(Value::Map(init)) = init else { return Ok(()) };
    if let Some(status) = init.get("status").filter(|status| !matches!(status, Value::Null | Value::Undefined)) {
      response.status = status_of(status)?;
      if !(200..=599).contains(&response.status) {
        return Err(Fail::internal(format!("RangeError: the status {} is outside 200 to 599", response.status)));
      }
    }
    match init.get("headers") {
      None | Some(Value::Null | Value::Undefined) => {}
      Some(Value::Map(headers)) => {
        for (header, value) in headers {
          response.headers.retain(|(name, _)| !name.eq_ignore_ascii_case(header));
          response.headers.push((header.to_string(), stringify(value)?));
        }
      }
      Some(Value::Seq(pairs)) => {
        for pair in pairs.iter() {
          let Value::Seq(pair) = pair else { return Err(type_error("a response header", "a [name, value] pair", pair)) };
          let (Some(header), Some(value)) = (pair.get(0), pair.get(1)) else { return Err(Fail::internal("TypeError: a header pair holds a name and a value")) };
          response.headers.push((stringify(header)?, stringify(value)?));
        }
      }
      Some(other) => return Err(type_error("response headers", "an object of strings", other)),
    }
    Ok(())
  };
  let mut response = HttpResponse::new(200);
  match name {
    Builtin::Response => {
      init(&mut response, args.get(1))?;
      match args.first() {
        None | Some(Value::Null | Value::Undefined) => {}
        Some(body) if [204, 205, 304].contains(&response.status) => return Err(type_error("a response with a null body status", "no body", body)),
        Some(Value::Bytes(bytes)) => response.body = bytes.clone(),
        Some(Value::TypedArray(snapfire_fsr_core::TypedArray::U8(bytes))) => response.body = bytes.clone(),
        Some(body) => {
          response.body = stringify(body)?.into_bytes();
          response.header_default("content-type", "text/plain;charset=UTF-8");
        }
      }
    }
    Builtin::ResponseJson => {
      let out = snapfire_fsr_core::json::stringify(args.first().unwrap_or(&Value::Undefined), "", snapfire_fsr_core::json::Integers::Digits).map_err(|e| Fail::internal(e.0))?;
      response.headers.push(("content-type".to_owned(), "application/json".to_owned()));
      init(&mut response, args.get(1))?;
      response.body = out.into_bytes();
    }
    Builtin::ResponseRedirect | Builtin::RedirectTo => {
      let to = stringify(args.first().unwrap_or(&Value::Undefined))?;
      if name == Builtin::RedirectTo {
        snapfire_fsr_runtime::RedirectHandle::check(&to).map_err(|e| Fail::new(e.kind, e.message))?;
        response.status = 303;
      } else {
        if !to.starts_with('/') && url::Url::parse(&to).is_err() {
          return Err(Fail::internal(format!("TypeError: Invalid URL: {to}")));
        }
        response.status = match args.get(1) {
          None | Some(Value::Null | Value::Undefined) => 302,
          Some(status) => status_of(status)?,
        };
        if !REDIRECT_STATUSES.contains(&response.status) {
          return Err(Fail::internal(format!("RangeError: {} is not a redirect status", response.status)));
        }
      }
      response.headers.push(("location".to_owned(), to));
    }
    _ => return Err(Fail::internal(format!("{name:?} does not make a response"))),
  }
  Ok(response.to_value())
}

/// `new Request(url)` and what a response's members read and write.
pub(super) fn message_builtin(name: Builtin, args: &[Value]) -> Result<Value, Fail> {
  use snapfire_fsr_runtime::HttpResponse;
  let text = |at: usize| args.get(at).map(stringify).unwrap_or_else(|| Ok(String::new()));
  if name == Builtin::RequestNew {
    let mut fields = ValueMap::default();
    fields.insert("url".to_owned(), Value::str(text(0)?));
    return Ok(Value::Variant { tag: snapfire_fsr_runtime::REQUEST_TAG.to_owned(), payload: Some(Box::new(Value::Map(fields))) });
  }
  let mut response = match args.first().and_then(HttpResponse::from_value) {
    Some(Ok(response)) => response,
    Some(Err(message)) => return Err(Fail::internal(message)),
    None => return Err(type_error("a response member", "a Response", args.first().unwrap_or(&Value::Undefined))),
  };
  Ok(match name {
    Builtin::ResponseMember => match text(1)?.as_str() {
      "status" => Value::F64(f64::from(response.status)),
      "ok" => Value::Bool((200..300).contains(&response.status)),
      other => return Err(Fail::internal(format!("`{other}` is not a member of a Response the build reads"))),
    },
    Builtin::ResponseHeaderGet => {
      let header = text(1)?;
      let found = response.header(&header).map(str::to_owned);
      match args.get(2) {
        Some(Value::Str(op)) if op.as_str() == "has" => Value::Bool(found.is_some()),
        _ => found.map(Value::str).unwrap_or(Value::Null),
      }
    }
    _ => {
      let (op, header) = (text(1)?, text(2)?);
      match op.as_str() {
        "set" => {
          response.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&header));
          response.headers.push((header, text(3)?));
        }
        "append" => response.headers.push((header, text(3)?)),
        "delete" => response.headers.retain(|(n, _)| !n.eq_ignore_ascii_case(&header)),
        other => return Err(Fail::internal(format!("`headers.{other}` is not one the build reads"))),
      }
      response.to_value()
    }
  })
}

/// The builtins JavaScript's standard library added beside the first set, apart from [`builtin`] so its frame stays the size a render nested deep on a small stack can afford.
#[inline(never)]
fn builtin_js(name: Builtin, args: Vec<Value>) -> Result<Value, Fail> {
  let arg = |i: usize| args.get(i).ok_or_else(|| Fail::internal(format!("{name:?} takes more arguments")));
  Ok(match name {
    Builtin::Tagged => Value::Variant { tag: text(name, arg(0)?)?.to_owned(), payload: Some(Box::new(arg(1)?.clone())) },
    Builtin::TaggedPayload => match arg(0)? {
      Value::Variant { tag, payload } if tag == text(name, arg(1)?)? => payload.as_deref().cloned().unwrap_or(Value::Undefined),
      _ => Value::Undefined,
    },
    Builtin::IsTagged => Value::Bool(matches!(arg(0)?, Value::Variant { tag, .. } if tag == text(name, arg(1)?)?)),
    Builtin::Settle => match arg(0)? {
      Value::Map(held) if held.get(snapfire_fsr_core::value::typed::DEFERRED).is_some() => match held.get("$state") {
        Some(Value::Str(state)) if state.as_str() == "fulfilled" => held.get("value").cloned().unwrap_or(Value::Undefined),
        Some(Value::Str(state)) if state.as_str() == "rejected" => {
          let reason = held.get("reason").cloned().unwrap_or(Value::Undefined);
          let message = match &reason {
            Value::Map(fields) => fields.get("message").cloned().unwrap_or_else(|| reason.clone()),
            other => other.clone(),
          };
          return Err(super::classes::thrown_fail(&[message, reason]));
        }
        _ => return Err(Fail::internal("a promise from `Promise.withResolvers()` that nothing resolved before it was awaited")),
      },
      other => other.clone(),
    },
    Builtin::ExecFrom | Builtin::TestFrom => {
      let Value::Map(held) = arg(0)? else { return Err(type_error(&format!("{name:?}"), "a regular expression", arg(0)?)) };
      let (pattern, flags) = crate::jsregex::of(arg(0)?).ok_or_else(|| type_error(&format!("{name:?}"), "a regular expression", arg(0).unwrap_or(&Value::Null)))?;
      let s = text(name, arg(1)?)?;
      let moves = flags.contains('g') || flags.contains('y');
      let from = if moves { to_number(held.get(crate::jsregex::LAST_INDEX).unwrap_or(&Value::F64(0.0))).filter(|n| !n.is_nan()).unwrap_or(0.0).max(0.0) } else { 0.0 };
      let re = crate::jsregex::compiled(pattern, flags).map_err(Fail::internal)?;
      let found = if from > s.encode_utf16().count() as f64 { None } else { re.at(s, from as usize).map_err(Fail::internal)? };
      let mut after = held.clone();
      if moves {
        let last = found.as_ref().map_or(0.0, |m| crate::jsregex::utf16_index(s, m.whole().end) as f64);
        after.insert(crate::jsregex::LAST_INDEX.to_owned(), Value::F64(last));
      }
      let answer = match (name, found) {
        (Builtin::TestFrom, found) => Value::Bool(found.is_some()),
        (_, None) => Value::Null,
        (_, Some(m)) => Value::seq(m.texts(s).map(|g| g.map_or(Value::Null, Value::str)).collect::<Vec<_>>()),
      };
      Value::seq(vec![answer, Value::Map(after)])
    }
    Builtin::Substr => {
      let units: Vec<u16> = stringify(arg(0)?)?.encode_utf16().collect();
      let from = relative(integer(name, args.get(1))?, units.len());
      let count = match args.get(2) {
        None | Some(Value::Undefined) => units.len() as f64,
        Some(v) => integer(name, Some(v))?.max(0.0),
      };
      let to = (from as f64 + count).min(units.len() as f64) as usize;
      Value::str(from_units(name, &units[from..to.max(from)])?)
    }
    Builtin::IsWellFormed => {
      stringify(arg(0)?)?;
      Value::Bool(true)
    }
    Builtin::ToWellFormed => Value::str(stringify(arg(0)?)?),
    Builtin::UtcString => {
      let ms = number(name, arg(0)?)?;
      if !(ms.abs() <= MAX_DATE_MS) {
        return Ok(Value::str("Invalid Date"));
      }
      let ms = ms.trunc();
      let (y, m, d) = crate::std::time::civil_from_ms(ms);
      let (hh, mm, ss, _) = crate::std::time::clock_from_ms(ms);
      const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
      const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
      let weekday = DAYS[((ms / 86_400_000.0).floor() + 4.0).rem_euclid(7.0) as usize];
      let year = if y < 0 { format!("-{:04}", -y) } else { format!("{y:04}") };
      Value::str(format!("{weekday}, {d:02} {} {year} {hh:02}:{mm:02}:{ss:02} GMT", MONTHS[m as usize - 1]))
    }
    Builtin::SameValue => {
      let numeric = |v: &Value| matches!(v, Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_));
      let (a, b) = (arg(0)?, arg(1)?);
      if numeric(a) && numeric(b) {
        let (x, y) = (number(name, a)?, number(name, b)?);
        Value::Bool((x.is_nan() && y.is_nan()) || (x == y && x.is_sign_negative() == y.is_sign_negative()))
      } else {
        Value::Bool(strictly_equal(a, b))
      }
    }
    Builtin::FromCharCode => {
      let units: Vec<u16> = args.iter().map(|v| to_number(v).filter(|n| n.is_finite()).map_or(0, |n| n.trunc().rem_euclid(65_536.0) as u16)).collect();
      Value::str(from_units(name, &units)?)
    }
    Builtin::FromCodePoint => {
      let mut out = String::with_capacity(args.len());
      for v in &args {
        let point = to_number(v).filter(|n| n.fract() == 0.0 && (0.0..=1_114_111.0).contains(n)).ok_or_else(|| Fail::internal(format!("{name:?}: `{}` is not a code point", stringify(v).unwrap_or_default())))?;
        out.push(char::from_u32(point as u32).ok_or_else(|| Fail::internal(format!("{name:?}: {point} is a lone surrogate, which the value model holds no half of")))?);
      }
      Value::str(out)
    }
    Builtin::DateUtc => {
      if args.is_empty() {
        return Ok(whole(f64::NAN));
      }
      let mut fields = [f64::NAN, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0];
      for (field, v) in fields.iter_mut().zip(&args) {
        *field = to_number(v).unwrap_or(f64::NAN);
      }
      if fields.iter().any(|f| !f.is_finite()) {
        return Ok(whole(f64::NAN));
      }
      let [year, month, date, hours, minutes, seconds, millis] = fields.map(f64::trunc);
      let year = if (0.0..=99.0).contains(&year) { 1900.0 + year } else { year };
      let year = year + (month / 12.0).floor();
      if year.abs() > 400_000.0 {
        return Ok(whole(f64::NAN));
      }
      let days = crate::std::time::days_from_civil(year as i64, month.rem_euclid(12.0) as u32 + 1, 1) as f64 + date - 1.0;
      let ms = days * 86_400_000.0 + hours * 3_600_000.0 + minutes * 60_000.0 + seconds * 1000.0 + millis;
      whole(if ms.abs() <= MAX_DATE_MS { ms + 0.0 } else { f64::NAN })
    }
    Builtin::ToPrecision => {
      let n = number(name, arg(0)?)?;
      match args.get(1) {
        None | Some(Value::Undefined) => Value::str(stringify(&whole(n))?),
        Some(p) => {
          let p = integer(name, Some(p))?;
          if n.is_finite() && !(1.0..=100.0).contains(&p) {
            return Err(Fail::internal(format!("{name:?} takes 1 to 100 digits, got {p}")));
          }
          Value::str(to_precision(n, p as usize))
        }
      }
    }
    Builtin::ToExponential => {
      let n = number(name, arg(0)?)?;
      let fraction = match args.get(1) {
        None | Some(Value::Undefined) => None,
        Some(d) => Some(integer(name, Some(d))?),
      };
      if let Some(d) = fraction.filter(|d| n.is_finite() && !(0.0..=100.0).contains(d)) {
        return Err(Fail::internal(format!("{name:?} takes 0 to 100 digits, got {d}")));
      }
      Value::str(to_exponential(n, fraction.map(|d| d as usize)))
    }
    Builtin::Normalize => {
      use unicode_normalization::UnicodeNormalization;
      let s = stringify(arg(0)?)?;
      let form = match args.get(1) {
        None | Some(Value::Undefined) => "NFC".to_owned(),
        Some(v) => stringify(v)?,
      };
      Value::str(match form.as_str() {
        "NFC" => s.nfc().collect::<String>(),
        "NFD" => s.nfd().collect(),
        "NFKC" => s.nfkc().collect(),
        "NFKD" => s.nfkd().collect(),
        other => return Err(Fail::internal(format!("{name:?}: the form must be NFC, NFD, NFKC or NFKD, got `{other}`"))),
      })
    }
    Builtin::ToSpliced | Builtin::Splice | Builtin::ArrayWith | Builtin::Fill | Builtin::CopyWithin => {
      let Value::Seq(items) = arg(0)? else { return Err(type_error(&format!("{name:?}"), "an array", arg(0)?)) };
      let len = items.len();
      let mut out = items.to_vec();
      match name {
        Builtin::ToSpliced | Builtin::Splice => {
          let start = relative(integer(name, args.get(1))?, len);
          let delete = match args.get(2) {
            None | Some(Value::Undefined) => len - start,
            Some(v) => integer(name, Some(v))?.clamp(0.0, (len - start) as f64) as usize,
          };
          let rest = match args.get(3) {
            Some(Value::Seq(rest)) => rest.to_vec(),
            _ => Vec::new(),
          };
          let removed: Vec<Value> = out.splice(start..start + delete, rest).collect();
          if name == Builtin::Splice {
            return Ok(Value::seq(vec![Value::seq(out), Value::seq(removed)]));
          }
        }
        Builtin::ArrayWith => {
          let i = integer(name, args.get(1))?;
          let at = if i < 0.0 { i + len as f64 } else { i };
          if !(0.0..len as f64).contains(&at) {
            return Err(Fail::internal(format!("{name:?}: index {i} is outside an array of {len}")));
          }
          out[at as usize] = arg(2)?.clone();
        }
        Builtin::Fill => {
          let (from, to) = span_of(name, &args[1..], len)?;
          for slot in out.iter_mut().take(to).skip(from) {
            *slot = arg(1)?.clone();
          }
        }
        _ => {
          let target = relative(integer(name, args.get(1))?, len);
          let (from, to) = span_of(name, &args[1..], len)?;
          let count = to.saturating_sub(from).min(len - target);
          out[target..target + count].clone_from_slice(&items[from..from + count]);
        }
      }
      Value::seq(out)
    }
    Builtin::Iterate => {
      let kind = text(name, arg(0)?)?;
      match arg(1)? {
        Value::Seq(items) => {
          let set = items.kind() == Collection::Set;
          Value::seq(match kind {
            "values" => items.to_vec(),
            "keys" if set => items.to_vec(),
            "keys" => (0..items.len()).map(|i| whole(i as f64)).collect(),
            _ if set => items.iter().map(|v| Value::seq(vec![v.clone(), v.clone()])).collect(),
            _ => items.iter().enumerate().map(|(i, v)| Value::seq(vec![whole(i as f64), v.clone()])).collect(),
          })
        }
        other => return Err(type_error(&format!("{name:?}"), "an array or a Set", other)),
      }
    }
    other => unreachable!("{other:?} is not one of builtin_js's"),
  })
}

/// The URL a `UrlParse` href holds.
pub(super) fn href(value: &Value) -> Result<url::Url, Fail> {
  let text = stringify(value)?;
  url::Url::parse(&text).map_err(|_| Fail::internal(format!("Invalid URL: {text}")))
}

/// `application/x-www-form-urlencoded` as `URLSearchParams` writes it: letters, digits and `*-._` as they are, a space as `+`, every other byte `%XX`.
pub(super) fn form_encode(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  for byte in s.bytes() {
    match byte {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => out.push(byte as char),
      b' ' => out.push('+'),
      _ => out.push_str(&format!("%{byte:02X}")),
    }
  }
  out
}

/// The inverse of [`form_encode`], a malformed escape kept as written.
/// `a ** b` as JavaScript computes it: a base of 1 or -1 to an infinite power is NaN where `powf` answers 1.
pub(super) fn js_pow(a: f64, b: f64) -> f64 {
  if b.is_infinite() && a.abs() == 1.0 {
    return f64::NAN;
  }
  a.powf(b)
}

/// JavaScript's `ToIntegerOrInfinity` of an argument; absent is 0.
pub(super) fn integer(what: Builtin, value: Option<&Value>) -> Result<f64, Fail> {
  match value {
    None | Some(Value::Null | Value::Undefined) => Ok(0.0),
    Some(Value::Bool(b)) => Ok(if *b { 1.0 } else { 0.0 }),
    Some(v) => {
      let n = number(what, v)?;
      Ok(if n.is_nan() { 0.0 } else { n.trunc() })
    }
  }
}

/// A relative index as `slice` reads one: negative counts from the end, clamped to `0..=len`.
pub(super) fn relative(i: f64, len: usize) -> usize {
  if i < 0.0 {
    (len as f64 + i).max(0.0) as usize
  } else {
    i.min(len as f64) as usize
  }
}

/// What `encodeURI` leaves and `decodeURI` keeps escaped: the characters that separate a URI's parts.
const URI_RESERVED: &[u8] = b";/?:@&=+$,#";

/// `slice`'s start and end over `len` items, from arguments 1 and 2.
pub(super) fn span_of(what: Builtin, args: &[Value], len: usize) -> Result<(usize, usize), Fail> {
  let from = relative(integer(what, args.get(1))?, len);
  let to = match args.get(2) {
    None | Some(Value::Null | Value::Undefined) => len,
    Some(v) => relative(integer(what, Some(v))?, len),
  };
  Ok((from, to))
}

/// UTF-16 code units back to a string; a cut through a surrogate pair leaves a half no Rust string holds.
pub(super) fn from_units(what: Builtin, units: &[u16]) -> Result<String, Fail> {
  String::from_utf16(units).map_err(|_| Fail::internal(format!("{what:?} would split a surrogate pair, which the value model holds no half of")))
}

/// `===` over values: numbers by value whatever their width, objects and arrays by contents.
pub(super) fn strictly_equal(a: &Value, b: &Value) -> bool {
  match (a, b) {
    (Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_), Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_)) => {
      let n = |v: &Value| match v {
        Value::Int(n) => *n as f64,
        Value::UInt(n) => *n as f64,
        Value::F32(f) => *f as f64,
        Value::F64(f) => *f,
        _ => f64::NAN,
      };
      n(a) == n(b)
    }
    (Value::Map(a), Value::Map(b)) => a.same(b),
    (Value::Seq(a), Value::Seq(b)) => a.same(b),
    _ => a == b,
  }
}

/// A comparator's answer as a number, NaN and anything that is not one as 0.
pub(crate) fn comparator_order(value: &Value) -> f64 {
  let n = match value {
    Value::Int(n) => *n as f64,
    Value::UInt(n) => *n as f64,
    Value::F32(f) => *f as f64,
    Value::F64(f) => *f,
    Value::Bool(b) => f64::from(u8::from(*b)),
    _ => 0.0,
  };
  if n.is_nan() { 0.0 } else { n }
}

/// `sort`'s order without a comparator: `String(x)` compared in UTF-16 code units.
pub(crate) fn default_order(a: &Value, b: &Value) -> Result<f64, Fail> {
  let (a, b) = (stringify(a)?, stringify(b)?);
  Ok(match a.encode_utf16().cmp(b.encode_utf16()) {
    std::cmp::Ordering::Less => -1.0,
    std::cmp::Ordering::Equal => 0.0,
    std::cmp::Ordering::Greater => 1.0,
  })
}

/// A value as `console.log` writes it: a string as it is, a scalar as `String` makes it and an object or an array as JSON.
pub(crate) fn console_text(value: &Value) -> String {
  match value {
    Value::Str(s) => s.to_string(),
    Value::Seq(_) | Value::Map(_) => {
      snapfire_fsr_core::json::stringify(value, "", snapfire_fsr_core::json::Integers::Digits).unwrap_or_else(|_| format!("[{}]", snapfire_fsr_core::ext::kind_name(value)))
    }
    other => stringify(other).unwrap_or_else(|_| format!("[{}]", snapfire_fsr_core::ext::kind_name(other))),
  }
}

/// A parsed JSON document as a value: every number an `F64`, as JavaScript's `JSON.parse` reads them, and an object's keys in document order.
pub(super) fn json_value(parsed: serde_json::Value) -> Value {
  match parsed {
    serde_json::Value::Null => Value::Null,
    serde_json::Value::Bool(b) => Value::Bool(b),
    serde_json::Value::Number(n) => Value::F64(n.as_f64().unwrap_or(f64::NAN)),
    serde_json::Value::String(s) => Value::str(s),
    serde_json::Value::Array(items) => Value::Seq(items.into_iter().map(json_value).collect()),
    serde_json::Value::Object(fields) => {
      let mut map = ValueMap::default();
      for (key, value) in fields {
        map.insert(key, json_value(value));
      }
      Value::Map(map)
    }
  }
}

/// Binds a lambda's parameters to `args` in order, `Undefined` past the end; a parameter named `...x` takes every argument left as an array, as a rest parameter does.
pub(crate) fn bind(scope: &mut Vec<(Name, Value)>, params: &[String], args: Vec<Value>) {
  bind_params(scope, params, args)
}

/// Whether `key` names an own property of an array or a string of `len` items: an index below `len`, written as JavaScript writes it, or `length`.
fn own_index(key: &str, len: usize) -> bool {
  key == "length" || key.parse::<usize>().is_ok_and(|i| i < len && i.to_string() == key)
}

pub(super) fn bind_params(scope: &mut Vec<(Name, Value)>, params: &[String], args: Vec<Value>) {
  let mut args = args.into_iter();
  for param in params {
    if param.starts_with("...") {
      scope.push((Name::new(param), Value::Seq(args.by_ref().collect())));
      return;
    }
    scope.push((Name::new(param), args.next().unwrap_or(Value::Undefined)));
  }
}

/// How many items `Range(n)` makes: none below zero. A count past [`MAX_RANGE`] or without end fails.
pub(crate) fn range_len(n: f64) -> Result<usize, Fail> {
  let n = n.max(0.0);
  // A length from data would otherwise ask for an allocation the process
  // does not survive, which aborts rather than failing the request.
  if !n.is_finite() || n > MAX_RANGE as f64 {
    return Err(Fail::internal(format!("{:?} would build {n:.0} items", Builtin::Range)));
  }
  Ok(n as i64 as usize)
}

/// [`range_len`] of a value, read as `Range` reads its argument.
pub(crate) fn range_len_of(value: &Value) -> Result<usize, Fail> {
  range_len(number(Builtin::Range, value)?)
}

pub(super) fn number(what: Builtin, value: &Value) -> Result<f64, Fail> {
  match value {
    Value::Int(n) => Ok(*n as f64),
    Value::UInt(n) => Ok(*n as f64),
    Value::F32(f) => Ok(*f as f64),
    Value::F64(f) => Ok(*f),
    other => Err(type_error(&format!("{what:?}"), "a number", other)),
  }
}

/// JavaScript's `GetSubstitution` for a string pattern, which carries no
/// capture groups, so `$$`, `$&`, `` $` `` and `$'` are the whole vocabulary
/// and a `$1` stays the two characters it is written as.
pub(super) fn substitute(out: &mut String, replacement: &str, before: &str, matched: &str, after: &str) {
  let mut rest = replacement;
  while let Some(at) = rest.find('$') {
    out.push_str(&rest[..at]);
    let tail = &rest[at + 1..];
    match tail.as_bytes().first() {
      Some(b'$') => out.push('$'),
      Some(b'&') => out.push_str(matched),
      Some(b'`') => out.push_str(before),
      Some(b'\'') => out.push_str(after),
      _ => {
        out.push('$');
        rest = tail;
        continue;
      }
    }
    rest = &tail[1..];
  }
  out.push_str(rest);
}

pub(super) fn text<'a>(what: Builtin, value: &'a Value) -> Result<&'a str, Fail> {
  match value {
    Value::Str(s) => Ok(s),
    other => Err(type_error(&format!("{what:?}"), "a string", other)),
  }
}

/// A JavaScript number, which every builtin produces; integers stay `Int`
/// only when they were `BigInt`s.
pub(super) fn whole(f: f64) -> Value {
  Value::F64(f)
}

/// One builtin, with JavaScript's semantics for the value model: `round` is
/// half up, `toFixed` rounds half away from zero, `localeNumber` groups by
/// thousands with a comma.
pub(super) fn builtin(name: Builtin, args: Vec<Value>) -> Result<Value, Fail> {
  if name == Builtin::Spread {
    let mut args = args.into_iter();
    let (Some(Value::Str(inner)), Some(Value::Seq(items))) = (args.next(), args.next()) else {
      return Err(Fail::internal("Spread takes a builtin's name and an array of its arguments"));
    };
    let inner = crate::sexpr::builtin_of(inner.as_str()).filter(|b| *b != Builtin::Spread).ok_or_else(|| Fail::internal(format!("Spread: `{inner}` is not a builtin")))?;
    return builtin(inner, items.to_vec());
  }
  if matches!(name, Builtin::AsMap | Builtin::AsSet) {
    return Ok(match (name, args.into_iter().next()) {
      (Builtin::AsMap, Some(Value::Map(map))) => Value::Map(map.with_kind(Collection::Map)),
      (Builtin::AsSet, Some(Value::Seq(items))) => Value::Seq(items.with_kind(Collection::Set)),
      (_, other) => other.unwrap_or(Value::Undefined),
    });
  }
  let arg = |i: usize| args.get(i).ok_or_else(|| Fail::internal(format!("{name:?} takes more arguments")));
  Ok(match name {
    Builtin::Round => whole((number(name, arg(0)?)? + 0.5).floor()),
    Builtin::Floor => whole(number(name, arg(0)?)?.floor()),
    Builtin::Ceil => whole(number(name, arg(0)?)?.ceil()),
    Builtin::Abs => whole(number(name, arg(0)?)?.abs()),
    Builtin::Min | Builtin::Max => {
      let mut best: Option<f64> = None;
      for value in &args {
        let n = number(name, value)?;
        best = Some(match best {
          None => n,
          Some(b) if name == Builtin::Min => b.min(n),
          Some(b) => b.max(n),
        });
      }
      whole(best.ok_or_else(|| Fail::internal(format!("{name:?} takes a number")))?)
    }
    Builtin::ToFixed => {
      let n = number(name, arg(0)?)?;
      // JavaScript throws a RangeError past 100 digits; without the bound the
      // count saturates to `usize::MAX` and `{:.digits$}` panics the render.
      let digits = args.get(1).map(|d| number(name, d)).transpose()?.unwrap_or(0.0).max(0.0);
      if digits > 100.0 {
        return Err(Fail::internal(format!("{name:?} takes at most 100 digits, got {digits}")));
      }
      let digits = digits as usize;
      let scale = 10f64.powi(digits as i32);
      let scaled = (n.abs() * scale + 0.5).floor();
      // Below 2^53 the scaled value is a whole number an f64 holds exactly, so
      // the digits can be printed from an integer instead of through the float
      // formatter. Anything above falls back and negative zero keeps its sign
      // the way `{:.digits$}` gives it.
      match (scaled < 9007199254740992.0 && digits <= 18).then(|| scaled as u64) {
        Some(whole) => {
          let mut out = String::with_capacity(digits + 24);
          if n < 0.0 {
            out.push('-');
          }
          let unit = 10u64.pow(digits as u32);
          let mut buffer = itoa::Buffer::new();
          out.push_str(buffer.format(whole / unit));
          if digits > 0 {
            out.push('.');
            let fraction = whole % unit;
            let printed = buffer.format(fraction);
            for _ in printed.len()..digits {
              out.push('0');
            }
            out.push_str(printed);
          }
          Value::str(out)
        }
        None => {
          let rounded = scaled / scale;
          let rounded = if n < 0.0 { -rounded } else { rounded };
          Value::str(format!("{rounded:.digits$}"))
        }
      }
    }
    Builtin::Repeat => {
      let s = text(name, arg(0)?)?;
      let n = number(name, arg(1)?)?.max(0.0);
      // JavaScript throws a RangeError when the result cannot be a string;
      // unbounded this is an allocation the process does not survive.
      let len = s.len() as f64 * n;
      if !n.is_finite() || len > MAX_REPEAT as f64 {
        return Err(Fail::internal(format!("{name:?} would build {len:.0} bytes")));
      }
      Value::str(s.repeat(n as usize))
    }
    Builtin::Join => {
      let Value::Seq(items) = arg(0)? else { return Err(type_error(&format!("{name:?}"), "an array", arg(0)?)) };
      let sep = match args.get(1) {
        Some(v) => text(name, v)?.to_owned(),
        None => ",".to_owned(),
      };
      let parts: Result<Vec<String>, Fail> = items.iter().map(|v| if matches!(v, Value::Null | Value::Undefined) { Ok(String::new()) } else { stringify(v) }).collect();
      Value::str(parts?.join(&sep))
    }
    Builtin::Trim => {
      let s = text(name, arg(0)?)?;
      Value::str(match args.get(1) {
        Some(Value::Str(side)) if side.as_str() == "start" => s.trim_start_matches(js_whitespace),
        Some(Value::Str(side)) if side.as_str() == "end" => s.trim_end_matches(js_whitespace),
        _ => s.trim_matches(js_whitespace),
      }.to_owned())
    }
    Builtin::Upper => Value::str(text(name, arg(0)?)?.to_uppercase()),
    Builtin::Lower => Value::str(text(name, arg(0)?)?.to_lowercase()),
    Builtin::Includes => match arg(0)? {
      Value::Str(s) => Value::Bool(s.contains(text(name, arg(1)?)?)),
      Value::Seq(items) => {
        let wanted = arg(1)?;
        let nan = |v: &Value| is_number(v) && to_number(v).is_some_and(f64::is_nan);
        Value::Bool(items.iter().any(|item| strictly_equal(item, wanted) || (nan(item) && nan(wanted))))
      }
      other => return Err(type_error(&format!("{name:?}"), "a string or an array", other)),
    },
    Builtin::StartsWith => {
      let s = text(name, arg(0)?)?;
      let search = text(name, arg(1)?)?;
      match args.get(2) {
        None | Some(Value::Null | Value::Undefined) => Value::Bool(s.starts_with(search)),
        Some(position) => {
          let units: Vec<u16> = s.encode_utf16().collect();
          let from = (integer(name, Some(position))?.max(0.0) as usize).min(units.len());
          Value::Bool(units[from..].starts_with(&search.encode_utf16().collect::<Vec<u16>>()))
        }
      }
    }
    Builtin::EndsWith => Value::Bool(text(name, arg(0)?)?.ends_with(text(name, arg(1)?)?)),
    Builtin::Split if crate::jsregex::of(arg(1)?).is_none() => {
      let s = text(name, arg(0)?)?;
      let sep = text(name, arg(1)?)?;
      // JavaScript splits an empty separator into UTF-16 code units, so an
      // astral character becomes two surrogate halves no Rust string can hold.
      if sep.is_empty() {
        if let Some(astral) = s.chars().find(|c| *c > '\u{FFFF}') {
          return Err(Fail::internal(format!("{name:?} with an empty separator splits {astral:?}, a character outside the Basic Multilingual Plane, into UTF-16 halves; use `Array.from(s)` for its characters")));
        }
        let pieces = s.chars().count();
        if pieces > MAX_SPLIT {
          return Err(Fail::internal(format!("{name:?} would build {pieces} pieces")));
        }
        return Ok(Value::Seq(s.chars().take(split_limit(&args)).map(|c| Value::str(c.to_string())).collect()));
      }
      let pieces = s.matches(sep).count() + 1;
      if pieces > MAX_SPLIT {
        return Err(Fail::internal(format!("{name:?} would build {pieces} pieces")));
      }
      Value::Seq(s.split(sep).take(split_limit(&args)).map(Value::str).collect())
    }
    Builtin::Replace if crate::jsregex::of(arg(1)?).is_some() => {
      let s = text(name, arg(0)?)?;
      let (pattern, flags) = crate::jsregex::of(arg(1)?).expect("checked");
      let re = crate::jsregex::compiled(pattern, flags).map_err(Fail::internal)?;
      let to = stringify(arg(2)?)?;
      let mut out = String::with_capacity(s.len());
      let mut last = 0;
      let all = if flags.contains('g') { re.all(s) } else { re.first(s).map(Vec::from_iter) };
      for m in all.map_err(Fail::internal)? {
        let whole = m.whole();
        out.push_str(&s[last..whole.start]);
        crate::jsregex::expand(&mut out, &to, s, &m);
        last = whole.end;
      }
      out.push_str(&s[last..]);
      Value::str(out)
    }
    Builtin::ReplaceAll => {
      let s = text(name, arg(0)?)?;
      let from = text(name, arg(1)?)?;
      let to = stringify(arg(2)?)?;
      let mut out = String::with_capacity(s.len());
      if from.is_empty() {
        for (i, c) in s.char_indices() {
          substitute(&mut out, &to, &s[..i], "", &s[i..]);
          out.push(c);
        }
        substitute(&mut out, &to, s, "", "");
      } else {
        let mut last = 0;
        for (at, _) in s.match_indices(from) {
          out.push_str(&s[last..at]);
          substitute(&mut out, &to, &s[..at], from, &s[at + from.len()..]);
          last = at + from.len();
        }
        out.push_str(&s[last..]);
      }
      Value::str(out)
    }
    Builtin::Split => {
      let s = text(name, arg(0)?)?;
      let (pattern, flags) = crate::jsregex::of(arg(1)?).expect("checked");
      let unsticky: String = flags.chars().filter(|f| *f != 'y').collect();
      let re = crate::jsregex::compiled(pattern, &unsticky).map_err(Fail::internal)?;
      let mut out = Vec::new();
      if s.is_empty() {
        if !re.is_match(s).map_err(Fail::internal)? {
          out.push(Value::str(""));
        }
        return Ok(Value::seq(out));
      }
      let mut last = 0;
      for m in re.all(s).map_err(Fail::internal)? {
        let whole = m.whole();
        if whole.end == last || whole.start >= s.len() {
          continue;
        }
        out.push(Value::str(&s[last..whole.start]));
        for group in m.texts(s).skip(1) {
          out.push(group.map_or(Value::Null, Value::str));
        }
        last = whole.end;
        if out.len() > MAX_SPLIT {
          return Err(Fail::internal(format!("{name:?} would build more than {MAX_SPLIT} pieces")));
        }
      }
      out.push(Value::str(&s[last..]));
      out.truncate(split_limit(&args));
      Value::seq(out)
    }
    Builtin::RegexTest | Builtin::Exec | Builtin::Match | Builtin::MatchAll | Builtin::Search => {
      let (re_at, s_at) = if matches!(name, Builtin::RegexTest | Builtin::Exec) { (0, 1) } else { (1, 0) };
      let s = text(name, arg(s_at)?)?;
      let (pattern, flags) = match (crate::jsregex::of(arg(re_at)?), arg(re_at)?) {
        (Some(found), _) => found,
        (None, Value::Str(pattern)) if matches!(name, Builtin::Match | Builtin::Search) => (&**pattern, ""),
        (None, other) => return Err(type_error(&format!("{name:?}"), "a regular expression", other)),
      };
      let re = crate::jsregex::compiled(pattern, flags).map_err(Fail::internal)?;
      let groups = |m: &crate::jsregex::Found| Value::seq(m.texts(s).map(|g| g.map_or(Value::Null, Value::str)).collect::<Vec<_>>());
      match name {
        Builtin::RegexTest => Value::Bool(re.is_match(s).map_err(Fail::internal)?),
        Builtin::Exec if flags.contains('g') || flags.contains('y') => {
          return Err(Fail::internal("exec of a regular expression with `g` or `y`, which advances a `lastIndex` no value holds"));
        }
        Builtin::Exec => re.first(s).map_err(Fail::internal)?.map_or(Value::Null, |m| groups(&m)),
        Builtin::Search => whole(re.first(s).map_err(Fail::internal)?.map_or(-1.0, |m| crate::jsregex::utf16_index(s, m.whole().start) as f64)),
        Builtin::Match if flags.contains('g') => {
          let all: Vec<Value> = re.all(s).map_err(Fail::internal)?.iter().map(|m| Value::str(&s[m.whole()])).collect();
          if all.is_empty() { Value::Null } else { Value::seq(all) }
        }
        Builtin::Match => re.first(s).map_err(Fail::internal)?.map_or(Value::Null, |m| groups(&m)),
        _ => {
          if !flags.contains('g') {
            return Err(Fail::internal("matchAll of a regular expression without `g`, which JavaScript throws on"));
          }
          Value::seq(re.all(s).map_err(Fail::internal)?.iter().map(groups).collect::<Vec<_>>())
        }
      }
    }
    Builtin::Replace => {
      let s = text(name, arg(0)?)?;
      let from = text(name, arg(1)?)?;
      let to = text(name, arg(2)?)?;
      match s.find(from) {
        None => Value::str(s),
        Some(at) => {
          let (before, rest) = s.split_at(at);
          let after = &rest[from.len()..];
          let mut out = String::with_capacity(s.len() + to.len());
          out.push_str(before);
          substitute(&mut out, to, before, from, after);
          out.push_str(after);
          Value::str(out)
        }
      }
    }
    Builtin::EncodeUriComponent | Builtin::EncodeUri => {
      let s = stringify(arg(0)?)?;
      let mut out = String::with_capacity(s.len());
      for byte in s.bytes() {
        match byte {
          b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => out.push(byte as char),
          _ if name == Builtin::EncodeUri && URI_RESERVED.contains(&byte) => out.push(byte as char),
          _ => out.push_str(&format!("%{byte:02X}")),
        }
      }
      Value::str(out)
    }
    Builtin::DecodeUriComponent | Builtin::DecodeUri => {
      let s = stringify(arg(0)?)?;
      let malformed = || {
        let mut thrown = ValueMap::default();
        thrown.insert("name".to_owned(), Value::str("URIError"));
        thrown.insert("message".to_owned(), Value::str("URI malformed"));
        Fail::internal("decodeURIComponent: URI malformed").with_thrown(Value::Map(thrown))
      };
      let digit = |byte: Option<&u8>| byte.and_then(|b| (*b as char).to_digit(16));
      let bytes = s.as_bytes();
      let mut out = Vec::with_capacity(bytes.len());
      let mut i = 0;
      while i < bytes.len() {
        if bytes[i] == b'%' {
          let (Some(high), Some(low)) = (digit(bytes.get(i + 1)), digit(bytes.get(i + 2))) else { return Err(malformed()) };
          let byte = (high * 16 + low) as u8;
          if name == Builtin::DecodeUri && URI_RESERVED.contains(&byte) {
            out.extend_from_slice(&bytes[i..i + 3]);
          } else {
            out.push(byte);
          }
          i += 3;
        } else {
          out.push(bytes[i]);
          i += 1;
        }
      }
      Value::str(String::from_utf8(out).map_err(|_| malformed())?)
    }
    Builtin::LocaleNumber => {
      let n = number(name, arg(0)?)?;
      let text = stringify(&whole(n))?;
      let (sign, digits) = text.strip_prefix('-').map(|d| ("-", d)).unwrap_or(("", &text));
      let (int, frac) = digits.split_once('.').map(|(i, f)| (i, Some(f))).unwrap_or((digits, None));
      let mut grouped = String::new();
      for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
          grouped.push(',');
        }
        grouped.push(c);
      }
      Value::str(match frac {
        Some(f) => format!("{sign}{grouped}.{f}"),
        None => format!("{sign}{grouped}"),
      })
    }
    Builtin::Range => {
      let n = range_len(number(name, arg(0)?)?)?;
      Value::Seq((0..n).map(|i| Value::F64(i as f64)).collect())
    }
    Builtin::Slice => match arg(0)? {
      Value::Seq(items) => {
        let (from, to) = span_of(name, &args, items.len())?;
        Value::seq(items.iter().skip(from).take(to.saturating_sub(from)).cloned().collect::<Vec<_>>())
      }
      Value::Str(s) => {
        let units: Vec<u16> = s.encode_utf16().collect();
        let (from, to) = span_of(name, &args, units.len())?;
        Value::str(from_units(name, &units[from..to.max(from)])?)
      }
      other => return Err(type_error(&format!("{name:?}"), "an array or a string", other)),
    },
    Builtin::At => {
      let len = match arg(0)? {
        Value::Seq(items) => items.len(),
        Value::Str(s) => s.encode_utf16().count(),
        other => return Err(type_error(&format!("{name:?}"), "an array or a string", other)),
      };
      let i = integer(name, args.get(1))?;
      let i = if i < 0.0 { i + len as f64 } else { i };
      if i < 0.0 || i >= len as f64 {
        return Ok(Value::Undefined);
      }
      match arg(0)? {
        Value::Seq(items) => items.get(i as usize).cloned().unwrap_or(Value::Undefined),
        Value::Str(s) => {
          let units: Vec<u16> = s.encode_utf16().collect();
          Value::str(from_units(name, &units[i as usize..i as usize + 1])?)
        }
        _ => Value::Null,
      }
    }
    Builtin::IndexOf => match arg(0)? {
      Value::Seq(items) => {
        let from = relative(integer(name, args.get(2))?, items.len());
        let wanted = arg(1)?;
        whole(items.iter().skip(from).position(|item| strictly_equal(item, wanted)).map_or(-1.0, |i| (i + from) as f64))
      }
      Value::Str(s) => {
        let units: Vec<u16> = s.encode_utf16().collect();
        let wanted: Vec<u16> = stringify(arg(1)?)?.encode_utf16().collect();
        let from = (integer(name, args.get(2))?.max(0.0) as usize).min(units.len());
        let found = (from..=units.len()).find(|&at| units[at..].starts_with(&wanted));
        whole(found.map_or(-1.0, |at| at as f64))
      }
      other => return Err(type_error(&format!("{name:?}"), "an array or a string", other)),
    },
    Builtin::Concat => match arg(0)? {
      Value::Seq(items) => {
        let mut out = items.iter().cloned().collect::<Vec<_>>();
        for value in &args[1..] {
          match value {
            Value::Seq(more) => out.extend(more.iter().cloned()),
            other => out.push(other.clone()),
          }
        }
        Value::seq(out)
      }
      Value::Str(s) => {
        let mut out = s.to_string();
        for value in &args[1..] {
          out.push_str(&stringify(value)?);
        }
        Value::str(out)
      }
      other => return Err(type_error(&format!("{name:?}"), "an array or a string", other)),
    },
    Builtin::Reverse => match arg(0)? {
      Value::Seq(items) => Value::seq(items.iter().rev().cloned().collect::<Vec<_>>()),
      other => return Err(type_error(&format!("{name:?}"), "an array", other)),
    },
    Builtin::PadStart | Builtin::PadEnd => {
      let s = text(name, arg(0)?)?;
      let length = integer(name, args.get(1))?;
      let fill = match args.get(2) {
        None | Some(Value::Null | Value::Undefined) => " ".to_owned(),
        Some(v) => stringify(v)?,
      };
      let have = s.encode_utf16().count();
      if length <= have as f64 || fill.is_empty() {
        return Ok(Value::str(s));
      }
      if length > MAX_REPEAT as f64 {
        return Err(Fail::internal(format!("{name:?} would build {length:.0} code units")));
      }
      let needed = length as usize - have;
      let fill: Vec<u16> = fill.encode_utf16().collect();
      let pad: Vec<u16> = fill.iter().copied().cycle().take(needed).collect();
      let pad = from_units(name, &pad)?;
      Value::str(if name == Builtin::PadStart { format!("{pad}{s}") } else { format!("{s}{pad}") })
    }
    Builtin::Substring => {
      let s = text(name, arg(0)?)?;
      let units: Vec<u16> = s.encode_utf16().collect();
      let clamp = |v: f64| if v.is_nan() { 0 } else { v.clamp(0.0, units.len() as f64) as usize };
      let start = clamp(integer(name, args.get(1))?);
      let end = match args.get(2) {
        None | Some(Value::Null | Value::Undefined) => units.len(),
        Some(v) => clamp(integer(name, Some(v))?),
      };
      let (from, to) = (start.min(end), start.max(end));
      Value::str(from_units(name, &units[from..to])?)
    }
    Builtin::Json => {
      let indent = match args.get(2) {
        None | Some(Value::Null | Value::Undefined) => String::new(),
        Some(Value::Str(s)) => s.chars().take(10).collect(),
        Some(v) => " ".repeat(number(name, v)?.clamp(0.0, 10.0) as usize),
      };
      if matches!(arg(0)?, Value::Variant { tag, .. } if tag == snapfire_fsr_core::value::typed::SYMBOL) {
        return Ok(Value::Undefined);
      }
      Value::str(snapfire_fsr_core::json::stringify(arg(0)?, &indent, snapfire_fsr_core::json::Integers::Refuse).map_err(|e| Fail::internal(e.0))?)
    }
    Builtin::JsonParse => {
      let Value::Str(text) = arg(0)? else { return Err(type_error("JSON.parse", "a string", arg(0)?)) };
      match serde_json::from_str::<serde_json::Value>(text) {
        Ok(parsed) => json_value(parsed),
        Err(e) => {
          let message = format!("{e}");
          let mut thrown = ValueMap::default();
          thrown.insert("name".to_owned(), Value::str("SyntaxError"));
          thrown.insert("message".to_owned(), Value::str(message.clone()));
          return Err(Fail::internal(format!("JSON.parse: {message}")).with_thrown(Value::Map(thrown)));
        }
      }
    }
    Builtin::MinOf | Builtin::MaxOf => {
      let Value::Seq(items) = arg(0)? else { return Err(type_error(&format!("{name:?}"), "an array", arg(0)?)) };
      let mut best = if name == Builtin::MinOf { f64::INFINITY } else { f64::NEG_INFINITY };
      for item in items.iter() {
        let n = number(name, item)?;
        best = if n.is_nan() || best.is_nan() { f64::NAN } else if name == Builtin::MinOf { best.min(n) } else { best.max(n) };
      }
      whole(best)
    }
    Builtin::DateMs => {
      let ms = match arg(0)? {
        Value::Str(s) => crate::std::time::parse_iso(s).unwrap_or(f64::NAN),
        Value::Null | Value::Undefined => f64::NAN,
        other => number(name, other)?,
      };
      whole(if ms.abs() <= MAX_DATE_MS { ms.trunc() } else { f64::NAN })
    }
    Builtin::DatePart => {
      let ms = number(name, arg(0)?)?;
      if !(ms.abs() <= MAX_DATE_MS) {
        return Ok(whole(f64::NAN));
      }
      let (y, m, d) = crate::std::time::civil_from_ms(ms);
      let (hh, mm, ss, sss) = crate::std::time::clock_from_ms(ms);
      let days = (ms / 86_400_000.0).floor();
      whole(match text(name, arg(1)?)? {
        "year" => y as f64,
        "month" => (m - 1) as f64,
        "date" => d as f64,
        "day" => (days + 4.0).rem_euclid(7.0),
        "hours" => hh as f64,
        "minutes" => mm as f64,
        "seconds" => ss as f64,
        "milliseconds" => sss as f64,
        other => return Err(Fail::internal(format!("{name:?}: `{other}` is not a part of a date"))),
      })
    }
    Builtin::IsoString => {
      match snapfire_fsr_core::time::iso_string(number(name, arg(0)?)?) {
        Some(iso) => Value::str(iso),
        None => return Err(Fail::internal("toISOString of an invalid date, which JavaScript throws on")),
      }
    }
    Builtin::FromEntries => match arg(0)? {
      Value::Seq(pairs) => {
        let mut out = ValueMap::default();
        for pair in pairs.iter() {
          let Value::Seq(kv) = pair else { return Err(type_error(&format!("{name:?}"), "an array of [key, value] pairs", pair)) };
          let key = stringify(kv.get(0).unwrap_or(&Value::Null))?;
          out.insert(key, kv.get(1).cloned().unwrap_or(Value::Null));
        }
        Value::Map(out)
      }
      Value::Map(map) => Value::Map(map.clone()),
      Value::Null | Value::Undefined => Value::Map(ValueMap::default()),
      other => return Err(type_error(&format!("{name:?}"), "an array of pairs", other)),
    },
    Builtin::Unique => match arg(0)? {
      Value::Seq(items) => {
        let mut out: Vec<Value> = Vec::with_capacity(items.len());
        for item in items.iter() {
          if !out.iter().any(|seen| strictly_equal(seen, item)) {
            out.push(item.clone());
          }
        }
        Value::seq(out)
      }
      Value::Null | Value::Undefined => Value::seq(Vec::<Value>::new()),
      other => return Err(type_error(&format!("{name:?}"), "an array", other)),
    },
    Builtin::IsArray => Value::Bool(matches!(arg(0)?, Value::Seq(items) if items.kind() == Collection::Plain)),
    Builtin::WriteBack => {
      let mut tagged = ValueMap::default();
      tagged.insert(crate::render::WRITE_BACK.to_owned(), Value::seq(vec![arg(0)?.clone(), arg(1)?.clone()]));
      Value::Map(tagged)
    }
    Builtin::IsError => {
      const ERROR_NAMES: [&str; 7] = ["Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError"];
      let class = text(name, arg(1)?)?;
      Value::Bool(match arg(0)? {
        Value::Map(map) => match (map.get("name"), map.get("message")) {
          (Some(Value::Str(error_name)), Some(Value::Str(_))) => ERROR_NAMES.contains(&error_name.as_str()) && (class == "Error" || error_name.as_str() == class),
          _ => false,
        },
        _ => false,
      })
    }
    Builtin::Throw => return Err(thrown_fail(&args)),
    Builtin::Log | Builtin::LogOf => {
      let items: Vec<&Value> = match name {
        Builtin::LogOf => match arg(1)? {
          Value::Seq(items) => items.iter().collect(),
          other => return Err(type_error("LogOf", "an array", other)),
        },
        _ => args.iter().skip(1).collect(),
      };
      let line = || items.iter().map(|v| console_text(v)).collect::<Vec<_>>().join(" ");
      match args.first() {
        Some(Value::Str(method)) if method.as_str() == "debug" => tracing::debug!(target: CONSOLE_TARGET, "{}", line()),
        Some(Value::Str(method)) if method.as_str() == "warn" => tracing::warn!(target: CONSOLE_TARGET, "{}", line()),
        Some(Value::Str(method)) if method.as_str() == "error" => tracing::error!(target: CONSOLE_TARGET, "{}", line()),
        _ => tracing::info!(target: CONSOLE_TARGET, "{}", line()),
      }
      Value::Null
    }
    Builtin::IsElement => Value::Bool(crate::render::is_made(arg(0)?)),
    Builtin::CloneElement => return Err(Fail::internal("cloneElement is evaluated in a render")),
    Builtin::ComponentRef => Value::Variant { tag: crate::render::COMPONENT_VALUE.to_owned(), payload: Some(Box::new(arg(0)?.clone())) },
    Builtin::TypeOf => Value::str(match arg(0)? {
      Value::Undefined => "undefined",
      Value::Null => "object",
      Value::Bool(_) => "boolean",
      Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) => "number",
      Value::Str(_) => "string",
      Value::Variant { tag, .. } if tag == crate::render::HANDLER_VALUE || tag == FUNCTION_VALUE || tag == crate::render::COMPONENT_VALUE => "function",
      Value::Ref { .. } => "function",
      Value::Variant { tag, .. } if tag == snapfire_fsr_core::value::typed::SYMBOL => "symbol",
      _ => "object",
    }),
    Builtin::ClassNames => {
      fn add(value: &Value, out: &mut Vec<String>) -> Result<(), Fail> {
        match value {
          Value::Seq(items) => {
            for item in items.iter() {
              add(item, out)?;
            }
          }
          Value::Map(map) => {
            for (key, held) in map.iter() {
              if truthy(held) {
                out.push(key.to_string());
              }
            }
          }
          Value::Str(_) | Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) if truthy(value) => out.push(stringify(value)?),
          _ => {}
        }
        Ok(())
      }
      let mut out = Vec::new();
      add(arg(0)?, &mut out)?;
      Value::str(out.join(" "))
    }
    Builtin::HasKey => match arg(0)? {
      Value::Map(map) => Value::Bool(map.contains_key(stringify(arg(1)?)?.as_str())),
      Value::Seq(items) => Value::Bool(own_index(&stringify(arg(1)?)?, items.len())),
      Value::Str(s) => Value::Bool(own_index(&stringify(arg(1)?)?, s.encode_utf16().count())),
      other => return Err(type_error(&format!("{name:?}"), "an object or an array", other)),
    },
    Builtin::FormEncode => {
      let pairs: Vec<(String, String)> = match arg(0)? {
        Value::Map(map) => map.iter().map(|(k, v)| Ok((k.to_string(), stringify(v)?))).collect::<Result<_, Fail>>()?,
        Value::Seq(items) => items
          .iter()
          .map(|pair| match pair {
            Value::Seq(kv) => Ok((stringify(kv.get(0).unwrap_or(&Value::Null))?, stringify(kv.get(1).unwrap_or(&Value::Null))?)),
            other => Err(type_error(&format!("{name:?}"), "an array of [key, value] pairs", other)),
          })
          .collect::<Result<_, Fail>>()?,
        Value::Str(s) => form_urlencoded::parse(s.strip_prefix('?').unwrap_or(s).as_bytes())
          .map(|(k, v)| (k.into_owned(), v.into_owned()))
          .collect(),
        Value::Null | Value::Undefined => Vec::new(),
        other => return Err(type_error(&format!("{name:?}"), "an object, pairs or a query string", other)),
      };
      Value::str(pairs.iter().map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v))).collect::<Vec<_>>().join("&"))
    }
    Builtin::FormGet => {
      let key = stringify(arg(1)?)?;
      match arg(0)? {
        Value::Str(s) => form_urlencoded::parse(s.strip_prefix('?').unwrap_or(s).as_bytes()).find(|(k, _)| *k == key.as_str()).map_or(Value::Null, |(_, v)| Value::str(v.into_owned())),
        Value::Null | Value::Undefined => Value::Null,
        other => return Err(type_error(&format!("{name:?}"), "a query string", other)),
      }
    }
    Builtin::CallMethod | Builtin::GetMember => return Err(Fail::internal("a method call evaluated outside an environment")),
    Builtin::ClassOf => match arg(0)? {
      Value::Map(fields) => fields.get(CLASS_FIELD).filter(|c| matches!(c, Value::Str(_))).cloned().unwrap_or(Value::Null),
      _ => Value::Null,
    },
    Builtin::FormEntries => match arg(0)? {
      Value::Str(s) => Value::Seq(form_urlencoded::parse(s.strip_prefix('?').unwrap_or(s).as_bytes()).map(|(k, v)| Value::Seq(vec![Value::str(k.into_owned()), Value::str(v.into_owned())].into_iter().collect())).collect()),
      Value::Null | Value::Undefined => Value::Seq(Default::default()),
      other => return Err(type_error(&format!("{name:?}"), "a query string", other)),
    },
    Builtin::FormHas | Builtin::FormGetAll => {
      let key = stringify(arg(1)?)?;
      let values: Vec<Value> = match arg(0)? {
        Value::Str(s) => form_urlencoded::parse(s.strip_prefix('?').unwrap_or(s).as_bytes()).filter(|(k, _)| *k == key.as_str()).map(|(_, v)| Value::str(v.into_owned())).collect(),
        Value::Null | Value::Undefined => Vec::new(),
        other => return Err(type_error(&format!("{name:?}"), "a query string", other)),
      };
      match name {
        Builtin::FormHas => Value::Bool(!values.is_empty()),
        _ => Value::Seq(values.into_iter().collect()),
      }
    }
    Builtin::FormSet | Builtin::FormAppend | Builtin::FormDelete => {
      let mut pairs: Vec<(String, String)> = match arg(0)? {
        Value::Str(s) => form_urlencoded::parse(s.strip_prefix('?').unwrap_or(s).as_bytes()).map(|(k, v)| (k.into_owned(), v.into_owned())).collect(),
        Value::Null | Value::Undefined => Vec::new(),
        other => return Err(type_error(&format!("{name:?}"), "a query string", other)),
      };
      let key = stringify(arg(1)?)?;
      match name {
        Builtin::FormAppend => pairs.push((key, stringify(arg(2)?)?)),
        Builtin::FormDelete => pairs.retain(|(k, _)| *k != key),
        _ => {
          let value = stringify(arg(2)?)?;
          match pairs.iter().position(|(k, _)| *k == key) {
            Some(at) => {
              pairs[at].1 = value;
              let mut seen = 0;
              pairs.retain(|(k, _)| *k != key || { seen += 1; seen == 1 });
            }
            None => pairs.push((key, value)),
          }
        }
      }
      Value::str(pairs.iter().map(|(k, v)| format!("{}={}", form_encode(k), form_encode(v))).collect::<Vec<_>>().join("&"))
    }
    Builtin::Response | Builtin::ResponseJson | Builtin::ResponseRedirect | Builtin::RedirectTo => return response_builtin(name, &args),
    Builtin::RequestNew | Builtin::ResponseMember | Builtin::ResponseHeaderGet | Builtin::ResponseHeaderSet => return message_builtin(name, &args),
    Builtin::Uint8Array => Value::TypedArray(snapfire_fsr_core::TypedArray::U8(match args.first() {
      None | Some(Value::Null | Value::Undefined) => Vec::new(),
      Some(Value::Seq(items)) => items.iter().map(|item| number(name, item).map(|n| if n.is_finite() { n.trunc().rem_euclid(256.0) as u8 } else { 0 })).collect::<Result<_, _>>()?,
      Some(Value::Bytes(bytes)) => bytes.clone(),
      Some(Value::TypedArray(snapfire_fsr_core::TypedArray::U8(bytes))) => bytes.clone(),
      Some(length) => vec![0; number(name, length)?.max(0.0) as usize],
    })),
    Builtin::UrlParse => {
      let text = stringify(arg(0)?)?;
      let parsed = match args.get(1) {
        None | Some(Value::Undefined) => url::Url::parse(&text),
        Some(base) => url::Url::parse(&stringify(base)?).and_then(|base| base.join(&text)),
      };
      match parsed {
        Ok(url) => Value::str(url.to_string()),
        Err(_) => return Err(Fail::internal(format!("Invalid URL: {text}"))),
      }
    }
    Builtin::UrlPart => {
      let url = href(arg(0)?)?;
      let part = stringify(arg(1)?)?;
      Value::str(match part.as_str() {
        "href" => url.to_string(),
        "origin" => url.origin().ascii_serialization(),
        "protocol" => format!("{}:", url.scheme()),
        "username" => url.username().to_owned(),
        "password" => url.password().unwrap_or_default().to_owned(),
        "host" => match (url.host_str(), url.port()) {
          (Some(host), Some(port)) => format!("{host}:{port}"),
          (host, None) => host.unwrap_or_default().to_owned(),
          (None, Some(_)) => String::new(),
        },
        "hostname" => url.host_str().unwrap_or_default().to_owned(),
        "port" => url.port().map(|p| p.to_string()).unwrap_or_default(),
        "pathname" => url.path().to_owned(),
        "search" => url.query().filter(|q| !q.is_empty()).map(|q| format!("?{q}")).unwrap_or_default(),
        "hash" => url.fragment().filter(|f| !f.is_empty()).map(|f| format!("#{f}")).unwrap_or_default(),
        other => return Err(Fail::internal(format!("`{other}` is not a part of a URL"))),
      })
    }
    Builtin::UrlSet => {
      let mut url = href(arg(0)?)?;
      let part = stringify(arg(1)?)?;
      let value = stringify(arg(2)?)?;
      let _: Result<(), ()> = match part.as_str() {
        "href" => match url::Url::parse(&value) {
          Ok(next) => {
            url = next;
            Ok(())
          }
          Err(_) => return Err(Fail::internal(format!("Invalid URL: {value}"))),
        },
        "protocol" => url.set_scheme(value.trim_end_matches(':')),
        "username" => url.set_username(&value),
        "password" => url.set_password(Some(&value).filter(|v| !v.is_empty()).map(String::as_str)),
        "host" => match value.rsplit_once(':') {
          Some((host, port)) if port.parse::<u16>().is_ok() => url.set_host(Some(host)).map_err(|_| ()).and_then(|()| url.set_port(port.parse().ok())),
          _ => url.set_host(Some(&value)).map_err(|_| ()),
        },
        "hostname" => url.set_host(Some(&value)).map_err(|_| ()),
        "port" => url.set_port(if value.is_empty() { None } else { value.parse().ok() }),
        "pathname" => {
          url.set_path(&value);
          Ok(())
        }
        "search" => {
          let query = value.strip_prefix('?').unwrap_or(&value);
          url.set_query(Some(query).filter(|q| !q.is_empty()));
          Ok(())
        }
        "hash" => {
          let fragment = value.strip_prefix('#').unwrap_or(&value);
          url.set_fragment(Some(fragment).filter(|f| !f.is_empty()));
          Ok(())
        }
        other => return Err(Fail::internal(format!("`{other}` is not a part of a URL"))),
      };
      Value::str(url.to_string())
    }
    Builtin::LooseMatch => {
      let wanted = stringify(args.get(1).unwrap_or(&Value::Null))?;
      match arg(0)? {
        Value::Seq(items) => Value::Bool(items.iter().any(|item| stringify(item).is_ok_and(|text| text == wanted))),
        other => Value::Bool(stringify(other).is_ok_and(|text| text == wanted)),
      }
    }
    Builtin::Checked => match arg(0)? {
      Value::Seq(items) => {
        let wanted = stringify(args.get(1).unwrap_or(&Value::Null))?;
        Value::Bool(items.iter().any(|item| stringify(item).is_ok_and(|text| text == wanted)))
      }
      other => Value::Bool(truthy(other)),
    },
    Builtin::LooseEq => Value::Bool(loose_equal(arg(0)?, arg(1)?)?),
    Builtin::MapKey => Value::str(map_key(arg(0)?)?),
    Builtin::HeapOn | Builtin::Overwrite => Value::Null,
    Builtin::AsMap | Builtin::AsSet | Builtin::Spread => unreachable!("taken before the match"),
    Builtin::CopyDeep => own_copy(arg(0)?),
    Builtin::MapOf => match arg(0)? {
      Value::Seq(pairs) => {
        let mut out = ValueMap::default();
        for pair in pairs.iter() {
          let Value::Seq(kv) = pair else { return Err(type_error(&format!("{name:?}"), "an array of [key, value] pairs", pair)) };
          out.insert(map_key(kv.first().unwrap_or(&Value::Undefined))?, kv.get(1).cloned().unwrap_or(Value::Undefined));
        }
        Value::Map(out)
      }
      Value::Null | Value::Undefined => Value::Map(ValueMap::default()),
      other => return Err(type_error(&format!("{name:?}"), "an array of [key, value] pairs", other)),
    },
    Builtin::IsNullish => Value::Bool(matches!(arg(0)?, Value::Null | Value::Undefined)),
    Builtin::IsNaN => Value::Bool(match arg(0)? {
      Value::Map(_) => true,
      Value::Seq(_) => string_number(&stringify(arg(0)?)?).is_nan(),
      other => to_number(other).is_none_or(f64::is_nan),
    }),
    Builtin::ParseInt => {
      let radix = match args.get(1) {
        None | Some(Value::Null | Value::Undefined) => 0.0,
        Some(v) => to_number(v).ok_or_else(|| type_error(&format!("{name:?}"), "a number", v))?,
      };
      whole(parse_int(&stringify(arg(0)?)?, int32(radix)))
    }
    Builtin::ParseFloat => whole(parse_float(&stringify(arg(0)?)?)),
    Builtin::LastIndexOf => match arg(0)? {
      Value::Seq(items) => {
        let len = items.len() as f64;
        let from = match args.get(2) {
          None => len - 1.0,
          Some(_) => integer(name, args.get(2))?,
        };
        let from = if from < 0.0 { len + from } else { from.min(len - 1.0) };
        if from < 0.0 {
          return Ok(whole(-1.0));
        }
        let wanted = arg(1)?;
        whole(items[..=from as usize].iter().rposition(|item| strictly_equal(item, wanted)).map_or(-1.0, |i| i as f64))
      }
      Value::Str(s) => {
        let units: Vec<u16> = s.encode_utf16().collect();
        let wanted: Vec<u16> = stringify(arg(1)?)?.encode_utf16().collect();
        let from = match args.get(2) {
          None | Some(Value::Null | Value::Undefined) => f64::INFINITY,
          Some(v) => {
            let n = to_number(v).ok_or_else(|| type_error(&format!("{name:?}"), "a number", v))?;
            if n.is_nan() { f64::INFINITY } else { n.trunc() }
          }
        };
        if wanted.len() > units.len() {
          return Ok(whole(-1.0));
        }
        let start = from.clamp(0.0, (units.len() - wanted.len()) as f64) as usize;
        whole((0..=start).rev().find(|&at| units[at..].starts_with(&wanted)).map_or(-1.0, |at| at as f64))
      }
      other => return Err(type_error(&format!("{name:?}"), "an array or a string", other)),
    },
    Builtin::CharAt => {
      let units: Vec<u16> = text(name, arg(0)?)?.encode_utf16().collect();
      let at = integer(name, args.get(1))?;
      if at < 0.0 || at >= units.len() as f64 {
        return Ok(Value::str(""));
      }
      Value::str(from_units(name, &units[at as usize..at as usize + 1])?)
    }
    Builtin::CharCodeAt => {
      let units: Vec<u16> = text(name, arg(0)?)?.encode_utf16().collect();
      let at = integer(name, args.get(1))?;
      whole(if at < 0.0 || at >= units.len() as f64 { f64::NAN } else { f64::from(units[at as usize]) })
    }
    Builtin::CodePointAt => {
      let units: Vec<u16> = text(name, arg(0)?)?.encode_utf16().collect();
      let at = integer(name, args.get(1))?;
      if at < 0.0 || at >= units.len() as f64 {
        return Ok(Value::Null);
      }
      let at = at as usize;
      let point = match char::decode_utf16(units[at..].iter().copied()).next() {
        Some(Ok(c)) => u32::from(c),
        _ => u32::from(units[at]),
      };
      whole(f64::from(point))
    }
    Builtin::Graphemes => {
      use unicode_segmentation::UnicodeSegmentation;
      let input = text(name, arg(0)?)?;
      let at = match args.get(1) {
        None => None,
        Some(v) => Some(to_number(v).ok_or_else(|| type_error(&format!("{name:?}"), "a number", v))?),
      };
      let mut segments = Vec::new();
      let mut index = 0usize;
      for segment in input.graphemes(true) {
        let units = segment.encode_utf16().count();
        let made = || {
          let mut map = ValueMap::default();
          map.insert("segment".to_owned(), Value::str(segment));
          map.insert("index".to_owned(), Value::F64(index as f64));
          map.insert("input".to_owned(), Value::str(input));
          Value::Map(map)
        };
        match at {
          Some(at) if at >= index as f64 && at < (index + units) as f64 => return Ok(made()),
          Some(_) => {}
          None => segments.push(made()),
        }
        index += units;
      }
      match at {
        Some(_) => Value::Null,
        None => Value::seq(segments),
      }
    }
    Builtin::Emit => Value::Null,
    Builtin::SetAt => {
      let value = arg(2)?.clone();
      match arg(0)? {
        Value::Seq(items) => {
          let index = to_number(arg(1)?).filter(|i| i.fract() == 0.0 && (0.0..=items.len() as f64).contains(i)).ok_or_else(|| Fail::internal(format!("{name:?}: `{}` is not an index up to the array's length {}", stringify(&args[1]).unwrap_or_default(), items.len())))? as usize;
          let mut items = items.to_vec();
          if index == items.len() {
            items.push(value);
          } else {
            items[index] = value;
          }
          Value::seq(items)
        }
        Value::Map(map) => {
          let mut map = map.clone();
          map.insert(stringify(arg(1)?)?, value);
          Value::Map(map)
        }
        other => return Err(Fail::internal(format!("{name:?}: a write at a key of {}, which is not an array or an object", snapfire_fsr_core::ext::kind_name(other)))),
      }
    }
    Builtin::Radix => {
      if let (Value::Bytes(bytes), Some(Value::Str(encoding))) = (arg(0)?, args.get(1)) {
        return crate::std::crypto::encode("toString", bytes, encoding);
      }
      let radix = match args.get(1) {
        None | Some(Value::Null | Value::Undefined) => 10.0,
        v => integer(name, v)?,
      };
      if !(2.0..=36.0).contains(&radix) {
        return Err(Fail::internal(format!("{name:?}: a radix of {radix}, where JavaScript throws a RangeError for one outside 2 to 36")));
      }
      match arg(0)? {
        v if radix == 10.0 || !is_number(v) => Value::str(stringify(v)?),
        v => Value::str(radix_digits(number(name, v)?, radix as u32)),
      }
    }
    Builtin::Pow => whole(js_pow(number(name, arg(0)?)?, number(name, arg(1)?)?)),
    Builtin::Sqrt => whole(number(name, arg(0)?)?.sqrt()),
    Builtin::Math if matches!(text(name, arg(0)?)?, "hypot") => {
      let mut sum = 0.0f64;
      let mut infinite = false;
      let mut nan = false;
      for v in &args[1..] {
        let x = number(name, v)?;
        infinite |= x.is_infinite();
        nan |= x.is_nan();
        sum = sum.hypot(x);
      }
      whole(if infinite { f64::INFINITY } else if nan { f64::NAN } else { sum })
    }
    Builtin::Math => {
      use crate::jsmath as m;
      let x = number(name, arg(1)?)?;
      let uint32 = |n: f64| if n.is_finite() { n.trunc().rem_euclid(4_294_967_296.0) as u32 } else { 0 };
      whole(match text(name, arg(0)?)? {
        "sin" => m::sin(x),
        "cos" => m::cos(x),
        "tan" => m::tan(x),
        "asin" => m::asin(x),
        "acos" => m::acos(x),
        "atan" => m::atan(x),
        "atan2" => m::atan2(x, number(name, arg(2)?)?),
        "sinh" => m::sinh(x),
        "cosh" => m::cosh(x),
        "tanh" => m::tanh(x),
        "asinh" => m::asinh(x),
        "acosh" => m::acosh(x),
        "atanh" => m::atanh(x),
        "exp" => m::exp(x),
        "expm1" => m::expm1(x),
        "log" => m::log(x),
        "log2" => m::log2(x),
        "log10" => m::log10(x),
        "log1p" => m::log1p(x),
        "cbrt" => m::cbrt(x),
        "imul" => f64::from((uint32(x) as i32).wrapping_mul(uint32(number(name, arg(2)?)?) as i32)),
        "clz32" => f64::from(uint32(x).leading_zeros()),
        "fround" => f64::from(x as f32),
        "f16round" => m::f16round(x),
        other => return Err(Fail::internal(format!("`Math.{other}`, which the interpreter does not compute"))),
      })
    }
    Builtin::Trunc => whole(number(name, arg(0)?)?.trunc()),
    Builtin::Sign => {
      let n = number(name, arg(0)?)?;
      whole(if n.is_nan() || n == 0.0 { n } else { n.signum() })
    }
    Builtin::Omit => {
      let mut map = match arg(0)? {
        Value::Map(map) => map.clone(),
        Value::Null | Value::Undefined => ValueMap::default(),
        other => return Err(type_error(&format!("{name:?}"), "an object", other)),
      };
      for key in args.iter().skip(1) {
        map.shift_remove(text(name, key)?);
      }
      Value::Map(map)
    }
    Builtin::Tagged | Builtin::TaggedPayload | Builtin::IsTagged | Builtin::Settle | Builtin::ExecFrom | Builtin::TestFrom | Builtin::Substr | Builtin::IsWellFormed | Builtin::ToWellFormed | Builtin::UtcString | Builtin::SameValue | Builtin::FromCharCode | Builtin::FromCodePoint | Builtin::DateUtc | Builtin::ToPrecision | Builtin::ToExponential | Builtin::Normalize | Builtin::ToSpliced | Builtin::Splice | Builtin::ArrayWith | Builtin::Fill | Builtin::CopyWithin | Builtin::Iterate => return builtin_js(name, args),
  })
}
