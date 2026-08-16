use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

/// The example ships in the published crate and the README tells people to run it, so the claims it
/// makes are worth holding to. Assertions stay on documented behaviour rather than emitted text, so
/// editing the example does not mean editing this file.
fn example() -> Fixture {
  Fixture::from_path(Path::new("example"))
}

#[test]
fn test_the_shipped_example_builds() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
}

#[test]
fn test_the_example_mirrors_its_nested_tree() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root())).stdout(predicate::str::contains(r#"Root Dir: "src""#));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/ui/toast.js")));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/ui/toast.css")));
}

#[test]
fn test_the_example_rewrites_its_relative_imports() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();
  assert!(content.contains(r#"from "./ui/toast.js""#));
  assert!(content.contains(r#"from "./utils.js""#));
}

#[test]
fn test_the_example_delivers_its_imported_asset() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();
  assert!(content.contains("./data/config.json"));
  assert!(
    predicate::path::exists().eval(&fixture.root().join("dist/data/config.json")),
    "the module names it, so the build has to deliver it"
  );
}

#[test]
fn test_the_examples_import_map_covers_its_externals() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--import-map")
      .arg("importmap.json"),
  )
  .stdout(predicate::str::contains("All externals resolve"));
}

#[test]
fn test_the_example_defers_its_dynamic_import() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = fs::read_to_string(fixture.root().join("dist/preload-manifest.json")).unwrap();
  let entry = manifest.lines().find(|l| l.contains("\"index.js\"")).unwrap();

  assert!(!entry.contains("editor.js"), "a deferred chunk was preloaded");
  assert!(manifest.contains("\"editor.js\""), "it is still an entry of its own");
}

#[test]
fn test_the_example_flattens_its_nested_css() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/ui/toast.css")).unwrap();
  assert!(content.contains(".sonner-toast span"));
  assert!(!content.contains('&'));
}

#[test]
fn test_the_example_builds_without_warnings() {
  let fixture = example();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--import-map")
      .arg("importmap.json"),
  )
  .stderr(predicate::str::is_empty());
}
