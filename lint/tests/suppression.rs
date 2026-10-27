use std::path::Path;

use snapfire_lint::{Config, Linter, Report};

fn lint(source: &str) -> Report {
  Linter::new(Config::default()).unwrap().lint_source(Path::new("a.ts"), source)
}

fn rules(report: &Report) -> Vec<(&str, usize)> {
  report.findings.iter().map(|f| (f.rule.as_str(), f.line)).collect()
}

#[test]
fn a_next_line_comment_silences_the_next_line_only() {
  for comment in ["// fsr-lint-disable-next-line eslint/no-debugger", "// eslint-disable-next-line no-debugger", "/* eslint-disable-next-line no-debugger */"] {
    let report = lint(&format!("{comment}\ndebugger;\ndebugger;\n"));
    assert_eq!(rules(&report), [("eslint/no-debugger", 3)], "{comment}");
    assert_eq!(report.suppressed.len(), 1, "{comment}");
  }
}

#[test]
fn a_same_line_comment_silences_its_own_line() {
  for comment in ["// fsr-lint-disable-line eslint/no-debugger", "// eslint-disable-line no-debugger"] {
    let report = lint(&format!("debugger; {comment}\ndebugger;\n"));
    assert_eq!(rules(&report), [("eslint/no-debugger", 2)], "{comment}");
  }
}

#[test]
fn a_comment_naming_no_rule_silences_every_rule() {
  let report = lint("// eslint-disable-next-line\ndebugger; if (x) {}\n");
  assert!(report.findings.is_empty(), "{:?}", report.findings);
  assert_eq!(report.suppressed.len(), 2);
}

#[test]
fn a_comment_silences_only_the_rules_it_names() {
  let report = lint("// fsr-lint-disable-next-line eslint/no-empty\ndebugger; if (x) {}\n");
  assert_eq!(rules(&report), [("eslint/no-debugger", 2)]);
}

#[test]
fn a_comment_names_several_rules_with_commas() {
  let report = lint("// eslint-disable-next-line no-debugger, no-empty\ndebugger; if (x) {}\n");
  assert!(report.findings.is_empty(), "{:?}", report.findings);
}

#[test]
fn disable_and_enable_bound_a_range() {
  for (disable, enable) in [("/* fsr-lint-disable eslint/no-debugger */", "/* fsr-lint-enable eslint/no-debugger */"), ("/* eslint-disable no-debugger */", "/* eslint-enable no-debugger */")] {
    let report = lint(&format!("debugger;\n{disable}\ndebugger;\ndebugger;\n{enable}\ndebugger;\n"));
    assert_eq!(rules(&report), [("eslint/no-debugger", 1), ("eslint/no-debugger", 6)], "{disable}");
  }
}

#[test]
fn a_disable_with_no_enable_runs_to_the_end_of_the_file() {
  let report = lint("debugger;\n/* eslint-disable */\ndebugger;\ndebugger;\n");
  assert_eq!(rules(&report), [("eslint/no-debugger", 1)]);
}

#[test]
fn the_reason_after_two_dashes_is_kept_with_what_it_silenced() {
  let report = lint("// fsr-lint-disable-next-line eslint/no-debugger -- kept for the profiler\ndebugger;\n");
  assert!(report.findings.is_empty());
  assert_eq!(report.suppressed[0].reason.as_deref(), Some("kept for the profiler"));
  assert_eq!(report.suppressed[0].finding.rule, "eslint/no-debugger");
}

#[test]
fn a_comment_that_silences_nothing_is_reported() {
  let report = lint("// eslint-disable-next-line no-debugger\nexport const x = 1;\n");
  assert_eq!(rules(&report), [("lint/unused-disable", 1)]);
}

#[test]
fn a_comment_in_a_string_is_not_a_directive() {
  let report = lint("export const s = \"// eslint-disable-next-line\";\n\ndebugger;\n");
  assert_eq!(rules(&report), [("eslint/no-debugger", 3)]);
}
