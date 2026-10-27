use std::path::Path;

use snapfire_lint::oxc_ast::AstKind;
use snapfire_lint::{Check, Config, File, Level, Linter, RuleInfo};

/// Reports every reference to `foo`.
struct NoFoo;

impl Check for NoFoo {
  fn info(&self) -> RuleInfo {
    RuleInfo::new("x/no-foo", Level::Warn, "correctness", "### What it does\n\nDisallows `foo`.\n")
  }

  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String)) {
    for node in file.semantic.nodes().iter() {
      if let AstKind::IdentifierReference(ident) = node.kind()
        && ident.name == "foo"
      {
        found(ident.span.start as usize, "`foo` is not allowed".to_owned());
      }
    }
  }
}

fn linter(config: &str) -> Linter {
  let config = Config::parse(config, Path::new("fsr.lint.toml")).unwrap();
  Linter::new_with(config, vec![Box::new(NoFoo)]).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_check_from_outside_reports_like_a_rule() {
  let report = linter("").lint_source(Path::new("a.ts"), "export const a = 1;\nfoo();\n");
  let found: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.line, f.column, f.level)).collect();
  assert_eq!(found, [("x/no-foo", 2, 1, Level::Warn)]);
}

#[test]
fn a_check_s_level_is_set_like_a_rule_s() {
  assert!(linter("[rules]\n\"x/no-foo\" = \"off\"\n").lint_source(Path::new("a.ts"), "foo();\n").findings.is_empty());
  let report = linter("[rules]\n\"x/no-foo\" = \"error\"\n").lint_source(Path::new("a.ts"), "foo();\n");
  assert_eq!(report.findings[0].level, Level::Error);
}

#[test]
fn a_comment_silences_a_check() {
  let report = linter("").lint_source(Path::new("a.ts"), "// fsr-lint-disable-next-line x/no-foo\nfoo();\n");
  assert!(report.findings.is_empty(), "{}", report.render());
}

#[test]
fn a_check_runs_in_each_script_block_of_a_component() {
  let report = linter("").lint_source(Path::new("A.vue"), "<template><p /></template>\n<script setup>\nfoo();\n</script>\n");
  let found: Vec<_> = report.findings.iter().map(|f| (f.rule.as_str(), f.line, f.column)).collect();
  assert_eq!(found, [("x/no-foo", 3, 1)]);
}

#[test]
fn the_linter_lists_its_checks_beside_the_rules() {
  let ids: Vec<_> = linter("").rules().into_iter().map(|r| r.id).collect();
  assert!(ids.contains(&"x/no-foo"));
  assert!(ids.contains(&"eslint/no-debugger"));
}
