//! `fsr build` checks the host features an application calls for against the binary that will serve it: the stock list for an application with no crate, which `fsr serve` and `fsr-server` both carry, and the features cargo resolves for the host when it builds the application's own crate.

use std::path::Path;
use std::process::Command;

use snapfire_fsr_host::features::{needs, refusal, Need, STOCK};

use crate::{BuildError, Built};

/// The features `app` calls for that its binary lacks: an error for what only the feature answers, and a line to print for what a crate can answer in code instead. Nothing when the configuration does not load, which the build refuses on its own.
pub fn check(app: &Path, built: &Built) -> Result<Vec<String>, BuildError> {
  check_with(app, !built.manifest.images.is_empty())
}

/// [`check`] given whether the plan draws a card image.
pub fn check_with(app: &Path, draws_cards: bool) -> Result<Vec<String>, BuildError> {
  let Some(config) = crate::config_beside(app) else { return Ok(Vec::new()) };
  let needs = needs(&config, draws_cards).map_err(|e| BuildError::Features(e.to_string()))?;
  let root = crate::serve::project_root(app);
  if !root.join("Cargo.toml").is_file() {
    let missing: Vec<&Need> = needs.iter().filter(|need| !STOCK.contains(&need.feature)).collect();
    return match missing.is_empty() {
      true => Ok(Vec::new()),
      false => Err(BuildError::Features(format!("{}\nfsr serve and fsr-server carry the host's `stock` features; an application that needs more serves itself from a crate of its own", refusal(&missing)))),
    };
  }
  let held = match resolved_host_features(&root) {
    Ok(held) => held,
    Err(why) => return Ok(vec![format!("features  not checked: {why}")]),
  };
  let missing: Vec<&Need> = needs.iter().filter(|need| !held.iter().any(|feature| feature == need.feature)).collect();
  let (in_code, certain): (Vec<&Need>, Vec<&Need>) = missing.into_iter().partition(|need| need.instead.is_some());
  if !certain.is_empty() {
    return Err(BuildError::Features(format!("{}\nturn them on for snapfire_fsr_host in {}", refusal(&certain), root.join("Cargo.toml").display())));
  }
  Ok(in_code.iter().map(|need| format!("features  the host lacks {need}; {}", need.instead.map(|instead| instead.describe()).unwrap_or_default())).collect())
}

/// The features cargo resolves for `snapfire_fsr_host` when it builds the crate at `root` alone, as `cargo build -p` does. A workspace's other members do not count, since their features are not this crate's.
fn resolved_host_features(root: &Path) -> Result<Vec<String>, String> {
  let manifest = root.join("Cargo.toml");
  let text = std::fs::read_to_string(&manifest).map_err(|e| format!("{}: {e}", manifest.display()))?;
  let table: toml::Table = text.parse().map_err(|e| format!("{}: {e}", manifest.display()))?;
  let Some(package) = table.get("package").and_then(|package| package.get("name")).and_then(|name| name.as_str()) else {
    return Err(format!("{} names no package", manifest.display()));
  };
  let out = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
    .args(["tree", "-p", package, "-e", "normal", "-i", "snapfire_fsr_host", "-f", "{f}", "--prefix", "none", "--depth", "0"])
    .current_dir(root)
    .output()
    .map_err(|e| format!("cargo tree: {e}"))?;
  if !out.status.success() {
    return Err(format!("cargo tree: {}", String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("failed")));
  }
  let mut features: Vec<String> = String::from_utf8_lossy(&out.stdout).lines().flat_map(|line| line.split(',')).map(str::trim).filter(|feature| !feature.is_empty()).map(str::to_owned).collect();
  features.sort();
  features.dedup();
  Ok(features)
}
