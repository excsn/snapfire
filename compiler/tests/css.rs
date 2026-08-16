use predicates::prelude::*;
use std::fs;

mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_nesting_is_flattened() {
  let fixture = Fixture::new("css-processing");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let output_file = fixture.root().join("dist/styles.css");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  assert!(content.contains("body .container"));
  assert!(!content.contains("/*"));
}

#[test]
fn test_plain_css_output_is_readable() {
  let fixture = Fixture::new("css-processing");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/styles.css")).unwrap();

  assert!(content.contains('\n'));
  assert!(content.contains("  padding: 20px;"));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/styles.min.css")));
}

#[test]
fn test_minified_css_is_an_additional_file() {
  let fixture = Fixture::new("css-processing");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let readable = fs::read_to_string(fixture.root().join("dist/styles.css")).unwrap();
  let minified = fs::read_to_string(fixture.root().join("dist/styles.min.css")).unwrap();

  assert!(readable.contains('\n'));
  // Minification normalises declaration order, so `margin` overtakes `padding` here.
  assert_eq!(
    minified.trim(),
    "body{font-family:sans-serif}body .container{margin:0 auto;padding:20px}"
  );
}
