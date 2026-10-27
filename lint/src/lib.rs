//! Lint TypeScript and JavaScript with ESLint's rules, configured the way ESLint is.

use std::{
  collections::HashMap,
  fmt::Write as _,
  path::{Path, PathBuf},
};

mod config;
mod js;
mod suppress;

pub use config::Config;

use config::Setting;

/// The file a project's levels live in, at its root.
pub const CONFIG_FILE: &str = "fsr.lint.toml";

/// A file that does not parse. No rule runs on it.
pub const PARSE: &str = "lint/parse";

/// A suppression comment that silences nothing.
pub const UNUSED_DISABLE: &str = "lint/unused-disable";

/// How loud a rule is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
  Off,
  Warn,
  Error,
}

impl Level {
  /// As a configuration file writes it.
  pub fn as_str(self) -> &'static str {
    match self {
      Level::Off => "off",
      Level::Warn => "warn",
      Level::Error => "error",
    }
  }
}

/// One thing a rule found.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Finding {
  /// `eslint/no-debugger`.
  pub rule: String,
  pub level: Level,
  pub path: PathBuf,
  /// 1-based.
  pub line: usize,
  /// 1-based, in characters.
  pub column: usize,
  pub message: String,
  pub help: Option<String>,
}

/// A finding a comment silenced, with the reason the comment gave.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Suppressed {
  pub finding: Finding,
  pub reason: Option<String>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ConfigError {
  #[error("{path}: {message}", path = path.display())]
  Parse { path: PathBuf, message: String },
  #[error("{path}: no rule is called `{rule}`", path = path.display())]
  UnknownRule { path: PathBuf, rule: String },
  #[error("{path}: `{rule}` does not take these options: {message}", path = path.display())]
  Options { path: PathBuf, rule: String, message: String },
}

/// What one run found.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Report {
  pub files: usize,
  pub findings: Vec<Finding>,
  pub suppressed: Vec<Suppressed>,
}

impl Report {
  pub fn errors(&self) -> usize {
    self.findings.iter().filter(|f| f.level == Level::Error).count()
  }

  pub fn warnings(&self) -> usize {
    self.findings.iter().filter(|f| f.level == Level::Warn).count()
  }

  /// Whether the run fails: an error, or a warning when `deny_warnings`.
  pub fn failed(&self, deny_warnings: bool) -> bool {
    self.errors() > 0 || (deny_warnings && self.warnings() > 0)
  }

  /// One finding per line with its help beneath it, then the totals.
  pub fn render(&self) -> String {
    let mut out = String::new();
    for f in &self.findings {
      let level = match f.level {
        Level::Error => "error",
        _ => "warning",
      };
      let _ = writeln!(out, "{level:<8} {:<34} {}:{}:{}  {}", f.rule, f.path.display(), f.line, f.column, f.message);
      if let Some(help) = &f.help {
        let _ = writeln!(out, "{:<8} {:<34} {help}", "", "");
      }
    }
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let _ = write!(out, "{:<8} {}, {} in {}", "lint", plural(self.errors(), "error", "errors"), plural(self.warnings(), "warning", "warnings"), plural(self.files, "file", "files"));
    out
  }

  pub fn to_json(&self) -> serde_json::Value {
    let finding = |f: &Finding| {
      serde_json::json!({
        "rule": f.rule,
        "level": f.level.as_str(),
        "path": f.path.display().to_string(),
        "line": f.line,
        "column": f.column,
        "message": f.message,
        "help": f.help,
      })
    };
    serde_json::json!({
      "files": self.files,
      "errors": self.errors(),
      "warnings": self.warnings(),
      "findings": self.findings.iter().map(finding).collect::<Vec<_>>(),
      "suppressed": self.suppressed.iter().map(|s| {
        let mut value = finding(&s.finding);
        value["reason"] = serde_json::json!(s.reason);
        value
      }).collect::<Vec<_>>(),
    })
  }

  fn absorb(&mut self, other: Report) {
    self.files += other.files;
    self.findings.extend(other.findings);
    self.suppressed.extend(other.suppressed);
  }
}

/// What a rule is, for listing.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RuleInfo {
  pub id: &'static str,
  pub default: Level,
  /// `correctness`, `suspicious`, `pedantic` and the rest of clippy's categories; `lint` for the linter's own.
  pub category: &'static str,
  /// Whether the rule offers a fix or a suggestion for some of what it finds.
  pub fixable: bool,
  pub docs: &'static str,
}

const CORE: [RuleInfo; 2] = [
  RuleInfo { id: PARSE, default: Level::Error, category: "lint", fixable: false, docs: "A file that does not parse. No rule runs on it.\n" },
  RuleInfo { id: UNUSED_DISABLE, default: Level::Warn, category: "lint", fixable: false, docs: "A suppression comment that silences nothing, such as one left after the code it covered was fixed.\n" },
];

/// Every rule this crate has, sorted by id.
pub fn rules() -> Vec<RuleInfo> {
  let mut all: Vec<RuleInfo> = CORE.into_iter().collect();
  all.extend(js::RULES.iter().map(|e| RuleInfo { id: e.id, default: if e.recommended { Level::Error } else { Level::Off }, category: e.category, fixable: e.fixable, docs: e.docs }));
  all.sort_by_key(|r| r.id);
  all
}

/// The rules a configuration turns on, ready to run.
pub struct Linter {
  config: Config,
  /// Each rule built with each set of options the configuration gives it, keyed by rule id and options.
  built: HashMap<(&'static str, String), Box<dyn js::Run>>,
}

impl Linter {
  pub fn new(config: Config) -> Result<Linter, ConfigError> {
    let mut built = HashMap::new();
    for entry in js::RULES {
      built.insert((entry.id, String::new()), (entry.build)(None).expect("every rule builds with no options"));
    }
    for layer in config.layers() {
      for (id, setting) in &layer.rules {
        if CORE.iter().any(|r| r.id == id) {
          continue;
        }
        let Some(entry) = js::RULES.iter().find(|e| e.id == id) else {
          return Err(ConfigError::UnknownRule { path: layer.path.clone(), rule: id.clone() });
        };
        if let Some(options) = &setting.options {
          let rule = (entry.build)(Some(options.clone())).map_err(|e| ConfigError::Options { path: layer.path.clone(), rule: id.clone(), message: e.to_string() })?;
          built.insert((entry.id, options.to_string()), rule);
        }
      }
    }
    Ok(Linter { config, built })
  }

  /// Each rule's setting for a file at `relative`, a path from the project root.
  fn settings(&self, relative: &Path) -> HashMap<&'static str, Setting> {
    let mut settings: HashMap<&'static str, Setting> = rules().into_iter().map(|r| (r.id, Setting { level: r.default, options: None })).collect();
    let relative = relative.strip_prefix(".").unwrap_or(relative);
    for layer in self.config.layers() {
      if layer.files.as_ref().is_some_and(|globs| !globs.is_match(relative)) {
        continue;
      }
      for (id, setting) in &layer.rules {
        if let Some(current) = settings.iter_mut().find(|(known, _)| *known == id).map(|(_, s)| s) {
          current.level = setting.level;
          if setting.options.is_some() {
            current.options = setting.options.clone();
          }
        }
      }
    }
    settings
  }

  /// One file's source, as if read from `path`.
  pub fn lint_source(&self, path: &Path, text: &str) -> Report {
    self.lint_file(path, path, text)
  }

  fn lint_file(&self, path: &Path, relative: &Path, text: &str) -> Report {
    if !js::reads(path) {
      return Report::default();
    }
    let settings = self.settings(relative);
    let active: Vec<(&'static str, &dyn js::Run)> = js::RULES
      .iter()
      .filter_map(|entry| {
        let setting = &settings[entry.id];
        if setting.level == Level::Off {
          return None;
        }
        let key = setting.options.as_ref().map(ToString::to_string).unwrap_or_default();
        self.built.get(&(entry.id, key)).map(|rule| (entry.id, rule.as_ref()))
      })
      .collect();
    let outcome = js::lint(path, text, &active);
    let lines = suppress::Lines::new(text);
    let directives = suppress::directives(outcome.comments.iter().map(|c| (c.text.as_str(), c.start, c.end)), &lines);
    let mut report = Report { files: 1, ..Report::default() };
    let finding = |rule: &str, level: Level, start: usize, message: String, help: Option<String>| Finding { rule: rule.to_owned(), level, path: path.to_owned(), line: lines.line(start), column: lines.column(start), message, help };
    let mut reported = outcome.reported;
    reported.sort_by_key(|r| (r.start, r.end));
    for r in reported {
      let level = settings.get(r.rule).map_or(Level::Error, |s| s.level);
      if level == Level::Off {
        continue;
      }
      let found = finding(r.rule, level, r.start, r.message, r.help);
      let silenced = match r.rule == PARSE {
        true => None,
        false => suppress::silencing(&directives, r.rule, r.start, found.line),
      };
      match silenced {
        Some(directive) => report.suppressed.push(Suppressed { finding: found, reason: directive.reason.clone() }),
        None => report.findings.push(found),
      }
    }
    let unused = settings[UNUSED_DISABLE].level;
    if unused != Level::Off {
      for d in directives.iter().filter(|d| d.silences() && !d.used()) {
        report.findings.push(finding(UNUSED_DISABLE, unused, d.start, format!("`{}` silences nothing", d.written), Some("remove the comment".to_owned())));
      }
      report.findings.sort_by_key(|f| (f.line, f.column));
    }
    report
  }

  /// Every file under `root` a front end reads, skipping what `.gitignore` ignores.
  pub fn lint_dir(&self, root: &Path) -> Report {
    let mut report = Report::default();
    let mut paths: Vec<PathBuf> = ignore::WalkBuilder::new(root).require_git(false).build().filter_map(Result::ok).filter(|e| e.file_type().is_some_and(|t| t.is_file())).map(ignore::DirEntry::into_path).filter(|p| js::reads(p)).collect();
    paths.sort();
    for path in paths {
      let Ok(text) = std::fs::read_to_string(&path) else { continue };
      let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
      report.absorb(self.lint_file(&path, &relative, &text));
    }
    report
  }
}
