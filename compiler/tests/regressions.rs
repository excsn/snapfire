use assert_cmd::prelude::*;
use predicates::prelude::*;
use std::fs;

mod common;
use common::{Fixture, get_snapfirec_cmd, run_snapfirec};

#[test]
fn test_stripping_only_matches_the_console_object() {
  let fixture = Fixture::new("strip-console-precision");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--strip-log")
      .arg("--strip-debug"),
  );

  let content = fs::read_to_string(fixture.root().join("dist/main.js")).unwrap();

  assert!(content.contains("logger.log('method survives')"));
  assert!(content.contains("logger.debug('method survives')"));
  assert!(content.contains("console.warn('warn survives')"));
  assert!(content.contains("const captured = console.log('value survives')"));
}

#[test]
fn test_top_level_console_calls_are_stripped() {
  let fixture = Fixture::new("strip-console-precision");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .arg("--root")
      .arg(fixture.root())
      .arg("--strip-log")
      .arg("--strip-debug"),
  );

  let content = fs::read_to_string(fixture.root().join("dist/main.js")).unwrap();

  assert!(!content.contains("top level log"));
  assert!(!content.contains("top level debug"));
  assert!(!content.contains("nested log"));
  assert!(!content.contains("nested debug"));
}

#[test]
fn test_import_specifiers_resolve_to_loadable_paths() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();

  assert!(content.contains(r#"from "./state.js""#));
  assert!(content.contains(r#"from "./widgets/index.js""#));
  assert!(content.contains(r#"from "./typed.js""#));
  assert!(content.contains("from './already.js'"));
  assert!(content.contains("from 'some-package'"));
  assert!(content.contains("import './theme.css'"));
  assert!(content.contains(r#"export * from "./state.js""#));

  assert!(!content.contains("./theme.css.js"));
  assert!(!content.contains("./typed.ts.js"));
  assert!(!content.contains("./already.js.js"));
  assert!(!content.contains(r#""./widgets.js""#));
}

#[test]
fn test_dynamic_import_is_rewritten() {
  let fixture = Fixture::new("import-resolution");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();

  assert!(content.contains(r#"import("./state.js")"#));
}

#[test]
fn test_jsonc_tsconfig_is_accepted() {
  let fixture = Fixture::new("jsonc-config");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let output_file = fixture.root().join("dist/index.js");
  assert!(predicate::path::exists().eval(&output_file));

  let content = fs::read_to_string(output_file).unwrap();
  assert!(content.contains("export const greet"));
}

#[test]
fn test_declaration_files_are_skipped() {
  let fixture = Fixture::new("declaration-files");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/types.d.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/types.js")));
}

#[test]
fn test_build_with_no_inputs_fails() {
  let fixture = Fixture::new("missing-include");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains(r#"'include' pattern "srcc" matched no files"#))
    .stderr(predicate::str::contains("No inputs were found"));
}

#[test]
fn test_absent_include_entry_is_skipped_when_another_matches() {
  let fixture = Fixture::new("optional-include");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()))
    .stderr(predicate::str::contains(r#"'include' pattern "generated" matched no files"#));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
}

#[test]
fn test_output_collision_fails_build() {
  let fixture = Fixture::new("output-collision");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("Output collision"));
}

#[test]
fn test_javascript_sources_are_compiled() {
  let fixture = Fixture::new("js-sources");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()).arg("--strip-log"));

  let plain = fs::read_to_string(fixture.root().join("dist/plain.js")).unwrap();
  assert!(plain.contains(r#"from "./helper.js""#));
  assert!(!plain.contains("plain js log"));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/helper.js")));
  assert!(predicate::path::exists().eval(&fixture.root().join("dist/module.mjs")));
}

#[test]
fn test_browserslist_config_is_inherited_from_a_parent_directory() {
  let fixture = Fixture::new("browserslist-inherited");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root().join("pkg")))
    .stdout(predicate::str::contains("Browser Targets: 'chrome 100'"));
}

#[test]
fn test_unresolvable_browser_targets_warn_without_failing() {
  let fixture = Fixture::new("basic-ts");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(
    cmd
      .env("BROWSERSLIST", "op_mini all")
      .arg("--root")
      .arg(fixture.root()),
  )
  .stderr(predicate::str::contains("No browser targets resolved"));
}

#[test]
fn test_recovered_parse_errors_fail_the_build() {
  let fixture = Fixture::new("recovered-syntax");

  // An accessibility modifier on a private name is recovered by the parser, so `parse_module`
  // returns a module and the diagnostic is only reachable through `take_errors`.
  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "loose.ts:2:3: An accessibility modifier cannot be used with a private identifier.",
    ));

  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/loose.js")));
}

#[test]
fn test_output_directory_failure_does_not_abort_the_build() {
  let fixture = Fixture::new("mkdir-conflict");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("Error creating output directory"));

  assert!(predicate::path::exists().eval(&fixture.root().join("outdir/ok.js")));
}

#[test]
fn test_non_asset_files_leave_no_empty_directories() {
  let fixture = Fixture::new("non-asset-files");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  assert!(predicate::path::exists().eval(&fixture.root().join("dist/index.js")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/docs")));
  assert!(!predicate::path::exists().eval(&fixture.root().join("dist/assets")));
}

#[test]
fn test_each_file_reports_its_own_source_positions() {
  let fixture = Fixture::new("multi-error");

  let mut cmd = get_snapfirec_cmd();
  cmd
    .arg("--root")
    .arg(fixture.root())
    .assert()
    .failure()
    .stderr(predicate::str::contains("first.ts:2:16: Expression expected"))
    .stderr(predicate::str::contains("second.ts:4:20: Expression expected"));
}

#[test]
fn test_namespace_and_enum_emit_valid_javascript() {
  let fixture = Fixture::new("ts-enum");

  let mut cmd = get_snapfirec_cmd();
  run_snapfirec(cmd.arg("--root").arg(fixture.root()));

  let content = fs::read_to_string(fixture.root().join("dist/index.js")).unwrap();

  assert!(content.contains("export var Color"));
  assert!(content.contains("(function(Shapes) {"));
  assert!(content.contains("})(Shapes || (Shapes = {}))"));
  assert!(content.contains("this.x = x"));
}
