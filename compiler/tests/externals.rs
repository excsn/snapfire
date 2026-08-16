use predicates::prelude::*;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_bare_specifiers_are_reported() {
  let fixture = Fixture::new("externals");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()))
    .stdout(predicate::str::contains(
      "Externals: '@scope/pkg', 'lit', 'lodash/debounce'",
    ))
    .stdout(predicate::str::contains("need an import map"));
}

#[test]
fn test_natively_resolvable_specifiers_are_not_externals() {
  let fixture = Fixture::new("externals");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()))
    .stdout(predicate::str::contains("cdn.example.com").not())
    .stdout(predicate::str::contains("/assets/vendor.js").not());
}

#[test]
fn test_an_external_used_twice_is_listed_once() {
  let fixture = Fixture::new("externals");

  let mut cmd = get_snapfirec_cmd();
  let assert = run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let line = stdout.lines().find(|l| l.contains("Externals:")).unwrap();

  assert_eq!(line.matches("'lit'").count(), 1);
}

#[test]
fn test_the_minified_graph_does_not_duplicate_externals() {
  let fixture = Fixture::new("externals");

  let mut cmd = get_snapfirec_cmd();
  let assert = run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  let line = stdout.lines().find(|l| l.contains("Externals:")).unwrap();

  assert_eq!(line.matches("'lit'").count(), 1);
}

#[test]
fn test_nothing_is_reported_when_there_are_no_externals() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root())).stdout(predicate::str::contains("Externals:").not());
}
