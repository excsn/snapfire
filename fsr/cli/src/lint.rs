//! `fsr lint`: ESLint's rules over an application's source, for development and CI.

use std::collections::BTreeMap;
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
  let mut checks = snapfire_fsr_lint::checks();
  if let Ok(layout) = Layout::of(app) {
    checks.push(Box::new(snapfire_fsr_lint::ExtraneousDependencies::new(vendored_for(app, &layout))));
    for dir in [&layout.vendor, &layout.types] {
      config.ignore(&format!("{}/**", dir.trim_end_matches('/')))?;
    }
  }
  Ok(Linter::new_with(config, checks)?.with_compilers(Box::new(Plugins::default())))
}

/// Each package the application vendored only because another imports it, with the packages that do: from
/// the vendor manifest's `dependency_of`, or for an xwpm application each import map entry no module in
/// `[modules]` covers. What the site's shell supplies and FSR's own modules are the application's.
fn vendored_for(app: &Path, layout: &Layout) -> BTreeMap<String, Vec<String>> {
  let mut found: BTreeMap<String, Vec<String>> = BTreeMap::new();
  if layout.xwpm {
    let map = crate::vendor::read_import_map(app, layout).unwrap_or_default();
    let scopes = map.get("scopes").and_then(|scopes| scopes.as_object()).into_iter().flat_map(|scopes| scopes.values());
    let keys = map.get("imports").into_iter().chain(scopes).filter_map(|table| table.as_object()).flat_map(|table| table.keys());
    for key in keys {
      let name = key.trim_end_matches('/');
      if !layout.modules.iter().any(|module| under(name, module)) {
        found.entry(name.to_owned()).or_default();
      }
    }
  } else if let Ok(manifest) = crate::vendor::VendorManifest::read(app, layout) {
    for (name, package) in manifest.packages {
      if !package.dependency_of.is_empty() {
        found.insert(name, package.dependency_of);
      }
    }
  }
  let shell: Vec<String> = crate::vendor::shell_beside(app).ok().flatten().map(|contract| contract.imports.into_keys().collect()).unwrap_or_default();
  found.retain(|name, _| !under(name, "@snapfire") && !shell.iter().any(|served| under(name, served.trim_end_matches('/'))));
  found
}

/// Whether `specifier` is `name` or sits under it.
fn under(specifier: &str, name: &str) -> bool {
  specifier.strip_prefix(name).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
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
  let mut checks = snapfire_fsr_lint::checks();
  checks.push(Box::new(snapfire_fsr_lint::ExtraneousDependencies::new(BTreeMap::new())));
  Linter::new_with(snapfire_fsr_lint::base(), checks).map(|linter| linter.rules()).unwrap_or_default()
}
