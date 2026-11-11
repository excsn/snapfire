//! Temporal as JavaScript's `Temporal` global answers it, over `temporal_rs`, the implementation V8 runs. A value is its kind and its canonical string, which every operation parses and every result writes back, so a caller holds Temporal values as plain data.

use std::cmp::Ordering;

use temporal_rs::options::{DifferenceSettings, DisplayCalendar, DisplayOffset, DisplayTimeZone, Overflow, RoundingIncrement, RoundingOptions, ToStringRoundingOptions, Unit};
use temporal_rs::partial::{PartialDuration, PartialTime};
use temporal_rs::provider::TimeZoneProvider;
use temporal_rs::{Calendar, Duration, Instant, PlainDate, PlainDateTime, PlainMonthDay, PlainTime, PlainYearMonth, TimeZone, ZonedDateTime};

use crate::Error;

/// A Temporal type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  Instant,
  PlainDate,
  PlainTime,
  PlainDateTime,
  ZonedDateTime,
  Duration,
  PlainYearMonth,
  PlainMonthDay,
}

impl Kind {
  /// The type's name under `Temporal`, `PlainDate`.
  pub fn name(self) -> &'static str {
    match self {
      Kind::Instant => "Instant",
      Kind::PlainDate => "PlainDate",
      Kind::PlainTime => "PlainTime",
      Kind::PlainDateTime => "PlainDateTime",
      Kind::ZonedDateTime => "ZonedDateTime",
      Kind::Duration => "Duration",
      Kind::PlainYearMonth => "PlainYearMonth",
      Kind::PlainMonthDay => "PlainMonthDay",
    }
  }

  /// The type `name` names, `None` for one this crate does not hold.
  pub fn of(name: &str) -> Option<Kind> {
    [Kind::Instant, Kind::PlainDate, Kind::PlainTime, Kind::PlainDateTime, Kind::ZonedDateTime, Kind::Duration, Kind::PlainYearMonth, Kind::PlainMonthDay].into_iter().find(|k| k.name() == name)
  }
}

/// An argument as JavaScript passes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
  Undefined,
  Null,
  Bool(bool),
  Num(f64),
  BigInt(i128),
  Str(String),
  /// An object's own fields in order.
  Obj(Vec<(String, Arg)>),
  /// A Temporal value: its kind and canonical string.
  Temporal(Kind, String),
}

/// What an operation answers.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
  Undefined,
  Bool(bool),
  Num(f64),
  BigInt(i128),
  Str(String),
  Temporal(Kind, String),
}

fn range(e: impl std::fmt::Display) -> Error {
  Error::Range(e.to_string())
}

fn type_error(message: impl Into<String>) -> Error {
  Error::Type(message.into())
}

fn arg(args: &[Arg], i: usize) -> &Arg {
  args.get(i).unwrap_or(&Arg::Undefined)
}

fn field<'a>(fields: &'a [(String, Arg)], name: &str) -> Option<&'a Arg> {
  fields.iter().find(|(k, _)| k == name).map(|(_, v)| v).filter(|v| !matches!(v, Arg::Undefined))
}

fn number(what: &str, a: &Arg) -> Result<f64, Error> {
  match a {
    Arg::Num(n) => Ok(*n),
    Arg::BigInt(n) => Ok(*n as f64),
    Arg::Str(s) => s.trim().parse::<f64>().map_err(|_| range(format!("{what}: `{s}` is not a number"))),
    Arg::Bool(b) => Ok(f64::from(u8::from(*b))),
    Arg::Undefined => Err(range(format!("{what} is required"))),
    _ => Err(type_error(format!("{what} must be a number"))),
  }
}

/// A field JavaScript reads with `ToIntegerWithTruncation`: finite, truncated.
fn integer(what: &str, a: &Arg) -> Result<i64, Error> {
  let n = number(what, a)?;
  if !n.is_finite() {
    return Err(range(format!("{what} must be finite")));
  }
  Ok(n.trunc() as i64)
}

fn int_field<T: TryFrom<i64>>(fields: &[(String, Arg)], name: &str) -> Result<Option<T>, Error> {
  match field(fields, name) {
    None => Ok(None),
    Some(a) => {
      let n = integer(name, a)?;
      T::try_from(n).map(Some).map_err(|_| range(format!("{name} {n} is out of range")))
    }
  }
}

fn text<'a>(what: &str, a: &'a Arg) -> Result<&'a str, Error> {
  match a {
    Arg::Str(s) => Ok(s),
    _ => Err(type_error(format!("{what} must be a string"))),
  }
}

fn option_text<'a>(options: &'a Arg, name: &str) -> Result<Option<&'a str>, Error> {
  match options {
    Arg::Obj(fields) => field(fields, name).map(|a| text(name, a)).transpose(),
    Arg::Undefined => Ok(None),
    _ => Err(type_error("options must be an object")),
  }
}

fn parsed<T: std::str::FromStr>(name: &str, value: Option<&str>) -> Result<Option<T>, Error> {
  value.map(|v| v.parse::<T>().map_err(|_| range(format!("`{v}` is not a valid {name}")))).transpose()
}

fn overflow(options: &Arg) -> Result<Option<Overflow>, Error> {
  parsed("overflow", option_text(options, "overflow")?)
}

fn increment(options: &Arg) -> Result<Option<RoundingIncrement>, Error> {
  match options {
    Arg::Obj(fields) => field(fields, "roundingIncrement").map(|a| RoundingIncrement::try_from(number("roundingIncrement", a)?).map_err(range)).transpose(),
    _ => Ok(None),
  }
}

fn difference(options: &Arg) -> Result<DifferenceSettings, Error> {
  let mut settings = DifferenceSettings::default();
  settings.largest_unit = parsed("unit", option_text(options, "largestUnit")?)?;
  settings.smallest_unit = parsed("unit", option_text(options, "smallestUnit")?)?;
  settings.rounding_mode = parsed("roundingMode", option_text(options, "roundingMode")?)?;
  settings.increment = increment(options)?;
  Ok(settings)
}

/// `round`'s argument: a unit string read as `{ smallestUnit }` or the options object.
fn rounding(given: &Arg) -> Result<RoundingOptions, Error> {
  let mut options = RoundingOptions::default();
  match given {
    Arg::Str(unit) => options.smallest_unit = parsed("unit", Some(unit))?,
    Arg::Obj(_) => {
      options.largest_unit = parsed("unit", option_text(given, "largestUnit")?)?.or(options.largest_unit);
      options.smallest_unit = parsed("unit", option_text(given, "smallestUnit")?)?;
      options.rounding_mode = parsed("roundingMode", option_text(given, "roundingMode")?)?;
      options.increment = increment(given)?;
    }
    _ => return Err(type_error("round takes a unit or an options object")),
  }
  Ok(options)
}

fn to_string_options(options: &Arg) -> Result<ToStringRoundingOptions, Error> {
  let mut out = ToStringRoundingOptions::default();
  out.smallest_unit = parsed("unit", option_text(options, "smallestUnit")?)?;
  out.rounding_mode = parsed("roundingMode", option_text(options, "roundingMode")?)?;
  if let Arg::Obj(fields) = options {
    if let Some(digits) = field(fields, "fractionalSecondDigits") {
      out.precision = match digits {
        Arg::Str(s) if s == "auto" => temporal_rs::parsers::Precision::Auto,
        other => temporal_rs::parsers::Precision::Digit(u8::try_from(integer("fractionalSecondDigits", other)?).ok().filter(|d| *d <= 9).ok_or_else(|| range("fractionalSecondDigits is 0 to 9 or `auto`"))?),
      };
    }
  }
  Ok(out)
}

fn calendar_display(options: &Arg) -> Result<DisplayCalendar, Error> {
  Ok(parsed("calendarName", option_text(options, "calendarName")?)?.unwrap_or_default())
}

fn with_provider<T>(f: impl FnOnce(&dyn TimeZoneProvider) -> Result<T, Error>) -> Result<T, Error> {
  let source = crate::source()?;
  crate::provide(source, f)?
}

fn time_zone(id: &str, provider: &dyn TimeZoneProvider) -> Result<TimeZone, Error> {
  TimeZone::try_from_str_with_provider(id, provider).map_err(|_| range(format!("`{id}` is not a time zone")))
}

fn calendar(a: &Arg) -> Result<Calendar, Error> {
  match a {
    Arg::Undefined => Ok(Calendar::ISO),
    Arg::Str(id) => Calendar::try_from_utf8(id.as_bytes()).map_err(|_| range(format!("`{id}` is not a calendar"))),
    _ => Err(type_error("a calendar is a string")),
  }
}

// ---- canonical strings ----

fn date_text(d: &PlainDate) -> String {
  d.to_ixdtf_string(DisplayCalendar::Auto)
}

fn time_text(t: &PlainTime) -> Result<String, Error> {
  t.to_ixdtf_string(ToStringRoundingOptions::default()).map_err(range)
}

fn date_time_text(dt: &PlainDateTime) -> Result<String, Error> {
  dt.to_ixdtf_string(ToStringRoundingOptions::default(), DisplayCalendar::Auto).map_err(range)
}

fn instant_text(i: &Instant, provider: &dyn TimeZoneProvider) -> Result<String, Error> {
  i.to_ixdtf_string_with_provider(None, ToStringRoundingOptions::default(), provider).map_err(range)
}

fn zoned_text(z: &ZonedDateTime, provider: &dyn TimeZoneProvider) -> Result<String, Error> {
  z.to_ixdtf_string_with_provider(DisplayOffset::Auto, DisplayTimeZone::Auto, DisplayCalendar::Auto, ToStringRoundingOptions::default(), provider).map_err(range)
}

fn answer_date(d: PlainDate) -> Answer {
  Answer::Temporal(Kind::PlainDate, date_text(&d))
}

fn answer_time(t: PlainTime) -> Result<Answer, Error> {
  Ok(Answer::Temporal(Kind::PlainTime, time_text(&t)?))
}

fn answer_date_time(dt: PlainDateTime) -> Result<Answer, Error> {
  Ok(Answer::Temporal(Kind::PlainDateTime, date_time_text(&dt)?))
}

fn answer_duration(d: Duration) -> Answer {
  Answer::Temporal(Kind::Duration, d.to_string())
}

fn answer_instant(i: Instant) -> Result<Answer, Error> {
  with_provider(|p| Ok(Answer::Temporal(Kind::Instant, instant_text(&i, p)?)))
}

fn answer_year_month(v: PlainYearMonth) -> Answer {
  Answer::Temporal(Kind::PlainYearMonth, v.to_ixdtf_string(DisplayCalendar::Auto))
}

fn answer_month_day(v: PlainMonthDay) -> Answer {
  Answer::Temporal(Kind::PlainMonthDay, v.to_ixdtf_string(DisplayCalendar::Auto))
}

fn answer_zoned(z: ZonedDateTime) -> Result<Answer, Error> {
  with_provider(|p| Ok(Answer::Temporal(Kind::ZonedDateTime, zoned_text(&z, p)?)))
}

// ---- reading arguments as values ----

fn calendar_fields(fields: &[(String, Arg)]) -> Result<temporal_rs::fields::CalendarFields, Error> {
  let mut out = temporal_rs::fields::CalendarFields::default();
  out.year = int_field(fields, "year")?;
  out.month = int_field(fields, "month")?;
  out.day = int_field(fields, "day")?;
  if let Some(code) = field(fields, "monthCode") {
    let code = text("monthCode", code)?;
    out.month_code = Some(temporal_rs::MonthCode::try_from_utf8(code.as_bytes()).map_err(|_| range(format!("`{code}` is not a month code")))?);
  }
  Ok(out)
}

fn partial_time(fields: &[(String, Arg)]) -> Result<PartialTime, Error> {
  let mut out = PartialTime::default();
  out.hour = int_field(fields, "hour")?;
  out.minute = int_field(fields, "minute")?;
  out.second = int_field(fields, "second")?;
  out.millisecond = int_field(fields, "millisecond")?;
  out.microsecond = int_field(fields, "microsecond")?;
  out.nanosecond = int_field(fields, "nanosecond")?;
  Ok(out)
}

fn plain_date(a: &Arg, options: &Arg) -> Result<PlainDate, Error> {
  match a {
    Arg::Str(s) => PlainDate::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDate, s) => PlainDate::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDateTime, s) => Ok(PlainDateTime::from_utf8(s.as_bytes()).map_err(range)?.to_plain_date()),
    Arg::Temporal(Kind::ZonedDateTime, s) => Ok(zoned(&Arg::Str(s.clone()))?.to_plain_date()),
    Arg::Obj(fields) => {
      let partial = temporal_rs::partial::PartialDate { calendar_fields: calendar_fields(fields)?, calendar: calendar(field(fields, "calendar").unwrap_or(&Arg::Undefined))? };
      PlainDate::from_partial(partial, overflow(options)?).map_err(range)
    }
    _ => Err(type_error("a PlainDate is a string, an object or a Temporal date")),
  }
}

fn year_month(a: &Arg, options: &Arg) -> Result<PlainYearMonth, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::PlainYearMonth, s) => PlainYearMonth::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDate, s) => PlainDate::from_utf8(s.as_bytes()).map_err(range)?.to_plain_year_month().map_err(range),
    Arg::Obj(fields) => {
      let mut wanted = temporal_rs::fields::YearMonthCalendarFields::default();
      let given = calendar_fields(fields)?;
      (wanted.year, wanted.month, wanted.month_code) = (given.year, given.month, given.month_code);
      let partial = temporal_rs::partial::PartialYearMonth { calendar_fields: wanted, calendar: calendar(field(fields, "calendar").unwrap_or(&Arg::Undefined))? };
      PlainYearMonth::from_partial(partial, overflow(options)?).map_err(range)
    }
    _ => Err(type_error("a PlainYearMonth is a string, an object or a Temporal year and month")),
  }
}

fn month_day(a: &Arg, options: &Arg) -> Result<PlainMonthDay, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::PlainMonthDay, s) => PlainMonthDay::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDate, s) => PlainDate::from_utf8(s.as_bytes()).map_err(range)?.to_plain_month_day().map_err(range),
    Arg::Obj(fields) => {
      let partial = temporal_rs::partial::PartialDate { calendar_fields: calendar_fields(fields)?, calendar: calendar(field(fields, "calendar").unwrap_or(&Arg::Undefined))? };
      PlainMonthDay::from_partial(partial, overflow(options)?).map_err(range)
    }
    _ => Err(type_error("a PlainMonthDay is a string, an object or a Temporal month and day")),
  }
}

fn plain_time(a: &Arg, options: &Arg) -> Result<PlainTime, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::PlainTime, s) => PlainTime::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDateTime, s) => Ok(PlainDateTime::from_utf8(s.as_bytes()).map_err(range)?.to_plain_time()),
    Arg::Temporal(Kind::ZonedDateTime, s) => Ok(zoned(&Arg::Str(s.clone()))?.to_plain_time()),
    Arg::Obj(fields) => PlainTime::from_partial(partial_time(fields)?, overflow(options)?).map_err(range),
    _ => Err(type_error("a PlainTime is a string, an object or a Temporal time")),
  }
}

fn plain_date_time(a: &Arg, options: &Arg) -> Result<PlainDateTime, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::PlainDateTime, s) => PlainDateTime::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::PlainDate, s) => PlainDate::from_utf8(s.as_bytes()).map_err(range)?.to_plain_date_time(None).map_err(range),
    Arg::Temporal(Kind::ZonedDateTime, s) => Ok(zoned(&Arg::Str(s.clone()))?.to_plain_date_time()),
    Arg::Obj(fields) => {
      let partial = temporal_rs::partial::PartialDateTime {
        fields: temporal_rs::fields::DateTimeFields { calendar_fields: calendar_fields(fields)?, time: partial_time(fields)? },
        calendar: calendar(field(fields, "calendar").unwrap_or(&Arg::Undefined))?,
      };
      PlainDateTime::from_partial(partial, overflow(options)?).map_err(range)
    }
    _ => Err(type_error("a PlainDateTime is a string, an object or a Temporal date-time")),
  }
}

fn instant(a: &Arg) -> Result<Instant, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::Instant, s) => Instant::from_utf8(s.as_bytes()).map_err(range),
    Arg::Temporal(Kind::ZonedDateTime, s) => Ok(zoned(&Arg::Str(s.clone()))?.to_instant()),
    _ => Err(type_error("an Instant is a string or a Temporal instant")),
  }
}

fn zoned(a: &Arg) -> Result<ZonedDateTime, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::ZonedDateTime, s) => with_provider(|p| ZonedDateTime::from_utf8_with_provider(s.as_bytes(), temporal_rs::options::Disambiguation::Compatible, temporal_rs::options::OffsetDisambiguation::Reject, p).map_err(range)),
    _ => Err(type_error("a ZonedDateTime is a string or a Temporal zoned date-time")),
  }
}

fn duration(a: &Arg) -> Result<Duration, Error> {
  match a {
    Arg::Str(s) | Arg::Temporal(Kind::Duration, s) => Duration::from_utf8(s.as_bytes()).map_err(range),
    Arg::Obj(fields) => {
      let mut partial = PartialDuration::default();
      partial.years = int_field(fields, "years")?;
      partial.months = int_field(fields, "months")?;
      partial.weeks = int_field(fields, "weeks")?;
      partial.days = int_field(fields, "days")?;
      partial.hours = int_field(fields, "hours")?;
      partial.minutes = int_field(fields, "minutes")?;
      partial.seconds = int_field(fields, "seconds")?;
      partial.milliseconds = int_field(fields, "milliseconds")?;
      partial.microseconds = field(fields, "microseconds").map(|a| integer("microseconds", a).map(i128::from)).transpose()?;
      partial.nanoseconds = field(fields, "nanoseconds").map(|a| integer("nanoseconds", a).map(i128::from)).transpose()?;
      Duration::from_partial_duration(partial).map_err(range)
    }
    _ => Err(type_error("a Duration is a string, an object or a Temporal duration")),
  }
}

fn relative_to(options: &Arg) -> Result<Option<temporal_rs::options::RelativeTo>, Error> {
  let Arg::Obj(fields) = options else { return Ok(None) };
  match field(fields, "relativeTo") {
    None => Ok(None),
    Some(a @ Arg::Temporal(Kind::ZonedDateTime, _)) => Ok(Some(zoned(a)?.into())),
    Some(Arg::Str(s)) if s.contains('[') => Ok(Some(zoned(&Arg::Str(s.clone()))?.into())),
    Some(a) => Ok(Some(plain_date(a, &Arg::Undefined)?.into())),
  }
}

fn ordering(o: Ordering) -> Answer {
  Answer::Num(match o {
    Ordering::Less => -1.0,
    Ordering::Equal => 0.0,
    Ordering::Greater => 1.0,
  })
}

fn num(n: impl Into<f64>) -> Answer {
  Answer::Num(n.into())
}

// ---- the surface ----

/// `new Temporal.<kind>(...args)`.
pub fn construct(kind: Kind, args: &[Arg]) -> Result<Answer, Error> {
  let int = |i: usize, what: &str| -> Result<i64, Error> {
    match arg(args, i) {
      Arg::Undefined => Ok(0),
      a => integer(what, a),
    }
  };
  let narrow = |n: i64, what: &str| -> Result<u8, Error> { u8::try_from(n).map_err(|_| range(format!("{what} {n} is out of range"))) };
  let wide = |n: i64, what: &str| -> Result<u16, Error> { u16::try_from(n).map_err(|_| range(format!("{what} {n} is out of range"))) };
  match kind {
    Kind::PlainDate => {
      let year = i32::try_from(integer("year", arg(args, 0))?).map_err(range)?;
      PlainDate::try_new(year, narrow(integer("month", arg(args, 1))?, "month")?, narrow(integer("day", arg(args, 2))?, "day")?, calendar(arg(args, 3))?).map(answer_date).map_err(range)
    }
    Kind::PlainTime => answer_time(PlainTime::try_new(narrow(int(0, "hour")?, "hour")?, narrow(int(1, "minute")?, "minute")?, narrow(int(2, "second")?, "second")?, wide(int(3, "millisecond")?, "millisecond")?, wide(int(4, "microsecond")?, "microsecond")?, wide(int(5, "nanosecond")?, "nanosecond")?).map_err(range)?),
    Kind::PlainDateTime => {
      let year = i32::try_from(integer("year", arg(args, 0))?).map_err(range)?;
      let made = PlainDateTime::try_new(year, narrow(integer("month", arg(args, 1))?, "month")?, narrow(integer("day", arg(args, 2))?, "day")?, narrow(int(3, "hour")?, "hour")?, narrow(int(4, "minute")?, "minute")?, narrow(int(5, "second")?, "second")?, wide(int(6, "millisecond")?, "millisecond")?, wide(int(7, "microsecond")?, "microsecond")?, wide(int(8, "nanosecond")?, "nanosecond")?, calendar(arg(args, 9))?);
      answer_date_time(made.map_err(range)?)
    }
    Kind::PlainYearMonth => {
      let year = i32::try_from(integer("year", arg(args, 0))?).map_err(range)?;
      let day = match arg(args, 3) {
        Arg::Undefined => None,
        a => Some(narrow(integer("referenceISODay", a)?, "referenceISODay")?),
      };
      PlainYearMonth::try_new(year, narrow(integer("month", arg(args, 1))?, "month")?, day, calendar(arg(args, 2))?).map(answer_year_month).map_err(range)
    }
    Kind::PlainMonthDay => {
      let year = match arg(args, 3) {
        Arg::Undefined => None,
        a => Some(i32::try_from(integer("referenceISOYear", a)?).map_err(range)?),
      };
      PlainMonthDay::new_with_overflow(narrow(integer("month", arg(args, 0))?, "month")?, narrow(integer("day", arg(args, 1))?, "day")?, calendar(arg(args, 2))?, Overflow::Reject, year).map(answer_month_day).map_err(range)
    }
    Kind::Duration => {
      let d = Duration::new(int(0, "years")?, int(1, "months")?, int(2, "weeks")?, int(3, "days")?, int(4, "hours")?, int(5, "minutes")?, int(6, "seconds")?, int(7, "milliseconds")?, int(8, "microseconds")?.into(), int(9, "nanoseconds")?.into());
      Ok(answer_duration(d.map_err(range)?))
    }
    Kind::Instant => {
      let ns = match arg(args, 0) {
        Arg::BigInt(n) => *n,
        _ => return Err(type_error("new Temporal.Instant takes a bigint of nanoseconds")),
      };
      answer_instant(Instant::try_new(ns).map_err(range)?)
    }
    Kind::ZonedDateTime => {
      let ns = match arg(args, 0) {
        Arg::BigInt(n) => *n,
        _ => return Err(type_error("new Temporal.ZonedDateTime takes a bigint of nanoseconds")),
      };
      let zone = text("timeZone", arg(args, 1))?.to_owned();
      let cal = calendar(arg(args, 2))?;
      let made = with_provider(|p| ZonedDateTime::try_new_with_provider(ns, time_zone(&zone, p)?, cal, p).map_err(range))?;
      answer_zoned(made)
    }
  }
}

/// `Temporal.<kind>.<name>(...args)`: `from`, `compare` and the `Instant` constructors from epoch values.
pub fn call_static(kind: Kind, name: &str, args: &[Arg]) -> Result<Answer, Error> {
  let (a, b) = (arg(args, 0), arg(args, 1));
  match (kind, name) {
    (Kind::PlainDate, "from") => Ok(answer_date(plain_date(a, b)?)),
    (Kind::PlainTime, "from") => answer_time(plain_time(a, b)?),
    (Kind::PlainDateTime, "from") => answer_date_time(plain_date_time(a, b)?),
    (Kind::Instant, "from") => answer_instant(instant(a)?),
    (Kind::ZonedDateTime, "from") => answer_zoned(zoned(a)?),
    (Kind::Duration, "from") => Ok(answer_duration(duration(a)?)),
    (Kind::PlainYearMonth, "from") => Ok(answer_year_month(year_month(a, b)?)),
    (Kind::PlainMonthDay, "from") => Ok(answer_month_day(month_day(a, b)?)),
    (Kind::PlainYearMonth, "compare") => Ok(ordering(year_month(a, &Arg::Undefined)?.compare_iso(&year_month(b, &Arg::Undefined)?))),
    (Kind::Instant, "fromEpochMilliseconds") => answer_instant(Instant::from_epoch_milliseconds(integer("epochMilliseconds", a)?).map_err(range)?),
    (Kind::Instant, "fromEpochNanoseconds") => match a {
      Arg::BigInt(n) => answer_instant(Instant::try_new(*n).map_err(range)?),
      _ => Err(type_error("fromEpochNanoseconds takes a bigint")),
    },
    (Kind::PlainDate, "compare") => Ok(ordering(plain_date(a, &Arg::Undefined)?.compare_iso(&plain_date(b, &Arg::Undefined)?))),
    (Kind::PlainTime, "compare") => Ok(ordering(plain_time(a, &Arg::Undefined)?.cmp(&plain_time(b, &Arg::Undefined)?))),
    (Kind::PlainDateTime, "compare") => Ok(ordering(plain_date_time(a, &Arg::Undefined)?.compare_iso(&plain_date_time(b, &Arg::Undefined)?))),
    (Kind::Instant, "compare") => Ok(ordering(instant(a)?.cmp(&instant(b)?))),
    (Kind::ZonedDateTime, "compare") => Ok(ordering(zoned(a)?.compare_instant(&zoned(b)?))),
    (Kind::Duration, "compare") => {
      let (x, y, relative) = (duration(a)?, duration(b)?, relative_to(arg(args, 2))?);
      with_provider(|p| x.compare_with_provider(&y, relative, p).map(ordering).map_err(range))
    }
    _ => Err(type_error(format!("Temporal.{}.{name} is not a function", kind.name()))),
  }
}

/// The calendar fields every date-bearing kind answers, read through `get`.
macro_rules! date_field {
  ($v:expr, $name:expr) => {
    match $name {
      "year" => Some(num($v.year())),
      "month" => Some(num($v.month())),
      "monthCode" => Some(Answer::Str($v.month_code().as_str().to_owned())),
      "day" => Some(num($v.day())),
      "dayOfWeek" => Some(num($v.day_of_week())),
      "dayOfYear" => Some(num($v.day_of_year())),
      "weekOfYear" => Some($v.week_of_year().map_or(Answer::Undefined, num)),
      "yearOfWeek" => Some($v.year_of_week().map_or(Answer::Undefined, |y| num(y))),
      "daysInWeek" => Some(num($v.days_in_week())),
      "daysInMonth" => Some(num($v.days_in_month())),
      "daysInYear" => Some(num($v.days_in_year())),
      "monthsInYear" => Some(num($v.months_in_year())),
      "inLeapYear" => Some(Answer::Bool($v.in_leap_year())),
      "calendarId" => Some(Answer::Str($v.calendar().identifier().to_owned())),
      "era" => Some($v.era().map_or(Answer::Undefined, |e| Answer::Str(e.to_string()))),
      "eraYear" => Some($v.era_year().map_or(Answer::Undefined, |y| num(y))),
      _ => None,
    }
  };
}

macro_rules! time_field {
  ($v:expr, $name:expr) => {
    match $name {
      "hour" => Some(num($v.hour())),
      "minute" => Some(num($v.minute())),
      "second" => Some(num($v.second())),
      "millisecond" => Some(num($v.millisecond())),
      "microsecond" => Some(num($v.microsecond())),
      "nanosecond" => Some(num($v.nanosecond())),
      _ => None,
    }
  };
}

/// `value.<name>` of a Temporal value.
pub fn get(kind: Kind, value: &str, name: &str) -> Result<Answer, Error> {
  let this = Arg::Str(value.to_owned());
  let found = match kind {
    Kind::PlainDate => date_field!(plain_date(&this, &Arg::Undefined)?, name),
    Kind::PlainTime => time_field!(plain_time(&this, &Arg::Undefined)?, name),
    Kind::PlainDateTime => {
      let v = plain_date_time(&this, &Arg::Undefined)?;
      date_field!(v, name).or_else(|| time_field!(v, name))
    }
    Kind::ZonedDateTime => {
      let v = zoned(&this)?;
      match name {
        "timeZoneId" => Some(Answer::Str(with_provider(|p| v.time_zone().identifier_with_provider(p).map_err(range))?)),
        "offset" => Some(Answer::Str(v.offset())),
        "offsetNanoseconds" => Some(num(v.offset_nanoseconds() as f64)),
        "epochMilliseconds" => Some(num(v.epoch_milliseconds() as f64)),
        "epochNanoseconds" => Some(Answer::BigInt(v.epoch_nanoseconds().as_i128())),
        "hoursInDay" => Some(num(with_provider(|p| v.hours_in_day_with_provider(p).map_err(range))?)),
        _ => date_field!(v, name).or_else(|| time_field!(v, name)),
      }
    }
    Kind::PlainYearMonth => {
      let v = year_month(&this, &Arg::Undefined)?;
      match name {
        "year" => Some(num(v.year())),
        "month" => Some(num(v.month())),
        "monthCode" => Some(Answer::Str(v.month_code().as_str().to_owned())),
        "daysInMonth" => Some(num(v.days_in_month())),
        "daysInYear" => Some(num(v.days_in_year())),
        "monthsInYear" => Some(num(v.months_in_year())),
        "inLeapYear" => Some(Answer::Bool(v.in_leap_year())),
        "calendarId" => Some(Answer::Str(v.calendar_id().to_owned())),
        "era" => Some(v.era().map_or(Answer::Undefined, |e| Answer::Str(e.to_string()))),
        "eraYear" => Some(v.era_year().map_or(Answer::Undefined, |y| num(y))),
        _ => None,
      }
    }
    Kind::PlainMonthDay => {
      let v = month_day(&this, &Arg::Undefined)?;
      match name {
        "monthCode" => Some(Answer::Str(v.month_code().as_str().to_owned())),
        "day" => Some(num(v.day())),
        "calendarId" => Some(Answer::Str(v.calendar_id().to_owned())),
        _ => None,
      }
    }
    Kind::Instant => {
      let v = instant(&this)?;
      match name {
        "epochMilliseconds" => Some(num(v.epoch_milliseconds() as f64)),
        "epochNanoseconds" => Some(Answer::BigInt(v.epoch_nanoseconds().as_i128())),
        _ => None,
      }
    }
    Kind::Duration => {
      let v = duration(&this)?;
      match name {
        "years" => Some(num(v.years() as f64)),
        "months" => Some(num(v.months() as f64)),
        "weeks" => Some(num(v.weeks() as f64)),
        "days" => Some(num(v.days() as f64)),
        "hours" => Some(num(v.hours() as f64)),
        "minutes" => Some(num(v.minutes() as f64)),
        "seconds" => Some(num(v.seconds() as f64)),
        "milliseconds" => Some(num(v.milliseconds() as f64)),
        "microseconds" => Some(num(v.microseconds() as f64)),
        "nanoseconds" => Some(num(v.nanoseconds() as f64)),
        "sign" => Some(num(f64::from(v.sign() as i8))),
        "blank" => Some(Answer::Bool(v.is_zero())),
        _ => None,
      }
    }
  };
  Ok(found.unwrap_or(Answer::Undefined))
}

/// `value.<method>(...args)` of a Temporal value.
pub fn call(kind: Kind, value: &str, method: &str, args: &[Arg]) -> Result<Answer, Error> {
  let this = Arg::Str(value.to_owned());
  let (a, b) = (arg(args, 0), arg(args, 1));
  let not_a_method = || type_error(format!("Temporal.{}.prototype.{method} is not a function", kind.name()));
  if method == "valueOf" {
    return Err(type_error(format!("Temporal.{} can't be converted to a primitive; use compare", kind.name())));
  }
  match kind {
    Kind::PlainDate => {
      let v = plain_date(&this, &Arg::Undefined)?;
      match method {
        "add" => v.add(&duration(a)?, overflow(b)?).map(answer_date).map_err(range),
        "subtract" => v.subtract(&duration(a)?, overflow(b)?).map(answer_date).map_err(range),
        "until" => v.until(&plain_date(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "since" => v.since(&plain_date(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "with" => match a {
          Arg::Obj(fields) => v.with(calendar_fields(fields)?, overflow(b)?).map(answer_date).map_err(range),
          _ => Err(type_error("with takes an object")),
        },
        "equals" => Ok(Answer::Bool(v == plain_date(a, &Arg::Undefined)?)),
        "toString" => Ok(Answer::Str(v.to_ixdtf_string(calendar_display(a)?))),
        "toJSON" => Ok(Answer::Str(date_text(&v))),
        "toPlainDateTime" => {
          let time = match a {
            Arg::Undefined => None,
            other => Some(plain_time(other, &Arg::Undefined)?),
          };
          answer_date_time(v.to_plain_date_time(time).map_err(range)?)
        }
        "toPlainYearMonth" => v.to_plain_year_month().map(answer_year_month).map_err(range),
        "toPlainMonthDay" => v.to_plain_month_day().map(answer_month_day).map_err(range),
        "toZonedDateTime" => {
          let (zone, time) = match a {
            Arg::Str(zone) => (zone.clone(), None),
            Arg::Obj(fields) => (text("timeZone", field(fields, "timeZone").unwrap_or(&Arg::Undefined))?.to_owned(), field(fields, "plainTime").map(|t| plain_time(t, &Arg::Undefined)).transpose()?),
            _ => return Err(type_error("toZonedDateTime takes a time zone or { timeZone, plainTime }")),
          };
          let made = with_provider(|p| v.to_zoned_date_time_with_provider(time_zone(&zone, p)?, time, p).map_err(range))?;
          answer_zoned(made)
        }
        _ => Err(not_a_method()),
      }
    }
    Kind::PlainTime => {
      let v = plain_time(&this, &Arg::Undefined)?;
      match method {
        "add" => answer_time(v.add(&duration(a)?).map_err(range)?),
        "subtract" => answer_time(v.subtract(&duration(a)?).map_err(range)?),
        "until" => v.until(&plain_time(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "since" => v.since(&plain_time(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "with" => match a {
          Arg::Obj(fields) => answer_time(v.with(partial_time(fields)?, overflow(b)?).map_err(range)?),
          _ => Err(type_error("with takes an object")),
        },
        "round" => answer_time(v.round(rounding(a)?).map_err(range)?),
        "equals" => Ok(Answer::Bool(v == plain_time(a, &Arg::Undefined)?)),
        "toString" => Ok(Answer::Str(v.to_ixdtf_string(to_string_options(a)?).map_err(range)?)),
        "toJSON" => Ok(Answer::Str(time_text(&v)?)),
        _ => Err(not_a_method()),
      }
    }
    Kind::PlainDateTime => {
      let v = plain_date_time(&this, &Arg::Undefined)?;
      match method {
        "add" => answer_date_time(v.add(&duration(a)?, overflow(b)?).map_err(range)?),
        "subtract" => answer_date_time(v.subtract(&duration(a)?, overflow(b)?).map_err(range)?),
        "until" => v.until(&plain_date_time(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "since" => v.since(&plain_date_time(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "with" => match a {
          Arg::Obj(fields) => answer_date_time(v.with(temporal_rs::fields::DateTimeFields { calendar_fields: calendar_fields(fields)?, time: partial_time(fields)? }, overflow(b)?).map_err(range)?),
          _ => Err(type_error("with takes an object")),
        },
        "withPlainTime" => {
          let time = match a {
            Arg::Undefined => None,
            other => Some(plain_time(other, &Arg::Undefined)?),
          };
          answer_date_time(v.with_time(time).map_err(range)?)
        }
        "round" => answer_date_time(v.round(rounding(a)?).map_err(range)?),
        "equals" => Ok(Answer::Bool(v == plain_date_time(a, &Arg::Undefined)?)),
        "toString" => Ok(Answer::Str(v.to_ixdtf_string(to_string_options(a)?, calendar_display(a)?).map_err(range)?)),
        "toJSON" => Ok(Answer::Str(date_time_text(&v)?)),
        "toPlainDate" => Ok(answer_date(v.to_plain_date())),
        "toPlainTime" => answer_time(v.to_plain_time()),
        "toZonedDateTime" => {
          let zone = text("timeZone", a)?.to_owned();
          let disambiguation = parsed("disambiguation", option_text(b, "disambiguation")?)?.unwrap_or_default();
          let made = with_provider(|p| v.to_zoned_date_time_with_provider(time_zone(&zone, p)?, disambiguation, p).map_err(range))?;
          answer_zoned(made)
        }
        _ => Err(not_a_method()),
      }
    }
    Kind::Instant => {
      let v = instant(&this)?;
      match method {
        "add" => answer_instant(v.add(&duration(a)?).map_err(range)?),
        "subtract" => answer_instant(v.subtract(&duration(a)?).map_err(range)?),
        "until" => v.until(&instant(a)?, difference(b)?).map(answer_duration).map_err(range),
        "since" => v.since(&instant(a)?, difference(b)?).map(answer_duration).map_err(range),
        "round" => answer_instant(v.round(rounding(a)?).map_err(range)?),
        "equals" => Ok(Answer::Bool(v == instant(a)?)),
        "toString" | "toJSON" => {
          let options = if method == "toJSON" { Arg::Undefined } else { a.clone() };
          let zone = option_text(&options, "timeZone")?.map(str::to_owned);
          let rounding = to_string_options(&options)?;
          with_provider(|p| {
            let zone = zone.map(|z| time_zone(&z, p)).transpose()?;
            Ok(Answer::Str(v.to_ixdtf_string_with_provider(zone, rounding, p).map_err(range)?))
          })
        }
        "toZonedDateTimeISO" => {
          let zone = text("timeZone", a)?.to_owned();
          let made = with_provider(|p| v.to_zoned_date_time_iso_with_provider(time_zone(&zone, p)?, p).map_err(range))?;
          answer_zoned(made)
        }
        _ => Err(not_a_method()),
      }
    }
    Kind::ZonedDateTime => {
      let v = zoned(&this)?;
      match method {
        "add" => answer_zoned(with_provider(|p| v.add_with_provider(&duration(a)?, overflow(b)?, p).map_err(range))?),
        "subtract" => answer_zoned(with_provider(|p| v.subtract_with_provider(&duration(a)?, overflow(b)?, p).map_err(range))?),
        "until" => {
          let other = zoned(a)?;
          with_provider(|p| v.until_with_provider(&other, difference(b)?, p).map(answer_duration).map_err(range))
        }
        "since" => {
          let other = zoned(a)?;
          with_provider(|p| v.since_with_provider(&other, difference(b)?, p).map(answer_duration).map_err(range))
        }
        "round" => answer_zoned(with_provider(|p| v.round_with_provider(rounding(a)?, p).map_err(range))?),
        "equals" => {
          let other = zoned(a)?;
          with_provider(|p| v.equals_with_provider(&other, p).map(Answer::Bool).map_err(range))
        }
        "startOfDay" => answer_zoned(with_provider(|p| v.start_of_day_with_provider(p).map_err(range))?),
        "withTimeZone" => {
          let zone = text("timeZone", a)?.to_owned();
          answer_zoned(with_provider(|p| v.with_time_zone_with_provider(time_zone(&zone, p)?, p).map_err(range))?)
        }
        "withPlainTime" => {
          let time = match a {
            Arg::Undefined => None,
            other => Some(plain_time(other, &Arg::Undefined)?),
          };
          answer_zoned(with_provider(|p| v.with_plain_time_and_provider(time, p).map_err(range))?)
        }
        "toInstant" => answer_instant(v.to_instant()),
        "toPlainDate" => Ok(answer_date(v.to_plain_date())),
        "toPlainTime" => answer_time(v.to_plain_time()),
        "toPlainDateTime" => answer_date_time(v.to_plain_date_time()),
        "toString" | "toJSON" => {
          let options = if method == "toJSON" { Arg::Undefined } else { a.clone() };
          let display_offset = parsed("offset", option_text(&options, "offset")?)?.unwrap_or_default();
          let display_zone = parsed("timeZoneName", option_text(&options, "timeZoneName")?)?.unwrap_or_default();
          let display_calendar = calendar_display(&options)?;
          let rounding = to_string_options(&options)?;
          with_provider(|p| Ok(Answer::Str(v.to_ixdtf_string_with_provider(display_offset, display_zone, display_calendar, rounding, p).map_err(range)?)))
        }
        _ => Err(not_a_method()),
      }
    }
    Kind::PlainYearMonth => {
      let v = year_month(&this, &Arg::Undefined)?;
      match method {
        "add" => v.add(&duration(a)?, overflow(b)?.unwrap_or_default()).map(answer_year_month).map_err(range),
        "subtract" => v.subtract(&duration(a)?, overflow(b)?.unwrap_or_default()).map(answer_year_month).map_err(range),
        "until" => v.until(&year_month(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "since" => v.since(&year_month(a, &Arg::Undefined)?, difference(b)?).map(answer_duration).map_err(range),
        "with" => match a {
          Arg::Obj(fields) => {
            let mut wanted = temporal_rs::fields::YearMonthCalendarFields::default();
            let given = calendar_fields(fields)?;
            (wanted.year, wanted.month, wanted.month_code) = (given.year, given.month, given.month_code);
            v.with(wanted, overflow(b)?).map(answer_year_month).map_err(range)
          }
          _ => Err(type_error("with takes an object")),
        },
        "equals" => Ok(Answer::Bool(v == year_month(a, &Arg::Undefined)?)),
        "toPlainDate" => match a {
          Arg::Obj(fields) => v.to_plain_date(Some(calendar_fields(fields)?)).map(answer_date).map_err(range),
          _ => Err(type_error("toPlainDate takes { day }")),
        },
        "toString" => Ok(Answer::Str(v.to_ixdtf_string(calendar_display(a)?))),
        "toJSON" => Ok(Answer::Str(v.to_ixdtf_string(DisplayCalendar::Auto))),
        _ => Err(not_a_method()),
      }
    }
    Kind::PlainMonthDay => {
      let v = month_day(&this, &Arg::Undefined)?;
      match method {
        "with" => match a {
          Arg::Obj(fields) => v.with(calendar_fields(fields)?, overflow(b)?).map(answer_month_day).map_err(range),
          _ => Err(type_error("with takes an object")),
        },
        "equals" => Ok(Answer::Bool(v == month_day(a, &Arg::Undefined)?)),
        "toPlainDate" => match a {
          Arg::Obj(fields) => v.to_plain_date(Some(calendar_fields(fields)?)).map(answer_date).map_err(range),
          _ => Err(type_error("toPlainDate takes { year }")),
        },
        "toString" => Ok(Answer::Str(v.to_ixdtf_string(calendar_display(a)?))),
        "toJSON" => Ok(Answer::Str(v.to_ixdtf_string(DisplayCalendar::Auto))),
        _ => Err(not_a_method()),
      }
    }
    Kind::Duration => {
      let v = duration(&this)?;
      match method {
        "add" => Ok(answer_duration(v.add(&duration(a)?).map_err(range)?)),
        "subtract" => Ok(answer_duration(v.subtract(&duration(a)?).map_err(range)?)),
        "negated" => Ok(answer_duration(v.negated())),
        "abs" => Ok(answer_duration(v.abs())),
        "with" => {
          let Arg::Obj(given) = a else { return Err(type_error("with takes an object")) };
          let mut fields: Vec<(String, Arg)> = ["years", "months", "weeks", "days", "hours", "minutes", "seconds", "milliseconds", "microseconds", "nanoseconds"]
            .iter()
            .map(|n| Ok((n.to_string(), match get(Kind::Duration, value, n)? { Answer::Num(x) => Arg::Num(x), _ => Arg::Undefined })))
            .collect::<Result<_, Error>>()?;
          for (k, val) in given {
            if let Some(slot) = fields.iter_mut().find(|(n, _)| n == k) {
              slot.1 = val.clone();
            }
          }
          Ok(answer_duration(duration(&Arg::Obj(fields))?))
        }
        "round" => {
          let (options, relative) = (rounding(a)?, relative_to(a)?);
          with_provider(|p| v.round_with_provider(options, relative, p).map(answer_duration).map_err(range))
        }
        "total" => {
          let unit = match a {
            Arg::Str(unit) => unit.clone(),
            Arg::Obj(fields) => text("unit", field(fields, "unit").unwrap_or(&Arg::Undefined))?.to_owned(),
            _ => return Err(type_error("total takes a unit or { unit }")),
          };
          let unit: Unit = unit.parse().map_err(|_| range(format!("`{unit}` is not a unit")))?;
          let relative = relative_to(a)?;
          with_provider(|p| v.total_with_provider(unit, relative, p).map(|t| num(t.as_inner())).map_err(range))
        }
        "toString" => Ok(Answer::Str(v.as_temporal_string(to_string_options(a)?).map_err(range)?)),
        "toJSON" => Ok(Answer::Str(v.to_string())),
        _ => Err(not_a_method()),
      }
    }
  }
}

/// `Temporal.Now.<name>(...args)` at the instant `now_ms`, in the zone the argument names or [`crate::system_zone`].
pub fn now(name: &str, now_ms: f64, args: &[Arg]) -> Result<Answer, Error> {
  let at = Instant::from_epoch_milliseconds(now_ms as i64).map_err(range)?;
  let zone = match arg(args, 0) {
    Arg::Undefined => crate::system_zone().to_owned(),
    other => text("timeZone", other)?.to_owned(),
  };
  match name {
    "instant" => answer_instant(at),
    "timeZoneId" => Ok(Answer::Str(zone)),
    _ => {
      let here = with_provider(|p| at.to_zoned_date_time_iso_with_provider(time_zone(&zone, p)?, p).map_err(range))?;
      match name {
        "zonedDateTimeISO" => answer_zoned(here),
        "plainDateISO" => Ok(answer_date(here.to_plain_date())),
        "plainTimeISO" => answer_time(here.to_plain_time()),
        "plainDateTimeISO" => answer_date_time(here.to_plain_date_time()),
        _ => Err(type_error(format!("Temporal.Now.{name} is not a function"))),
      }
    }
  }
}
