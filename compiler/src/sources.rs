use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use walkdir::WalkDir;

/// tsc's `exclude` default. An explicit `exclude` replaces this rather than adding to it.
const DEFAULT_EXCLUDE: [&str; 3] = ["node_modules", "bower_components", "jspm_packages"];

/// Directory names pruned at any depth, whatever `exclude` says.
const ALWAYS_PRUNED: [&str; 2] = ["node_modules", ".git"];

pub struct Selection {
  pub root_dir: PathBuf,
  pub files: Vec<PathBuf>,
  pub include_patterns: Vec<String>,
  pub include_defaulted: bool,
  pub unmatched_patterns: Vec<String>,
  /// The directories the patterns can reach, which is also the smallest set a watcher has to
  /// subscribe to.
  pub search_bases: Vec<PathBuf>,
}

pub struct Request<'a> {
  pub config_dir: &'a Path,
  pub out_dir: &'a Path,
  pub files: Option<Vec<String>>,
  pub include: Option<Vec<String>>,
  pub exclude: Option<Vec<String>>,
  pub root_dir: Option<PathBuf>,
  /// Decides which matched files count as program inputs when `rootDir` has to be computed. tsc
  /// derives it from compilable files alone, so a stray README cannot drag the whole tree up.
  pub is_input: &'a dyn Fn(&Path) -> bool,
}

pub fn select(request: Request) -> Result<Selection> {
  let include_defaulted = request.include.is_none() && request.files.is_none();

  let include_patterns = match (&request.include, include_defaulted) {
    (Some(patterns), _) => patterns.clone(),
    (None, true) => vec!["**/*".to_string()],
    (None, false) => Vec::new(),
  };

  let exclude_patterns = request.exclude.unwrap_or_else(|| DEFAULT_EXCLUDE.map(String::from).to_vec());

  let include = Matcher::build(request.config_dir, &include_patterns)?;
  let exclude = Matcher::build(request.config_dir, &exclude_patterns)?;

  let mut files = Vec::new();
  let mut seen = HashSet::new();
  let mut matched = vec![false; include_patterns.len()];

  let search_bases = search_bases(&include.globs);

  for base in &search_bases {
    walk(base, request.out_dir, &exclude.literal_dirs, |path| {
      let Some(candidate) = path.to_str() else {
        return;
      };

      if !include.set.is_match(candidate) || exclude.set.is_match(candidate) {
        return;
      }

      for glob in include.set.matches(candidate) {
        matched[include.pattern_of[glob]] = true;
      }

      if seen.insert(path.to_path_buf()) {
        files.push(path.to_path_buf());
      }
    });
  }

  // `files` names inputs explicitly, so `exclude` does not apply to them.
  for named in request.files.into_iter().flatten() {
    let path = request.config_dir.join(&named);
    if !path.is_file() {
      bail!("File {:?} listed in 'files' does not exist.", named);
    }
    let path = path.canonicalize().with_context(|| format!("Failed to resolve {:?}", named))?;
    if seen.insert(path.clone()) {
      files.push(path);
    }
  }

  let inputs: Vec<&Path> = files
    .iter()
    .map(|p| p.as_path())
    .filter(|p| (request.is_input)(p))
    .collect();

  let root_dir = match request.root_dir {
    Some(explicit) => {
      let resolved = request.config_dir.join(explicit);
      let resolved = resolved
        .canonicalize()
        .with_context(|| format!("Failed to resolve 'rootDir' {:?}", resolved))?;

      for input in &inputs {
        if !input.starts_with(&resolved) {
          bail!("File {:?} is not under 'rootDir' {:?}.", input, resolved);
        }
      }

      resolved
    }
    None => common_root(&inputs).unwrap_or_else(|| request.config_dir.to_path_buf()),
  };

  let unmatched_patterns = include_patterns
    .iter()
    .zip(&matched)
    .filter(|(_, hit)| !**hit)
    .map(|(pattern, _)| pattern.clone())
    .collect();

  Ok(Selection {
    root_dir,
    files,
    include_patterns,
    include_defaulted,
    unmatched_patterns,
    search_bases,
  })
}

struct Matcher {
  set: GlobSet,
  globs: Vec<String>,
  pattern_of: Vec<usize>,
  literal_dirs: Vec<PathBuf>,
}

impl Matcher {
  /// Expands each pattern the way tsc does, resolved against the config directory so that a
  /// pattern reaching outside it with `..` still works.
  ///
  /// An entry with no glob metacharacter names a directory and stands for everything under it,
  /// which is what keeps `"include": ["src"]` meaning what it always did.
  fn build(config_dir: &Path, patterns: &[String]) -> Result<Self> {
    let mut builder = GlobSetBuilder::new();
    let mut globs = Vec::new();
    let mut pattern_of = Vec::new();
    let mut literal_dirs = Vec::new();

    for (index, pattern) in patterns.iter().enumerate() {
      let absolute = lexical_join(config_dir, &normalise(pattern));

      let expanded = if has_glob(&absolute) {
        vec![absolute]
      } else {
        let path = PathBuf::from(&absolute);
        if path.is_dir() {
          literal_dirs.push(path);
        }
        if PathBuf::from(&absolute).is_file() {
          vec![absolute]
        } else {
          vec![format!("{absolute}/**/*"), absolute]
        }
      };

      for glob in expanded {
        let compiled = GlobBuilder::new(&glob)
          .literal_separator(true)
          .build()
          .with_context(|| format!("Invalid pattern {:?}", pattern))?;
        builder.add(compiled);
        globs.push(glob);
        pattern_of.push(index);
      }
    }

    Ok(Self {
      set: builder.build().context("Failed to build the pattern matcher")?,
      globs,
      pattern_of,
      literal_dirs,
    })
  }
}

/// The deepest directory each glob can possibly match under, so the walk visits only what a
/// pattern can reach rather than the whole project.
fn search_bases(globs: &[String]) -> Vec<PathBuf> {
  let mut bases: Vec<PathBuf> = Vec::new();

  for glob in globs {
    let mut base = PathBuf::from("/");
    for segment in glob.split('/').skip(1) {
      if has_glob(segment) {
        break;
      }
      base.push(segment);
    }

    if base.is_file() {
      base = base.parent().map(Path::to_path_buf).unwrap_or(base);
    }

    if !bases.iter().any(|existing| base.starts_with(existing)) {
      bases.retain(|existing| !existing.starts_with(&base));
      bases.push(base);
    }
  }

  bases
}

fn walk(base: &Path, out_dir: &Path, exclude_dirs: &[PathBuf], mut visit: impl FnMut(&Path)) {
  let walker = WalkDir::new(base)
    .sort_by_file_name()
    .into_iter()
    .filter_entry(|e| {
      let path = e.path();

      if path == out_dir {
        return false;
      }

      if let Some(name) = path.file_name()
        && ALWAYS_PRUNED.iter().any(|pruned| name == *pruned)
      {
        return false;
      }

      !exclude_dirs.iter().any(|dir| path == dir)
    });

  for entry in walker {
    let entry = match entry {
      Ok(entry) => entry,
      Err(e) => {
        eprintln!("⚠️  Error accessing path: {}", e);
        continue;
      }
    };

    if entry.file_type().is_file() {
      visit(entry.path());
    }
  }
}

/// Joins a pattern onto a base and resolves `.` and `..` textually, because a pattern holding glob
/// metacharacters cannot be canonicalised against the filesystem.
fn lexical_join(base: &Path, pattern: &str) -> String {
  if pattern.starts_with('/') {
    return pattern.to_string();
  }

  let mut parts: Vec<String> = base
    .components()
    .filter_map(|c| match c {
      Component::Normal(s) => s.to_str().map(str::to_string),
      _ => None,
    })
    .collect();

  for segment in pattern.split('/') {
    match segment {
      "" | "." => {}
      ".." => {
        parts.pop();
      }
      other => parts.push(other.to_string()),
    }
  }

  format!("/{}", parts.join("/"))
}

fn normalise(pattern: &str) -> String {
  let pattern = pattern.replace('\\', "/");
  pattern.trim_end_matches('/').to_string()
}

fn has_glob(pattern: &str) -> bool {
  pattern.contains(['*', '?', '['])
}

fn common_root(inputs: &[&Path]) -> Option<PathBuf> {
  let mut root: Option<PathBuf> = None;

  for input in inputs {
    let dir = input.parent()?;

    root = Some(match root {
      None => dir.to_path_buf(),
      Some(current) => shared_prefix(&current, dir),
    });
  }

  root
}

fn shared_prefix(a: &Path, b: &Path) -> PathBuf {
  a.components()
    .zip(b.components())
    .take_while(|(x, y)| x == y)
    .map(|(x, _)| x)
    .collect()
}
