use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_no_declarations_without_the_flag() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/index.d.ts")));
}

#[test]
fn test_declaration_adds_one_file_per_source() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--declaration"));

  let dist = fixture.root().join("dist");

  for name in ["index.js", "index.d.ts", "state.js", "state.d.ts"] {
    assert!(predicate::path::exists().eval(&dist.join(name)), "{name} is missing");
  }
}

#[test]
fn test_declaration_is_read_from_tsconfig_too() {
  let fixture = Fixture::new("declaration-configured");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.d.ts")));
}

#[test]
fn test_declarations_name_what_the_modules_name() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--declaration"));

  let declared = fs::read_to_string(fixture.root().join("dist/index.d.ts")).unwrap();

  // The source imports './state', so the declaration has to reach 'state.d.ts' the same way the
  // emitted module reaches 'state.js'.
  assert!(declared.contains("\"./state.js\""), "specifier was not resolved: {declared}");
  assert!(!declared.contains("\"./state\""), "unresolved specifier survived: {declared}");
}

#[test]
fn test_declarations_leave_bare_specifiers_alone() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--declaration"));

  let declared = fs::read_to_string(fixture.root().join("dist/index.d.ts")).unwrap();

  assert!(!declared.contains("some-package.js"), "a bare specifier was rewritten: {declared}");
}

#[test]
fn test_the_minified_graph_gets_no_declarations_of_its_own() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--declaration")
      .arg("--minify"),
  );

  let dist = fixture.root().join("dist");

  assert!(predicate::path::exists().eval(&dist.join("index.min.js")));
  assert!(predicate::path::exists().eval(&dist.join("index.d.ts")));
  assert!(!predicate::path::exists().eval(&dist.join("index.min.d.ts")));
}

#[test]
fn test_declarations_are_not_preloaded() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--declaration"));

  let facts = fs::read_to_string(fixture.root().join("dist/.snapfire-build.json")).unwrap();

  // `outputs` names every emitted file, declarations included. The graph and its entry points are
  // what a page preloads from, and a declaration is never fetched, so naming one there would tell
  // the browser to load a file it cannot use.
  let preloaded: String = facts
    .lines()
    .filter(|line| !line.trim_start().starts_with(r#""outputs""#))
    .collect();

  assert!(!preloaded.contains(".d.ts"), "a declaration reached the graph: {facts}");
}

#[test]
fn test_declarations_get_no_source_map() {
  let fixture = Fixture::new("declaration-emit");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--declaration")
      .arg("--source-map"),
  );

  let dist = fixture.root().join("dist");

  assert!(predicate::path::exists().eval(&dist.join("index.js.map")));
  assert!(!predicate::path::exists().eval(&dist.join("index.d.ts.map")));
}

#[test]
fn test_an_export_that_needs_inference_fails_the_build() {
  let fixture = Fixture::new("declaration-unannotated");

  let mut cmd = get_snapfirec_cmd();
  let assert = cmd.arg("--root").arg(fixture.root()).assert();

  assert
    .failure()
    .stderr(predicate::str::contains("Error compiling"))
    .stderr(predicate::str::contains("inferred.ts:3:14"))
    .stderr(predicate::str::contains("TS9010"));
}
