//! What `snapfirec` and a framework compiler say to each other.
//!
//! One JSON object per line over the plugin's stdin and stdout. The plugin is
//! spawned once per build and lives across rebuilds, so the cost of starting
//! whatever it carries is paid once rather than per file.
//!
//! Only values cross: no handles, no callbacks. That is easy over a pipe and
//! awkward across WASM linear memory, and designing for the harder one keeps
//! both open.

use serde::{Deserialize, Serialize};

/// The wire version. A plugin announcing another number is refused by name
/// rather than failing later on a field that moved.
pub const PROTOCOL: u32 = 2;

/// The first line a plugin writes, unprompted, once it is ready for work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
  pub protocol: u32,
  /// What the plugin is, for the report and for a diagnostic's prefix.
  pub name: String,
  /// The plugin's own version, which is part of a cache key.
  pub version: String,
  /// What it compiles with, for a bug report; free text.
  pub compiler: String,
  /// The extensions it claims, each with the leading dot.
  pub extensions: Vec<String>,
}

/// A batch. Batching exists from the first version because both a subprocess
/// and a WASM module have a startup to amortise.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
  pub id: u64,
  pub units: Vec<Unit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
  /// The path as a diagnostic should name it, relative to the project root.
  pub filename: String,
  /// Where it is on disk, for a plugin that must resolve a sibling.
  pub path: String,
  pub source: String,
  pub options: Options,
  /// Sibling files the plugin asked for with [`Outcome::Needs`], by the
  /// specifier it asked with. The host reads them; a plugin never opens a
  /// file itself.
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub files: std::collections::BTreeMap<String, String>,
}

/// What the build asked for, in terms every plugin can honour.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Options {
  /// Emit for production: no development-only branches, no warnings kept.
  #[serde(default)]
  pub production: bool,
  /// Emit a source map beside the code.
  #[serde(default)]
  pub source_map: bool,
  /// Minify the emitted code and styles.
  #[serde(default)]
  pub minify: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
  pub id: u64,
  /// One per unit, in the order they were asked for.
  pub results: Vec<Outcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Outcome {
  Ok(Compiled),
  /// The unit did not compile. `diagnostics` says why and is never empty.
  Failed { diagnostics: Vec<Diagnostic> },
  /// The unit names files beside it, a `<style src>` among them, that the
  /// plugin must read to compile. The host answers by sending the unit again
  /// with them in `files`; a unit that asks twice is a failure.
  Needs { files: Vec<String> },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Compiled {
  /// The module the unit became. Its imports are the build's to resolve.
  pub js: String,
  /// What dialect `js` is written in. A plugin whose framework allows typed
  /// sources hands the types straight through rather than stripping them
  /// itself, since the build already has a TypeScript front end and a second
  /// one would be a second answer to the same question.
  #[serde(default)]
  pub lang: Lang,
  /// The component's styles, already scoped by the plugin if the source asked
  /// for it. One string, whatever the source's block count.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub css: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub source_map: Option<String>,
  /// Specifiers the plugin found while compiling that the source does not
  /// contain, which the build could not have seen. The load-bearing field:
  /// these become edges in the module graph.
  #[serde(default)]
  pub deps: Vec<String>,
  /// Anything worth saying that did not stop the compile.
  #[serde(default)]
  pub diagnostics: Vec<Diagnostic>,
}

/// Structured rather than formatted, so build output does not read like three
/// tools stapled together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
  pub severity: Severity,
  pub message: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub file: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub line: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub column: Option<u32>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
  #[default]
  Js,
  Ts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Error,
  Warning,
}

impl Diagnostic {
  pub fn error(message: impl Into<String>) -> Self {
    Self { severity: Severity::Error, message: message.into(), file: None, line: None, column: None }
  }

  pub fn at(mut self, file: impl Into<String>, line: Option<u32>, column: Option<u32>) -> Self {
    self.file = Some(file.into());
    self.line = line;
    self.column = column;
    self
  }
}
