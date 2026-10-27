use std::path::Path;

use snapfire_lint::{Config, Level, Linter, Report};

fn lint(source: &str) -> Report {
  Linter::new(Config::default()).unwrap().lint_source(Path::new("templates/page.tera"), source)
}

fn found(report: &Report) -> Vec<(&str, usize, usize, Level)> {
  report.findings.iter().map(|f| (f.rule.as_str(), f.line, f.column, f.level)).collect()
}

#[test]
fn a_template_tera_cannot_parse_is_a_parse_finding() {
  let report = lint("<p>\n{% if user %}\n<b>{{ user.name }}</b>\n");
  assert_eq!(report.findings.len(), 1, "{}", report.render());
  assert_eq!((report.findings[0].rule.as_str(), report.findings[0].level), ("lint/parse", Level::Error));
}

#[test]
fn what_only_the_host_s_registrations_decide_is_left_to_the_host() {
  let source = "{% extends \"base.tera\" %}\n{% block body %}{{ t(key=\"title\") | shout }}{% if x is fresh %}{% endif %}{% endblock %}\n";
  assert!(lint(source).findings.is_empty(), "{}", lint(source).render());
}

#[test]
fn each_tera_rule_finds_what_it_names() {
  assert_eq!(found(&lint("<p>{{ body | safe }}</p>\n")), [("tera/no-safe-filter", 1, 14, Level::Warn)]);
  assert_eq!(found(&lint("{% set greeting = \"hi\" %}\n<p>hello</p>\n")), [("tera/no-unused-set", 1, 8, Level::Warn)]);
  assert_eq!(found(&lint("<pre>{{ __tera_context }}</pre>\n")), [("tera/no-debug", 1, 9, Level::Error)]);
}

#[test]
fn a_set_that_is_read_is_used() {
  for source in ["{% set greeting = \"hi\" %}\n<p>{{ greeting }}</p>\n", "{% set n = 1 %}{% if n > 0 %}yes{% endif %}\n", "{% set items = [1, 2] %}{% for i in items %}{{ i }}{% endfor %}\n"] {
    assert!(lint(source).findings.is_empty(), "{source}: {}", lint(source).render());
  }
}

#[test]
fn a_tera_comment_silences_a_finding() {
  let report = lint("{# fsr-lint-disable-next-line tera/no-safe-filter -- rendered from our own markdown #}\n<div>{{ html | safe }}</div>\n");
  assert!(report.findings.is_empty(), "{}", report.render());
  assert_eq!(report.suppressed[0].reason.as_deref(), Some("rendered from our own markdown"));
}

#[test]
fn delimiters_inside_strings_are_text() {
  assert!(lint("<p>{{ \"| safe\" }}</p>\n").findings.is_empty());
}

/// tera 2.4.0's own parser inputs (`src/snapshot_tests/parser_inputs`, MIT, `tera_inputs/LICENSE`): each
/// one parses here exactly when it parses in tera.
#[test]
fn tera_s_own_inputs_parse_here_as_they_do_in_tera() {
  let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/tera_inputs");
  let linter = Linter::new(Config::default()).unwrap();
  for (dir, parses) in [("success", true), ("errors", false)] {
    let mut count = 0;
    for entry in std::fs::read_dir(root.join(dir)).unwrap() {
      let path = entry.unwrap().path();
      let report = linter.lint_source(&path, &std::fs::read_to_string(&path).unwrap());
      let parse = report.findings.iter().any(|f| f.rule == "lint/parse");
      assert_eq!(!parse, parses, "{}: {}", path.display(), report.render());
      count += 1;
    }
    assert!(count > 20, "{dir} holds tera's inputs");
  }
}
