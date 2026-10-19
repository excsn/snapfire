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

pub mod driven;
pub mod host;

/// The wire version. A plugin announcing another number is refused by name
/// rather than failing later on a field that moved.
///
/// Protocol 2 adds a unit's sibling files and the `needs` answer that asks for
/// them. Protocol 3 adds the `describe` request and its `described` answer.
/// Protocol 4 adds `Options::compiler`, the application's own compiler, and
/// the `.svelte.ts` and `.svelte.js` suffixes a plugin claims. A plugin
/// speaking 3 would ignore the compiler it was handed and compile with the one
/// it carries, so a host refuses it rather than mix two versions of a
/// framework.
///
/// Raise this only for a change to a protocol a published release of this
/// crate already speaks. A change to the protocol since the last publish is
/// part of the protocol this number already names: add it to that number's
/// line above. Check the number in the last published release before raising
/// it.
pub const PROTOCOL: u32 = 4;

/// The extensions a plugin compiles, one framework each. `<ext>` is compiled
/// by `snapfirec-<ext>`, which `cargo install snapfire_<ext>` puts on PATH.
pub const EXTENSIONS: &[&str] = &["vue", "svelte"];

/// The extension as a static name when a plugin compiles it.
pub fn claimed(ext: &str) -> Option<&'static str> {
  EXTENSIONS.iter().find(|known| **known == ext).copied()
}

/// File name suffixes a plugin claims ahead of the last extension, with the
/// extension whose plugin compiles them: a Svelte 5 runes module is
/// `name.svelte.ts` or `name.svelte.js`.
pub const SUFFIXES: &[(&str, &str)] = &[(".svelte.ts", "svelte"), (".svelte.js", "svelte")];

/// Whether the host strips a file's types before sending it: a claimed suffix
/// ending in `.ts` reaches the plugin as JavaScript, since Svelte's
/// `compileModule` reads no TypeScript.
pub fn stripped_first(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  SUFFIXES.iter().any(|(suffix, _)| suffix.ends_with(".ts") && lower.ends_with(suffix))
}

/// The plugin a file name asks for: a suffix in [`SUFFIXES`] first, then its last extension.
pub fn claimed_name(name: &str) -> Option<&'static str> {
  let lower = name.to_ascii_lowercase();
  if let Some((_, ext)) = SUFFIXES.iter().find(|(suffix, _)| lower.ends_with(suffix)) {
    return Some(ext);
  }
  let (_, ext) = lower.rsplit_once('.')?;
  claimed(ext)
}

/// The binary an extension asks for.
pub fn binary_for(ext: &str) -> String {
  format!("snapfirec-{ext}")
}

/// What a reader types when the binary is not there. A plugin is a crate, the
/// way every other tool in this project is.
pub fn install_hint(ext: &str) -> String {
  format!("cargo install snapfire_{ext}")
}

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
  /// What the host wants of each unit. Absent, a compile.
  #[serde(default, skip_serializing_if = "Kind::is_compile")]
  pub kind: Kind,
  pub units: Vec<Unit>,
}

/// What a request asks of a plugin.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
  /// The unit as a module for the browser: [`Outcome::Ok`].
  #[default]
  Compile,
  /// The unit as the framework's own parser reads it, for a host that lowers
  /// the component rather than shipping it: [`Outcome::Described`].
  Describe,
}

impl Kind {
  fn is_compile(&self) -> bool {
    *self == Kind::Compile
  }
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
  /// A framework compiler file the plugin boots in place of the one it
  /// carries: the application's own, so what it compiles calls the runtime
  /// the application vendors.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub compiler: Option<String>,
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
  /// The answer to a [`Kind::Describe`] request.
  Described(Described),
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

/// A component as its framework's parser reads it, before anything is
/// compiled: what a host needs to lower the component to markup of its own
/// instead of mounting it empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Described {
  /// The template as a tree in the plugin's own shape. The wire carries it
  /// as JSON and the host's front end for that framework reads it; the
  /// plugin's documentation says what the nodes are.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub template: Option<serde_json::Value>,
  /// The component's script, when it has one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub script: Option<Script>,
  /// Per name the script binds, how the template reads it, in the
  /// framework's own words.
  #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
  pub bindings: std::collections::BTreeMap<String, String>,
  /// The attribute a scoped style selects on, when any style is scoped.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub scope: Option<String>,
  /// Whether a scoped style has a `:slotted()` rule, so the framework stamps the content its slots render.
  #[serde(default)]
  pub slotted: bool,
  /// Specifiers the plugin found that the source does not contain.
  #[serde(default)]
  pub deps: Vec<String>,
  #[serde(default)]
  pub diagnostics: Vec<Diagnostic>,
}

/// A script block as written, with where it starts in the file so a
/// diagnostic against it names the right line.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Script {
  pub content: String,
  #[serde(default)]
  pub lang: Lang,
  /// The one-based line the content starts on.
  pub line: u32,
  /// The one-based column the content starts at on that line.
  pub column: u32,
  /// Whether the block is the framework's setup form rather than a plain script.
  #[serde(default)]
  pub setup: bool,
  /// Whether a plain script stands beside the setup block.
  #[serde(default)]
  pub plain: bool,
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

  pub fn warning(message: impl Into<String>) -> Self {
    Self { severity: Severity::Warning, message: message.into(), file: None, line: None, column: None }
  }

  pub fn at(mut self, file: impl Into<String>, line: Option<u32>, column: Option<u32>) -> Self {
    self.file = Some(file.into());
    self.line = line;
    self.column = column;
    self
  }
}
