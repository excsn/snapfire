//! Framework compilers, found on PATH and spoken to over a pipe.
//!
//! `snapfirec` gains no dependency, no engine and no size from a framework it
//! does not compile: a `.vue` file asks for `snapfirec-vue` the way `git` asks
//! for `git-lfs`, and someone who never writes one never learns the plugin
//! exists.
//!
//! A worker is spawned once and kept for the life of the build, watch mode
//! included, because spawning per file is fatal when a project has hundreds of
//! components and a boot costs tens of milliseconds. Under `--driven` the
//! driver owns the workers and the compiler asks it, so one worker per
//! extension serves everything the driver does with a component as well.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{bail, Result};
use snapfire_compiler_wire::driven::{PluginAnswer, PluginRequest, PLUGIN};
use snapfire_compiler_wire::host::Worker;
use snapfire_compiler_wire::{Hello, Outcome, Unit};

/// A claimed extension makes a file a source rather than an asset to copy.
pub use snapfire_compiler_wire::claimed;

/// The workers this build has, one per extension, started on first use.
#[derive(Default)]
pub struct Plugins {
  workers: BTreeMap<String, Worker>,
  /// An extension whose plugin could not start, so the build says it once
  /// rather than once per file.
  refused: BTreeMap<String, String>,
  /// What the driver's plugin said of itself, per extension, when the driver owns the workers.
  driven: Option<BTreeMap<String, Hello>>,
}

impl Plugins {
  pub fn new() -> Self {
    Self::default()
  }

  /// A pool that asks the driver, over stdout and stdin, for each plugin.
  pub fn driven() -> Self {
    Self { driven: Some(BTreeMap::new()), ..Self::default() }
  }

  /// Writes `request` for the driver and reads its answer. Stdin is read a line at a time, so the
  /// driven loop and a plugin request share it without either holding it.
  fn ask(request: &PluginRequest) -> Result<PluginAnswer> {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    writeln!(out, "{PLUGIN}")?;
    writeln!(out, "{}", serde_json::to_string(request)?)?;
    out.flush()?;
    drop(out);
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line)? == 0 {
      bail!("the driver went away before answering a plugin request");
    }
    Ok(serde_json::from_str(&line)?)
  }

  /// The worker for `ext`, started on first use. What it said of itself is
  /// what a cache key needs before anything is sent.
  pub fn hello(&mut self, ext: &str) -> Result<Hello> {
    if let Some(why) = self.refused.get(ext) {
      bail!("{why}");
    }
    if let Some(hellos) = &mut self.driven {
      if let Some(hello) = hellos.get(ext) {
        return Ok(hello.clone());
      }
      return match Self::ask(&PluginRequest::Hello { ext: ext.to_owned() })? {
        PluginAnswer::Hello { hello } => {
          hellos.insert(ext.to_owned(), hello.clone());
          Ok(hello)
        }
        PluginAnswer::Refused { why } => {
          self.refused.insert(ext.to_owned(), why.clone());
          bail!("{why}");
        }
        PluginAnswer::Compiled { .. } => bail!("the driver answered a hello with outcomes"),
      };
    }
    if !self.workers.contains_key(ext) {
      match Worker::start(ext) {
        Ok(worker) => {
          self.workers.insert(ext.to_owned(), worker);
        }
        Err(e) => {
          let why = e.to_string();
          self.refused.insert(ext.to_owned(), why.clone());
          bail!("{why}");
        }
      }
    }
    Ok(self.workers[ext].hello().clone())
  }

  /// Compiles every unit of one extension in a single batch, which is what the
  /// long-lived worker is for.
  pub fn compile(&mut self, ext: &str, units: Vec<Unit>) -> Result<Vec<Outcome>> {
    self.hello(ext)?;
    if self.driven.is_some() {
      return match Self::ask(&PluginRequest::Compile { ext: ext.to_owned(), units })? {
        PluginAnswer::Compiled { outcomes } => Ok(outcomes),
        PluginAnswer::Refused { why } => bail!("{why}"),
        PluginAnswer::Hello { .. } => bail!("the driver answered a compile with a hello"),
      };
    }
    Ok(self.workers.get_mut(ext).expect("just started").compile(units)?)
  }

  /// What each worker said it was, for the build's banner.
  pub fn report(&self) -> Vec<String> {
    if let Some(hellos) = &self.driven {
      return hellos.values().map(|hello| format!("{} {} ({}), through the driver", hello.name, hello.version, hello.compiler)).collect();
    }
    self
      .workers
      .values()
      .map(|w| format!("{} {} ({})", w.name(), w.hello().version, w.hello().compiler))
      .collect()
  }
}

/// The extension of a path, lowercased, when a plugin would claim it.
pub fn plugin_ext(path: &Path) -> Option<&'static str> {
  let ext = path.extension()?.to_str()?.to_ascii_lowercase();
  claimed(&ext)
}
