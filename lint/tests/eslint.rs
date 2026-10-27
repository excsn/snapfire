use std::path::Path;

use snapfire_lint::{Config, Level, Linter, eslint};

fn levels(toml: &str, path: &str, source: &str) -> Vec<(String, Level)> {
  let config = Config::parse(toml, Path::new("fsr.lint.toml")).unwrap_or_else(|e| panic!("{e}\n{toml}"));
  Linter::new(config).unwrap_or_else(|e| panic!("{e}\n{toml}")).lint_source(Path::new(path), source).findings.into_iter().map(|f| (f.rule, f.level)).collect()
}

#[test]
fn an_eslintrc_translates_rules_overrides_ignores_and_settings() {
  let rc = serde_json::json!({
    "extends": ["eslint:recommended", "plugin:react/recommended", "next/core-web-vitals"],
    "rules": {
      "no-debugger": "warn",
      "@typescript-eslint/no-unused-vars": ["error", { "argsIgnorePattern": "^_" }],
      "react-hooks/exhaustive-deps": 1,
      "no-console": "off",
      "no-restricted-syntax": "error",
      "import/order": "error"
    },
    "overrides": [{ "files": ["tests/**"], "rules": { "no-debugger": "off" } }],
    "ignorePatterns": ["dist/"],
    "settings": { "react": { "version": "18.2.0" } },
    "env": { "browser": true }
  });
  let translated = eslint::translate_eslintrc(&rc);
  assert_eq!(levels(&translated.toml, "src/a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Warn)], "{}", translated.toml);
  assert!(levels(&translated.toml, "tests/a.ts", "debugger;\n").is_empty(), "{}", translated.toml);
  assert!(levels(&translated.toml, "src/a.ts", "function f(_a) {}\nf();\n").is_empty(), "the options carry over: {}", translated.toml);
  for named in ["no-restricted-syntax", "import/order", "next/core-web-vitals", "env"] {
    assert!(translated.notes.iter().any(|n| n.contains(named)), "{named} is noted: {:?}", translated.notes);
  }
  assert!(!translated.notes.iter().any(|n| n.contains("eslint:recommended") && n.contains("no counterpart")), "{:?}", translated.notes);
  assert!(translated.toml.contains("\"eslint/no-console\" = \"off\""), "{}", translated.toml);
  assert!(translated.toml.contains("ignores"), "{}", translated.toml);
}

#[test]
fn a_flat_config_translates_what_it_writes_literally() {
  let source = r#"
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";

export default [
  js.configs.recommended,
  { ignores: ["dist/**"] },
  {
    files: ["**/*.tsx"],
    rules: { "no-debugger": ["warn"], "react-hooks/rules-of-hooks": "error" },
  },
  ...someShared(),
];
"#;
  let translated = eslint::translate_flat(source, Path::new("eslint.config.js"));
  assert_eq!(levels(&translated.toml, "src/a.tsx", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Warn)], "{}", translated.toml);
  assert_eq!(levels(&translated.toml, "src/a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)], "{}", translated.toml);
  assert!(translated.notes.iter().any(|n| n.contains("someShared")), "{:?}", translated.notes);
  assert!(!translated.notes.iter().any(|n| n.contains("js.configs.recommended")), "the recommended set is the default: {:?}", translated.notes);
}

#[test]
fn a_config_that_is_not_one_is_a_note() {
  let translated = eslint::translate_flat("export default makeConfig();\n", Path::new("eslint.config.mjs"));
  assert!(translated.notes.iter().any(|n| n.contains("makeConfig")), "{:?}", translated.notes);
  Config::parse(&translated.toml, Path::new("fsr.lint.toml")).unwrap();
}
