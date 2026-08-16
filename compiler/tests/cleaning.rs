use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

fn build(fixture: &Fixture) {
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));
}

#[test]
fn test_a_manifest_records_what_was_emitted() {
  let fixture = Fixture::new("computed-root");
  build(&fixture);

  let manifest = fs::read_to_string(fixture.root().join("dist/.snapfirec-manifest")).unwrap();
  let mut listed: Vec<&str> = manifest.lines().collect();
  listed.sort();

  // The preload manifest is an output too, so it is tracked and therefore prunable.
  assert_eq!(listed, vec!["button.js", "panel.js", "preload-manifest.json"]);
}

#[test]
fn test_a_renamed_source_takes_its_old_output_with_it() {
  let fixture = Fixture::new("computed-root");
  build(&fixture);

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/panel.js")));

  fs::rename(
    fixture.root().join("src/ui/panel.ts"),
    fixture.root().join("src/ui/board.ts"),
  )
  .unwrap();

  build(&fixture);

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/board.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/panel.js")));
}

#[test]
fn test_files_snapfirec_did_not_write_are_never_removed() {
  let fixture = Fixture::new("computed-root");
  build(&fixture);

  let foreign = fixture.root().join("dist/favicon.ico");
  fs::write(&foreign, "hand written").unwrap();

  fs::remove_file(fixture.root().join("src/ui/panel.ts")).unwrap();
  build(&fixture);

  assert!(predicate::path::exists().eval(&foreign));
  assert_eq!(fs::read_to_string(&foreign).unwrap(), "hand written");
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/panel.js")));
}

#[test]
fn test_dropping_the_minify_flag_removes_the_min_graph() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/button.min.js")));

  build(&fixture);
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/button.min.js")));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/button.js")));
}

#[test]
fn test_a_failed_build_prunes_nothing() {
  let fixture = Fixture::new("computed-root");
  build(&fixture);

  fs::write(fixture.root().join("src/ui/panel.ts"), "const broken = ;").unwrap();

  let mut cmd = get_snapfirec_cmd();
  cmd.arg("--root").arg(fixture.root()).assert().failure();

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/panel.js")));
}
