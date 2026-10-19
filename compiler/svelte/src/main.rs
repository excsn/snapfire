//! `snapfirec-svelte`, spoken to over stdin and stdout.
//!
//! One JSON object per line in, one per line out, per
//! [`snapfire_compiler_wire`]. The process is spawned once per build and lives
//! across rebuilds.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use snapfire_compiler_wire::{Diagnostic, Hello, Kind, Outcome, Request, Response, PROTOCOL};
use snapfire_svelte::Compiler;

const NAME: &str = "svelte";

fn main() -> ExitCode {
  let mut args = std::env::args().skip(1);
  match args.next().as_deref() {
    Some("--version") => version(),
    Some(other) => {
      eprintln!("snapfirec-svelte: unknown argument `{other}`; it is spoken to over stdin, not run by hand");
      ExitCode::from(2)
    }
    None => serve(),
  }
}

fn version() -> ExitCode {
  match Compiler::new().and_then(|c| c.version()) {
    Ok(version) => {
      println!("snapfirec-svelte {} (svelte {version})", env!("CARGO_PKG_VERSION"));
      ExitCode::SUCCESS
    }
    Err(e) => {
      eprintln!("snapfirec-svelte: {e}");
      ExitCode::FAILURE
    }
  }
}

/// Boots the compiler before the greeting, so a host that reads a greeting
/// knows the worker is ready rather than merely running.
fn serve() -> ExitCode {
  let compiler = match Compiler::new() {
    Ok(compiler) => compiler,
    Err(e) => {
      eprintln!("snapfirec-svelte: {e}");
      return ExitCode::FAILURE;
    }
  };
  let carried = compiler.version().unwrap_or_else(|_| "unknown".to_owned());

  let stdout = std::io::stdout();
  let mut out = stdout.lock();
  let hello = Hello {
    protocol: PROTOCOL,
    name: NAME.to_owned(),
    version: env!("CARGO_PKG_VERSION").to_owned(),
    compiler: format!("svelte {carried}"),
    extensions: vec![".svelte".to_owned()],
  };
  if !say(&mut out, &hello) {
    return ExitCode::FAILURE;
  }

  for line in std::io::stdin().lock().lines() {
    let line = match line {
      Ok(line) => line,
      Err(e) => {
        eprintln!("snapfirec-svelte: reading the request: {e}");
        return ExitCode::FAILURE;
      }
    };
    if line.trim().is_empty() {
      continue;
    }
    let request: Request = match serde_json::from_str(&line) {
      Ok(request) => request,
      Err(e) => {
        eprintln!("snapfirec-svelte: the request did not parse: {e}");
        return ExitCode::FAILURE;
      }
    };
    let results = request
      .units
      .iter()
      .map(|unit| {
        let answered = match request.kind {
          Kind::Compile => compiler.compile(&unit.filename, &unit.source, &unit.options, &unit.files),
          Kind::Describe => Ok(Outcome::Failed { diagnostics: vec![Diagnostic::error("snapfirec-svelte does not describe a component yet; it renders in the browser").at(unit.filename.clone(), None, None)] }),
        };
        match answered {
          Ok(outcome) => outcome,
          // A thrown value stops this unit rather than the worker.
          Err(e) => Outcome::Failed { diagnostics: vec![Diagnostic::error(e.to_string()).at(unit.filename.clone(), None, None)] },
        }
      })
      .collect();
    if !say(&mut out, &Response { id: request.id, results }) {
      return ExitCode::FAILURE;
    }
  }
  ExitCode::SUCCESS
}

/// One object, one line, flushed.
fn say<T: serde::Serialize>(out: &mut impl Write, value: &T) -> bool {
  let line = match serde_json::to_string(value) {
    Ok(line) => line,
    Err(e) => {
      eprintln!("snapfirec-svelte: encoding the answer: {e}");
      return false;
    }
  };
  match writeln!(out, "{line}").and_then(|()| out.flush()) {
    Ok(()) => true,
    Err(e) => {
      eprintln!("snapfirec-svelte: writing the answer: {e}");
      false
    }
  }
}
