//! The plugin as a host sees it: a process on the other end of a pipe,
//! spawned once and kept for as long as the host has work.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use crate::{binary_for, install_hint, Hello, Kind, Outcome, Request, Response, Unit, PROTOCOL};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
  #[error("`{binary}` is not on PATH; `{hint}` puts it there")]
  NotFound { binary: String, hint: String },
  #[error("`{binary}` would not start: {error}")]
  Spawn { binary: String, error: std::io::Error },
  #[error("{binary}: exited before it said anything{stderr}")]
  Silent { binary: String, stderr: String },
  #[error("{binary}: its greeting did not parse: {line}")]
  Greeting { binary: String, line: String },
  #[error("{binary} speaks plugin protocol {theirs} and this host speaks {ours}; update whichever is older")]
  Protocol { binary: String, theirs: u32, ours: u32 },
  #[error("{binary}: the plugin closed its pipe{stderr}")]
  Closed { binary: String, stderr: String },
  #[error("{binary}: died while answering{stderr}")]
  Died { binary: String, stderr: String },
  #[error("{binary}: its answer did not parse: {error}")]
  Answer { binary: String, error: serde_json::Error },
  #[error("{binary}: answered batch {got} when asked for {wanted}")]
  Batch { binary: String, got: u64, wanted: u64 },
  #[error("{binary}: answered {got} results for {wanted} units")]
  Count { binary: String, got: usize, wanted: usize },
}

/// One plugin process. Dropping it closes the pipe, which is how the plugin
/// is told the work is over; a plugin that does not take the hint is killed.
pub struct Worker {
  child: Child,
  stdin: ChildStdin,
  stdout: BufReader<ChildStdout>,
  hello: Hello,
  next: u64,
}

impl Worker {
  /// Spawns `snapfirec-<ext>` and reads its greeting, refusing a protocol
  /// other than this crate's by name.
  pub fn start(ext: &str) -> Result<Self, HostError> {
    let binary = binary_for(ext);
    let mut child = Command::new(&binary).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|error| match error.kind() {
      std::io::ErrorKind::NotFound => HostError::NotFound { binary: binary.clone(), hint: install_hint(ext) },
      _ => HostError::Spawn { binary: binary.clone(), error },
    })?;
    let stdin = child.stdin.take().expect("stdin is piped");
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout is piped"));
    let mut line = String::new();
    if stdout.read_line(&mut line).unwrap_or(0) == 0 {
      return Err(HostError::Silent { binary, stderr: stderr_of(&mut child) });
    }
    let hello: Hello = serde_json::from_str(&line).map_err(|_| HostError::Greeting { binary: binary.clone(), line: line.clone() })?;
    if hello.protocol != PROTOCOL {
      return Err(HostError::Protocol { binary, theirs: hello.protocol, ours: PROTOCOL });
    }
    Ok(Self { child, stdin, stdout, hello, next: 1 })
  }

  /// What the plugin said of itself.
  pub fn hello(&self) -> &Hello {
    &self.hello
  }

  /// The binary's name, for a message.
  pub fn name(&self) -> String {
    binary_for(&self.hello.name)
  }

  /// Compiles every unit in one batch.
  pub fn compile(&mut self, units: Vec<Unit>) -> Result<Vec<Outcome>, HostError> {
    self.ask(Kind::Compile, units)
  }

  /// Describes every unit in one batch: each answer is [`Outcome::Described`]
  /// or one of the refusals.
  pub fn describe(&mut self, units: Vec<Unit>) -> Result<Vec<Outcome>, HostError> {
    self.ask(Kind::Describe, units)
  }

  fn ask(&mut self, kind: Kind, units: Vec<Unit>) -> Result<Vec<Outcome>, HostError> {
    let id = self.next;
    self.next += 1;
    let wanted = units.len();
    let request = serde_json::to_string(&Request { id, kind, units }).expect("a request serializes");
    if writeln!(self.stdin, "{request}").and_then(|()| self.stdin.flush()).is_err() {
      return Err(HostError::Closed { binary: self.name(), stderr: stderr_of(&mut self.child) });
    }
    let mut line = String::new();
    if self.stdout.read_line(&mut line).unwrap_or(0) == 0 {
      return Err(HostError::Died { binary: self.name(), stderr: stderr_of(&mut self.child) });
    }
    let response: Response = serde_json::from_str(&line).map_err(|error| HostError::Answer { binary: self.name(), error })?;
    if response.id != id {
      return Err(HostError::Batch { binary: self.name(), got: response.id, wanted: id });
    }
    if response.results.len() != wanted {
      return Err(HostError::Count { binary: self.name(), got: response.results.len(), wanted });
    }
    Ok(response.results)
  }
}

impl Drop for Worker {
  fn drop(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

/// Whatever the plugin printed before it stopped, as a suffix for the error
/// about to be raised.
fn stderr_of(child: &mut Child) -> String {
  let Some(mut err) = child.stderr.take() else { return String::new() };
  let mut text = String::new();
  let _ = err.read_to_string(&mut text);
  match text.trim().is_empty() {
    true => String::new(),
    false => format!(":\n{}", text.trim()),
  }
}
