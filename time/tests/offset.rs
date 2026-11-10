//! Each offset is `new Date(ms).getTimezoneOffset()` as node v26.8.1 answers it with `TZ` set to the zone.

use snapfire_time::{offset_in, Source};

const NODE: &[(&str, f64, f64)] = &[
  ("America/New_York", 1726011440123.0, 240.0),
  ("America/New_York", 1704067200000.0, 300.0),
  ("America/New_York", 1710054000000.0, 240.0),
  ("America/New_York", -2208988800000.0, 300.0),
  ("Asia/Kolkata", 1726011440123.0, -330.0),
  ("Asia/Kolkata", -2208988800000.0, -321.0),
  ("Europe/Berlin", 1726011440123.0, -120.0),
  ("Europe/Berlin", 1704067200000.0, -60.0),
  ("Australia/Lord_Howe", 1726011440123.0, -630.0),
  ("Australia/Lord_Howe", 1704067200000.0, -660.0),
  ("Australia/Lord_Howe", -2208988800000.0, -600.0),
  ("America/St_Johns", 1726011440123.0, 150.0),
  ("America/St_Johns", 1704067200000.0, 210.0),
  ("UTC", 1726011440123.0, 0.0),
];

fn each_source(check: impl Fn(Source)) {
  for source in [Source::Compiled, Source::System].into_iter().filter(|s| s.built()) {
    check(source);
  }
}

#[test]
fn a_build_without_a_source_says_which_features_give_one() {
  if Source::Compiled.built() || Source::System.built() {
    return;
  }
  assert_eq!(snapfire_time::local_offset(0.0), Err(snapfire_time::Error::NoSource));
  assert!(snapfire_time::Error::NoSource.to_string().contains("`tz_compiled`"));
}

#[test]
fn offsets_answer_what_node_answers_from_each_source_built_in() {
  each_source(|source| {
    let failed: Vec<String> = NODE
      .iter()
      .filter_map(|(zone, ms, want)| match offset_in(source, zone, *ms) {
        Ok(got) if got == *want => None,
        other => Some(format!("{source:?} {zone} {ms}: {other:?}, want {want}")),
      })
      .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
  });
}

#[test]
fn an_unknown_zone_and_an_instant_out_of_range_are_errors() {
  each_source(|source| {
    assert!(offset_in(source, "Not/AZone", 0.0).is_err());
    assert!(offset_in(source, "UTC", f64::NAN).is_err());
    assert!(offset_in(source, "UTC", 8.64e15 + 1.0).is_err());
  });
}

#[test]
fn the_system_zone_reads_tz_as_node_does() {
  use snapfire_time::zone_from_tz;
  assert_eq!(zone_from_tz(Some("America/New_York")), Some("America/New_York".to_owned()));
  assert_eq!(zone_from_tz(Some(":America/New_York")), Some("America/New_York".to_owned()));
  assert_eq!(zone_from_tz(Some("/usr/share/zoneinfo/Europe/Berlin")), Some("Europe/Berlin".to_owned()));
  assert_eq!(zone_from_tz(Some("")), None);
  assert_eq!(zone_from_tz(None), None);
}
