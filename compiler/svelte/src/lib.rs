//! Svelte components compiled without a JavaScript runtime on the machine:
//! the crate carries Svelte's own compiler bundle and runs it in QuickJS.
//!
//! The bundle is a UMD script that sets `globalThis.svelte` and imports
//! nothing, so it is evaluated as a script rather than declared as a module.

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use rquickjs::{Context, Function, Runtime};
use snapfire_compiler_wire::{Compiled, Diagnostic, Lang, Options, Outcome};

/// `svelte/compiler` 5.57.2 as published. Never edited here.
const COMPILER: &str = include_str!("../vendor/compiler.js");

const DRIVER: &str = include_str!("driver.js");

/// The compiler's parser and its walkers recurse once per nesting level and
/// QuickJS refuses at 256 KiB by default. The engine runs on a thread with
/// room to spare and is told how much of it to use.
const THREAD_STACK: usize = 64 << 20;
const ENGINE_STACK: usize = 48 << 20;

#[derive(Debug, thiserror::Error)]
pub enum SvelteError {
  #[error("the Svelte compiler: {0}")]
  Js(String),
}

enum Ask {
  Version(Sender<Result<String, SvelteError>>),
  Compile { filename: String, source: String, options: Options, reply: Sender<Result<Outcome, SvelteError>> },
}

/// One QuickJS context holding the compiler, owned by the thread it runs on.
pub struct Compiler {
  asks: Sender<Ask>,
  thread: Option<JoinHandle<()>>,
}

impl Compiler {
  pub fn new() -> Result<Self, SvelteError> {
    let (asks, inbox) = channel::<Ask>();
    let (booted, boot) = channel::<Result<(), SvelteError>>();
    let thread = std::thread::Builder::new()
      .name("snapfire-svelte".to_owned())
      .stack_size(THREAD_STACK)
      .spawn(move || serve(inbox, booted))
      .map_err(|e| SvelteError::Js(format!("the compiler thread would not start: {e}")))?;
    boot.recv().map_err(|_| SvelteError::Js("the compiler thread died while booting".to_owned()))??;
    Ok(Self { asks, thread: Some(thread) })
  }

  /// What the carried compiler reports itself as.
  pub fn version(&self) -> Result<String, SvelteError> {
    let (reply, answer) = channel();
    self.asks.send(Ask::Version(reply)).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// One component, compiled for the browser. `filename` is what a diagnostic
  /// names it, so it should be relative to the project. A Svelte component
  /// reads no other file, so `files` is never consulted.
  pub fn compile(&self, filename: &str, source: &str, options: &Options, _files: &BTreeMap<String, String>) -> Result<Outcome, SvelteError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Compile { filename: filename.to_owned(), source: source.to_owned(), options: options.clone(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }
}

impl Drop for Compiler {
  fn drop(&mut self) {
    let (asks, _) = channel();
    drop(std::mem::replace(&mut self.asks, asks));
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

fn gone() -> SvelteError {
  SvelteError::Js("the compiler thread is gone".to_owned())
}

fn serve(inbox: Receiver<Ask>, booted: Sender<Result<(), SvelteError>>) {
  let engine = match Engine::new() {
    Ok(engine) => engine,
    Err(e) => {
      let _ = booted.send(Err(e));
      return;
    }
  };
  let _ = booted.send(Ok(()));
  for ask in inbox {
    match ask {
      Ask::Version(reply) => {
        let _ = reply.send(engine.version());
      }
      Ask::Compile { filename, source, options, reply } => {
        let _ = reply.send(engine.compile(&filename, &source, &options));
      }
    }
  }
}

struct Engine {
  context: Context,
  _runtime: Runtime,
}

impl Engine {
  fn new() -> Result<Self, SvelteError> {
    let runtime = Runtime::new().map_err(js)?;
    runtime.set_max_stack_size(ENGINE_STACK);
    let context = Context::full(&runtime).map_err(js)?;
    context.with(|ctx| -> Result<(), SvelteError> {
      ctx.eval::<(), _>(COMPILER).map_err(|e| threw(&ctx, e))?;
      ctx.eval::<(), _>(DRIVER).map_err(|e| threw(&ctx, e))?;
      Ok(())
    })?;
    Ok(Self { context, _runtime: runtime })
  }

  fn version(&self) -> Result<String, SvelteError> {
    self.context.with(|ctx| {
      let version: Function = ctx.globals().get("__svelte_version").map_err(|e| threw(&ctx, e))?;
      version.call(()).map_err(|e| threw(&ctx, e))
    })
  }

  fn compile(&self, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError> {
    let answer = self.context.with(|ctx| -> Result<String, SvelteError> {
      let function: Function = ctx.globals().get("__svelte_compile").map_err(|e| threw(&ctx, e))?;
      let options = serde_json::to_string(options).map_err(|e| SvelteError::Js(e.to_string()))?;
      let options = ctx.json_parse(options).map_err(|e| threw(&ctx, e))?;
      function.call((filename, source, options)).map_err(|e| threw(&ctx, e))
    })?;
    decode(&answer, filename)
  }
}

/// The driver answers one JSON object. A shape it cannot produce is a bug in
/// the driver rather than in the component, so it is reported as one.
fn decode(answer: &str, filename: &str) -> Result<Outcome, SvelteError> {
  #[derive(serde::Deserialize)]
  struct Answer {
    status: String,
    #[serde(default)]
    js: String,
    #[serde(default)]
    css: Option<String>,
    #[serde(default)]
    source_map: Option<String>,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
  }
  let answer: Answer = serde_json::from_str(answer).map_err(|e| SvelteError::Js(format!("{filename}: the driver answered {e}")))?;
  match answer.status.as_str() {
    "ok" => Ok(Outcome::Ok(Compiled { js: answer.js, lang: Lang::Js, css: answer.css, source_map: answer.source_map, deps: Vec::new(), diagnostics: answer.diagnostics })),
    _ => {
      let diagnostics = match answer.diagnostics.is_empty() {
        true => vec![Diagnostic::error("the component did not compile").at(filename, None, None)],
        false => answer.diagnostics,
      };
      Ok(Outcome::Failed { diagnostics })
    }
  }
}

fn js(e: rquickjs::Error) -> SvelteError {
  SvelteError::Js(e.to_string())
}

/// A thrown value with its message and stack, rather than `Error: exception`.
fn threw(ctx: &rquickjs::Ctx<'_>, e: rquickjs::Error) -> SvelteError {
  match e {
    rquickjs::Error::Exception => {
      let caught = ctx.catch();
      match caught.as_exception() {
        Some(exception) => {
          let message = exception.message().unwrap_or_default();
          match exception.stack() {
            Some(stack) => SvelteError::Js(format!("{message}\n{stack}")),
            None => SvelteError::Js(message),
          }
        }
        None => SvelteError::Js(format!("{caught:?}")),
      }
    }
    other => SvelteError::Js(other.to_string()),
  }
}
