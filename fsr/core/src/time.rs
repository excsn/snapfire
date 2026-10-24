//! Proleptic Gregorian calendar arithmetic over milliseconds since 1970-01-01 UTC, the instant a JavaScript `Date` holds.

pub const MS_PER_DAY: f64 = 86_400_000.0;

/// The largest instant a `Date` holds either side of 1970, in milliseconds.
pub const MAX_DATE_MS: f64 = 8.64e15;

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

/// `Date.prototype.toISOString`: a year outside 0 to 9999 as a sign and six digits. `None` for an instant no `Date` holds, where JavaScript throws.
pub fn iso_string(ms: f64) -> Option<String> {
  if !(ms.abs() <= MAX_DATE_MS) {
    return None;
  }
  let ms = ms.trunc();
  let (y, m, d) = civil_from_ms(ms);
  let (hh, mm, ss, sss) = clock_from_ms(ms);
  let year = match y {
    0..=9999 => format!("{y:04}"),
    y if y < 0 => format!("-{:06}", -y),
    y => format!("+{y:06}"),
  };
  Some(format!("{year}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{sss:03}Z"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_iso_string_is_what_javascript_writes() {
    assert_eq!(iso_string(0.0).as_deref(), Some("1970-01-01T00:00:00.000Z"));
    assert_eq!(iso_string(1_729_771_200_123.0).as_deref(), Some("2024-10-24T12:00:00.123Z"));
    assert_eq!(iso_string(-62_198_755_200_000.0).as_deref(), Some("-000001-01-01T00:00:00.000Z"));
    assert_eq!(iso_string(253_402_300_800_000.0).as_deref(), Some("+010000-01-01T00:00:00.000Z"));
    assert_eq!(iso_string(f64::NAN), None);
    assert_eq!(iso_string(8.64e15 + 1.0), None);
  }
}
