use std::cmp::Ordering;
use std::sync::LazyLock;

use regex::Regex;

use crate::utils::js::{parse_int, to_number};

/// Compares two strings with runs of digits compared as numbers.
pub fn compare(a: &str, z: &str) -> Ordering {
  let a16: Vec<u16> = a.encode_utf16().collect();
  let z16: Vec<u16> = z.encode_utf16().collect();
  let digit = |c: u16| (b'0' as u16..=b'9' as u16).contains(&c);
  let min = a16.len().min(z16.len());

  let mut i = 0;
  while i < min {
    let ac = a16[i];
    let zc = z16[i];
    if digit(ac) && digit(zc) {
      let mut a_end = i + 1;
      while a_end < a16.len() && digit(a16[a_end]) {
        a_end += 1;
      }
      let mut z_end = i + 1;
      while z_end < z16.len() && digit(z16[z_end]) {
        z_end += 1;
      }
      let a_num = String::from_utf16_lossy(&a16[i..a_end]);
      let z_num = String::from_utf16_lossy(&z16[i..z_end]);
      let diff = to_number(&a_num) - to_number(&z_num);
      if diff != 0.0 && !diff.is_nan() {
        return if diff < 0.0 { Ordering::Less } else { Ordering::Greater };
      }
      match a_num.cmp(&z_num) {
        Ordering::Equal => {}
        other => return other,
      }
      i += 1;
      continue;
    }
    if ac == zc {
      i += 1;
      continue;
    }
    return ac.cmp(&zc);
  }
  a16.len().cmp(&z16.len())
}

static DIGITS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[0-9.]+").unwrap());

/// Orders breakpoint values by unit or function name, then by number.
pub fn compare_breakpoints(a: &str, z: &str, ascending: bool) -> Ordering {
  if a == z {
    return Ordering::Equal;
  }
  let bucket = |v: &str| -> String {
    match v.find('(') {
      None => DIGITS.replace_all(v, "").into_owned(),
      Some(i) => v[..i].to_owned(),
    }
  };
  let a_bucket = bucket(a);
  let z_bucket = bucket(z);

  if a_bucket != z_bucket {
    return if a_bucket < z_bucket {
      Ordering::Less
    } else {
      Ordering::Greater
    };
  }
  let order = if ascending {
    parse_int(a) - parse_int(z)
  } else {
    parse_int(z) - parse_int(a)
  };
  if order.is_nan() {
    return if a < z { Ordering::Less } else { Ordering::Greater };
  }
  if order < 0.0 {
    Ordering::Less
  } else if order > 0.0 {
    Ordering::Greater
  } else {
    Ordering::Equal
  }
}

/// `64rem` as `(64, Some("rem"))`, a bare number with no unit.
pub fn dimension(input: &str) -> Option<(f64, Option<String>)> {
  static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^([-+]?(?:[0-9]*\.)?[0-9]+)([a-z]+|%)?$").unwrap());
  let caps = RE.captures(input)?;
  let value = to_number(caps.get(1)?.as_str());
  if value.is_nan() {
    return None;
  }
  Some((value, caps.get(2).map(|m| m.as_str().to_owned())))
}
