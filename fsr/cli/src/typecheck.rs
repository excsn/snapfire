//! The typechecker as a peer process: `fsr` spawns `snapfiretc` over the
//! `tsconfig.json` the build wrote and renders what it says in its own
//! report. The checker owns which TypeScript runs and where it comes from;
//! `[typecheck]` beside the app says which version and, once one has been
//! resolved, `fsr` records it there so every later build asks for the same.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use serde::Deserialize;

use crate::BuildError;

/// The checker binary, found the way `snapfirec` is.
pub const CHECKER: &str = "snapfiretc";

#[derive(Debug, Clone, Default)]
pub struct Typecheck {
  pub enabled: bool,
  /// The checker; `$SNAPFIRETC`, beside this binary, then `PATH` when absent.
  pub checker: Option<PathBuf>,
  /// A compiler for the checker to use as given, rather than resolving one.
  pub tsc: Option<PathBuf>,
  /// The TypeScript version to check with; the checker's default when absent.
  pub version: Option<String>,
  /// The integrity a fetched compiler must have, for a version the checker pins no hash for.
  pub expect: Option<String>,
  /// The configuration file a resolved version is recorded in, when the project has one.
  pub record: Option<PathBuf>,
}

impl Typecheck {
  /// The `[typecheck]` section of the configuration beside `app`. A project
  /// with no configuration checks with the checker's default version and
  /// records nothing.
  pub fn beside(app: &Path) -> Self {
    let mut options = Self { enabled: true, ..Self::default() };
    let root = crate::serve::project_root(app);
    let Ok(config) = snapfire_fsr_host::config::Config::load(&root) else { return options };
    options.record = config.sources.first().filter(|p| p.extension().is_some_and(|x| x == "toml")).cloned();
    if let Some(section) = &config.typecheck {
      options.enabled = section.enabled.unwrap_or(true);
      options.version = section.version.clone();
      options.expect = section.sha512.clone();
      options.tsc = section.tsc.as_ref().map(PathBuf::from);
    }
    options
  }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Diagnostic {
  #[serde(default)]
  pub file: Option<String>,
  pub line: u32,
  pub column: u32,
  pub code: String,
  pub severity: String,
  pub message: String,
}

impl std::fmt::Display for Diagnostic {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    if let Some(file) = &self.file {
      write!(f, "{file}({},{}): ", self.line, self.column)?;
    }
    write!(f, "{} ", self.severity)?;
    if !self.code.is_empty() {
      write!(f, "{}: ", self.code)?;
    }
    f.write_str(&self.message)
  }
}

impl Diagnostic {
  pub fn is_error(&self) -> bool {
    self.severity == "error"
  }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Checked {
  pub version: String,
  pub source: String,
  #[serde(default)]
  pub sha512: Option<String>,
  /// Whether the checker's own source carries the hash for this version, so an integrity is worth recording.
  #[serde(default)]
  pub pinned: bool,
  pub diagnostics: Vec<Diagnostic>,
  /// The configuration file the version was recorded in, when this run recorded it.
  #[serde(skip)]
  pub recorded: Option<PathBuf>,
}

impl Checked {
  pub fn errors(&self) -> usize {
    self.diagnostics.iter().filter(|d| d.is_error()).count()
  }

  /// The report row: what ran, from where and what it found.
  pub fn row(&self) -> String {
    let found = match (self.errors(), self.diagnostics.len()) {
      (0, 0) => "clean".to_owned(),
      (0, n) => plural(n, "warning"),
      (e, n) if e == n => plural(e, "error"),
      (e, n) => format!("{}, {}", plural(e, "error"), plural(n - e, "warning")),
    };
    format!("tsc {} from {}, {found}", self.version, self.source)
  }
}

fn plural(count: usize, word: &str) -> String {
  if count == 1 { format!("1 {word}") } else { format!("{count} {word}s") }
}

/// The checker: as given, else `$SNAPFIRETC`, else beside this binary, else on `PATH`.
pub fn find_checker(explicit: Option<&Path>) -> PathBuf {
  match explicit {
    Some(path) => path.to_path_buf(),
    None if std::env::var_os("SNAPFIRETC").is_some_and(|v| !v.is_empty()) => PathBuf::from(std::env::var_os("SNAPFIRETC").unwrap()),
    None => {
      let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join(CHECKER)));
      beside.filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(CHECKER))
    }
  }
}

/// Starts the checker over the app's `tsconfig.json`, so it runs while the
/// bundle compiles. `None` when typechecking is off, the tsconfig has not
/// been written yet or no checker is installed.
pub fn spawn(app: &Path, options: &Typecheck) -> Result<Option<Child>, BuildError> {
  if !options.enabled || !app.join("tsconfig.json").is_file() {
    return Ok(None);
  }
  let checker = find_checker(options.checker.as_deref());
  let mut command = Command::new(&checker);
  command.arg("--root").arg(app).args(["--config", "tsconfig.json", "--format", "json"]);
  if let Some(tsc) = &options.tsc {
    command.arg("--tsc").arg(tsc);
  }
  if let Some(version) = &options.version {
    command.args(["--tsc-version", version]);
  }
  if let Some(expect) = &options.expect {
    command.args(["--expect", expect]);
  }
  match command.stdout(Stdio::piped()).spawn() {
    Ok(child) => Ok(Some(child)),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(e) => Err(BuildError::Typecheck(format!("{}: {e}", checker.display()))),
  }
}

/// Waits for a spawned checker and reads its report, recording the version it
/// resolved when the configuration names none.
pub fn finish(child: Option<Child>, options: &Typecheck) -> Result<Option<Checked>, BuildError> {
  let Some(child) = child else { return Ok(None) };
  let output = child.wait_with_output().map_err(|e| BuildError::Typecheck(e.to_string()))?;
  let text = String::from_utf8_lossy(&output.stdout);
  if text.trim().is_empty() {
    return Err(BuildError::Typecheck(format!("exited with {} and printed nothing", output.status)));
  }
  let mut checked: Checked = serde_json::from_str(text.trim()).map_err(|e| BuildError::Typecheck(format!("{e}: {}", text.trim())))?;
  if options.version.is_none() {
    if let Some(path) = &options.record {
      let sha512 = checked.sha512.clone().filter(|_| !checked.pinned);
      if record(path, &checked.version, sha512.as_deref())? {
        checked.recorded = Some(path.clone());
      }
    }
  }
  Ok(Some(checked))
}

/// Both halves at once, for a caller with nothing to do meanwhile.
pub fn run(app: &Path, options: &Typecheck) -> Result<Option<Checked>, BuildError> {
  finish(spawn(app, options)?, options)
}

/// Writes `version` into the file's `[typecheck]` section, adding the section
/// when it has none. `false` when the section already names a version, so a
/// pin a person wrote is never rewritten.
pub fn record(path: &Path, version: &str, sha512: Option<&str>) -> Result<bool, BuildError> {
  let text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  let Some(updated) = with_version(&text, version, sha512) else { return Ok(false) };
  std::fs::write(path, updated).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
  Ok(true)
}

/// The file's text with the version in its `[typecheck]` section, or `None` when it already names one.
fn with_version(text: &str, version: &str, sha512: Option<&str>) -> Option<String> {
  let mut keys = vec![format!("version = \"{version}\"")];
  if let Some(sha512) = sha512 {
    keys.push(format!("sha512 = \"{sha512}\""));
  }
  let Some(header) = text.lines().position(|line| line.trim() == "[typecheck]") else {
    let separator = if text.is_empty() || text.ends_with("\n\n") { "" } else if text.ends_with('\n') { "\n" } else { "\n\n" };
    return Some(format!("{text}{separator}[typecheck]\n{}\n", keys.join("\n")));
  };
  let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
  for line in lines.iter().skip(header + 1) {
    let trimmed = line.trim();
    if trimmed.starts_with('[') {
      break;
    }
    if trimmed.starts_with("version") && trimmed.split('=').next().is_some_and(|k| k.trim() == "version") {
      return None;
    }
  }
  for (offset, key) in keys.into_iter().enumerate() {
    lines.insert(header + 1 + offset, key);
  }
  Some(lines.join("\n") + "\n")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_section_is_added_when_the_file_has_none() {
    let text = "[server]\nlisten = \"127.0.0.1:8080\"\n";
    let updated = with_version(text, "7.0.2", None).unwrap();
    assert!(updated.starts_with(text), "{updated}");
    assert!(updated.ends_with("[typecheck]\nversion = \"7.0.2\"\n"), "{updated}");
  }

  #[test]
  fn a_version_lands_in_the_section_that_exists_and_an_integrity_joins_it() {
    let text = "[typecheck]\nenabled = true\n\n[server]\nlisten = \"x\"\n";
    let updated = with_version(text, "7.1.0", Some("sha512-abc")).unwrap();
    assert_eq!(updated, "[typecheck]\nversion = \"7.1.0\"\nsha512 = \"sha512-abc\"\nenabled = true\n\n[server]\nlisten = \"x\"\n");
  }

  #[test]
  fn a_version_a_person_wrote_is_never_rewritten() {
    let text = "[typecheck]\nversion = \"7.0.2\"\n";
    assert_eq!(with_version(text, "7.1.0", None), None);
    let other = "[server]\nversion = \"9\"\n";
    assert!(with_version(other, "7.0.2", None).is_some());
  }

  #[test]
  fn the_row_counts_what_the_checker_found() {
    let clean = Checked { version: "7.0.2".to_owned(), source: "cache".to_owned(), sha512: None, pinned: true, diagnostics: Vec::new(), recorded: None };
    assert_eq!(clean.row(), "tsc 7.0.2 from cache, clean");
    let error = Diagnostic { file: Some("routes/page.tsx".to_owned()), line: 1, column: 15, code: "TS2305".to_owned(), severity: "error".to_owned(), message: "no".to_owned() };
    let warning = Diagnostic { severity: "warning".to_owned(), ..error.clone() };
    let found = Checked { diagnostics: vec![error.clone(), warning], ..clean.clone() };
    assert_eq!(found.errors(), 1);
    assert_eq!(found.row(), "tsc 7.0.2 from cache, 1 error, 1 warning");
    assert_eq!(error.to_string(), "routes/page.tsx(1,15): error TS2305: no");
  }
}
