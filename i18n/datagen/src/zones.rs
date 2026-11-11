//! `snapfire_i18n_datagen --zones <cldr-json> <out>`: the numeric date patterns and the time zone names of the `cldr-dates-full`, `cldr-numbers-full`, `cldr-core` and `cldr-bcp47` packages, in the line format `snapfire_i18n`'s `src/zone.rs` describes.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;

use crate::{field, locales, read, str_at};

/// Milliseconds since 1970-01-01 UTC of a CLDR `YYYY-MM-DD HH:MM` time.
fn millis(text: &str) -> Result<i64, String> {
  let bad = || format!("`{text}` is not a CLDR time");
  let (date, time) = text.split_once(' ').ok_or_else(bad)?;
  let d: Vec<i64> = date.split('-').map(|p| p.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
  let t: Vec<i64> = time.split(':').map(|p| p.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
  let [year, month, day] = d[..] else { return Err(bad()) };
  let [hour, minute] = t[..] else { return Err(bad()) };
  let y = if month <= 2 { year - 1 } else { year };
  let era = y.div_euclid(400);
  let yoe = y - era * 400;
  let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
  let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
  Ok(((days * 24 + hour) * 60 + minute) * 60_000)
}

/// Each zone under `node` by its path, with the metazone periods it lists.
fn periods(node: &Value, path: &mut Vec<String>, out: &mut Vec<String>) -> Result<(), String> {
  match node {
    Value::Array(uses) => {
      let zone = path.join("/");
      for entry in uses {
        let used = entry.get("usesMetazone").ok_or_else(|| format!("{zone}: no usesMetazone"))?;
        let metazone = str_at(used, &["_mzone"]).ok_or_else(|| format!("{zone}: no _mzone"))?;
        let from = str_at(used, &["_from"]).map(millis).transpose()?.map_or(String::new(), |ms| ms.to_string());
        let to = str_at(used, &["_to"]).map(millis).transpose()?.map_or(String::new(), |ms| ms.to_string());
        out.push(format!("M\t{zone}\t{metazone}\t{from}\t{to}"));
      }
    }
    Value::Object(children) => {
      for (name, child) in children {
        path.push(name.clone());
        periods(child, path, out)?;
        path.pop();
      }
    }
    _ => {}
  }
  Ok(())
}

/// The long standard and daylight names under `node`, keyed by metazone or by zone path.
fn names(node: &Value, path: &mut Vec<String>, out: &mut BTreeMap<String, (String, String)>) {
  let Value::Object(children) = node else { return };
  if let Some(long) = children.get("long") {
    let standard = str_at(long, &["standard"]).unwrap_or("").to_owned();
    let daylight = str_at(long, &["daylight"]).unwrap_or("").to_owned();
    if !standard.is_empty() || !daylight.is_empty() {
      out.insert(path.join("/"), (standard, daylight));
    }
  }
  for (name, child) in children {
    if name != "long" && name != "short" && child.is_object() {
      path.push(name.clone());
      names(child, path, out);
      path.pop();
    }
  }
}

struct Own {
  names: BTreeMap<String, (String, String)>,
  formats: [String; 3],
  digits: Option<String>,
  dates: [String; 2],
  /// Duration unit patterns by width, unit and plural form, and the time separator.
  units: BTreeMap<String, String>,
}

const DURATION_UNITS: [&str; 10] = ["year", "month", "week", "day", "hour", "minute", "second", "millisecond", "microsecond", "nanosecond"];

pub(crate) fn generate(root: &Path) -> Result<String, String> {
  let core = root.join("cldr-core");
  let version = str_at(&read(&core.join("package.json"))?, &["version"]).ok_or("no cldr-core version")?.to_owned();
  let mut out = String::new();
  writeln!(out, "V\t{version}").unwrap();

  let zones = read(&root.join("cldr-bcp47/bcp47/timezone.json"))?;
  let keys = zones.pointer("/keyword/u/tz").and_then(Value::as_object).ok_or("no bcp47 time zones")?;
  let canonical = |key: &str| keys.get(key).and_then(|k| str_at(k, &["_alias"])).and_then(|a| a.split(' ').next()).map(str::to_owned);
  for (key, info) in keys {
    let Some(aliases) = str_at(info, &["_alias"]) else { continue };
    let names: Vec<&str> = aliases.split(' ').collect();
    let target = match str_at(info, &["_preferred"]) {
      Some(preferred) => canonical(preferred).ok_or_else(|| format!("{key}: preferred `{preferred}` has no zone"))?,
      None => names[0].to_owned(),
    };
    let rest: Vec<&str> = names.into_iter().filter(|n| *n != target).collect();
    writeln!(out, "K\t{target}\t{}", rest.join("\t")).unwrap();
  }

  let meta = read(&core.join("supplemental/metaZones.json"))?;
  let mut lines = Vec::new();
  periods(meta.pointer("/supplemental/metaZones/metazoneInfo/timezone").ok_or("no metazoneInfo")?, &mut Vec::new(), &mut lines)?;
  for line in lines {
    writeln!(out, "{line}").unwrap();
  }
  for entry in meta.pointer("/supplemental/metaZones/metazones").and_then(Value::as_array).ok_or("no metazones")? {
    let Some(map) = entry.get("mapZone") else { continue };
    if str_at(map, &["_territory"]) == Some("001") {
      let (metazone, zone) = (str_at(map, &["_other"]).ok_or("mapZone without _other")?, str_at(map, &["_type"]).ok_or("mapZone without _type")?);
      writeln!(out, "G\t{metazone}\t{zone}").unwrap();
    }
  }

  let systems = read(&core.join("supplemental/numberingSystems.json"))?;
  let systems = systems.pointer("/supplemental/numberingSystems").and_then(Value::as_object).ok_or("no numbering systems")?;
  let parents = read(&core.join("supplemental/parentLocales.json"))?;
  let parents = parents.pointer("/supplemental/parentLocales/parentLocale").and_then(Value::as_object).ok_or("no parent locales")?;
  let dates = root.join("cldr-dates-full/main");
  let numbers = root.join("cldr-numbers-full/main");
  let mut own: HashMap<String, Own> = HashMap::new();
  for locale in locales(&dates)? {
    let json = read(&dates.join(&locale).join("timeZoneNames.json"))?;
    let tz = json.pointer(&format!("/main/{locale}/dates/timeZoneNames")).ok_or_else(|| format!("{locale}: no timeZoneNames"))?;
    let mut found = BTreeMap::new();
    if let Some(metazones) = tz.get("metazone") {
      names(metazones, &mut Vec::new(), &mut found);
    }
    let mut zoned = BTreeMap::new();
    if let Some(zones) = tz.get("zone") {
      names(zones, &mut Vec::new(), &mut zoned);
    }
    found.extend(zoned);
    let format = |key: &str| str_at(tz, &[key]).unwrap_or("").to_owned();
    let calendar = read(&dates.join(&locale).join("ca-gregorian.json"))?;
    let formats = calendar.pointer(&format!("/main/{locale}/dates/calendars/gregorian/dateTimeFormats")).ok_or_else(|| format!("{locale}: no dateTimeFormats"))?;
    let glue = match formats.get("short") {
      Some(Value::String(glue)) => glue.clone(),
      Some(other) => str_at(other, &["_value"]).unwrap_or("").to_owned(),
      None => String::new(),
    };
    let numeric = str_at(formats, &["availableFormats", "yMd"]).unwrap_or("").to_owned();
    let mut units = BTreeMap::new();
    if let Ok(json) = read(&root.join("cldr-units-full/main").join(&locale).join("units.json")) {
      if let Some(all) = json.pointer(&format!("/main/{locale}/units")) {
        for width in ["long", "short", "narrow"] {
          for unit in DURATION_UNITS {
            let Some(patterns) = all.pointer(&format!("/{width}/duration-{unit}")).and_then(Value::as_object) else { continue };
            for (key, pattern) in patterns {
              if let (Some(plural), Some(pattern)) = (key.strip_prefix("unitPattern-count-"), pattern.as_str()) {
                units.insert(format!("{width}\u{1f}{unit}\u{1f}{plural}"), pattern.to_owned());
              }
            }
          }
        }
      }
    }
    if let Some(separator) = read(&numbers.join(&locale).join("numbers.json")).ok().and_then(|n| str_at(&n, &["main", &locale, "numbers", "symbols-numberSystem-latn", "timeSeparator"]).map(str::to_owned)) {
      units.insert("timeSeparator".to_owned(), separator);
    }
    let system = read(&numbers.join(&locale).join("numbers.json")).ok().and_then(|n| str_at(&n, &["main", &locale, "numbers", "defaultNumberingSystem"]).map(str::to_owned));
    let digits = system.filter(|s| s != "latn").and_then(|s| systems.get(&s).and_then(|info| str_at(info, &["_digits"])).map(str::to_owned));
    own.insert(locale, Own { names: found, formats: [format("gmtFormat"), format("gmtZeroFormat"), format("hourFormat")], digits, dates: [numeric, glue], units });
  }
  let parent_of = |locale: &str| -> Option<String> {
    if locale == "und" {
      return None;
    }
    let mut parent = parents.get(locale).and_then(Value::as_str).map(str::to_owned).or_else(|| locale.rfind('-').map(|at| locale[..at].to_owned())).unwrap_or_else(|| "und".to_owned());
    while parent != "und" && !own.contains_key(&parent) {
      parent = parents.get(&parent).and_then(Value::as_str).map(str::to_owned).or_else(|| parent.rfind('-').map(|at| parent[..at].to_owned())).unwrap_or_else(|| "und".to_owned());
    }
    Some(if parent == "root" { "und".to_owned() } else { parent })
  };
  let chain = |locale: &str| -> Vec<String> {
    let mut chain = vec![locale.to_owned()];
    while let Some(parent) = parent_of(chain.last().expect("a chain is never empty")) {
      chain.push(parent);
    }
    chain
  };
  let resolved_name = |locale: &str, key: &str| -> (String, String) {
    let (mut standard, mut daylight) = (String::new(), String::new());
    for l in chain(locale) {
      if let Some((s, d)) = own.get(&l).and_then(|o| o.names.get(key)) {
        if standard.is_empty() {
          standard = s.clone();
        }
        if daylight.is_empty() {
          daylight = d.clone();
        }
      }
    }
    (standard, daylight)
  };
  let resolved_formats = |locale: &str| -> [String; 3] {
    let mut formats: [String; 3] = Default::default();
    for l in chain(locale) {
      if let Some(o) = own.get(&l) {
        for (slot, value) in formats.iter_mut().zip(&o.formats) {
          if slot.is_empty() {
            *slot = value.clone();
          }
        }
      }
    }
    formats
  };
  let resolved_dates = |locale: &str| -> [String; 2] {
    let mut dates: [String; 2] = Default::default();
    for l in chain(locale) {
      if let Some(o) = own.get(&l) {
        for (slot, value) in dates.iter_mut().zip(&o.dates) {
          if slot.is_empty() {
            *slot = value.clone();
          }
        }
      }
    }
    dates
  };
  let mut sorted: Vec<&String> = own.keys().collect();
  sorted.sort();
  for locale in sorted {
    let mine = &own[locale];
    let parent = parent_of(locale);
    if let Some(parent) = &parent {
      writeln!(out, "P\t{locale}\t{parent}").unwrap();
    }
    if let Some(digits) = &mine.digits {
      writeln!(out, "D\t{locale}\t{}", field(digits)?).unwrap();
    }
    let formats = resolved_formats(locale);
    if parent.as_deref().is_none_or(|p| resolved_formats(p) != formats) {
      writeln!(out, "F\t{locale}\t{}\t{}\t{}", field(&formats[0])?, field(&formats[1])?, field(&formats[2])?).unwrap();
    }
    let dates = resolved_dates(locale);
    if parent.as_deref().is_none_or(|p| resolved_dates(p) != dates) {
      writeln!(out, "Y\t{locale}\t{}\t{}", field(&dates[0])?, field(&dates[1])?).unwrap();
    }
    let resolved_unit = |locale: &str, key: &str| chain(locale).iter().find_map(|l| own.get(l).and_then(|o| o.units.get(key)).cloned());
    for key in mine.units.keys() {
      let here = resolved_unit(locale, key);
      if parent.as_deref().is_none_or(|p| resolved_unit(p, key) != here) {
        let value = here.unwrap_or_default();
        match key.split('\u{1f}').collect::<Vec<_>>()[..] {
          [width, unit, plural] => writeln!(out, "U\t{locale}\t{width}\t{unit}\t{plural}\t{}", field(&value)?).unwrap(),
          _ => writeln!(out, "T\t{locale}\t{}", field(&value)?).unwrap(),
        }
      }
    }
    for key in mine.names.keys() {
      let here = resolved_name(locale, key);
      if parent.as_deref().is_none_or(|p| resolved_name(p, key) != here) {
        writeln!(out, "N\t{locale}\t{key}\t{}\t{}", field(&here.0)?, field(&here.1)?).unwrap();
      }
    }
  }
  Ok(out)
}
