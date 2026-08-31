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

#[test]
fn test_the_minified_stylesheet_imports_the_minified_graph() {
  let fixture = Fixture::new("css-import-graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let dist = fixture.root().join("dist");
  let minified = fs::read_to_string(dist.join("kit.min.css")).unwrap();

  for specifier in ["./base.min.css", "./layout.min.css"] {
    assert!(minified.contains(specifier), "{specifier} missing");
    assert!(
      predicate::path::exists().eval(&dist.join(specifier.trim_start_matches("./"))),
      "{specifier} is referenced but was never emitted"
    );
  }

  assert!(!minified.contains("\"./base.css\""));
  assert!(!minified.contains("\"./layout.css\""));
}

#[test]
fn test_the_readable_stylesheet_keeps_the_readable_graph() {
  let fixture = Fixture::new("css-import-graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let readable = fs::read_to_string(fixture.root().join("dist/kit.css")).unwrap();

  assert!(readable.contains("./base.css"));
  assert!(readable.contains("./layout.css"));
  assert!(!readable.contains(".min.css"));
}

#[test]
fn test_an_absolute_import_is_left_alone_in_both_graphs() {
  let fixture = Fixture::new("css-import-graph");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--minify"));

  let dist = fixture.root().join("dist");

  for name in ["kit.css", "kit.min.css"] {
    let content = fs::read_to_string(dist.join(name)).unwrap();
    assert!(
      content.contains("https://fonts.example.com/inter.css"),
      "{name} lost the absolute import"
    );
  }
}
