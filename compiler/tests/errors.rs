use assert_cmd::prelude::*;
use predicates::prelude::*;

mod common;
use common::Fixture;

use crate::common::get_snapfirec_cmd;
#[test]
fn test_syntax_error_fails_build() {
  let fixture = Fixture::new("invalid-syntax");
  
  let mut cmd = get_snapfirec_cmd();
  let assert = cmd
    .arg("--root")
    .arg(fixture.root())
    .assert();

  assert
    .failure()
    .stderr(predicate::str::contains("Error compiling"))
    .stderr(predicate::str::contains("bad.ts:2:29: Expression expected"));
}