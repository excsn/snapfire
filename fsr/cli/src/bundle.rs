//! `fsr bundle`: the deploy tree. Everything the host reads that the build
//! produced, laid out as a server wants it, so a deployment copies a directory
//! instead of restating what the application serves.

use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;
use snapfire_fsr_sites::layout::{self, Source};

use crate::BuildError;
use crate::serve::project_root;

/// The `serve/` prefix, the one directory a web server is pointed at. Nothing
/// outside it is reachable from the network.
pub const SERVE: &str = layout::SERVE;

pub struct Bundled {
  pub out: PathBuf,
  /// Route and the directory under `serve/` that answers it, for every static
  /// root.
  pub served: Vec<(String, String)>,
  /// Everything under the tree the host reads, in the order it is laid out.
  pub read: Vec<String>,
  /// How many files the tree holds and how many bytes they come to.
  pub files: usize,
  pub bytes: u64,
  /// What the bundle does not hold and a deployment still places beside it.
  pub beside: Vec<&'static str>,
}

/// Writes the deploy tree for `app` under `out`: configuration under
/// `config/`, everything the application reads under `app/`, every static root
/// under `serve/<route>/` for a web server to point at, and a generated
/// `config/bundle.toml` naming the paths that moved.
///
/// Every destination is derived from what a file is rather than from where it
/// sat in the project, so nothing the configuration says can write outside
/// `out`.
pub fn run(app: &Path, out: &Path) -> Result<Bundled, BuildError> {
  run_checked(app, out, true)
}

/// `run` with the check made optional. A bundle is a thing about to be
/// shipped, so the checks run before anything is written and a finding stops
/// it; `check` is false only for a caller that means to bundle anyway.
pub fn run_checked(app: &Path, out: &Path, check: bool) -> Result<Bundled, BuildError> {
  let root = project_root(app);
  let config = Config::load(&root).map_err(|e| BuildError::Bundle(e.to_string()))?;

  if check {
    let report = crate::doctor::run(&root).map_err(|e| BuildError::Bundle(e.to_string()))?;
    if !report.is_clean() {
      return Err(BuildError::Doctor(report));
    }
  }

  let laid = layout::layout(&root, &config).map_err(|e| BuildError::Bundle(e.to_string()))?;
  let rows = laid.rows().map_err(|e| BuildError::Bundle(e.to_string()))?;

  if out.exists() {
    std::fs::remove_dir_all(out).map_err(|e| BuildError::Io(out.to_path_buf(), e))?;
  }

  let mut bytes = 0;
  for row in &rows {
    let to = out.join(&row.path);
    match &row.from {
      Source::Path(from) => {
        bytes += std::fs::metadata(from).map(|m| m.len()).unwrap_or(0);
        copy_file(from, &to)?;
      }
      Source::Text(text) => {
        bytes += text.len() as u64;
        write_file(text.as_bytes(), &to)?;
      }
    }
  }

  let client = snapfire_fsr_host::client::ROUTE;
  let mut served: Vec<(String, String)> = config
    .statics
    .iter()
    .map(|root| {
      let route = root.route.trim_matches('/');
      let at = if route.is_empty() {
        SERVE.to_owned()
      } else {
        format!("{SERVE}/{route}")
      };
      (root.route.clone(), at)
    })
    .collect();
  if !served.iter().any(|(route, _)| route.trim_end_matches('/') == client) {
    served.push((client.to_owned(), format!("{SERVE}{client}")));
  }

  Ok(Bundled {
    out: out.to_path_buf(),
    served,
    read: laid.places.iter().filter(|p| p.exists()).map(|p| p.to.clone()).collect(),
    files: rows.len(),
    bytes,
    beside: vec!["the binary", "the logging configuration"],
  })
}

fn write_file(bytes: &[u8], to: &Path) -> Result<(), BuildError> {
  if let Some(parent) = to.parent() {
    std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
  }
  std::fs::write(to, bytes).map_err(|e| BuildError::Io(to.to_path_buf(), e))
}

fn copy_file(from: &Path, to: &Path) -> Result<(), BuildError> {
  if let Some(parent) = to.parent() {
    std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
  }
  std::fs::copy(from, to).map_err(|e| BuildError::Io(to.to_path_buf(), e))?;
  Ok(())
}
