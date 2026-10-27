//! An application's ESLint configuration becomes `fsr.lint.toml`, and what has no counterpart is noted.

use snapfire_fsr_migrate::{migrate, Tree};

fn tree(files: &[(&str, &str)]) -> Tree {
  let mut t = Tree::new();
  for (p, s) in files {
    t.insert(*p, *s);
  }
  t
}

const PACKAGE: &str = r#"{ "name": "tool", "type": "module", "dependencies": { "ink": "^5", "react": "^18" } }"#;
const ENTRY: &str = "import React from 'react';\nimport {render, Text} from 'ink';\n\nrender(<Text>hi</Text>);\n";

#[test]
fn an_eslintrc_becomes_the_lint_file() {
  let rc = r#"{ "extends": ["eslint:recommended", "xo"], "rules": { "no-debugger": "warn", "unicorn/filename-case": "error" } }"#;
  let (out, report) = migrate(&tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), (".eslintrc.json", rc), (".eslintignore", "dist\n# built\nbuild/\n")]));
  let lint = out.text("fsr.lint.toml").expect("the lint file is written");
  assert!(lint.contains("\"eslint/no-debugger\" = \"warn\""), "{lint}");
  assert!(lint.contains("\"dist\"") && lint.contains("\"build/**\""), "the ignore file carries over: {lint}");
  assert!(out.get(".eslintrc.json").is_none() && out.get(".eslintignore").is_none(), "ESLint's files are not left behind");
  for named in ["xo", "unicorn/filename-case"] {
    assert!(report.notes.iter().any(|n| n.file == ".eslintrc.json" && n.message.contains(named)), "{named}: {:?}", report.notes);
  }
}

#[test]
fn a_flat_config_wins_over_an_eslintrc_as_eslint_9_reads_them() {
  let flat = "export default [{ rules: { 'no-empty': 'off' } }];\n";
  let rc = r#"{ "rules": { "no-debugger": "off" } }"#;
  let (out, report) = migrate(&tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY), ("eslint.config.js", flat), (".eslintrc.json", rc)]));
  let lint = out.text("fsr.lint.toml").expect("the lint file is written");
  assert!(lint.contains("eslint/no-empty") && !lint.contains("eslint/no-debugger"), "{lint}");
  assert!(report.notes.iter().any(|n| n.file == ".eslintrc.json" && n.message.contains("eslint.config.js")), "{:?}", report.notes);
}

#[test]
fn package_json_s_eslint_config_is_read() {
  let package = r#"{ "name": "tool", "type": "module", "dependencies": { "ink": "^5", "react": "^18" }, "eslintConfig": { "rules": { "no-debugger": 1 } } }"#;
  let (out, _) = migrate(&tree(&[("package.json", package), ("src/cli.tsx", ENTRY)]));
  assert!(out.text("fsr.lint.toml").is_some_and(|l| l.contains("\"eslint/no-debugger\" = 1")), "{:?}", out.text("fsr.lint.toml"));
}

#[test]
fn no_eslint_configuration_writes_no_lint_file() {
  let (out, _) = migrate(&tree(&[("package.json", PACKAGE), ("src/cli.tsx", ENTRY)]));
  assert!(out.get("fsr.lint.toml").is_none());
}
