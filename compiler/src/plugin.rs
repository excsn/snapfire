//! Framework compilers, found on PATH and spoken to over a pipe.
//!
//! `snapfirec` gains no dependency, no engine and no size from a framework it
//! does not compile: a `.vue` file asks for `snapfirec-vue` the way `git` asks
//! for `git-lfs`, and someone who never writes one never learns the plugin
//! exists.
//!
//! A worker is spawned once and kept for the life of the build, watch mode
//! included, because spawning per file is fatal when a project has hundreds of
//! components and a boot costs tens of milliseconds.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{anyhow, bail, Context, Result};
use snapfire_compiler_wire::{Hello, Outcome, Request, Response, Unit, PROTOCOL};

/// The extensions snapfirec hands to a plugin, and the binary each asks for.
/// A name is the convention rather than a lookup: `<ext>` is compiled by
/// `snapfirec-<ext>`.
const KNOWN: &[&str] = &["vue", "svelte"];

/// The extension as a static name when a plugin would claim it, which is what
/// makes such a file a source rather than an asset to copy.
pub fn claimed(ext: &str) -> Option<&'static str> {
  KNOWN.iter().find(|known| **known == ext).copied()
}

/// The binary an extension asks for.
pub fn binary_for(ext: &str) -> String {
  format!("snapfirec-{ext}")
}

/// What a reader types when the binary is not there. A plugin is a crate, the
/// way every other tool in this project is.
pub fn install_hint(ext: &str) -> String {
  format!("cargo install snapfire_{ext}")
}

struct Worker {
  child: Child,
  stdin: ChildStdin,
  stdout: BufReader<ChildStdout>,
  hello: Hello,
  next: u64,
}

impl Worker {
  fn start(ext: &str) -> Result<Self> {
    let binary = binary_for(ext);
    let mut child = Command::new(&binary)
      .stdin(Stdio::piped())
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .spawn()
      .map_err(|e| match e.kind() {
        // The kind is the whole of what a reader needs, and the os error under
        // it only repeats the sentence above in worse words.
        std::io::ErrorKind::NotFound => anyhow!("`{binary}` is not on PATH; `{}` puts it there", install_hint(ext)),
        _ => anyhow!("`{binary}` would not start: {e}"),
      })?;
    let stdin = child.stdin.take().ok_or_else(|| anyhow!("{binary}: no stdin"))?;
    let mut stdout = BufReader::new(child.stdout.take().ok_or_else(|| anyhow!("{binary}: no stdout"))?);

    let mut line = String::new();
    if stdout.read_line(&mut line).unwrap_or(0) == 0 {
      bail!("{binary}: exited before it said anything{}", stderr_of(&mut child));
    }
    let hello: Hello = serde_json::from_str(&line).with_context(|| format!("{binary}: its greeting did not parse: {line}"))?;
    if hello.protocol != PROTOCOL {
      bail!(
        "{binary} speaks plugin protocol {} and this snapfirec speaks {PROTOCOL}; update whichever is older",
        hello.protocol
      );
    }
    Ok(Self { child, stdin, stdout, hello, next: 1 })
  }

  fn compile(&mut self, units: Vec<Unit>) -> Result<Vec<Outcome>> {
    let id = self.next;
    self.next += 1;
    let wanted = units.len();
    let request = serde_json::to_string(&Request { id, units })?;
    writeln!(self.stdin, "{request}")
      .and_then(|()| self.stdin.flush())
      .with_context(|| format!("{}: the plugin closed its pipe{}", self.name(), stderr_of(&mut self.child)))?;

    let mut line = String::new();
    if self.stdout.read_line(&mut line).unwrap_or(0) == 0 {
      bail!("{}: died while compiling{}", self.name(), stderr_of(&mut self.child));
    }
    let response: Response = serde_json::from_str(&line).with_context(|| format!("{}: its answer did not parse", self.name()))?;
    if response.id != id {
      bail!("{}: answered batch {} when asked for {id}", self.name(), response.id);
    }
    if response.results.len() != wanted {
      bail!("{}: answered {} results for {wanted} units", self.name(), response.results.len());
    }
    Ok(response.results)
  }

  fn name(&self) -> String {
    binary_for(&self.hello.name)
  }
}

impl Drop for Worker {
  fn drop(&mut self) {
    // Closing the pipe is how a plugin is told the build is over; killing it is
    // only for one that did not take the hint.
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// Whatever the plugin printed before it stopped, as a suffix for the error
/// that is about to be raised. A crash the build reports without it is a
/// mystery the developer cannot act on.
fn stderr_of(child: &mut Child) -> String {
  let Some(mut err) = child.stderr.take() else { return String::new() };
  let mut text = String::new();
  use std::io::Read;
  let _ = err.read_to_string(&mut text);
  match text.trim().is_empty() {
    true => String::new(),
    false => format!(":\n{}", text.trim()),
  }
}

/// The workers this build has, one per extension, started on first use.
#[derive(Default)]
pub struct Plugins {
  workers: BTreeMap<String, Worker>,
  /// An extension whose plugin could not start, so the build says it once
  /// rather than once per file.
  refused: BTreeMap<String, String>,
}

impl Plugins {
  pub fn new() -> Self {
    Self::default()
  }

  /// The worker for `ext`, started on first use. What it said of itself is
  /// what a cache key needs before anything is sent.
  pub fn hello(&mut self, ext: &str) -> Result<Hello> {
    if let Some(why) = self.refused.get(ext) {
      bail!("{why}");
    }
    if !self.workers.contains_key(ext) {
      match Worker::start(ext) {
        Ok(worker) => {
          self.workers.insert(ext.to_owned(), worker);
        }
        Err(e) => {
          let why = format!("{e:#}");
          self.refused.insert(ext.to_owned(), why.clone());
          bail!("{why}");
        }
      }
    }
    Ok(self.workers[ext].hello.clone())
  }

  /// Compiles every unit of one extension in a single batch, which is what the
  /// long-lived worker is for.
  pub fn compile(&mut self, ext: &str, units: Vec<Unit>) -> Result<Vec<Outcome>> {
    self.hello(ext)?;
    self.workers.get_mut(ext).expect("just started").compile(units)
  }

  /// What each worker said it was, for the build's banner.
  pub fn report(&self) -> Vec<String> {
    self
      .workers
      .values()
      .map(|w| format!("{} {} ({})", w.name(), w.hello.version, w.hello.compiler))
      .collect()
  }
}

/// The extension of a path, lowercased, when a plugin would claim it.
pub fn plugin_ext(path: &Path) -> Option<&'static str> {
  let ext = path.extension()?.to_str()?.to_ascii_lowercase();
  claimed(&ext)
}
