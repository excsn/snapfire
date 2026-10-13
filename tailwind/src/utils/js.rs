//! JavaScript's number conversions, which Tailwind's output depends on: `Number(s)`,
//! `String(n)` and `parseInt(s)`.

fn is_js_whitespace(c: char) -> bool {
  matches!(
    c,
    '\u{0009}' | '\u{000A}' | '\u{000B}' | '\u{000C}' | '\u{000D}' | '\u{0020}' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
      ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
  )
}

/// `Number(input)` for a string.
pub fn to_number(input: &str) -> f64 {
  let s = input.trim_matches(is_js_whitespace);
  if s.is_empty() {
    return 0.0;
  }

  match s {
    "Infinity" | "+Infinity" => return f64::INFINITY,
    "-Infinity" => return f64::NEG_INFINITY,
    _ => {}
  }

  let bytes = s.as_bytes();
  if bytes.len() > 2 && bytes[0] == b'0' {
    let radix = match bytes[1] {
      b'x' | b'X' => Some(16),
      b'o' | b'O' => Some(8),
      b'b' | b'B' => Some(2),
      _ => None,
    };
    if let Some(radix) = radix {
      let digits = &s[2..];
      if digits.chars().all(|c| c.is_digit(radix)) {
        let mut value = 0.0f64;
        for c in digits.chars() {
          value = value * radix as f64 + c.to_digit(radix).unwrap() as f64;
        }
        return value;
      }
      return f64::NAN;
    }
  }

  if !is_decimal_literal(bytes) {
    return f64::NAN;
  }

  s.parse::<f64>().unwrap_or(f64::NAN)
}

fn is_decimal_literal(bytes: &[u8]) -> bool {
  let mut i = 0;
  if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
    i += 1;
  }
  let int_start = i;
  while i < bytes.len() && bytes[i].is_ascii_digit() {
    i += 1;
  }
  let int_digits = i - int_start;
  let mut frac_digits = 0;
  if i < bytes.len() && bytes[i] == b'.' {
    i += 1;
    let frac_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
      i += 1;
    }
    frac_digits = i - frac_start;
  }
  if int_digits == 0 && frac_digits == 0 {
    return false;
  }
  if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
    i += 1;
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
      i += 1;
    }
    let exp_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
      i += 1;
    }
    if i == exp_start {
      return false;
    }
  }
  i == bytes.len()
}

/// `String(n)` for a number.
pub fn number_to_string(n: f64) -> String {
  if n.is_nan() {
    return "NaN".to_owned();
  }
  if n == 0.0 {
    return "0".to_owned();
  }
  if n.is_infinite() {
    return if n > 0.0 {
      "Infinity".to_owned()
    } else {
      "-Infinity".to_owned()
    };
  }
  if n < 0.0 {
    return format!("-{}", number_to_string(-n));
  }

  let exp = format!("{n:e}");
  let (mantissa, exponent) = exp.split_once('e').expect("exponent notation");
  let exponent: i32 = exponent.parse().expect("an integer exponent");
  let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
  let k = digits.len() as i32;
  let point = exponent + 1;

  if k <= point && point <= 21 {
    let mut out = digits;
    out.extend(std::iter::repeat_n('0', (point - k) as usize));
    out
  } else if 0 < point && point <= 21 {
    let (whole, frac) = digits.split_at(point as usize);
    format!("{whole}.{frac}")
  } else if -6 < point && point <= 0 {
    let mut out = String::from("0.");
    out.extend(std::iter::repeat_n('0', (-point) as usize));
    out.push_str(&digits);
    out
  } else {
    let sign = if point - 1 < 0 { '-' } else { '+' };
    let e = (point - 1).abs();
    if k == 1 {
      format!("{digits}e{sign}{e}")
    } else {
      format!("{}.{}e{sign}{e}", &digits[..1], &digits[1..])
    }
  }
}

/// `parseInt(input)` with no radix, `NaN` when nothing parses.
pub fn parse_int(input: &str) -> f64 {
  let s = input.trim_start_matches(is_js_whitespace);
  let (negative, s) = match s.as_bytes().first() {
    Some(b'-') => (true, &s[1..]),
    Some(b'+') => (false, &s[1..]),
    _ => (false, s),
  };
  let (radix, s) = if s.len() >= 2 && (s.starts_with("0x") || s.starts_with("0X")) {
    (16, &s[2..])
  } else {
    (10, s)
  };
  let mut value = 0.0f64;
  let mut any = false;
  for c in s.chars() {
    match c.to_digit(radix) {
      Some(d) => {
        value = value * radix as f64 + d as f64;
        any = true;
      }
      None => break,
    }
  }
  if !any {
    return f64::NAN;
  }
  if negative { -value } else { value }
}

/// `Number.isInteger(n)`.
pub fn is_integer(n: f64) -> bool {
  n.is_finite() && n.trunc() == n
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn numbers_print_like_javascript() {
    assert_eq!(number_to_string(50.0), "50");
    assert_eq!(number_to_string(0.07 * 100.0), "7.000000000000001");
    assert_eq!(number_to_string(1.5), "1.5");
    assert_eq!(number_to_string(1e21), "1e+21");
    assert_eq!(number_to_string(1e-7), "1e-7");
    assert_eq!(number_to_string(0.000001), "0.000001");
    assert_eq!(number_to_string(-2.5), "-2.5");
    assert_eq!(number_to_string(123456789012345680000.0), "123456789012345680000");
  }

  #[test]
  fn strings_convert_like_javascript() {
    assert_eq!(to_number(""), 0.0);
    assert_eq!(to_number(" 12 "), 12.0);
    assert_eq!(to_number("0x10"), 16.0);
    assert_eq!(to_number("1e3"), 1000.0);
    assert_eq!(to_number(".5"), 0.5);
    assert_eq!(to_number("5."), 5.0);
    assert!(to_number("inf").is_nan());
    assert!(to_number("1px").is_nan());
    assert!(to_number("-0x10").is_nan());
    assert_eq!(parse_int("40rem"), 40.0);
    assert!(parse_int("calc(1px)").is_nan());
  }
}
