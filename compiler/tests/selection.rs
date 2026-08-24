use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

/// Compiled artefacts only. The tracking manifest and the preload manifest are build metadata, so
/// they are not what these assertions are about.
fn emitted(root: &Path) -> Vec<String> {
  const METADATA: [&str; 1] = [".snapfire-build.json"];

  let dist = root.join("dist");
  let mut found = Vec::new();

  fn walk(dir: &Path, base: &Path, found: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
      return;
    };
    for entry in entries.flatten() {
      let path = entry.path();
      let name = entry.file_name().to_string_lossy().into_owned();
      if path.is_dir() {
        walk(&path, base, found);
      } else if !name.starts_with('.') && !METADATA.contains(&name.as_ref()) {
        found.push(path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/"));
      }
    }
  }

  walk(&dist, &dist, &mut found);
  found.sort();
  found
}

#[test]
fn test_include_accepts_glob_patterns() {
  let fixture = Fixture::new("include-globs");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert_eq!(
    emitted(fixture.root()),
    vec!["src/index.js", "src/ui/button.js", "vendor/legacy.js"]
  );
}

#[test]
fn test_exclude_filters_matched_files() {
  let fixture = Fixture::new("include-globs");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/src/ui/button.test.js")));
}

#[test]
fn test_single_star_does_not_cross_directories() {
  let fixture = Fixture::new("include-globs");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/vendor/legacy.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/vendor/deep/skipped.js")));
}

#[test]
fn test_root_dir_is_computed_from_the_common_prefix() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root())).stdout(predicate::str::contains(r#"Root Dir: "src/ui""#));

  assert_eq!(emitted(fixture.root()), vec!["button.js", "panel.js"]);
}

#[test]
fn test_explicit_root_dir_pins_the_output_layout() {
  let fixture = Fixture::new("explicit-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root())).stdout(predicate::str::contains(r#"Root Dir: "src""#));

  assert_eq!(emitted(fixture.root()), vec!["ui/button.js", "ui/panel.js"]);
}

#[test]
fn test_files_list_bypasses_exclude() {
  let fixture = Fixture::new("files-list");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert_eq!(emitted(fixture.root()), vec!["entry.js"]);
}

#[test]
fn test_absent_include_defaults_to_everything_and_warns() {
  let fixture = Fixture::new("default-include");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()))
    .stdout(predicate::str::contains(r#"Sources:  ["**/*"]"#))
    .stderr(predicate::str::contains("compiling every file under"));

  assert_eq!(emitted(fixture.root()), vec!["scripts/tool.js", "src/index.js"]);
}

#[test]
fn test_default_exclude_still_prunes_node_modules() {
  let fixture = Fixture::new("default-include");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/node_modules")));
}

#[test]
fn test_patterns_resolve_against_the_config_directory() {
  let fixture = Fixture::new("nested-config");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--config")
      .arg("configs/tsconfig.build.json"),
  );

  assert_eq!(emitted(fixture.root()), vec!["index.js"]);
}

#[test]
fn test_target_below_es2017_fails() {
  let fixture = Fixture::new("target-too-old");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains(r#"'target': "es5" cannot be honoured"#));
}

#[test]
fn test_a_satisfiable_target_is_not_mentioned() {
  let fixture = Fixture::new("target-modern");

  // tsc uses this key for real, so a project setting it is correct and has nothing to fix. A
  // warning on every build of every correct config only teaches people to ignore warnings.
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root())).stderr(predicate::str::contains("target").not());

  assert_eq!(emitted(fixture.root()), vec!["index.js"]);
}

#[test]
fn test_an_unrecognised_target_is_reported() {
  let fixture = Fixture::new("target-nonsense");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()))
    .stderr(predicate::str::contains(r#"'target': "es20200" is not recognised"#));
}
