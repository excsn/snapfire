use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, run_snapfirec};

use crate::common::get_snapfirec_cmd;

#[test]
fn test_tsx_is_preserved() {
  // 1. Setup
  let fixture = Fixture::new("tsx-support");
  
  // 2. Action
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  // 3. Assertion
  let output_file = fixture.root().join("dist/component.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  // Verify TS types are stripped
  assert!(!content.contains(": string"));
  
  // Verify JSX is preserved (we are not compiling to React.createElement)
  assert!(content.contains("<div>Hello, {props.name}</div>"));
}