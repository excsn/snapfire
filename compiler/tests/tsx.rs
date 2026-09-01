use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, run_snapfirec};

use crate::common::get_snapfirec_cmd;

#[test]
fn test_tsx_is_preserved() {
  let fixture = Fixture::new("tsx-support");
  
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let output_file = fixture.root().join("dist/component.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  assert!(!content.contains(": string"));
  
  // JSX is deliberately not lowered when 'jsx' is unset.
  assert!(content.contains("<div>Hello, {props.name}</div>"));
}

fn with_tsconfig(fixture: &Fixture, tsconfig: &str) {
  fs::write(fixture.root().join("tsconfig.json"), tsconfig).unwrap();
}

#[test]
fn test_automatic_runtime_lowers_jsx_and_imports_the_runtime() {
  let fixture = Fixture::new("jsx-automatic");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/badge.js")).unwrap();
  assert!(content.contains(r#"from "react/jsx-runtime""#), "{content}");
  assert!(content.contains("_jsx(\"span\""), "{content}");
  assert!(!content.contains("<span"), "no markup survives: {content}");
  assert!(!content.contains(": string"), "types are still stripped: {content}");
}

#[test]
fn test_an_import_used_only_as_an_element_survives_stripping() {
  let fixture = Fixture::new("jsx-automatic");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/card.js")).unwrap();
  assert!(content.contains(r#"import { Badge } from "./badge.js""#), "{content}");
  assert!(content.contains("_jsx(Badge,"), "{content}");
}

#[test]
fn test_jsx_import_source_redirects_the_runtime() {
  let fixture = Fixture::new("jsx-automatic");
  with_tsconfig(
    &fixture,
    r#"{"compilerOptions":{"outDir":"dist","jsx":"react-jsx","jsxImportSource":"preact"},"include":["input"]}"#,
  );

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/badge.js")).unwrap();
  assert!(content.contains(r#"from "preact/jsx-runtime""#), "{content}");
}

#[test]
fn test_dev_runtime_is_a_separate_module() {
  let fixture = Fixture::new("jsx-automatic");
  with_tsconfig(
    &fixture,
    r#"{"compilerOptions":{"outDir":"dist","jsx":"react-jsxdev"},"include":["input"]}"#,
  );

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/badge.js")).unwrap();
  assert!(content.contains(r#"from "react/jsx-dev-runtime""#), "{content}");
}

#[test]
fn test_the_runtime_import_is_checked_against_the_import_map() {
  let fixture = Fixture::new("jsx-automatic");
  fs::write(
    fixture.root().join("importmap.json"),
    r#"{"imports":{"react":"/vendor/react.js"}}"#,
  )
  .unwrap();

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .arg("--import-map")
    .arg("importmap.json")
    .assert()
    .failure()
    .stderr(predicate::str::contains("'react/jsx-runtime' is not resolved"));
}

#[test]
fn test_the_classic_runtime_is_refused_with_guidance() {
  let fixture = Fixture::new("jsx-automatic");
  with_tsconfig(&fixture, r#"{"compilerOptions":{"outDir":"dist","jsx":"react"},"include":["input"]}"#);

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("automatic runtime only"));
}

#[test]
fn test_an_unknown_jsx_mode_is_refused() {
  let fixture = Fixture::new("jsx-automatic");
  with_tsconfig(&fixture, r#"{"compilerOptions":{"outDir":"dist","jsx":"solid"},"include":["input"]}"#);

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("is not a recognised mode"));
}