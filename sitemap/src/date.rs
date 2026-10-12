use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};

/// A W3C Datetime, which the protocol allows at either whole-day or
/// whole-second precision.
///
/// `lastmod` accepts both. A news `publication_date` is conventionally a day
/// and a video `publication_date` an instant, but the schema permits either
/// everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Date {
  Day(NaiveDate),
  Instant(DateTime<FixedOffset>),
}

impl Date {
  pub fn render(&self) -> String {
    match self {
      Self::Day(day) => day.format("%Y-%m-%d").to_string(),
      Self::Instant(instant) => instant.to_rfc3339_opts(SecondsFormat::Secs, true),
    }
  }
}

impl From<NaiveDate> for Date {
  fn from(value: NaiveDate) -> Self {
    Self::Day(value)
  }
}

impl From<DateTime<FixedOffset>> for Date {
  fn from(value: DateTime<FixedOffset>) -> Self {
    Self::Instant(value)
  }
}

impl From<DateTime<Utc>> for Date {
  fn from(value: DateTime<Utc>) -> Self {
    Self::Instant(value.fixed_offset())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_day_renders_without_a_time() {
    let day = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();

    assert_eq!(Date::from(day).render(), "2026-06-01");
  }

  #[test]
  fn a_utc_instant_renders_with_a_z() {
    let instant = DateTime::from_timestamp(1_780_272_000, 0).unwrap();

    assert_eq!(Date::from(instant).render(), "2026-06-01T00:00:00Z");
  }

  #[test]
  fn an_offset_instant_keeps_its_offset() {
    let instant = DateTime::parse_from_rfc3339("2026-06-01T09:30:00+02:00").unwrap();

    assert_eq!(Date::from(instant).render(), "2026-06-01T09:30:00+02:00");
  }
}
