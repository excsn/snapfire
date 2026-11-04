//! The production binary of an application with no crate beside it: `fsr-server` from `snapfire_fsr_sites`, built once into a cache per version and target and copied into each deploy tree.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::BuildError;

/// The file the binary has, in the cache and in a deploy tree.
pub const SERVER_BIN: &str = "fsr-server";

/// A binary to use in place of the cached one.
pub const SERVER_ENV: &str = "FSR_SERVER";

/// The sites crate this `fsr` was built against, when it was built from a checkout.
const CHECKOUT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../sites");

/// Whether `root` holds a crate of its own, whose binary a deployment ships instead.
pub fn has_crate(root: &Path) -> bool {
  root.join("Cargo.toml").is_file()
}

/// The server binary: `$FSR_SERVER`, else the cached build for this version and target, built into the cache when it is missing.
pub fn binary() -> Result<PathBuf, BuildError> {
  if let Some(given) = std::env::var_os(SERVER_ENV) {
    return Ok(PathBuf::from(given));
  }
  let checkout = Path::new(CHECKOUT).join("Cargo.toml").is_file().then(|| PathBuf::from(CHECKOUT));
  let dir = cache_dir(checkout.is_some())?;
  let built = dir.join("bin").join(SERVER_BIN);
  if built.is_file() {
    return Ok(built);
  }
  let mut install = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
  install.args(["install", "--bin", SERVER_BIN, "--features", "server", "--root"]).arg(&dir);
  install.args(["--config", "profile.release.debug=false", "--config", "profile.release.strip=true"]);
  match &checkout {
    Some(path) => install.arg("--locked").arg("--path").arg(path),
    None => install.args(["snapfire_fsr_sites", "--version"]).arg(format!("={}", snapfire_fsr_sites::VERSION)),
  };
  eprintln!("building {SERVER_BIN} {} into {}", snapfire_fsr_sites::VERSION, dir.display());
  let status = install.status().map_err(|e| BuildError::Bundle(format!("cargo install {SERVER_BIN}: {e}")))?;
  if !status.success() || !built.is_file() {
    return Err(BuildError::Bundle(format!("cargo install {SERVER_BIN} {}: {status}", snapfire_fsr_sites::VERSION)));
  }
  Ok(built)
}

/// `<cache>/snapfire/server/<version>/<arch>-<os>`. A build from a checkout is keyed by this `fsr` too, since the checkout's source moves under one version.
fn cache_dir(from_checkout: bool) -> Result<PathBuf, BuildError> {
  let base = std::env::var_os("XDG_CACHE_HOME")
    .map(PathBuf::from)
    .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
    .ok_or_else(|| BuildError::Bundle(format!("no cache directory: set XDG_CACHE_HOME, HOME or {SERVER_ENV}")))?;
  let mut version = snapfire_fsr_sites::VERSION.to_owned();
  if from_checkout {
    let exe = std::env::current_exe().and_then(|exe| exe.metadata()).map_err(|e| BuildError::Bundle(format!("this fsr: {e}")))?;
    let at = exe.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    version = format!("{version}-{:x}-{at:x}", exe.len());
  }
  Ok(base.join("snapfire/server").join(version).join(format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS)))
}
