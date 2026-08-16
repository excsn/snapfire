use crate::build::{self, Build, Options};
use anyhow::{Context, Result};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::Duration;

/// Editors save in bursts: a write, a rename and a chmod can all land within a few milliseconds.
/// Batching until this much quiet has passed turns one save into one rebuild.
const SETTLE: Duration = Duration::from_millis(120);

pub fn run(opts: &Options, mut build: Build) -> Result<()> {
  let (tx, rx) = channel();
  let mut watcher = RecommendedWatcher::new(tx, notify::Config::default()).context("Failed to start the watcher")?;

  for base in &build.search_bases {
    watcher
      .watch(base, RecursiveMode::Recursive)
      .with_context(|| format!("Failed to watch {:?}", base))?;
  }

  let config_path = opts.root.join(&opts.config_path);
  for extra in [config_path.as_path(), &opts.root.join(".browserslistrc")] {
    if extra.is_file() {
      let _ = watcher.watch(extra, RecursiveMode::NonRecursive);
    }
  }

  let watching: Vec<String> = build
    .search_bases
    .iter()
    .map(|b| build::display(b, &opts.root))
    .collect();
  println!("👀 watching {}; press Ctrl-C to stop", watching.join(", "));

  loop {
    let changed = match collect(&rx) {
      Some(changed) => changed,
      None => return Ok(()),
    };

    if changed.is_empty() {
      continue;
    }

    if structural(&changed, &build, &config_path) {
      match build::full(opts, false) {
        Ok(next) => build = next,
        Err(e) => eprintln!("❌ {:#}", e),
      }
    } else {
      build.has_error = false;
      for path in &changed {
        build::refresh(opts, &mut build, path);
      }
    }

    if build.has_error {
      println!("   waiting for changes");
    }
  }
}

/// Blocks for the first event, then keeps draining until the filesystem has been quiet for
/// [`SETTLE`]. Returns `None` once the watcher has hung up.
fn collect(rx: &Receiver<notify::Result<notify::Event>>) -> Option<Vec<PathBuf>> {
  let mut paths: HashSet<PathBuf> = HashSet::new();

  let first = rx.recv().ok()?;
  absorb(first, &mut paths);

  loop {
    match rx.recv_timeout(SETTLE) {
      Ok(event) => absorb(event, &mut paths),
      Err(RecvTimeoutError::Timeout) => break,
      Err(RecvTimeoutError::Disconnected) => return None,
    }
  }

  Some(paths.into_iter().collect())
}

fn absorb(event: notify::Result<notify::Event>, paths: &mut HashSet<PathBuf>) {
  match event {
    Ok(event) => paths.extend(event.paths),
    Err(e) => eprintln!("⚠️  Watch error: {}", e),
  }
}

/// Anything that changes the shape of the build rather than the contents of one known file: a new
/// or deleted file, or a config the whole selection was derived from.
fn structural(changed: &[PathBuf], build: &Build, config_path: &Path) -> bool {
  changed.iter().any(|path| {
    if path == config_path || path.file_name().is_some_and(|n| n == ".browserslistrc") {
      return true;
    }

    !path.is_file() || !build.files.contains(path)
  })
}
