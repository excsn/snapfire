//! Lint TypeScript and JavaScript with ESLint's rules, configured the way ESLint is.

use std::{
  collections::HashMap,
  fmt::Write as _,
  path::{Path, PathBuf},
};

mod config;
pub mod eslint;
mod js;
mod markdown;
mod suppress;
mod templates;
mod tera;

pub use config::Config;
pub use oxc_ast;
pub use oxc_semantic;
pub use oxc_span;

/// One script as a [`Check`] sees it: a whole file, or one `<script>` block of a component.
#[non_exhaustive]
pub struct File<'a> {
  pub path: &'a Path,
  pub source: &'a str,
  pub semantic: &'a oxc_semantic::Semantic<'a>,
}

/// A rule from outside this crate. It reports a byte offset into `file.source` and a message; the
/// configuration, its levels and suppression comments apply to it as to any rule.
pub trait Check: Send + Sync {
  fn info(&self) -> RuleInfo;
  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String));
}
pub use templates::Plugins;

/// Asks a framework's compiler about one component: `ext` is `vue` or `svelte`. `fsr lint` answers
/// through the `snapfirec-vue` and `snapfirec-svelte` plugins; anything that holds the compilers can.
pub trait Compilers: Send + Sync {
  fn describe(&self, ext: &str, unit: snapfire_compiler_wire::Unit) -> Result<snapfire_compiler_wire::Outcome, String>;
  fn compile(&self, ext: &str, unit: snapfire_compiler_wire::Unit) -> Result<snapfire_compiler_wire::Outcome, String>;
}

use config::Setting;

/// The file a project's levels live in, at its root.
pub const CONFIG_FILE: &str = "fsr.lint.toml";

/// A file that does not parse. No rule runs on it.
pub const PARSE: &str = "lint/parse";

/// A suppression comment that silences nothing.
pub const UNUSED_DISABLE: &str = "lint/unused-disable";

/// A framework's compiler that could not be asked about a component, so its template went unlinted.
pub const COMPILER: &str = "lint/compiler";

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

  /// SARIF 2.1.0, for a CI system that annotates the changed lines.
  pub fn to_sarif(&self) -> serde_json::Value {
    let mut ids: Vec<&str> = self.findings.iter().map(|f| f.rule.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let results: Vec<_> = self
      .findings
      .iter()
      .map(|f| {
        serde_json::json!({
          "ruleId": f.rule,
          "level": match f.level { Level::Error => "error", _ => "warning" },
          "message": { "text": f.help.as_ref().map_or_else(|| f.message.clone(), |help| format!("{} {help}", f.message)) },
          "locations": [{ "physicalLocation": { "artifactLocation": { "uri": f.path.display().to_string() }, "region": { "startLine": f.line, "startColumn": f.column } } }],
        })
      })
      .collect();
    serde_json::json!({
      "version": "2.1.0",
      "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
      "runs": [{
        "tool": { "driver": { "name": "fsr lint", "informationUri": "https://github.com/excsn/snapfire", "rules": ids.iter().map(|id| serde_json::json!({ "id": id })).collect::<Vec<_>>() } },
        "results": results,
      }],
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
  /// Whether the rule is in its plugin's own recommended set, which extending the plugin's preset turns on.
  pub recommended: bool,
  /// `correctness`, `suspicious`, `pedantic` and the rest of clippy's categories; `lint` for the linter's own.
  pub category: &'static str,
  /// Whether the rule offers a fix or a suggestion for some of what it finds.
  pub fixable: bool,
  pub docs: &'static str,
}

const CORE: [RuleInfo; 3] = [
  RuleInfo { id: COMPILER, default: Level::Warn, recommended: false, category: "lint", fixable: false, docs: "A `.vue` or `.svelte` file whose framework compiler could not be asked, so its template is not linted. Its scripts still are.\n" },
  RuleInfo { id: PARSE, default: Level::Error, recommended: false, category: "lint", fixable: false, docs: "A file that does not parse. No rule runs on it.\n" },
  RuleInfo { id: UNUSED_DISABLE, default: Level::Warn, recommended: false, category: "lint", fixable: false, docs: "A suppression comment that silences nothing, such as one left after the code it covered was fixed.\n" },
];

/// Recommended rules that stay off unless a configuration turns them on: the automatic JSX runtime every
/// FSR build uses compiles JSX without `React` in scope, as eslint-plugin-react's `jsx-runtime` config says.
const OFF_BY_DEFAULT: [&str; 1] = ["react/react-in-jsx-scope"];

/// The plugins whose recommended rules run without a configuration naming them, as the presets a TypeScript, React
/// or Vue application's ESLint configuration extends. Another plugin's recommended rules are off until its preset or
/// a rule is named.
const ON_BY_DEFAULT: [&str; 5] = ["eslint", "jsx-a11y", "react", "typescript", "vue"];

impl RuleInfo {
  pub const fn new(id: &'static str, default: Level, category: &'static str, docs: &'static str) -> Self {
    RuleInfo { id, default, recommended: false, category, fixable: false, docs }
  }
}

/// Every rule this crate has, sorted by id.
pub fn rules() -> Vec<RuleInfo> {
  let mut all: Vec<RuleInfo> = CORE.into_iter().collect();
  all.extend(js::RULES.iter().map(|e| RuleInfo { id: e.id, default: if e.recommended && !OFF_BY_DEFAULT.contains(&e.id) && ON_BY_DEFAULT.iter().any(|plugin| e.id.split('/').next() == Some(*plugin)) { Level::Error } else { Level::Off }, recommended: e.recommended, category: e.category, fixable: e.fixable, docs: e.docs }));
  all.extend(templates::VUE_RULES);
  all.extend(markdown::RULES);
  all.extend(tera::RULES);
  all.push(templates::SVELTE_FAMILY);
  all.sort_by_key(|r| r.id);
  all
}

/// A file's text after its safe fixes.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Fixed {
  pub text: String,
  /// How many fixes were applied.
  pub applied: usize,
}

/// Whether some front end reads `path`.
fn reads(path: &Path) -> bool {
  js::reads(path) || markdown::reads(path).is_some() || tera::reads(path)
}

/// The rules a configuration turns on, ready to run.
pub struct Linter {
  config: Config,
  ignored: globset::GlobSet,
  settings: js::OxlintSettings,
  /// Each rule built with each set of options the configuration gives it, keyed by rule id and options.
  built: HashMap<(&'static str, String), Box<dyn js::Run>>,
  compilers: Option<Box<dyn Compilers>>,
  checks: Vec<(RuleInfo, Box<dyn Check>)>,
}

/// Whether `id` names one of Svelte's warnings, which are known by prefix rather than listed.
fn svelte_warning(id: &str) -> bool {
  id.starts_with("svelte/") && id != templates::SVELTE_FAMILY.id
}

impl Linter {
  pub fn new(config: Config) -> Result<Linter, ConfigError> {
    Linter::new_with(config, Vec::new())
  }

  /// A linter that runs `checks` beside this crate's rules.
  pub fn new_with(config: Config, checks: Vec<Box<dyn Check>>) -> Result<Linter, ConfigError> {
    let checks: Vec<(RuleInfo, Box<dyn Check>)> = checks.into_iter().map(|c| (c.info(), c)).collect();
    let mut built = HashMap::new();
    for entry in js::RULES {
      built.insert((entry.id, String::new()), (entry.build)(None).expect("every rule builds with no options"));
    }
    for layer in config.layers() {
      for (id, setting) in &layer.rules {
        if CORE.iter().chain(&templates::VUE_RULES).chain(&markdown::RULES).chain(&tera::RULES).any(|r| r.id == id) || checks.iter().any(|(info, _)| info.id == id) || svelte_warning(id) {
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
    let settings = js::settings(config.settings()).map_err(|e| ConfigError::Parse { path: PathBuf::from(CONFIG_FILE), message: format!("`settings`: {e}") })?;
    Ok(Linter { ignored: config.ignored(), settings, config, built, compilers: None, checks })
  }

  /// Each rule's setting for a file at `relative`, a path from the project root.
  fn settings(&self, relative: &Path) -> HashMap<String, Setting> {
    let mut settings: HashMap<String, Setting> = self.rules().into_iter().map(|r| (r.id.to_owned(), Setting { level: r.default, options: None })).collect();
    let relative = relative.strip_prefix(".").unwrap_or(relative);
    for layer in self.config.layers() {
      if layer.files.as_ref().is_some_and(|globs| !globs.is_match(relative)) {
        continue;
      }
      for (id, setting) in &layer.rules {
        if svelte_warning(id) && !settings.contains_key(id) {
          settings.insert(id.clone(), Setting { level: templates::SVELTE_FAMILY.default, options: None });
        }
        if let Some(current) = settings.get_mut(id) {
          current.level = setting.level;
          if setting.options.is_some() {
            current.options = setting.options.clone();
          }
        }
      }
    }
    settings
  }

  /// This crate's rules and the checks given to `new_with`, sorted by id.
  pub fn rules(&self) -> Vec<RuleInfo> {
    let mut all = rules();
    all.extend(self.checks.iter().map(|(info, _)| info.clone()));
    all.sort_by_key(|r| r.id);
    all
  }

  /// Lint the templates of `.vue` and `.svelte` files through `compilers`.
  pub fn with_compilers(mut self, compilers: Box<dyn Compilers>) -> Self {
    self.compilers = Some(compilers);
    self
  }

  /// `text` with every safe fix applied, until none is left or ten passes have run.
  pub fn fix_source(&self, path: &Path, text: &str) -> Fixed {
    self.fix_file(path, path, text)
  }

  /// Every file under `root` that some fix changes, with its fixed text.
  pub fn fix_dir(&self, root: &Path) -> Vec<(PathBuf, Fixed)> {
    self
      .files(root)
      .into_iter()
      .filter_map(|path| {
        let text = std::fs::read_to_string(&path).ok()?;
        let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
        let fixed = self.fix_file(&path, &relative, &text);
        (fixed.applied > 0).then_some((path, fixed))
      })
      .collect()
  }

  fn fix_file(&self, path: &Path, relative: &Path, text: &str) -> Fixed {
    let mut fixed = Fixed { text: text.to_owned(), applied: 0 };
    if !js::reads(path) {
      return fixed;
    }
    let settings = self.settings(relative);
    let active = self.active(&settings);
    for _ in 0..10 {
      let (comments, messages) = js::fixes(path, &fixed.text, &active, &self.settings);
      let lines = suppress::Lines::new(&fixed.text);
      let directives = suppress::directives(comments.iter().map(|c| (c.text.as_str(), c.start, c.end)), &lines);
      let messages: Vec<_> = messages.into_iter().filter(|(rule, start, _)| suppress::silencing(&directives, rule, *start, lines.line(*start)).is_none()).map(|(_, _, m)| m).collect();
      let (text, applied) = js::apply(path, &fixed.text, messages);
      if applied == 0 {
        break;
      }
      fixed.text = text;
      fixed.applied += applied;
    }
    fixed
  }

  /// The JS rules a file's settings turn on, each built with its options.
  fn active(&self, settings: &HashMap<String, Setting>) -> Vec<(&'static str, &dyn js::Run)> {
    js::RULES
      .iter()
      .filter_map(|entry| {
        let setting = &settings[entry.id];
        if setting.level == Level::Off {
          return None;
        }
        let key = setting.options.as_ref().map(ToString::to_string).unwrap_or_default();
        self.built.get(&(entry.id, key)).map(|rule| (entry.id, rule.as_ref()))
      })
      .collect()
  }

  /// One file's source, as if read from `path`.
  pub fn lint_source(&self, path: &Path, text: &str) -> Report {
    self.lint_file(path, path, text)
  }

  fn lint_file(&self, path: &Path, relative: &Path, text: &str) -> Report {
    if !reads(path) {
      return Report::default();
    }
    let settings = self.settings(relative);
    if let Some(mdx) = markdown::reads(path) {
      let lines = suppress::Lines::new(text);
      let comments = markdown::comments(text, mdx);
      let titles = markdown::Titles::of(|rule| settings.get(rule).and_then(|setting| setting.options.clone()));
      let reported = match markdown::lint(text, mdx, &titles) {
        Ok(found) => found.into_iter().map(|f| (f.rule.to_owned(), f.start, f.message, None)).collect(),
        Err((at, message)) => vec![(PARSE.to_owned(), at, message, None)],
      };
      return self.report(path, &lines, &settings, comments, reported);
    }
    if tera::reads(path) {
      let lines = suppress::Lines::new(text);
      let reported = match tera::lint(path, text) {
        Ok(found) => found.into_iter().map(|f| (f.rule.to_owned(), f.start, f.message, None)).collect(),
        Err((at, message)) => vec![(PARSE.to_owned(), at, message, None)],
      };
      return self.report(path, &lines, &settings, tera::comments(text), reported);
    }
    let active = self.active(&settings);
    let checks: Vec<(&'static str, &dyn Check)> = self.checks.iter().filter(|(info, _)| settings.get(info.id).is_some_and(|s| s.level != Level::Off)).map(|(info, check)| (info.id, check.as_ref())).collect();
    let outcome = js::lint(path, text, &active, &checks, &self.settings);
    let lines = suppress::Lines::new(text);
    let mut comments: Vec<(String, usize, usize)> = outcome.comments.into_iter().map(|c| (c.text, c.start, c.end)).collect();
    let mut reported: Vec<(String, usize, String, Option<String>)> = outcome.reported.into_iter().map(|r| (r.rule.to_owned(), r.start, r.message, r.help)).collect();
    if let (Some(ext), Some(compilers)) = (templates::reads(path), &self.compilers) {
      comments.extend(templates::comments(text));
      for found in templates::lint(ext, path, text, compilers.as_ref()) {
        reported.push((found.rule, lines.offset(found.line, found.column), found.message, found.help));
      }
    }
    self.report(path, &lines, &settings, comments, reported)
  }

  /// The report of one file from what its front end found, after its suppression comments.
  fn report(&self, path: &Path, lines: &suppress::Lines, settings: &HashMap<String, Setting>, comments: Vec<(String, usize, usize)>, mut reported: Vec<(String, usize, String, Option<String>)>) -> Report {
    let directives = suppress::directives(comments.iter().map(|(text, start, end)| (text.as_str(), *start, *end)), lines);
    let mut report = Report { files: 1, ..Report::default() };
    let finding = |rule: &str, level: Level, start: usize, message: String, help: Option<String>| Finding { rule: rule.to_owned(), level, path: path.to_owned(), line: lines.line(start), column: lines.column(start), message, help };
    reported.sort_by_key(|r| r.1);
    for (rule, start, message, help) in reported {
      let level = settings.get(&rule).map(|s| s.level).unwrap_or(if svelte_warning(&rule) { templates::SVELTE_FAMILY.default } else { Level::Error });
      if level == Level::Off {
        continue;
      }
      let found = finding(&rule, level, start, message, help);
      let silenced = match rule == PARSE {
        true => None,
        false => suppress::silencing(&directives, &rule, start, found.line),
      };
      match silenced {
        Some(directive) => report.suppressed.push(Suppressed { finding: found, reason: directive.reason.clone() }),
        None => report.findings.push(found),
      }
    }
    let unused = settings[UNUSED_DISABLE].level;
    if unused != Level::Off {
      let known = |id: &String| settings.contains_key(id) || svelte_warning(id);
      for d in directives.iter().filter(|d| d.silences() && !d.used() && d.rules().is_some_and(|rules| rules.iter().all(known))) {
        report.findings.push(finding(UNUSED_DISABLE, unused, d.start, format!("`{}` silences nothing", d.written), Some("remove the comment".to_owned())));
      }
      report.findings.sort_by_key(|f| (f.line, f.column));
    }
    report
  }

  /// Every file under `root` a front end reads, skipping what `.gitignore` ignores.
  pub fn lint_dir(&self, root: &Path) -> Report {
    let mut report = Report::default();
    for path in self.files(root) {
      let Ok(text) = std::fs::read_to_string(&path) else { continue };
      let relative = path.strip_prefix(root).unwrap_or(&path).to_owned();
      report.absorb(self.lint_file(&path, &relative, &text));
    }
    report
  }

  /// The files under `root` a front end reads, skipping what `.gitignore` and the ignores match, in path order.
  pub fn files(&self, root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = ignore::WalkBuilder::new(root)
      .require_git(false)
      .build()
      .filter_map(Result::ok)
      .filter(|e| e.file_type().is_some_and(|t| t.is_file()))
      .map(ignore::DirEntry::into_path)
      .filter(|p| reads(p) && !self.ignored.is_match(p.strip_prefix(root).unwrap_or(p)))
      .collect();
    paths.sort();
    paths
  }
}
