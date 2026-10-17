//! Every case built as an application of its own, its outcome held against `outcomes.tsv`. `PATTERNS_BLESS=1` rewrites the file from this run.

use patterns::{app_of, cases, outcome, root};

#[test]
fn every_case_builds_to_the_outcome_it_is_held_to() {
  let scratch = std::env::temp_dir().join(format!("fsr-patterns-{}", std::process::id()));
  let mut lines = Vec::new();
  for case in cases() {
    let app = app_of(&case, &scratch);
    lines.push(format!("{}\t{}", case.name, outcome(&app).0));
    let _ = std::fs::remove_dir_all(app.parent().unwrap());
  }
  let _ = std::fs::remove_dir_all(&scratch);
  let actual = lines.join("\n") + "\n";
  let held_at = root().join("outcomes.tsv");
  if std::env::var_os("PATTERNS_BLESS").is_some_and(|v| v == "1") {
    std::fs::write(&held_at, &actual).unwrap();
    return;
  }
  let held = std::fs::read_to_string(&held_at).unwrap_or_default();
  let changed: Vec<String> = held.lines().zip(actual.lines()).filter(|(was, now)| was != now).map(|(was, now)| format!("was {was}\n now {now}")).collect();
  assert!(changed.is_empty() && held.lines().count() == actual.lines().count(), "{} outcomes changed; PATTERNS_BLESS=1 takes this run:\n{}", changed.len(), changed.join("\n"));
}

#[test]
fn fsr_markers_come_out_and_what_they_hold_stays() {
  use patterns::without_fsr_markers;
  assert_eq!(without_fsr_markers("<button><sf-s data-sf-m=\"0\"><b>Light</b></sf-s>off</button>"), "<button><b>Light</b>off</button>");
  assert_eq!(without_fsr_markers("<p>a<template data-sf-children><i>held</i></template><script type=\"application/json\" data-sf-props=\"x\">{}</script></p>"), "<p>a</p>");
  assert_eq!(without_fsr_markers("<template><b>plain</b></template>"), "<template><b>plain</b></template>");
}
