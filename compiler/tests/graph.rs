use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

fn manifest(root: &Path) -> String {
  fs::read_to_string(root.join("dist/.snapfire-build.json")).expect("no build facts")
}

/// The `graph` record for one module. `entries` names the same modules, so a plain substring
/// search would find that line first and report every entry as a dependency of every other.
fn graph_line<'a>(facts: &'a str, module: &str) -> &'a str {
  let key = format!("\"{module}\":");

  facts
    .lines()
    .find(|line| line.trim_start().starts_with(&key))
    .unwrap_or_else(|| panic!("no graph record for {module} in {facts}"))
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
  let line = graph_line(&manifest, "index.js");

  assert!(line.contains("deep/a.js"), "the direct import is missing");
  assert!(line.contains("deep/b.js"), "the transitive import is missing");
}

#[test]
fn test_a_cycle_does_not_hang_the_walk() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let line = manifest(fixture.root());
  let line = graph_line(&line, "index.js").to_string();

  assert_eq!(line.matches("deep/a.js").count(), 1);
  assert_eq!(line.matches("deep/b.js").count(), 1);
}

#[test]
fn test_dynamic_imports_are_not_preloaded() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());
  let entry = graph_line(&manifest, "index.js");

  assert!(!entry.contains("deferred.js"), "a deferred import was preloaded");
  assert!(manifest.contains("\"deferred.js\""), "it is still an entry of its own");
}

#[test]
fn test_stylesheets_are_not_module_preloads() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  // `outputs` names every emitted file, stylesheets included. The graph is what a page preloads
  // from, and a stylesheet needs a different `rel`, so listing one there produces dead markup.
  let facts = manifest(fixture.root());
  let preloaded: String = facts
    .lines()
    .filter(|line| !line.trim_start().starts_with(r#""outputs""#))
    .collect();

  assert!(!preloaded.contains(".css"), "a stylesheet reached the graph: {facts}");
}

#[test]
fn test_the_facts_stay_in_the_output_directorys_own_terms() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let manifest = manifest(fixture.root());
  assert!(manifest.contains("\"index.js\""));
  assert!(!manifest.contains("\"/"));
}

#[test]
fn test_a_public_path_is_recorded_and_never_baked_into_the_paths() {
  let fixture = Fixture::new("graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--public-path").arg("/assets/"));

  let manifest = manifest(fixture.root());

  // One build is mountable anywhere, so where it is served is a field rather than a prefix on
  // every path. A consumer joins the two; a packager that mounts it elsewhere ignores the field.
  assert!(manifest.contains("\"publicPath\": \"/assets/\""));
  assert!(manifest.contains("\"index.js\""));
  assert!(!manifest.contains("\"/assets/index.js\""));
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
