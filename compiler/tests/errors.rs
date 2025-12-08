use assert_cmd::prelude::*;
use predicates::prelude::*;

mod common;
use common::Fixture;

use crate::common::get_snapfirec_cmd;
#[test]
fn test_syntax_error_fails_build() {
  // 1. Setup
  let fixture = Fixture::new("invalid-syntax");
  
  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  let assert = cmd
    .arg("--root")
    .arg(fixture.root())
    .assert();

  // 3. Assertion
  assert
    .failure() // Expect non-zero exit code
    .stderr(predicate::str::contains("Parser Error")); // Expect error log
}