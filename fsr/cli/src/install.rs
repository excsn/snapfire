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

#[non_exhaustive]
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
  /// The earliest release the command works with, read from the last word `--version` prints.
  pub floor: Option<&'static str>,
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

/// The earliest `snapfirec` this fsr builds with. Raised with the compiler's release when fsr
/// starts relying on what that release emits.
pub const COMPILER_FLOOR: &str = "0.18.0";

pub const COMPILER: Tool =
  Tool { binary: "snapfirec", package: "snapfire_compiler", needed_for: "compiling the browser bundle", required: true, registry: None, floor: Some(COMPILER_FLOOR) };

pub const CHECKER: Tool =
  Tool { binary: "snapfiretc", package: "snapfire_typecheck", needed_for: "checking the application's types", required: false, registry: None, floor: None };

#[non_exhaustive]
pub enum Ready {
  /// The tool is there, either already or because it was just installed.
  Yes,
  /// It is not and the command goes on without it.
  Skipped,
}

/// Whether `path` runs at all.
pub fn present(path: &Path) -> bool {
  Command::new(path).arg("--version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

pub fn ensure(tool: &Tool, path: &Path) -> Result<Ready, BuildError> {
  if present(path) {
    recent(tool, path)?;
    return Ok(Ready::Yes);
  }
  if asked(tool)? {
    install(tool)?;
    recent(tool, path)?;
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

/// Refuses a release older than the tool's floor.
fn recent(tool: &Tool, path: &Path) -> Result<(), BuildError> {
  use crate::types::range::Version;
  let Some(floor) = tool.floor else {
    return Ok(());
  };
  let output = Command::new(path).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| BuildError::Tool(format!("{} --version: {e}", path.display())))?;
  let printed = String::from_utf8_lossy(&output.stdout).trim().to_owned();
  let least = Version::parse(floor).expect("a floor is a version");
  match printed.split_whitespace().last().and_then(Version::parse) {
    Some(found) if found >= least => Ok(()),
    Some(found) => Err(BuildError::Tool(format!("{} is {} {found} and this fsr needs {floor} or later; `{}`", path.display(), tool.binary, tool.command()))),
    None => Err(BuildError::Tool(format!("{} --version printed `{printed}`, which names no release; this fsr needs {} {floor} or later; `{}`", path.display(), tool.binary, tool.command()))),
  }
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

#[cfg(all(test, unix))]
mod tests {
  use super::*;

  fn stub(name: &str, version: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!("fsr-install-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("snapfirec");
    std::fs::write(&path, format!("#!/bin/sh\necho '{version}'\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
  }

  #[test]
  fn a_compiler_older_than_the_floor_is_refused_with_the_install_command() {
    let path = stub("older", "snapfire_compiler 0.1.0");
    let Err(BuildError::Tool(message)) = ensure(&COMPILER, &path) else { panic!("an older compiler was accepted") };
    assert!(message.contains("0.1.0") && message.contains(COMPILER_FLOOR) && message.contains("cargo install snapfire_compiler"), "{message}");
  }

  #[test]
  fn a_compiler_at_the_floor_is_accepted() {
    let path = stub("floor", &format!("snapfire_compiler {COMPILER_FLOOR}"));
    assert!(matches!(ensure(&COMPILER, &path), Ok(Ready::Yes)));
  }

  #[test]
  fn a_compiler_that_names_no_version_is_refused() {
    let path = stub("unnamed", "snapfirec");
    let Err(BuildError::Tool(message)) = ensure(&COMPILER, &path) else { panic!("a compiler naming no version was accepted") };
    assert!(message.contains(COMPILER_FLOOR), "{message}");
  }
}
