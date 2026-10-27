//! `fsr lint`: ESLint's rules over an application's source, for development and CI.

use std::path::Path;

use snapfire_lint::{Config, ConfigError, Linter, Report};

use crate::xwpm::Layout;

/// Lints every script under `app` with the levels of each `fsr.lint.toml` from the repository down to
/// the project. The vendored packages and the fetched declarations are not the application's and are
/// skipped.
pub fn run(app: &Path) -> Result<Report, ConfigError> {
  let mut config = Config::discover(&crate::serve::project_root(app))?;
  if let Ok(layout) = Layout::of(app) {
    for dir in [layout.vendor, layout.types] {
      config.ignore(&format!("{}/**", dir.trim_end_matches('/')))?;
    }
  }
  Ok(Linter::new(config)?.lint_dir(app))
}
