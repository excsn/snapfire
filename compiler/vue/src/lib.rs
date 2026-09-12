//! Vue single-file components compiled without a JavaScript runtime on the
//! machine: the crate carries Vue's own browser build of `@vue/compiler-sfc`
//! and runs it in QuickJS.
//!
//! The browser build imports no modules and shims `process` itself. Its one
//! reach outside the source it is handed is type resolution for a
//! `defineProps<T>()` whose `T` is imported, which it takes through an `fs`
//! object on the compile options rather than through a module, so the host
//! answers those reads and the plugin never opens a file it was not asked to.

use rquickjs::{Context, Function, Module, Runtime};
use snapfire_plugin::{Compiled, Diagnostic, Lang, Options, Outcome, Severity};

/// Vue's browser build, as published. Never edited here.
const COMPILER: &str = include_str!("../vendor/compiler-sfc.esm-browser.js");

/// The four stages, in the order the emitted module needs them.
const DRIVER: &str = include_str!("driver.js");

/// The name the bundle is declared under, which is what a stack frame says.
const MODULE: &str = "@vue/compiler-sfc";

#[derive(Debug, thiserror::Error)]
pub enum VueError {
  #[error("the Vue compiler: {0}")]
  Js(String),
}

pub struct Compiler {
  context: Context,
  /// A `Context` keeps the runtime alive itself; naming it here keeps the
  /// ownership legible rather than implied.
  _runtime: Runtime,
}

impl Compiler {
  /// Boots one QuickJS context with the compiler and the driver in it.
  /// Expensive once and cheap per file afterwards, which is the whole reason
  /// a plugin worker is long-lived.
  pub fn new() -> Result<Self, VueError> {
    let runtime = Runtime::new().map_err(js)?;
    let context = Context::full(&runtime).map_err(js)?;
    context.with(|ctx| -> Result<(), VueError> {
      declare(&ctx, MODULE, COMPILER)?;
      declare(&ctx, "__driver__", DRIVER)?;
      Ok(())
    })?;
    Ok(Self { context, _runtime: runtime })
  }

  /// What the carried compiler reports itself as, which is half of a cache key
  /// and the whole of what a bug report needs.
  pub fn version(&self) -> Result<String, VueError> {
    self.context.with(|ctx| {
      let source = format!("import * as sfc from \"{MODULE}\";\nglobalThis.__version = sfc.version;\n");
      declare(&ctx, "__version__", &source)?;
      ctx.globals().get("__version").map_err(|e| threw(&ctx, e))
    })
  }

  /// One component. `filename` is what a diagnostic names it, so it should be
  /// relative to the project rather than absolute.
  pub fn compile(&self, filename: &str, source: &str, options: &Options) -> Result<Outcome, VueError> {
    let answer = self.context.with(|ctx| -> Result<String, VueError> {
      let compile: Function = ctx.globals().get("__vue_compile").map_err(|e| threw(&ctx, e))?;
      let options = serde_json::to_string(options).map_err(|e| VueError::Js(e.to_string()))?;
      let options = ctx
        .json_parse(options)
        .map_err(|e| threw(&ctx, e))?;
      compile.call((filename, source, options)).map_err(|e| threw(&ctx, e))
    })?;
    decode(&answer, filename)
  }
}

/// The driver answers one JSON object. A shape it cannot produce is a bug in
/// the driver rather than in the component, so it is reported as one.
fn decode(answer: &str, filename: &str) -> Result<Outcome, VueError> {
  #[derive(serde::Deserialize)]
  struct Answer {
    status: String,
    #[serde(default)]
    js: String,
    #[serde(default)]
    lang: Lang,
    #[serde(default)]
    css: Option<String>,
    #[serde(default)]
    deps: Vec<String>,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
  }
  let answer: Answer = serde_json::from_str(answer).map_err(|e| VueError::Js(format!("{filename}: the driver answered {e}")))?;
  match answer.status.as_str() {
    "ok" => Ok(Outcome::Ok(Compiled {
      js: answer.js,
      lang: answer.lang,
      css: answer.css,
      source_map: None,
      deps: answer.deps,
      diagnostics: answer.diagnostics,
    })),
    _ => {
      let diagnostics = match answer.diagnostics.is_empty() {
        true => vec![Diagnostic::error("the component did not compile").at(filename, None, None)],
        false => answer.diagnostics,
      };
      Ok(Outcome::Failed { diagnostics })
    }
  }
}

/// Whether any of these stops a build.
pub fn fatal(diagnostics: &[Diagnostic]) -> bool {
  diagnostics.iter().any(|d| d.severity == Severity::Error)
}

fn declare(ctx: &rquickjs::Ctx<'_>, name: &str, source: &str) -> Result<(), VueError> {
  let (_, promise) = Module::declare(ctx.clone(), name, source)
    .and_then(|m| m.eval())
    .map_err(|e| threw(ctx, e))?;
  promise.finish::<()>().map_err(|e| threw(ctx, e))
}

fn js(e: rquickjs::Error) -> VueError {
  VueError::Js(e.to_string())
}

/// A thrown value with its message and stack, rather than `Error: exception`.
fn threw(ctx: &rquickjs::Ctx<'_>, e: rquickjs::Error) -> VueError {
  match e {
    rquickjs::Error::Exception => {
      let caught = ctx.catch();
      match caught.as_exception() {
        Some(exception) => {
          let message = exception.message().unwrap_or_default();
          match exception.stack() {
            Some(stack) => VueError::Js(format!("{message}\n{stack}")),
            None => VueError::Js(message),
          }
        }
        None => VueError::Js(format!("{caught:?}")),
      }
    }
    other => VueError::Js(other.to_string()),
  }
}
