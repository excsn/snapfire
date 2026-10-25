use predicates::prelude::*;
use std::fs;

mod common;
use common::Fixture;

use crate::common::{get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_import_rewriter_appends_js_extension() {
  let fixture = Fixture::new("import-rewrite");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();

  assert!(
    content.contains("import { helper } from \"./utils.js\";"),
    "The import statement was not rewritten to include '.js'"
  );
}

#[test]
fn test_strip_log_flag() {
  let fixture = Fixture::new("console-stripping");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd.arg("--root").arg(fixture.root()).arg("--strip-log"),
  );

  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(!content.contains("console.log"));
  assert!(content.contains("console.debug"));
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}

#[test]
fn test_strip_debug_flag() {
  let fixture = Fixture::new("console-stripping");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--strip-debug"));

  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(content.contains("console.log"));
  assert!(!content.contains("console.debug"));
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}

#[test]
fn test_strip_both_flags() {
  let fixture = Fixture::new("console-stripping");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--strip-log")
      .arg("--strip-debug"),
  );

  let output_file = fixture.root().join("dist/main.js");
  let content = fs::read_to_string(output_file).unwrap();

  assert!(!content.contains("console.log"));
  assert!(!content.contains("console.debug"));
  assert!(content.contains("console.warn"));
  assert!(content.contains("console.error"));
}

#[test]
fn a_formatter_written_without_a_locale_takes_the_page_locale_when_one_is_named() {
  let fixture = Fixture::new("page-locale");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--locale-from").arg("@snapfire/fsr/client/intl"));
  let content = fs::read_to_string(fixture.root().join("dist/main.js")).unwrap();
  for written in [
    "import { localeTag as __sf_localeTag } from \"@snapfire/fsr/client/intl\";",
    "new Intl.NumberFormat(__sf_localeTag())",
    "new Intl.DateTimeFormat(__sf_localeTag(), {",
    "Intl.Collator(__sf_localeTag())",
    ".toLocaleString(__sf_localeTag())",
    ".toLocaleDateString(__sf_localeTag(), {",
    "\"a\".localeCompare(\"b\", __sf_localeTag())",
    "new Intl.NumberFormat(\"de-DE\")",
    ".toLocaleString(\"fr-FR\")",
    "\"a\".localeCompare(\"b\", \"sv\")",
    ".NumberFormat();",
  ] {
    assert!(content.contains(written), "{written} in:\n{content}");
  }

  let plain = Fixture::new("page-locale");
  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(plain.root()));
  let content = fs::read_to_string(plain.root().join("dist/main.js")).unwrap();
  assert!(!content.contains("localeTag") && content.contains("new Intl.NumberFormat().format(1234.5)"), "{content}");
}
