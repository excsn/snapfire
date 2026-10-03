use crate::build::{self, Build, Options};
use crate::watch;

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use snapfire_compiler_wire::driven::{FAILED, MAPPED, REBUILT, REFERENCES};

/// Reads batches of changed paths from stdin, one path per line and an empty line to end the
/// batch, and rebuilds each batch the way [`watch`] rebuilds what its watcher reported. An empty
/// batch is a full rebuild. Returns when stdin closes. The protocol is
/// `snapfire_compiler_wire::driven`; the hello line was written before the first build.
pub fn run(opts: &Options, mut build: Build) -> Result<()> {
  let config_path = opts.root.join(&opts.config_path);
  let stdin = std::io::stdin();
  let mut lines = stdin.lock().lines();
  let mut changed: Vec<PathBuf> = Vec::new();

  if !settle(opts, &mut build, &mut lines)? {
    return Ok(());
  }

  while let Some(line) = lines.next() {
    let line = line?;
    if !line.is_empty() {
      changed.push(opts.root.join(line));
      continue;
    }

    if changed.is_empty() || watch::structural(&changed, &build, &config_path) {
      match build::full(opts, false) {
        // The workers and what they answered outlive the build that spawned
        // them: a structural change is not a reason to boot a compiler again.
        Ok(mut next) => {
          next.plugins = std::mem::take(&mut build.plugins);
          next.plugin_cache = std::mem::take(&mut build.plugin_cache);
          build = next;
        }
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
    if !settle(opts, &mut build, &mut lines)? {
      return Ok(());
    }
  }

  Ok(())
}

/// Ends a batch. A source waiting on an asset the map does not define is
/// reported first, as `REFERENCES` and the paths, then compiled again once the
/// driver answers `MAPPED`; then the batch's line is printed. `Ok(false)` when
/// the driver went away mid-handshake.
fn settle(opts: &Options, build: &mut Build, lines: &mut impl Iterator<Item = std::io::Result<String>>) -> Result<bool> {
  if opts.asset_map.is_some() && !build.misses.is_empty() {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{REFERENCES}")?;
    for path in build.missing_assets() {
      writeln!(out, "{path}")?;
    }
    writeln!(out)?;
    out.flush()?;
    drop(out);
    match lines.next() {
      Some(Ok(answer)) if answer.trim_end() == MAPPED => {
        if let Err(e) = build.remap(opts) {
          eprintln!("❌ {:#}", e);
          build.has_error = true;
        }
      }
      Some(Ok(other)) => {
        eprintln!("❌ expected `{MAPPED}` after `{REFERENCES}`, got {other:?}");
        build.has_error = true;
      }
      Some(Err(e)) => return Err(e.into()),
      None => return Ok(false),
    }
  }
  build.report_misses(opts);
  println!("{}", if build.failed() { FAILED } else { REBUILT });
  std::io::stdout().flush()?;
  Ok(true)
}
