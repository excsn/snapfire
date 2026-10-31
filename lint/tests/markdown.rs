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
    let of_rule: Vec<_> = found(&report).into_iter().filter(|(r, _, _)| *r == rule).collect();
    assert_eq!(of_rule, [(rule, line, column)], "{source}\n{}", report.render());
    assert!(report.findings.iter().filter(|f| f.rule == rule).all(|f| f.level == Level::Error));
  }
}

#[test]
fn a_clean_document_has_no_findings() {
  let source = "---\ntitle: Guide\n---\n\n## Setting up\n\nSee [setting up](#setting-up) and [the site](https://example.com).\n\n![A chart of requests](chart.png)\n\n```ts\nconst a = 1;\n```\n\n[ref]: https://example.com\n\nThe [reference][ref].\n";
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
  assert_eq!(ids.len(), 21, "{ids:?}");
}

#[test]
fn frontmatter_title_names_the_key_that_counts_as_the_first_heading() {
  let config = |toml: &str| Linter::new(Config::parse(toml, Path::new("fsr.lint.toml")).unwrap()).unwrap();
  let page = "---\ntitle: FAQ\n---\n\n# Questions\n\n### Deep\n";
  let off = config("[rules]\n\"markdown/no-multiple-h1\" = [\"error\", { frontmatterTitle = \"\" }]\n\"markdown/heading-increment\" = [\"error\", { frontmatterTitle = \"\" }]\n");
  assert_eq!(found(&off.lint_source(Path::new("page.md"), page)), [("markdown/heading-increment", 7, 1)], "the title is not a heading; `# Questions` is the first");
  let named = config("[rules]\n\"markdown/no-multiple-h1\" = [\"error\", { frontmatterTitle = \"^heading:\" }]\n");
  assert!(found(&named.lint_source(Path::new("page.md"), page)).iter().all(|(rule, _, _)| *rule != "markdown/no-multiple-h1"), "only a `heading:` key counts");
  assert_eq!(found(&named.lint_source(Path::new("page.md"), "---\nheading: FAQ\n---\n\n# Questions\n")), [("markdown/no-multiple-h1", 5, 1)]);
}

fn all_on(path: &str, source: &str, extra: &str) -> Vec<(String, usize, usize)> {
  let on = ["markdown/fenced-code-meta", "markdown/no-bare-urls", "markdown/no-duplicate-headings", "markdown/no-html", "markdown/table-column-count"];
  let base: String = on.iter().filter(|id| !extra.contains(&format!("\"{id}\""))).map(|id| format!("\"{id}\" = \"error\"\n")).collect();
  let toml = format!("[rules]\n{base}{extra}");
  let linter = Linter::new(Config::parse(&toml, Path::new("fsr.lint.toml")).unwrap()).unwrap();
  linter.lint_source(Path::new(path), source).findings.into_iter().map(|f| (f.rule, f.line, f.column)).collect()
}

fn of(found: &[(String, usize, usize)], rule: &str) -> Vec<(usize, usize)> {
  found.iter().filter(|(r, _, _)| r == rule).map(|(_, l, c)| (*l, *c)).collect()
}

#[test]
fn the_rest_of_eslint_markdowns_rules_find_its_cases() {
  let cases: [(&str, &str, &[(usize, usize)]); 20] = [
    ("markdown/no-invalid-label-refs", "[foo][ ]\n\n[foo]: http://bar.com/image.jpg\n", &[(1, 6)]),
    ("markdown/no-invalid-label-refs", "[eslint][\n]\n", &[(1, 9)]),
    ("markdown/no-missing-label-refs", "[foo][bar]\n", &[(1, 7)]),
    ("markdown/no-missing-label-refs", "[foo][]\n", &[(1, 2)]),
    ("markdown/no-missing-label-refs", "foo][bar][baz]\n\n[bar]: http://bar.com\n", &[(1, 11)]),
    ("markdown/no-reference-like-urls", "[Mercury](mercury)\n\n[mercury]: https://example.com/mercury\n", &[(1, 1)]),
    ("markdown/no-space-in-emphasis", "Broken * emphasis * with spaces\n", &[(1, 8), (1, 18)]),
    ("markdown/no-space-in-emphasis", "** Strong** with left space\n", &[(1, 1)]),
    ("markdown/no-unused-definitions", "\n[mercury]: https://example.com/mercury/\n", &[(2, 1)]),
    ("markdown/no-unused-definitions", "\n[^Mercury]: Hello, Mercury!\n", &[(2, 1)]),
    ("markdown/fenced-code-meta", "```javascript\nlet a = 1;\n```\n", &[(1, 1)]),
    ("markdown/no-bare-urls", "text https://www.google.com/\n", &[(1, 6)]),
    ("markdown/no-bare-urls", "Text <a>https://example.com</a> https://example.com\n", &[(1, 33)]),
    ("markdown/no-duplicate-headings", "\n# Heading 1\n\n# Heading 1\n", &[(4, 1)]),
    ("markdown/no-duplicate-headings", "\n# Heading 1\n\n## Heading 1\n", &[(4, 1)]),
    ("markdown/no-html", "<img />\n", &[(1, 1)]),
    ("markdown/no-html", "<!-- hi --><b>Hello world!</b>\n", &[(1, 12)]),
    ("markdown/table-column-count", "| Head1 | Head2 |\n| ----- | ----- |\n| R1C1  | R1C2  | R2C3  |\n", &[(3, 17)]),
    ("markdown/table-column-count", "| A |\n| - |\n| 1 | 2 |\n", &[(3, 5)]),
    ("markdown/no-html", "<b>Hello world!</b><i>Goodbye world!</i>\n", &[(1, 1), (1, 20)]),
  ];
  let wrong: Vec<String> = cases.iter().filter_map(|(rule, source, at)| {
    let found = all_on("a.md", source, "");
    (of(&found, rule) != *at).then(|| format!("{rule} on {source:?}: {:?}, all {found:?}", of(&found, rule)))
  }).collect();
  assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn the_rest_of_eslint_markdowns_rules_pass_its_valid_cases() {
  let cases: [(&str, &str); 15] = [
    ("markdown/no-missing-label-refs", "> [!WARNING]\n> Read this first.\n"),
    ("markdown/no-invalid-label-refs", "[foo][ foo ]\n\n[foo]: http://bar.com\n"),
    ("markdown/no-invalid-label-refs", "[foo][]\n\n[foo]: http://bar.com/image.jpg\n"),
    ("markdown/no-missing-label-refs", "[][]\n"),
    ("markdown/no-missing-label-refs", "\\[escaped\\]\n"),
    ("markdown/no-reference-like-urls", "`[Mercury](mercury)`\n\n[mercury]: https://example.com/mercury\n"),
    ("markdown/no-reference-like-urls", "![image](<uri>)\n\n[uri]: https://example.com\n"),
    ("markdown/no-space-in-emphasis", "Line with ***Normal strong and emphasis***\n"),
    ("markdown/no-space-in-emphasis", "Emphasis `inside * code * blocks` is okay.\n"),
    ("markdown/no-unused-definitions", "\n[//]: # (This is a comment 1)\n[//]: <> (This is a comment 2)\n"),
    ("markdown/fenced-code-meta", "```js foo\nlet a = 1;\n```\n"),
    ("markdown/no-bare-urls", "<https://www.google.com/>\n"),
    ("markdown/no-duplicate-headings", "# Heading 1\n\n# Heading *1*\n"),
    ("markdown/no-html", "```html\n<b>Hello world!</b>\n```\n"),
    ("markdown/table-column-count", "| Header | Header | Header |\n| ------ | ------ | ------ |\n| Cell   | Cell   |\n"),
  ];
  let wrong: Vec<String> = cases.iter().filter_map(|(rule, source)| {
    let found = all_on("a.md", source, "");
    (!of(&found, rule).is_empty()).then(|| format!("{rule} on {source:?}: {:?}", of(&found, rule)))
  }).collect();
  assert!(wrong.is_empty(), "{wrong:#?}");
}

#[test]
fn their_options_change_what_they_find() {
  let found = all_on("a.md", "[foo][bar]\n", "\"markdown/no-missing-label-refs\" = [\"error\", { allowLabels = [\"bar\"] }]\n");
  assert!(of(&found, "markdown/no-missing-label-refs").is_empty());
  let found = all_on("a.md", "Text ~ strikethrough ~ with spaces\n", "\"markdown/no-space-in-emphasis\" = [\"error\", { checkStrikethrough = true }]\n");
  assert_eq!(of(&found, "markdown/no-space-in-emphasis").len(), 2);
  let found = all_on("a.md", "```js title=\"example.js\"\nlet a = 1;\n```\n", "\"markdown/fenced-code-meta\" = [\"error\", \"never\"]\n");
  assert_eq!(of(&found, "markdown/fenced-code-meta"), [(1, 7)]);
  let found = all_on("a.md", "<DiV>Hello world!</DiV>\n", "\"markdown/no-html\" = [\"error\", { allowed = [\"div\"], allowedIgnoreCase = true }]\n");
  assert!(of(&found, "markdown/no-html").is_empty());
  let found = all_on("a.md", "| Header | Header | Header |\n| ------ | ------ | ------ |\n| Cell   | Cell   |\n", "\"markdown/table-column-count\" = [\"error\", { checkMissingCells = true }]\n");
  assert_eq!(of(&found, "markdown/table-column-count"), [(3, 19)]);
  let changelog = "# Change log\n\n## 1.0.0\n\n### Features\n\n## 2.0.0\n\n### Features\n";
  assert!(of(&all_on("a.md", changelog, "\"markdown/no-duplicate-headings\" = [\"error\", { checkSiblingsOnly = true }]\n"), "markdown/no-duplicate-headings").is_empty());
}

#[test]
fn an_mdx_pages_script_is_linted_where_it_sits() {
  let source = "import Chart from \"./Chart\";\nimport { unused } from \"./unused\";\nexport const meta = { title: \"Report\" };\n\n# Report\n\n<Chart data={[1, 2]} />\n\nThe total is {(() => { debugger; return 3; })()}.\n";
  let report = lint("report.mdx", source);
  let found: Vec<_> = report.findings.iter().filter(|f| f.rule.starts_with("eslint/")).map(|f| (f.rule.as_str(), f.line, f.column)).collect();
  assert_eq!(found, [("eslint/no-unused-vars", 2, 10), ("eslint/no-debugger", 9, 24)], "{}", report.render());
}

#[test]
fn an_mdx_comment_silences_a_script_finding() {
  let source = "{/* eslint-disable-next-line eslint/no-debugger */}\n{(() => { debugger; })()}\n";
  let report = lint("page.mdx", source);
  assert!(report.findings.iter().all(|f| f.rule != "eslint/no-debugger"), "{}", report.render());
}

#[test]
fn mdx_values_side_by_side_are_separate_renders() {
  let source = "import { a, b } from \"./values\";\n\nx{a}\n{b}\n\n<Card title={a}>{b}</Card>\n";
  let report = lint("page.mdx", source);
  assert!(report.findings.iter().all(|f| !f.rule.starts_with("eslint/")), "{}", report.render());
}

#[test]
fn an_mdx_expression_reads_on_past_a_brace_inside_a_string() {
  let source = "<style>{`\n  @media screen {\n    .a { color: red; }\n  }\n`}</style>\n\n{'}' + `{`}\n";
  let report = lint("page.mdx", source);
  assert!(report.findings.iter().all(|f| f.rule != "lint/parse"), "{}", report.render());
}
