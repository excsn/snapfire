use crate::transforms::Import;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};

/// Extensions a `<link rel="modulepreload">` is valid for. A stylesheet needs a different `rel`, so
/// listing one here would produce markup that silently does nothing.
const PRELOADABLE: [&str; 2] = ["js", "mjs"];

#[derive(Default)]
pub struct Graph {
  /// Static edges only. A dynamic import is a dependency but never a preload, since deferring it is
  /// the author's explicit choice.
  edges: HashMap<PathBuf, Vec<PathBuf>>,
  nodes: BTreeSet<PathBuf>,
}

impl Graph {
  pub fn add(&mut self, module: &Path, imports: &[Import]) {
    self.nodes.insert(module.to_path_buf());

    let dir = module.parent().unwrap_or(Path::new("/"));
    let targets: Vec<PathBuf> = imports
      .iter()
      .filter(|i| !i.dynamic)
      .map(|i| normalise(&dir.join(&i.specifier)))
      .collect();

    self.edges.insert(module.to_path_buf(), targets);
  }

  /// Modules nothing else statically imports, which is what a page loads directly.
  pub fn entry_points(&self) -> Vec<&Path> {
    let imported: BTreeSet<&Path> = self.edges.values().flatten().map(PathBuf::as_path).collect();

    self
      .nodes
      .iter()
      .map(PathBuf::as_path)
      .filter(|node| preloadable(node) && !imported.contains(node))
      .collect()
  }

  /// Everything an entry point pulls in before it can run, in the order a page should preload it.
  /// Cycles are legal between ES modules, so the walk tracks what it has already seen.
  pub fn dependencies_of(&self, entry: &Path) -> Vec<&Path> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![entry];
    let mut found = BTreeSet::new();

    while let Some(module) = queue.pop() {
      let Some(targets) = self.edges.get(module) else {
        continue;
      };

      for target in targets {
        if !seen.insert(target.as_path()) {
          continue;
        }

        if preloadable(target) {
          found.insert(target.as_path());
        }

        queue.push(target);
      }
    }

    found.into_iter().collect()
  }

  pub fn manifest(&self, out_dir: &Path, public_path: Option<&str>) -> Option<String> {
    let mut entries: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for entry in self.entry_points() {
      let deps: Vec<String> = self
        .dependencies_of(entry)
        .into_iter()
        .filter_map(|d| url_for(d, out_dir, public_path))
        .collect();

      if let Some(name) = url_for(entry, out_dir, public_path) {
        entries.insert(name, deps);
      }
    }

    if entries.is_empty() {
      return None;
    }

    let body: Vec<String> = entries
      .iter()
      .map(|(entry, deps)| {
        let listed: Vec<String> = deps.iter().map(|d| format!("\"{d}\"")).collect();
        format!("  \"{}\": [{}]", entry, listed.join(", "))
      })
      .collect();

    Some(format!("{{\n{}\n}}\n", body.join(",\n")))
  }
}

fn preloadable(path: &Path) -> bool {
  path
    .extension()
    .and_then(|e| e.to_str())
    .is_some_and(|e| PRELOADABLE.contains(&e.to_ascii_lowercase().as_str()))
}

/// Without a public path the manifest stays in the output directory's own terms, which keeps the
/// build deployment-independent and leaves prefixing to whatever renders the page.
fn url_for(path: &Path, out_dir: &Path, public_path: Option<&str>) -> Option<String> {
  let relative = path.strip_prefix(out_dir).ok()?.to_str()?.replace('\\', "/");

  match public_path {
    Some(base) => Some(format!("{}{}", base, relative)),
    None => Some(relative),
  }
}

/// Resolves `.` and `..` without touching the filesystem, since a dependency may legitimately not
/// have been written yet when the edge is recorded.
fn normalise(path: &Path) -> PathBuf {
  let mut out = PathBuf::new();

  for component in path.components() {
    match component {
      Component::CurDir => {}
      Component::ParentDir => {
        out.pop();
      }
      other => out.push(other),
    }
  }

  out
}
