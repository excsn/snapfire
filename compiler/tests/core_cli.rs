use predicates::prelude::*;
use std::fs;

mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_basic_transpilation() {
  let fixture = Fixture::new("basic-ts");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()),
  );

  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();
  assert!(content.contains("export const greet"));
  assert!(content.contains("Hello, ${name}!"));
  assert!(!content.contains(": string"));
}

#[test]
fn test_root_flag_works_from_parent_dir() {
  let fixture = Fixture::new("basic-ts");
  let parent_dir = fixture.root().parent().unwrap();

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.current_dir(parent_dir).arg("--root").arg(fixture.root()));

  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();
  assert!(content.contains("export const greet"));
  assert!(content.contains("Hello, ${name}!"));
  assert!(!content.contains(": string"));
}

#[test]
fn test_out_dir_flag_overrides_tsconfig() {
  let fixture = Fixture::new("out-dir-override");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()).arg("--out-dir").arg("build"),
  );

  let correct_output_file = fixture.root().join("build/index.js");
  let incorrect_output_file = fixture.root().join("dist/index.js");

  assert!(predicate::path::exists().eval(&correct_output_file));
  assert!(!predicate::path::exists().eval(&incorrect_output_file));
}

#[test]
fn test_tsconfig_include_paths_are_respected() {
  let fixture = Fixture::new("tsconfig-include");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let included_output = fixture.root().join("lib/main.js");
  let ignored_output = fixture.root().join("lib/extra.js");

  assert!(predicate::path::exists().eval(&included_output));

  assert!(!predicate::path::exists().eval(&ignored_output));
}
