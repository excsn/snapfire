//! Time zone names as `Date.prototype.toString` writes them: ICU's long specific name in the system locale, such as `Central European Summer Time`.
//!
//! `data/zones.txt` is written by `datagen --zones` from a cldr-json release. One record per line, fields separated by tabs, tagged by the first field:
//!
//! - `V version`: the CLDR release.
//! - `K zone aliases…`: a zone as CLDR names it and the other names of the same zone.
//! - `M zone metazone from to`: a metazone the zone belongs to from `from` until `to`, both milliseconds since 1970 UTC and empty for no bound.
//! - `G metazone zone`: the zone whose rules stand for the metazone's.
//! - `P locale parent`: the locale a locale's names fall back to.
//! - `D locale digits system`: the ten digits of a locale's numbering system and its name, `arab`, when they are not ASCII.
//! - `F locale gmt zero hours`: the localized GMT format, the format for a zero offset and the positive and negative hour patterns.
//! - `N locale key standard daylight`: the long names of a metazone or of a zone, given by its path. An empty name falls back to the parent's.
//! - `Y locale date glue`: the `yMd` pattern a date with no options is written in and the short `atTime` pattern joining a date to a time.
//! - `J locale h H hm Hm`: the patterns of an hour alone and of an hour and its minutes on a 12-hour and on a 24-hour clock.
//! - `O locale hms Hms am pm`: the patterns a time is written in on a 12-hour and on a 24-hour clock and the abbreviated day period markers.
//! - `H locale hour calendar`: the clock a locale writes times on (`h` or `H`) and the calendar it writes dates in (`gregorian`, `persian` or `buddhist`). A locale with no record has its parent's.
//! - `C locale calendar date abbreviated narrow hms Hms am pm`: the pattern a date in a calendar other than the Gregorian is written in, the abbreviated and narrow names of the era its dates fall in and the calendar's own time patterns and day period markers, which ICU writes a time in when the date is in that calendar.
//! - `U locale width unit plural pattern`: a duration unit's pattern, `{0} hours`, at a width and a plural form.
//! - `T locale separator`: the separator between hours, minutes and seconds.

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
  numbering: Option<&'static str>,
  formats: Option<[&'static str; 3]>,
  dates: Option<[&'static str; 2]>,
  clock: Option<[&'static str; 4]>,
  hours: Option<[&'static str; 4]>,
  defaults: Option<(&'static str, &'static str)>,
  calendars: HashMap<&'static str, [&'static str; 7]>,
  units: HashMap<(&'static str, &'static str, &'static str), &'static str>,
  time_separator: Option<&'static str>,
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
        "D" => {
          let names = zones.locales.entry(f[1]).or_default();
          names.digits = Some(f[2]);
          names.numbering = f.get(3).copied();
        }
        "F" => zones.locales.entry(f[1]).or_default().formats = Some([f[2], f[3], f[4]]),
        "Y" => zones.locales.entry(f[1]).or_default().dates = Some([f[2], f[3]]),
        "O" => zones.locales.entry(f[1]).or_default().clock = Some([f[2], f[3], f[4], f[5]]),
        "J" => zones.locales.entry(f[1]).or_default().hours = Some([f[2], f[3], f[4], f[5]]),
        "H" => zones.locales.entry(f[1]).or_default().defaults = Some((f[2], f[3])),
        "C" => {
          zones.locales.entry(f[1]).or_default().calendars.insert(f[2], [f[3], f[4], f[5], f[6], f[7], f[8], f[9]]);
        }
        "U" => {
          zones.locales.entry(f[1]).or_default().units.insert((f[2], f[3], f[4]), f[5]);
        }
        "T" => zones.locales.entry(f[1]).or_default().time_separator = Some(f[2]),
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

/// The CLDR locales `tag` reads names from, most specific first, ending at the root locale.
pub(crate) fn locale_chain(tag: &str) -> Vec<&'static str> {
  let data = zones();
  let mut chain = Vec::new();
  let mut at = crate::locale::name(tag, &data.locales).or(Some("und"));
  while let Some(name) = at {
    let Some((key, names)) = data.locales.get_key_value(name) else { break };
    chain.push(*key);
    at = names.parent;
  }
  chain
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

/// A calendar a numeric date is written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calendar {
  Gregorian,
  Persian,
  Buddhist,
}

impl Calendar {
  fn name(self) -> &'static str {
    match self {
      Calendar::Gregorian => "gregorian",
      Calendar::Persian => "persian",
      Calendar::Buddhist => "buddhist",
    }
  }
}

/// The value of the `-u-` extension key `key` in `tag`, `islamic-civil` for `ca` in `ar-u-ca-islamic-civil`.
fn keyword(tag: &str, key: &str) -> Option<String> {
  let mut parts = tag.split(['-', '_']).skip_while(|part| !part.eq_ignore_ascii_case("u")).skip(1);
  parts.by_ref().find(|part| part.eq_ignore_ascii_case(key))?;
  let value: Vec<String> = parts.take_while(|part| part.len() > 2).map(str::to_ascii_lowercase).collect();
  (!value.is_empty()).then(|| value.join("-"))
}

/// The calendar `tag` writes numeric dates in: the one its `-u-ca-` keyword names, else the one CLDR's region of the locale prefers, the Persian for `fa` and the Buddhist for `th`. `None` when the keyword names a calendar this crate does not write.
pub fn calendar(tag: &str) -> Option<Calendar> {
  match keyword(tag, "ca").as_deref() {
    Some("gregory") => Some(Calendar::Gregorian),
    Some("persian") => Some(Calendar::Persian),
    Some("buddhist") => Some(Calendar::Buddhist),
    Some(_) => None,
    None => match chain(tag).iter().find_map(|names| names.defaults).map_or("gregorian", |(_, calendar)| calendar) {
      "persian" => Some(Calendar::Persian),
      "buddhist" => Some(Calendar::Buddhist),
      _ => Some(Calendar::Gregorian),
    },
  }
}

/// `pattern` written out, a run of one ASCII letter replaced by what `field` gives for the letter and the run's length, quoted text unquoted and `''` a quote. A letter `field` gives nothing for is dropped.
fn write_pattern(pattern: &str, field: impl Fn(char, usize) -> Option<String>) -> String {
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
      c if c.is_ascii_alphabetic() => {
        let mut width = 1;
        while chars.peek() == Some(&c) {
          chars.next();
          width += 1;
        }
        if let Some(text) = field(c, width) {
          out.push_str(&text);
        }
      }
      other => out.push(other),
    }
  }
  out
}

/// The Julian day number of a proleptic Gregorian date.
fn julian_day(year: i64, month: u32, day: u32) -> i64 {
  let y = if month <= 2 { year - 1 } else { year };
  let era = y.div_euclid(400);
  let yoe = y - era * 400;
  let m = i64::from(month);
  let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
  era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy + 1_721_120
}

/// The Julian calendar date of a Julian day number.
fn julian_calendar(day: i64) -> (i64, u32, u32) {
  let c = day + 32_082;
  let d = (4 * c + 3).div_euclid(1461);
  let e = c - (1461 * d).div_euclid(4);
  let m = (5 * e + 2).div_euclid(153);
  ((d - 4800 + m / 10), (m + 3 - 12 * (m / 10)) as u32, (e - (153 * m + 2).div_euclid(5) + 1) as u32)
}

/// The Buddhist year, month and day of a Gregorian date, as ICU's `BuddhistCalendar` counts: the year is 543 later. Before the Gregorian reform of 1582-10-15 the month and day are the Julian calendar's.
fn buddhist(year: i64, month: u32, day: u32) -> (i64, u32, u32) {
  const REFORM: i64 = 2_299_161;
  let jd = julian_day(year, month, day);
  let (year, month, day) = if jd < REFORM { julian_calendar(jd) } else { (year, month, day) };
  (year + 543, month, day)
}

/// The Persian year, month and day of a Gregorian date, by the 33-year arithmetic cycle ICU's `PersianCalendar` counts in.
fn persian(year: i64, month: u32, day: u32) -> (i64, u32, u32) {
  const EPOCH: i64 = 1_948_320;
  const MONTH_STARTS: [i64; 12] = [0, 31, 62, 93, 124, 155, 186, 216, 246, 276, 306, 336];
  let since = julian_day(year, month, day) - EPOCH;
  let year = 1 + (33 * since + 3).div_euclid(12_053);
  let day_of_year = since - (365 * (year - 1) + (8 * year + 21).div_euclid(33));
  let month = if day_of_year < 216 { day_of_year / 31 } else { (day_of_year - 6) / 30 };
  (year, month as u32 + 1, (day_of_year - MONTH_STARTS[month as usize] + 1) as u32)
}

/// `date.toLocaleDateString(tag)` with no options: the Gregorian date `year`, `month` and `day` written in the locale's calendar, `yMd` pattern and digits, a Gregorian year before 1 as its year before the common era, `9/10/2024` in English, `10.9.2024` in German and `۱۴۰۳/۶/۲۰` in Persian. A narrow no-break space is written as a space, as V8 writes it. `None` when [`calendar`] is.
pub fn numeric_date(tag: &str, year: i64, month: u32, day: u32) -> Option<String> {
  let chain = chain(tag);
  let calendar = calendar(tag)?;
  let (pattern, era, (year, month, day)) = match calendar {
    Calendar::Gregorian => (chain.iter().find_map(|names| names.dates).map_or("y-MM-dd", |[date, _]| date), None, (if year < 1 { 1 - year } else { year }, month, day)),
    other => {
      let [pattern, abbreviated, narrow, ..] = chain.iter().find_map(|names| names.calendars.get(other.name()).copied()).unwrap_or(["G y-MM-dd", "", "", "", "", "", ""]);
      let date = if other == Calendar::Persian { persian(year, month, day) } else { buddhist(year, month, day) };
      (pattern, Some((abbreviated, narrow)), date)
    }
  };
  let minus = crate::data::data().numbers[crate::locale::resolve(tag).numbers].minus;
  let out = write_pattern(pattern, |c, width| match c {
    'y' if width == 2 => Some(format!("{:02}", year.rem_euclid(100))),
    'y' if year < 0 => Some(format!("{minus}{:0width$}", -year)),
    'y' => Some(format!("{year:0width$}")),
    'M' | 'L' => Some(format!("{month:0width$}")),
    'd' => Some(format!("{day:0width$}")),
    'G' => era.map(|(abbreviated, narrow)| if width == 5 { narrow } else { abbreviated }.to_owned()),
    _ => None,
  });
  Some(localized_digits(&chain, out).replace('\u{202f}', " "))
}

/// `date.toLocaleTimeString(tag)` with no options: `hour`, `minute` and `second` on the clock the locale's region keeps, in the pattern of the locale's [`calendar`] and its digits, `7:37:20 PM` in English and `19:37:20` in German. A narrow no-break space is written as a space, as V8 writes it.
pub fn clock(tag: &str, hour: u32, minute: u32, second: u32) -> String {
  let chain = chain(tag);
  let twelve = chain.iter().find_map(|names| names.defaults).is_none_or(|(hour, _)| hour == "h");
  let gregorian = || chain.iter().find_map(|names| names.clock).unwrap_or(["h:mm:ss a", "HH:mm:ss", "AM", "PM"]);
  let [hms, hms24, am, pm] = match calendar(tag) {
    Some(other @ (Calendar::Persian | Calendar::Buddhist)) => chain.iter().find_map(|names| names.calendars.get(other.name())).map_or_else(gregorian, |c| [c[3], c[4], c[5], c[6]]),
    _ => gregorian(),
  };
  let out = write_pattern(if twelve { hms } else { hms24 }, |c, width| match c {
    'h' => Some(format!("{:0width$}", if hour % 12 == 0 { 12 } else { hour % 12 })),
    'K' => Some(format!("{:0width$}", hour % 12)),
    'H' => Some(format!("{hour:0width$}")),
    'k' => Some(format!("{:0width$}", if hour == 0 { 24 } else { hour })),
    'm' => Some(format!("{minute:0width$}")),
    's' => Some(format!("{second:0width$}")),
    'a' | 'b' | 'B' => Some(if hour < 12 { am } else { pm }.to_owned()),
    _ => None,
  });
  localized_digits(&chain, out).replace('\u{202f}', " ")
}

/// `date.toLocaleString(tag)` with no options: `date` and `time`, each already written, joined as the locale's short `atTime` pattern joins them, `9/10/2024, 7:37:20 PM`.
pub fn date_with_time(tag: &str, date: &str, time: &str) -> String {
  let chain = chain(tag);
  let glue = chain.iter().find_map(|names| names.dates).map_or("{1} {0}", |[_, glue]| glue);
  write_pattern(&glue.replace("{1}", "\u{1}").replace("{0}", "\u{2}"), |_, _| None).replace('\u{202f}', " ").replace('\u{1}', date).replace('\u{2}', time)
}

/// The pattern a duration of `unit` is written in at `width`, `"long"`, `"short"` or `"narrow"`, for the plural form `plural`, `{0} hours`; `other` when the locale has no pattern for that form.
pub fn duration_unit(tag: &str, width: &str, unit: &str, plural: &str) -> Option<&'static str> {
  let chain = chain(tag);
  let find = |plural: &str| chain.iter().find_map(|names| names.units.get(&(width, unit, plural)).copied());
  find(plural).or_else(|| find("other"))
}

/// The separator `tag` writes between hours, minutes and seconds, `:` in English and `.` in Finnish.
pub fn time_separator(tag: &str) -> &'static str {
  chain(tag).iter().find_map(|names| names.time_separator).unwrap_or(":")
}

/// The numbering system `tag` writes numbers in, as `Intl` names it: `arab` for `ar-EG`, `latn` for a locale whose digits are ASCII.
pub fn numbering_system(tag: &str) -> &'static str {
  chain(tag).first().and_then(|names| names.numbering).unwrap_or("latn")
}

/// The hour cycle `tag` writes a time on, as `Intl` names it: `h12` for `en` and `h23` for `de`. A `-u-hc-` keyword names it.
pub fn hour_cycle(tag: &str) -> &'static str {
  match keyword(tag, "hc").as_deref() {
    Some("h11") => "h11",
    Some("h12") => "h12",
    Some("h23") => "h23",
    Some("h24") => "h24",
    _ if chain(tag).iter().find_map(|names| names.defaults).is_none_or(|(hour, _)| hour == "h") => "h12",
    _ => "h23",
  }
}

/// Whether the hour, the minutes and the seconds are written with two digits in the pattern `tag` writes a time of the hour alone, with `minute`, or with `minute` and `second` in on the hour cycle `cycle`: the `j`, `jm` and `jms` skeletons. `[false, true, true]` for `en` on `h12` with all three.
pub fn time_widths(tag: &str, cycle: &str, minute: bool, second: bool) -> [bool; 3] {
  let chain = chain(tag);
  let twelve = matches!(cycle, "h11" | "h12");
  let mut hours = chain.iter().find_map(|names| names.hours).unwrap_or(["h a", "HH", "h:mm a", "HH:mm"]);
  let mut clock = chain.iter().find_map(|names| names.clock).unwrap_or(["h:mm:ss a", "HH:mm:ss", "AM", "PM"]);
  // A locale writing dates in another calendar writes times in that calendar's patterns too.
  if let Some(own) = calendar(tag).filter(|c| *c != Calendar::Gregorian).and_then(|other| chain.iter().find_map(|names| names.calendars.get(other.name()))) {
    clock = [own[3], own[4], own[5], own[6]];
    hours = [own[3], own[4], own[3], own[4]];
  }
  let pattern = match (second, minute, twelve) {
    (true, _, true) => clock[0],
    (true, _, false) => clock[1],
    (false, true, true) => hours[2],
    (false, true, false) => hours[3],
    (false, false, true) => hours[0],
    (false, false, false) => hours[1],
  };
  let doubled = |letters: &[&str]| letters.iter().any(|run| pattern.contains(run));
  [doubled(&["hh", "HH", "KK", "kk"]), doubled(&["mm"]), doubled(&["ss"])]
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
