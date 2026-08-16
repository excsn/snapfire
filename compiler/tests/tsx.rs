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
  
  // JSX is deliberately not lowered to React.createElement.
  assert!(content.contains("<div>Hello, {props.name}</div>"));
}