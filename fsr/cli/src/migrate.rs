//! `fsr migrate`: an application written for another framework, rewritten
//! into a new FSR project. The rewrite itself is `snapfire_fsr_migrate`'s;
//! this reads the source, scaffolds the project the way `fsr new` does with
//! the framework the source renders with, then writes the result over it.

use std::path::{Path, PathBuf};

use snapfire_fsr_migrate::{is_old_layout, migrate, relayout as relayout_tree, Report, Source, Tree, CFONTS_DIR, MAP_FILE, WORKSPACE_DIR};

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
pub fn run(from: &Path, to: &Path, fetch: bool, map: Option<&Path>) -> Result<Migrated, BuildError> {
  let mut input = Tree::new();
  read(from, from, &mut input)?;
  read_cfonts(from, &mut input)?;
  if let Some(map) = map {
    let text = std::fs::read(map).map_err(|e| BuildError::Io(map.to_path_buf(), e))?;
    input.insert(MAP_FILE, text);
  }
  let mut dirs = serde_json::Map::new();
  for (name, dir) in workspace_dependencies(from) {
    let mut package = Tree::new();
    read(&dir, &dir, &mut package)?;
    for (path, bytes) in package.iter() {
      input.insert(format!("{WORKSPACE_DIR}{name}/{path}"), bytes.to_vec());
    }
    dirs.insert(name, serde_json::Value::String(relative_dir(from, &dir)));
  }
  if !dirs.is_empty() {
    input.insert(format!("{WORKSPACE_DIR}dirs.json"), serde_json::Value::Object(dirs).to_string());
  }
  let (tree, report) = migrate(&input);
  let Some(source) = report.source else {
    return Err(BuildError::Dev(format!("{}: its package.json names neither next, @sveltejs/kit, nuxt nor ink, so nothing says what it was written for", from.display())));
  };
  if !report.refusals.is_empty() {
    let refused: Vec<String> = report.refusals.iter().map(|note| format!("  {note}")).collect();
    return Err(BuildError::Dev(format!("{}: not migrated, since CommonJS is not read; rewrite these files as ES modules first:\n{}", from.display(), refused.join("\n"))));
  }
  if report.passes.is_empty() {
    return Err(BuildError::Dev(format!("{}: {} is not migrated yet", from.display(), source.name())));
  }
  let scaffold = match source {
    Source::Ink => NewOptions::default().fetch(fetch).terminal(true),
    _ => NewOptions::default().fetch(fetch).with(vec![source.direction().to_owned()]),
  };
  let created = new::create(to, scaffold)?;
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

/// The packages of the workspace around `app` it depends on with a `workspace:` range, theirs included, by name and directory.
fn workspace_dependencies(app: &Path) -> Vec<(String, PathBuf)> {
  let Some((root, globs)) = app.ancestors().skip(1).find_map(|dir| workspace_globs(dir).map(|g| (dir.to_path_buf(), g))) else { return Vec::new() };
  let mut by_name: std::collections::BTreeMap<String, PathBuf> = std::collections::BTreeMap::new();
  for glob in globs {
    let dirs: Vec<PathBuf> = match glob.trim_end_matches('/').strip_suffix("/*") {
      Some(parent) => std::fs::read_dir(root.join(parent)).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect(),
      None => vec![root.join(&glob)],
    };
    for dir in dirs {
      if let Some(name) = manifest(&dir).and_then(|m| m.get("name").and_then(|n| n.as_str()).map(str::to_owned)) {
        by_name.insert(name, dir);
      }
    }
  }
  let mut out: Vec<(String, PathBuf)> = Vec::new();
  let mut queue = vec![app.to_path_buf()];
  while let Some(dir) = queue.pop() {
    let Some(json) = manifest(&dir) else { continue };
    let Some(deps) = json.get("dependencies").and_then(|d| d.as_object()) else { continue };
    for (name, range) in deps {
      if !range.as_str().is_some_and(|r| r.starts_with("workspace:")) || out.iter().any(|(n, _)| n == name) {
        continue;
      }
      if let Some(found) = by_name.get(name) {
        out.push((name.clone(), found.clone()));
        queue.push(found.clone());
      }
    }
  }
  out
}

/// The package globs `dir` declares as a workspace root, from `pnpm-workspace.yaml` or `package.json`'s `workspaces`.
fn workspace_globs(dir: &Path) -> Option<Vec<String>> {
  if let Ok(yaml) = std::fs::read_to_string(dir.join("pnpm-workspace.yaml")) {
    let mut globs = Vec::new();
    let mut listing = false;
    for line in yaml.lines() {
      if !line.starts_with(' ') && !line.starts_with('-') {
        listing = line.trim_end() == "packages:";
        continue;
      }
      if let (true, Some(item)) = (listing, line.trim_start().strip_prefix("- ")) {
        let item = item.split(" #").next().unwrap_or(item).trim().trim_matches(|c| c == '"' || c == '\'');
        if !item.starts_with('!') {
          globs.push(item.to_owned());
        }
      }
    }
    return Some(globs);
  }
  let json = manifest(dir)?;
  let workspaces = json.get("workspaces")?;
  let list = workspaces.as_array().or_else(|| workspaces.get("packages").and_then(|p| p.as_array()))?;
  Some(list.iter().filter_map(|g| g.as_str().map(str::to_owned)).collect())
}

/// `to` as a `/`-separated path from `from`, through `..` where it must.
fn relative_dir(from: &Path, to: &Path) -> String {
  let from: Vec<_> = from.components().collect();
  let to: Vec<_> = to.components().collect();
  let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
  let up = std::iter::repeat_n("..".to_owned(), from.len() - common);
  let down = to[common..].iter().map(|c| c.as_os_str().to_string_lossy().into_owned());
  up.chain(down).collect::<Vec<_>>().join("/")
}

fn manifest(dir: &Path) -> Option<serde_json::Value> {
  serde_json::from_str(&std::fs::read_to_string(dir.join("package.json")).ok()?).ok()
}

/// The font files of the cfonts package installed for `from`, in its own `node_modules` or one a workspace hoisted it to, under `CFONTS_DIR`.
fn read_cfonts(from: &Path, tree: &mut Tree) -> Result<(), BuildError> {
  let Some(fonts) = from.ancestors().map(|dir| dir.join("node_modules/cfonts/fonts")).find(|dir| dir.is_dir()) else { return Ok(()) };
  for entry in std::fs::read_dir(&fonts).map_err(|e| BuildError::Io(fonts.clone(), e))?.flatten() {
    let path = entry.path();
    if path.extension().is_some_and(|x| x == "json") {
      let bytes = std::fs::read(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
      tree.insert(format!("{CFONTS_DIR}{}", entry.file_name().to_string_lossy()), bytes);
    }
  }
  Ok(())
}

/// What the build writes or fetches under `app/`, which `relayout` leaves alone.
const BUILT: &[&str] = &["vendor", "generated", "types", "dist", ".fsr"];

/// One file `relayout` changed: its path under the project and each changed line, as it was and as it is.
#[derive(Debug)]
#[non_exhaustive]
pub struct Relaid {
  pub path: String,
  pub lines: Vec<(String, String)>,
}

/// Rewrites the FSR project at `project` from `@snapfire/fsr-client`, `@snapfire/fsr-authoring` and `@snapfire/fsr`'s `Ctx` onto `@snapfire/fsr/client`, `/host`, `/terminal`, `/template` and `/components`, in place unless `dry_run`.
pub fn relayout(project: &Path, dry_run: bool) -> Result<Vec<Relaid>, BuildError> {
  let mut tree = Tree::new();
  read_app(&project.join("app"), project, &mut tree)?;
  if !is_old_layout(&tree) {
    return Ok(Vec::new());
  }
  let (out, _) = relayout_tree(&tree);
  let mut changed = Vec::new();
  for (path, bytes) in out.iter() {
    if tree.get(path) == Some(bytes) {
      continue;
    }
    let (before, after) = (tree.text(path).unwrap_or_default(), std::str::from_utf8(bytes).unwrap_or_default());
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    let lines = old.iter().filter(|l| !new.contains(l)).map(|l| ((*l).to_owned(), String::new())).chain(new.iter().filter(|l| !old.contains(l)).map(|l| (String::new(), (*l).to_owned()))).collect();
    if !dry_run {
      let target = project.join(path);
      std::fs::write(&target, bytes).map_err(|e| BuildError::Io(target, e))?;
    }
    changed.push(Relaid { path: path.to_owned(), lines });
  }
  Ok(changed)
}

fn read_app(dir: &Path, root: &Path, tree: &mut Tree) -> Result<(), BuildError> {
  let entries = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?;
  for entry in entries.flatten() {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    if path.is_dir() {
      if !BUILT.contains(&name.as_str()) && !SKIPPED.contains(&name.as_str()) {
        read_app(&path, root, tree)?;
      }
    } else if path.is_file() {
      let bytes = std::fs::read(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
      tree.insert(path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/"), bytes);
    }
  }
  Ok(())
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
