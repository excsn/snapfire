use std::path::Path;

use snapfire_lint::{Config, Level, Linter, Report};

fn lint(path: &str, source: &str) -> Report {
  Linter::new(Config::default()).unwrap().lint_source(Path::new(path), source)
}

fn found(report: &Report) -> Vec<(&str, usize, usize)> {
  report.findings.iter().map(|f| (f.rule.as_str(), f.line, f.column)).collect()
}

#[test]
fn each_markdown_rule_finds_what_eslint_markdown_does() {
  let cases = [
    ("# One\n\n### Three\n", "markdown/heading-increment", 3, 1),
    ("# One\n\n# Again\n", "markdown/no-multiple-h1", 3, 1),
    ("---\ntitle: One\n---\n\n# Two\n", "markdown/no-multiple-h1", 5, 1),
    ("# T\n\nSee [the docs]().\n", "markdown/no-empty-links", 3, 5),
    ("# T\n\nSee [the docs](#).\n", "markdown/no-empty-links", 3, 5),
    ("# T\n\n![a chart]()\n", "markdown/no-empty-images", 3, 1),
    ("# T\n\n![](chart.png)\n", "markdown/require-alt-text", 3, 1),
    ("# T\n\n```\nlet a = 1;\n```\n", "markdown/fenced-code-language", 3, 1),
    ("# T\n\n[a]: https://a.example\n[a]: https://b.example\n", "markdown/no-duplicate-definitions", 4, 1),
    ("# T\n\n[a]: <>\n", "markdown/no-empty-definitions", 3, 1),
    ("# Title\n\nSee [below](#missing).\n", "markdown/no-missing-link-fragments", 3, 5),
    ("# T\n\nSee (the docs)[https://a.example].\n", "markdown/no-reversed-media-syntax", 3, 5),
    ("# T\n\n#Heading\n", "markdown/no-missing-atx-heading-space", 3, 1),
  ];
  for (source, rule, line, column) in cases {
    let report = lint("doc.md", source);
    assert_eq!(found(&report), [(rule, line, column)], "{source}\n{}", report.render());
    assert_eq!(report.findings[0].level, Level::Error);
  }
}

#[test]
fn a_clean_document_has_no_findings() {
  let source = "---\ntitle: Guide\n---\n\n## Setting up\n\nSee [setting up](#setting-up) and [the site](https://example.com).\n\n![A chart of requests](chart.png)\n\n```ts\nconst a = 1;\n```\n\n[ref]: https://example.com\n";
  let report = lint("doc.md", source);
  assert!(report.findings.is_empty(), "{}", report.render());
}

#[test]
fn mdx_reads_its_jsx_and_imports_without_tripping() {
  let source = "import Chart from './chart';\n\n# Title\n\n<Chart data={[1, 2]} />\n\nSome {props.name} text.\n\n### Skipped\n";
  let report = lint("page.mdx", source);
  assert_eq!(found(&report), [("markdown/heading-increment", 9, 1)], "{}", report.render());
}

#[test]
fn a_comment_silences_a_markdown_finding() {
  let md = "# One\n\n<!-- eslint-disable-next-line markdown/heading-increment -->\n### Three\n";
  let report = lint("doc.md", md);
  assert!(report.findings.is_empty(), "{}", report.render());
  let mdx = "# One\n\n{/* fsr-lint-disable-next-line markdown/heading-increment */}\n### Three\n";
  let report = lint("page.mdx", mdx);
  assert!(report.findings.is_empty(), "{}", report.render());
}

#[test]
fn the_rules_are_listed_with_eslint_markdown_s_names() {
  let ids: Vec<_> = snapfire_lint::rules().into_iter().filter(|r| r.id.starts_with("markdown/")).map(|r| r.id).collect();
  assert_eq!(ids.len(), 11, "{ids:?}");
}
