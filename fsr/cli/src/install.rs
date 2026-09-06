//! Offering to install a tool the command needs and cannot find.
//!
//! A terminal is asked. Anything else, a build script, CI, a container, is
//! told and never asked, since a prompt with nobody to answer it is a hang;
//! `FSR_NO_INSTALL` says the same for a terminal that would rather decide for
//! itself. What declining costs depends on the tool: an optional one is
//! skipped with a warning, a required one stops the command and says what it
//! was for.

use std::io::{IsTerminal, Write};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::BuildError;

pub struct Tool {
  pub binary: &'static str,
  pub package: &'static str,
  /// What the command wanted it for, said when declining has to stop it.
  pub needed_for: &'static str,
  /// Whether the command can go on without it.
  pub required: bool,
  /// The registry it is published to; crates.io when absent. Not every tool a
  /// project reaches for is public.
  pub registry: Option<&'static str>,
}

impl Tool {
  /// What a reader would type to get it.
  pub fn command(&self) -> String {
    match self.registry {
      Some(registry) => format!("cargo install {} --registry {registry}", self.package),
      None => format!("cargo install {}", self.package),
    }
  }
}

pub const COMPILER: Tool =
  Tool { binary: "snapfirec", package: "snapfire_compiler", needed_for: "compiling the browser bundle", required: true, registry: None };

pub const CHECKER: Tool =
  Tool { binary: "snapfiretc", package: "snapfire_typecheck", needed_for: "checking the application's types", required: false, registry: None };

pub enum Ready {
  /// The tool is there, either already or because it was just installed.
  Yes,
  /// It is not, and the command goes on without it.
  Skipped,
}

/// Whether `path` runs at all.
pub fn present(path: &Path) -> bool {
  Command::new(path).arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

pub fn ensure(tool: &Tool, path: &Path) -> Result<Ready, BuildError> {
  if present(path) {
    return Ok(Ready::Yes);
  }
  if asked(tool)? {
    install(tool)?;
    return Ok(Ready::Yes);
  }
  if tool.required {
    return Err(BuildError::Tool(format!(
      "`{}` is needed for {} and was not found; `{}`",
      tool.binary, tool.needed_for, tool.command()
    )));
  }
  eprintln!("note      {} is skipped: no `{}` on PATH; `{}`", tool.needed_for, tool.binary, tool.command());
  Ok(Ready::Skipped)
}

/// Whether the answer was yes. A prompt is only put to a terminal.
fn asked(tool: &Tool) -> Result<bool, BuildError> {
  if !std::io::stdin().is_terminal() || std::env::var_os("FSR_NO_INSTALL").is_some() {
    return Ok(false);
  }
  eprint!("no `{}` on PATH, which {} needs. install {} now? [Y/n] ", tool.binary, tool.needed_for, tool.package);
  std::io::stderr().flush().ok();
  let mut answer = String::new();
  if std::io::stdin().read_line(&mut answer).is_err() {
    return Ok(false);
  }
  Ok(matches!(answer.trim(), "" | "y" | "Y" | "yes" | "Yes"))
}

fn install(tool: &Tool) -> Result<(), BuildError> {
  let mut command = Command::new("cargo");
  command.args(["install", tool.package, "--locked"]);
  if let Some(registry) = tool.registry {
    command.args(["--registry", registry]);
  }
  let status = command
    .status()
    .map_err(|e| BuildError::Tool(format!("cargo install {}: {e}", tool.package)))?;
  if !status.success() {
    return Err(BuildError::Tool(format!("cargo install {} exited with {status}", tool.package)));
  }
  Ok(())
}
