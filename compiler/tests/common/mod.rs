// Every integration test binary compiles this module in full, so whatever a given binary does not
// call reads as dead code to that binary alone.
#![allow(dead_code)]

use assert_cmd::assert::Assert;
use assert_cmd::cargo_bin;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::{TempDir, tempdir};

pub fn get_snapfirec_cmd() -> Command {
  Command::new(cargo_bin!("snapfirec"))
}

pub struct Fixture {
  // We keep the TempDir object in scope so it doesn't get dropped (and deleted)
  // until the Fixture itself is dropped.
  _temp_dir: TempDir,
}

impl Fixture {
  /// Creates a new test fixture by copying a directory from `tests/fixtures`
  /// into a new temporary directory.
  pub fn new(fixture_name: &str) -> Self {
    Self::from_path(&Path::new("tests/fixtures").join(fixture_name))
  }

  /// Copies any directory, so the shipped example can be exercised without being moved into
  /// `tests/fixtures` where nobody would read it.
  pub fn from_path(source: &Path) -> Self {
    let temp_dir = tempdir().expect("Failed to create temp directory");

    copy_dir_all(source, temp_dir.path()).unwrap_or_else(|e| panic!("Failed to copy {source:?}: {e}"));

    Fixture { _temp_dir: temp_dir }
  }

  /// Returns the absolute path to the temporary fixture directory.
  pub fn root(&self) -> &Path {
    self._temp_dir.path()
  }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
  fs::create_dir_all(dst)?;
  for entry in fs::read_dir(src)? {
    let entry = entry?;
    let ty = entry.file_type()?;
    if ty.is_dir() {
      copy_dir_all(&entry.path(), &dst.join(entry.file_name()))?;
    } else {
      fs::copy(entry.path(), dst.join(entry.file_name()))?;
    }
  }
  Ok(())
}

pub fn run_snapfirec(cmd: &mut Command) -> Assert {
  let output = cmd.output().expect("Failed to execute command");

  // Always print output. Cargo test captures this and only shows it on failure.
  println!("\n🔍 --- snapfirec execution ---");
  println!("Command: {:?}", cmd);
  println!("Status: {}", output.status);
  println!("Stdout:\n{}", String::from_utf8_lossy(&output.stdout));
  println!("Stderr:\n{}", String::from_utf8_lossy(&output.stderr));
  println!("------------------------------\n");

  if !output.status.success() {
    panic!("Command failed with status {}", output.status);
  }

  Assert::new(output)
}
