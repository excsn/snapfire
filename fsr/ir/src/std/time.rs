//! `time`: instants as milliseconds since the epoch, always UTC, so the
//! viewer's zone cannot split the server from the browser. Units are `ms`,
//! `s`, `m`, `h` and `d`.

use snapfire_fsr_core::Value;

use crate::ext::{number, text, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("time.format", Reach::Render, format);
  extensions.register("time.add", Reach::Render, add);
  extensions.register("time.diff", Reach::Render, diff);
  extensions.register("time.parse", Reach::Render, |_, args| Ok(parse_iso(text("time.parse", args, 0)?).map(Value::F64).unwrap_or(Value::Null)));
  extensions.register("time.now", Reach::Body, |ambient, _| Ok(Value::Int(ambient.now)));
}

const MS_PER_DAY: f64 = 86_400_000.0;

fn unit_ms(what: &str, unit: &str) -> Result<f64, Fail> {
  Ok(match unit {
    "ms" => 1.0,
    "s" => 1_000.0,
    "m" => 60_000.0,
    "h" => 3_600_000.0,
    "d" => MS_PER_DAY,
    other => return Err(Fail::internal(format!("{what}: `{other}` is not a unit; ms, s, m, h or d"))),
  })
}

/// `time.add(when, amount, unit)`: the instant `amount` units later.
fn add(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "time.add";
  let ms = number(what, args, 0)?;
  let amount = number(what, args, 1)?;
  Ok(Value::F64(ms + amount * unit_ms(what, text(what, args, 2)?)?))
}

/// `time.diff(later, earlier, unit)`: the difference in units, fractional.
fn diff(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "time.diff";
  let a = number(what, args, 0)?;
  let b = number(what, args, 1)?;
  Ok(Value::F64((a - b) / unit_ms(what, text(what, args, 2)?)?))
}

/// Days since 1970-01-01 for a proleptic Gregorian date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
  let y = if m <= 2 { y - 1 } else { y };
  let era = if y >= 0 { y } else { y - 399 } / 400;
  let yoe = y - era * 400;
  let mp = (m as i64 + 9) % 12;
  let doy = (153 * mp + 2) / 5 + d as i64 - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146_097 + doe - 719_468
}

/// The proleptic Gregorian date of a day count since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
  let z = z + 719_468;
  let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
  let doe = z - era * 146_097;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let y = yoe + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
  (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The UTC calendar date of an instant.
pub fn civil_from_ms(ms: f64) -> (i64, u32, u32) {
  civil_from_days((ms / MS_PER_DAY).floor() as i64)
}

/// The UTC clock fields of an instant: hours, minutes, seconds, milliseconds.
pub fn clock_from_ms(ms: f64) -> (u32, u32, u32, u32) {
  let of_day = ms.rem_euclid(MS_PER_DAY) as u64;
  ((of_day / 3_600_000) as u32, (of_day / 60_000 % 60) as u32, (of_day / 1_000 % 60) as u32, (of_day % 1_000) as u32)
}

/// `time.format(when, pattern)`: `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and
/// `SSS` replaced in UTC, every other character kept.
fn format(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "time.format";
  let ms = number(what, args, 0)?;
  let pattern = text(what, args, 1)?;
  Ok(Value::Str(format_utc(ms, pattern)))
}

pub fn format_utc(ms: f64, pattern: &str) -> String {
  let (y, m, d) = civil_from_ms(ms);
  let (hh, mm, ss, sss) = clock_from_ms(ms);
  let mut out = String::with_capacity(pattern.len());
  let mut rest = pattern;
  while !rest.is_empty() {
    let (token, len) = if rest.starts_with("YYYY") {
      (format!("{y:04}"), 4)
    } else if rest.starts_with("SSS") {
      (format!("{sss:03}"), 3)
    } else if rest.starts_with("MM") {
      (format!("{m:02}"), 2)
    } else if rest.starts_with("DD") {
      (format!("{d:02}"), 2)
    } else if rest.starts_with("HH") {
      (format!("{hh:02}"), 2)
    } else if rest.starts_with("mm") {
      (format!("{mm:02}"), 2)
    } else if rest.starts_with("ss") {
      (format!("{ss:02}"), 2)
    } else {
      let c = rest.chars().next().expect("not empty");
      (c.to_string(), c.len_utf8())
    };
    out.push_str(&token);
    rest = &rest[len..];
  }
  out
}

/// `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`;
/// a date alone or a `Z` is UTC, a bare time is UTC too. `None` for
/// anything else, where `Date.parse` in the browser is also refused by the
/// browser half.
pub fn parse_iso(text: &str) -> Option<f64> {
  let text = text.trim();
  let bytes = text.as_bytes();
  let num = |from: usize, len: usize| -> Option<i64> {
    let slice = bytes.get(from..from + len)?;
    if !slice.iter().all(u8::is_ascii_digit) {
      return None;
    }
    std::str::from_utf8(slice).ok()?.parse().ok()
  };
  let y = num(0, 4)?;
  if bytes.get(4) != Some(&b'-') || bytes.get(7) != Some(&b'-') {
    return None;
  }
  let m = num(5, 2)? as u32;
  let d = num(8, 2)? as u32;
  if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
    return None;
  }
  let mut ms = days_from_civil(y, m, d) as f64 * MS_PER_DAY;
  let mut i = 10;
  if bytes.get(i) == Some(&b'T') || bytes.get(i) == Some(&b' ') {
    let hh = num(i + 1, 2)?;
    if bytes.get(i + 3) != Some(&b':') {
      return None;
    }
    let mm = num(i + 4, 2)?;
    i += 6;
    let mut ss = 0;
    let mut frac = 0.0;
    if bytes.get(i) == Some(&b':') {
      ss = num(i + 1, 2)?;
      i += 3;
      if bytes.get(i) == Some(&b'.') {
        let start = i + 1;
        let mut end = start;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
          end += 1;
        }
        if end == start {
          return None;
        }
        let digits = &text[start..end];
        frac = digits.parse::<f64>().ok()? / 10f64.powi(digits.len() as i32) * 1000.0;
        i = end;
      }
    }
    if hh > 24 || mm > 59 || ss > 59 {
      return None;
    }
    ms += (hh * 3_600_000 + mm * 60_000 + ss * 1_000) as f64 + frac.floor();
    match bytes.get(i) {
      None | Some(b'Z') => {}
      Some(sign @ (b'+' | b'-')) => {
        let oh = num(i + 1, 2)?;
        if bytes.get(i + 3) != Some(&b':') {
          return None;
        }
        let om = num(i + 4, 2)?;
        let offset = (oh * 3_600_000 + om * 60_000) as f64;
        ms += if *sign == b'+' { -offset } else { offset };
        i += 6;
        if i != bytes.len() {
          return None;
        }
      }
      _ => return None,
    }
    if bytes.get(i) == Some(&b'Z') && i + 1 != bytes.len() {
      return None;
    }
  } else if i != bytes.len() {
    return None;
  }
  Some(ms)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn civil_round_trips() {
    for days in [-719_468, -1, 0, 1, 19_600, 20_701, 2_932_896] {
      let (y, m, d) = civil_from_days(days);
      assert_eq!(days_from_civil(y, m, d), days, "{y}-{m}-{d}");
    }
    assert_eq!(civil_from_days(20_701), (2026, 9, 5));
  }

  #[test]
  fn parses_the_iso_subset() {
    let day = 20_701.0 * MS_PER_DAY;
    assert_eq!(parse_iso("2026-09-05"), Some(day));
    assert_eq!(parse_iso("2026-09-05T16:41:07Z"), Some(day + 60_067_000.0));
    assert_eq!(parse_iso("2026-09-05T16:41:07.250Z"), Some(day + 60_067_250.0));
    assert_eq!(parse_iso("2026-09-05T18:41+02:00"), Some(day + 60_060_000.0));
    assert_eq!(parse_iso("2026-09-05T16:41:07-01:30"), Some(day + 60_067_000.0 + 5_400_000.0));
    assert_eq!(parse_iso("Sep 5 2026"), None);
    assert_eq!(parse_iso("2026-13-05"), None);
    assert_eq!(parse_iso("2026-09-05T16"), None);
  }

  #[test]
  fn formats_in_utc() {
    let ms = 20_701.0 * MS_PER_DAY + 60_067_250.0;
    assert_eq!(format_utc(ms, "YYYY-MM-DD HH:mm:ss.SSS"), "2026-09-05 16:41:07.250");
    assert_eq!(format_utc(ms, "DD/MM/YYYY à HH:mm"), "05/09/2026 à 16:41");
  }
}
