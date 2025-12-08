use predicates::prelude::*;
use std::fs;

// Declare the common module
mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_css_is_minified() {
  // 1. Setup
  let fixture = Fixture::new("css-processing");

  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  // 3. Assertion
  let output_file = fixture.root().join("dist/styles.css");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  // The expected output after minification and nesting is flattened.
  let expected_content = "body{font-family:sans-serif}body .container{padding:20px;margin:0 auto}";

  // We need to normalize the output slightly because lightningcss might change property order.
  // For this simple case, a direct comparison is fine. For more complex CSS,
  // we might need a more sophisticated comparison.
  assert_eq!(content.trim(), expected_content);

  // Also verify that comments and newlines are gone.
  assert!(!content.contains("/*"));
  assert!(!content.contains('\n'));
}
