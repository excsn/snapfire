//! Where an application keeps its vendor tree, import map and declarations.
//! The defaults are fsr's conventions; an `xwpm.wmf` in the app directory
//! names its own and marks the application as one xwpm manages, in which case
//! `fsr add` and `fsr types` delegate to it rather than fetching themselves.
//! The wmf syntax is `/Users/norm/excsn/project_support/xwm/docs/wmf.md`.

use std::path::Path;
use std::process::Command;

use crate::BuildError;

pub const XWPM_FILE: &str = "xwpm.wmf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
  /// Directory the runtime modules land in, relative to the app.
  pub vendor: String,
  /// URL prefix the vendor directory is served from.
  pub base: String,
  /// The import map file, relative to the app.
  pub importmap: String,
  /// Directory the declarations land in, relative to the app.
  pub types: String,
  /// True when `xwpm.wmf` is present.
  pub xwpm: bool,
}

impl Default for Layout {
  fn default() -> Self {
    Self { vendor: "vendor".to_owned(), base: "/static/js/vendor".to_owned(), importmap: "importmap.json".to_owned(), types: "types".to_owned(), xwpm: false }
  }
}

impl Layout {
  pub fn of(app: &Path) -> Result<Self, BuildError> {
    let path = app.join(XWPM_FILE);
    if !path.is_file() {
      return Ok(Self::default());
    }
    let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
    Self::from_wmf(&text).map_err(|why| BuildError::Manifest(path, why))
  }

  /// The root records of an `xwpm.wmf`; sections are skipped.
  pub fn from_wmf(text: &str) -> Result<Self, String> {
    let mut layout = Self { xwpm: true, ..Self::default() };
    for (i, raw) in text.lines().enumerate() {
      let line = raw.trim();
      if line.is_empty() || line.starts_with('#') {
        continue;
      }
      if line.starts_with('[') {
        break;
      }
      let Some((key, value)) = line.split_once('=') else {
        return Err(format!("line {}: not `key = value`: {line}", i + 1));
      };
      let value = value.trim().to_owned();
      match key.trim() {
        "vendor" => layout.vendor = value,
        "base" => layout.base = value,
        "importmap" => layout.importmap = value,
        "types" => layout.types = value,
        _ => {}
      }
    }
    Ok(layout)
  }
}

/// Runs `xwpm <args>` in the app directory, failing when the binary is absent or the command does.
pub fn run(app: &Path, args: &[&str]) -> Result<(), BuildError> {
  let status = Command::new("xwpm")
    .args(args)
    .current_dir(app)
    .status()
    .map_err(|e| BuildError::Xwpm(format!("xwpm {}: {e}; `xwpm.wmf` names this application as one xwpm manages, so xwpm must be on PATH", args.join(" "))))?;
  if !status.success() {
    return Err(BuildError::Xwpm(format!("xwpm {} exited with {status}", args.join(" "))));
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_root_records_name_the_layout_and_sections_are_skipped() {
    let wmf = "# app\nvendor    = public/js/vendor\nbase      = /js/vendor\nimportmap = public/js/importmap.json\ntypes     = types\nregistry  = npm\n\n[modules]\nnormansoven/editor = cove:0.1.0\n";
    let layout = Layout::from_wmf(wmf).unwrap();
    assert_eq!(layout.vendor, "public/js/vendor");
    assert_eq!(layout.base, "/js/vendor");
    assert_eq!(layout.importmap, "public/js/importmap.json");
    assert_eq!(layout.types, "types");
    assert!(layout.xwpm);
    assert!(Layout::from_wmf("vendor\n").is_err());
    assert_eq!(Layout::default().types, "types");
  }
}
