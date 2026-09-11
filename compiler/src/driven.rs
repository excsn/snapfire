use crate::build::{self, Build, Options};
use crate::watch;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;

/// Printed once the batch before it has been compiled, so the driving process knows a rebuild
/// landed without watching the output directory for it.
pub const REBUILT: &str = "snapfirec: rebuilt";
pub const FAILED: &str = "snapfirec: failed";

/// Reads batches of changed paths from stdin, one path per line and an empty line to end the
/// batch, and rebuilds each batch the way [`watch`] rebuilds what its watcher reported. An empty
/// batch is a full rebuild. Returns when stdin closes.
pub fn run(opts: &Options, mut build: Build) -> Result<()> {
  settled(&build)?;

  let config_path = opts.root.join(&opts.config_path);
  let stdin = std::io::stdin();
  let mut changed: Vec<PathBuf> = Vec::new();

  for line in stdin.lock().lines() {
    let line = line?;
    if !line.is_empty() {
      changed.push(opts.root.join(line));
      continue;
    }

    if changed.is_empty() || watch::structural(&changed, &build, &config_path) {
      match build::full(opts, false) {
        Ok(next) => build = next,
        Err(e) => {
          eprintln!("❌ {:#}", e);
          build.has_error = true;
        }
      }
    } else {
      build.has_error = false;
      for path in &changed {
        build::refresh(opts, &mut build, path);
      }
    }

    changed.clear();
    settled(&build)?;
  }

  Ok(())
}

fn settled(build: &Build) -> Result<()> {
  println!("{}", if build.has_error { FAILED } else { REBUILT });
  std::io::stdout().flush()?;
  Ok(())
}
