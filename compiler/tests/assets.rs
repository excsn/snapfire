use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_imported_assets_ship_without_a_flag() {
  let fixture = Fixture::new("asset-copy");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/data/config.json")));
}

#[test]
fn test_unreferenced_files_stay_behind_without_the_flag() {
  let fixture = Fixture::new("asset-copy");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/img/logo.png")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/README.md")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/types.d.ts")));
}

#[test]
fn test_copy_assets_sweeps_everything_uncompiled() {
  let fixture = Fixture::new("asset-copy");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--copy-assets"));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/img/logo.png")));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/README.md")));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/types.d.ts")));
}

#[test]
fn test_compiled_assets_are_not_also_copied() {
  let fixture = Fixture::new("asset-copy");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--copy-assets"));

  let source = fs::read_to_string(fixture.root().join("input/theme.css")).unwrap();
  let emitted = fs::read_to_string(fixture.root().join("dist/theme.css")).unwrap();

  assert!(emitted.contains("color: red"));
  assert_ne!(emitted, source, "the stylesheet was copied verbatim rather than compiled");
}

#[test]
fn test_every_emitted_specifier_resolves_in_the_output() {
  let fixture = Fixture::new("asset-copy");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let dist = fixture.root().join("dist");
  let content = fs::read_to_string(dist.join("index.js")).unwrap();

  for specifier in ["./data/config.json", "./theme.css"] {
    assert!(content.contains(specifier), "{specifier} missing from the emitted module");
    assert!(
      predicate::path::exists().eval(&dist.join(specifier.trim_start_matches("./"))),
      "{specifier} is imported but was never delivered"
    );
  }
}
