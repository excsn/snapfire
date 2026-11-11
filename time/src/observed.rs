//! What a zone observes at an instant, read from its TZif rules.
//!
//! The main form of the IANA database marks Dublin's winter as daylight saving time an hour below its standard time. ICU reads the rearguard form, which has no negative saving. This module reads the rules the same way: of two neighbouring periods, the one with the larger offset has the saving.

use std::collections::HashMap;
use std::sync::Arc;

use combine::Parser;
use parking_lot::Mutex;

use crate::{Error, Source, MAX_DATE_MS};

/// What a zone observes at an instant, in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Observed {
  /// The offset from UTC, east positive: `-14400` for New York in summer.
  pub offset: i32,
  /// The part of the offset that is daylight saving, never negative: `3600` for New York in summer, `0` in winter.
  pub saving: i32,
}

#[derive(Debug, Clone, Copy)]
struct Kind {
  offset: i32,
  daylight: bool,
}

struct Footer {
  standard: i32,
  daylight: Option<i32>,
}

struct Rules {
  transitions: Vec<i64>,
  kinds: Vec<Kind>,
  /// The kind in force from each transition, `kinds[0]` before the first.
  at: Vec<usize>,
  footer: Option<Footer>,
}

fn bytes(source: Source, zone: &str) -> Result<Vec<u8>, Error> {
  let unknown = || Error::UnknownZone(zone.to_owned());
  match source {
    #[cfg(feature = "tz_compiled")]
    Source::Compiled => jiff_tzdb::get(zone).map(|(_, data)| data.to_vec()).ok_or_else(unknown),
    #[cfg(feature = "tz_system")]
    Source::System => {
      if zone.split('/').any(|part| part.is_empty() || part == "." || part == "..") {
        return Err(unknown());
      }
      std::fs::read(std::path::Path::new("/usr/share/zoneinfo").join(zone)).map_err(|_| unknown())
    }
    #[allow(unreachable_patterns)]
    other => Err(Error::NotBuilt(other)),
  }
}

fn parse(zone: &str, data: &[u8]) -> Result<Rules, Error> {
  let unknown = || Error::UnknownZone(zone.to_owned());
  let (tzif, _) = tzif::parse::tzif::tzif().parse(data).map_err(|_| unknown())?;
  let block = tzif.data_block2.as_ref().unwrap_or(&tzif.data_block1);
  let kinds: Vec<Kind> = block.local_time_type_records.iter().map(|r| Kind { offset: r.utoff.0 as i32, daylight: r.is_dst }).collect();
  if kinds.is_empty() {
    return Err(unknown());
  }
  let footer = tzif.footer.as_ref().map(|posix| Footer { standard: -(posix.std_info.offset.0 as i32), daylight: posix.dst_info.as_ref().map(|d| -(d.variant_info.offset.0 as i32)) });
  Ok(Rules { transitions: block.transition_times.iter().map(|t| t.0).collect(), kinds, at: block.transition_types.clone(), footer })
}

fn rules(source: Source, zone: &str) -> Result<Arc<Rules>, Error> {
  static LOADED: Mutex<Option<HashMap<(Source, String), Arc<Rules>>>> = Mutex::new(None);
  let key = (source, zone.to_owned());
  if let Some(found) = LOADED.lock().as_ref().and_then(|map| map.get(&key)) {
    return Ok(found.clone());
  }
  if !source.built() {
    return Err(if Source::Compiled.built() || Source::System.built() { Error::NotBuilt(source) } else { Error::NoSource });
  }
  let loaded = Arc::new(parse(zone, &bytes(source, zone)?)?);
  LOADED.lock().get_or_insert_with(HashMap::new).insert(key, loaded.clone());
  Ok(loaded)
}

impl Rules {
  /// The period in force at `seconds`: its index among the transitions plus one, `0` before the first.
  fn period(&self, seconds: i64) -> usize {
    self.transitions.partition_point(|&t| t <= seconds)
  }

  fn kind(&self, period: usize) -> Kind {
    match period {
      0 => self.kinds.iter().copied().find(|k| !k.daylight).unwrap_or(self.kinds[0]),
      n => self.kinds[self.at[n - 1]],
    }
  }

  fn in_footer(&self, seconds: i64) -> bool {
    self.footer.is_some() && self.transitions.last().is_none_or(|&last| seconds >= last)
  }

  /// The saving of `period`. A daylight period counts its offset above the nearest standard period before it, else after it. A standard period above a daylight neighbour, a negative saving in the main form, counts its offset above that neighbour.
  fn saving(&self, period: usize) -> i32 {
    let here = self.kind(period);
    let last = self.transitions.len();
    if here.daylight {
      let standard = (0..period).rev().chain(period + 1..=last).map(|p| self.kind(p)).find(|k| !k.daylight);
      return standard.map_or(0, |s| (here.offset - s.offset).max(0));
    }
    let neighbours = [period.checked_sub(1), (period < last).then_some(period + 1)];
    neighbours.into_iter().flatten().map(|p| self.kind(p)).filter(|k| k.daylight && k.offset < here.offset).map(|k| here.offset - k.offset).max().unwrap_or(0)
  }

  fn footer_saving(footer: &Footer, offset: i32) -> i32 {
    match footer.daylight {
      Some(daylight) => offset - footer.standard.min(daylight),
      None => 0,
    }
  }
}

fn seconds(ms: f64) -> Result<i64, Error> {
  if !ms.is_finite() || ms.abs() > MAX_DATE_MS {
    return Err(Error::OutOfRange);
  }
  Ok((ms / 1000.0).floor() as i64)
}

/// What `zone` observes at the instant `ms` from `source`.
pub fn observed_in(source: Source, zone: &str, ms: f64) -> Result<Observed, Error> {
  let at = seconds(ms)?;
  let rules = rules(source, zone)?;
  if rules.in_footer(at) {
    let offset = crate::offset_seconds(source, zone, ms)?;
    let footer = rules.footer.as_ref().expect("in_footer holds a footer");
    return Ok(Observed { offset, saving: Rules::footer_saving(footer, offset) });
  }
  let period = rules.period(at);
  Ok(Observed { offset: rules.kind(period).offset, saving: rules.saving(period) })
}

/// The daylight saving `zone` observes during the UTC calendar year holding `ms`, the largest when it observes more than one; `None` when it observes none that year. ICU's `useDaylightTime` and `getDSTSavings` read the year this way.
pub fn saving_in_year(source: Source, zone: &str, ms: f64) -> Result<Option<i32>, Error> {
  let at = seconds(ms)?;
  let rules = rules(source, zone)?;
  if rules.in_footer(at) {
    let footer = rules.footer.as_ref().expect("in_footer holds a footer");
    return Ok(footer.daylight.map(|daylight| (daylight - footer.standard).abs()));
  }
  let days = at.div_euclid(86_400);
  let (year, _, _) = civil(days);
  let (start, end) = (days_from_civil(year, 1, 1) * 86_400, days_from_civil(year + 1, 1, 1) * 86_400);
  let (first, last) = (rules.period(start), rules.period(end - 1));
  let most = (first..=last).map(|p| rules.saving(p)).max().unwrap_or(0);
  Ok((most > 0).then_some(most))
}

/// The year, month and day of `days` since 1970-01-01, in the proleptic Gregorian calendar.
pub(crate) fn civil(days: i64) -> (i64, u32, u32) {
  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = z - era * 146_097;
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
  (yoe + era * 400 + i64::from(month <= 2), month, day)
}

pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
  let year = if month <= 2 { year - 1 } else { year };
  let era = year.div_euclid(400);
  let yoe = year - era * 400;
  let month = i64::from(month);
  let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146_097 + doe - 719_468
}
