//! `fsr lint`: ESLint's rules over an application's source, for development and CI.

use std::path::Path;

use snapfire_lint::{Config, ConfigError, Linter, Plugins, Report};

use crate::xwpm::Layout;

/// The attributes of htmx 2 and htmx 4, which a JSX element may carry when htmx drives it.
const HTMX_ATTRIBUTES: &[&str] = &[
  "hx-action",
  "hx-boost",
  "hx-config",
  "hx-confirm",
  "hx-delete",
  "hx-disable",
  "hx-disabled-elt",
  "hx-disinherit",
  "hx-encoding",
  "hx-ext",
  "hx-get",
  "hx-headers",
  "hx-history",
  "hx-history-elt",
  "hx-ignore",
  "hx-include",
  "hx-indicator",
  "hx-inherit",
  "hx-method",
  "hx-morph-skip",
  "hx-morph-skip-children",
  "hx-on",
  "hx-params",
  "hx-patch",
  "hx-post",
  "hx-preserve",
  "hx-prompt",
  "hx-push-url",
  "hx-put",
  "hx-query",
  "hx-replace-url",
  "hx-request",
  "hx-select",
  "hx-select-oob",
  "hx-status",
  "hx-swap",
  "hx-swap-oob",
  "hx-sync",
  "hx-target",
  "hx-trigger",
  "hx-validate",
  "hx-vals",
  "hx-vars",
];

/// What FSR sets under a project's own `fsr.lint.toml`, as a framework's shared ESLint config does.
fn base() -> Config {
  let ignore = HTMX_ATTRIBUTES.iter().map(|a| format!("\"{a}\"")).collect::<Vec<_>>().join(", ");
  Config::parse(&format!("[rules]\n\"react/no-unknown-property\" = [\"error\", {{ ignore = [{ignore}] }}]\n"), Path::new("fsr")).expect("FSR's base configuration parses")
}

/// Lints every script under `app` with FSR's base levels and then those of each `fsr.lint.toml` from the
/// repository down to the project. The templates of `.vue` and `.svelte` files go through `snapfirec-vue` and
/// `snapfirec-svelte`. The vendored packages and the fetched declarations are not the application's and are skipped.
pub fn run(app: &Path) -> Result<Report, ConfigError> {
  let mut config = Config::discover(&crate::serve::project_root(app))?.over(base());
  if let Ok(layout) = Layout::of(app) {
    for dir in [layout.vendor, layout.types] {
      config.ignore(&format!("{}/**", dir.trim_end_matches('/')))?;
    }
  }
  Ok(Linter::new(config)?.with_compilers(Box::new(Plugins::default())).lint_dir(app))
}
