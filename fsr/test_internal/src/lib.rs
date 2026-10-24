//! What FSR's own tests share. A dev-dependency inside this workspace, never published.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// A directory of its own under the system temporary directory, removed when dropped. Two tests, two processes or two runs never share one.
#[derive(Debug)]
pub struct Scratch {
  path: PathBuf,
}

impl Scratch {
  /// An empty directory whose name starts `fsr-<tag>-`.
  pub fn new(tag: &str) -> Self {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!("fsr-{tag}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap_or_else(|e| panic!("creating {}: {e}", path.display()));
    Self { path }
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
    self.path.join(relative)
  }

  /// Writes `text` at `relative`, making the directories above it.
  pub fn write(&self, relative: impl AsRef<Path>, text: &str) -> PathBuf {
    let file = self.path.join(relative);
    if let Some(parent) = file.parent() {
      std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("creating {}: {e}", parent.display()));
    }
    std::fs::write(&file, text).unwrap_or_else(|e| panic!("writing {}: {e}", file.display()));
    file
  }
}

impl Drop for Scratch {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.path);
  }
}

/// A request a test needs answered before it can run.
#[derive(Debug, Clone, Copy)]
pub enum Probe<'a> {
  Get(&'a str),
  /// For a document too large to fetch only to learn that it is there.
  Head(&'a str),
}

/// Whether every probe answers with a success within ten seconds. When one does not, `SKIPPED <test>: <url> ...` goes to stderr, which nextest shows for the targets `fsr/.config/nextest.toml` names and `cargo test` shows always.
pub fn answers(test: &str, probes: &[Probe<'_>]) -> bool {
  let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(10)).build() {
    Ok(client) => client,
    Err(e) => return skipped(test, &format!("no HTTP client ({e})")),
  };
  for probe in probes {
    let (url, sent) = match probe {
      Probe::Get(url) => (*url, client.get(*url).send()),
      Probe::Head(url) => (*url, client.head(*url).send()),
    };
    match sent {
      Ok(response) if response.status().is_success() => {}
      Ok(response) => return skipped(test, &format!("{url} answered HTTP {}", response.status())),
      Err(e) => return skipped(test, &format!("{url} did not answer ({e})")),
    }
  }
  true
}

/// Writes the skip line past libtest's capture and answers `false`.
pub fn skipped(test: &str, why: &str) -> bool {
  let _ = writeln!(std::io::stderr(), "SKIPPED {test}: {why}");
  false
}

/// The snapfire repository root, two levels above this crate.
pub fn snapfire_root() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The checker a test runs: `$SNAPFIRETC`, else the snapfire root's debug build. The path is returned whether or not a file is there.
pub fn snapfiretc() -> PathBuf {
  std::env::var_os("SNAPFIRETC").filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| snapfire_root().join("target/debug/snapfiretc"))
}

/// A response's body as text.
pub async fn body_of<B>(response: http::Response<B>) -> String
where
  B: http_body::Body,
  B::Error: std::fmt::Debug,
{
  use http_body_util::BodyExt;
  let bytes = response.into_body().collect().await.expect("the body reads").to_bytes();
  String::from_utf8(bytes.to_vec()).expect("the body is UTF-8")
}
