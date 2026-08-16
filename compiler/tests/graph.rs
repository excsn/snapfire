use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

fn manifest(root: &Path) -> String {
  fs::read_to_string(root.join("dist/preload-manifest.json")).expect("no preload manifest")
}

#[test]
fn test_entry_points_are_the_modules_nothing_imports() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());

  assert!(manifest.contains("\"index.js\""));
  assert!(manifest.contains("\"standalone.js\""));
  assert!(!manifest.contains("\"deep/a.js\":"), "an imported module is not an entry");
}

#[test]
fn test_transitive_dependencies_are_listed() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());
  let line = manifest.lines().find(|l| l.contains("\"index.js\"")).unwrap();

  assert!(line.contains("deep/a.js"), "the direct import is missing");
  assert!(line.contains("deep/b.js"), "the transitive import is missing");
}

#[test]
fn test_a_cycle_does_not_hang_the_walk() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let line = manifest(fixture.root());
  let line = line.lines().find(|l| l.contains("\"index.js\"")).unwrap();

  assert_eq!(line.matches("deep/a.js").count(), 1);
  assert_eq!(line.matches("deep/b.js").count(), 1);
}

#[test]
fn test_dynamic_imports_are_not_preloaded() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());
  let entry = manifest.lines().find(|l| l.contains("\"index.js\"")).unwrap();

  assert!(!entry.contains("deferred.js"), "a deferred import was preloaded");
  assert!(manifest.contains("\"deferred.js\""), "it is still an entry of its own");
}

#[test]
fn test_stylesheets_are_not_module_preloads() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(!manifest(fixture.root()).contains(".css"));
}

#[test]
fn test_without_a_public_path_the_manifest_stays_relative() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());
  assert!(manifest.contains("\"index.js\""));
  assert!(!manifest.contains("\"/"));
}

#[test]
fn test_a_public_path_prefixes_every_url() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--public-path").arg("/assets"));

  let manifest = manifest(fixture.root());
  assert!(manifest.contains("\"/assets/index.js\""));
  assert!(manifest.contains("\"/assets/deep/a.js\""));
}

#[test]
fn test_a_complete_import_map_passes() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--import-map")
      .arg("complete.json"),
  )
  .stdout(predicate::str::contains("All externals resolve"));
}

#[test]
fn test_a_missing_entry_fails_the_build() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .arg("--import-map")
    .arg("empty.json")
    .assert()
    .failure()
    .stderr(predicate::str::contains("'lit' is not resolved by"));
}

#[test]
fn test_a_trailing_slash_key_resolves_a_prefix() {
  let fixture = Fixture::new("graph");

  fs::write(
    fixture.root().join("input/uses-prefix.ts"),
    "import { debounce } from 'lodash/debounce';\nexport const d = debounce;\n",
  )
  .unwrap();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--import-map")
      .arg("prefix.json"),
  )
  .stdout(predicate::str::contains("All externals resolve"));
}

#[test]
fn test_scopes_need_a_public_path_to_be_checked() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .arg("--import-map")
    .arg("scoped.json")
    .assert()
    .failure()
    .stderr(predicate::str::contains("defines scopes"))
    .stderr(predicate::str::contains("Without --public-path"));
}

#[test]
fn test_a_scope_resolves_once_the_url_is_known() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--import-map")
      .arg("scoped.json")
      .arg("--public-path")
      .arg("/assets"),
  )
  .stdout(predicate::str::contains("All externals resolve"));
}
