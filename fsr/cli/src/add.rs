//! `fsr add`: the packages vendored, then the declarations of what the import map now names, the way `fsr new` and `fsr use` leave an application `fsr check` can read.

use std::path::{Path, PathBuf};

use crate::vendor::{self, AddReport, Spec};
use crate::BuildError;

#[derive(Default)]
#[non_exhaustive]
pub struct Added {
  /// One report per group vendored: a framework's companions as published first, then the rest.
  pub groups: Vec<AddReport>,
  /// Package, version and where its declarations came from, for each one fetched.
  pub typed: Vec<(String, String, String)>,
  /// The configuration file and the TypeScript version pinned in it.
  pub pinned: Option<(PathBuf, String)>,
  pub written: Vec<PathBuf>,
  /// What did not finish after the packages were vendored, each with the command that finishes it.
  pub notes: Vec<String>,
}

/// Vendors `specs` into `app`, a package vendored as published before by every specifier it recorded and a framework with what its adapter imports, then fetches the types the import map names. `published` vendors the rest as published too.
pub fn add(app: &Path, specs: Vec<Spec>, externals: &[String], published: bool) -> Result<Added, BuildError> {
  let groups = match crate::published::recorded(app, &specs)? {
    Some(specs) => vec![(specs, true)],
    None => {
      let (plain, companions) = crate::direction::with_companions(specs);
      [(companions, true), (plain, published)].into_iter().filter(|(specs, _)| !specs.is_empty()).collect()
    }
  };
  let mut added = Added::default();
  for (specs, published) in groups {
    added.groups.push(match published {
      true => crate::published::add_published(app, &specs)?,
      false => vendor::add(app, &specs, externals)?,
    });
  }
  match crate::types::fetch(app, false) {
    Ok(report) => {
      added.typed = report.fetched;
      added.pinned = report.pinned;
      match crate::new::generate(app) {
        Ok(written) => added.written = written,
        Err(e) => added.notes.push(format!("writing the generated modules failed ({e}); run `fsr build {}`", app.display())),
      }
    }
    Err(e) => added.notes.push(format!("fetching types failed ({e}); run `fsr types {}`", app.display())),
  }
  Ok(added)
}
