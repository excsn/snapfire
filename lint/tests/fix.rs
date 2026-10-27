use std::path::Path;

use snapfire_lint::{Config, Linter};

fn linter(config: &str) -> Linter {
  Linter::new(Config::parse(config, Path::new("fsr.lint.toml")).unwrap()).unwrap()
}

#[test]
fn fix_applies_the_safe_fixes_and_leaves_the_rest() {
  let source = "let a = /a  b/;\nexport const b = a;\nloop: while (b) { break; }\nif (b) {}\n";
  let fixed = linter("").fix_source(Path::new("a.ts"), source);
  assert_eq!(fixed.text, "let a = /a {2}b/;\nexport const b = a;\nwhile (b) { break; }\nif (b) {}\n");
  assert_eq!(fixed.applied, 2);
  let left: Vec<_> = linter("").lint_source(Path::new("a.ts"), &fixed.text).findings.into_iter().map(|f| f.rule).collect();
  assert_eq!(left, ["eslint/no-empty"], "a rule with no safe fix is still reported");
}

#[test]
fn fix_leaves_what_a_comment_silences_and_what_is_off() {
  let source = "// eslint-disable-next-line no-regex-spaces\nexport const a = /a  b/;\nexport const c = /c  d/;\n";
  assert_eq!(linter("").fix_source(Path::new("a.ts"), source).text, "// eslint-disable-next-line no-regex-spaces\nexport const a = /a  b/;\nexport const c = /c {2}d/;\n");
  let off = linter("[rules]\n\"eslint/no-regex-spaces\" = \"off\"\n").fix_source(Path::new("a.ts"), "export const c = /c  d/;\n");
  assert_eq!(off.applied, 0);
}

#[test]
fn a_file_with_nothing_to_fix_comes_back_unchanged() {
  let source = "export const a = 1;\n";
  let fixed = linter("").fix_source(Path::new("a.ts"), source);
  assert_eq!((fixed.text.as_str(), fixed.applied), (source, 0));
}
