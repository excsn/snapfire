use std::path::Path;

use snapfire_lint::{Config, Linter, Report};

fn lint(config: &str, source: &str) -> Report {
  let config = Config::parse(config, Path::new("fsr.lint.toml")).unwrap();
  Linter::new(config).unwrap().lint_source(Path::new("a.ts"), source)
}

#[test]
fn an_error_fails_the_run() {
  let report = lint("", "debugger;\n");
  assert_eq!((report.errors(), report.warnings()), (1, 0));
  assert!(report.failed(false));
}

#[test]
fn a_warning_fails_the_run_only_when_warnings_are_denied() {
  let report = lint("[rules]\n\"eslint/no-debugger\" = \"warn\"\n", "debugger;\n");
  assert_eq!((report.errors(), report.warnings()), (0, 1));
  assert!(!report.failed(false));
  assert!(report.failed(true));
}

#[test]
fn a_clean_run_passes() {
  let report = lint("", "export const a = 1;\n");
  assert!(report.findings.is_empty(), "{:?}", report.findings);
  assert!(!report.failed(true));
}

#[test]
fn the_terminal_form_names_level_rule_and_place_then_the_totals() {
  let report = lint("[rules]\n\"eslint/no-debugger\" = \"warn\"\n", "debugger;\n");
  let text = report.render();
  let first = text.lines().next().unwrap();
  assert!(first.starts_with("warning"), "{text}");
  assert!(first.contains("eslint/no-debugger"), "{text}");
  assert!(first.contains("a.ts:1:1"), "{text}");
  assert!(text.lines().last().unwrap().contains("0 errors, 1 warning in 1 file"), "{text}");
}

#[test]
fn the_json_form_carries_every_field() {
  let report = lint("", "debugger;\n");
  let json = report.to_json();
  let finding = &json["findings"][0];
  assert_eq!(finding["rule"], "eslint/no-debugger");
  assert_eq!(finding["level"], "error");
  assert_eq!(finding["path"], "a.ts");
  assert_eq!(finding["line"], 1);
  assert_eq!(finding["column"], 1);
  assert!(finding["message"].is_string());
  assert_eq!(json["errors"], 1);
  assert_eq!(json["warnings"], 0);
  assert_eq!(json["files"], 1);
}

#[test]
fn the_sarif_form_is_one_run_with_the_rules_and_a_result_per_finding() {
  let report = lint("[rules]\n\"eslint/no-debugger\" = \"warn\"\n", "export const a = 1;\ndebugger;\n");
  let sarif = report.to_sarif();
  assert_eq!(sarif["version"], "2.1.0");
  let run = &sarif["runs"][0];
  assert_eq!(run["tool"]["driver"]["name"], "fsr lint");
  assert!(run["tool"]["driver"]["rules"].as_array().unwrap().iter().any(|r| r["id"] == "eslint/no-debugger"));
  let result = &run["results"][0];
  assert_eq!(result["ruleId"], "eslint/no-debugger");
  assert_eq!(result["level"], "warning");
  assert!(result["message"]["text"].is_string());
  let region = &result["locations"][0]["physicalLocation"];
  assert_eq!(region["artifactLocation"]["uri"], "a.ts");
  assert_eq!(region["region"]["startLine"], 2);
  assert_eq!(region["region"]["startColumn"], 1);
}
