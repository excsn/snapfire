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
  /// A W3C Datetime as a page or a feed writes one: `2026-06-01`, or an
  /// instant with an offset to the minute or past it, `2026-06-01T09:30+02:00`
  /// or `2026-06-01T09:30:00.5Z`. `None` for anything else, a year or a month
  /// alone included.
  pub fn parse(text: &str) -> Option<Self> {
    let text = text.trim();
    if let Ok(day) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
      return Some(Self::Day(day));
    }
    DateTime::parse_from_rfc3339(text)
      .or_else(|_| DateTime::parse_from_str(text, "%Y-%m-%dT%H:%M%:z"))
      .or_else(|_| DateTime::parse_from_str(&text.replace('Z', "+00:00"), "%Y-%m-%dT%H:%M%:z"))
      .ok()
      .map(Self::Instant)
  }

  /// An HTTP date, the form a `Last-Modified` header carries:
  /// `Mon, 01 Jun 2026 09:30:00 GMT`.
  pub fn parse_http(text: &str) -> Option<Self> {
    DateTime::parse_from_rfc2822(text.trim()).ok().map(Self::Instant)
  }

  /// The instant this date starts at, a day at midnight UTC, for ordering
  /// dates of either precision.
  pub fn as_instant(&self) -> DateTime<FixedOffset> {
    match self {
      Self::Day(day) => day.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc().fixed_offset(),
      Self::Instant(instant) => *instant,
    }
  }

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
  fn a_page_date_parses_at_either_precision() {
    assert_eq!(Date::parse("2026-06-01").unwrap().render(), "2026-06-01");
    assert_eq!(Date::parse("2026-06-01T09:30:00+02:00").unwrap().render(), "2026-06-01T09:30:00+02:00");
    assert_eq!(Date::parse("2026-06-01T09:30:00.250Z").unwrap().render(), "2026-06-01T09:30:00Z");
    assert_eq!(Date::parse("2026-06-01T09:30+02:00").unwrap().render(), "2026-06-01T09:30:00+02:00");
    assert_eq!(Date::parse("2026-06-01T09:30Z").unwrap().render(), "2026-06-01T09:30:00Z");
    for refused in ["2026-06", "2026", "June 1st", "2026-06-01T09:30:00", ""] {
      assert_eq!(Date::parse(refused), None, "{refused}");
    }
  }

  #[test]
  fn an_http_date_parses_as_an_instant() {
    assert_eq!(Date::parse_http("Mon, 01 Jun 2026 09:30:00 GMT").unwrap().render(), "2026-06-01T09:30:00Z");
    assert_eq!(Date::parse_http("2026-06-01"), None);
  }

  #[test]
  fn a_day_orders_as_its_midnight() {
    let day = Date::parse("2026-06-01").unwrap();
    let later = Date::parse("2026-06-01T00:00:01Z").unwrap();
    assert!(later.as_instant() > day.as_instant());
  }

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
