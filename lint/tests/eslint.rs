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
  let translated = eslint::translate_eslintrc(&rc, &snapfire_lint::rules());
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
  let translated = eslint::translate_flat(source, Path::new("eslint.config.js"), &snapfire_lint::rules());
  assert_eq!(levels(&translated.toml, "src/a.tsx", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Warn)], "{}", translated.toml);
  assert_eq!(levels(&translated.toml, "src/a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)], "{}", translated.toml);
  assert!(translated.notes.iter().any(|n| n.contains("someShared")), "{:?}", translated.notes);
  assert!(!translated.notes.iter().any(|n| n.contains("js.configs.recommended")), "the recommended set is the default: {:?}", translated.notes);
}

#[test]
fn a_config_that_is_not_one_is_a_note() {
  let translated = eslint::translate_flat("export default makeConfig();\n", Path::new("eslint.config.mjs"), &snapfire_lint::rules());
  assert!(translated.notes.iter().any(|n| n.contains("makeConfig")), "{:?}", translated.notes);
  Config::parse(&translated.toml, Path::new("fsr.lint.toml")).unwrap();
}

#[test]
fn a_plugin_preset_off_by_default_writes_its_rules() {
  let rules = snapfire_lint::rules();
  let preset = |plugin: &str| rules.iter().filter(|r| r.id.starts_with(&format!("{plugin}/")) && r.recommended).map(|r| r.id).collect::<Vec<_>>();
  assert!(preset("unicorn").len() > 10 && preset("jsdoc").len() > 5 && preset("promise").len() > 3);
  let rc = serde_json::json!({ "extends": ["plugin:unicorn/recommended", "plugin:jsdoc/recommended", "plugin:promise/recommended"] });
  let translated = eslint::translate_eslintrc(&rc, &rules);
  let table: toml::Table = translated.toml.parse().unwrap();
  let level = |id: &str| table["rules"].get(id).and_then(|v| v.as_str()).map(str::to_owned);
  for id in preset("unicorn") {
    assert_eq!(level(id).as_deref(), Some("error"), "{id}: {}", translated.toml);
  }
  for id in preset("jsdoc") {
    assert_eq!(level(id).as_deref(), Some("warn"), "{id}");
  }
  assert_eq!(level("promise/no-nesting").as_deref(), Some("warn"));
  assert_eq!(level("promise/catch-or-return").as_deref(), Some("error"));
  assert!(!translated.notes.iter().any(|n| n.contains("unicorn") || n.contains("jsdoc") || n.contains("promise")), "{:?}", translated.notes);
  let flat = "import unicorn from \"eslint-plugin-unicorn\";\nexport default [unicorn.configs.recommended, { rules: { \"unicorn/no-null\": \"off\" } }];\n";
  let translated = eslint::translate_flat(flat, Path::new("eslint.config.js"), &rules);
  let table: toml::Table = translated.toml.parse().unwrap();
  assert_eq!(table["rules"].get("unicorn/no-null").and_then(|v| v.as_str()), Some("off"), "a later rule wins over the preset: {}", translated.toml);
  assert!(table["rules"].get(preset("unicorn")[0]).is_some(), "{}", translated.toml);
}

#[test]
fn a_plugins_rules_are_off_until_a_configuration_names_them() {
  for rule in snapfire_lint::rules().iter().filter(|r| ["unicorn/", "promise/", "jsdoc/"].iter().any(|p| r.id.starts_with(p))) {
    assert_eq!(rule.default, Level::Off, "{}", rule.id);
  }
}
