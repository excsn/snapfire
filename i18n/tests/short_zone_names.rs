//! `tests/node/short_zone_names.txt` holds the `timeZoneName` part node v26.8.1 (ICU 78.3, CLDR 48) writes under `timeZoneName: "short"`, for every locale it ships, eight zones and an instant in September and one in January 2024. One row per line: locale, zone, instant, name.

use snapfire_i18n::{short_zone_name, ZoneRules};
use snapfire_time::{observed_in, saving_in_year, Source};

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
fn a_zones_short_name_is_what_node_writes() {
  let mut failed = Vec::new();
  let mut total = 0;
  for line in include_str!("node/short_zone_names.txt").lines() {
    let [locale, zone, at, want] = line.split('\t').collect::<Vec<_>>()[..] else { panic!("a row of four fields: {line}") };
    total += 1;
    let got = short_zone_name(locale, zone, at.parse().unwrap(), &Compiled);
    if got != want {
      failed.push(format!("{locale} {zone} {at}: got {got}, want {want}"));
    }
  }
  assert!(failed.is_empty(), "{} of {total}:\n{}", failed.len(), failed.iter().take(40).cloned().collect::<Vec<_>>().join("\n"));
}
