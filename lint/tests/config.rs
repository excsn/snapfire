use std::path::Path;

use snapfire_lint::{Config, ConfigError, Level, Linter};

fn levels(config: Config, path: &str, source: &str) -> Vec<(String, Level)> {
  let linter = Linter::new(config).unwrap_or_else(|e| panic!("{e}"));
  linter.lint_source(Path::new(path), source).findings.into_iter().map(|f| (f.rule, f.level)).collect()
}

fn parse(text: &str) -> Config {
  Config::parse(text, Path::new("fsr.lint.toml")).unwrap_or_else(|e| panic!("{e}"))
}

#[test]
fn a_recommended_rule_is_an_error_with_no_configuration() {
  assert_eq!(levels(Config::default(), "a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)]);
}

#[test]
fn levels_read_as_eslint_writes_them() {
  for (written, level) in [("\"off\"", None), ("\"warn\"", Some(Level::Warn)), ("\"error\"", Some(Level::Error)), ("0", None), ("1", Some(Level::Warn)), ("2", Some(Level::Error)), ("[\"warn\"]", Some(Level::Warn))] {
    let config = parse(&format!("[rules]\n\"eslint/no-debugger\" = {written}\n"));
    let found = levels(config, "a.ts", "debugger;\n");
    assert_eq!(found, level.map(|l| ("eslint/no-debugger".to_owned(), l)).into_iter().collect::<Vec<_>>(), "{written}");
  }
}

#[test]
fn an_eslint_name_without_a_plugin_means_the_eslint_rule() {
  let found = levels(parse("[rules]\n\"no-debugger\" = \"warn\"\n"), "a.ts", "debugger;\n");
  assert_eq!(found, [("eslint/no-debugger".to_owned(), Level::Warn)]);
}

#[test]
fn a_rule_takes_its_eslint_options() {
  let source = "function f(unused) {}\nf();\n";
  assert_eq!(levels(Config::default(), "a.js", source).len(), 1, "the parameter is reported by default");
  let config = parse("[rules]\n\"eslint/no-unused-vars\" = [\"error\", { args = \"none\" }]\n");
  assert!(levels(config, "a.js", source).is_empty());
}

#[test]
fn an_unknown_rule_is_an_error_naming_it() {
  let config = parse("[rules]\n\"eslint/no-such-rule\" = \"warn\"\n");
  match Linter::new(config) {
    Err(ConfigError::UnknownRule { rule, .. }) => assert_eq!(rule, "eslint/no-such-rule"),
    Err(e) => panic!("{e}"),
    Ok(_) => panic!("an unknown rule is refused"),
  }
}

#[test]
fn options_a_rule_does_not_take_are_an_error() {
  for options in ["{ nonsense = true }", "{ allowEmptyCatch = \"yes\" }"] {
    let config = parse(&format!("[rules]\n\"eslint/no-empty\" = [\"error\", {options}]\n"));
    assert!(matches!(Linter::new(config), Err(ConfigError::Options { .. })), "{options}");
  }
}

#[test]
fn a_level_that_is_not_one_is_an_error() {
  assert!(Config::parse("[rules]\n\"eslint/no-debugger\" = \"loud\"\n", Path::new("fsr.lint.toml")).is_err());
  assert!(Config::parse("[rules]\n\"eslint/no-debugger\" = 3\n", Path::new("fsr.lint.toml")).is_err());
}

#[test]
fn overrides_apply_in_order_to_the_files_their_globs_match() {
  let config = parse(
    "[rules]\n\"eslint/no-debugger\" = \"warn\"\n\n\
     [[overrides]]\nfiles = [\"tests/**\"]\nrules = { \"eslint/no-debugger\" = \"off\" }\n\n\
     [[overrides]]\nfiles = [\"tests/strict/**\"]\nrules = { \"eslint/no-debugger\" = \"error\" }\n",
  );
  assert_eq!(levels(config.clone(), "src/a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Warn)]);
  assert!(levels(config.clone(), "tests/a.ts", "debugger;\n").is_empty());
  assert_eq!(levels(config, "tests/strict/a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)]);
}

#[test]
fn a_site_inherits_the_shells_levels_and_overrides_what_it_names() {
  let shell = parse("[rules]\n\"eslint/no-debugger\" = \"warn\"\n\"eslint/no-empty\" = \"off\"\n");
  let site = parse("[rules]\n\"eslint/no-empty\" = \"error\"\n");
  let found = levels(site.over(shell), "a.ts", "debugger;\nif (x) {}\n");
  assert_eq!(found, [("eslint/no-debugger".to_owned(), Level::Warn), ("eslint/no-empty".to_owned(), Level::Error)]);
}

#[test]
fn a_site_that_does_not_inherit_starts_from_the_defaults() {
  let shell = parse("[rules]\n\"eslint/no-debugger\" = \"off\"\n");
  let site = parse("inherit = false\n");
  assert_eq!(levels(site.over(shell), "a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)]);
}

#[test]
fn a_project_without_the_file_has_the_defaults() {
  let dir = tempfile::tempdir().unwrap();
  let config = Config::load(dir.path()).unwrap();
  assert_eq!(levels(config, "a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)]);
  std::fs::write(dir.path().join(snapfire_lint::CONFIG_FILE), "[rules]\n\"eslint/no-debugger\" = \"off\"\n").unwrap();
  assert!(levels(Config::load(dir.path()).unwrap(), "a.ts", "debugger;\n").is_empty());
}

#[test]
fn ignores_skip_files_a_walk_would_lint() {
  let dir = tempfile::tempdir().unwrap();
  let root = dir.path();
  std::fs::create_dir_all(root.join("vendor/react")).unwrap();
  std::fs::create_dir_all(root.join("routes")).unwrap();
  std::fs::write(root.join("vendor/react/index.js"), "debugger;\n").unwrap();
  std::fs::write(root.join("routes/page.ts"), "debugger;\n").unwrap();
  std::fs::write(root.join("routes/old.ts"), "debugger;\n").unwrap();
  let mut config = parse("ignores = [\"routes/old.ts\"]\n");
  config.ignore("vendor/**").unwrap();
  let report = Linter::new(config).unwrap().lint_dir(root);
  assert_eq!(report.files, 1);
  let paths: Vec<_> = report.findings.iter().map(|f| f.path.clone()).collect();
  assert_eq!(paths, [root.join("routes/page.ts")]);
}

#[test]
fn discover_layers_every_file_from_the_repository_down() {
  let dir = tempfile::tempdir().unwrap();
  let repo = dir.path();
  std::fs::create_dir_all(repo.join(".git")).unwrap();
  std::fs::create_dir_all(repo.join("sites/notes")).unwrap();
  std::fs::write(repo.join("fsr.lint.toml"), "[rules]\n\"eslint/no-debugger\" = \"warn\"\n\"eslint/no-empty\" = \"off\"\n").unwrap();
  std::fs::write(repo.join("sites/notes/fsr.lint.toml"), "[rules]\n\"eslint/no-empty\" = \"error\"\n").unwrap();
  let config = Config::discover(&repo.join("sites/notes")).unwrap();
  let found = levels(config, "a.ts", "debugger;\nif (x) {}\n");
  assert_eq!(found, [("eslint/no-debugger".to_owned(), Level::Warn), ("eslint/no-empty".to_owned(), Level::Error)]);
}

#[test]
fn discover_stops_at_a_file_that_does_not_inherit_and_at_the_repository() {
  let dir = tempfile::tempdir().unwrap();
  let outside = dir.path();
  std::fs::write(outside.join("fsr.lint.toml"), "[rules]\n\"eslint/no-debugger\" = \"off\"\n").unwrap();
  let repo = outside.join("repo");
  std::fs::create_dir_all(repo.join(".git")).unwrap();
  std::fs::create_dir_all(repo.join("app")).unwrap();
  assert_eq!(levels(Config::discover(&repo.join("app")).unwrap(), "a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)], "a file above the repository is not read");
  std::fs::write(repo.join("fsr.lint.toml"), "[rules]\n\"eslint/no-debugger\" = \"off\"\n").unwrap();
  std::fs::write(repo.join("app/fsr.lint.toml"), "inherit = false\n").unwrap();
  assert_eq!(levels(Config::discover(&repo.join("app")).unwrap(), "a.ts", "debugger;\n"), [("eslint/no-debugger".to_owned(), Level::Error)], "inherit = false starts from the defaults");
}
