//! `snapfirec-vue`, spoken to over stdin and stdout.
//!
//! One JSON object per line in, one per line out, per
//! [`snapfire_plugin`]. The process is spawned once per build and lives across
//! rebuilds, so the second and later components pay a pipe write rather than
//! the cost of booting a compiler.

use std::io::{BufRead, Write};
use std::process::ExitCode;

use snapfire_plugin::{Diagnostic, Hello, Outcome, Request, Response, PROTOCOL};
use snapfire_vue::Compiler;

const NAME: &str = "vue";

fn main() -> ExitCode {
  let mut args = std::env::args().skip(1);
  match args.next().as_deref() {
    Some("--version") => version(),
    Some(other) => {
      eprintln!("snapfirec-vue: unknown argument `{other}`; it is spoken to over stdin, not run by hand");
      ExitCode::from(2)
    }
    None => serve(),
  }
}

fn version() -> ExitCode {
  match Compiler::new().and_then(|c| c.version()) {
    Ok(version) => {
      println!("snapfirec-vue {} (@vue/compiler-sfc {version})", env!("CARGO_PKG_VERSION"));
      ExitCode::SUCCESS
    }
    Err(e) => {
      eprintln!("snapfirec-vue: {e}");
      ExitCode::FAILURE
    }
  }
}

/// Boots the compiler, announces itself, then answers batches until stdin
/// closes. The boot happens before the greeting, so a host that reads a
/// greeting knows the worker is ready rather than merely running.
fn serve() -> ExitCode {
  let compiler = match Compiler::new() {
    Ok(compiler) => compiler,
    Err(e) => {
      eprintln!("snapfirec-vue: {e}");
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
    compiler: format!("@vue/compiler-sfc {carried}"),
    extensions: vec![".vue".to_owned()],
  };
  if !say(&mut out, &hello) {
    return ExitCode::FAILURE;
  }

  for line in std::io::stdin().lock().lines() {
    let line = match line {
      Ok(line) => line,
      Err(e) => {
        eprintln!("snapfirec-vue: reading the request: {e}");
        return ExitCode::FAILURE;
      }
    };
    if line.trim().is_empty() {
      continue;
    }
    let request: Request = match serde_json::from_str(&line) {
      Ok(request) => request,
      Err(e) => {
        eprintln!("snapfirec-vue: the request did not parse: {e}");
        return ExitCode::FAILURE;
      }
    };
    let results = request
      .units
      .iter()
      .map(|unit| match compiler.compile(&unit.filename, &unit.source, &unit.options) {
        Ok(outcome) => outcome,
        // A thrown value is the plugin's fault rather than the component's, and
        // it stops this unit rather than the worker: the next file may be fine
        // and a build that dies here loses every diagnostic it had gathered.
        Err(e) => Outcome::Failed {
          diagnostics: vec![Diagnostic::error(e.to_string()).at(unit.filename.clone(), None, None)],
        },
      })
      .collect();
    if !say(&mut out, &Response { id: request.id, results }) {
      return ExitCode::FAILURE;
    }
  }
  ExitCode::SUCCESS
}

/// One object, one line, flushed. A host blocked on a read never sees a
/// response sitting in a buffer.
fn say<T: serde::Serialize>(out: &mut impl Write, value: &T) -> bool {
  let line = match serde_json::to_string(value) {
    Ok(line) => line,
    Err(e) => {
      eprintln!("snapfirec-vue: encoding the answer: {e}");
      return false;
    }
  };
  match writeln!(out, "{line}").and_then(|()| out.flush()) {
    Ok(()) => true,
    Err(e) => {
      eprintln!("snapfirec-vue: writing the answer: {e}");
      false
    }
  }
}
