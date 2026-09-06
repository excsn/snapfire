//! `fsr bundle`: the deploy tree. Everything the host reads that the build
//! produced, laid out as a server wants it, so a deployment copies a directory
//! instead of restating what the application serves.

use std::path::{Path, PathBuf};

use snapfire_fsr_host::config::Config;

use crate::BuildError;
use crate::serve::project_root;

/// The `serve/` prefix, the one directory a web server is pointed at. Nothing
/// outside it is reachable from the network.
pub const SERVE: &str = "serve";

pub struct Bundled {
  pub out: PathBuf,
  /// Route and the directory it came from, for every static root.
  pub served: Vec<(String, PathBuf)>,
  /// The plan and contract files under `app/`.
  pub read: Vec<PathBuf>,
  /// What the bundle does not hold and a deployment still places beside it.
  pub beside: Vec<&'static str>,
}

/// Writes the deploy tree for `app` under `out`: every static root under
/// `serve/<route>/`, and the plan and contracts under `app/` where the host's
/// own configuration already looks for them.
pub fn run(app: &Path, out: &Path) -> Result<Bundled, BuildError> {
  let root = project_root(app);
  let config = Config::load(&root).map_err(|e| BuildError::Bundle(e.to_string()))?;

  if out.exists() {
    std::fs::remove_dir_all(out).map_err(|e| BuildError::Io(out.to_path_buf(), e))?;
  }

  let mut served = Vec::new();
  for root in &config.statics {
    let from = config.resolve(&root.dir);
    if !from.is_dir() {
      continue;
    }
    let to = out.join(SERVE).join(root.route.trim_start_matches('/'));
    copy_dir(&from, &to)?;
    served.push((root.route.clone(), from));
  }

  let app_out = out.join(config.app.file_name().unwrap_or("app".as_ref()));
  let mut read = Vec::new();
  let plan = config.resolve(&config.server.plan);
  if plan.is_file() {
    let to = app_out.join(&config.server.plan);
    copy_file(&plan, &to)?;
    read.push(to);
  }
  let contracts = config.resolve(&config.server.contracts);
  if contracts.is_dir() {
    let to = app_out.join(&config.server.contracts);
    copy_dir(&contracts, &to)?;
    read.push(to);
  }
  if let Some(prerender) = &config.server.prerender {
    let from = config.resolve(prerender);
    if from.is_dir() {
      let to = app_out.join(prerender);
      copy_dir(&from, &to)?;
      read.push(to);
    }
  }

  Ok(Bundled { out: out.to_path_buf(), served, read, beside: vec!["the binary", "config/", "the logging configuration"] })
}

fn copy_file(from: &Path, to: &Path) -> Result<(), BuildError> {
  if let Some(parent) = to.parent() {
    std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
  }
  std::fs::copy(from, to).map_err(|e| BuildError::Io(to.to_path_buf(), e))?;
  Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), BuildError> {
  std::fs::create_dir_all(to).map_err(|e| BuildError::Io(to.to_path_buf(), e))?;
  for entry in std::fs::read_dir(from).map_err(|e| BuildError::Io(from.to_path_buf(), e))?.flatten() {
    let path = entry.path();
    let target = to.join(entry.file_name());
    if path.is_dir() {
      copy_dir(&path, &target)?;
    } else {
      copy_file(&path, &target)?;
    }
  }
  Ok(())
}
