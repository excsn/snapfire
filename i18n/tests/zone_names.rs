//! `tests/node/zone_names.txt` holds the zone name in `new Date(ms).toTimeString()` as node v26.8.1 (ICU 78.3, CLDR 48, tz 2026c) printed it on 2026-11-10, with `TZ` set to the zone and `LANG` to the locale. One row per line: zone, locale, instant, name.

use snapfire_i18n::{in_daylight, zone_name, ZoneRules};
use snapfire_time::{equivalent_time, observed_in, saving_in_year, Source};

const NOW: f64 = 1_794_312_000_000.0;

/// Rows the compiled rules answer differently from node's tz 2026c: tz 2026e keeps Manitoba and the Northwest Territories on daylight time from November 2026, the equivalent year V8 reads for 1900.
const NEWER_RULES: &[(&str, &str, &str)] = &[("America/Inuvik", "-2208988800000", "Mountain Daylight Time"), ("America/Winnipeg", "-2208988800000", "Central Daylight Time")];

struct Compiled;

impl ZoneRules for Compiled {
  fn observed(&self, zone: &str, ms: f64) -> Option<(i32, i32)> {
    observed_in(Source::Compiled, zone, ms).ok().map(|o| (o.offset, o.saving))
  }

  fn saving_in_year(&self, zone: &str, ms: f64) -> Option<i32> {
    saving_in_year(Source::Compiled, zone, ms).ok().flatten()
  }
}

#[test]
fn a_zone_is_named_as_node_names_it() {
  let rows = include_str!("node/zone_names.txt");
  let mut failed = Vec::new();
  let mut total = 0;
  for line in rows.lines() {
    let [zone, locale, at, want] = line.split('\t').collect::<Vec<_>>()[..] else { panic!("a row of four fields: {line}") };
    let want = NEWER_RULES.iter().find(|(z, t, _)| *z == zone && *t == at && locale == "en-US").map_or(want, |(_, _, newer)| *newer);
    let ms: f64 = at.parse().unwrap();
    total += 1;
    let daylight = in_daylight(zone, equivalent_time(ms), &Compiled);
    let got = zone_name(locale, zone, daylight, NOW, &Compiled);
    if got != want {
      failed.push(format!("{zone} {locale} {ms}: got {got}, want {want}"));
    }
  }
  assert!(failed.is_empty(), "{} of {total}:\n{}", failed.len(), failed.join("\n"));
}

#[test]
fn a_zone_icu_does_not_know_is_named_by_its_zero_offset() {
  assert_eq!(zone_name("en-US", "Nowhere/Atlantis", false, NOW, &Compiled), "GMT+00:00");
  assert_eq!(zone_name("fr", "Nowhere/Atlantis", true, NOW, &Compiled), "UTC+00:00");
}
