//! The framework plugins one command uses: one worker per extension, started
//! the first time something needs it and kept until the command ends. The
//! build's reading of a component and every compile the driven compiler asks
//! for go to that same worker, so `fsr build`, `fsr dev` and `fsr test` start
//! each plugin once rather than once per tool and per rebuild.

use std::collections::BTreeMap;

use snapfire_compiler_wire::driven::{PluginAnswer, PluginRequest};
use snapfire_compiler_wire::host::{HostError, Worker};
use snapfire_compiler_wire::{Outcome, Unit};

/// Why a plugin cannot answer, remembered so a command says it once.
#[derive(Debug, Clone)]
pub enum Refusal {
  /// The plugin's binary is not on `PATH`; `hint` is the command that installs it.
  Missing { binary: String, hint: String },
  /// It started and failed, or would not start for another reason.
  Failed(String),
}

impl std::fmt::Display for Refusal {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Missing { binary, hint } => write!(f, "`{binary}` is not on PATH; `{hint}` puts it there"),
      Self::Failed(why) => f.write_str(why),
    }
  }
}

#[derive(Default)]
pub struct Plugins {
  workers: BTreeMap<String, Worker>,
  refused: BTreeMap<String, Refusal>,
}

impl Plugins {
  pub fn new() -> Self {
    Self::default()
  }

  fn worker(&mut self, ext: &str) -> Result<&mut Worker, Refusal> {
    if let Some(refusal) = self.refused.get(ext) {
      return Err(refusal.clone());
    }
    if !self.workers.contains_key(ext) {
      let worker = Worker::start(ext).map_err(|e| {
        let refusal = match e {
          HostError::NotFound { binary, hint } => Refusal::Missing { binary, hint },
          other => Refusal::Failed(other.to_string()),
        };
        self.refused.insert(ext.to_owned(), refusal.clone());
        refusal
      })?;
      self.workers.insert(ext.to_owned(), worker);
    }
    Ok(self.workers.get_mut(ext).expect("just started"))
  }

  /// The plugin's name for `ext`, for a message about something it said.
  pub(crate) fn name(&mut self, ext: &str) -> String {
    self.worker(ext).map(|worker| worker.name().to_owned()).unwrap_or_else(|_| format!("the `.{ext}` plugin"))
  }

  /// Each unit as the plugin reads it. A worker that dies is dropped, so the next request starts it again.
  pub(crate) fn describe(&mut self, ext: &str, units: Vec<Unit>) -> Result<Vec<Outcome>, Refusal> {
    let answered = self.worker(ext)?.describe(units);
    answered.map_err(|e| {
      self.workers.remove(ext);
      Refusal::Failed(e.to_string())
    })
  }

  /// Answers one request the driven compiler made.
  pub(crate) fn answer(&mut self, request: PluginRequest) -> PluginAnswer {
    match request {
      PluginRequest::Hello { ext } => match self.worker(&ext) {
        Ok(worker) => PluginAnswer::Hello { hello: worker.hello().clone() },
        Err(refusal) => PluginAnswer::Refused { why: refusal.to_string() },
      },
      PluginRequest::Compile { ext, units } => {
        let answered = match self.worker(&ext) {
          Ok(worker) => worker.compile(units).map_err(|e| e.to_string()),
          Err(refusal) => return PluginAnswer::Refused { why: refusal.to_string() },
        };
        match answered {
          Ok(outcomes) => PluginAnswer::Compiled { outcomes },
          Err(why) => {
            self.workers.remove(&ext);
            PluginAnswer::Refused { why }
          }
        }
      }
    }
  }
}
