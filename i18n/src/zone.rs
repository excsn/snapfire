//! Time zone names as `Date.prototype.toString` writes them: ICU's long specific name in the system locale, such as `Central European Summer Time`.
//!
//! `data/zones.txt` is written by `datagen --zones` from a cldr-json release. One record per line, fields separated by tabs, tagged by the first field:
//!
//! - `V version`: the CLDR release.
//! - `K zone aliases…`: a zone as CLDR names it and the other names of the same zone.
//! - `M zone metazone from to`: a metazone the zone belongs to from `from` until `to`, both milliseconds since 1970 UTC and empty for no bound.
//! - `G metazone zone`: the zone whose rules stand for the metazone's.
//! - `P locale parent`: the locale a locale's names fall back to.
//! - `D locale digits`: the ten digits of a locale's numbering system when they are not ASCII.
//! - `F locale gmt zero hours`: the localized GMT format, the format for a zero offset and the positive and negative hour patterns.
//! - `N locale key standard daylight`: the long names of a metazone or of a zone, given by its path. An empty name falls back to the parent's.
//! - `Y locale date glue`: the `yMd` pattern a date with no options is written in and the short pattern joining a date to a time.

use std::collections::HashMap;
use std::sync::OnceLock;

const TEXT: &str = include_str!("../data/zones.txt");

/// What a zone observes, from a time zone database the caller holds.
pub trait ZoneRules {
  /// The offset from UTC `zone` observes at the instant `ms` and the part of it that is daylight saving, both in seconds, the saving never negative. `None` for a zone the rules do not know.
  fn observed(&self, zone: &str, ms: f64) -> Option<(i32, i32)>;

  /// The daylight saving `zone` observes during the UTC calendar year holding `ms`, in seconds. `None` when it observes none that year.
  fn saving_in_year(&self, zone: &str, ms: f64) -> Option<i32>;
}

struct Period {
  metazone: &'static str,
  from: f64,
  to: f64,
}

#[derive(Default)]
struct Names {
  parent: Option<&'static str>,
  digits: Option<&'static str>,
  formats: Option<[&'static str; 3]>,
  dates: Option<[&'static str; 2]>,
  names: HashMap<&'static str, (&'static str, &'static str)>,
}

#[derive(Default)]
struct Zones {
  version: &'static str,
  canonical: HashMap<&'static str, &'static str>,
  periods: HashMap<&'static str, Vec<Period>>,
  golden: HashMap<&'static str, &'static str>,
  locales: HashMap<&'static str, Names>,
}

fn bound(text: &str, open: f64) -> f64 {
  if text.is_empty() { open } else { text.parse().unwrap_or_else(|_| panic!("snapfire_i18n zone data: `{text}` is not a time")) }
}

fn zones() -> &'static Zones {
  static ZONES: OnceLock<Zones> = OnceLock::new();
  ZONES.get_or_init(|| {
    let mut zones = Zones::default();
    for line in TEXT.lines() {
      let f: Vec<&'static str> = line.split('\t').collect();
      match f[0] {
        "V" => zones.version = f[1],
        "K" => {
          for name in &f[1..] {
            zones.canonical.insert(name, f[1]);
          }
        }
        "M" => zones.periods.entry(f[1]).or_default().push(Period { metazone: f[2], from: bound(f[3], f64::NEG_INFINITY), to: bound(f[4], f64::INFINITY) }),
        "G" => {
          zones.golden.insert(f[1], f[2]);
        }
        "P" => zones.locales.entry(f[1]).or_default().parent = Some(f[2]),
        "D" => zones.locales.entry(f[1]).or_default().digits = Some(f[2]),
        "F" => zones.locales.entry(f[1]).or_default().formats = Some([f[2], f[3], f[4]]),
        "Y" => zones.locales.entry(f[1]).or_default().dates = Some([f[2], f[3]]),
        "N" => {
          zones.locales.entry(f[1]).or_default().names.insert(f[2], (f[3], f[4]));
        }
        other => panic!("snapfire_i18n zone data: unknown record `{other}`"),
      }
    }
    zones
  })
}


/// The metazone `canonical` belongs to at `ms`.
fn metazone(canonical: &str, ms: f64) -> Option<&'static str> {
  zones().periods.get(canonical)?.iter().find(|p| p.from <= ms && ms < p.to).map(|p| p.metazone)
}

/// Whether ICU counts the instant `ms` in `zone` as daylight time. A zone in a metazone at `ms` is in daylight time when its offset is above the standard offset of the metazone's own zone. The rearguard IANA rules ICU reads count Vancouver's permanent offset from November 2026 as daylight time, which the main rules call standard. Any other zone is in daylight time when its rules say it saves daylight.
pub fn in_daylight(zone: &str, ms: f64, rules: &impl ZoneRules) -> bool {
  let data = zones();
  let golden = data.canonical.get(zone).and_then(|canonical| metazone(canonical, ms)).and_then(|m| data.golden.get(m));
  if let (Some(golden), Some((offset, _))) = (golden, rules.observed(zone, ms)) {
    if let Some((golden_offset, golden_saving)) = rules.observed(golden, ms) {
      return offset > golden_offset - golden_saving;
    }
  }
  rules.observed(zone, ms).is_some_and(|(_, saving)| saving != 0)
}

/// The locales the names of `locale` are looked up in, most specific first.
fn chain(locale: &str) -> Vec<&'static Names> {
  let data = zones();
  let mut chain = Vec::new();
  let mut at = crate::locale::name(locale, &data.locales).or(Some("und"));
  while let Some(name) = at {
    let Some(names) = data.locales.get(name) else { break };
    chain.push(names);
    at = names.parent;
  }
  chain
}

/// `zone`'s long specific name in `locale`, as ICU's `TimeZone::getDisplayName(daylight, LONG, locale)` writes it, its metazone read at the instant `now`. A zone with no name in the locale is named by its localized GMT offset: the standard offset at `now`, plus the year's saving when `daylight` holds. A zone CLDR does not know is named by a zero offset written with its sign, `GMT+00:00`, as is `Etc/Unknown`.
pub fn zone_name(locale: &str, zone: &str, daylight: bool, now: f64, rules: &impl ZoneRules) -> String {
  let data = zones();
  let chain = chain(locale);
  let canonical = match data.canonical.get(zone) {
    Some(&"Etc/Unknown") | None => return gmt(&chain, 0, true),
    Some(&canonical) => canonical,
  };
  let pick = |(standard, daylight_name): (&'static str, &'static str)| if daylight { daylight_name } else { standard };
  let named = |key: &str| chain.iter().find_map(|names| names.names.get(key).copied().map(pick).filter(|n| !n.is_empty()));
  if let Some(name) = named(canonical).or_else(|| metazone(canonical, now).and_then(named)) {
    return name.to_owned();
  }
  let (offset, saving) = rules.observed(zone, now).unwrap_or((0, 0));
  let extra = if daylight { rules.saving_in_year(zone, now).unwrap_or(0) } else { 0 };
  gmt(&chain, offset - saving + extra, false)
}

/// `date.toLocaleDateString(tag)` with no options: `year`, `month` and `day` in the locale's `yMd` pattern and digits, `9/10/2024` in English and `10.9.2024` in German.
pub fn numeric_date(tag: &str, year: i64, month: u32, day: u32) -> String {
  let chain = chain(tag);
  let pattern = chain.iter().find_map(|names| names.dates).map_or("y-MM-dd", |[date, _]| date);
  let mut out = String::new();
  let mut chars = pattern.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      '\'' => {
        if chars.peek() == Some(&'\'') {
          chars.next();
          out.push('\'');
          continue;
        }
        for quoted in chars.by_ref() {
          if quoted == '\'' {
            break;
          }
          out.push(quoted);
        }
      }
      'y' | 'M' | 'd' => {
        let mut width = 1;
        while chars.peek() == Some(&c) {
          chars.next();
          width += 1;
        }
        let value = match c {
          'y' => year,
          'M' => i64::from(month),
          _ => i64::from(day),
        };
        out.push_str(&if c == 'y' && width == 2 { format!("{:02}", value.rem_euclid(100)) } else { format!("{value:0width$}") });
      }
      other => out.push(other),
    }
  }
  localized_digits(&chain, out)
}

/// `date.toLocaleString(tag)` with no options: `date` and `time`, each already written, joined as the locale's short date-time pattern joins them, `9/10/2024, 7:37:20 PM`.
pub fn date_with_time(tag: &str, date: &str, time: &str) -> String {
  let chain = chain(tag);
  let glue = chain.iter().find_map(|names| names.dates).map_or("{1} {0}", |[_, glue]| glue);
  glue.replace("{1}", date).replace("{0}", time)
}

/// `text` with its ASCII digits in the numbering system of the first locale in `chain`.
fn localized_digits(chain: &[&'static Names], text: String) -> String {
  match chain.first().and_then(|names| names.digits) {
    Some(digits) => {
      let digits: Vec<char> = digits.chars().collect();
      text.chars().map(|c| c.to_digit(10).and_then(|d| digits.get(d as usize).copied()).unwrap_or(c)).collect()
    }
    None => text,
  }
}

/// `offset` in seconds as the localized GMT format writes it, `GMT-05:00`. A zero offset takes the zero format unless `signed`.
fn gmt(chain: &[&'static Names], offset: i32, signed: bool) -> String {
  let formats = chain.iter().find_map(|names| names.formats).unwrap_or(["GMT{0}", "GMT", "+HH:mm;-HH:mm"]);
  if offset == 0 && !signed {
    return formats[1].to_owned();
  }
  let (positive, negative) = formats[2].split_once(';').unwrap_or((formats[2], formats[2]));
  let pattern = if offset < 0 { negative } else { positive };
  let total = offset.unsigned_abs();
  let (hours, minutes, seconds) = (total / 3600, total / 60 % 60, total % 60);
  let mut text = String::new();
  let mut rest = pattern;
  while let Some(c) = rest.chars().next() {
    if let Some(after) = rest.strip_prefix("HH") {
      text.push_str(&format!("{hours:02}"));
      rest = after;
    } else if let Some(after) = rest.strip_prefix('H') {
      text.push_str(&hours.to_string());
      rest = after;
    } else if let Some(after) = rest.strip_prefix("mm") {
      text.push_str(&format!("{minutes:02}"));
      if seconds != 0 {
        let separator = pattern.split("mm").next().and_then(|before| before.chars().last()).filter(|c| !c.is_alphanumeric()).unwrap_or(':');
        text.push_str(&format!("{separator}{seconds:02}"));
      }
      rest = after;
    } else {
      text.push(c);
      rest = &rest[c.len_utf8()..];
    }
  }
  let text = localized_digits(chain, text);
  formats[0].replace("{0}", &text)
}

#[cfg(test)]
mod tests {
  #[test]
  fn the_zone_names_come_from_the_cldr_release_the_other_data_does() {
    assert_eq!(super::zones().version, crate::cldr_version());
  }
}
