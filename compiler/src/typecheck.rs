//! The typechecker, which is another executable: `snapfirec` never reads a
//! type for meaning, it spawns `snapfiretc` over the same tsconfig it built
//! with and lets the diagnostics through.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

pub const CHECKER: &str = "snapfiretc";

#[derive(Debug, Default)]
pub struct Options {
  /// A compiler for the checker to use as given.
  pub tsc: Option<PathBuf>,
  /// The TypeScript version to check with; the checker's default when absent.
  pub version: Option<String>,
  /// The checker; `$SNAPFIRETC`, beside this binary, then `PATH` when absent.
  pub checker: Option<PathBuf>,
}

fn find(explicit: Option<&Path>) -> PathBuf {
  match explicit {
    Some(path) => path.to_path_buf(),
    None if std::env::var_os("SNAPFIRETC").is_some_and(|v| !v.is_empty()) => PathBuf::from(std::env::var_os("SNAPFIRETC").unwrap()),
    None => {
      let beside = std::env::current_exe().ok().and_then(|exe| exe.parent().map(|d| d.join(CHECKER)));
      beside.filter(|p| p.is_file()).unwrap_or_else(|| PathBuf::from(CHECKER))
    }
  }
}

/// Runs the checker over `config` in `root` and fails when it reports an error.
pub fn run(root: &Path, config: &Path, options: &Options) -> Result<()> {
  let checker = find(options.checker.as_deref());
  let mut command = Command::new(&checker);
  command.arg("--root").arg(root).arg("--config").arg(config);
  if let Some(tsc) = &options.tsc {
    command.arg("--tsc").arg(tsc);
  }
  if let Some(version) = &options.version {
    command.args(["--tsc-version", version]);
  }
  let status = match command.status() {
    Ok(status) => status,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      bail!("'--typecheck' needs {CHECKER} beside snapfirec or on PATH: cargo install snapfire_typecheck");
    }
    Err(e) => bail!("{}: {e}", checker.display()),
  };
  if !status.success() {
    bail!("Typecheck failed. See the diagnostics above.");
  }
  Ok(())
}
