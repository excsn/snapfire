//! Typechecking for a TypeScript project, as a peer process rather than a
//! library: [`resolve`] puts a `tsc` of the requested version on disk and
//! [`check`] runs it over a tsconfig and returns its diagnostics with
//! TypeScript's own codes intact.
//!
//! Every resolution is against a version the caller asked for. A compiler
//! found on `PATH` is used only once it has said what it is, and a fetched
//! one is verified against a hash before it runs.

mod acquire;
mod diagnose;

use std::fmt;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub use acquire::{cache_root, cached, install_dir, is_pinned, platform, resolve, DEFAULT_VERSION, PINNED_PLATFORMS, REGISTRY};
pub use diagnose::{check, parse};

/// What to run and where it may come from.
#[derive(Debug, Clone)]
pub struct Options {
  /// The version every step below resolves against.
  pub version: String,
  /// A compiler to use as given. Its `--version` must be `version`.
  pub tsc: Option<PathBuf>,
  /// The cache root; the platform's own when absent.
  pub cache: Option<PathBuf>,
  pub registry: String,
  /// The `sha512-` integrity a fetched tarball must have, for a version this
  /// crate pins no hash for. The crate's own pin wins when it has one.
  pub expect: Option<String>,
  /// Refuse to fetch, so a machine with no network fails rather than hangs.
  pub offline: bool,
}

impl Default for Options {
  fn default() -> Self {
    Self { version: DEFAULT_VERSION.to_owned(), tsc: None, cache: None, registry: REGISTRY.to_owned(), expect: None, offline: false }
  }
}

/// Which rung of the ladder answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
  Given,
  Cache,
  Path,
  Fetched,
}

impl fmt::Display for Source {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let name = match self {
      Source::Given => "given",
      Source::Cache => "cache",
      Source::Path => "PATH",
      Source::Fetched => "fetched",
    };
    f.write_str(name)
  }
}

/// The compiler a resolution settled on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolved {
  pub tsc: PathBuf,
  pub version: String,
  pub source: Source,
  /// The integrity of the tarball this fetch verified, for the caller to record.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub sha512: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
  Error,
  Warning,
}

impl fmt::Display for Severity {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(match self {
      Severity::Error => "error",
      Severity::Warning => "warning",
    })
  }
}

/// One diagnostic, carrying TypeScript's own code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
  /// Relative to the root the check ran in; absent when the diagnostic is about the project rather than a file.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub file: Option<String>,
  pub line: u32,
  pub column: u32,
  pub code: String,
  pub severity: Severity,
  pub message: String,
}

impl Diagnostic {
  pub fn is_error(&self) -> bool {
    self.severity == Severity::Error
  }
}

impl fmt::Display for Diagnostic {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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

#[derive(Debug, thiserror::Error)]
pub enum Error {
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("{0}: {1}")]
  Http(String, String),
  #[error("{path}: reports `{found}`, and the requested TypeScript is {want}")]
  Mismatch { path: PathBuf, found: String, want: String },
  #[error("{path}: {source}")]
  Spawn { path: PathBuf, source: std::io::Error },
  #[error("no TypeScript is published for {0}; the platform packages are per os-arch, such as darwin-arm64 or linux-x64")]
  Platform(String),
  #[error("{url}: the bytes hash to {found}, not the expected {want}")]
  Integrity { url: String, found: String, want: String },
  #[error("{0}: the registry document carries no integrity and this crate pins no hash for it")]
  NoIntegrity(String),
  #[error("TypeScript {0} is not in the cache and fetching is off")]
  Offline(String),
  #[error("{0}: no `lib/tsc` in the package")]
  NoBinary(PathBuf),
  #[error("no home directory to put the cache in; pass a cache directory or set SNAPFIRE_CACHE")]
  NoHome,
  #[error("{path} exited with {status} and printed no diagnostic\n{output}")]
  Tsc { path: PathBuf, status: String, output: String },
}
