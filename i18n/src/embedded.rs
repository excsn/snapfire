//! The crate's functions over the data built into it, each the [`Data`] method of its name.

use std::collections::BTreeMap;
use std::sync::{Mutex, PoisonError};

use crate::{Calendar, Data, NameKind, ZoneRules};

/// The fields of a record's value as a slice, made once for each record.
fn listed(value: &'static str) -> &'static [&'static str] {
  static HELD: Mutex<BTreeMap<usize, &'static [&'static str]>> = Mutex::new(BTreeMap::new());
  if value.is_empty() {
    return &[];
  }
  let mut held = HELD.lock().unwrap_or_else(PoisonError::into_inner);
  held.entry(value.as_ptr() as usize).or_insert_with(|| Box::leak(value.split('\t').collect::<Vec<_>>().into_boxed_slice()))
}

/// [`Data::cldr_version`] of the embedded data.
pub fn cldr_version() -> &'static str {
  Data::embedded().cldr_version()
}

/// Whether this build holds the display names, the `display_names` feature.
pub fn has_display_names() -> bool {
  Data::embedded().has_display_names()
}

/// [`Data::display_name`] from the embedded data.
pub fn display_name(tag: &str, kind: NameKind, code: &str, style: &str, standard: bool) -> Option<String> {
  Data::embedded().display_name(tag, kind, code, style, standard)
}

/// [`Data::in_daylight`] from the embedded data.
pub fn in_daylight(zone: &str, ms: f64, rules: &impl ZoneRules) -> bool {
  Data::embedded().in_daylight(zone, ms, rules)
}

/// [`Data::zone_name`] from the embedded data.
pub fn zone_name(locale: &str, zone: &str, daylight: bool, now: f64, rules: &impl ZoneRules) -> String {
  Data::embedded().zone_name(locale, zone, daylight, now, rules)
}

/// [`Data::short_zone_name`] from the embedded data.
pub fn short_zone_name(locale: &str, zone: &str, ms: f64, rules: &impl ZoneRules) -> String {
  Data::embedded().short_zone_name(locale, zone, ms, rules)
}

/// [`Data::calendar`] from the embedded data.
pub fn calendar(tag: &str) -> Option<Calendar> {
  Data::embedded().calendar(tag)
}

/// [`Data::preferred_calendars`] from the embedded data.
pub fn preferred_calendars(region: &str) -> &'static [&'static str] {
  listed(Data::embedded().preferred_calendars_record(region))
}

/// [`Data::region_zones`] from the embedded data.
pub fn region_zones(region: &str) -> &'static [&'static str] {
  listed(Data::embedded().region_zones_record(region))
}

/// [`Data::numeric_date`] from the embedded data.
pub fn numeric_date(tag: &str, year: i64, month: u32, day: u32) -> Option<String> {
  Data::embedded().numeric_date(tag, year, month, day)
}

/// [`Data::numeric_date_parts`] from the embedded data.
pub fn numeric_date_parts(tag: &str, year: i64, month: u32, day: u32) -> Option<Vec<(&'static str, String)>> {
  Data::embedded().numeric_date_parts(tag, year, month, day)
}

/// [`Data::clock`] from the embedded data.
pub fn clock(tag: &str, hour: u32, minute: u32, second: u32) -> String {
  Data::embedded().clock(tag, hour, minute, second)
}

/// [`Data::date_with_time`] from the embedded data.
pub fn date_with_time(tag: &str, date: &str, time: &str) -> String {
  Data::embedded().date_with_time(tag, date, time)
}

/// [`Data::duration_unit`] from the embedded data.
pub fn duration_unit(tag: &str, width: &str, unit: &str, plural: &str) -> Option<&'static str> {
  Data::embedded().duration_unit(tag, width, unit, plural)
}

/// [`Data::time_separator`] from the embedded data.
pub fn time_separator(tag: &str) -> &'static str {
  Data::embedded().time_separator(tag)
}

/// [`Data::supported`] from the embedded data.
pub fn supported(tag: &str) -> bool {
  Data::embedded().supported(tag)
}

/// [`Data::numbering_system`] from the embedded data.
pub fn numbering_system(tag: &str) -> &'static str {
  Data::embedded().numbering_system(tag)
}

/// [`Data::hour_cycle`] from the embedded data.
pub fn hour_cycle(tag: &str) -> &'static str {
  Data::embedded().hour_cycle(tag)
}

/// [`Data::time_widths`] from the embedded data.
pub fn time_widths(tag: &str, cycle: &str, minute: bool, second: bool) -> [bool; 3] {
  Data::embedded().time_widths(tag, cycle, minute, second)
}
