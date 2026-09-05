//! QuickJS in process, for `fsr test` only. One engine holds one context: a
//! DOM from linkedom, timers on a virtual clock, a `fetch` the host answers
//! and the application's compiled modules resolved through its import map.
//! Nothing here runs at request time; JS_ENGINE.md keeps that open.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use futures_channel::oneshot;
use futures_util::future::LocalBoxFuture;
use rquickjs::loader::{Loader, Resolver};
use rquickjs::{CatchResultExt, CaughtError, Context, Ctx, Error, Exception, Function, Module, Persistent, Promise, Runtime};

const PRELUDE: &str = include_str!("prelude.js");
const DOM: &str = include_str!("dom.js");
/// The specifier `dom.js` imports linkedom under.
const DOM_SPECIFIER: &str = "__sf_dom__";

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
  #[error("{0}")]
  Engine(#[from] rquickjs::Error),
  #[error("{0}")]
  Js(String),
  #[error("cannot resolve `{specifier}` from `{from}`: {reason}")]
  Resolve { specifier: String, from: String, reason: String },
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
}

/// How a bare specifier reaches a file: the import map's URL, then the URL's
/// prefix to a directory. `overrides` answer first, for test-only builds.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
  pub import_map: HashMap<String, String>,
  pub roots: Vec<(String, PathBuf)>,
  pub overrides: HashMap<String, PathBuf>,
}

impl Resolution {
  fn bare(&self, specifier: &str) -> Option<PathBuf> {
    if let Some(path) = self.overrides.get(specifier) {
      return Some(path.clone());
    }
    let url = self.import_map.get(specifier)?;
    self.roots.iter().find_map(|(prefix, dir)| url.strip_prefix(prefix.as_str()).map(|rest| dir.join(rest.trim_start_matches('/'))))
  }
}

struct FileResolver {
  resolution: Resolution,
  dom: PathBuf,
}

impl Resolver for FileResolver {
  fn resolve<'js>(&mut self, ctx: &Ctx<'js>, base: &str, name: &str, _attributes: Option<rquickjs::loader::ImportAttributes<'js>>) -> rquickjs::Result<String> {
    if name == DOM_SPECIFIER {
      return Ok(self.dom.to_string_lossy().into_owned());
    }
    if name.starts_with('/') {
      return Ok(normalise(Path::new(name)));
    }
    if name.starts_with("./") || name.starts_with("../") {
      let dir = Path::new(base).parent().unwrap_or_else(|| Path::new("/"));
      return Ok(normalise(&dir.join(name)));
    }
    match self.resolution.bare(name) {
      Some(path) => Ok(normalise(&path)),
      None => Err(Exception::throw_message(ctx, &format!("cannot resolve `{name}` from `{base}`: not in the import map"))),
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

struct FileLoader;

impl Loader for FileLoader {
  fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str, _attributes: Option<rquickjs::loader::ImportAttributes<'js>>) -> rquickjs::Result<Module<'js>> {
    let source = std::fs::read_to_string(name).map_err(|e| Exception::throw_message(ctx, &format!("{name}: {e}")))?;
    Module::declare(ctx.clone(), name, source)
  }
}

pub struct FetchResponse {
  pub status: u16,
  pub body: String,
  /// Response headers the page may read with `res.headers.get(name)`.
  pub headers: Vec<(String, String)>,
}

impl FetchResponse {
  pub fn new(status: u16, body: impl Into<String>) -> Self {
    Self { status, body: body.into(), headers: Vec::new() }
  }

  pub fn header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
    self.headers.push((name.into(), value.into()));
    self
  }
}

/// What the runner answers for the page: the mock contexts a spec builds, the
/// server render of a lowered module and every `fetch` the page makes.
pub trait Hooks {
  fn ctx(&self, spec: &str) -> Result<u32, String>;
  fn use_ctx(&self, id: u32) -> Result<(), String>;
  fn session(&self, id: u32) -> Result<String, String>;
  /// The locale tag a ctx runs under.
  fn locale(&self, id: u32) -> Result<String, String>;
  fn calls(&self, id: u32) -> Result<String, String>;
  fn render(&self, module: &str, props: &str) -> Result<Option<String>, String>;
  /// `headers` are the request's, as the page gave them.
  fn fetch(&self, method: String, url: String, body: Option<String>, headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse>;
}

struct PendingCall {
  key: String,
  args: String,
  reply: oneshot::Sender<Result<String, String>>,
}

/// A handle a hook's future uses to call back into the page, for a mocked
/// service method. The engine serves these between jobs. `Send`, since a
/// service transport's future must be.
#[derive(Clone, Default)]
pub struct JsCalls(Arc<parking_lot::Mutex<VecDeque<PendingCall>>>);

impl JsCalls {
  pub fn new() -> Self {
    Self::default()
  }

  pub async fn call(&self, key: String, args: String) -> Result<String, String> {
    let (reply, wait) = oneshot::channel();
    self.0.lock().push_back(PendingCall { key, args, reply });
    wait.await.unwrap_or_else(|_| Err("the engine dropped the call".to_owned()))
  }
}

struct PendingFetch {
  id: u32,
  method: String,
  url: String,
  body: Option<String>,
  headers: Vec<(String, String)>,
}

#[derive(Default)]
struct State {
  console: RefCell<Vec<(String, String)>>,
  fetches: RefCell<Vec<PendingFetch>>,
  completions: RefCell<Vec<(u32, FetchResponse)>>,
  next_fetch: Cell<u32>,
  in_flight: Cell<usize>,
}

pub struct Engine {
  #[allow(dead_code)]
  runtime: Runtime,
  context: Context,
  state: Rc<State>,
  hooks: Rc<dyn Hooks>,
  calls: JsCalls,
}

/// An exception value as text: an `Error`'s name, message and stack, anything else as JSON.
fn describe<'js>(ctx: &Ctx<'js>, value: rquickjs::Value<'js>) -> String {
  if let Some(obj) = value.as_object() {
    let name: Option<String> = obj.get("name").ok();
    let message: Option<String> = obj.get("message").ok();
    let stack: Option<String> = obj.get("stack").ok();
    if let Some(message) = message {
      let mut text = format!("{}: {message}", name.unwrap_or_else(|| "Error".to_owned()));
      if let Some(stack) = stack.filter(|s| !s.is_empty()) {
        text.push('\n');
        text.push_str(&stack);
      }
      return text;
    }
  }
  ctx.json_stringify(value).ok().flatten().and_then(|s| s.to_string().ok()).unwrap_or_else(|| "a non-JSON value".to_owned())
}

fn js_error(ctx: &Ctx<'_>, err: Error) -> EngineError {
  EngineError::Js(CaughtError::from_error(ctx, err).to_string())
}

fn throw(ctx: &Ctx<'_>, message: String) -> Error {
  Exception::throw_message(ctx, &message)
}

impl Engine {
  /// A context with the prelude, the DOM at `dom` (linkedom's worker bundle)
  /// and the hooks bound as globals.
  pub fn new(resolution: Resolution, dom: &Path, hooks: Rc<dyn Hooks>, calls: JsCalls) -> Result<Self, EngineError> {
    let runtime = Runtime::new().map_err(|e| EngineError::Js(e.to_string()))?;
    runtime.set_loader(FileResolver { resolution, dom: dom.to_path_buf() }, FileLoader);
    let context = Context::full(&runtime).map_err(|e| EngineError::Js(e.to_string()))?;
    let state = Rc::new(State::default());
    let tracker = state.clone();
    runtime.set_host_promise_rejection_tracker(Some(Box::new(move |ctx: Ctx<'_>, _promise: rquickjs::Value<'_>, reason: rquickjs::Value<'_>, handled: bool| {
      let text = describe(&ctx, reason);
      let mut console = tracker.console.borrow_mut();
      if handled {
        if let Some(i) = console.iter().rposition(|(level, t)| level == "error" && *t == format!("unhandled rejection: {text}")) {
          console.remove(i);
        }
      } else {
        console.push(("error".to_owned(), format!("unhandled rejection: {text}")));
      }
    })));
    let engine = Self { runtime, context, state, hooks, calls };
    engine.install()?;
    engine.evaluate_module("__sf_dom_boot__", DOM)?;
    Ok(engine)
  }

  fn install(&self) -> Result<(), EngineError> {
    let state = self.state.clone();
    let hooks = self.hooks.clone();
    self.context.with(|ctx| {
      let globals = ctx.globals();
      let s = state.clone();
      globals.set("__sf_log", Function::new(ctx.clone(), move |level: String, text: String| s.console.borrow_mut().push((level, text)))?)?;
      let s = state.clone();
      globals.set(
        "__sf_fetch",
        Function::new(ctx.clone(), move |url: String, method: String, body: Option<String>, flat: Vec<String>| -> u32 {
          let id = s.next_fetch.get() + 1;
          s.next_fetch.set(id);
          let headers = flat.chunks_exact(2).map(|pair| (pair[0].clone(), pair[1].clone())).collect();
          s.fetches.borrow_mut().push(PendingFetch { id, method, url, body, headers });
          id
        })?,
      )?;
      let h = hooks.clone();
      globals.set("__sf_ctx", Function::new(ctx.clone(), move |ctx: Ctx<'_>, spec: String| h.ctx(&spec).map_err(|m| throw(&ctx, m)))?)?;
      let h = hooks.clone();
      globals.set("__sf_use", Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u32| h.use_ctx(id).map_err(|m| throw(&ctx, m)))?)?;
      let h = hooks.clone();
      globals.set("__sf_session", Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u32| h.session(id).map_err(|m| throw(&ctx, m)))?)?;
      let h = hooks.clone();
      globals.set("__sf_locale", Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u32| h.locale(id).map_err(|m| throw(&ctx, m)))?)?;
      let h = hooks.clone();
      globals.set("__sf_calls", Function::new(ctx.clone(), move |ctx: Ctx<'_>, id: u32| h.calls(id).map_err(|m| throw(&ctx, m)))?)?;
      let h = hooks.clone();
      globals.set("__sf_render", Function::new(ctx.clone(), move |ctx: Ctx<'_>, module: String, props: String| h.render(&module, &props).map_err(|m| throw(&ctx, m)))?)?;
      ctx.eval::<(), _>(PRELUDE).catch(&ctx).map_err(|e| EngineError::Js(e.to_string()))?;
      Ok::<(), EngineError>(())
    })
  }

  /// Declares and evaluates `source` as a module named `name`, running the job queue until it settles.
  fn evaluate_module(&self, name: &str, source: &str) -> Result<(), EngineError> {
    let promise = self.context.with(|ctx| {
      let (_, promise) = Module::declare(ctx.clone(), name, source).and_then(|m| m.eval()).map_err(|e| js_error(&ctx, e))?;
      Ok::<_, EngineError>(Persistent::save(&ctx, promise))
    })?;
    self.drain_jobs()?;
    self.settled(promise)
  }

  /// Imports the module at `path` and runs everything it starts to completion.
  pub async fn import(&self, path: &Path) -> Result<(), EngineError> {
    let name = normalise(path);
    let promise = self.context.with(|ctx| Module::import(&ctx, name.as_str()).map(|p| Persistent::save(&ctx, p)).map_err(|e| js_error(&ctx, e)))?;
    self.settle().await?;
    self.settled(promise)
  }

  fn settled(&self, promise: Persistent<Promise<'static>>) -> Result<(), EngineError> {
    self.context.with(|ctx| {
      let promise = promise.restore(&ctx).map_err(|e| js_error(&ctx, e))?;
      match promise.state() {
        rquickjs::promise::PromiseState::Pending => Err(EngineError::Js("the module never finished loading; something awaits forever".to_owned())),
        rquickjs::promise::PromiseState::Resolved => Ok(()),
        rquickjs::promise::PromiseState::Rejected => {
          Err(match promise.result::<rquickjs::Value>() {
            Some(Err(e)) => js_error(&ctx, e),
            _ => EngineError::Js("rejected".to_owned()),
          })
        }
      }
    })
  }

  fn drain_jobs(&self) -> Result<(), EngineError> {
    loop {
      match self.runtime.execute_pending_job() {
        Ok(true) => continue,
        Ok(false) => return Ok(()),
        Err(e) => {
          let text = e.0.with(|ctx| CaughtError::from_error(&ctx, Error::Exception).to_string());
          self.state.console.borrow_mut().push(("error".to_owned(), format!("uncaught in a job: {text}")));
        }
      }
    }
  }

  fn call_global<R: for<'js> rquickjs::FromJs<'js>>(&self, name: &str, args: impl for<'js> rquickjs::function::IntoArgs<'js>) -> Result<R, EngineError> {
    self.context.with(|ctx| {
      let f: Function = ctx.globals().get(name).map_err(|e| js_error(&ctx, e))?;
      f.call(args).map_err(|e| js_error(&ctx, e))
    })
  }

  fn start_fetches(&self) -> bool {
    let fetches: Vec<PendingFetch> = std::mem::take(&mut *self.state.fetches.borrow_mut());
    if fetches.is_empty() {
      return false;
    }
    for fetch in fetches {
      let state = self.state.clone();
      let future = self.hooks.fetch(fetch.method, fetch.url, fetch.body, fetch.headers);
      state.in_flight.set(state.in_flight.get() + 1);
      let id = fetch.id;
      tokio::task::spawn_local(async move {
        let response = future.await;
        state.completions.borrow_mut().push((id, response));
        state.in_flight.set(state.in_flight.get() - 1);
      });
    }
    true
  }

  fn serve_calls(&self) -> Result<bool, EngineError> {
    let mut served = false;
    loop {
      let Some(call) = self.calls.0.lock().pop_front() else { break };
      served = true;
      let result: Result<String, String> = self.context.with(|ctx| {
        let f: Function = ctx.globals().get("__sf_call").map_err(|e| js_error(&ctx, e).to_string())?;
        f.call::<_, String>((call.key.as_str(), call.args.as_str())).map_err(|e| js_error(&ctx, e).to_string())
      });
      let _ = call.reply.send(result);
    }
    Ok(served)
  }

  fn complete_fetches(&self) -> Result<bool, EngineError> {
    let done: Vec<(u32, FetchResponse)> = std::mem::take(&mut *self.state.completions.borrow_mut());
    if done.is_empty() {
      return Ok(false);
    }
    for (id, response) in done {
      let headers: Vec<String> = response.headers.into_iter().flat_map(|(name, value)| [name, value]).collect();
      self.call_global::<()>("__sf_complete", (id, response.status, response.body, headers))?;
    }
    Ok(true)
  }

  /// Runs microtasks, mocked service calls, fetches, timers and idle waiters until none is left.
  pub async fn settle(&self) -> Result<(), EngineError> {
    loop {
      self.drain_jobs()?;
      let mut progressed = self.start_fetches();
      progressed |= self.serve_calls()?;
      progressed |= self.complete_fetches()?;
      if progressed {
        continue;
      }
      if self.state.in_flight.get() > 0 {
        tokio::task::yield_now().await;
        continue;
      }
      if self.call_global::<bool>("__sf_tick", ())? {
        continue;
      }
      if self.call_global::<bool>("__sf_flush_idle", ())? {
        continue;
      }
      return Ok(());
    }
  }

  /// The names the loaded spec files registered with `test(...)`.
  pub fn test_names(&self) -> Result<Vec<String>, EngineError> {
    self.call_global("__sf_tests", ())
  }

  /// Runs test `index` to completion; a rejection is the failure text.
  pub async fn run_test(&self, index: usize) -> Result<Result<(), String>, EngineError> {
    let promise = self.context.with(|ctx| {
      let f: Function = ctx.globals().get("__sf_run").map_err(|e| js_error(&ctx, e))?;
      let promise: Promise = f.call((index as u32,)).map_err(|e| js_error(&ctx, e))?;
      Ok::<_, EngineError>(Persistent::save(&ctx, promise))
    })?;
    self.settle().await?;
    Ok(match self.settled(promise) {
      Ok(()) => Ok(()),
      Err(EngineError::Js(text)) => Err(text),
      Err(other) => return Err(other),
    })
  }

  /// Everything `console` received since the last take, as `(level, text)`.
  pub fn take_console(&self) -> Vec<(String, String)> {
    std::mem::take(&mut *self.state.console.borrow_mut())
  }

  /// Evaluates a script expression for its string value.
  pub fn eval_string(&self, source: &str) -> Result<String, EngineError> {
    self.context.with(|ctx| ctx.eval::<String, _>(source).catch(&ctx).map_err(|e| EngineError::Js(e.to_string())))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  struct NoHooks;

  impl Hooks for NoHooks {
    fn ctx(&self, _spec: &str) -> Result<u32, String> {
      Ok(1)
    }
    fn use_ctx(&self, _id: u32) -> Result<(), String> {
      Ok(())
    }
    fn session(&self, _id: u32) -> Result<String, String> {
      Ok("{}".to_owned())
    }
    fn locale(&self, _id: u32) -> Result<String, String> {
      Ok("en".to_owned())
    }
    fn calls(&self, _id: u32) -> Result<String, String> {
      Ok("[]".to_owned())
    }
    fn render(&self, _module: &str, _props: &str) -> Result<Option<String>, String> {
      Ok(None)
    }
    fn fetch(&self, method: String, url: String, body: Option<String>, _headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse> {
      Box::pin(async move { FetchResponse { headers: Vec::new(), status: 200, body: format!("{{\"echo\":\"{method} {url} {}\"}}", body.unwrap_or_default().replace('"', "'")) } })
    }
  }

  fn engine() -> Engine {
    let dom = std::env::temp_dir().join(format!("fsr_engine_stub_{}.mjs", std::process::id()));
    std::fs::write(&dom, "export function parseHTML() { return { document: { body: {} } }; }\nexport class Element {}\nexport class HTMLElement extends Element {}\nexport class Event { constructor(type, init = {}) { this.type = type; Object.assign(this, init); } }\n").unwrap();
    Engine::new(Resolution::default(), &dom, Rc::new(NoHooks), JsCalls::new()).unwrap()
  }

  fn run<F: std::future::Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(f))
  }

  #[test]
  fn url_follows_the_standard_for_the_shapes_the_client_uses() {
    let e = engine();
    let cases = [
      ("new URL('/cart?x=1#h', 'http://localhost/').href", "http://localhost/cart?x=1#h"),
      ("new URL('../a/./b', 'http://h:8080/x/y/z').pathname", "/x/a/b"),
      ("new URL('?q=pla', 'http://h/p').href", "http://h/p?q=pla"),
      ("new URL('http://H/a b').hostname", "h"),
      ("String(new URL('http://h/?a=1&b=2').searchParams.get('b'))", "2"),
      ("(() => { const p = new URLSearchParams(); p.set('q', 'a b'); p.set('c', '1'); p.set('q', 'x'); return p.toString(); })()", "q=x&c=1"),
      ("[...new URLSearchParams('a=1&a=2&b=%20').entries()].map(([k, v]) => k + '=' + v).join(',')", "a=1,a=2,b= "),
    ];
    for (source, expected) in cases {
      assert_eq!(e.eval_string(source).unwrap(), expected, "{source}");
    }
    assert!(e.eval_string("new URL('relative')").is_err());
  }

  #[test]
  fn timers_run_on_a_virtual_clock_that_only_advance_moves() {
    let e = engine();
    e.eval_string("globalThis.log = []; setTimeout(() => log.push('later'), 1000); setTimeout(() => log.push('now'), 0); setImmediate(() => log.push('immediate')); Promise.resolve().then(() => log.push('micro')); ''").unwrap();
    run(e.settle()).unwrap();
    assert_eq!(e.eval_string("log.join(',')").unwrap(), "micro,now,immediate");
    e.eval_string("__sf.advance(999); ''").unwrap();
    run(e.settle()).unwrap();
    assert_eq!(e.eval_string("log.join(',')").unwrap(), "micro,now,immediate");
    e.eval_string("__sf.advance(1); ''").unwrap();
    run(e.settle()).unwrap();
    assert_eq!(e.eval_string("log.join(',')").unwrap(), "micro,now,immediate,later");
    assert_eq!(e.eval_string("String(performance.now())").unwrap(), "1000");
  }

  #[test]
  fn fetch_is_answered_by_the_hooks_and_console_is_captured() {
    let e = engine();
    e.eval_string("globalThis.got = null; fetch('/_sf/action/x', { method: 'POST', body: '{\"a\":1}' }).then((r) => r.json()).then((j) => { got = j.echo; console.warn('done %s', got); }); ''").unwrap();
    run(e.settle()).unwrap();
    assert_eq!(e.eval_string("got").unwrap(), "POST /_sf/action/x {'a':1}");
    assert_eq!(e.take_console(), vec![("warn".to_owned(), "done POST /_sf/action/x {'a':1}".to_owned())]);
    e.eval_string("Promise.reject(new TypeError('nope')); ''").unwrap();
    run(e.settle()).unwrap();
    let console = e.take_console();
    assert_eq!(console.len(), 1);
    assert!(console[0].1.starts_with("unhandled rejection: TypeError: nope"), "{}", console[0].1);
  }

  #[test]
  fn tests_register_and_run_with_their_failures_as_text() {
    let e = engine();
    e.eval_string("globalThis.__sf_tests = () => ['passes', 'fails']; globalThis.__sf_run = (i) => i === 0 ? Promise.resolve() : Promise.reject(new Error('boom')); ''").unwrap();
    assert_eq!(e.test_names().unwrap(), vec!["passes", "fails"]);
    assert_eq!(run(e.run_test(0)).unwrap(), Ok(()));
    let failure = run(e.run_test(1)).unwrap().unwrap_err();
    assert!(failure.contains("boom"), "{failure}");
  }
}
