use predicates::prelude::*;
use std::fs;

// Declare the common module
mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_basic_transpilation() {
  // 1. Setup: Let the Fixture handle creating and cleaning up the temp directory.
  let fixture = Fixture::new("basic-ts");

  // 2. Action: Run the command against the temporary fixture root.
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()),
  );

  // 3. Assertion: Check if the output file exists inside the temp directory.
  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  // Assert on essential parts, ignoring whitespace differences.
  let content = fs::read_to_string(output_file).unwrap();
  assert!(content.contains("export const greet")); // Check for the export
  assert!(content.contains("Hello, ${name}!")); // Check for the core logic
  assert!(!content.contains(": string"));
}

#[test]
fn test_root_flag_works_from_parent_dir() {
  // 1. Setup
  let fixture = Fixture::new("basic-ts");
  // Get the parent directory of our temporary fixture root
  let parent_dir = fixture.root().parent().unwrap();

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.current_dir(parent_dir).arg("--root").arg(fixture.root()));

  // 3. Assertion
  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();
  assert!(content.contains("export const greet"));
  assert!(content.contains("Hello, ${name}!"));
  assert!(!content.contains(": string"));
}

#[test]
fn test_out_dir_flag_overrides_tsconfig() {
  // 1. Setup
  let fixture = Fixture::new("out-dir-override");

  // 2. Action: Run with --out-dir flag pointing to "build"
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()).arg("--out-dir").arg("build"), // Override the "dist" from tsconfig.json
  );

  // 3. Assertion: Check for output in "build", not "dist"
  let correct_output_file = fixture.root().join("build/index.js");
  let incorrect_output_file = fixture.root().join("dist/index.js");

  assert!(predicate::path::exists().eval(&correct_output_file));
  assert!(!predicate::path::exists().eval(&incorrect_output_file));
}

#[test]
fn test_tsconfig_include_paths_are_respected() {
  // 1. Setup
  let fixture = Fixture::new("tsconfig-include");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  // 3. Assertion
  let included_output = fixture.root().join("lib/main.js");
  let ignored_output = fixture.root().join("lib/extra.js");

  // Check that the included file was compiled
  assert!(predicate::path::exists().eval(&included_output));

  // Check that the ignored file was NOT compiled
  assert!(!predicate::path::exists().eval(&ignored_output));
}
