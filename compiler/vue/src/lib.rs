//! Vue single-file components compiled without a JavaScript runtime on the
//! machine: the crate carries Vue's own browser build of `@vue/compiler-sfc`
//! and runs it in QuickJS.
//!
//! The browser build imports no modules and shims `process` itself. Its one
//! reach outside the source it is handed is type resolution for a
//! `defineProps<T>()` whose `T` is imported, which it takes through an `fs`
//! object on the compile options rather than through a module, so the host
//! answers those reads and the plugin never opens a file it was not asked to.

use std::collections::BTreeMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use rquickjs::loader::{BuiltinLoader, BuiltinResolver};
use rquickjs::{Context, Function, Module, Promise, Runtime};
use snapfire_compiler_wire::{Compiled, Described, Diagnostic, Lang, Options, Outcome, Severity};

/// Vue's browser build, as published. Never edited here.
const COMPILER: &str = include_str!("../vendor/compiler-sfc.esm-browser.js");

/// The four stages, in the order the emitted module needs them.
const DRIVER: &str = include_str!("driver.js");

/// The name the bundle is declared under, which is what a stack frame says.
const MODULE: &str = "@vue/compiler-sfc";

/// The parser inside the compiler recurses once per nesting level of the
/// script it reads and QuickJS refuses at 256 KiB by default, which a
/// `computed(() => props.items.map(...))` already exceeds. The engine runs on
/// a thread with room to spare and is told how much of it to use.
const THREAD_STACK: usize = 64 << 20;
const ENGINE_STACK: usize = 48 << 20;

#[derive(Debug, thiserror::Error)]
pub enum VueError {
  #[error("the Vue compiler: {0}")]
  Js(String),
}

enum Ask {
  Version(Sender<Result<String, VueError>>),
  Compile { filename: String, source: String, options: Options, files: BTreeMap<String, String>, reply: Sender<Result<Outcome, VueError>> },
  Describe { filename: String, source: String, options: Options, files: BTreeMap<String, String>, reply: Sender<Result<Outcome, VueError>> },
  SsrModule { filename: String, source: String, options: Options, files: BTreeMap<String, String>, reply: Sender<Result<Outcome, VueError>> },
  Render { js: String, props: String, children: Option<String>, modules: BTreeMap<String, String>, reply: Sender<Result<String, VueError>> },
}

/// One QuickJS context holding the compiler, owned by the thread it runs on.
/// Booting is the expensive step and compiling is cheap after it, which is
/// the whole reason a plugin worker is long-lived.
pub struct Compiler {
  asks: Sender<Ask>,
  thread: Option<JoinHandle<()>>,
}

impl Compiler {
  pub fn new() -> Result<Self, VueError> {
    let (asks, inbox) = channel::<Ask>();
    let (booted, boot) = channel::<Result<(), VueError>>();
    let thread = std::thread::Builder::new()
      .name("snapfire-vue".to_owned())
      .stack_size(THREAD_STACK)
      .spawn(move || serve(inbox, booted))
      .map_err(|e| VueError::Js(format!("the compiler thread would not start: {e}")))?;
    boot.recv().map_err(|_| VueError::Js("the compiler thread died while booting".to_owned()))??;
    Ok(Self { asks, thread: Some(thread) })
  }

  /// What the carried compiler reports itself as, which is half of a cache key
  /// and the whole of what a bug report needs.
  pub fn version(&self) -> Result<String, VueError> {
    let (reply, answer) = channel();
    self.asks.send(Ask::Version(reply)).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// One component. `filename` is what a diagnostic names it, so it should be
  /// relative to the project rather than absolute. A block with `src` reads
  /// from `files` by the specifier it wrote; a specifier missing from it is
  /// answered with [`Outcome::Needs`].
  pub fn compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Compile { filename: filename.to_owned(), source: source.to_owned(), options: options.clone(), files: files.clone(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// One component as Vue's parser reads it, for a host that lowers it:
  /// [`Outcome::Described`] or a refusal the way `compile` refuses. The
  /// template tree is the shape `describeNode` in the driver writes.
  pub fn describe(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Describe { filename: filename.to_owned(), source: source.to_owned(), options: options.clone(), files: files.clone(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// One component as a module for Vue's own server renderer: the script
  /// plus `ssrRender`, the way `compile` answers with `render`. The module is
  /// in the script's language, so one written in TypeScript is handed on
  /// typed, for the caller to strip before running it.
  pub fn ssr_module(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::SsrModule { filename: filename.to_owned(), source: source.to_owned(), options: options.clone(), files: files.clone(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// The markup Vue's own server renderer writes for a component module
  /// `js`, as `ssr_module` answers it and stripped to JavaScript, given
  /// `props` as JSON and the caller's `children` as markup for the default
  /// slot. `modules` are the sources the component may import by bare
  /// specifier: `vue` and `vue/server-renderer` among them, since the plugin
  /// carries a compiler and no runtime. The component renders under the root
  /// the client's Vue mounter uses, so the children sit in the same region.
  pub fn render_module(&self, js: &str, props: &str, children: Option<&str>, modules: &BTreeMap<String, String>) -> Result<String, VueError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Render { js: js.to_owned(), props: props.to_owned(), children: children.map(str::to_owned), modules: modules.clone(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }
}

impl Drop for Compiler {
  fn drop(&mut self) {
    // Dropping the sender ends the thread's loop; joining it keeps a test's
    // runtime from being torn down under a context still in use.
    let (asks, _) = channel();
    drop(std::mem::replace(&mut self.asks, asks));
    if let Some(thread) = self.thread.take() {
      let _ = thread.join();
    }
  }
}

fn gone() -> VueError {
  VueError::Js("the compiler thread is gone".to_owned())
}

/// The thread's whole life: boot, report, then answer until the last
/// `Compiler` handle is dropped.
fn serve(inbox: Receiver<Ask>, booted: Sender<Result<(), VueError>>) {
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
      Ask::Compile { filename, source, options, files, reply } => {
        let _ = reply.send(engine.compile(&filename, &source, &options, &files));
      }
      Ask::Describe { filename, source, options, files, reply } => {
        let _ = reply.send(engine.describe(&filename, &source, &options, &files));
      }
      Ask::SsrModule { filename, source, options, files, reply } => {
        let _ = reply.send(engine.ssr_module(&filename, &source, &options, &files));
      }
      Ask::Render { js, props, children, modules, reply } => {
        let _ = reply.send(engine.render_module(&js, &props, children.as_deref(), &modules));
      }
    }
  }
}

struct Engine {
  context: Context,
  runtime: Runtime,
  /// Component modules declared for a render so far, so each takes a name of its own.
  rendered: std::cell::Cell<u64>,
}

impl Engine {
  fn new() -> Result<Self, VueError> {
    let runtime = Runtime::new().map_err(js)?;
    runtime.set_max_stack_size(ENGINE_STACK);
    let context = Context::full(&runtime).map_err(js)?;
    context.with(|ctx| -> Result<(), VueError> {
      declare(&ctx, MODULE, COMPILER)?;
      declare(&ctx, "__driver__", DRIVER)?;
      Ok(())
    })?;
    Ok(Self { context, runtime, rendered: std::cell::Cell::new(0) })
  }

  fn version(&self) -> Result<String, VueError> {
    self.context.with(|ctx| {
      let version: Function = ctx.globals().get("__vue_version").map_err(|e| threw(&ctx, e))?;
      version.call(()).map_err(|e| threw(&ctx, e))
    })
  }

  fn compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let answer = self.drive("__vue_compile", filename, source, options, files)?;
    decode(&answer, filename)
  }

  fn describe(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let answer = self.drive("__vue_describe", filename, source, options, files)?;
    decode(&answer, filename)
  }

  /// Calls one of the driver's entry points with the unit's parts as JSON values.
  fn drive(&self, entry: &str, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<String, VueError> {
    self.context.with(|ctx| -> Result<String, VueError> {
      let function: Function = ctx.globals().get(entry).map_err(|e| threw(&ctx, e))?;
      let options = serde_json::to_string(options).map_err(|e| VueError::Js(e.to_string()))?;
      let options = ctx.json_parse(options).map_err(|e| threw(&ctx, e))?;
      let files = serde_json::to_string(files).map_err(|e| VueError::Js(e.to_string()))?;
      let files = ctx.json_parse(files).map_err(|e| threw(&ctx, e))?;
      function.call((filename, source, options, files)).map_err(|e| threw(&ctx, e))
    })
  }

  fn ssr_module(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError> {
    let answer = self.drive("__vue_ssr_module", filename, source, options, files)?;
    decode(&answer, filename)
  }

  fn render_module(&self, js: &str, props: &str, children: Option<&str>, modules: &BTreeMap<String, String>) -> Result<String, VueError> {
    let n = self.rendered.get() + 1;
    self.rendered.set(n);
    let name = format!("component-{n}.vue");
    let mut resolver = BuiltinResolver::default().with_module(name.clone());
    let mut loader = BuiltinLoader::default().with_module(name.clone(), js.to_owned());
    for (specifier, source) in modules {
      resolver = resolver.with_module(specifier.clone());
      loader = loader.with_module(specifier.clone(), source.clone());
    }
    self.runtime.set_loader(resolver, loader);
    self.context.with(|ctx| -> Result<String, VueError> {
      let render: Function = ctx.globals().get("__vue_render").map_err(|e| threw(&ctx, e))?;
      let props = ctx.json_parse(props).map_err(|e| threw(&ctx, e))?;
      let promise: Promise = render.call((name.as_str(), props, children)).map_err(|e| threw(&ctx, e))?;
      promise.finish::<String>().map_err(|e| threw(&ctx, e))
    })
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
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    template: Option<serde_json::Value>,
    #[serde(default)]
    script: Option<snapfire_compiler_wire::Script>,
    #[serde(default)]
    bindings: BTreeMap<String, String>,
    #[serde(default)]
    scope: Option<String>,
  }
  let answer: Answer = serde_json::from_str(answer).map_err(|e| VueError::Js(format!("{filename}: the driver answered {e}")))?;
  match answer.status.as_str() {
    "needs" => Ok(Outcome::Needs { files: answer.files }),
    "described" => Ok(Outcome::Described(Described { template: answer.template, script: answer.script, bindings: answer.bindings, scope: answer.scope, deps: answer.deps, diagnostics: answer.diagnostics })),
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
  let (_, promise) = Module::declare(ctx.clone(), name, source).and_then(|m| m.eval()).map_err(|e| threw(ctx, e))?;
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
