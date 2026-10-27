use std::path::Path;

use snapfire_lint::{Config, Level, Linter, Report};

fn lint(path: &str, source: &str) -> Report {
  Linter::new(Config::default()).unwrap().lint_source(Path::new(path), source)
}

#[test]
fn a_finding_names_the_rule_the_file_and_where() {
  let report = lint("src/page.ts", "export const a = 1;\n\n  debugger;\n");
  assert_eq!(report.files, 1);
  let finding = &report.findings[0];
  assert_eq!(finding.rule, "eslint/no-debugger");
  assert_eq!(finding.path, Path::new("src/page.ts"));
  assert_eq!((finding.line, finding.column), (3, 3));
  assert_eq!(finding.level, Level::Error);
  assert!(finding.message.contains("debugger"), "{}", finding.message);
}

#[test]
fn every_script_dialect_is_read() {
  let cases = [
    ("a.js", "debugger;\n"),
    ("a.mjs", "debugger;\n"),
    ("a.cjs", "debugger;\n"),
    ("a.jsx", "export const e = <div />;\ndebugger;\n"),
    ("a.ts", "export const n: number = 1;\ndebugger;\n"),
    ("a.mts", "debugger;\n"),
    ("a.tsx", "export const e: JSX.Element = <div />;\ndebugger;\n"),
  ];
  for (path, source) in cases {
    let rules: Vec<_> = lint(path, source).findings.into_iter().map(|f| f.rule).collect();
    assert_eq!(rules, ["eslint/no-debugger"], "{path}");
  }
}

#[test]
fn a_file_that_does_not_parse_is_a_finding() {
  let report = lint("a.ts", "let = ;\n");
  assert_eq!(report.findings.len(), 1, "{:?}", report.findings);
  assert_eq!(report.findings[0].rule, "lint/parse");
  assert_eq!(report.findings[0].level, Level::Error);
}

#[test]
fn rules_that_read_scopes_see_them() {
  let report = lint("a.ts", "const x = 1;\nx = 2;\nfunction unused() {}\n");
  let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
  assert!(rules.contains(&"eslint/no-const-assign"), "{rules:?}");
  assert!(rules.contains(&"eslint/no-unused-vars"), "{rules:?}");
}

#[test]
fn rules_that_read_control_flow_see_it() {
  let report = lint("a.ts", "function f() {\n  return 1;\n  f();\n}\nf();\n");
  let rules: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.line)).collect();
  assert!(rules.contains(&("eslint/no-unreachable", 3)), "{rules:?}");
}

#[test]
fn rules_that_read_regular_expressions_see_them() {
  let report = lint("a.ts", "export const r = /a  b/;\n");
  let rules: Vec<_> = report.findings.iter().map(|f| f.rule.as_str()).collect();
  assert_eq!(rules, ["eslint/no-regex-spaces"]);
}

#[test]
fn a_directory_is_walked_honouring_gitignore() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  std::fs::create_dir_all(root.join("app/routes")).unwrap();
  std::fs::create_dir_all(root.join("dist")).unwrap();
  std::fs::write(root.join(".gitignore"), "dist/\n").unwrap();
  std::fs::write(root.join("app/routes/page.tsx"), "debugger;\n").unwrap();
  std::fs::write(root.join("app/routes/notes.txt"), "debugger;\n").unwrap();
  std::fs::write(root.join("dist/page.js"), "debugger;\n").unwrap();
  let report = Linter::new(Config::default()).unwrap().lint_dir(root);
  assert_eq!(report.files, 1);
  let paths: Vec<_> = report.findings.iter().map(|f| f.path.clone()).collect();
  assert_eq!(paths, [root.join("app/routes/page.tsx")]);
}

#[test]
fn every_rule_has_a_namespaced_id_and_a_level() {
  let rules = snapfire_lint::rules();
  assert_eq!(rules.iter().filter(|r| r.id.starts_with("eslint/")).count(), 61, "ESLint's recommended rules");
  for rule in rules {
    assert!(rule.id.contains('/'), "{}", rule.id);
    assert!(!rule.docs.is_empty(), "{} has documentation", rule.id);
  }
}
