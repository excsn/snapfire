//! `clipboard`: the system clipboard of the machine a terminal page runs on,
//! through the platform's own copy and paste commands. `Process` reach.

use std::io::Write;
use std::process::{Command, Stdio};

use crate::ext::{text, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::Value;

pub fn register(extensions: &mut Extensions) {
  extensions.register("clipboard.write", Reach::Process, |_, args| {
    let written = text("clipboard.write", args, 0)?;
    let tried = commands(std::env::consts::OS, std::env::var_os("WAYLAND_DISPLAY").is_some());
    first_that_runs(&tried, |(copy, _)| copy_through(copy, written)).map(|()| Value::Null)
  });
  extensions.register("clipboard.read", Reach::Process, |_, _| {
    let tried = commands(std::env::consts::OS, std::env::var_os("WAYLAND_DISPLAY").is_some());
    first_that_runs(&tried, |(_, paste)| paste_through(paste)).map(Value::str)
  });
}

/// A command line: the program and its arguments.
pub type CommandLine = &'static [&'static str];

/// The copy and paste commands to try on `os`, in order: `pbcopy` and `pbpaste` on macOS; `clip` and PowerShell's `Get-Clipboard` on Windows; on Linux `wl-copy` first under Wayland, then `xclip` and `xsel`.
pub fn commands(os: &str, wayland: bool) -> Vec<(CommandLine, CommandLine)> {
  const WAYLAND: (CommandLine, CommandLine) = (&["wl-copy"], &["wl-paste", "--no-newline"]);
  const XCLIP: (CommandLine, CommandLine) = (&["xclip", "-selection", "clipboard"], &["xclip", "-selection", "clipboard", "-o"]);
  const XSEL: (CommandLine, CommandLine) = (&["xsel", "--clipboard", "--input"], &["xsel", "--clipboard", "--output"]);
  match os {
    "macos" => vec![(&["pbcopy"], &["pbpaste"])],
    "windows" => vec![(&["clip"], &["powershell", "-NoProfile", "-Command", "[Console]::Out.Write((Get-Clipboard -Raw))"])],
    _ if wayland => vec![WAYLAND, XCLIP, XSEL],
    _ => vec![XCLIP, XSEL, WAYLAND],
  }
}

/// What `run` answers for the first of `tried` whose program is installed, or a failure naming every program tried.
fn first_that_runs<T>(tried: &[(CommandLine, CommandLine)], run: impl Fn(&(CommandLine, CommandLine)) -> Result<T, std::io::Error>) -> Result<T, Fail> {
  for commands in tried {
    match run(commands) {
      Ok(done) => return Ok(done),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
      Err(e) => return Err(Fail::internal(format!("clipboard: {}: {e}", commands.0[0]))),
    }
  }
  let names: Vec<&str> = tried.iter().map(|(copy, _)| copy[0]).collect();
  Err(Fail::internal(format!("clipboard: none of {} is installed", names.join(", "))))
}

/// Runs `command` with `text` on its standard input.
pub fn copy_through(command: &[&str], text: &str) -> Result<(), std::io::Error> {
  let mut child = Command::new(command[0]).args(&command[1..]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
  child.stdin.take().expect("stdin is piped").write_all(text.as_bytes())?;
  let status = child.wait()?;
  match status.success() {
    true => Ok(()),
    false => Err(std::io::Error::other(format!("exited with {status}"))),
  }
}

/// What `command` writes to its standard output.
pub fn paste_through(command: &[&str]) -> Result<String, std::io::Error> {
  let out = Command::new(command[0]).args(&command[1..]).stdin(Stdio::null()).stderr(Stdio::null()).output()?;
  match out.status.success() {
    true => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
    false => Err(std::io::Error::other(format!("exited with {}", out.status))),
  }
}
