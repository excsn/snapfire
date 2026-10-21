//! `fsr migrate`: an application written for another framework, rewritten
//! into a new FSR project. The rewrite itself is `snapfire_fsr_migrate`'s;
//! this reads the source, scaffolds the project the way `fsr new` does with
//! the framework the source renders with, then writes the result over it.

use std::path::{Path, PathBuf};

use snapfire_fsr_migrate::{migrate, Report, Tree};

use crate::new::{self, Created, NewOptions};
use crate::BuildError;

/// Directories a source tree holds that are no part of the application:
/// installed packages, version control and the frameworks' build output.
const SKIPPED: &[&str] = &["node_modules", ".git", ".next", ".nuxt", ".output", ".svelte-kit", ".vercel", ".turbo"];

#[derive(Debug)]
#[non_exhaustive]
pub struct Migrated {
  /// What the scaffold wrote and vendored.
  pub created: Created,
  /// The files the migration wrote, over the scaffold.
  pub written: Vec<PathBuf>,
  pub report: Report,
}

/// Reads the application at `from` and writes it, migrated, as a new
/// project at `to`. `fetch` is `fsr new`'s: whether the scaffold vendors
/// what its direction pins.
pub fn run(from: &Path, to: &Path, fetch: bool) -> Result<Migrated, BuildError> {
  let mut input = Tree::new();
  read(from, from, &mut input)?;
  let (tree, report) = migrate(&input);
  let Some(source) = report.source else {
    return Err(BuildError::Dev(format!("{}: its package.json names neither next, @sveltejs/kit nor nuxt, so nothing says what it was written for", from.display())));
  };
  if report.passes.is_empty() {
    return Err(BuildError::Dev(format!("{}: {} is not migrated yet", from.display(), source.name())));
  }
  let created = new::create(to, NewOptions::default().fetch(fetch).with(vec![source.direction().to_owned()]))?;
  let routes = to.join("app/routes");
  std::fs::remove_dir_all(&routes).map_err(|e| BuildError::Io(routes.clone(), e))?;
  let mut written = Vec::new();
  for (path, bytes) in tree.iter() {
    let target = to.join(path);
    if let Some(parent) = target.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&target, bytes).map_err(|e| BuildError::Io(target.clone(), e))?;
    written.push(target);
  }
  if !report.config.is_empty() {
    let config = to.join("config/app.toml");
    let mut toml = std::fs::read_to_string(&config).map_err(|e| BuildError::Io(config.clone(), e))?;
    for table in &report.config {
      toml.push('\n');
      toml.push_str(table);
    }
    std::fs::write(&config, toml).map_err(|e| BuildError::Io(config.clone(), e))?;
  }
  Ok(Migrated { created, written, report })
}

fn read(dir: &Path, root: &Path, tree: &mut Tree) -> Result<(), BuildError> {
  let entries = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
  let mut entries: Vec<_> = entries.flatten().collect();
  entries.sort_by_key(|e| e.file_name());
  for entry in entries {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    if path.is_dir() {
      if !SKIPPED.contains(&name.as_str()) {
        read(&path, root, tree)?;
      }
    } else if path.is_file() {
      let bytes = std::fs::read(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
      let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");
      tree.insert(rel, bytes);
    }
  }
  Ok(())
}
