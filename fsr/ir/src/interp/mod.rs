use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use futures_util::future::{join_all, BoxFuture};
use snapfire_fsr_core::{Collection, Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, RequestCtx, SessionCell};

use crate::catalog::Catalogs;
use crate::ext::{Ambient, Extensions};
use crate::ast::{ArithOp, Body, Builtin, CompareOp, Consts, Entry, Expr, Lit, LogicOp, ScopedStyle, Stmt};
use crate::bind::kind_name;
use crate::render::{Frameworks, Markup};

pub use snapfire_fsr_core::ext::Fail;

mod builtins;
mod classes;
mod closures;
mod eval;
mod numbers;
mod places;
mod sync;
mod text;
mod values;

pub(crate) use self::builtins::*;
use self::classes::*;
pub(crate) use self::numbers::*;
pub(crate) use self::text::*;
pub(crate) use self::values::*;

#[non_exhaustive]
pub struct Outcome {
  pub value: Value,
  /// Session keys the body wrote or deleted, in order, committed before this
  /// was returned.
  pub written: Vec<String>,
  /// Seconds from now the body moved the session's end to, already applied
  /// to the cell; `None` when it did not extend.
  pub extended: Option<u64>,
  /// Where the response the body returned sends the caller, the `Location` of a redirect; `None` for anything else. An action's host checks it is a path on this origin.
  pub redirected: Option<String>,
}

/// What `ctx.now` reads. Milliseconds since the Unix epoch, as `Value::Int`.
pub trait Clock: Send + Sync {
  fn now(&self) -> i128;
}

struct SystemClock;

impl Clock for SystemClock {
  fn now(&self) -> i128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i128).unwrap_or(0)
  }
}

/// The store keys each prepared component reads, by the component's address, which the entry pins by holding the component so the address names it for as long as the cache lives.
pub(crate) type StoreKeyCache = parking_lot::Mutex<std::collections::HashMap<usize, (Arc<crate::ast::Component>, Arc<Vec<String>>)>>;

#[derive(Clone)]
pub struct Interpreter {
  clock: Arc<dyn Clock>,
  store_keys: Arc<StoreKeyCache>,
  extensions: Arc<Extensions>,
  catalogs: Option<Arc<Catalogs>>,
  files: Option<Arc<snapfire_fsr_core::ext::FileRoots>>,
  consts: Option<Arc<Consts>>,
  held: Arc<Held>,
  /// The module's functions prepared once, by constant key, [`crate::compiled::Function::of_consts`].
  functions: Arc<HashMap<String, Arc<crate::compiled::Function>>>,
  frameworks: Frameworks,
  /// The store keys each island the server does not render reads, by module.
  island_reads: Arc<std::collections::BTreeMap<String, Vec<String>>>,
  /// The derived store keys, sources before the keys they feed.
  derives: Arc<Vec<Derive>>,
  /// `[public]` from the deployment's configuration, the same on every request, which a component render reads `Expr::Config` from.
  config: Arc<ValueMap>,
}

/// A store key the browser's `derive` computes: its sources and the lowered
/// compute, reading each source as `Expr::Store`; no body for one the build
/// could not lower.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Derive {
  pub key: String,
  pub sources: Vec<String>,
  pub body: Option<Expr>,
}

impl Derive {
  pub fn new(key: String, sources: Vec<String>, body: Option<Expr>) -> Self {
    Self { key, sources, body }
  }
}

/// The value of each constant that reads nothing from a request and calls
/// nothing, computed on its first read and shared by every read after it.
/// A constant that is missing here is evaluated on every read.
#[derive(Default)]
pub(crate) struct Held {
  cells: HashMap<String, OnceLock<Value>>,
}

impl Held {
  fn new(consts: &Consts) -> Self {
    let mut pure = HashMap::new();
    for key in consts.keys() {
      is_pure(key, consts, &mut pure);
    }
    Self { cells: pure.into_iter().filter(|(_, p)| *p == Some(true)).map(|(k, _)| (k, OnceLock::new())).collect() }
  }

  fn get(&self, key: &str) -> Option<&Value> {
    self.cells.get(key)?.get()
  }

  fn hold(&self, key: &str, value: &Value) {
    if holds_closure(value) {
      return;
    }
    if let Some(cell) = self.cells.get(key) {
      let _ = cell.set(value.frozen());
    }
  }
}

fn undeclared(key: &str) -> Fail {
  Fail::new(FailureKind::Internal, format!("`{key}` is not a constant this plan declares"))
}

/// `None` in `seen` marks a constant whose answer is still being worked out,
/// so a cycle counts as impure.
fn is_pure(key: &str, consts: &Consts, seen: &mut HashMap<String, Option<bool>>) -> bool {
  if let Some(known) = seen.get(key) {
    return known.unwrap_or(false);
  }
  let Some(expr) = consts.get(key) else { return false };
  seen.insert(key.to_owned(), None);
  let mut free = Vec::new();
  expr.free_vars(&mut free);
  let mut pure = free.is_empty();
  let mut refs = Vec::new();
  expr.visit(&mut |e| match e {
    Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Document | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Call { .. } | Expr::NativeCall { .. } | Expr::Next(_) | Expr::After(_) | Expr::Ext { .. } | Expr::Hoist { .. } | Expr::HoistId(_) | Expr::Cell(_) | Expr::ModuleCell { .. } | Expr::ReadCell(_) | Expr::WriteCell(..) | Expr::WriteStore(..) => pure = false,
    Expr::Const(other) => refs.push(other.clone()),
    _ => {}
  });
  if pure {
    pure = refs.iter().all(|other| is_pure(other, consts, seen));
  }
  seen.insert(key.to_owned(), Some(pure));
  pure
}

impl Default for Interpreter {
  fn default() -> Self {
    Self { clock: Arc::new(SystemClock), store_keys: Arc::default(), extensions: Arc::new(Extensions::standard()), catalogs: None, files: None, consts: None, held: Arc::default(), functions: Arc::default(), frameworks: Frameworks::default(), island_reads: Arc::default(), derives: Arc::default(), config: Arc::default() }
  }
}

impl Interpreter {
  pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
    Self { clock, store_keys: Arc::default(), extensions: Arc::new(Extensions::standard()), catalogs: None, files: None, consts: None, held: Arc::default(), functions: Arc::default(), frameworks: Frameworks::default(), island_reads: Arc::default(), derives: Arc::default(), config: Arc::default() }
  }

  /// The frameworks the application vendors. A component one of them
  /// hydrates is written the way that framework's server renderer writes it;
  /// with none set, every component is written as plain markup.
  pub fn with_frameworks(mut self, frameworks: Frameworks) -> Self {
    self.frameworks = frameworks;
    self
  }

  /// The store keys an island the server does not render reads, by module: the
  /// plan's `reads`. Such an island is held while one of them is pending.
  pub fn with_island_reads(mut self, reads: std::collections::BTreeMap<String, Vec<String>>) -> Self {
    self.island_reads = Arc::new(reads);
    self
  }

  /// The derived store keys, sources before the keys they feed: a step
  /// recomputes them after its writes and a server-mode island asks the
  /// browser for their sources as well as for them.
  pub fn with_derives(mut self, derives: Vec<Derive>) -> Self {
    self.derives = Arc::new(derives);
    self
  }

  /// What `body` computes over `store` under `ctx`, as a derived key's
  /// compute: `None` when it fails, where the browser's compute would throw.
  pub fn compute(&self, ctx: &RequestCtx, body: &Expr, store: &ValueMap) -> Option<Value> {
    let mut env = Env::detached(self, Vec::new());
    env.ctx = ctx.clone();
    env.store = store.clone();
    env.eval_sync(body).ok()
  }

  pub fn frameworks(&self) -> Frameworks {
    self.frameworks
  }

  pub fn with_config(mut self, config: Arc<ValueMap>) -> Self {
    self.config = config;
    self
  }

  pub fn config(&self) -> &Arc<ValueMap> {
    &self.config
  }

  /// Where `file` resolves and what it reaches: a web host's roots. None by default, as a terminal program has, where a path is the working directory's.
  pub fn with_files(mut self, files: Option<Arc<snapfire_fsr_core::ext::FileRoots>>) -> Self {
    self.files = files;
    self
  }

  /// The message catalogs `i18n.t` reads; none by default, where every key answers as itself.
  pub fn with_catalogs(mut self, catalogs: Option<Arc<Catalogs>>) -> Self {
    self.catalogs = catalogs;
    self
  }

  pub fn catalogs(&self) -> Option<&Arc<Catalogs>> {
    self.catalogs.as_ref()
  }

  /// The plan's module-level constants, which `Expr::Const` reads. A plan that
  /// named none leaves this empty and no body refers to one.
  pub fn with_consts(mut self, consts: Option<Arc<Consts>>) -> Self {
    self.held = Arc::new(consts.as_deref().map(Held::new).unwrap_or_default());
    self.functions = Arc::new(consts.as_deref().map(crate::compiled::Function::of_consts).unwrap_or_default());
    self.consts = consts;
    self
  }

  pub fn consts(&self) -> Option<&Arc<Consts>> {
    self.consts.as_ref()
  }

  /// The same interpreter answering `extensions` in place of the standard library alone.
  pub fn with_extensions(mut self, extensions: Arc<Extensions>) -> Self {
    self.extensions = extensions;
    self
  }

  pub fn extensions(&self) -> &Arc<Extensions> {
    &self.extensions
  }

  /// Evaluates one expression over `scope` with no request: what a test's
  /// assertion or a rendered component reads.
  pub async fn evaluate(&self, expr: &Expr, scope: Vec<(String, Value)>) -> Result<Value, Fail> {
    let mut env = Env::detached(self, scope.into_iter().map(|(name, value)| (Name::new(name), value)).collect());
    env.eval(expr).await
  }

  /// A body compiled into the application run with no request, its value as [`Self::evaluate`] answers a block's.
  pub fn evaluate_native(&self, native: crate::native::NativeBody) -> Result<Value, Fail> {
    let mut env = Env::detached(self, Vec::new());
    native(&mut crate::native::Frame::new(&mut env))
  }

  /// A prepared body run with no request, its value as [`Self::evaluate`] answers a block's.
  pub fn evaluate_program(&self, program: &crate::compiled::Program) -> Result<Value, Fail> {
    let mut env = Env::detached(self, Vec::new());
    program.run(&mut env)?.value()
  }

  /// Applies a lambda to `args` with no request: what a mocked service method is.
  pub async fn apply(&self, lambda: &Expr, args: Vec<Value>) -> Result<Value, Fail> {
    let mut env = Env::detached(self, Vec::new());
    env.apply(lambda, args).await
  }

  /// Runs a body. Session writes land in a draft committed only on success.
  /// A guard that reads no bound variable and sits before the first statement
  /// that could write the session runs before anything else, so a call is
  /// never made for a request a guard would refuse.
  pub async fn run(&self, body: &Body, ctx: &RequestCtx, input: Option<Value>) -> Result<Outcome, Fail> {
    self.run_prepared(body, crate::compiled::Program::of(body).as_ref(), ctx, input).await
  }

  /// [`Self::run`] with `program`, [`crate::compiled::Program::of`] the body, prepared once for every request.
  pub async fn run_prepared(&self, body: &Body, program: Option<&crate::compiled::Program>, ctx: &RequestCtx, input: Option<Value>) -> Result<Outcome, Fail> {
    self.run_with(body, program.map_or(Prepared::No, Prepared::Compiled), ctx, input).await
  }

  /// [`Self::run`] running `body` as `prepared` has it: interpreted, prepared once or compiled into the application.
  pub async fn run_with(&self, body: &Body, prepared: Prepared<'_>, ctx: &RequestCtx, input: Option<Value>) -> Result<Outcome, Fail> {
    let (data, identity) = ctx.session.snapshot();
    let mut env = Env {
      store_keys: self.store_keys.clone(),
      ctx: ctx.clone(),
      store: ValueMap::default(),
      pending: Default::default(),
      hoists: None,
      state: None,
      probe: None,
      entered: Vec::new(),
      emitted: Vec::new(),
      server_mode: false,
      acts: Vec::new(),
      handling: false,
      calls: 0,
      depth: 0,
      frameworks: self.frameworks,
      island_reads: self.island_reads.clone(),
      derives: self.derives.clone(),
      markup: Markup::Plain,
      in_svg: false,
      in_noscript: false,
      in_framework: false,
      component_path: String::new(),
      unrendered: false,
      contexts: Vec::new(),
      island: None,
      stamp: Vec::new(),
      fallthrough: None,
      slotted: Vec::new(),
      slot_stamps: None,
      scoped: None,
      made: Some(Vec::new()),
      closures: Some(Vec::new()),
      cells: Some(Vec::new()),
      heap_live: false,
      heap_copies: Vec::new(),
      thawed: Default::default(),
      object_keys: Vec::new(),
      module_cells: Vec::new(),
      store_written: ValueMap::default(),
      handler_calls: Vec::new(),
      written_back: None,
      backing: false,
      module_seed: snapfire_fsr_core::ValueMap::default(),
      cycles: Cycles::default(),
      marks: Vec::new(),
      placed: Vec::new(),
      input: input.unwrap_or(Value::Null),
      identity: identity.map(|id| {
        let mut map = ValueMap::default();
        map.insert("subject".to_owned(), Value::str(id.subject));
        map.insert("claims".to_owned(), Value::Map(id.claims));
        Value::Map(map)
      }),
      session: data,
      written: Vec::new(),
      extend: None,
      scope: Vec::new(),
      clock: self.clock.clone(),
      extensions: self.extensions.clone(),
      catalogs: self.catalogs.clone(),
      files: self.files.clone(),
      consts: self.consts.clone(),
      functions: self.functions.clone(),
      held: self.held.clone(),
    };

    for stmt in body {
      let mut queues = false;
      if let Stmt::Let { expr, .. } | Stmt::Expr(expr) | Stmt::Return(expr) = stmt {
        expr.visit(&mut |e| queues |= matches!(e, Expr::After(_)));
      }
      if queues {
        break;
      }
      match stmt {
        Stmt::Guard { cond, kind, message } => {
          let mut free = Vec::new();
          cond.free_vars(&mut free);
          if free.is_empty() && !cond.has_call() {
            env.guard(cond, kind, message).await?;
          }
        }
        Stmt::Let { .. } | Stmt::Set { .. } | Stmt::Return(_) | Stmt::Expr(_) => {}
        Stmt::If { .. } | Stmt::ForOf { .. } | Stmt::While { .. } | Stmt::Break | Stmt::Continue | Stmt::Try { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } | Stmt::Act { .. } => break,
      }
    }

    let flow = match prepared {
      Prepared::Compiled(program) => program.run(&mut env)?,
      Prepared::Native(native) => Flow::Return(native(&mut crate::native::Frame::new(&mut env))?),
      Prepared::No => env.block(body).await?,
    };
    let value = match flow {
      Flow::Next => Value::Null,
      flow => leaves(env.settled(flow.value()?))?,
    };
    commit(&ctx.session, &env.session, &env.written);
    if let Some(seconds) = env.extend {
      ctx.session.extend(std::time::Duration::from_secs(seconds));
    }
    let redirected = snapfire_fsr_runtime::HttpResponse::from_value(&value).and_then(Result::ok).and_then(|response| response.location().map(str::to_owned));
    Ok(Outcome { value, written: env.written, extended: env.extend, redirected })
  }
}

fn commit(cell: &SessionCell, draft: &ValueMap, written: &[String]) {
  for key in written {
    match draft.get(key) {
      Some(value) => cell.insert(key.clone(), value.clone()),
      None => {
        cell.remove(key);
      }
    }
  }
}

pub(crate) enum Flow {
  Next,
  Return(Value),
  Break,
  Continue,
}

impl Flow {
  /// What a function's or a block's body gave once it ended this way.
  pub(crate) fn value(self) -> Result<Value, Fail> {
    match self {
      Flow::Return(value) => Ok(value),
      Flow::Next => Ok(Value::Null),
      Flow::Break | Flow::Continue => Err(Fail::internal("a `break` or `continue` outside a loop")),
    }
  }
}

pub(crate) struct Env {
  pub(crate) ctx: RequestCtx,
  pub(crate) store_keys: Arc<StoreKeyCache>,
  input: Value,
  identity: Option<Value>,
  session: ValueMap,
  written: Vec<String>,
  /// The last `SessionExtend` the body ran, applied with the draft on success.
  extend: Option<u64>,
  pub(crate) scope: Vec<(Name, Value)>,
  pub(crate) store: ValueMap,
  /// The store keys `WriteStore` wrote, which a step returns with its patch's.
  pub(crate) store_written: ValueMap,
  /// The handlers a handler called through a reference, by token, each with what it was called with; the step runs them after it.
  pub(crate) handler_calls: Vec<(String, Value)>,
  /// What the last function applied changed of its parameters, [`Builtin::WriteBack`], for the call to write back to its arguments.
  written_back: Option<Vec<(usize, Value)>>,
  /// Set by a call that can write back to its arguments, taken by the function it applies.
  backing: bool,
  /// The store keys a deferred segment has yet to seed: an island reading one is held.
  pub(crate) pending: std::collections::BTreeSet<String>,
  clock: Arc<dyn Clock>,
  extensions: Arc<Extensions>,
  catalogs: Option<Arc<Catalogs>>,
  files: Option<Arc<snapfire_fsr_core::ext::FileRoots>>,
  consts: Option<Arc<Consts>>,
  functions: Arc<HashMap<String, Arc<crate::compiled::Function>>>,
  held: Arc<Held>,
  /// Where a render records hoisted values; `None` in a body, which hoists nothing.
  pub(crate) hoists: Option<Hoists>,
  /// Values that stand in for a component's state `let`s, when an island in
  /// server mode renders after a handler ran: the island's own by name, a
  /// component rendered inside it by `<path>/<name>`.
  pub(crate) state: Option<ValueMap>,
  /// A step's search for the component a handler token addresses.
  pub(crate) probe: Option<Probe>,
  /// A step's record of each component it rendered, by path and module, root first.
  pub(crate) entered: Vec<(String, String)>,
  /// A step's record of each value its bodies emitted, by kind, in render order.
  pub(crate) emitted: Vec<crate::render::Emitted>,
  /// Rendering an island in server mode: handler markers print as attributes
  /// the browser binds, which a browser-mode render must never show React.
  pub(crate) server_mode: bool,
  /// The actions a handler asked for, in order, with their inputs evaluated.
  /// The host dispatches them once the step is done.
  pub(crate) acts: Vec<(String, Value)>,
  /// Set while a handler's body runs in a step: an action it dispatches is one of the step's `acts`.
  pub(crate) handling: bool,
  /// Components entered and not yet left, which bounds a render that recurses.
  pub(crate) calls: usize,
  /// Function values applied and not yet returned, which bounds a function that calls itself.
  pub(crate) depth: usize,
  /// The frameworks the application vendors, read on entering each component.
  pub(crate) frameworks: Frameworks,
  pub(crate) island_reads: Arc<std::collections::BTreeMap<String, Vec<String>>>,
  pub(crate) derives: Arc<Vec<Derive>>,
  /// The rules the markup being written is under.
  pub(crate) markup: Markup,
  /// Inside `<svg>` short of a `<foreignObject>`, where React 19 leaves those tags in place.
  pub(crate) in_svg: bool,
  /// Inside `<noscript>`, where React 19 leaves them in place too.
  pub(crate) in_noscript: bool,
  /// Inside a tree a framework renders again in the browser: an island's body
  /// or a component that framework hydrates. Outside one, a hydrating
  /// component is placed as an island rather than rendered inline.
  pub(crate) in_framework: bool,
  /// The path the component being rendered was entered at, which a handler
  /// marker addresses whatever loop inside that component the element sits in.
  pub(crate) component_path: String,
  /// Set when a tree a framework renders reached a component the build could
  /// not lower: the server cannot write that tree, so the island holding it
  /// mounts fresh.
  pub(crate) unrendered: bool,
  /// The values the providers around the render point hold, by context, innermost last.
  pub(crate) contexts: Vec<(String, Value)>,
  /// The framework of the island being rendered, whose tree an island of the same framework inside it joins in the browser. None outside every island.
  pub(crate) island: Option<crate::ast::Owner>,
  /// Attributes waiting for the first element the component just placed renders, its root: a Vue parent's scoped-style id, followed inside slot content by the slot host's slotted stamp.
  pub(crate) stamp: Vec<String>,
  /// The attributes a Vue parent gave a child that the child does not declare as props, waiting for the child's root element, which takes them as Vue's `mergeProps` does.
  pub(crate) fallthrough: Option<ValueMap>,
  /// The slotted stamps every element of the slot content being rendered carries.
  pub(crate) slotted: Vec<String>,
  /// Set while slot content renders: the stamps a component placed in it takes on its root after the parent scope its placement carries, the slot host's slotted stamp when its style uses `:slotted()`.
  pub(crate) slot_stamps: Option<Vec<String>>,
  /// The scoped style of the component whose template is being rendered, which its slots stamp their content by.
  pub(crate) scoped: Option<ScopedStyle>,
  /// The markup values this render made, which a value made here names by index. `None` in a copy for parallel evaluation, whose table would be lost.
  pub(crate) made: Option<Vec<crate::render::Made>>,
  /// The markup values among the props of the island being rendered, by made index in the order the browser numbers them: each renders in a region the browser adopts.
  pub(crate) marks: Vec<usize>,
  /// The made indices among `marks` this island's render placed, whose markup the browser reads from where they were placed.
  pub(crate) placed: Vec<usize>,
  /// The function values this render made, which a value made here names by index. `None` in a copy for parallel evaluation.
  pub(crate) closures: Option<Vec<Closure>>,
  /// The cells this render made, which a handle names by index. `None` in a copy for parallel evaluation.
  pub(crate) cells: Option<Vec<Value>>,
  /// Whether this render or body writes objects, so that it reads its own copy of what requests share.
  pub(crate) heap_live: bool,
  pub(crate) heap_copies: Vec<(String, Value)>,
  /// The frozen objects this body wrote, by address, with the copy each became for it.
  pub(crate) thawed: std::collections::HashMap<usize, Value, snapfire_fsr_core::ValueHasher>,
  /// The objects used as `Map` keys, by the text they are kept under.
  pub(crate) object_keys: Vec<(String, Value)>,
  /// The render's named cells, `Expr::ModuleCell`, by key with their index in `cells`.
  pub(crate) module_cells: Vec<(String, usize)>,
  /// The value a named cell starts from instead of its `init`, by key: a host that keeps module state from one step to the next hands it in as [`crate::render::CELLS_PROP`].
  pub(crate) module_seed: snapfire_fsr_core::ValueMap,
  /// The objects a write may have closed a cycle through. The last field, so it drops after everything else the body held.
  cycles: Cycles,
}

/// What an in-place write makes of the object or array it writes, its parts evaluated: fields set, items pushed, one key or index set or the whole contents replaced.
pub(crate) enum Write {
  Field(Name, Value),
  Fields(Vec<(Name, Value)>),
  Item(Value),
  Items(Vec<Value>),
  At(Value, Value),
  Whole(Value),
}

/// How many objects and arrays a write looks through for the object it writes into before it leaves the question to the end of the request.
const CYCLE_SEARCH: usize = 64;

/// What a request wrote that may sit on a cycle, freed by [`snapfire_fsr_core::collect_cycles`] once the request lets go of everything else.
#[derive(Default)]
struct Cycles(Vec<Value>);

impl Drop for Cycles {
  fn drop(&mut self) {
    snapfire_fsr_core::collect_cycles(std::mem::take(&mut self.0));
  }
}

/// A read of the request a body makes, the session draft and the clock included, answered by [`Env::read_request`].
#[derive(Debug, Clone, Copy)]
pub enum Read<'a> {
  Param(&'a str),
  Query(&'a str),
  ParamsAll,
  QueryAll,
  Session(&'a str),
  Locale,
  Locales,
  Path,
  Document,
  Host,
  Origin,
  Address,
  Config(&'a str),
  Input,
  Now,
}

/// How a body runs: as written, prepared once as a [`crate::compiled::Program`] or compiled into the application as a [`crate::native::NativeBody`].
#[derive(Clone, Copy)]
pub enum Prepared<'a> {
  No,
  Compiled(&'a crate::compiled::Program),
  Native(crate::native::NativeBody),
}

/// A function value: its parameters, its body and the names it reads with their values where it was written.
#[derive(Debug, Clone)]
pub(crate) struct Closure {
  params: Vec<String>,
  body: std::sync::Arc<Expr>,
  scope: Vec<(Name, Value)>,
  /// The name the function sees itself under while it runs, for one that calls itself.
  name: Option<String>,
  /// The body prepared, run in place of `body` when the scope was captured as it expects.
  compiled: Option<Arc<crate::compiled::Function>>,
}

/// The tag of a function value: its payload indexes the render's closure table.
pub const FUNCTION_VALUE: &str = snapfire_fsr_core::value::typed::FUNCTION;

/// How deep function values may nest their calls before the step fails. The stack grows to fit, [`on_enough_stack`].
pub const MAX_APPLIES: usize = 256;

/// How many passes one `Stmt::While` may run before the step fails, where JavaScript would loop on.
pub const MAX_PASSES: usize = 10_000_000;

fn past_passes() -> Fail {
  Fail::internal(format!("a loop ran past {MAX_PASSES} passes"))
}

pub(crate) use snapfire_fsr_core::on_enough_stack;

/// A local's name in a scope: a short one is held inline, so binding it allocates nothing.
pub(crate) type Name = smol_str::SmolStr;

/// How deep a value may nest when it leaves a body. What reads it past there walks it recursively.
const MAX_NESTING: usize = 512;

fn leaves(value: Value) -> Result<Value, Fail> {
  if value.nested_past(MAX_NESTING) {
    return Err(Fail::internal(format!("a value nested more than {MAX_NESTING} levels deep leaving a body")));
  }
  Ok(value)
}

/// The tag of a cell's handle, a `Value::Variant` whose payload is the cell's index in the render's table.
pub const CELL_VALUE: &str = "$cell";

/// The name a named cell's key evaluated to.
fn cell_key(key: Value) -> Result<String, Fail> {
  match key {
    Value::Str(key) => Ok(key.to_string()),
    other => Err(Fail::internal(format!("a named cell's key that is not a string: {other:?}"))),
  }
}

fn cell_handle(index: usize) -> Value {
  Value::Variant { tag: CELL_VALUE.to_owned(), payload: Some(Box::new(Value::Int(index as i128))) }
}

fn cell_index(handle: &Value) -> Option<usize> {
  match handle {
    Value::Variant { tag, payload: Some(index) } if tag == CELL_VALUE => match **index {
      Value::Int(index) => usize::try_from(index).ok(),
      _ => None,
    },
    _ => None,
  }
}

fn closure_index(value: &Value) -> Option<usize> {
  match value {
    Value::Variant { tag, payload: Some(payload) } if tag == FUNCTION_VALUE => match &**payload {
      Value::Int(n) => usize::try_from(*n).ok(),
      _ => None,
    },
    _ => None,
  }
}

/// Whether `value` is or holds a function value, which only a render can call.
pub(crate) fn holds_closure(value: &Value) -> bool {
  match value {
    Value::Seq(items) => items.iter().any(holds_closure),
    Value::Map(map) => map.values().any(holds_closure),
    other => closure_index(other).is_some(),
  }
}

/// One step of the path a key is taken under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
  /// The index of an enclosing loop's iteration, printed as the number.
  Iteration(usize),
  /// A keyed placement of a component, printed `c` and the placement id.
  Placement(u32),
}

/// The hoisted values of one island: keyed by the module, the hoist id and
/// the path enclosing it, `module|id@i.j` with `c<id>` for a keyed placement.
/// A key recorded twice with different values is dead: the browser computes
/// it instead.
/// The path a handler token names and, once a render reached the component
/// there, its module and the props it was given.
#[derive(Debug)]
pub(crate) struct Probe {
  pub(crate) path: String,
  pub(crate) found: Option<(String, ValueMap)>,
}

/// What `Hoists::id` puts before and after the hoist key, so an id FSR
/// minted reads apart from React's `_R_` and `_r_` ones.
pub const ID_OPEN: &str = "_S_";
pub const ID_CLOSE: &str = "_";

#[derive(Debug, Default, Clone)]
#[non_exhaustive]
pub struct Hoists {
  pub module: String,
  pub path: Vec<Step>,
  pub table: ValueMap,
  dead: Vec<String>,
  spelled: String,
}

impl Hoists {
  pub fn new(module: impl Into<String>) -> Self {
    Self { module: module.into(), path: Vec::new(), table: ValueMap::default(), dead: Vec::new(), spelled: String::new() }
  }

  /// The region key of an island placement, numbered apart from the hoist
  /// candidates and marked `i` so the two namespaces cannot be read as one.
  pub fn island_key(&self, id: u32) -> String {
    let mut digits = itoa::Buffer::new();
    let mut marked = String::with_capacity(12);
    marked.push('i');
    marked.push_str(digits.format(id));
    self.keyed(&marked)
  }

  pub fn key(&self, id: u32) -> String {
    let mut digits = itoa::Buffer::new();
    self.keyed(digits.format(id))
  }

  /// The id a `useId` call numbered `id` yields: `ID_OPEN`, the hoist key
  /// and `ID_CLOSE`, which the browser's reader spells the same way from the
  /// module and path it tracks. Valid as a DOM id and unique within a root,
  /// as React's own is.
  pub fn id(&self, id: u32) -> String {
    let key = self.key(id);
    let mut out = String::with_capacity(ID_OPEN.len() + key.len() + ID_CLOSE.len());
    out.push_str(ID_OPEN);
    out.push_str(&key);
    out.push_str(ID_CLOSE);
    out
  }

  fn keyed(&self, id: &str) -> String {
    let mut key = String::with_capacity(self.module.len() + id.len() + 2 + self.path.len() * 3);
    self.write_keyed(id, &mut key);
    key
  }

  fn write_keyed(&self, id: &str, key: &mut String) {
    key.push_str(&self.module);
    key.push('|');
    key.push_str(id);
    if !self.path.is_empty() {
      key.push('@');
      self.write_path(key);
    }
  }

  /// The path alone, which addresses a component rendered inside an island
  /// in server mode: the steps joined by `.`, `c<id>` for a keyed placement,
  /// empty at the island's own component. Its state rides in the island's
  /// map as `<path>/<name>` and its handlers bind as `<path>/<index>`.
  pub fn path_key(&self) -> String {
    let mut key = String::with_capacity(self.path.len() * 3);
    self.write_path(&mut key);
    key
  }

  fn write_path(&self, key: &mut String) {
    for (i, step) in self.path.iter().enumerate() {
      if i > 0 {
        key.push('.');
      }
      let mut digits = itoa::Buffer::new();
      match step {
        Step::Iteration(index) => key.push_str(digits.format(*index)),
        Step::Placement(id) => {
          key.push('c');
          key.push_str(digits.format(*id));
        }
      }
    }
  }

  pub fn record(&mut self, id: u32, value: Value) {
    let mut key = std::mem::take(&mut self.spelled);
    key.clear();
    let mut digits = itoa::Buffer::new();
    self.write_keyed(digits.format(id), &mut key);
    if !self.dead.iter().any(|dead| *dead == key) {
      match self.table.get(key.as_str()) {
        Some(existing) if *existing == value => {}
        Some(_) => {
          self.table.shift_remove(key.as_str());
          self.dead.push(key.clone());
        }
        None => {
          self.table.insert(key.as_str(), value);
        }
      }
    }
    self.spelled = key;
  }
}

/// The furthest instant a JavaScript `Date` holds, in milliseconds either side of the epoch; past it a date is invalid.
const MAX_DATE_MS: f64 = 8.64e15;

/// The most items `range` may build, for the same reason as [`MAX_REPEAT`].
const MAX_RANGE: usize = 1_000_000;

/// The most pieces `split` may build, for the same reason as [`MAX_REPEAT`].
const MAX_SPLIT: usize = 1_000_000;

/// The largest string `repeat` may build, so a count from data cannot ask for
/// an allocation the process does not survive.
const MAX_REPEAT: usize = 64 * 1024 * 1024;

/// The field an instance of a class the build lowered carries its class in, `file#Class`.
pub const CLASS_FIELD: &str = "$c";

/// The constant the instance method `name` of the class `class` is lowered into, a function of the instance and the method's arguments.
pub fn class_method_key(class: &str, name: &str) -> String {
  format!("{class}.{name}")
}

/// The constant the getter `name` of `class` is lowered into, a function of the instance.
pub fn class_getter_key(class: &str, name: &str) -> String {
  format!("{class}:get:{name}")
}

/// The constant naming the class `class` extends, a string; absent for one that extends none.
pub fn class_parent_key(class: &str) -> String {
  format!("{class}:parent")
}

/// The `tracing` target a component's `console` calls write to.
pub const CONSOLE_TARGET: &str = "fsr::console";

/// JavaScript's `Number.MAX_SAFE_INTEGER`; a double past it has no exact
/// integer to promote to.
const SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// The key JavaScript makes of a number written as a property name, `0` for `0` and `1.5` for `1.5`.
pub fn number_key(n: f64) -> String {
  stringify(&Value::F64(n)).expect("a number is a scalar")
}

/// Where a call writes back what a function changed of an argument.
#[derive(PartialEq)]
enum Place {
  Var(String),
  Cell(Value),
}

#[cfg(test)]
mod held_tests;
