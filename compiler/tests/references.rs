use assert_cmd::prelude::*;
use predicates::prelude::*;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_a_dangling_static_import_fails_the_build() {
  let fixture = Fixture::new("dangling-import");

  let mut cmd = get_snapfirec_cmd();

  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("imports './gone.js', which resolves to nothing"));
}

#[test]
fn test_a_dangling_dynamic_import_fails_too() {
  let fixture = Fixture::new("dangling-import");

  let mut cmd = get_snapfirec_cmd();

  // Deferring the fetch does not make the target optional; it only moves the
  // 404 to whenever the branch runs.
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("imports './absent.js', which resolves to nothing"));
}

#[test]
fn test_a_stylesheet_the_build_emits_is_not_dangling() {
  let fixture = Fixture::new("dangling-import");

  let mut cmd = get_snapfirec_cmd();

  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("theme.css").not());
}

#[test]
fn test_every_resolvable_form_passes() {
  let fixture = Fixture::new("import-resolution");

  // Directory indexes, extensionless specifiers, already-suffixed ones and a
  // stylesheet all resolve, so none of them may be reported.
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));
}

#[test]
fn test_the_minified_graph_is_checked_against_itself() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));
}
