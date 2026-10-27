//! `fsr lint`: ESLint's rules over an application's source, for development and CI.

use std::path::Path;

use snapfire_lint::{Config, ConfigError, Linter, Plugins, Report, RuleInfo};

use crate::xwpm::Layout;

/// Lints every script under `app` with FSR's base levels and then those of each `fsr.lint.toml` from the
/// repository down to the project. The templates of `.vue` and `.svelte` files go through `snapfirec-vue` and
/// `snapfirec-svelte`. The vendored packages and the fetched declarations are not the application's and are skipped.
pub fn run(app: &Path) -> Result<Report, ConfigError> {
  Ok(linter(app)?.lint_dir(app))
}

fn linter(app: &Path) -> Result<Linter, ConfigError> {
  let mut config = Config::discover(&crate::serve::project_root(app))?.over(snapfire_fsr_lint::base());
  if let Ok(layout) = Layout::of(app) {
    for dir in [layout.vendor, layout.types] {
      config.ignore(&format!("{}/**", dir.trim_end_matches('/')))?;
    }
  }
  Ok(Linter::new_with(config, snapfire_fsr_lint::checks())?.with_compilers(Box::new(Plugins::default())))
}

/// Applies every safe fix under `app`, writing each file it changes, then lints again: the report is what
/// is left, with how many fixes were applied.
pub fn fix(app: &Path) -> Result<(Report, usize), ConfigError> {
  let linter = linter(app)?;
  let mut applied = 0;
  for (path, fixed) in linter.fix_dir(app) {
    if std::fs::write(&path, &fixed.text).is_ok() {
      applied += fixed.applied;
    }
  }
  Ok((linter.lint_dir(app), applied))
}

/// Every rule `fsr lint` runs, FSR's own among them, with its default level.
pub fn rules() -> Vec<RuleInfo> {
  Linter::new_with(snapfire_fsr_lint::base(), snapfire_fsr_lint::checks()).map(|linter| linter.rules()).unwrap_or_default()
}
