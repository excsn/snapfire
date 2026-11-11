//! What a zone observes at an instant, with daylight saving counted as the rearguard form of the IANA database counts it, the form ICU reads.

use snapfire_time::{observed_in, saving_in_year, Observed, Source};

const JAN_2024: f64 = 1704067200000.0;
const JUL_2024: f64 = 1720000000000.0;
const NOV_2026: f64 = 1794312000000.0;

const OBSERVED: &[(&str, f64, i32, i32)] = &[
  ("America/New_York", JAN_2024, -18000, 0),
  ("America/New_York", JUL_2024, -14400, 3600),
  ("America/New_York", 4118083200000.0, -14400, 3600),
  ("Europe/Dublin", JAN_2024, 0, 0),
  ("Europe/Dublin", JUL_2024, 3600, 3600),
  ("Africa/Casablanca", JUL_2024, 3600, 3600),
  ("Africa/Casablanca", 1710892800000.0, 0, 0),
  ("Africa/Windhoek", 962409600000.0, 3600, 0),
  ("Africa/Windhoek", 946684800000.0, 7200, 3600),
  ("Australia/Lord_Howe", JAN_2024, 39600, 1800),
  ("Australia/Lord_Howe", JUL_2024, 37800, 0),
  ("Antarctica/Troll", JUL_2024, 7200, 7200),
  ("Asia/Tokyo", JUL_2024, 32400, 0),
  ("Europe/London", 0.0, 3600, 0),
  ("US/Eastern", JUL_2024, -14400, 3600),
];

const SAVING: &[(&str, f64, Option<i32>)] = &[
  ("America/New_York", NOV_2026, Some(3600)),
  ("America/Coyhaique", NOV_2026, None),
  ("America/Coyhaique", 1748736000000.0, Some(3600)),
  ("Antarctica/Troll", NOV_2026, Some(7200)),
  ("Asia/Tokyo", NOV_2026, None),
  ("Africa/Casablanca", NOV_2026, Some(3600)),
  ("Europe/Dublin", NOV_2026, Some(3600)),
  ("Australia/Lord_Howe", NOV_2026, Some(1800)),
];

fn each_source(check: impl Fn(Source)) {
  for source in [Source::Compiled, Source::System].into_iter().filter(|s| s.built()) {
    check(source);
  }
}

#[test]
fn a_zone_observes_its_offset_and_saving_from_each_source_built_in() {
  each_source(|source| {
    let failed: Vec<String> = OBSERVED
      .iter()
      .filter_map(|&(zone, ms, offset, saving)| {
        let got = observed_in(source, zone, ms);
        (got != Ok(Observed { offset, saving })).then(|| format!("{zone} at {ms}: got {got:?}, want offset {offset} saving {saving}"))
      })
      .collect();
    assert!(failed.is_empty(), "{source:?}:\n{}", failed.join("\n"));
  });
}

#[test]
fn a_year_has_the_saving_its_zone_observes_in_it() {
  each_source(|source| {
    let failed: Vec<String> = SAVING
      .iter()
      .filter_map(|&(zone, ms, want)| {
        let got = saving_in_year(source, zone, ms);
        (got != Ok(want)).then(|| format!("{zone} at {ms}: got {got:?}, want {want:?}"))
      })
      .collect();
    assert!(failed.is_empty(), "{source:?}:\n{}", failed.join("\n"));
  });
}

#[test]
fn a_zone_the_source_does_not_know_is_named() {
  each_source(|source| {
    assert_eq!(observed_in(source, "Nowhere/Atlantis", 0.0), Err(snapfire_time::Error::UnknownZone("Nowhere/Atlantis".to_owned())));
  });
}
