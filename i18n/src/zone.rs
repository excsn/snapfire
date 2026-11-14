//! Time zone names as `Date.prototype.toString` writes them: ICU's long specific name in the system locale, such as `Central European Summer Time`.
//!
//! `data/zones.txt` is written by `datagen --zones` from a cldr-json release and a tz database `zone.tab`. One record per line, fields separated by tabs, tagged by the first field:
//!
//! - `V version`: the CLDR release.
//! - `K zone aliases…`: a zone as CLDR names it and the other names of the same zone.
//! - `M zone metazone from to`: a metazone the zone belongs to from `from` until `to`, both milliseconds since 1970 UTC and empty for no bound.
//! - `G metazone zone`: the zone whose rules stand for the metazone's.
//! - `P locale parent`: the locale a locale's names fall back to.
//! - `D locale digits system`: the ten digits of a locale's numbering system and its name, `arab`, when they are not ASCII.
//! - `F locale gmt zero hours`: the localized GMT format, the format for a zero offset and the positive and negative hour patterns.
//! - `N locale key standard daylight`: the long names of a metazone or of a zone, given by its path. An empty name falls back to the parent's.
//! - `S locale key standard daylight`: the short names, `EST` and `EDT`. Empty names block the parent's, so the locale writes the GMT format.
//! - `Y locale date glue`: the `yMd` pattern a date with no options is written in and the short `atTime` pattern joining a date to a time.
//! - `J locale h H hm Hm`: the patterns of an hour alone and of an hour and its minutes on a 12-hour and on a 24-hour clock.
//! - `O locale hms Hms am pm`: the patterns a time is written in on a 12-hour and on a 24-hour clock and the abbreviated day period markers.
//! - `H locale hour calendar`: the clock a locale writes times on (`h` or `H`) and the calendar it writes dates in (`gregorian`, `persian` or `buddhist`). A locale with no record has its parent's.
//! - `C locale calendar date abbreviated narrow hms Hms am pm`: the pattern a date in a calendar other than the Gregorian is written in, the abbreviated and narrow names of the era its dates fall in and the calendar's own time patterns and day period markers, which ICU writes a time in when the date is in that calendar.
//! - `U locale width unit plural pattern`: a duration unit's pattern, `{0} hours`, at a width and a plural form.
//! - `T locale separator`: the separator between hours, minutes and seconds.
//! - `R region calendars…`: the calendars a region prefers, most preferred first, by their BCP 47 names.
//! - `Z region zones…`: the zones a region holds, by their canonical names, from the tz database's `zone.tab`.

use crate::data::{fields, Data};

/// What a zone observes, from a time zone database the caller holds.
pub trait ZoneRules {
  /// The offset from UTC `zone` observes at the instant `ms` and the part of it that is daylight saving, both in seconds, the saving never negative. `None` for a zone the rules do not know.
  fn observed(&self, zone: &str, ms: f64) -> Option<(i32, i32)>;

  /// The daylight saving `zone` observes during the UTC calendar year holding `ms`, in seconds. `None` when it observes none that year.
  fn saving_in_year(&self, zone: &str, ms: f64) -> Option<i32>;
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

/// `pattern` written out, a run of one ASCII letter replaced by what `field` gives for the letter and the run's length, quoted text unquoted and `''` a quote. A letter `field` gives nothing for is dropped.
fn write_pattern(pattern: &str, field: impl Fn(char, usize) -> Option<String>) -> String {
  pattern_parts(pattern, field).into_iter().map(|(_, text)| text).collect()
}

/// `pattern` written out as [`write_pattern`] writes it, in pieces: each field with the letter it was written for and each run of literal text with none.
fn pattern_parts(pattern: &str, field: impl Fn(char, usize) -> Option<String>) -> Vec<(Option<char>, String)> {
  let mut parts: Vec<(Option<char>, String)> = Vec::new();
  let literal = |parts: &mut Vec<(Option<char>, String)>, c: char| match parts.last_mut() {
    Some((None, text)) => text.push(c),
    _ => parts.push((None, c.to_string())),
  };
  let mut chars = pattern.chars().peekable();
  while let Some(c) = chars.next() {
    match c {
      '\'' => {
        if chars.peek() == Some(&'\'') {
          chars.next();
          literal(&mut parts, '\'');
          continue;
        }
        while let Some(quoted) = chars.next() {
          if quoted == '\'' {
            break;
          }
          literal(&mut parts, quoted);
        }
      }
      c if c.is_ascii_alphabetic() => {
        let mut width = 1;
        while chars.peek() == Some(&c) {
          chars.next();
          width += 1;
        }
        if let Some(text) = field(c, width) {
          parts.push((Some(c), text));
        }
      }
      other => literal(&mut parts, other),
    }
  }
  parts
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

/// A value of two fields as a pair.
fn pair(value: &'static str) -> (&'static str, &'static str) {
  let [first, second] = fields(value);
  (first, second)
}

impl Data {
  /// The value of `kind` in the first locale of `chain` that has one.
  fn first(&self, chain: &[&'static str], kind: &str) -> Option<&'static str> {
    chain.iter().find_map(|locale| self.get(&[kind, locale]))
  }

  /// The digits of the numbering system the first locale of `chain` writes in and the system's name, when they are not ASCII.
  fn digits(&self, chain: &[&'static str]) -> Option<[&'static str; 2]> {
    chain.first().and_then(|locale| self.get(&["zD", locale])).map(fields)
  }

  /// The calendars `region` prefers, separated by tabs.
  pub(crate) fn preferred_calendars_record(&self, region: &str) -> &'static str {
    self.get(&["zR", region]).or_else(|| self.get(&["zR", "001"])).unwrap_or("")
  }

  /// The zones `region` holds, separated by tabs.
  pub(crate) fn region_zones_record(&self, region: &str) -> &'static str {
    self.get(&["zZ", region]).unwrap_or("")
  }

  /// The date pattern, the abbreviated and narrow era names, the two time patterns and the two day period markers `calendar` has in the first locale of `chain` that writes it.
  fn calendar_patterns(&self, chain: &[&'static str], calendar: Calendar) -> Option<[&'static str; 7]> {
    chain.iter().find_map(|locale| self.get(&["zC", locale, calendar.name()])).map(fields)
  }

  /// The metazone `canonical` belongs to at `ms`.
  fn metazone(&self, canonical: &str, ms: f64) -> Option<&'static str> {
    self.get(&["zM", canonical])?.split('\u{1e}').find_map(|period| {
      let mut parts = period.split('\u{1f}');
      let metazone = parts.next()?;
      let from = parts.next().and_then(|from| from.parse().ok()).unwrap_or(f64::NEG_INFINITY);
      let to = parts.next().and_then(|to| to.parse().ok()).unwrap_or(f64::INFINITY);
      (from <= ms && ms < to).then_some(metazone)
    })
  }

  /// Whether ICU counts the instant `ms` in `zone` as daylight time. A zone in a metazone at `ms` is in daylight time when its offset is above the standard offset of the metazone's own zone. The rearguard IANA rules ICU reads count Vancouver's permanent offset from November 2026 as daylight time, which the main rules call standard. Any other zone is in daylight time when its rules say it saves daylight.
  pub fn in_daylight(&self, zone: &str, ms: f64, rules: &impl ZoneRules) -> bool {
    let golden = self.get(&["zK", zone]).and_then(|canonical| self.metazone(canonical, ms)).and_then(|m| self.get(&["zG", m]));
    if let (Some(golden), Some((offset, _))) = (golden, rules.observed(zone, ms)) {
      if let Some((golden_offset, golden_saving)) = rules.observed(golden, ms) {
        return offset > golden_offset - golden_saving;
      }
    }
    rules.observed(zone, ms).is_some_and(|(_, saving)| saving != 0)
  }

  /// The CLDR locales `tag` reads names from, most specific first, ending at the root locale.
  pub(crate) fn locale_chain(&self, tag: &str) -> Vec<&'static str> {
    let mut chain: Vec<&'static str> = Vec::with_capacity(4);
    let mut at = self.named(tag).or_else(|| self.get(&["zE", "und"]));
    while let Some(name) = at.filter(|name| !chain.contains(name)) {
      chain.push(name);
      at = self.get(&["zP", name]);
    }
    chain
  }

  /// `zone`'s long specific name in `locale`, as ICU's `TimeZone::getDisplayName(daylight, LONG, locale)` writes it, its metazone read at the instant `now`. A zone with no name in the locale is named by its localized GMT offset: the standard offset at `now`, plus the year's saving when `daylight` holds. A zone CLDR does not know is named by a zero offset written with its sign, `GMT+00:00`, as is `Etc/Unknown`.
  pub fn zone_name(&self, locale: &str, zone: &str, daylight: bool, now: f64, rules: &impl ZoneRules) -> String {
    let chain = self.locale_chain(locale);
    let canonical = match self.get(&["zK", zone]) {
      Some("Etc/Unknown") | None => return self.gmt(&chain, 0, true),
      Some(canonical) => canonical,
    };
    let pick = |(standard, daylight_name): (&'static str, &'static str)| if daylight { daylight_name } else { standard };
    let named = |key: &str| chain.iter().find_map(|locale| self.get(&["zN", locale, key]).map(pair).map(pick).filter(|n| !n.is_empty()));
    if let Some(name) = named(canonical).or_else(|| self.metazone(canonical, now).and_then(named)) {
      return name.to_owned();
    }
    let (offset, saving) = rules.observed(zone, now).unwrap_or((0, 0));
    let extra = if daylight { rules.saving_in_year(zone, now).unwrap_or(0) } else { 0 };
    self.gmt(&chain, offset - saving + extra, false)
  }

  /// The short name `zone` goes by at the instant `ms` in `locale`, as `Intl.DateTimeFormat` writes it under `timeZoneName: "short"`: `EDT` for `America/New_York` in `en-US`, else the short GMT format, `GMT+2` for `Europe/Paris` in `en-US` and `GMT+5:30` for `Asia/Kolkata`.
  pub fn short_zone_name(&self, locale: &str, zone: &str, ms: f64, rules: &impl ZoneRules) -> String {
    let names = self.locale_chain(locale);
    let (offset, _) = rules.observed(zone, ms).unwrap_or((0, 0));
    if let Some(canonical) = self.get(&["zK", zone]).filter(|c| *c != "Etc/Unknown") {
      let daylight = self.in_daylight(zone, ms, rules);
      let pick = |(standard, daylight_name): (&'static str, &'static str)| if daylight { daylight_name } else { standard };
      let own = |key: &str| names.iter().find_map(|locale| self.get(&["zS", locale, key]).map(pair));
      let named = |key: &str| own(key).map(pick).filter(|n| !n.is_empty());
      if let Some(name) = named(canonical).or_else(|| self.metazone(canonical, ms).and_then(named)) {
        return name.to_owned();
      }
    }
    self.short_gmt(&names, offset)
  }

  /// `offset` in seconds as the short localized GMT format writes it: the hours unpadded and the minutes only when there are some, separated as the locale's hour format separates them: `GMT+2`, `GMT+5:30` and `GMT+0`.
  fn short_gmt(&self, chain: &[&'static str], offset: i32) -> String {
    let formats = self.first(chain, "zF").map_or(["GMT{0}", "GMT", "+HH:mm;-HH:mm"], fields);
    let total = offset.unsigned_abs();
    let (hours, minutes) = (total / 3600, total / 60 % 60);
    let (positive, negative) = formats[2].split_once(';').unwrap_or((formats[2], formats[2]));
    let pattern = if offset < 0 { negative } else { positive };
    let (sign, after) = pattern.split_once('H').unwrap_or(("+", "H:mm"));
    let separator = after.trim_start_matches('H').split_once('m').map_or(":", |(between, _)| between);
    let amount = if minutes == 0 { format!("{sign}{hours}") } else { format!("{sign}{hours}{separator}{minutes:02}") };
    self.localized_digits(chain, formats[0].replace("{0}", &amount))
  }

  /// The calendar `tag` writes numeric dates in: the one its `-u-ca-` keyword names, else the one CLDR's region of the locale prefers, the Persian for `fa` and the Buddhist for `th`. `None` when the keyword names a calendar this crate does not write.
  pub fn calendar(&self, tag: &str) -> Option<Calendar> {
    match keyword(tag, "ca").as_deref() {
      Some("gregory") => Some(Calendar::Gregorian),
      Some("persian") => Some(Calendar::Persian),
      Some("buddhist") => Some(Calendar::Buddhist),
      Some(_) => None,
      None => match self.first(&self.locale_chain(tag), "zH").map(fields).map_or("gregorian", |[_, calendar]: [&str; 2]| calendar) {
        "persian" => Some(Calendar::Persian),
        "buddhist" => Some(Calendar::Buddhist),
        _ => Some(Calendar::Gregorian),
      },
    }
  }

  /// The calendars `region` prefers, most preferred first, by their BCP 47 names, as `Intl.Locale.prototype.getCalendars` lists them. A region with none of its own has the world's, `gregory`.
  pub fn preferred_calendars(&self, region: &str) -> Vec<&'static str> {
    self.preferred_calendars_record(region).split('\t').filter(|calendar| !calendar.is_empty()).collect()
  }

  /// The zones `region` holds by their canonical names, sorted, as `Intl.Locale.prototype.getTimeZones` lists them. Empty for a region the tz database gives none.
  pub fn region_zones(&self, region: &str) -> Vec<&'static str> {
    self.region_zones_record(region).split('\t').filter(|zone| !zone.is_empty()).collect()
  }

  /// `date.toLocaleDateString(tag)` with no options: the Gregorian date `year`, `month` and `day` written in the locale's calendar, `yMd` pattern and digits, a Gregorian year before 1 as its year before the common era, `9/10/2024` in English, `10.9.2024` in German and `۱۴۰۳/۶/۲۰` in Persian. A narrow no-break space is written as a space, as V8 writes it. `None` when [`calendar`] is.
  pub fn numeric_date(&self, tag: &str, year: i64, month: u32, day: u32) -> Option<String> {
    Some(self.numeric_date_parts(tag, year, month, day)?.into_iter().map(|(_, text)| text).collect())
  }

  /// [`numeric_date`] in the parts `Intl.DateTimeFormat.prototype.formatToParts` gives: each piece with its type, `year`, `month`, `day`, `era` or `literal`.
  pub fn numeric_date_parts(&self, tag: &str, year: i64, month: u32, day: u32) -> Option<Vec<(&'static str, String)>> {
    let chain = self.locale_chain(tag);
    let calendar = self.calendar(tag)?;
    let (pattern, era, (year, month, day)) = match calendar {
      Calendar::Gregorian => (self.first(&chain, "zY").map(fields).map_or("y-MM-dd", |[date, _]: [&str; 2]| date), None, (if year < 1 { 1 - year } else { year }, month, day)),
      other => {
        let [pattern, abbreviated, narrow, ..] = self.calendar_patterns(&chain, other).unwrap_or(["G y-MM-dd", "", "", "", "", "", ""]);
        let date = if other == Calendar::Persian { persian(year, month, day) } else { buddhist(year, month, day) };
        (pattern, Some((abbreviated, narrow)), date)
      }
    };
    let minus = self.numbers(self.resolve(tag).numbers).minus;
    let parts = pattern_parts(pattern, |c, width| match c {
      'y' if width == 2 => Some(format!("{:02}", year.rem_euclid(100))),
      'y' if year < 0 => Some(format!("{minus}{:0width$}", -year)),
      'y' => Some(format!("{year:0width$}")),
      'M' | 'L' => Some(format!("{month:0width$}")),
      'd' => Some(format!("{day:0width$}")),
      'G' => era.map(|(abbreviated, narrow)| if width == 5 { narrow } else { abbreviated }.to_owned()),
      _ => None,
    });
    Some(parts.into_iter().map(|(letter, text)| {
      let kind = match letter {
        Some('y') => "year",
        Some('M' | 'L') => "month",
        Some('d') => "day",
        Some('G') => "era",
        _ => "literal",
      };
      (kind, self.localized_digits(&chain, text).replace('\u{202f}', " "))
    }).collect())
  }

  /// `date.toLocaleTimeString(tag)` with no options: `hour`, `minute` and `second` on the clock the locale's region keeps, in the pattern of the locale's [`calendar`] and its digits, `7:37:20 PM` in English and `19:37:20` in German. A narrow no-break space is written as a space, as V8 writes it.
  pub fn clock(&self, tag: &str, hour: u32, minute: u32, second: u32) -> String {
    let chain = self.locale_chain(tag);
    let twelve = self.first(&chain, "zH").map(fields).is_none_or(|[hour, _]: [&str; 2]| hour == "h");
    let gregorian = || self.first(&chain, "zO").map_or(["h:mm:ss a", "HH:mm:ss", "AM", "PM"], fields);
    let [hms, hms24, am, pm] = match self.calendar(tag) {
      Some(other @ (Calendar::Persian | Calendar::Buddhist)) => self.calendar_patterns(&chain, other).map_or_else(gregorian, |c| [c[3], c[4], c[5], c[6]]),
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
    self.localized_digits(&chain, out).replace('\u{202f}', " ")
  }

  /// `date.toLocaleString(tag)` with no options: `date` and `time`, each already written, joined as the locale's short `atTime` pattern joins them, `9/10/2024, 7:37:20 PM`.
  pub fn date_with_time(&self, tag: &str, date: &str, time: &str) -> String {
    let chain = self.locale_chain(tag);
    let glue = self.first(&chain, "zY").map(fields).map_or("{1} {0}", |[_, glue]: [&str; 2]| glue);
    write_pattern(&glue.replace("{1}", "\u{1}").replace("{0}", "\u{2}"), |_, _| None).replace('\u{202f}', " ").replace('\u{1}', date).replace('\u{2}', time)
  }

  /// The pattern a duration of `unit` is written in at `width`, `"long"`, `"short"` or `"narrow"`, for the plural form `plural`, `{0} hours`; `other` when the locale has no pattern for that form.
  pub fn duration_unit(&self, tag: &str, width: &str, unit: &str, plural: &str) -> Option<&'static str> {
    let chain = self.locale_chain(tag);
    let find = |plural: &str| chain.iter().find_map(|locale| self.get(&["zU", locale, width, unit, plural]));
    find(plural).or_else(|| find("other"))
  }

  /// The separator `tag` writes between hours, minutes and seconds, `:` in English and `.` in Finnish.
  pub fn time_separator(&self, tag: &str) -> &'static str {
    self.first(&self.locale_chain(tag), "zT").unwrap_or(":")
  }

  /// Whether CLDR holds the locale `tag` names, by itself or a locale it falls back to other than the root, as `Intl`'s `supportedLocalesOf` answers.
  pub fn supported(&self, tag: &str) -> bool {
    self.named(tag).is_some_and(|name| !matches!(name, "root" | "und"))
  }

  /// The numbering system `tag` writes numbers in, as `Intl` names it: `arab` for `ar-EG`, `latn` for a locale whose digits are ASCII.
  pub fn numbering_system(&self, tag: &str) -> &'static str {
    self.digits(&self.locale_chain(tag)).map(|[_, system]| system).filter(|system| !system.is_empty()).unwrap_or("latn")
  }

  /// The hour cycle `tag` writes a time on, as `Intl` names it: `h12` for `en` and `h23` for `de`. A `-u-hc-` keyword names it.
  pub fn hour_cycle(&self, tag: &str) -> &'static str {
    match keyword(tag, "hc").as_deref() {
      Some("h11") => "h11",
      Some("h12") => "h12",
      Some("h23") => "h23",
      Some("h24") => "h24",
      _ if self.first(&self.locale_chain(tag), "zH").map(fields).is_none_or(|[hour, _]: [&str; 2]| hour == "h") => "h12",
      _ => "h23",
    }
  }

  /// Whether the hour, the minutes and the seconds are written with two digits in the pattern `tag` writes a time of the hour alone, with `minute`, or with `minute` and `second` in on the hour cycle `cycle`: the `j`, `jm` and `jms` skeletons. `[false, true, true]` for `en` on `h12` with all three.
  pub fn time_widths(&self, tag: &str, cycle: &str, minute: bool, second: bool) -> [bool; 3] {
    let chain = self.locale_chain(tag);
    let twelve = matches!(cycle, "h11" | "h12");
    let mut hours = self.first(&chain, "zJ").map_or(["h a", "HH", "h:mm a", "HH:mm"], fields);
    let mut clock = self.first(&chain, "zO").map_or(["h:mm:ss a", "HH:mm:ss", "AM", "PM"], fields);
    // A locale writing dates in another calendar writes times in that calendar's patterns too.
    if let Some(own) = self.calendar(tag).filter(|c| *c != Calendar::Gregorian).and_then(|other| self.calendar_patterns(&chain, other)) {
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
  fn localized_digits(&self, chain: &[&'static str], text: String) -> String {
    match self.digits(chain).map(|[digits, _]| digits) {
      Some(digits) => {
        let digits: Vec<char> = digits.chars().collect();
        text.chars().map(|c| c.to_digit(10).and_then(|d| digits.get(d as usize).copied()).unwrap_or(c)).collect()
      }
      None => text,
    }
  }

  /// `offset` in seconds as the localized GMT format writes it, `GMT-05:00`. A zero offset takes the zero format unless `signed`.
  fn gmt(&self, chain: &[&'static str], offset: i32, signed: bool) -> String {
    let formats = self.first(chain, "zF").map_or(["GMT{0}", "GMT", "+HH:mm;-HH:mm"], fields);
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
    let text = self.localized_digits(chain, text);
    formats[0].replace("{0}", &text)
  }
}

#[cfg(test)]
mod tests {
  #[cfg(feature = "embedded")]
  #[test]
  fn the_zone_names_come_from_the_cldr_release_the_other_data_does() {
    let data = crate::Data::embedded();
    assert_eq!(data.get(&["zV"]), Some(data.cldr_version()));
  }
}
