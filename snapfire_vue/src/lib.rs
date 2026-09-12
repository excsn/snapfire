//! Vue single-file components compiled without a JavaScript runtime on the
//! machine: the crate carries Vue's own browser build of `@vue/compiler-sfc`
//! and runs it in QuickJS.
//!
//! The browser build imports no modules and shims `process` itself. Its one
//! reach outside the source it is handed is type resolution for a
//! `defineProps<T>()` whose `T` is imported, which it takes through an `fs`
//! object on the compile options rather than through a module, so the host
//! answers those reads and the plugin never opens a file it was not asked to.

use rquickjs::{Context, Function, Module, Object, Runtime, Value};

/// Vue's browser build, as published. Never edited here.
const COMPILER: &str = include_str!("../vendor/compiler-sfc.esm-browser.js");

/// The name the bundle is declared under, which is what a stack frame says.
const MODULE: &str = "@vue/compiler-sfc";

#[derive(Debug, thiserror::Error)]
pub enum VueError {
  #[error("the Vue compiler: {0}")]
  Js(String),
  #[error("{0}")]
  Compile(String),
}

pub struct Compiler {
  context: Context,
  /// Held so the context outlives nothing: a `Context` keeps the runtime alive
  /// itself, and naming it here keeps the ownership legible.
  _runtime: Runtime,
}

impl Compiler {
  /// Boots one QuickJS context with the compiler evaluated in it. Expensive
  /// once and free afterwards, which is why a plugin worker is long-lived.
  pub fn new() -> Result<Self, VueError> {
    let runtime = Runtime::new().map_err(js)?;
    let context = Context::full(&runtime).map_err(js)?;
    context.with(|ctx| -> Result<(), VueError> {
      let (_, promise) = Module::declare(ctx.clone(), MODULE, COMPILER)
        .and_then(|m| m.eval())
        .map_err(|e| threw(&ctx, e))?;
      promise.finish::<()>().map_err(|e| threw(&ctx, e))?;
      Ok(())
    })?;
    Ok(Self { context, _runtime: runtime })
  }

  /// The `version` the carried compiler reports, which is half of a plugin's
  /// cache key and the whole of what a mismatched bug report needs.
  pub fn version(&self) -> Result<String, VueError> {
    let quoted = self.eval("return sfc.version;")?;
    Ok(quoted.trim_matches('"').to_owned())
  }

  /// Evaluates `source` with the compiler's namespace bound as `sfc`, which is
  /// how every call into it is made until the protocol settles.
  pub fn eval(&self, source: &str) -> Result<String, VueError> {
    self.context.with(|ctx| {
      let wrapped = format!("import * as sfc from \"{MODULE}\";\nglobalThis.__out = (() => {{\n{source}\n}})();\n");
      let (_, promise) = Module::declare(ctx.clone(), "__probe__", wrapped)
        .and_then(|m| m.eval())
        .map_err(|e| threw(&ctx, e))?;
      promise.finish::<()>().map_err(|e| threw(&ctx, e))?;
      let out: Value = ctx.globals().get("__out").map_err(|e| threw(&ctx, e))?;
      let json: Function = ctx
        .globals()
        .get::<_, Object>("JSON")
        .and_then(|j| j.get("stringify"))
        .map_err(|e| threw(&ctx, e))?;
      json.call((out,)).map_err(|e| threw(&ctx, e))
    })
  }
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
