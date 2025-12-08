use predicates::prelude::*;
use std::fs;

// Declare the common module
mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_import_rewriter_appends_js_extension() {
  // 1. Setup
  let fixture = Fixture::new("import-rewrite");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  // 3. Assertion
  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  // Verify that the import path was correctly rewritten
  assert!(
    content.contains("import { helper } from \"./utils.js\";"),
    "The import statement was not rewritten to include '.js'"
  );
}

#[test]
fn test_strip_log_flag() {
  // 1. Setup
  let fixture = Fixture::new("console-stripping");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()).arg("--strip-log"),
  );

  // 3. Assertion
  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(!content.contains("console.log"));
  assert!(content.contains("console.debug")); // Should NOT be stripped
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}

#[test]
fn test_strip_debug_flag() {
  // 1. Setup
  let fixture = Fixture::new("console-stripping");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--strip-debug"));

  // 3. Assertion
  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(content.contains("console.log")); // Should NOT be stripped
  assert!(!content.contains("console.debug"));
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}

#[test]
fn test_strip_both_flags() {
  // 1. Setup
  let fixture = Fixture::new("console-stripping");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--strip-log")
      .arg("--strip-debug"),
  );

  // 3. Assertion
  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(!content.contains("console.log"));
  assert!(!content.contains("console.debug"));
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}
