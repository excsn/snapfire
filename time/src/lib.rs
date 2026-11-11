//! Time zones for SnapFire as JavaScript reads them: the offset `Date.prototype.getTimezoneOffset` gives for an instant, in the zone `TZ` names or the host's own.
//!
//! The zone rules come from a [`Source`] the application picks with a feature: `tz_compiled` builds the IANA database into the binary and `tz_system` reads the host's zoneinfo. With both built in, [`choose`] picks one before the first lookup. Without a call the compiled one answers. With neither, every lookup is [`Error::NoSource`].

use std::sync::OnceLock;

mod observed;
pub mod temporal;

pub use observed::{observed_in, saving_in_year, Observed};

use temporal_rs::provider::TimeZoneProvider;
use temporal_rs::{TimeZone, ZonedDateTime};

/// Where zone rules are read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
  /// The IANA database compiled into the binary, feature `tz_compiled`.
  Compiled,
  /// The host's zoneinfo under `/usr/share/zoneinfo`, feature `tz_system`.
  System,
}

impl Source {
  /// `compiled` or `system`, as configuration names it.
  pub fn name(self) -> &'static str {
    match self {
      Source::Compiled => "compiled",
      Source::System => "system",
    }
  }

  /// The feature that builds the source in.
  pub fn feature(self) -> &'static str {
    match self {
      Source::Compiled => "tz_compiled",
      Source::System => "tz_system",
    }
  }

  /// Whether this build holds the source.
  pub fn built(self) -> bool {
    match self {
      Source::Compiled => cfg!(feature = "tz_compiled"),
      Source::System => cfg!(feature = "tz_system"),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
  /// The build holds neither source; enable `tz_compiled` or `tz_system`.
  NoSource,
  /// A source this build does not hold.
  NotBuilt(Source),
  /// [`choose`] after the source was already in use or chosen.
  AlreadyChosen(Source),
  /// A zone the source does not know.
  UnknownZone(String),
  /// An instant that is not a number or lies outside the range a `Date` holds.
  OutOfRange,
  /// What JavaScript throws as a `RangeError`: a value out of range or a string that does not parse.
  Range(String),
  /// What JavaScript throws as a `TypeError`: an argument of the wrong kind.
  Type(String),
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::NoSource => write!(f, "this build has no time zone source; enable the `tz_compiled` or the `tz_system` feature"),
      Error::NotBuilt(source) => write!(f, "this build does not hold the {} time zone source; enable the `{}` feature", source.name(), source.feature()),
      Error::AlreadyChosen(source) => write!(f, "the {} time zone source is already in use; a change takes a restart", source.name()),
      Error::UnknownZone(zone) => write!(f, "`{zone}` is not a time zone the source knows"),
      Error::OutOfRange => write!(f, "an instant outside the range of a `Date`"),
      Error::Range(message) | Error::Type(message) => f.write_str(message),
    }
  }
}

impl std::error::Error for Error {}

static CHOSEN: OnceLock<Source> = OnceLock::new();

/// Picks the source every later lookup reads, once, before the first. A build holding one source needs no call.
pub fn choose(source: Source) -> Result<(), Error> {
  if !source.built() {
    return Err(Error::NotBuilt(source));
  }
  let held = *CHOSEN.get_or_init(|| source);
  if held == source { Ok(()) } else { Err(Error::AlreadyChosen(held)) }
}

/// The source lookups read: the one [`choose`] picked, else the compiled one when built, else the system one.
pub fn source() -> Result<Source, Error> {
  let fallback = || if Source::Compiled.built() { Source::Compiled } else { Source::System };
  let held = *CHOSEN.get_or_init(fallback);
  if held.built() { Ok(held) } else { Err(Error::NoSource) }
}

pub(crate) fn provide<T>(source: Source, f: impl FnOnce(&dyn TimeZoneProvider) -> T) -> Result<T, Error> {
  if !source.built() {
    return Err(if Source::Compiled.built() || Source::System.built() { Error::NotBuilt(source) } else { Error::NoSource });
  }
  match source {
    Source::Compiled => {
      static COMPILED: OnceLock<timezone_provider::tzif::CompiledTzdbProvider> = OnceLock::new();
      Ok(f(COMPILED.get_or_init(Default::default)))
    }
    Source::System => {
      static SYSTEM: OnceLock<timezone_provider::tzif::FsTzdbProvider> = OnceLock::new();
      Ok(f(SYSTEM.get_or_init(Default::default)))
    }
  }
}

/// The zone a `TZ` value names, as node reads it: a leading `:` dropped and a path under a `zoneinfo` directory read as the zone below it. `None` for none or an empty value.
pub fn zone_from_tz(tz: Option<&str>) -> Option<String> {
  let tz = tz?.trim_start_matches(':');
  let zone = match tz.rfind("zoneinfo/") {
    Some(at) => &tz[at + "zoneinfo/".len()..],
    None => tz,
  };
  (!zone.is_empty()).then(|| zone.to_owned())
}

/// This machine's zone: the one `TZ` names, else the host's own, else `UTC`. Read once.
pub fn system_zone() -> &'static str {
  static ZONE: OnceLock<String> = OnceLock::new();
  ZONE.get_or_init(|| zone_from_tz(std::env::var("TZ").ok().as_deref()).or_else(|| iana_time_zone::get_timezone().ok()).unwrap_or_else(|| "UTC".to_owned()))
}

/// The largest instant a `Date` holds either side of the epoch, in milliseconds.
pub(crate) const MAX_DATE_MS: f64 = 8.64e15;

/// `getTimezoneOffset` of the instant `ms` in `zone` read from `source`: minutes to add to local time for UTC, so 240 for New York in summer. A zone offset with seconds, a city's mean time before standard time, is truncated to whole minutes as V8 does.
pub fn offset_in(source: Source, zone: &str, ms: f64) -> Result<f64, Error> {
  if !ms.is_finite() || ms.abs() > MAX_DATE_MS {
    return Err(Error::OutOfRange);
  }
  Ok((-f64::from(offset_seconds(source, zone, ms)?) / 60.0).trunc() + 0.0)
}

/// The offset from UTC of the instant `ms` in `zone`, in seconds east.
pub(crate) fn offset_seconds(source: Source, zone: &str, ms: f64) -> Result<i32, Error> {
  provide(source, |provider| {
    let tz = TimeZone::try_from_identifier_str_with_provider(zone, provider).map_err(|_| Error::UnknownZone(zone.to_owned()))?;
    let at = ZonedDateTime::try_new_iso_with_provider(ms as i128 * 1_000_000, tz, provider).map_err(|_| Error::OutOfRange)?;
    Ok((at.offset_nanoseconds() / 1_000_000_000) as i32)
  })?
}

/// The last instant V8 reads daylight saving at directly, 2038-01-19T03:14:07Z.
const MAX_EPOCH_MS: f64 = 2_147_483_647_000.0;

/// The instant V8 reads a zone's daylight saving at when naming the zone of `ms`. An instant before 1970 or after [`MAX_EPOCH_MS`] moves to the same month, day and time in a year from 2008 to 2035 that starts on the same weekday and has the same length (V8's `DateCache::EquivalentTime`). Any other instant stays.
pub fn equivalent_time(ms: f64) -> f64 {
  if !ms.is_finite() || (0.0..=MAX_EPOCH_MS).contains(&ms) {
    return ms;
  }
  const DAY: f64 = 86_400_000.0;
  let days = (ms / DAY).floor();
  let within = ms - days * DAY;
  let (year, month, day) = observed::civil(days as i64);
  let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
  let weekday = (observed::days_from_civil(year, 1, 1) + 4).rem_euclid(7);
  let recent = if leap { 1956 } else { 1967 } + (weekday * 12) % 28;
  let equivalent = 2008 + (recent + 3 * 28 - 2008) % 28;
  (observed::days_from_civil(equivalent, month, 1) + i64::from(day) - 1) as f64 * DAY + within
}

/// `new Date(ms).getTimezoneOffset()` in [`system_zone`] from [`source`]; `NaN` for an invalid instant. A zone the source does not know reads as `UTC`, as node treats an unknown `TZ`.
pub fn local_offset(ms: f64) -> Result<f64, Error> {
  if !ms.is_finite() || ms.abs() > MAX_DATE_MS {
    return Ok(f64::NAN);
  }
  let source = source()?;
  match offset_in(source, system_zone(), ms) {
    Err(Error::UnknownZone(_)) => Ok(0.0),
    other => other,
  }
}
