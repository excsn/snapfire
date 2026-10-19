//! Svelte components compiled without a JavaScript runtime on the machine:
//! the crate carries Svelte's own compiler bundle and runs it in QuickJS.
//!
//! The bundle is a UMD script that sets `globalThis.svelte` and imports
//! nothing, so it is evaluated as a script rather than declared as a module.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread::JoinHandle;

use rquickjs::loader::{Loader, Resolver};
use rquickjs::{Context, Ctx, Exception, Function, Module, Promise, Runtime};
use snapfire_compiler_wire::{Compiled, Described, Diagnostic, Kind, Lang, Options, Outcome, Script};

/// `svelte/compiler` 5.57.2 as published, the compiler used when the host names none. Never edited here.
const COMPILER: &str = include_str!("../vendor/compiler.js");

const DRIVER: &str = include_str!("driver.js");

/// The compiler's parser and its walkers recurse once per nesting level and
/// QuickJS refuses at 256 KiB by default. The engine runs on a thread with
/// room to spare and is told how much of it to use.
const THREAD_STACK: usize = 64 << 20;
const ENGINE_STACK: usize = 48 << 20;

/// The bare specifiers a server-compiled component and `svelte/server` import,
/// as paths under a runtime directory holding `svelte`, `esm-env`, `clsx` and
/// `devalue` as published. `esm-env` answers as a production server.
pub const SERVER_RUNTIME: &[(&str, &str)] = &[
  ("svelte/server", "svelte/src/server/index.js"),
  ("svelte/internal/server", "svelte/src/internal/server/index.js"),
  ("svelte/internal/flags/legacy", "svelte/src/internal/flags/legacy.js"),
  ("#client/constants", "svelte/src/internal/client/constants.js"),
  ("esm-env", "esm-env/index.js"),
  ("esm-env/browser", "esm-env/false.js"),
  ("esm-env/development", "esm-env/false.js"),
  ("esm-env/node", "esm-env/false.js"),
  ("clsx", "clsx/dist/clsx.mjs"),
  ("devalue", "devalue/index.js"),
];

#[derive(Debug, thiserror::Error)]
pub enum SvelteError {
  #[error("the Svelte compiler: {0}")]
  Js(String),
}

enum Ask {
  Version(Sender<Result<String, SvelteError>>),
  Run { kind: Kind, filename: String, source: String, options: Options, reply: Sender<Result<Outcome, SvelteError>> },
  Ssr { filename: String, source: String, reply: Sender<Result<Outcome, SvelteError>> },
  Render { entry: String, modules: BTreeMap<String, String>, props: String, runtime: PathBuf, reply: Sender<Result<String, SvelteError>> },
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
    self.run(Kind::Compile, filename, source, options)
  }

  /// One component's scripts for a host that follows their imports. The
  /// plugin does not describe a template yet, so the answer is always
  /// `Described` with no template and one `Error` diagnostic saying so, or
  /// `Failed` when the component does not parse.
  pub fn describe(&self, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError> {
    self.run(Kind::Describe, filename, source, options)
  }

  /// One component or runes module compiled for Svelte's own server
  /// renderer with the carried compiler, for the oracle a lowering is checked
  /// against.
  pub fn ssr_module(&self, filename: &str, source: &str) -> Result<Outcome, SvelteError> {
    let (reply, answer) = channel();
    self.asks.send(Ask::Ssr { filename: filename.to_owned(), source: source.to_owned(), reply }).map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  /// The body `svelte/server`'s `render` writes for the component `entry`
  /// names, given `props` as JSON. `modules` holds what `ssr_module` answered
  /// for it and for each component it imports, by an absolute path such as
  /// `/src/ui/Card.svelte`, which a relative import between them resolves
  /// against. `runtime` is a directory laid out as [`SERVER_RUNTIME`] reads
  /// it, since the plugin carries a compiler and no runtime.
  pub fn render_module(&self, entry: &str, modules: &BTreeMap<String, String>, props: &str, runtime: &Path) -> Result<String, SvelteError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Render { entry: entry.to_owned(), modules: modules.clone(), props: props.to_owned(), runtime: runtime.to_path_buf(), reply })
      .map_err(|_| gone())?;
    answer.recv().map_err(|_| gone())?
  }

  fn run(&self, kind: Kind, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError> {
    let (reply, answer) = channel();
    self
      .asks
      .send(Ask::Run { kind, filename: filename.to_owned(), source: source.to_owned(), options: options.clone(), reply })
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
  let engine = match Engine::new(COMPILER) {
    Ok(engine) => engine,
    Err(e) => {
      let _ = booted.send(Err(e));
      return;
    }
  };
  let _ = booted.send(Ok(()));
  // The application's own compilers, by path and content, so one that a new
  // vendoring replaced under the same path boots again.
  let mut named: HashMap<(String, u64), Engine> = HashMap::new();
  for ask in inbox {
    match ask {
      Ask::Version(reply) => {
        let _ = reply.send(engine.version());
      }
      Ask::Run { kind, filename, source, options, reply } => {
        let answered = match &options.compiler {
          None => engine.run(kind, &filename, &source, &options),
          Some(path) => match std::fs::read_to_string(path) {
            Err(e) => Ok(Outcome::Failed { diagnostics: vec![Diagnostic::error(format!("the Svelte compiler `{path}` could not be read: {e}")).at(filename.clone(), None, None)] }),
            Ok(text) => {
              let mut hasher = DefaultHasher::new();
              text.hash(&mut hasher);
              let key = (path.clone(), hasher.finish());
              let booted = match named.get(&key) {
                Some(_) => Ok(()),
                None => Engine::new(&text).map(|engine| {
                  named.retain(|(held, _), _| held != path);
                  named.insert(key.clone(), engine);
                }),
              };
              match booted {
                Ok(()) => named[&key].run(kind, &filename, &source, &options),
                Err(e) => Ok(Outcome::Failed { diagnostics: vec![Diagnostic::error(format!("the Svelte compiler `{path}` would not boot: {e}")).at(filename.clone(), None, None)] }),
              }
            }
          },
        };
        let _ = reply.send(answered);
      }
      Ask::Ssr { filename, source, reply } => {
        let _ = reply.send(engine.ssr_module(&filename, &source));
      }
      Ask::Render { entry, modules, props, runtime, reply } => {
        let _ = reply.send(engine.render_module(&entry, modules, &props, &runtime));
      }
    }
  }
}

struct Engine {
  context: Context,
  runtime: Runtime,
  rendered: std::cell::Cell<u32>,
}

impl Engine {
  fn new(compiler: &str) -> Result<Self, SvelteError> {
    let runtime = Runtime::new().map_err(js)?;
    runtime.set_max_stack_size(ENGINE_STACK);
    let context = Context::full(&runtime).map_err(js)?;
    context.with(|ctx| -> Result<(), SvelteError> {
      ctx.eval::<(), _>(compiler).map_err(|e| threw(&ctx, e))?;
      ctx.eval::<(), _>(DRIVER).map_err(|e| threw(&ctx, e))?;
      Ok(())
    })?;
    Ok(Self { context, runtime, rendered: std::cell::Cell::new(0) })
  }

  fn version(&self) -> Result<String, SvelteError> {
    self.context.with(|ctx| {
      let version: Function = ctx.globals().get("__svelte_version").map_err(|e| threw(&ctx, e))?;
      version.call(()).map_err(|e| threw(&ctx, e))
    })
  }

  fn ssr_module(&self, filename: &str, source: &str) -> Result<Outcome, SvelteError> {
    let answer = self.context.with(|ctx| -> Result<String, SvelteError> {
      let function: Function = ctx.globals().get("__svelte_ssr_module").map_err(|e| threw(&ctx, e))?;
      function.call((filename, source)).map_err(|e| threw(&ctx, e))
    })?;
    decode(&answer, filename)
  }

  /// Each render names its modules under a prefix of its own, since QuickJS
  /// keeps a module it loaded for the rest of the runtime's life.
  fn render_module(&self, entry: &str, modules: BTreeMap<String, String>, props: &str, runtime: &Path) -> Result<String, SvelteError> {
    let n = self.rendered.get() + 1;
    self.rendered.set(n);
    let prefix = format!("/render-{n}");
    let component = format!("{prefix}{entry}");
    let modules = modules.into_iter().map(|(path, js)| (format!("{prefix}{path}"), js)).collect();
    self.runtime.set_loader(RuntimeResolver { root: runtime.to_path_buf() }, RuntimeLoader { modules });
    self.context.with(|ctx| -> Result<String, SvelteError> {
      let render: Function = ctx.globals().get("__svelte_render").map_err(|e| threw(&ctx, e))?;
      let props = ctx.json_parse(props).map_err(|e| threw(&ctx, e))?;
      let promise: Promise = render.call((component.as_str(), props)).map_err(|e| threw(&ctx, e))?;
      promise.finish::<String>().map_err(|e| threw(&ctx, e))
    })
  }

  fn run(&self, kind: Kind, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError> {
    let entry = match kind {
      Kind::Compile => "__svelte_compile",
      Kind::Describe => "__svelte_describe",
    };
    let answer = self.context.with(|ctx| -> Result<String, SvelteError> {
      let function: Function = ctx.globals().get(entry).map_err(|e| threw(&ctx, e))?;
      let options = serde_json::to_string(options).map_err(|e| SvelteError::Js(e.to_string()))?;
      let options = ctx.json_parse(options).map_err(|e| threw(&ctx, e))?;
      function.call((filename, source, options)).map_err(|e| threw(&ctx, e))
    })?;
    decode(&answer, filename)
  }
}

/// A bare specifier through [`SERVER_RUNTIME`] under `root`, a relative one
/// against the importing file.
struct RuntimeResolver {
  root: PathBuf,
}

impl Resolver for RuntimeResolver {
  fn resolve<'js>(&mut self, ctx: &Ctx<'js>, base: &str, name: &str, _attributes: Option<rquickjs::loader::ImportAttributes<'js>>) -> rquickjs::Result<String> {
    if name.starts_with("./") || name.starts_with("../") {
      let dir = Path::new(base).parent().unwrap_or_else(|| Path::new("/"));
      return Ok(normalise(&dir.join(name)));
    }
    if name.starts_with('/') {
      return Ok(name.to_owned());
    }
    match SERVER_RUNTIME.iter().find(|(specifier, _)| *specifier == name) {
      Some((_, rel)) => Ok(normalise(&self.root.join(rel))),
      None => Err(Exception::throw_message(ctx, &format!("cannot resolve `{name}` from `{base}`: not in the server runtime"))),
    }
  }
}

fn normalise(path: &Path) -> String {
  let mut out = PathBuf::new();
  for part in path.components() {
    match part {
      std::path::Component::ParentDir => {
        out.pop();
      }
      std::path::Component::CurDir => {}
      other => out.push(other),
    }
  }
  out.to_string_lossy().into_owned()
}

/// The components under their own names, every other module from disk.
struct RuntimeLoader {
  modules: BTreeMap<String, String>,
}

impl Loader for RuntimeLoader {
  fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _attributes: Option<rquickjs::loader::ImportAttributes<'js>>) -> rquickjs::Result<Module<'js>> {
    let source = match self.modules.get(name) {
      Some(js) => js.clone(),
      None => std::fs::read_to_string(name).map_err(|e| Exception::throw_message(ctx, &format!("{name}: {e}")))?,
    };
    Module::declare(ctx.clone(), name, source)
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
    script: Option<Script>,
    #[serde(default)]
    diagnostics: Vec<Diagnostic>,
  }
  let answer: Answer = serde_json::from_str(answer).map_err(|e| SvelteError::Js(format!("{filename}: the driver answered {e}")))?;
  match answer.status.as_str() {
    "ok" => Ok(Outcome::Ok(Compiled { js: answer.js, lang: Lang::Js, css: answer.css, source_map: answer.source_map, deps: Vec::new(), diagnostics: answer.diagnostics })),
    "described" => Ok(Outcome::Described(Described { template: None, script: answer.script, bindings: Default::default(), scope: None, slotted: false, deps: Vec::new(), diagnostics: answer.diagnostics })),
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
