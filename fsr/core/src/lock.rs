//! Locks on a file, held the same across threads and processes: two hosts sharing a directory, `fsr dev` beside `fsr sites install`, two test processes under nextest. The lock is the operating system's on an open file, so it ends with the process that held it, crashed or not.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// A lock on `path`, released when dropped.
#[derive(Debug)]
pub struct Held {
  file: File,
  path: PathBuf,
}

impl Held {
  pub fn path(&self) -> &Path {
    &self.path
  }
}

impl Drop for Held {
  fn drop(&mut self) {
    let _ = self.file.unlock();
  }
}

fn open(path: &Path) -> io::Result<File> {
  if let Some(parent) = path.parent() {
    std::fs::create_dir_all(parent)?;
  }
  OpenOptions::new().create(true).truncate(false).read(true).write(true).open(path)
}

/// Waits until no one else holds `path` and holds it alone. The file is made when absent and left in place.
pub fn exclusive(path: impl AsRef<Path>) -> io::Result<Held> {
  let path = path.as_ref();
  let file = open(path)?;
  file.lock()?;
  Ok(Held { file, path: path.to_path_buf() })
}

/// Waits until no one holds `path` alone and holds it beside any other shared holder.
pub fn shared(path: impl AsRef<Path>) -> io::Result<Held> {
  let path = path.as_ref();
  let file = open(path)?;
  file.lock_shared()?;
  Ok(Held { file, path: path.to_path_buf() })
}

/// Why a [`once`] did not finish.
#[derive(Debug)]
pub enum OnceError<E> {
  Io(io::Error),
  /// The setup failed. Nothing was recorded, so the next caller runs it again.
  Setup(E),
}

impl<E: std::fmt::Display> std::fmt::Display for OnceError<E> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Io(e) => write!(f, "{e}"),
      Self::Setup(e) => write!(f, "{e}"),
    }
  }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for OnceError<E> {}

/// Runs `setup` once for `key` across every thread and process that shares `dir`: the first caller runs it while the others wait. A later caller finds it done. `key` names the inputs, so changing them runs it again. `true` when this call ran it.
pub fn once<E>(dir: impl AsRef<Path>, key: &str, setup: impl FnOnce() -> Result<(), E>) -> Result<bool, OnceError<E>> {
  let dir = dir.as_ref();
  let _held = exclusive(dir.join(format!("{key}.lock"))).map_err(OnceError::Io)?;
  let done = dir.join(format!("{key}.done"));
  if done.is_file() {
    return Ok(false);
  }
  setup().map_err(OnceError::Setup)?;
  std::fs::write(&done, b"").map_err(OnceError::Io)?;
  Ok(true)
}
