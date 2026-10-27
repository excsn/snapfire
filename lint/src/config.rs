use std::path::{Path, PathBuf};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::{ConfigError, Level};

/// A rule's level, with the ESLint options that follow it when the file gives any.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Setting {
  pub level: Level,
  pub options: Option<serde_json::Value>,
}

/// One table of levels: the file's `[rules]`, or an `[[overrides]]` with the globs it applies to.
#[derive(Debug, Clone)]
pub(crate) struct Layer {
  pub path: PathBuf,
  pub files: Option<GlobSet>,
  pub rules: Vec<(String, Setting)>,
}

/// The levels and options of `fsr.lint.toml`.
#[derive(Debug, Clone)]
pub struct Config {
  inherit: bool,
  layers: Vec<Layer>,
  ignores: Vec<Glob>,
}

impl Default for Config {
  fn default() -> Self {
    Config { inherit: true, layers: Vec::new(), ignores: Vec::new() }
  }
}

impl Config {
  pub fn parse(text: &str, path: &Path) -> Result<Config, ConfigError> {
    let fail = |message: String| ConfigError::Parse { path: path.to_owned(), message };
    let table: toml::Table = text.parse().map_err(|e: toml::de::Error| fail(e.message().to_owned()))?;
    let mut config = Config::default();
    if let Some(other) = table.keys().find(|k| !matches!(k.as_str(), "inherit" | "ignores" | "rules" | "overrides")) {
      return Err(fail(format!("no setting is called `{other}`")));
    }
    if let Some(value) = table.get("inherit") {
      config.inherit = value.as_bool().ok_or_else(|| fail("`inherit` is true or false".to_owned()))?;
    }
    if let Some(value) = table.get("ignores") {
      let globs = value.as_array().ok_or_else(|| fail("`ignores` is a list of globs".to_owned()))?;
      for glob in globs {
        let glob = glob.as_str().ok_or_else(|| fail("a glob in `ignores` is a string".to_owned()))?;
        config.ignores.push(Glob::new(glob).map_err(|e| fail(format!("`{glob}`: {e}")))?);
      }
    }
    if let Some(value) = table.get("rules") {
      config.layers.push(Layer { path: path.to_owned(), files: None, rules: rules(value, path)? });
    }
    if let Some(value) = table.get("overrides") {
      let blocks = value.as_array().ok_or_else(|| fail("`overrides` is a list of `[[overrides]]` tables".to_owned()))?;
      for block in blocks {
        let block = block.as_table().ok_or_else(|| fail("each `[[overrides]]` is a table".to_owned()))?;
        if let Some(other) = block.keys().find(|k| !matches!(k.as_str(), "files" | "rules")) {
          return Err(fail(format!("an `[[overrides]]` has no `{other}`")));
        }
        let files = block.get("files").and_then(toml::Value::as_array).ok_or_else(|| fail("an `[[overrides]]` names its `files` as a list of globs".to_owned()))?;
        let mut globs = GlobSetBuilder::new();
        for glob in files {
          let glob = glob.as_str().ok_or_else(|| fail("a glob in `files` is a string".to_owned()))?;
          globs.add(Glob::new(glob).map_err(|e| fail(format!("`{glob}`: {e}")))?);
        }
        let globs = globs.build().map_err(|e| fail(e.to_string()))?;
        let rules = match block.get("rules") {
          Some(value) => rules(value, path)?,
          None => Vec::new(),
        };
        config.layers.push(Layer { path: path.to_owned(), files: Some(globs), rules });
      }
    }
    Ok(config)
  }

  /// The project's `fsr.lint.toml`, or the defaults when there is none.
  pub fn load(root: &Path) -> Result<Config, ConfigError> {
    let path = root.join(crate::CONFIG_FILE);
    match std::fs::read_to_string(&path) {
      Ok(text) => Config::parse(&text, &path),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
      Err(e) => Err(ConfigError::Parse { path, message: e.to_string() }),
    }
  }

  /// Every `fsr.lint.toml` from `dir` up to the directory holding `.git`, each over the one above it.
  /// Outside a repository only `dir`'s own file is read.
  pub fn discover(dir: &Path) -> Result<Config, ConfigError> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_owned());
    let mut dirs: Vec<&Path> = Vec::new();
    for ancestor in dir.ancestors() {
      dirs.push(ancestor);
      if ancestor.join(".git").exists() {
        break;
      }
    }
    if !dirs.last().is_some_and(|top| top.join(".git").exists()) {
      dirs.truncate(1);
    }
    let mut config = Config::default();
    for dir in dirs.into_iter().rev() {
      config = Config::load(dir)?.over(config);
    }
    Ok(config)
  }

  /// Skip files matching `glob` when walking a directory, relative to the walk's root.
  pub fn ignore(&mut self, glob: &str) -> Result<(), ConfigError> {
    let glob = Glob::new(glob).map_err(|e| ConfigError::Parse { path: PathBuf::from(glob), message: e.to_string() })?;
    self.ignores.push(glob);
    Ok(())
  }

  /// This configuration over `parent`'s, unless it says `inherit = false`.
  pub fn over(self, parent: Config) -> Config {
    match self.inherit {
      false => self,
      true => Config { inherit: parent.inherit, layers: parent.layers.into_iter().chain(self.layers).collect(), ignores: parent.ignores.into_iter().chain(self.ignores).collect() },
    }
  }

  pub(crate) fn layers(&self) -> &[Layer] {
    &self.layers
  }

  pub(crate) fn ignored(&self) -> GlobSet {
    let mut set = GlobSetBuilder::new();
    for glob in &self.ignores {
      set.add(glob.clone());
    }
    set.build().unwrap_or_else(|_| GlobSet::empty())
  }
}

/// The typescript-eslint rules that extend an ESLint rule of the same name, which is the rule run here.
const TYPESCRIPT_EXTENDS: [&str; 18] = [
  "class-methods-use-this",
  "default-param-last",
  "init-declarations",
  "max-params",
  "no-array-constructor",
  "no-dupe-class-members",
  "no-empty-function",
  "no-invalid-this",
  "no-loop-func",
  "no-loss-of-precision",
  "no-magic-numbers",
  "no-redeclare",
  "no-restricted-imports",
  "no-shadow",
  "no-unused-expressions",
  "no-unused-vars",
  "no-use-before-define",
  "no-useless-constructor",
];

/// A rule id as the linter knows it. An ESLint name without a plugin is the `eslint/` rule, and a
/// `@typescript-eslint/` name is the `typescript/` rule, or the `eslint/` rule it extends.
pub(crate) fn full_id(id: &str) -> String {
  if let Some(name) = id.strip_prefix("@typescript-eslint/") {
    return match TYPESCRIPT_EXTENDS.contains(&name) {
      true => format!("eslint/{name}"),
      false => format!("typescript/{name}"),
    };
  }
  match id.contains('/') {
    true => id.to_owned(),
    false => format!("eslint/{id}"),
  }
}

fn rules(value: &toml::Value, path: &Path) -> Result<Vec<(String, Setting)>, ConfigError> {
  let fail = |message: String| ConfigError::Parse { path: path.to_owned(), message };
  let table = value.as_table().ok_or_else(|| fail("`rules` is a table of rule ids".to_owned()))?;
  let mut out = Vec::new();
  for (id, value) in table {
    let (level, options) = match value {
      toml::Value::Array(items) => {
        let (first, rest) = items.split_first().ok_or_else(|| fail(format!("`{id}`: the list starts with a level")))?;
        let options = match rest.is_empty() {
          true => None,
          false => Some(serde_json::to_value(rest).map_err(|e| fail(format!("`{id}`: {e}")))?),
        };
        (level(first).ok_or_else(|| fail(format!("`{id}`: `{first}` is not a level; use off, warn or error")))?, options)
      }
      other => (level(other).ok_or_else(|| fail(format!("`{id}`: `{other}` is not a level; use off, warn or error")))?, None),
    };
    out.push((full_id(id), Setting { level, options }));
  }
  Ok(out)
}

fn level(value: &toml::Value) -> Option<Level> {
  match value {
    toml::Value::String(s) => match s.as_str() {
      "off" => Some(Level::Off),
      "warn" => Some(Level::Warn),
      "error" => Some(Level::Error),
      _ => None,
    },
    toml::Value::Integer(0) => Some(Level::Off),
    toml::Value::Integer(1) => Some(Level::Warn),
    toml::Value::Integer(2) => Some(Level::Error),
    _ => None,
  }
}
