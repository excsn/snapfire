use std::fmt;
use std::str::FromStr;

/// Source path plus export name, `components/ServerChart.tsx#default`.
/// The content hash lives in the build manifest, never here.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId {
  pub path: String,
  pub export: String,
}

impl ModuleId {
  pub fn new(path: impl Into<String>, export: impl Into<String>) -> Self {
    Self { path: path.into(), export: export.into() }
  }
}

impl fmt::Display for ModuleId {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}#{}", self.path, self.export)
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseModuleIdError;

impl fmt::Display for ParseModuleIdError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str("module id must be `path#export` with a non-empty path and export")
  }
}

impl std::error::Error for ParseModuleIdError {}

impl FromStr for ModuleId {
  type Err = ParseModuleIdError;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let (path, export) = s.rsplit_once('#').ok_or(ParseModuleIdError)?;
    if path.is_empty() || export.is_empty() {
      return Err(ParseModuleIdError);
    }
    Ok(Self::new(path, export))
  }
}
