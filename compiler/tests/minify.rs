#[cfg(not(feature = "minify"))]
use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_no_min_files_without_the_flag() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/index.min.js")));
}

#[test]
fn test_minify_adds_files_rather_than_replacing_them() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let readable = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();
  let minified = fs::read_to_string(fixture.root().join("dist/index.min.js")).unwrap();

  assert!(readable.contains('\n'));
  assert!(readable.len() > minified.len());
}

#[test]
fn test_the_minified_graph_only_references_itself() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let dist = fixture.root().join("dist");
  let minified = fs::read_to_string(dist.join("index.min.js")).unwrap();

  for specifier in [
    "./state.min.js",
    "./widgets/index.min.js",
    "./already.min.js",
    "./typed.min.js",
    "./theme.min.css",
  ] {
    assert!(minified.contains(specifier), "{specifier} missing");
    assert!(
      predicate::path::exists().eval(&dist.join(specifier.trim_start_matches("./"))),
      "{specifier} is referenced but was never emitted"
    );
  }

  assert!(!minified.contains("\"./state.js\""));
  assert!(!minified.contains("\"./theme.css\""));
}

#[test]
fn test_bare_specifiers_survive_minification() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let minified = fs::read_to_string(fixture.root().join("dist/index.min.js")).unwrap();
  assert!(minified.contains("some-package"));
  assert!(!minified.contains("some-package.min"));
}

#[test]
fn test_dynamic_import_follows_the_minified_graph() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let minified = fs::read_to_string(fixture.root().join("dist/index.min.js")).unwrap();
  assert!(minified.contains(r#"import("./state.min.js")"#));
}

#[test]
fn test_each_variant_gets_its_own_map() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify").arg("--source-map"));

  let dist = fixture.root().join("dist");
  assert!(predicate::path::exists().eval(&dist.join("index.js.map")));
  assert!(predicate::path::exists().eval(&dist.join("index.min.js.map")));

  let minified = fs::read_to_string(dist.join("index.min.js")).unwrap();
  assert!(minified.contains("//# sourceMappingURL=index.min.js.map"));
}

#[test]
fn test_compact_keeps_identifiers() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let minified = fs::read_to_string(fixture.root().join("dist/button.min.js")).unwrap();
  assert!(minified.contains("button"));
  assert!(!minified.contains('\n') || minified.lines().count() <= 2);
}

#[cfg(not(feature = "minify"))]
#[test]
fn test_full_minify_is_refused_when_not_compiled_in() {
  let fixture = Fixture::new("computed-root");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .arg("--minify=full")
    .assert()
    .failure()
    .stderr(predicate::str::contains("needs a binary built with the 'minify' feature"));
}

#[cfg(feature = "minify")]
#[test]
fn test_full_minify_mangles_local_names() {
  let fixture = Fixture::new("minify-full");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify=full"));

  let compact = {
    let other = Fixture::new("minify-full");
    let mut cmd = get_snapfirec_cmd();
    run_snapfirec(cmd.arg("--root").arg(other.root()).arg("--minify"));
    fs::read_to_string(other.root().join("dist/index.min.js")).unwrap()
  };

  let full = fs::read_to_string(fixture.root().join("dist/index.min.js")).unwrap();

  assert!(compact.contains("aVeryLongLocalName"));
  assert!(!full.contains("aVeryLongLocalName"));
  assert!(full.len() < compact.len());
}
