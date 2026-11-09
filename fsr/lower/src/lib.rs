//! Reads a TypeScript loader or actions module and lowers each body to the IR.
//! The recognised language is the IR's own; anything outside it is residue,
//! reported with the line and the construct.

pub mod assets;
pub mod component;
mod classes;
mod create_element;
pub mod extract;
mod methods;
pub mod hoist;
pub mod meaning;
pub use meaning::{COMPONENTS_MODULE, TEMPLATE_MODULE};
mod placements;
pub mod schema;
mod scope;
mod syntax;
pub mod testing;
pub mod svelte;
pub mod terminal;
pub mod vue;

use component::{arrow_body, FunctionBody};
use snapfire_fsr_ir::ast::{ArithOp, Body, Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use snapfire_fsr_ir::{standard_reach, Reach};
use oxc_allocator::Allocator;
use oxc_ast::ast as js;
use oxc_span::{GetSpan, Span};
pub(crate) use scope::Ctxt;

/// Where a component that does not lower was placed: the file holding the
/// tag, the one-based line and column of the tag, plus the name as written.
/// A residue collects one of these per level it is re-raised through, so the
/// page that stops being server rendered can name the path down to the cause.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Placement {
  pub file: String,
  pub line: usize,
  pub column: usize,
  pub tag: String,
}

impl std::fmt::Display for Placement {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "<{}> {}:{}:{}", self.tag, self.file, self.line, self.column)
  }
}

/// Why a body is not IR. `line` and `column` are one-based in the source file.
/// `hint` names the rewrite that does the same thing in the IR, printed on a
/// second indented line, per DX.md section 5. `via` is the chain of placements
/// from the module the build asked for down to the file this residue is in,
/// empty when they are the same file.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Residue {
  pub file: String,
  pub line: usize,
  pub column: usize,
  pub message: String,
  pub hint: Option<String>,
  pub via: Vec<Placement>,
}

impl Residue {
  /// Records that a component holding this residue was placed at `at`, so the
  /// chain reads from the outermost module inwards.
  pub fn placed_at(mut self, at: Placement) -> Self {
    self.via.insert(0, at);
    self
  }

  /// The chain as one line, outermost placement first.
  pub fn chain(&self) -> String {
    self.via.iter().map(Placement::to_string).collect::<Vec<_>>().join(", ")
  }
}

impl std::fmt::Display for Residue {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}:{}:{}: {}", self.file, self.line, self.column, self.message)?;
    if !self.via.is_empty() {
      write!(f, "\n  reached through {}", self.chain())?;
    }
    match &self.hint {
      Some(hint) => write!(f, "\n  {hint}"),
      None => Ok(()),
    }
  }
}

impl std::error::Error for Residue {}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LowerError {
  #[error("{file}: {message}")]
  Parse { file: String, message: String },
  #[error("{file}: no exported `{export}`")]
  MissingExport { file: String, export: String },
  #[error(transparent)]
  Residue(#[from] Residue),
  /// A `body` extension on a component's render path. Never a client
  /// downgrade: the browser would run the component too.
  #[error("{0}; it runs on the server only, and a component's render path runs in the browser too")]
  Reach(Residue),
  /// An export under `ext/` that does not lower or a `native` declaration
  /// the build cannot read.
  #[error("{0}; every export under ext/ is an extension and must lower")]
  Extension(Residue),
  /// A spelling FSR no longer takes, refused with what to write instead.
  #[error("{0}")]
  Retired(Residue),
  /// Children a React component gives a component another framework mounts,
  /// which cannot cross into it.
  #[error("{0}")]
  ForeignChildren(Residue),
}

pub use schema::{builtin_types, read_schema, read_session_defaults, SchemaType, UPLOAD};

/// What a page reaching for Node's modules or its `process` global is told to use.
pub(crate) const NOT_FSR: &str = "read files and paths through `file` and `path` from `@snapfire/fsr/host`, which a web host confines to the directories `[files] roots` names; a terminal program reads the machine through `system` from `@snapfire/fsr/terminal` and its screen through Ink's `useStdout` and `useApp`; `fsr migrate` rewrites Node's onto them";

/// Whether `source` is one of Node's own modules, `node:fs` or its bare name `fs`.
pub(crate) fn node_module(source: &str) -> bool {
  const NODE: [&str; 22] = ["fs", "fs/promises", "path", "os", "process", "child_process", "crypto", "async_hooks", "util", "events", "stream", "url", "http", "https", "net", "readline", "buffer", "zlib", "tty", "worker_threads", "timers", "assert"];
  source.starts_with("node:") || NODE.contains(&source)
}

/// JavaScript's error constructors, which a `throw` and a `new` read as `{ name, message }`.
pub(crate) const ERROR_CONSTRUCTORS: [&str; 7] = ["Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError"];

/// The import aliases every fsr application has, each a prefix and the app
/// directory it stands for. The build writes them into both tsconfigs, snapfirec
/// rewrites them for the browser and the lowerers resolve them here.
pub const ALIASES: &[(&str, &str)] = &[("@app/", ""), ("@routes/", "routes/"), ("@src/", "src/"), ("@schemas/", "schemas/"), ("@generated/", "generated/"), ("@ext/", "ext/")];

/// Where `key` comes from: the client library's store.
pub const STORE_SPECIFIER: &str = "@snapfire/fsr/client/store";

/// The directory under the app whose modules are extensions: every export
/// lowers or the build fails, where a `native` declaration also lives.
pub const EXT_DIR: &str = "ext";

/// What a body lowerer resolves through the component set: module-level
/// names it could not bind and the native pairs declared so far.
#[derive(Debug, Clone, Default)]
pub(crate) struct Resolved {
  pub globals: Vec<(String, Expr)>,
  pub global_kinds: Vec<(String, Kind)>,
  /// The resolved module-level names that hold an `Intl` formatter, with the source text of the `new` that made each.
  pub global_formatters: Vec<(String, String)>,
  pub natives: Vec<(String, Reach)>,
  pub classes: Option<std::rc::Rc<std::cell::RefCell<crate::classes::ClassTable>>>,
  pub type_kinds: Vec<(String, AliasKind)>,
  /// The reaches the set's pages may use besides `Render` and `Body`, which its loaders, actions and data exports share.
  pub reaches: Vec<Reach>,
}

/// A body that did not lower, with the name the set may bind and retry.
pub(crate) type Unresolved = (LowerError, Option<String>);

/// The exact specifiers the build's tsconfig maps to a generated module, so
/// the lowerer resolves `@snapfire/fsr` to the same file the editor does.
pub const GENERATED: &[(&str, &str)] = &[("@snapfire/fsr/head", "generated/head")];

/// The one module under `generated/` a body may call into. Everything else
/// there is types or the `action` and `fail` the lowerer answers by name
/// rather than by reading; this file holds nothing but lowerable helpers.
pub const HEAD_MODULE: &str = "generated/head.ts";

/// The store keys the terminal sets to its size, which Ink's `useWindowSize` and `useStdout().stdout` read.
pub const TERMINAL_COLUMNS: &str = "$terminal.columns";
pub const TERMINAL_ROWS: &str = "$terminal.rows";

/// The store key the terminal sets to the focused id, or null, which Ink's `useFocus` and `useFocusManager().activeId` read.
pub const TERMINAL_FOCUS: &str = "$terminal.focus";

/// The store key the terminal sets to what it measured for each box a ref is on, by the ref's id, which Ink's `useBoxMetrics` reads.
pub const TERMINAL_METRICS: &str = "$terminal.metrics";

/// The field a terminal `useRef` holds its id in beside `current`, which `ref` on an Ink element writes as `ink-ref`.
pub const REF_ID: &str = "$id";

/// The module `fsr` writes from `[mdx.collections]`, which a body may read:
/// every content file under each collection's directory with its `meta` and `toc`.
pub const CONTENT_MODULE: &str = "generated/content.ts";

/// A specifier as a path relative to the app: an alias expanded or a relative
/// specifier joined to `from`'s directory. `None` for a bare specifier.
pub fn resolve_specifier(from: &str, specifier: &str) -> Option<String> {
  for (exact, path) in GENERATED {
    if specifier == *exact {
      return Some((*path).to_owned());
    }
  }
  for (alias, dir) in ALIASES {
    if let Some(rest) = specifier.strip_prefix(alias) {
      return Some(normalize_path(std::path::Path::new(&format!("{dir}{rest}"))));
    }
  }
  if !specifier.starts_with('.') {
    return None;
  }
  let dir = std::path::Path::new(from).parent().unwrap_or(std::path::Path::new(""));
  Some(normalize_path(&dir.join(specifier)))
}

fn normalize_path(path: &std::path::Path) -> String {
  let mut parts: Vec<String> = Vec::new();
  for component in path.components() {
    match component {
      std::path::Component::ParentDir => {
        parts.pop();
      }
      std::path::Component::CurDir => {}
      other => parts.push(other.as_os_str().to_string_lossy().into_owned()),
    }
  }
  parts.join("/")
}

/// A lowered action: the exported name, the body and the input type when
/// there is one, read from the parameter's `ActionCtx<T>` annotation or from
/// the older `action<T>(...)` spelling.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LoweredAction {
  pub export: String,
  pub input: Option<String>,
  pub body: Body,
}

/// A lowered route handler: the HTTP method it answers, the body and the
/// input type when the export is an `action<T>`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct LoweredHandler {
  pub method: String,
  pub input: Option<String>,
  pub body: Body,
}

/// The exports of a `route.ts` that are handlers.
pub const HANDLER_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

/// Names of JavaScript's own methods. A call of one that no arm lowers is refused by name rather than read as a call of a function value held in a field.
const STANDARD_METHODS: &[&str] = &["flat", "entries", "keys", "values", "forEach", "push", "pop", "shift", "unshift", "splice", "fill", "copyWithin", "findLast", "findLastIndex", "reduceRight", "toSpliced", "with", "normalize", "trimStart", "trimEnd", "toLocaleUpperCase", "toLocaleLowerCase", "hasOwnProperty", "isPrototypeOf", "propertyIsEnumerable", "toPrecision", "toExponential", "then", "catch", "finally", "get", "set", "has", "delete", "add", "clear", "anchor", "bind", "call", "apply"];

/// Session keys with the value a body reads when the key is absent, from
/// `export const defaults` in the session schema. A read of such a key lowers
/// to `session.key ?? default`.
pub type SessionDefaults = Vec<(String, Expr)>;

/// Lowers the exported `load` of a loader module.
pub fn lower_loader(file: &str, source: &str) -> Result<Body, LowerError> {
  lower_loader_with(file, source, &SessionDefaults::new())
}

pub fn lower_loader_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_loader(file)
}

std::thread_local! {
  static HEAP_PASS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
  static HEAP_WANTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
  static HEAP_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn heap_pass() -> bool {
  HEAP_PASS.get()
}

pub(crate) fn want_heap() {
  HEAP_WANTED.set(true);
}

/// `run`, then `run` again with the heap when the first pass wrote an object or could not lower the body.
pub(crate) fn with_heap<T, E: std::fmt::Debug>(mut run: impl FnMut() -> Result<T, E>, mark: impl FnOnce(&mut T)) -> Result<T, E> {
  if HEAP_ACTIVE.get() {
    return run();
  }
  with_heap_fresh(run, mark)
}

/// [`with_heap`] for a program of its own inside another's lowering.
pub(crate) fn with_heap_fresh<T, E: std::fmt::Debug>(mut run: impl FnMut() -> Result<T, E>, mark: impl FnOnce(&mut T)) -> Result<T, E> {
  let (outer_pass, outer_wanted, outer_active) = (HEAP_PASS.replace(false), HEAP_WANTED.replace(false), HEAP_ACTIVE.replace(true));
  let first = run();
  let out = if first.is_ok() && !HEAP_WANTED.get() {
    first
  } else {
    let first_ok = first.is_ok();
    HEAP_PASS.set(true);
    let second = run();
    HEAP_PASS.set(false);
    match second {
      Ok(mut value) => {
        mark(&mut value);
        Ok(value)
      }
      Err(error) if first_ok => Err(error),
      Err(second) => {
        if std::env::var("FSR_HEAP_LOG").is_ok() {
          eprintln!("heap pass refused too: {second:?}");
        }
        first
      }
    }
  };
  HEAP_PASS.set(outer_pass);
  HEAP_WANTED.set(outer_wanted);
  HEAP_ACTIVE.set(outer_active);
  out
}

pub(crate) fn heap_marker() -> Stmt {
  Stmt::Let { name: "$heap".to_owned(), expr: Expr::Builtin { name: Builtin::HeapOn, args: Vec::new() } }
}

pub(crate) fn lower_loader_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  with_heap(|| lower_loader_once(parsed, defaults, resolved), |body: &mut Body| body.insert(0, heap_marker()))
}

fn lower_loader_once(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let Some(function) = parsed.exports().find_map(|(name, decl)| (name == "load").then_some(decl)) else {
    if parsed.exports().any(|(name, _)| ["meta", "store", "defaults", "paths"].contains(&name)) {
      return Ok(vec![Stmt::Return(Expr::Object(Vec::new()))]);
    }
    return Err((LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None));
  };
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err((LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None)),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`load` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`load` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`load` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

/// Lowers the exported `meta` of a loader module when there is one: a
/// function of `{ data }`, the loader's result, returning the document's
/// `title` and `description`. `None` when the module exports no `meta`.
pub fn lower_meta_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError> {
  lower_of_data(file, source, defaults, "meta")
}

/// Lowers the exported `store` of a loader module when there is one: a
/// function of `{ data }` returning the store keys the route seeds. `None`
/// when the module exports no `store`.
pub fn lower_store_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError> {
  lower_of_data(file, source, defaults, "store")
}

/// Lowers the exported `defaults` of a loader module when there is one: a
/// function of `{ data }` returning store keys the route seeds as defaults,
/// which a value an island wrote in the browser keeps winning. `None` when
/// the module exports no `defaults`.
pub fn lower_defaults_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError> {
  lower_of_data(file, source, defaults, "defaults")
}

/// Lowers the exported `paths` of a page loader module when there is one: a
/// function of the context returning the parameter sets the route
/// prerenders, one object per path. `None` when the module exports no `paths`.
pub fn lower_paths_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_paths(file)
}

/// An export whose input is the loader's data, bound as `data`.
fn lower_of_data(file: &str, source: &str, defaults: &SessionDefaults, export: &str) -> Result<Option<Body>, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_of_data(file, export)
}

pub(crate) fn lower_of_data_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, export: &str) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, export, true)
}

pub(crate) fn lower_paths_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, "paths", false)
}

/// A card image module's `images`, or `generateImageMetadata` as Next names
/// it: a function of the context returning the image list.
pub(crate) fn lower_images_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Option<Body>, Unresolved> {
  match lower_optional_export_in(parsed, defaults, resolved, "images", false)? {
    Some(body) => Ok(Some(body)),
    None => lower_optional_export_in(parsed, defaults, resolved, "generateImageMetadata", false),
  }
}

/// An optional exported function of one parameter: the loader's data as
/// `data` when `of_data`, the context otherwise.
fn lower_optional_export_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, export: &str, of_data: bool) -> Result<Option<Body>, Unresolved> {
  let Some(exported) = parsed.exports().find_map(|(name, decl)| (name == export).then_some(decl)) else { return Ok(None) };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.meta = of_data;
  let takes = if of_data { "`{ data }`" } else { "the context" };
  let result = match exported {
    Exported::Function(first, body) => lowerer.bind_ctx(first).and_then(|()| lowerer.block(body)),
    Exported::Expr(first, expr) => lowerer.bind_ctx(first).and_then(|()| lowerer.expr(expr)).map(|e| vec![Stmt::Return(e)]),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`{export}` is {what}")).into(), None)),
    Exported::Value(value) if of_data && export == "meta" => lowerer.expr(value).map(|e| vec![Stmt::Return(e)]),
    Exported::Action { .. } | Exported::BadAction(_) | Exported::Value(_) | Exported::Other(_) => return Err((parsed.residue(parsed.module().span, format!("`{export}` must be a function of {takes}")).into(), None)),
  };
  result.map(Some).map_err(|r| (r.into(), lowerer.unbound.take()))
}

/// One entry of the object `useMDXComponents` returns in `mdx-components.tsx`:
/// a component by the name it has in that file, or any other expression by
/// its bytes there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappedValue {
  Name(String),
  Expr(std::ops::Range<usize>),
}

/// The elements `mdx-components.tsx` maps, in order: each key of the object
/// its `useMDXComponents` returns, a spread of the components it is given left out.
pub fn mdx_component_map(file: &str, source: &str) -> Result<Vec<(String, MappedValue)>, LowerError> {
  let arena = Allocator::default();
  let parsed = parse_with(&arena, file, source, true)?;
  let unreadable = |span: Span, message: &str| LowerError::Residue(parsed.residue(span, message));
  let Some(found) = component::find_function(&parsed, "useMDXComponents") else {
    return Err(unreadable(parsed.module().span, "no `useMDXComponents` function; export one returning the map, as Next's `mdx-components.tsx` does"));
  };
  fn returned_in<'s, 'a>(stmts: &'s [js::Statement<'a>]) -> Option<&'s js::Expression<'a>> {
    stmts.iter().rev().find_map(|stmt| match stmt {
      js::Statement::ReturnStatement(ret) => ret.argument.as_ref(),
      _ => None,
    })
  }
  let returned: Option<&js::Expression> = match &found {
    component::Found::Declared(_, stmts, _) => returned_in(stmts),
    component::Found::Arrow(arrow) => match arrow_body(arrow) {
      FunctionBody::Expr(e) => Some(e),
      FunctionBody::Block(stmts) => returned_in(stmts),
    },
  };
  let mut expr = returned.ok_or_else(|| unreadable(parsed.module().span, "`useMDXComponents` returns nothing"))?;
  loop {
    expr = match expr {
      js::Expression::ParenthesizedExpression(inner) => &inner.expression,
      js::Expression::TSAsExpression(inner) => &inner.expression,
      js::Expression::TSSatisfiesExpression(inner) => &inner.expression,
      _ => break,
    };
  }
  let js::Expression::ObjectExpression(object) = expr else { return Err(unreadable(expr.span(), "`useMDXComponents` returns something other than an object written out")) };
  let mut out = Vec::new();
  for prop in &object.properties {
    match prop {
      js::ObjectPropertyKind::SpreadProperty(_) => {}
      js::ObjectPropertyKind::ObjectProperty(p) if p.kind == js::PropertyKind::Init && !p.method && p.shorthand => {
        let js::Expression::Identifier(id) = &p.value else { return Err(unreadable(p.span, "an entry in `useMDXComponents` other than `key: Component`")) };
        out.push((id.name.to_string(), MappedValue::Name(id.name.to_string())));
      }
      js::ObjectPropertyKind::ObjectProperty(p) if p.kind == js::PropertyKind::Init && !p.method => {
        let Some(key) = prop_name(&p.key, p.computed) else { return Err(unreadable(p.key.span(), "a computed key in `useMDXComponents`")) };
        let value = match &p.value {
          js::Expression::Identifier(id) => MappedValue::Name(id.name.to_string()),
          other => MappedValue::Expr(parsed.range(other.span())),
        };
        out.push((key, value));
      }
      js::ObjectPropertyKind::ObjectProperty(other) => return Err(unreadable(other.span, "an entry in `useMDXComponents` other than `key: Component`")),
    }
  }
  Ok(out)
}

/// Lowers every `export const name = action(...)` of an actions module.
pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError> {
  lower_actions_with(file, source, &SessionDefaults::new())
}

pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_actions(file)
}

pub(crate) fn lower_actions_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Vec<LoweredAction>, Unresolved> {
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    let (input, first, body) = match exported {
      Exported::Action { input, first, body } => (input, first, body),
      Exported::BadAction(span) => return Err((parsed.residue(span, format!("`{name}` is an `action(...)` of something other than a function")).into(), None)),
      Exported::Unreadable(span, what) => {
        let hint = "every export of an actions module is an `action(...)`; the build passes over a type or a plain value and refuses what it cannot read";
        return Err((Residue { hint: Some(hint.to_owned()), ..parsed.residue(span, format!("`{name}` is {what}")) }.into(), None));
      }
      _ => continue,
    };
    let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
    lowerer.extends = true;
    lowerer.redirects = true;
    let body = with_heap(|| { let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved); lowerer.extends = true; lowerer.redirects = true; lower_function(&mut lowerer, first, body.clone()).map_err(|r| -> Unresolved { (r.into(), lowerer.unbound.take()) }) }, |body: &mut Body| body.insert(0, heap_marker()))?;
    let _ = &mut lowerer;
    out.push(LoweredAction { export: name.to_owned(), input, body });
  }
  Ok(out)
}

/// Lowers every export of a `route.ts` named for an HTTP method, in file
/// order. A method exported as a plain function reads the request body as
/// `input` unchecked; one exported as `action<T>(...)` has it checked
/// against `T` first.
pub fn lower_handlers(file: &str, source: &str) -> Result<Vec<LoweredHandler>, LowerError> {
  lower_handlers_with(file, source, &SessionDefaults::new())
}

pub fn lower_handlers_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredHandler>, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_handlers(file)
}

pub(crate) fn lower_handlers_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Vec<LoweredHandler>, Unresolved> {
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    if !HANDLER_METHODS.contains(&name) {
      continue;
    }
    let (input, first, body) = match exported {
      Exported::Function(first, body) => (None, first, FunctionBody::Block(body)),
      Exported::Action { input, first, body } => (input, first, body),
      Exported::Expr(first, e) => (None, first, FunctionBody::Expr(e)),
      Exported::BadAction(span) => return Err((parsed.residue(span, format!("`{name}` is an `action(...)` of something other than a function")).into(), None)),
      Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`{name}` is {what}")).into(), None)),
      Exported::Value(value) => return Err((parsed.residue(value.span(), format!("`{name}` must be a function or an `action(...)`")).into(), None)),
      Exported::Other(span) => return Err((parsed.residue(span, format!("`{name}` must be a function or an `action(...)`")).into(), None)),
    };
    let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
    lowerer.extends = true;
    lowerer.redirects = true;
    let body = lower_function(&mut lowerer, first, body).map_err(|r| (r.into(), lowerer.unbound.take()))?;
    out.push(LoweredHandler { method: name.to_owned(), input, body });
  }
  Ok(out)
}

/// Lowers the exported `middleware` of `middleware.ts`. The body reads the
/// request line as `request` (`method` and `path`), which reaches it as the
/// input. It returns nothing to continue or a map naming `redirect`,
/// `rewrite`, `status`, `body` or `headers`.
pub fn lower_middleware(file: &str, source: &str) -> Result<Body, LowerError> {
  lower_middleware_with(file, source, &SessionDefaults::new())
}

pub fn lower_middleware_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_middleware(file)
}

pub fn lower_theme(file: &str, source: &str) -> Result<Body, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).provide(file, source).lower_theme(file)
}

/// `export function theme(ctx)` of `theme.ts`: the document's theme for one request, with
/// `ctx.theme` the route's own as the plan names it, read as the body's input.
pub(crate) fn lower_theme_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let missing = || (LowerError::MissingExport { file: file.to_owned(), export: "theme".to_owned() }, None);
  let function = parsed.exports().find_map(|(name, decl)| (name == "theme").then_some(decl)).ok_or_else(missing)?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err(missing()),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`theme` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`theme` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`theme` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.theming = true;
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

pub(crate) fn lower_middleware_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let function = parsed
    .exports()
    .find_map(|(name, decl)| (name == "middleware").then_some(decl))
    .ok_or_else(|| (LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() }, None))?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err((LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() }, None)),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`middleware` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`middleware` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`middleware` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.middleware = true;
  lowerer.extends = true;
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

pub(crate) struct Parsed<'a> {
  pub(crate) file: String,
  program: js::Program<'a>,
  /// The arena the module lives in, where the lowerers write the nodes they add.
  arena: &'a Allocator,
  /// Which binding each identifier of the module names.
  pub(crate) scope: std::rc::Rc<scope::Scope>,
  /// The byte offset each line of the module's text starts at.
  line_starts: Vec<usize>,
  /// Expressions parsed at a line and column of the file after the module was, whose spans start past its text.
  snippets: std::cell::RefCell<Vec<Snippet>>,
  /// What the module's names mean, filled when a component set resolves the file.
  pub(crate) meanings: std::cell::OnceCell<std::rc::Rc<meaning::Meanings>>,
  /// The file the module was written from, when it was not written by hand.
  pub(crate) origin: Option<std::rc::Rc<Origin>>,
}

/// An expression [`Parsed::parse_expr_at`] read: where its spans start and its text, padded to the line and column it was written at.
struct Snippet {
  base: u32,
  padded: String,
}

/// A parameter as written: its pattern, its type, its default and whether it gathers the rest.
#[derive(Clone, Copy)]
pub(crate) struct Param<'a> {
  pub(crate) pattern: &'a js::BindingPattern<'a>,
  pub(crate) type_annotation: Option<&'a js::TSTypeAnnotation<'a>>,
  pub(crate) default: Option<&'a js::Expression<'a>>,
  pub(crate) rest: bool,
  pub(crate) span: Span,
}

impl<'a> Param<'a> {
  /// The name the parameter binds when it is one name with no default, as a plain `x` or `x: T`.
  pub(crate) fn ident(&self) -> Option<&'a js::BindingIdentifier<'a>> {
    match self.pattern {
      js::BindingPattern::BindingIdentifier(id) if !self.rest && self.default.is_none() => Some(id),
      _ => None,
    }
  }
}

impl<'a> Param<'a> {
  /// A pattern standing where a parameter would, as a declaration binds it: no type, no default, no rest.
  pub(crate) fn of_pattern(pattern: &'a js::BindingPattern<'a>) -> Self {
    Param { pattern, type_annotation: None, default: None, rest: false, span: pattern.span() }
  }
}

/// Each parameter of `params` in order, the rest parameter last.
pub(crate) fn params_of<'a>(params: &'a js::FormalParameters<'a>) -> Vec<Param<'a>> {
  let mut out: Vec<Param<'a>> = params
    .items
    .iter()
    .map(|p| match &p.pattern {
      js::BindingPattern::AssignmentPattern(assign) => Param { pattern: &assign.left, type_annotation: p.type_annotation.as_deref(), default: Some(&assign.right), rest: false, span: p.span },
      pattern => Param { pattern, type_annotation: p.type_annotation.as_deref(), default: p.initializer.as_deref(), rest: false, span: p.span },
    })
    .collect();
  if let Some(rest) = &params.rest {
    out.push(Param { pattern: &rest.rest.argument, type_annotation: rest.type_annotation.as_deref(), default: None, rest: true, span: rest.span });
  }
  out
}

/// The first parameter of `params`.
pub(crate) fn first_param<'a>(params: &'a js::FormalParameters<'a>) -> Option<Param<'a>> {
  params_of(params).into_iter().next()
}

pub(crate) enum Exported<'a> {
  Function(Option<Param<'a>>, &'a [js::Statement<'a>]),
  /// An arrow whose body is one expression.
  Expr(Option<Param<'a>>, &'a js::Expression<'a>),
  /// `action(<function>)`, the function an arrow with either body or a function expression.
  Action { input: Option<String>, first: Option<Param<'a>>, body: FunctionBody<'a> },
  /// `action(<something that is not a function>)`, which no lowering accepts.
  BadAction(Span),
  /// An export the build tried to read and could not classify, as against one
  /// it classified as something else. Never skipped in silence.
  Unreadable(Span, &'static str),
  /// An object literal, which `meta` may be when it reads nothing from the route's data.
  Value(&'a js::Expression<'a>),
  Other(Span),
}

/// The statements of a function's body, none for a declaration without one.
pub(crate) fn body_of<'a>(f: &'a js::Function<'a>) -> &'a [js::Statement<'a>] {
  f.body.as_ref().map(|b| b.statements.as_slice()).unwrap_or(&[])
}

/// A string literal's value, a lone surrogate read as U+FFFD.
pub(crate) fn lossy(value: oxc_str::JSStr<'_>) -> String {
  match value.as_str() {
    Some(text) => text.to_owned(),
    None => value.chars().map(|c| c.to_char().unwrap_or('\u{FFFD}')).collect(),
  }
}

/// Every name a pattern binds, for a declaration shape the build does not
/// otherwise read: the names are reported so nothing is skipped unseen.
fn bound_names<'a>(pat: &'a js::BindingPattern<'a>, out: &mut impl FnMut(&'a str, Span)) {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => out(id.name.as_str(), id.span),
    js::BindingPattern::ArrayPattern(arr) => {
      arr.elements.iter().flatten().for_each(|p| bound_names(p, out));
      if let Some(rest) = &arr.rest {
        bound_names(&rest.argument, out);
      }
    }
    js::BindingPattern::AssignmentPattern(assign) => bound_names(&assign.left, out),
    js::BindingPattern::ObjectPattern(obj) => {
      for prop in &obj.properties {
        bound_names(&prop.value, out);
      }
      if let Some(rest) = &obj.rest {
        bound_names(&rest.argument, out);
      }
    }
  }
}

/// Whether `source` declares `name` at module level without exporting it, which is a component the build can mount only once the bundle overlay exports it.
pub fn declares_unexported(file: &str, source: &str, name: &str) -> bool {
  let arena = Allocator::default();
  let Ok(parsed) = parse_with(&arena, file, source, jsx_parses(file)) else { return false };
  parsed.local(name).is_some() && !parsed.exports().any(|(exported, _)| exported == name)
}

/// Whether `file` may hold JSX, as TypeScript decides: a `.tsx` or any
/// JavaScript file. A `.ts` cannot, since there `<T>x` is a type assertion.
pub(crate) fn jsx_parses(file: &str) -> bool {
  [".tsx", ".jsx", ".js", ".mjs", ".cjs"].iter().any(|ext| file.ends_with(ext))
}

pub(crate) fn parse<'a>(arena: &'a Allocator, file: &str, source: &str) -> Result<Parsed<'a>, LowerError> {
  parse_with(arena, file, source, false)
}

pub(crate) fn parse_with<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool) -> Result<Parsed<'a>, LowerError> {
  parse_from(arena, file, source, tsx, None)
}

/// Where a module the build reads came from when it was written from a file in
/// another language, an `.mdx` file among them: that file's text and a map
/// from each run of the module back to it, so a residue names the line the
/// author wrote rather than one of the module.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Origin {
  pub source: String,
  pub segments: Vec<OriginSegment>,
}

impl Origin {
  pub fn new(source: String, segments: Vec<OriginSegment>) -> Self {
    Self { source, segments }
  }
}

/// A run of the module: `len` bytes from `generated`. A verbatim run maps byte
/// for byte from `original`; any other maps whole to `original`.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct OriginSegment {
  pub generated: usize,
  pub len: usize,
  pub original: usize,
  pub verbatim: bool,
}

impl OriginSegment {
  pub fn new(generated: usize, len: usize, original: usize, verbatim: bool) -> Self {
    Self { generated, len, original, verbatim }
  }
}

impl Origin {
  /// The one-based line and column in the original text of byte `offset` of the module.
  pub fn position(&self, offset: usize) -> (usize, usize) {
    let index = self.segments.partition_point(|s| s.generated <= offset);
    let original = match index.checked_sub(1).map(|i| self.segments[i]) {
      Some(s) if s.verbatim && offset < s.generated + s.len.max(1) => s.original + (offset - s.generated),
      Some(s) => s.original,
      None => 0,
    };
    let mut end = original.min(self.source.len());
    while !self.source.is_char_boundary(end) {
      end -= 1;
    }
    let before = &self.source[..end];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
    (line, column)
  }
}

/// The byte offset each line of `text` starts at.
fn line_starts(text: &str) -> Vec<usize> {
  std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect()
}

/// The one-based line and column of byte `offset` of `text`, the column counted in characters.
fn line_column(text: &str, starts: &[usize], offset: usize) -> (usize, usize) {
  let line = starts.partition_point(|start| *start <= offset).max(1);
  let start = starts[line - 1];
  let mut end = offset.min(text.len());
  while !text.is_char_boundary(end) {
    end -= 1;
  }
  (line, text[start..end.max(start)].chars().count() + 1)
}

/// The one-based line and column a parse error points at, in the file the author wrote.
fn error_at(source: &str, origin: Option<&Origin>, error: &oxc_diagnostics::OxcDiagnostic) -> (usize, usize) {
  let offset = error.labels.first().map(|label| label.offset() as usize).unwrap_or(0);
  match origin {
    Some(origin) => origin.position(offset),
    None => line_column(source, &line_starts(source), offset),
  }
}

fn source_type(tsx: bool) -> oxc_span::SourceType {
  oxc_span::SourceType::ts().with_jsx(tsx)
}

pub(crate) fn parse_from<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool, origin: Option<std::rc::Rc<Origin>>) -> Result<Parsed<'a>, LowerError> {
  parse_into(arena, file, source, tsx, origin, |_, _, _| {})
}

/// [`parse_from`] with `edit` run over the tree once its scopes are known.
pub(crate) fn parse_into<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool, origin: Option<std::rc::Rc<Origin>>, edit: impl FnOnce(&'a Allocator, &scope::Scope, &mut js::Program<'a>)) -> Result<Parsed<'a>, LowerError> {
  let parse_error = |error: &oxc_diagnostics::OxcDiagnostic| {
    let (line, column) = error_at(source, origin.as_deref(), error);
    LowerError::Parse { file: file.to_owned(), message: format!("{line}:{column}: {}", error.message) }
  };
  let starts = line_starts(source);
  let at = |span: Span| match &origin {
    Some(origin) => origin.position(span.start as usize),
    None => line_column(source, &starts, span.start as usize),
  };
  let (program, scope) = (|| {
    let scope::Analysed { mut program, scope, errors, .. } = scope::analyse(arena, source, source_type(tsx));
    if let Some(error) = errors.first() {
      return Err(parse_error(error));
    }
    create_element::desugar(arena, &scope, &mut program);
    methods::desugar(&mut program);
    if let Some((name, again, first)) = bound_twice(&program) {
      let ((line, column), (first_line, _)) = (at(again), at(first));
      return Err(LowerError::Residue(Residue {
        file: file.to_owned(),
        line,
        column,
        message: format!("`{name}`, bound twice at the top of the module; its first binding is line {first_line}"),
        hint: Some("JavaScript refuses a module that binds one name twice; rename one of them".to_owned()),
        via: Vec::new(),
      }));
    }
    if let Some((source, fsr, span)) = replaced_import(&program) {
      let (line, column) = at(span);
      return Err(LowerError::Residue(Residue {
        file: file.to_owned(),
        line,
        column,
        message: format!("`{source}`, which is not FSR's"),
        hint: Some(format!("FSR provides its own; import it from `{fsr}`, which `fsr migrate` writes")),
        via: Vec::new(),
      }));
    }
    edit(arena, &scope, &mut program);
    Ok((program, scope))
  })()?;
  Ok(Parsed { file: file.to_owned(), program, arena, scope: std::rc::Rc::new(scope), line_starts: starts, snippets: std::cell::RefCell::new(Vec::new()), meanings: std::cell::OnceCell::new(), origin })
}

/// The specifier `stmt` imports or re-exports from, with the statement's span.
fn module_source<'a>(stmt: &'a js::Statement<'a>) -> Option<(&'a js::StringLiteral<'a>, Span)> {
  match stmt {
    js::Statement::ImportDeclaration(import) => Some((&import.source, import.span)),
    js::Statement::ExportFromDeclaration(export) => Some((&export.source, export.span)),
    js::Statement::ExportAllDeclaration(all) => Some((&all.source, all.span)),
    _ => None,
  }
}

/// The first import or re-export in `module` from an npm package FSR provides its own of ([`terminal::REPLACED`]), with FSR's module and where.
fn replaced_import(module: &js::Program<'_>) -> Option<(String, &'static str, Span)> {
  module.body.iter().find_map(|stmt| {
    let (src, span) = module_source(stmt)?;
    let source = lossy(src.value);
    terminal::REPLACED.iter().find(|(npm, _)| *npm == source).map(|(_, fsr)| (source, *fsr, span))
  })
}

/// The first name `module` binds twice at its top level, with the spans of the second binding and the first: an import, a `let`, `const` or `var`, a function or a class. Two `var`s of one name, a function's overload signatures and `declare` statements bind it once, as JavaScript and TypeScript have it; types, enums and namespaces are a namespace of their own and are left out.
fn bound_twice(module: &js::Program<'_>) -> Option<(String, Span, Span)> {
  let mut bindings = Bindings(Vec::new());
  for stmt in &module.body {
    let twice = match stmt {
      js::Statement::ImportDeclaration(import) if !import.import_kind.is_type() => import.specifiers.iter().flatten().find_map(|specifier| match specifier {
        js::ImportDeclarationSpecifier::ImportSpecifier(n) if n.import_kind.is_type() => None,
        js::ImportDeclarationSpecifier::ImportSpecifier(n) => bindings.bind(&n.local, false),
        js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => bindings.bind(&d.local, false),
        js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => bindings.bind(&n.local, false),
      }),
      js::Statement::ExportDeclaration(export) => bindings.decl(&export.declaration),
      js::Statement::ExportDefaultDeclaration(export) => match &export.declaration {
        js::ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.body.is_some() => f.id.as_ref().and_then(|id| bindings.bind(id, false)),
        js::ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref().and_then(|id| bindings.bind(id, false)),
        _ => None,
      },
      other => other.as_declaration().and_then(|decl| bindings.decl(decl)),
    };
    if twice.is_some() {
      return twice;
    }
  }
  None
}

/// The names a module has bound so far, each with its span and whether a `var` bound it.
struct Bindings(Vec<(String, Span, bool)>);

impl Bindings {
  /// Binds `id`, answering the name and both spans when it was bound already by anything but two `var`s.
  fn bind(&mut self, id: &js::BindingIdentifier, var: bool) -> Option<(String, Span, Span)> {
    match self.0.iter().find(|(name, _, _)| *name == id.name.as_str()) {
      Some((_, _, true)) if var => None,
      Some((name, first, _)) => Some((name.clone(), id.span, *first)),
      None => {
        self.0.push((id.name.to_string(), id.span, var));
        None
      }
    }
  }

  fn decl(&mut self, decl: &js::Declaration) -> Option<(String, Span, Span)> {
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.body.is_some() && !f.declare => f.id.as_ref().and_then(|id| self.bind(id, false)),
      js::Declaration::ClassDeclaration(c) if !c.declare => c.id.as_ref().and_then(|id| self.bind(id, false)),
      js::Declaration::VariableDeclaration(var) if !var.declare => {
        let is_var = var.kind == js::VariableDeclarationKind::Var;
        let mut names = Vec::new();
        for d in &var.declarations {
          pattern_names(&d.id, &mut names);
        }
        names.iter().find_map(|id| self.bind(id, is_var))
      }
      _ => None,
    }
  }
}

/// Every name `pat` binds, in order.
pub(crate) fn pattern_names<'p>(pat: &'p js::BindingPattern<'p>, out: &mut Vec<&'p js::BindingIdentifier<'p>>) {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => out.push(id),
    js::BindingPattern::ArrayPattern(array) => {
      array.elements.iter().flatten().for_each(|p| pattern_names(p, out));
      if let Some(rest) = &array.rest {
        pattern_names(&rest.argument, out);
      }
    }
    js::BindingPattern::ObjectPattern(object) => {
      for prop in &object.properties {
        pattern_names(&prop.value, out);
      }
      if let Some(rest) = &object.rest {
        pattern_names(&rest.argument, out);
      }
    }
    js::BindingPattern::AssignmentPattern(a) => pattern_names(&a.left, out),
  }
}

/// Moves every span in `expr` on by `by`.
fn shift_spans(expr: &mut js::Expression<'_>, by: u32) {
  struct Shift(u32);
  impl<'a> oxc_ast_visit::VisitMut<'a> for Shift {
    fn visit_span(&mut self, span: &mut Span) {
      *span = Span::new(span.start + self.0, span.end + self.0);
    }
  }
  oxc_ast_visit::VisitMut::visit_expression(&mut Shift(by), expr);
}

impl<'a> Parsed<'a> {
  /// The module.
  pub(crate) fn module(&self) -> &js::Program<'a> {
    &self.program
  }

  /// The arena the module lives in, for the nodes the lowerers write.
  pub(crate) fn arena(&self) -> &'a Allocator {
    self.arena
  }

  /// The module's text.
  pub(crate) fn text(&self) -> &str {
    self.module().source_text
  }

  /// What `id` names where it is written.
  pub(crate) fn ctxt(&self, id: &js::IdentifierReference) -> Ctxt {
    self.scope.of(id)
  }

  /// What a binding binds.
  pub(crate) fn binding_ctxt(&self, id: &js::BindingIdentifier) -> Ctxt {
    self.scope.of_binding(id)
  }

  /// Parses `source` as one TypeScript expression written at `line` and
  /// `column` of this file. Its spans start past the module's text, so a
  /// residue inside it names where the expression sits.
  pub(crate) fn parse_expr_at(&self, source: &str, line: usize, column: usize) -> Result<js::Expression<'a>, String> {
    let padded = format!("{}{}{source}", "\n".repeat(line.saturating_sub(1)), " ".repeat(column.saturating_sub(1)));
    let text = self.arena().alloc_str(&padded);
    let mut expr = oxc_parser::Parser::new(self.arena(), text, oxc_span::SourceType::ts()).parse_expression().map_err(|errors| errors.first().map(|e| e.message.to_string()).unwrap_or_default())?;
    let mut snippets = self.snippets.borrow_mut();
    let base = snippets.last().map(|s| s.base as usize + s.padded.len() + 1).unwrap_or(self.text().len() + 1) as u32;
    shift_spans(&mut expr, base);
    snippets.push(Snippet { base, padded });
    Ok(expr)
  }

  /// The byte range of `span` in the file's text, or in the text of the expression that holds it.
  pub(crate) fn range(&self, span: Span) -> std::ops::Range<usize> {
    let base = self.snippet_base(span.start);
    (span.start - base) as usize..(span.end - base) as usize
  }

  /// Where the text `pos` falls in starts: 0 for the module, a snippet's base for an expression parsed after it.
  fn snippet_base(&self, pos: u32) -> u32 {
    self.snippets.borrow().iter().rev().find(|s| s.base <= pos).map(|s| s.base).unwrap_or(0)
  }

  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    let (line, column) = self.loc(span.start);
    Residue { file: self.file.clone(), line, column, message: message.into(), hint: None, via: Vec::new() }
  }

  /// The one-based line and column `pos` names, in the file the author wrote.
  pub(crate) fn loc(&self, pos: u32) -> (usize, usize) {
    if let Some(snippet) = self.snippets.borrow().iter().rev().find(|s| s.base <= pos) {
      return line_column(&snippet.padded, &line_starts(&snippet.padded), (pos - snippet.base) as usize);
    }
    if let Some(origin) = &self.origin {
      return origin.position(pos as usize);
    }
    line_column(self.text(), &self.line_starts, pos as usize)
  }

  /// The byte offset of a 1-based line and column as [`Parsed::position`] counts them.
  pub(crate) fn offset(&self, line: usize, column: usize) -> Option<usize> {
    let mut at = 0;
    for (i, text) in self.text().split_inclusive('\n').enumerate() {
      if i + 1 == line {
        return Some(at + text.char_indices().nth(column.saturating_sub(1)).map(|(byte, _)| byte).unwrap_or(0));
      }
      at += text.len();
    }
    None
  }

  /// The one-based line and column of a byte offset in the file's text.
  pub(crate) fn position(&self, offset: usize) -> (usize, usize) {
    self.loc(offset as u32)
  }

  pub(crate) fn exports(&self) -> impl Iterator<Item = (&str, Exported<'_>)> {
    let mut out: Vec<(&str, Exported<'_>)> = Vec::new();
    for stmt in &self.module().body {
      match stmt {
        js::Statement::ExportDeclaration(export) => self.declared(&export.declaration, &mut out),
        js::Statement::ExportNamedDeclaration(named) if !named.export_kind.is_type() => {
          for spec in &named.specifiers {
            if spec.export_kind.is_type() {
              continue;
            }
            let orig = match &spec.local {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            let name = match &spec.exported {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            match self.local(orig) {
              Some(exported) => out.push((name, exported)),
              None => out.push((name, Exported::Unreadable(spec.span, "exported from a name this module does not declare"))),
            }
          }
        }
        js::Statement::ExportFromDeclaration(from) if !from.export_kind.is_type() => {
          for spec in &from.specifiers {
            if spec.export_kind.is_type() {
              continue;
            }
            let name = match &spec.exported {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            if matches!(spec.local, js::ModuleExportName::StringLiteral(_)) {
              continue;
            }
            out.push((name, Exported::Unreadable(spec.span, "exported from a name this module does not declare")));
          }
        }
        _ => {}
      }
    }
    out.into_iter()
  }

  /// Every name a declaration exports, in source order.
  fn declared<'p>(&'p self, decl: &'p js::Declaration<'p>, out: &mut Vec<(&'p str, Exported<'p>)>) {
    match decl {
      js::Declaration::FunctionDeclaration(f) => {
        let Some(id) = &f.id else { return };
        out.push((id.name.as_str(), Exported::Function(first_param(&f.params), body_of(f))));
      }
      js::Declaration::VariableDeclaration(var) => {
        for d in &var.declarations {
          let js::BindingPattern::BindingIdentifier(name) = &d.id else {
            bound_names(&d.id, &mut |name, span| out.push((name, Exported::Unreadable(span, "bound by a destructuring the build does not read"))));
            continue;
          };
          let Some(init) = &d.init else { continue };
          out.push((name.name.as_str(), self.classify(init)));
        }
      }
      _ => {}
    }
  }

  /// What a module-level declaration of `name` is, for an `export { name }`
  /// that stands apart from it.
  fn local(&self, name: &str) -> Option<Exported<'_>> {
    let mut found = Vec::new();
    for stmt in &self.module().body {
      let decl = match stmt {
        js::Statement::ExportDeclaration(export) => &export.declaration,
        other => match other.as_declaration() {
          Some(decl) => decl,
          None => continue,
        },
      };
      self.declared(decl, &mut found);
    }
    found.into_iter().find(|(n, _)| *n == name).map(|(_, e)| e)
  }
}

impl Parsed<'_> {
  /// Whether `callee` names `want` from the server tier, however the module
  /// reaches it; with no meanings resolved, the bare name nothing binds.
  pub(crate) fn names_server(&self, callee: &js::Expression, want: &str) -> bool {
    match self.meanings.get() {
      Some(meanings) => meanings.names_server(callee, want),
      None => matches!(callee, js::Expression::Identifier(id) if self.ctxt(id) == Ctxt::Unresolved && id.name.as_str() == want),
    }
  }

  fn classify<'a>(&'a self, init: &'a js::Expression<'a>) -> Exported<'a> {
    classify_in(self, init)
  }
}

fn classify_in<'a>(parsed: &'a Parsed<'a>, init: &'a js::Expression<'a>) -> Exported<'a> {
  match init {
    js::Expression::ParenthesizedExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::TSAsExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::TSSatisfiesExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::FunctionExpression(f) => Exported::Function(first_param(&f.params), body_of(f)),
    js::Expression::ArrowFunctionExpression(arrow) => match arrow_body(arrow) {
      FunctionBody::Block(stmts) => Exported::Function(first_param(&arrow.params), stmts),
      FunctionBody::Expr(e) => Exported::Expr(first_param(&arrow.params), e),
    },
    js::Expression::CallExpression(call) => {
      if !parsed.names_server(&call.callee, "action") {
        return Exported::Unreadable(call.span, "a call the build does not recognise");
      }
      match call.arguments.last() {
        Some(js::Argument::ArrowFunctionExpression(arrow)) => {
          let first = first_param(&arrow.params);
          Exported::Action { input: action_input(call, first), first, body: arrow_body(arrow) }
        }
        Some(js::Argument::FunctionExpression(f)) => {
          let first = first_param(&f.params);
          Exported::Action { input: action_input(call, first), first, body: FunctionBody::Block(body_of(f)) }
        }
        Some(other) => Exported::BadAction(other.span()),
        None => Exported::BadAction(call.span),
      }
    }
    js::Expression::ObjectExpression(_) => Exported::Value(init),
    other => Exported::Other(other.span()),
  }
}

/// The input type of an `action(...)`: the call's type argument, else what
/// the function's first parameter names through `ActionCtx<T>`.
fn action_input(call: &js::CallExpression, first: Option<Param<'_>>) -> Option<String> {
  call.type_arguments.as_ref().and_then(|t| t.params.first()).and_then(type_ref_name).or_else(|| first.and_then(action_ctx_input))
}

/// Binds the context parameter and lowers a function body: a block as it
/// stands, an expression as the value returned.
fn lower_function<'p>(lowerer: &mut Lowerer<'p>, first: Option<Param<'p>>, body: FunctionBody<'p>) -> Lowered<Body> {
  lowerer.bind_ctx(first)?;
  match body {
    FunctionBody::Block(stmts) => lowerer.block(stmts),
    FunctionBody::Expr(expr) => lowerer.expr(expr).map(|e| vec![Stmt::Return(e)]),
  }
}

fn is_bitwise(op: js::BinaryOperator) -> bool {
  matches!(op, js::BinaryOperator::BitwiseAnd | js::BinaryOperator::BitwiseOR | js::BinaryOperator::BitwiseXOR | js::BinaryOperator::ShiftLeft | js::BinaryOperator::ShiftRight | js::BinaryOperator::ShiftRightZeroFill)
}

/// The name a type reference writes, `T` for `T` or `T<U>`.
fn type_ref_name(ty: &js::TSType) -> Option<String> {
  match ty {
    js::TSType::TSTypeReference(r) => match &r.type_name {
      js::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// The input type an action's parameter names: `ActionCtx<AddToCart>` on
/// `ctx` or on the destructuring of it.
fn action_ctx_input(param: Param<'_>) -> Option<String> {
  if param.rest || param.default.is_some() || !matches!(param.pattern, js::BindingPattern::BindingIdentifier(_) | js::BindingPattern::ObjectPattern(_)) {
    return None;
  }
  let ann = param.type_annotation?;
  let js::TSType::TSTypeReference(r) = &ann.type_annotation else { return None };
  match &r.type_name {
    js::TSTypeName::IdentifierReference(id) if id.name.as_str() == "ActionCtx" => {}
    _ => return None,
  }
  r.type_arguments.as_ref().and_then(|p| p.params.first()).and_then(type_ref_name)
}

#[derive(Clone)]
#[derive(PartialEq, Eq, Copy)]
enum Root {
  Params,
  Query,
  Session,
  Services,
  Native,
  Identity,
  Locale,
  Locales,
  Path,
  Host,
  Origin,
  Address,
  Config,
  Input,
  Now,
  Ctx,
  /// Middleware's `next`.
  Next,
}

pub(crate) struct Lowerer<'a> {
  parsed: &'a Parsed<'a>,
  defaults: &'a SessionDefaults,
  roots: Vec<(String, Root)>,
  /// A middleware body reads the request line as `request`, which is its input.
  middleware: bool,
  /// Lowering `theme.ts`, whose `ctx.theme` is the body's input.
  theming: bool,
  /// A meta body reads its loader's data as `data`, which is its input.
  meta: bool,
  /// An action, a route handler or middleware may call `session.extend`; a
  /// loader runs on every navigation, so it may not.
  extends: bool,
  /// An action may `redirect`; anything else answers for itself.
  redirects: bool,
  pub(crate) scope: Vec<(String, Expr)>,
  /// Markup a component lowered ahead of the expression that holds it, by
  /// that expression's span; `expr` returns it rather than refusing JSX.
  pub(crate) premade: std::collections::HashMap<(u32, u32), Expr>,
  /// Module-level names a component lowerer has resolved, read after the scope.
  pub(crate) globals: Vec<(String, Expr)>,
  /// The resolved module-level names that hold a `new Map(...)`, `new Set(...)` or `new URLSearchParams(...)`.
  pub(crate) global_kinds: Vec<(String, Kind)>,
  /// The last name `ident` could not resolve, so a caller that can may bind it and retry.
  pub(crate) unbound: Option<String>,
  /// Set by a component lowerer: every helper call and formatting builtin is
  /// wrapped as a hoist candidate and its source span kept for the rewrite.
  pub(crate) hoisting: Option<hoist::Candidates>,
  /// The native pairs declared so far, `module.member` and reach.
  pub(crate) natives: Vec<(String, Reach)>,
  /// Lowering an event handler, which runs once wherever it runs, so a
  /// `body` extension is allowed there.
  pub(crate) in_handler: bool,
  /// Lowering a render path with no hoist candidates kept, a Vue template
  /// for one: the browser runs it too, so a `body` extension is refused.
  pub(crate) render_path: bool,
  /// The last residue was a `body` extension on a render path, which the
  /// set reports as `LowerError::Reach` rather than a client downgrade.
  pub(crate) reach_violation: bool,
  /// How many temporaries a destructuring has bound, so each gets a name of its own.
  pub(crate) temps: usize,
  /// Names bound to a value of a known [`Kind`], with the binding they were noted against, so `.size`, `.get` and `.has` read the collection only where the name still holds one.
  pub(crate) kinds: Vec<(String, Expr, Kind)>,
  /// Names a `const` binds to an object literal, with the binding they were noted against and the fields written as functions, so `name.field(...)` applies the field where the name still holds that object.
  pub(crate) methods: Vec<(String, Expr, Vec<String>)>,
  /// The kinds of a component's props, from the props parameter's type or from the placements that pass them, by prop name.
  pub(crate) prop_kinds: Vec<(String, Kind)>,
  /// The props whose kind the lowering read, so a placement passing another kind for one is a conflict rather than a silent wrong constant.
  pub(crate) consulted: std::cell::RefCell<Vec<String>>,
  /// Names bound to an `Intl` formatter kept as a value, with the binding noted against and the `new` that made it, so `f.format(x)` lowers as the inline form would.
  pub(crate) formatters: Vec<(String, Expr, &'a js::NewExpression<'a>)>,
  /// The resolved module-level names that hold an `Intl` formatter.
  pub(crate) global_formatters: Vec<(String, &'a js::NewExpression<'a>)>,
  /// Why the last residue was a value only the browser knows, set by the site that refused it. A consumer that can leave the value to the browser, text under an element, a prop into a placement, a binding, takes it and lowers null in its place; anywhere else the residue stands.
  pub(crate) browser_value: Option<String>,
  /// The reaches this page may use besides `Render` and `Body`: [`Reach::Terminal`], [`Reach::Later`] and [`Reach::Process`] for a terminal page, none for a web page.
  pub(crate) reaches: Vec<Reach>,
  /// The names the body being lowered declares, each with the name it binds, which JavaScript binds from the start of the body: what runs after the body, a handler or an effect, reads one declared below it.
  pub(crate) declared_below: Vec<(String, String)>,
  /// While lowering what runs after the body: a handler, an effect or a continuation.
  pub(crate) after_body: bool,
  /// Each name read through [`Self::declared_below`], the name it binds, and where.
  pub(crate) read_below: Vec<(String, Span)>,
  /// What the module-level names of a component's file mean, set by the component lowerer.
  pub(crate) meanings: Option<std::rc::Rc<meaning::Meanings>>,
  /// The parameter a `derive` compute reads its sources through: a call of it is a store read.
  pub(crate) reader: Option<oxc_syntax::symbol::SymbolId>,
  /// A component's setters, dispatchers and inner functions, which only the browser calls. Read as a value on the server one is `render::BROWSER_HANDLER`, a function that renders nothing, as React renders nothing for one.
  pub(crate) browser_functions: Vec<String>,
  /// The functions a component's body declares, by name; a call to one in an expression applies it as a lambda made where the call stands.
  pub(crate) local_fns: Vec<(String, &'a js::Function<'a>)>,
  /// The application's functions whose every statement the browser alone runs, so a call to one as a statement does nothing on the server.
  pub(crate) browser_effects: Vec<String>,
  /// Names held in a cell, `Expr::Cell` or `Expr::ModuleCell`, because a function writes them, each with the expression for its handle: a write in a block is a `WriteCell` through it.
  pub(crate) cells: Vec<(String, Expr)>,
  /// One frame per open fold step or function path: the lets a declaration whose value reads the clock or randomness is bound by, so it is read once where it is declared.
  pub(crate) held: Vec<Vec<Stmt>>,
  /// For each frame in `held`, where its conditions start in `rebind_reach`.
  pub(crate) held_reach: Vec<usize>,
  pub(crate) shared: Vec<(String, crate::Ctxt)>,
  pub(crate) extra_lets: Vec<Stmt>,
  /// The conditions of the branches a rebinding is inside, outermost first: a `let` held for a statement inside them runs only where they all hold.
  pub(crate) rebind_reach: Vec<Expr>,
  /// The names a terminal promise's executor binds its `resolve` and `reject` to, while it lowers.
  pub(crate) settlers: Vec<String>,
  /// The continuations a terminal page's async functions lowered to, each the rest of a function after an `await`, which the component that runs it holds as handlers.
  pub(crate) continuations: Vec<snapfire_fsr_ir::ast::Handler>,
  /// The locals a terminal block bound to a process it spawned.
  pub(crate) spawned: Vec<String>,
  /// Where the scope of the async function lowering starts: its locals, declared before an `await` and carried to the continuation after it, are its own to write.
  pub(crate) async_floor: Option<usize>,
  /// While a pass of a loop that awaits lowers: the loop head's event and the carry of the live locals it is run with, which the pass issues when it ends; also the function's promise a `return` in the pass settles.
  pub(crate) loop_tail: Option<LoopTail>,
  /// The loops and `switch`es around the statement being lowered, innermost last, which a `break` or `continue` leaves. A function starts with none.
  pub(crate) breakable: Vec<Breakable>,
  /// Set while render statements inside a `try` lower: a `return` there is `[value]` and a path that ends without one is `null`, so what follows the `try` can tell the two apart.
  pub(crate) wrapped_returns: bool,
  /// Set while a component's `try` that returns markup runs as statements: where its scope starts. A `return` there ends the run with which `return` it was as `$path` and each name in scope from there as `$0`, `$1` and on. `markup_returns` records each by its position in the source with the names.
  pub(crate) markup_base: Option<usize>,
  pub(crate) markup_returns: Vec<(u32, Vec<String>)>,
  /// The contexts this component provides, by key, for a `getContext` of its own to read.
  pub(crate) provided: Vec<(String, Expr)>,
  /// Imports of the application's own the build followed and could not lower; a handler that reaches one is the browser's.
  pub(crate) unfollowed: Vec<String>,
  /// The classes the set has read, whose members a call site dispatches to.
  pub(crate) classes: Option<std::rc::Rc<std::cell::RefCell<crate::classes::ClassTable>>>,
  /// The class the member being lowered extends, which `super.m()` calls into.
  pub(crate) class_parent: Option<String>,
  /// The kinds the fields of the class whose member is being lowered are declared with, read as `this.field`.
  pub(crate) this_kinds: Vec<(String, Kind)>,
  /// The receivers a call is being lowered without class dispatch for, the call already dispatching on them.
  pub(crate) dispatching: Vec<String>,
  /// The type aliases the file declares or imports that stand for a kind.
  pub(crate) type_kinds: Vec<(String, AliasKind)>,
}

/// A JavaScript type the value model has no arm for and what it lowered to: a `Map` as an object keyed by `String(key)`, a `Set` as an array without repeats, a `URLSearchParams` as the encoded string, a `URL` as its href, a `Date` as milliseconds since the epoch and a `BigInt` as an integer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
  Map,
  Set,
  Query,
  Url,
  Date,
  BigInt,
  /// A `Response`: what `new Response`, `Response.json`, `Response.redirect` and middleware's `next` make.
  Response,
  /// A value only the browser knows, the viewer's clock for one: null on the server, read as an error the sites that can leave it to the browser catch.
  Browser,
  /// A `Scope` from the standard module: a module cell `run` sets around a call and `get` reads.
  Scope,
}

impl Kind {
  pub(crate) fn name(self) -> &'static str {
    match self {
      Kind::Map => "Map",
      Kind::Set => "Set",
      Kind::Query => "URLSearchParams",
      Kind::Url => "URL",
      Kind::Date => "Date",
      Kind::BigInt => "BigInt",
      Kind::Response => "Response",
      Kind::Browser => "value only the browser knows",
      Kind::Scope => "Scope",
    }
  }

  /// `built` marked as the collection it makes, for a `Map` or a `Set`, so it converts and serializes as one wherever it goes.
  pub(crate) fn made(self, built: Expr) -> Expr {
    match self {
      Kind::Map => Expr::Builtin { name: Builtin::AsMap, args: vec![built] },
      Kind::Set => Expr::Builtin { name: Builtin::AsSet, args: vec![built] },
      _ => built,
    }
  }

  /// The variant tag the value travels under into an island, so the browser makes the real type; a `URLSearchParams` is a string on both sides.
  pub(crate) fn tag(self) -> Option<&'static str> {
    use snapfire_fsr_ir::render::typed;
    match self {
      Kind::Map => Some(typed::MAP),
      Kind::Set => Some(typed::SET),
      Kind::Query | Kind::Browser | Kind::Response | Kind::Scope => None,
      Kind::Url => Some(typed::URL),
      Kind::Date => Some(typed::DATE),
      Kind::BigInt => Some(typed::BIGINT),
    }
  }

  /// The kind a type annotation names: `Map<K, V>`, `Set<T>`, `Date`, `URLSearchParams`, `URL` or `bigint`.
  pub(crate) fn of_type(ty: &js::TSType) -> Option<Kind> {
    Kind::of_type_in(ty, &[])
  }

  /// The kind `ty` names, an alias in `aliases` read through to what it stands for, so `Unique<URL>` for `type Unique<T> = T & Brand` is a `URL`.
  pub(crate) fn of_type_in(ty: &js::TSType, aliases: &[(String, AliasKind)]) -> Option<Kind> {
    match ty {
      js::TSType::TSParenthesizedType(p) => Kind::of_type_in(&p.type_annotation, aliases),
      js::TSType::TSIntersectionType(i) => i.types.iter().find_map(|t| Kind::of_type_in(t, aliases)),
      js::TSType::TSBigIntKeyword(_) => Some(Kind::BigInt),
      js::TSType::TSTypeReference(r) => match &r.type_name {
        js::TSTypeName::IdentifierReference(id) => match id.name.as_str() {
          "Map" | "ReadonlyMap" => Some(Kind::Map),
          "Set" | "ReadonlySet" => Some(Kind::Set),
          "Date" => Some(Kind::Date),
          "URLSearchParams" => Some(Kind::Query),
          "URL" => Some(Kind::Url),
          name => match aliases.iter().find(|(n, _)| n == name).map(|(_, kind)| *kind)? {
            AliasKind::Fixed(kind) => Some(kind),
            AliasKind::Param(i) => Kind::of_type_in(r.type_arguments.as_ref()?.params.get(i)?, aliases),
          },
        },
        _ => None,
      },
      _ => None,
    }
  }
}

/// What a type alias stands for as a kind: one kind whatever it is given, or the kind of its type argument at a position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AliasKind {
  Fixed(Kind),
  Param(usize),
}

/// The name the kind a module-level function returns is noted under beside the kinds of module-level values.
pub(crate) fn returned_by(name: &str) -> String {
  format!("{name}()")
}

/// The parts of a `URL` read through `UrlPart` and written through `UrlSet`.
pub(crate) const URL_PARTS: [&str; 10] = ["origin", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];

pub(crate) type Lowered<T> = Result<T, Residue>;

impl<'a> Lowerer<'a> {
  pub(crate) fn new(parsed: &'a Parsed<'a>, defaults: &'a SessionDefaults) -> Self {
    Self { shared: Vec::new(), extra_lets: Vec::new(), parsed, defaults, roots: Vec::new(), scope: Vec::new(), premade: std::collections::HashMap::new(), globals: Vec::new(), global_kinds: Vec::new(), unbound: None, middleware: false, theming: false, meta: false, extends: false, redirects: false, hoisting: None, natives: Vec::new(), in_handler: false, render_path: false, reach_violation: false, temps: 0, kinds: Vec::new(), methods: Vec::new(), prop_kinds: Vec::new(), consulted: std::cell::RefCell::new(Vec::new()), formatters: Vec::new(), global_formatters: Vec::new(), browser_value: None, reaches: Vec::new(), declared_below: Vec::new(), after_body: false, read_below: Vec::new(), meanings: parsed.meanings.get().cloned(), reader: None, browser_functions: Vec::new(), local_fns: Vec::new(), unfollowed: Vec::new(), browser_effects: Vec::new(), provided: Vec::new(), cells: Vec::new(), held: Vec::new(), held_reach: Vec::new(), rebind_reach: Vec::new(), settlers: Vec::new(), continuations: Vec::new(), spawned: Vec::new(), async_floor: None, wrapped_returns: false, loop_tail: None, breakable: Vec::new(), markup_base: None, markup_returns: Vec::new(), classes: None, class_parent: None, this_kinds: Vec::new(), dispatching: Vec::new(), type_kinds: Vec::new() }
  }

  pub(crate) fn resolved(mut self, resolved: &Resolved) -> Self {
    self.globals = resolved.globals.clone();
    self.global_kinds = resolved.global_kinds.clone();
    self.global_formatters.clear();
    self.formatters_from(&resolved.global_formatters);
    self.natives = resolved.natives.clone();
    self.classes = resolved.classes.clone();
    self.type_kinds = resolved.type_kinds.clone();
    self.reaches = resolved.reaches.clone();
    self
  }

  /// True while lowering a component's render path: the browser runs it too.
  fn on_render_path(&self) -> bool {
    (self.render_path || self.hoisting.is_some()) && !self.in_handler
  }

  /// Whether this page may use `reach`. Every page may use `Render` and `Body`.
  pub(crate) fn reaches(&self, reach: Reach) -> bool {
    reach.needs().is_none() || self.reaches.contains(&reach)
  }

  /// Refuses a `body` extension on a web page's render path and any reach the page may not use.
  /// A call to a standard member imported from a side, as the member's extension call. A member the side lacks and a host member the browser would run are refused with where the member is.
  fn std_call(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    use meaning::{Meaning, Side, CLIENT_ROOT, HOST_ROOT};
    let Some(meant) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) else { return Ok(None) };
    if let Meaning::StdAbsent { side, module, name } = &meant {
      self.reach_violation = true;
      let (why, hint) = meaning::absent(*side, module, name);
      return Err(match hint {
        Some(hint) => self.residue_with(call.span, why, hint),
        None => self.residue(call.span, why),
      });
    }
    if let Meaning::Library(meaning::Construct::StdModule(_, module)) = &meant {
      return Err(self.residue(callee.span(), format!("`{module}` from the standard library is a module; call one of its members")));
    }
    let Some((side, module, name)) = meant.std_member() else { return Ok(None) };
    let key = format!("{module}.{name}");
    let Some(reach) = standard_reach(module, name) else { return Ok(None) };
    if side == Side::Host && self.on_render_path() && !self.reaches(Reach::Terminal) {
      self.reach_violation = true;
      let why = match Side::Client.holds(module, reach) {
        true => format!("`{key}` from `{HOST_ROOT}` in code the browser also runs; import it from `{CLIENT_ROOT}`"),
        false => format!("`{key}` from `{HOST_ROOT}` in code the browser also runs: it runs only in a host. Read it in the route's loader `page.loader.ts` and pass it to the page as a prop"),
      };
      return Err(self.residue(call.span, why));
    }
    self.check_reach(call.span, &key, reach)?;
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      if crate::syntax::is_spread(a) {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("a spread into `{key}`")));
      }
      args.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    let expr = Expr::Ext { module: module.to_owned(), name: name.to_owned(), args };
    Ok(Some(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr }))
  }

  fn check_reach(&mut self, span: Span, key: &str, reach: Reach) -> Lowered<()> {
    if reach == Reach::Body && self.on_render_path() && !self.reaches(Reach::Terminal) {
      self.reach_violation = true;
      return Err(self.residue(span, format!("`{key}` on a render path")));
    }
    if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
      return Err(self.residue(span, format!("`{key}`, which {needs}")));
    }
    Ok(())
  }

  /// The first `body` extension a lowered expression calls, by name.
  pub(crate) fn body_extension_in(&self, expr: &Expr) -> Option<String> {
    let mut found = None;
    expr.visit(&mut |e| {
      if found.is_some() {
        return;
      }
      if let Expr::Ext { module, name, .. } = e {
        let key = format!("{module}.{name}");
        let reach = standard_reach(module, name).or_else(|| self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r));
        if reach == Some(Reach::Body) {
          found = Some(key);
        }
      }
    });
    found
  }

  /// Whether the only server-only call in `expr` is the clock `Date.now()` and `new Date()` lower to, a `time.now` read as a number, rather than `time.now` or another body extension the application calls itself.
  fn reads_only_the_date_clock(&self, expr: &Expr) -> bool {
    let is_clock = |e: &Expr| matches!(e, Expr::Ext { module, name, args } if module == "time" && name == "now" && args.is_empty());
    let (mut clocks, mut wrapped, mut other) = (0usize, 0usize, false);
    expr.visit(&mut |e| match e {
      Expr::Num(inner) if is_clock(inner) => wrapped += 1,
      e if is_clock(e) => clocks += 1,
      Expr::Ext { module, name, .. } => {
        let key = format!("{module}.{name}");
        other |= standard_reach(module, name).or_else(|| self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r)) == Some(Reach::Body);
      }
      _ => {}
    });
    !other && clocks > 0 && clocks == wrapped
  }

  /// `expr` as a hoist candidate when a component is being lowered, else itself.
  pub(crate) fn candidate(&mut self, span: Span, expr: Expr) -> Expr {
    match &mut self.hoisting {
      Some(candidates) => {
        let range = self.parsed.range(span);
        candidates.add(range, expr)
      }
      None => expr,
    }
  }

  /// `session.key`, with the schema's default folded in when there is one.
  fn session_read(&self, key: String) -> Expr {
    match self.defaults.iter().find(|(k, _)| *k == key) {
      Some((_, default)) => Expr::Coalesce(Box::new(Expr::Session(key)), Box::new(default.clone())),
      None => Expr::Session(key),
    }
  }

  pub(crate) fn literal(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    self.expr(expr)
  }

  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    self.parsed.residue(span, message)
  }

  /// A residue that names the rewrite doing the same thing in the IR.
  pub(crate) fn residue_with(&self, span: Span, message: impl Into<String>, hint: impl Into<String>) -> Residue {
    Residue { hint: Some(hint.into()), ..self.parsed.residue(span, message) }
  }

  /// The body's first parameter: `ctx` or a destructuring of it.
  fn bind_ctx(&mut self, first: Option<Param>) -> Lowered<()> {
    let Some(first) = first else { return Ok(()) };
    if first.rest || first.default.is_some() {
      return Err(self.residue(first.span, "the context parameter must be `ctx` or a destructuring"));
    }
    match first.pattern {
      js::BindingPattern::BindingIdentifier(id) => {
        self.roots.push((id.name.to_string(), Root::Ctx));
        Ok(())
      }
      js::BindingPattern::ObjectPattern(obj) => {
        for prop in crate::syntax::pat_props(obj) {
          match prop {
            crate::syntax::PatProp::Assign(a) => {
              let name = a.key.name.to_string();
              let root = self.root_named(&name).ok_or_else(|| self.residue(a.span, format!("`{name}` is not a field of the context")))?;
              self.roots.push((name, root));
            }
            crate::syntax::PatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed context field"))?;
              let js::BindingPattern::BindingIdentifier(local) = &kv.value else {
                return Err(self.residue(kv.value.span(), "a nested destructuring of the context"));
              };
              let root = self.root_named(&key).ok_or_else(|| self.residue(kv.key.span(), format!("`{key}` is not a field of the context")))?;
              self.roots.push((local.name.to_string(), root));
            }
            crate::syntax::PatProp::Rest(r) => return Err(self.residue(r.span, "a rest of the context")),
          }
        }
        Ok(())
      }
      other => Err(self.residue(other.span(), "the context parameter must be `ctx` or a destructuring")),
    }
  }

  /// Whether `init` is the context parameter itself, so a declaration of it binds as the parameter's own destructuring does.
  fn is_ctx(&self, init: Option<&js::Expression>) -> bool {
    matches!(init, Some(js::Expression::Identifier(id)) if matches!(self.root_of(id), Some(Root::Ctx)))
  }

  /// `params`, `query`, `session` or `config`, by name or as a field of the context: a root read only by key.
  fn keyed_root(&self, init: &js::Expression) -> Option<Root> {
    let keyed = |root: Root| matches!(root, Root::Params | Root::Query | Root::Session | Root::Config).then_some(root);
    match init {
      js::Expression::ParenthesizedExpression(p) => self.keyed_root(&p.expression),
      js::Expression::Identifier(id) if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) => self.root_of(id).and_then(keyed),
      m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
        let (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(field)) = (m.object(), crate::syntax::prop(m)) else { return None };
        if self.scope.iter().any(|(n, _)| n == obj.name.as_str()) || !matches!(self.root_of(obj), Some(Root::Ctx)) {
          return None;
        }
        root_named(field.name.as_str()).and_then(keyed)
      }
      _ => None,
    }
  }

  /// `const { id, slug: s = "a" } = params`: each name bound to its key's read.
  fn bind_keyed(&mut self, fields: &js::ObjectPattern, root: Root) -> Lowered<()> {
    let read = |lowerer: &Self, key: String| match root {
      Root::Params => Expr::Param(key),
      Root::Query => Expr::Query(key),
      Root::Session => lowerer.session_read(key),
      _ => Expr::Config(key),
    };
    for field in crate::syntax::pat_props(fields) {
      match field {
        crate::syntax::PatProp::Assign(a) => {
          let name = a.key.name.to_string();
          let bound = match a.value {
            Some(default) => Expr::Coalesce(Box::new(read(self, name.clone())), Box::new(self.expr(default)?)),
            None => read(self, name.clone()),
          };
          self.scope.push((name, bound));
        }
        crate::syntax::PatProp::KeyValue(kv) => {
          let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed key in a pattern"))?;
          let value = read(self, key);
          crate::component::bind_pattern(self, &kv.value, value)?;
        }
        crate::syntax::PatProp::Rest(r) => return Err(self.residue(r.span, "a rest of a value read only by key")),
      }
    }
    Ok(())
  }

  fn root_named(&self, name: &str) -> Option<Root> {
    if self.middleware && name == "request" {
      return Some(Root::Input);
    }
    if self.middleware && name == "next" {
      return Some(Root::Next);
    }
    if self.theming && name == "theme" {
      return Some(Root::Input);
    }
    if self.meta && name == "data" {
      return Some(Root::Input);
    }
    root_named(name)
  }

  pub(crate) fn block(&mut self, stmts: &[js::Statement]) -> Lowered<Body> {
    let depth = self.scope.len();
    let _ = self.shared.len();
    let shared = crate::component::closure_shared(&self.parsed.scope, stmts);
    self.shared.extend(shared);
    let mut out = Vec::with_capacity(stmts.len());
    if let Err(residue) = self.hoist_functions(stmts, &mut out) {
      self.scope.truncate(depth);
      return Err(residue);
    }
    for stmt in stmts {
      self.lift_updates(stmt, &mut out)?;
      match stmt {
        js::Statement::FunctionDeclaration(_) => {}
        js::Statement::ForStatement(counted) => out.extend(self.counted_for(counted)?),
        js::Statement::WhileStatement(looped) if self.reaches(Reach::Terminal) => out.push(self.capped_while(looped)?),
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            if self.is_ctx(decl.init.as_ref()) {
              self.bind_ctx(Some(Param::of_pattern(&decl.id)))?;
              continue;
            }
            if let (js::BindingPattern::ObjectPattern(fields), Some(root)) = (&decl.id, decl.init.as_ref().and_then(|init| self.keyed_root(init))) {
              self.bind_keyed(fields, root)?;
              continue;
            }
            out.push(self.declare(decl)?);
            out.append(&mut self.extra_lets);
            if var.kind == js::VariableDeclarationKind::Const {
              self.note_methods(decl);
            }
          }
        }
        js::Statement::SwitchStatement(sw) => out.extend(self.switch(sw)?),
        stmt => out.push(self.stmt(stmt)?),
      }
    }
    self.scope.truncate(depth);
    Ok(out)
  }

  /// The functions `stmts` declare, each bound before the first statement as JavaScript hoists them: a `Fix` when it calls itself.
  fn hoist_functions(&mut self, stmts: &[js::Statement], out: &mut Vec<Stmt>) -> Lowered<()> {
    let declared: Vec<&js::Function> = stmts.iter().filter_map(|stmt| match stmt {
      js::Statement::FunctionDeclaration(f) => Some(&**f),
      _ => None,
    }).collect();
    let name_of = |f: &js::Function| f.id.as_ref().map_or(String::new(), |id| id.name.to_string());
    for f in &declared {
      let name = name_of(f);
      self.scope.push((name.clone(), Expr::Var(name)));
    }
    for (at, f) in declared.iter().enumerate() {
      if f.generator {
        return Err(self.residue(f.span, "a generator function"));
      }
      let name = name_of(f);
      let pats = crate::params_of(&f.params);
      let body = f.body.as_ref().map(|body| body.statements.as_slice()).unwrap_or(&[]);
      let outer = std::mem::take(&mut self.breakable);
      let lambda = crate::component::function_to_lambda(self, &pats, crate::component::FunctionBody::Block(body), f.r#async);
      self.breakable = outer;
      let lambda = lambda?;
      let mut free = Vec::new();
      lambda.free_vars(&mut free);
      if let Some(later) = declared[at + 1..].iter().find(|later| free.iter().any(|name| *name == name_of(later))) {
        return Err(self.residue_with(f.span, format!("`{name}` calls `{}`, a function declared after it in the same body", name_of(later)), "declare the function it calls first"));
      }
      let expr = if free.contains(&name) { Expr::Fix { name: name.clone(), f: Box::new(lambda) } } else { lambda };
      out.push(Stmt::Let { name, expr });
    }
    Ok(())
  }

  /// `for (let i = a; i < b; i += s)` as its start under a temporary and a `for...of` over the count, the counter read as the start plus the item times the step. A bound that may change between passes, one that calls something, is tested every pass as JavaScript tests it.
  fn counted_for(&mut self, f: &js::ForStatement) -> Lowered<Vec<Stmt>> {
    let counted = crate::component::Counted::of(self, f)?;
    let (from, to, inclusive) = counted.bounds();
    let start = self.temp();
    let item = self.temp();
    let bound = to;
    let from = self.expr(from)?;
    let to = self.expr(bound)?;
    if !crate::component::plain(&to) {
      return self.tested_for(f, &counted, from, bound, inclusive);
    }
    let over = Expr::Builtin { name: Builtin::Range, args: vec![counted.count(self, Expr::Var(start.clone()), to)?] };
    let counter = counted.counter(self, Expr::Var(start.clone()), Expr::Var(item.clone()))?;
    let depth = self.scope.len();
    self.scope.push((counted.name.clone(), counter));
    self.breakable.push(Breakable::Loop);
    let body = self.branch(&f.body);
    self.breakable.pop();
    self.scope.truncate(depth);
    Ok(vec![Stmt::Let { name: start, expr: from }, Stmt::ForOf { name: item, over, body: body? }])
  }

  /// A counted loop whose bound is read again every pass: the counter a local, the test and the step after every pass as written.
  fn tested_for(&mut self, f: &js::ForStatement, counted: &crate::component::Counted, from: Expr, bound: &js::Expression, inclusive: bool) -> Lowered<Vec<Stmt>> {
    let counter = counted.name.as_str();
    let step = counted.step_expr(self)?;
    let depth = self.scope.len();
    self.scope.push((counter.to_owned(), Expr::Var(counter.to_owned())));
    let to = self.expr(bound);
    let body = to.and_then(|to| {
      let op = if inclusive { CompareOp::Le } else { CompareOp::Lt };
      let cond = Expr::Compare(op, Box::new(Expr::Var(counter.to_owned())), Box::new(to));
      self.breakable.push(Breakable::Loop);
      let body = self.branch(&f.body);
      self.breakable.pop();
      let step = Stmt::Set { name: counter.to_owned(), expr: Expr::Arith(ArithOp::Add, Box::new(Expr::Var(counter.to_owned())), Box::new(step)) };
      Ok(Stmt::While { cond, body: body?, step: vec![step] })
    });
    self.scope.truncate(depth);
    Ok(vec![Stmt::Let { name: counter.to_owned(), expr: from }, body?])
  }

  /// A terminal's `while (test) body`, the test read before every pass.
  fn capped_while(&mut self, looped: &js::WhileStatement) -> Lowered<Stmt> {
    let test = self.expr(&looped.test)?;
    self.breakable.push(Breakable::Loop);
    let body = self.branch(&looped.body);
    self.breakable.pop();
    Ok(Stmt::While { cond: test, body: body?, step: Vec::new() })
  }

  /// `switch` as its discriminant under a temporary and an `if` chain over the arms, each compared with `===`.
  fn switch(&mut self, sw: &js::SwitchStatement) -> Lowered<Vec<Stmt>> {
    self.breakable.push(Breakable::Switch);
    let lowered = self.switch_arms(sw);
    self.breakable.pop();
    lowered
  }

  fn switch_arms(&mut self, sw: &js::SwitchStatement) -> Lowered<Vec<Stmt>> {
    let disc = self.expr(&sw.discriminant)?;
    let held = self.temp();
    let (arms, default) = crate::component::switch_arms(self, sw)?;
    let mut otherwise = match default {
      Some(body) => self.block(body)?,
      None => Vec::new(),
    };
    for (tests, body) in arms.into_iter().rev() {
      let mut cond: Option<Expr> = None;
      for test in tests {
        let eq = Expr::Compare(CompareOp::Eq, Box::new(Expr::Var(held.clone())), Box::new(self.expr(test)?));
        cond = Some(match cond {
          None => eq,
          Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
        });
      }
      let then = self.block(body)?;
      otherwise = vec![Stmt::If { cond: cond.unwrap_or(Expr::Lit(Lit::Bool(false))), then, r#else: otherwise }];
    }
    let mut out = vec![Stmt::Let { name: held, expr: disc }];
    out.extend(otherwise);
    Ok(out)
  }

  /// A fresh name for a value a destructuring reads its fields from.
  pub(crate) fn temp(&mut self) -> String {
    self.temps += 1;
    format!("$d{}", self.temps)
  }

  /// The kind `e` holds: a `new Map(...)`, `new Set(...)`, `new Date(...)` or bigint written in place, a name noted as bound to one or a prop whose kind is known.
  pub(crate) fn kind_of(&self, e: &js::Expression) -> Option<Kind> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.kind_of(&p.expression),
      js::Expression::AwaitExpression(a) => self.kind_of(&a.argument),
      js::Expression::CallExpression(call) if self.calls_next(call) => Some(Kind::Response),
      js::Expression::TSAsExpression(a) => self.kind_of(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.kind_of(&a.expression),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        match self.scope.iter().rev().find(|(n, _)| n == name) {
          Some((_, bound)) => {
            let kind = self.kinds.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, kind)| *kind);
            if let (Some(_), Some(key)) = (kind, prop_key(bound)) {
              self.consulted.borrow_mut().push(key);
            }
            kind
          }
          None => self.global_kinds.iter().find(|(n, _)| n == name).map(|(_, kind)| *kind),
        }
      }
      js::Expression::CallExpression(call) => match &call.callee {
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Browser)) => Some(Kind::Browser),
        js::Expression::Identifier(f) if !self.scope.iter().any(|(n, _)| n == f.name.as_str()) => {
          let returned = returned_by(f.name.as_str());
          self.global_kinds.iter().find(|(n, _)| *n == returned).map(|(_, kind)| *kind).or_else(|| made_kind(e))
        }
        _ => made_kind(e),
      },
      member if member.is_member_expression() => self.member_kind(member.as_member_expression().expect("matched as a member")),
      other => made_kind(other),
    }
  }

  /// The kind a member read holds: a browser object's member, a URL's `searchParams`, a class field or a prop whose kind is known.
  pub(crate) fn member_kind(&self, m: &js::MemberExpression) -> Option<Kind> {
    if self.kind_of(m.object()) == Some(Kind::Browser) {
      return Some(Kind::Browser);
    }
    if self.kind_of(m.object()) == Some(Kind::Url) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "searchParams") {
      return Some(Kind::Query);
    }
    if let (js::Expression::Identifier(this), crate::syntax::MemberProp::Ident(field)) = (m.object(), crate::syntax::prop(m)) {
      if this.name.as_str() == crate::classes::THIS {
        return self.this_kinds.iter().find(|(n, _)| n == field.name.as_str()).map(|(_, kind)| *kind);
      }
    }
    let js::Expression::Identifier(obj) = m.object() else { return None };
    let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(m) else { return None };
    let (_, bound) = self.scope.iter().rev().find(|(n, _)| n == obj.name.as_str())?;
    if *bound != Expr::Var("$props".to_owned()) {
      return None;
    }
    let kind = self.prop_kinds.iter().find(|(k, _)| k == prop.name.as_str()).map(|(_, kind)| *kind)?;
    self.consulted.borrow_mut().push(prop.name.to_string());
    Some(kind)
  }

  /// `x instanceof Map`, `Set`, `Date` or `Array`: a constant where the build knows what `x` holds, `Array.isArray` for `Array`.
  fn instance_of(&mut self, bin: &js::BinaryExpression) -> Lowered<Expr> {
    if let Some(test) = self.class_instance_of(&bin.left, &bin.right)? {
      return Ok(test);
    }
    let js::Expression::Identifier(class) = &bin.right else { return Err(self.residue(bin.span, "`instanceof` of a computed class")) };
    let class = class.name.as_str();
    if self.scope.iter().any(|(n, _)| n == class) || self.globals.iter().any(|(n, _)| n == class) {
      return Err(self.residue(bin.span, format!("`instanceof {class}`, a class this file declares")));
    }
    if class == "Array" {
      return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(&bin.left)?] });
    }
    if ERROR_CONSTRUCTORS.contains(&class) {
      return Ok(Expr::Builtin { name: Builtin::IsError, args: vec![self.expr(&bin.left)?, Expr::lit_str(class)] });
    }
    if !matches!(class, "Map" | "Set" | "Date") {
      return Err(self.residue(bin.span, format!("`instanceof {class}`")));
    }
    match self.kind_of(&bin.left) {
      Some(Kind::Browser) => Err(self.browser_only(bin.left.span(), "`instanceof` of a value only the browser knows")),
      Some(kind) => Ok(Expr::Lit(Lit::Bool(kind.name() == class))),
      None => Err(self.residue_with(bin.left.span(), format!("`instanceof {class}` on a value whose type the build does not know"), "make the value in place with `new` or type the prop `Map<K, V>`, `Set<T>` or `Date` where the component declares its props")),
    }
  }

  /// The `Intl` formatter `e` holds: a `new Intl.NumberFormat(...)` written in place, or a name bound to one, local or module-level.
  pub(crate) fn formatter_of(&self, e: &js::Expression) -> Option<&'a js::NewExpression<'a>> {
    match strip_parens(e) {
      js::Expression::TSAsExpression(a) => self.formatter_of(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.formatter_of(&a.expression),
      js::Expression::NewExpression(n) if platform_object(n).is_some() => Some(self.keep(&**n)),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        match self.scope.iter().rev().find(|(n, _)| n == name) {
          Some((_, bound)) => self.formatters.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, made)| *made),
          None => self.global_formatters.iter().find(|(n, _)| n == name).map(|(_, made)| *made),
        }
      }
      _ => None,
    }
  }

  /// Builds nodes in the arena of the module being lowered.
  pub(crate) fn build(&self) -> syntax::Build<'a> {
    syntax::Build::new(self.parsed.arena())
  }

  /// A copy of `node` in the module's arena, which lives as long as the lowerer's view of the module.
  pub(crate) fn formatters_from(&mut self, formatters: &[(String, String)]) {
    for (name, text) in formatters {
      let Ok(js::Expression::NewExpression(made)) = self.parsed.parse_expr_at(text, 1, 1) else { continue };
      if platform_object(&made).is_some() {
        let made: &'a js::NewExpression<'a> = self.parsed.arena().alloc(made);
        self.global_formatters.push((name.clone(), made));
      }
    }
  }

  pub(crate) fn keep<T: oxc_allocator::CloneIn<'a>>(&self, node: &T) -> &'a T::Cloned {
    let b = self.build();
    b.alloc(b.copy(node))
  }

  /// Binds `name` to an `Intl` formatter `init` makes, so a later `.format(x)` on it lowers as the inline form; the name reads as null anywhere else.
  pub(crate) fn bind_formatter(&mut self, name: &str, init: &js::Expression) -> bool {
    let js::Expression::NewExpression(made) = strip_parens(init) else { return false };
    if platform_object(made).is_none() {
      return false;
    }
    self.scope.push((name.to_owned(), Expr::Lit(Lit::Null)));
    let made = self.keep(&**made);
    self.formatters.push((name.to_owned(), Expr::Lit(Lit::Null), made));
    true
  }

  /// Notes the fields of the object literal `decl` binds that are written as functions.
  fn note_methods(&mut self, decl: &js::VariableDeclarator) {
    let (js::BindingPattern::BindingIdentifier(name), Some(js::Expression::ObjectExpression(object))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) else { return };
    let fields: Vec<String> = object.properties.iter().filter_map(|prop| match prop {
      js::ObjectPropertyKind::ObjectProperty(prop) => match crate::syntax::prop_of(prop) {
        crate::syntax::Prop::KeyValue(kv) if matches!(crate::meaning::bare(&kv.value), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => prop_name(&kv.key, kv.computed),
        crate::syntax::Prop::Method(method) => prop_name(&method.key, method.computed),
        _ => None,
      },
      js::ObjectPropertyKind::SpreadProperty(_) => None,
    }).collect();
    let name = name.name.to_string();
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, e)| e.clone()) else { return };
    if !fields.is_empty() {
      self.methods.push((name, bound, fields));
    }
  }

  /// Whether `obj` is a name noted by [`Self::note_methods`] as holding an object whose field `field` is a function.
  fn holds_method(&self, obj: &js::Expression, field: &str) -> bool {
    let js::Expression::Identifier(id) = crate::meaning::bare(obj) else { return false };
    let name = id.name.as_str();
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e) else { return false };
    self.methods.iter().rev().any(|(n, at, fields)| n == name && at == bound && fields.iter().any(|f| f == field))
  }

  /// Notes `name`, just bound to `init`, as a collection when `init` is one.
  pub(crate) fn note_kind(&mut self, name: &str, init: &js::Expression) {
    let Some(kind) = self.kind_of(init) else { return };
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()) else { return };
    self.kinds.push((name.to_owned(), bound, kind));
  }

  /// One declarator as a `let`: a name binds the value; a pattern binds it under a temporary and each name it declares reads a part of that.
  fn declare(&mut self, decl: &js::VariableDeclarator) -> Lowered<Stmt> {
    let Some(init) = &decl.init else {
      return Err(self.residue(decl.span, "a declaration without a value"));
    };
    if let js::BindingPattern::BindingIdentifier(name) = &decl.id {
      if self.bind_formatter(name.name.as_str(), init) {
        return Ok(Stmt::Let { name: name.name.to_string(), expr: Expr::Lit(Lit::Null) });
      }
    }
    let expr = self.expr(init)?;
    let name = match &decl.id {
      js::BindingPattern::BindingIdentifier(name) if self.shared.iter().any(|(n, c)| *n == name.name.as_str() && *c == self.parsed.binding_ctxt(name)) => {
        let handle = self.temp();
        let local = name.name.to_string();
        self.cells.push((local.clone(), Expr::Var(handle.clone())));
        self.scope.push((local.clone(), Expr::ReadCell(Box::new(Expr::Var(handle.clone())))));
        self.note_kind(&local, init);
        return Ok(Stmt::Let { name: handle, expr: Expr::Cell(Box::new(expr)) });
      }
      js::BindingPattern::BindingIdentifier(name) => {
        let name = name.name.to_string();
        self.scope.push((name.clone(), Expr::Var(name.clone())));
        self.note_kind(&name, init);
        name
      }
      pattern => {
        let name = self.temp();
        let from = self.scope.len();
        crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
        {
          for at in from..self.scope.len() {
            let (bound, value) = self.scope[at].clone();
            if bound.starts_with('$') || crate::component::plain(&value) {
              continue;
            }
            let held = self.temp();
            self.extra_lets.push(Stmt::Let { name: held.clone(), expr: value });
            self.scope[at].1 = Expr::Var(held);
          }
        }
        name
      }
    };
    Ok(Stmt::Let { name, expr })
  }

  pub(crate) fn stmt(&mut self, stmt: &js::Statement) -> Lowered<Stmt> {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        if var.declarations.len() != 1 {
          return Err(self.residue(var.span, "one binding per declaration"));
        }
        self.declare(&var.declarations[0])
      }
      js::Statement::IfStatement(if_stmt) => {
        if let Some((kind, message)) = self.as_fail(&if_stmt.consequent) {
          if if_stmt.alternate.is_some() {
            return Err(self.residue(if_stmt.span, "an `else` after `fail`"));
          }
          let cond = self.expr(&if_stmt.test)?;
          let (kind, message) = (kind?, message?);
          return Ok(Stmt::Guard { cond, kind, message });
        }
        let cond = self.expr(&if_stmt.test)?;
        let then = self.branch(&if_stmt.consequent)?;
        let r#else = match &if_stmt.alternate {
          Some(alt) => self.branch(alt)?,
          None => Vec::new(),
        };
        Ok(Stmt::If { cond, then, r#else })
      }
      js::Statement::ForOfStatement(for_of) => {
        let js::ForStatementLeft::VariableDeclaration(decl) = &for_of.left else {
          return Err(self.residue(for_of.span, "`for...of` must declare its variable"));
        };
        let Some(pattern) = decl.declarations.first().map(|d| &d.id) else {
          return Err(self.residue(decl.span, "`for...of` must declare its variable"));
        };
        let mut over = self.expr(&for_of.right)?;
        if self.kind_of(&for_of.right) == Some(Kind::Map) {
          over = Expr::Entries(Box::new(over));
        }
        let depth = self.scope.len();
        let name = match pattern {
          js::BindingPattern::BindingIdentifier(name) => {
            let name = name.name.to_string();
            self.scope.push((name.clone(), Expr::Var(name.clone())));
            name
          }
          pattern => {
            let name = self.temp();
            crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
            name
          }
        };
        self.breakable.push(Breakable::Loop);
        let body = self.branch(&for_of.body);
        self.breakable.pop();
        self.scope.truncate(depth);
        Ok(Stmt::ForOf { name, over, body: body? })
      }
      js::Statement::ReturnStatement(ret) if self.markup_base.is_some() => {
        let base = self.markup_base.expect("checked");
        let path = self.markup_returns.len();
        let mut fields = vec![Entry::Field("$path".to_owned(), Expr::Lit(Lit::Int(path as i128)))];
        fields.extend(self.scope[base..].iter().enumerate().map(|(k, (_, expr))| Entry::Field(format!("${k}"), expr.clone())));
        let names = self.scope[base..].iter().map(|(name, _)| name.clone()).collect();
        self.markup_returns.push((crate::lo(ret.span), names));
        Ok(Stmt::Return(Expr::Object(fields)))
      }
      js::Statement::ReturnStatement(ret) => match &ret.argument {
        Some(arg) => Ok(Stmt::Return(self.expr(arg)?)),
        None => Ok(Stmt::Return(Expr::Lit(Lit::Null))),
      },
      js::Statement::ExpressionStatement(expr_stmt) => self.effect(&expr_stmt.expression),
      js::Statement::ThrowStatement(thrown) => Ok(Stmt::Expr(crate::component::throw_expr(self, thrown)?)),
      js::Statement::TryStatement(t) => self.try_stmt(t),
      js::Statement::BreakStatement(b) if b.label.is_none() && self.breakable.last() == Some(&Breakable::Loop) => Ok(Stmt::Break),
      js::Statement::ContinueStatement(c) if c.label.is_none() && self.breakable.contains(&Breakable::Loop) => Ok(Stmt::Continue),
      js::Statement::BlockStatement(block) => Err(self.residue(block.span, "a bare block")),
      other => {
        let (what, hint) = describe_stmt(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  /// `redirect(to)`: the response that sends the caller to `to`, which ends the body where it is a statement. An action or a route handler holds one.
  /// `after(fn)`: the function, written in place with no parameters, to run once the response is written. Its body lowers as a body does; it cannot write the session, whose cookie is already sent.
  fn after_of(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let body = match call.arguments.as_slice() {
      [work] if !crate::syntax::is_spread(work) => match strip_parens(crate::syntax::arg_expr(work)) {
        js::Expression::ArrowFunctionExpression(arrow) if crate::params_of(&arrow.params).is_empty() => match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(block) => self.block(&block.body)?,
          crate::syntax::ArrowBody::Expr(e) => vec![Stmt::Return(self.expr(e)?)],
        },
        js::Expression::FunctionExpression(f) if crate::params_of(&f.params).is_empty() => match &f.body {
          Some(block) => self.block(&block.statements)?,
          None => Vec::new(),
        },
        _ => return Err(self.residue(call.span, "`after` takes a function written in place, with no parameters")),
      },
      _ => return Err(self.residue(call.span, "`after` takes a function written in place, with no parameters")),
    };
    if writes_session(&body) {
      return Err(self.residue(call.span, "a session write inside `after`, which runs once the response and its cookie are sent"));
    }
    Ok(Expr::After(Box::new(Expr::Lambda { params: Vec::new(), body: Box::new(Expr::Block(body)) })))
  }

  fn redirect_to(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    if !self.redirects {
      return Err(self.residue_with(
        call.span,
        "`redirect` outside an action or a route handler",
        "middleware returns `Response.redirect(to)`; a loader cannot send the visitor elsewhere",
      ));
    }
    let [to] = call.arguments.as_slice() else {
      return Err(self.residue(call.span, "`redirect` takes the path to send the caller to"));
    };
    Ok(Expr::Builtin { name: Builtin::RedirectTo, args: vec![self.expr(crate::syntax::arg_expr(to))?] })
  }

  pub(crate) fn try_stmt(&mut self, t: &js::TryStatement) -> Lowered<Stmt> {
    let body = self.block(&t.block.body)?;
    let (name, handler) = match &t.handler {
      None => (None, None),
      Some(catch) => {
        let depth = self.scope.len();
        let name = crate::component::catch_param(self, catch.param.as_ref());
        let handler = name.and_then(|name| Ok((name, self.block(&catch.body.body)?)));
        self.scope.truncate(depth);
        let (name, handler) = handler?;
        (name, Some(handler))
      }
    };
    let finally = match &t.finalizer {
      Some(finalizer) => self.block(&finalizer.body)?,
      None => Vec::new(),
    };
    Ok(Stmt::Try { body, name, handler, finally })
  }

  fn branch(&mut self, stmt: &js::Statement) -> Lowered<Body> {
    match stmt {
      js::Statement::BlockStatement(block) => self.block(&block.body),
      single => self.block(std::slice::from_ref(single)),
    }
  }

  /// `fail("kind", "message")` as a statement, bare or in a one-statement block.
  fn as_fail(&mut self, stmt: &js::Statement) -> Option<(Lowered<String>, Lowered<Expr>)> {
    let inner = match stmt {
      js::Statement::BlockStatement(b) if b.body.len() == 1 => &b.body[0],
      other => other,
    };
    let js::Statement::ExpressionStatement(e) = inner else { return None };
    let js::Expression::CallExpression(call) = &e.expression else { return None };
    if !self.parsed.names_server(&call.callee, "fail") {
      return None;
    }
    let kind = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
      Some(js::Expression::StringLiteral(s)) => Ok(crate::lossy(s.value)),
      Some(other) => Err(self.residue(other.span(), "`fail` takes its kind as a string literal, since the kind is matched at build time")),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    let message = match call.arguments.get(1) {
      Some(a) => self.expr(crate::syntax::arg_expr(a)),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    Some((kind, message))
  }

  /// A statement that is an expression: a session write, a delete, a bare
  /// `fail` or an awaited call for its effect.
  pub(crate) fn effect(&mut self, expr: &js::Expression) -> Lowered<Stmt> {
    if self.object_write(expr) {
      if heap_pass() {
        return Ok(Stmt::Expr(self.heap_write(expr)?));
      }
      want_heap();
    }
    // A `console` call in a helper's block logs on the server as one in a component's body does.
    if let js::Expression::CallExpression(call) = expr {
      if let Some((response, op, args)) = self.response_headers_call(call) {
        if matches!(op.as_str(), "set" | "append" | "delete") {
          let js::Expression::Identifier(local) = crate::meaning::bare(response) else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let Some((_, Expr::Var(name))) = self.scope.iter().rev().find(|(n, _)| n == local.name.as_str()).cloned() else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let wanted = if op == "delete" { 1 } else { 2 };
          if args.len() != wanted {
            return Err(self.residue(call.span, format!("`headers.{op}` takes {wanted} arguments")));
          }
          let mut lowered = vec![Expr::Var(name.clone()), Expr::lit_str(&op)];
          for arg in args {
            lowered.push(self.expr(crate::syntax::arg_expr(arg))?);
          }
          return Ok(Stmt::Set { name, expr: Expr::Builtin { name: Builtin::ResponseHeaderSet, args: lowered } });
        }
      }
      if let Some(logged) = crate::component::console_call(self, call)? {
        return Ok(Stmt::Expr(logged));
      }
      if let Some(act) = self.settling(call)? {
        return Ok(act);
      }
      if let Some(listen) = self.listening(call)? {
        return Ok(listen);
      }
      if let Some(arrow) = self.async_iife(call) {
        let ran = match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => self.async_stmts(&b.body)?,
          crate::syntax::ArrowBody::Expr(e) => {
            let b = self.build();
            self.async_stmts(&[b.expr_stmt(b.copy(e))])?
          }
        };
        return Ok(Stmt::Expr(ran));
      }
    }
    match expr {
      js::Expression::AssignmentExpression(assign) if self.cell_of_target(&assign.left).is_some() => {
        let handle = self.cell_of_target(&assign.left).expect("checked");
        let b = syntax::Build::new(self.parsed.arena());
        let read = syntax::target_expr(b, &assign.left).expect("cell_of_target answers for a name or a path");
        let value = match syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
          None => self.expr(&assign.right)?,
          Some(computed) => self.expr(&computed)?,
        };
        let value = match &read {
          member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
            let (_, path) = write_root(member).expect("cell_of_target found the root");
            self.written_at(Expr::ReadCell(Box::new(handle.clone())), &path, value)?
          }
          _ => value,
        };
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.cells.iter().any(|(name, _)| *name == id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!() };
        let handle = self.cells.iter().find(|(name, _)| *name == id.name.as_str()).map(|(_, handle)| handle.clone()).expect("checked");
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = self.build();
        let one = b.number(1.0, update.span);
        let value = self.expr(&b.binary(b.updated(&update.argument), op, one, update.span))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expression::AssignmentExpression(assign) if self.local_target(&assign.left).is_some() => self.local_assign(assign),
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.is_local(id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!() };
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = self.build();
        let one = b.number(1.0, update.span);
        let expr = self.expr(&b.binary(b.updated(&update.argument), op, one, update.span))?;
        Ok(Stmt::Set { name: id.name.to_string(), expr })
      }
      js::Expression::CallExpression(call) if self.cell_collection_write(call).is_some() => {
        let (handle, kind, method) = self.cell_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::ReadCell(Box::new(handle.clone())))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(expr))))
      }
      js::Expression::CallExpression(call) if self.local_url_search_write(call).is_some() => {
        let (name, method) = self.local_url_search_write(call).expect("checked");
        let search = Expr::Builtin { name: Builtin::UrlPart, args: vec![Expr::Var(name.clone()), Expr::lit_str("search")] };
        let written = self.collection_written(call, Kind::Query, &method, search)?;
        Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str("search"), written] }, name })
      }
      js::Expression::CallExpression(call) if self.local_collection_write(call).is_some() => {
        let (name, kind, method) = self.local_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::Var(name.clone()))?;
        Ok(Stmt::Set { name, expr })
      }
      js::Expression::CallExpression(call) if self.unbound_collection_write(call).is_some() => {
        let (name, span) = self.unbound_collection_write(call).expect("checked");
        self.unbound = Some(name.clone());
        Err(self.residue(span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body")))
      }
      js::Expression::CallExpression(call) if self.local_push(call).is_some() => {
        let (name, method) = self.local_push(call).expect("checked");
        let mut items = Vec::with_capacity(call.arguments.len() + 1);
        for arg in &call.arguments {
          let value = self.expr(crate::syntax::arg_expr(arg))?;
          items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
        }
        let held = Entry::Spread(Expr::Var(name.clone()));
        if method == "push" {
          items.insert(0, held);
        } else {
          items.push(held);
        }
        Ok(Stmt::Set { name, expr: Expr::Array(items) })
      }
      js::Expression::AssignmentExpression(assign) => {
        if assign.operator != js::AssignmentOperator::Assign {
          return Err(self.residue(assign.span, "a compound assignment; write the full expression"));
        }
        let Some(member) = assign.left.as_member_expression() else {
          return Err(self.residue(assign.left.span(), "an assignment to something other than the session"));
        };
        let (key, path) = self.session_target(member)?;
        let value = self.expr(&assign.right)?;
        Ok(Stmt::SessionSet { key, path, value })
      }
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => {
        let member = match &unary.argument {
          js::Expression::ChainExpression(o) => match o.expression.as_member_expression() {
            Some(member) => member,
            None => return Err(self.residue(o.expression.span(), "`delete` of a call")),
          },
          other => match other.as_member_expression() {
            Some(member) => member,
            None => return Err(self.residue(other.span(), "`delete` of something other than a session entry")),
          },
        };
        let (key, path) = self.session_target(member)?;
        Ok(Stmt::SessionDelete { key, path })
      }
      js::Expression::CallExpression(call) if self.parsed.names_server(&call.callee, "redirect") => {
        Ok(Stmt::Return(self.redirect_to(call)?))
      }
      js::Expression::CallExpression(call) if self.parsed.names_server(&call.callee, "after") => Ok(Stmt::Expr(self.after_of(call)?)),
      js::Expression::CallExpression(call) if self.is_ident_call(call, "fail") => {
        let b = syntax::Build::new(self.parsed.arena());
        let stmt = b.expr_stmt(b.copy(expr));
        let (kind, message) = self.as_fail(&stmt).expect("checked");
        Ok(Stmt::Guard { cond: Expr::Lit(Lit::Bool(true)), kind: kind?, message: message? })
      }
      js::Expression::CallExpression(call) if self.is_session_extend(call) => {
        if !self.extends {
          return Err(self.residue_with(
            call.span,
            "`session.extend` outside an action, a route handler or middleware",
            "a loader runs on every navigation, so extending there is a store write per page view; extend from an action, a route handler or middleware, which can decide when",
          ));
        }
        let [arg] = call.arguments.as_slice() else {
          return Err(self.residue(call.span, "`session.extend` takes seconds, one number"));
        };
        Ok(Stmt::SessionExtend { seconds: self.expr(crate::syntax::arg_expr(arg))? })
      }
      other => Ok(Stmt::Expr(self.expr(other)?)),
    }
  }

  /// The argument of the global `Promise.all(...)` or, where no handler resumes later, `Promise.resolve(...)`. A body has no promise values, since a call is its answer, so the argument is what it reads as.
  fn promise_all<'e>(&self, e: &'e js::Expression<'e>) -> Option<&'e js::Expression<'e>> {
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
    let callee = &call.callee;
    let Some(m) = (callee).as_member_expression() else { return None };
    let (js::Expression::Identifier(promise), crate::syntax::MemberProp::Ident(all)) = (m.object(), crate::syntax::prop(m)) else { return None };
    let settled_here = all.name.as_str() == "resolve" && !self.reaches(Reach::Later);
    if self.parsed.ctxt(promise) != crate::Ctxt::Unresolved || promise.name.as_str() != "Promise" || !(all.name.as_str() == "all" || settled_here) {
      return None;
    }
    let [arg] = call.arguments.as_slice() else { return None };
    (!crate::syntax::is_spread(arg)).then_some(crate::syntax::arg_expr(arg))
  }

  /// Whether `name` is a binding the body declared, which a write may rebind.
  fn is_local(&self, name: &str) -> bool {
    match self.scope.iter().rev().find(|(n, _)| n == name) {
      Some((_, bound)) => matches!(bound, Expr::Var(var) if var == name),
      // A module-level `let` a helper writes resolves to its own name, bound by the component that reads it.
      None => matches!(self.globals.iter().rev().find(|(n, _)| n == name), Some((_, Expr::Var(var))) if var == name),
    }
  }

  /// `x++` or `--x` inside an expression, with `x` a local: the old value is held under a temporary, the write becomes a statement before the one holding it and the expression reads the temporary, the new value for a prefix. Only an update the statement reaches unconditionally is lifted; one under a condition or inside a function stays where it is.
  pub(crate) fn lift_updates(&mut self, stmt: &js::Statement, out: &mut Vec<Stmt>) -> Lowered<()> {
    if let js::Statement::ExpressionStatement(e) = stmt {
      if matches!(strip_parens(&e.expression), js::Expression::UpdateExpression(_)) {
        return Ok(());
      }
    }
    for (span, name, prefix, plus) in crate::component::updates_in(stmt) {
      if !self.is_local(&name) {
        // A name bound nowhere here may be a module-level `let`, which the set binds and retries with.
        if !self.scope.iter().any(|(n, _)| *n == name) && !self.globals.iter().any(|(n, _)| *n == name) {
          self.unbound = Some(name.clone());
          return Err(self.residue(span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body")));
        }
        continue;
      }
      let op = if plus { ArithOp::Add } else { ArithOp::Sub };
      let next = Expr::Arith(op, Box::new(Expr::Var(name.clone())), Box::new(Expr::Lit(Lit::Float(1.0))));
      if prefix {
        out.push(Stmt::Set { name: name.clone(), expr: next });
        self.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(name));
      } else {
        let held = self.temp();
        out.push(Stmt::Let { name: held.clone(), expr: Expr::Var(name.clone()) });
        out.push(Stmt::Set { name, expr: next });
        self.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(held));
      }
    }
    Ok(())
  }

  /// The local a write lands on: `x` or `x.field` with `x` declared in the body.
  fn local_target(&self, target: &js::AssignmentTarget) -> Option<String> {
    match target {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.is_local(id.name.as_str()).then(|| id.name.to_string()),
      other => match write_root(other.as_member_expression()?) {
        Some((id, _)) if self.is_local(id.name.as_str()) => Some(id.name.to_string()),
        _ => None,
      },
    }
  }

  /// `current` with what `path` reaches replaced by `value`, each step on the way copied: a field as a spread with the field changed, an index through `SetAt`.
  pub(crate) fn written_at(&mut self, current: Expr, path: &[&js::MemberExpression], value: Expr) -> Lowered<Expr> {
    let Some((step, rest)) = path.split_first() else { return Ok(value) };
    if let Some(field) = self.member_name(step) {
      let inner = self.written_at(current.clone().field(field.clone()), rest, value)?;
      return Ok(Expr::Object(vec![Entry::Spread(current), Entry::Field(field, inner)]));
    }
    let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(step) else { return Err(self.residue(step.span(), "a write through a private field")) };
    let key = self.expr(key)?;
    let inner = self.written_at(Expr::Index(Box::new(current.clone()), Box::new(key.clone())), rest, value)?;
    Ok(Expr::Builtin { name: Builtin::SetAt, args: vec![current, key, inner] })
  }

  /// Whether `call` is one [`Lowerer::settling`] makes an act of. An async arrow called in place is one too, since it issues an act at its first `await`.
  pub(crate) fn issues_act(&self, call: &js::CallExpression) -> bool {
    if self.async_iife(call).is_some() || self.listens(call) {
      return true;
    }
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return false };
    let settler = |e: &js::Expression| matches!(crate::meaning::bare(e), js::Expression::Identifier(id) if self.settlers.iter().any(|s| *s == id.name.as_str()));
    settler(callee) || (self.reaches(Reach::Later) && id.name.as_str() == "setTimeout" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved && matches!(call.arguments.as_slice(), [callback, _] if settler(crate::syntax::arg_expr(callback)) || matches!(crate::meaning::bare(crate::syntax::arg_expr(callback)), js::Expression::ArrowFunctionExpression(_))))
  }

  /// Whether `call` is `system.spawn` from the standard module.
  pub(crate) fn spawns(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    self.meanings.as_ref().is_some_and(|meanings| meanings.names_spawn(callee))
  }

  /// `spawn(command, args, options)` in a terminal block: a fresh process id and the act [`snapfire_fsr_ir::host::Act::Spawn`] starting it, whose value is the process, [`snapfire_fsr_ir::host::PROCESS`].
  fn spawn_value(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let id = self.temp();
    let mut fields = vec![Entry::Field("id".to_owned(), Expr::Var(id.clone()))];
    for (i, key) in ["command", "args", "options"].iter().enumerate() {
      let value = match call.arguments.get(i) {
        Some(arg) if crate::syntax::is_spread(arg) => return Err(self.residue(crate::syntax::arg_expr(arg).span(), "a spread into `spawn`")),
        Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      fields.push(Entry::Field((*key).to_owned(), value));
    }
    let random = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let spawn = Stmt::Act { action: snapfire_fsr_ir::host::Act::Spawn.name().to_owned(), input: Expr::Object(fields) };
    Ok(Expr::Block(vec![Stmt::Let { name: id.clone(), expr: random }, spawn, Stmt::Return(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::PROCESS.to_owned(), Expr::Var(id))]))]))
  }

  /// The process, the report and the listener of `sub.on(event, f)`, `sub.stdout.on("data", f)` or `sub.stderr.on("data", f)` on a local a block bound to a spawn; also `stdout.on("resize", f)` and `stdout.off("resize", f)` on the terminal's output, `true` for `off`; the report `None` for `setEncoding`, which a terminal takes as read.
  fn listener<'c>(&self, call: &'c js::CallExpression<'c>) -> Option<(&'c js::Expression<'c>, Option<snapfire_fsr_ir::host::Report>, Option<&'c js::Expression<'c>>, bool)> {
    use snapfire_fsr_ir::host::Report;
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let event = call.arguments.first().and_then(|a| string_literal(crate::syntax::arg_expr(a)));
    if let (Some(on @ ("on" | "off")), Some(event)) = (self.member_name(method).as_deref(), event.as_deref()) {
      if event == Report::Resize.name() {
        return Some((method.object(), Some(Report::Resize), call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), on == "off"));
      }
    }
    let (process, stream) = match crate::meaning::bare(method.object()) {
      process @ js::Expression::Identifier(id) if self.spawned.iter().any(|s| *s == id.name.as_str()) => (process, None),
      inner_expr if inner_expr.is_member_expression() => { let inner = inner_expr.as_member_expression().expect("matched as a member"); match (crate::meaning::bare(inner.object()), self.member_name(inner).as_deref()) {
        (process @ js::Expression::Identifier(id), Some(stream @ ("stdout" | "stderr"))) if self.spawned.iter().any(|s| *s == id.name.as_str()) => (process, Some(if stream == "stdout" { Report::Stdout } else { Report::Stderr })),
        _ => return None,
      } },
      _ => return None,
    };
    let report = match (stream, self.member_name(method).as_deref(), event.as_deref()) {
      (Some(_), Some("setEncoding"), _) => return Some((process, None, None, false)),
      (Some(stream), Some("on"), Some("data")) => stream,
      (None, Some("on"), Some(event)) => Report::parse(event)?,
      _ => return None,
    };
    Some((process, Some(report), call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), false))
  }

  /// Whether `call` is a listener [`Lowerer::listener`] reads.
  fn listens(&self, call: &js::CallExpression) -> bool {
    self.reaches(Reach::Process) && self.listener(call).is_some()
  }

  /// A listener on a spawned process as the act [`snapfire_fsr_ir::host::Act::Listen`]. Its callback is a continuation with `code` and `signal` bound from an exit's report; a promise's `resolve` or `reject` settles the promise instead. `None` for any other call.
  fn listening(&mut self, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    let Some((process, report, callback, removes)) = self.listener(call) else { return Ok(None) };
    let Some(report) = report else { return Ok(Some(Stmt::Expr(Expr::Lit(Lit::Null)))) };
    let Some(callback) = callback else { return Err(self.residue(call.span, "a process listener without a function")) };
    if removes {
      return Err(self.residue(call.span, "an `off` outside a component's handler"));
    }
    let process = self.expr(process)?;
    let mut fields = vec![Entry::Field("process".to_owned(), process), Entry::Field("report".to_owned(), Expr::lit_str(report.name()))];
    if matches!(crate::meaning::bare(callback), js::Expression::Identifier(id) if self.settlers.iter().any(|s| *s == id.name.as_str())) {
      fields.push(Entry::Field("settle".to_owned(), self.expr(callback)?));
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }));
    }
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(callback) else { return Err(self.residue(callback.span(), "a process listener that is not an arrow written in place or a promise's `resolve` or `reject`")) };
    let params: Vec<String> = crate::params_of(&arrow.params).iter().map(|p| match (p.pattern, p.default.is_none() && !p.rest) {
      (js::BindingPattern::BindingIdentifier(name), true) => Ok(name.name.to_string()),
      _ => Err(self.residue(p.span, "a process listener's parameter that is not a name")),
    }).collect::<Lowered<_>>()?;
    let exits = matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close);
    let (event, carry) = self.continuation(call.span, None, |this| {
      let value = Expr::Var("$resume".to_owned()).field("value");
      for (i, param) in params.iter().enumerate() {
        let bound = match (exits, i) {
          (true, 0) => value.clone().field("code"),
          (true, _) => value.clone().field("signal"),
          (false, _) => value.clone(),
        };
        this.scope.push((param.clone(), bound));
      }
      match crate::syntax::arrow_body_of(arrow) {
        crate::syntax::ArrowBody::Block(b) => this.async_stmts(&b.body),
        crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          this.async_stmts(&[b.expr_stmt(b.copy(e))])
        }
      }
    })?;
    fields.push(Entry::Field("event".to_owned(), Expr::lit_str(event)));
    fields.push(Entry::Field("carry".to_owned(), carry));
    Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }))
  }

  /// `p.then(f)` in a terminal page: a fresh promise and a continuation on `p` running `f` with what `p` resolved to, which settles the fresh one with what `f` returns; one `p` rejects rejects it as well.
  fn then_call(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let (true, callee) = (self.reaches(Reach::Later), &call.callee) else { return Ok(None) };
    let Some(member) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(None) };
    if self.member_name(member).as_deref() != Some("then") || call.arguments.len() != 1 {
      return Ok(None);
    }
    let Some(js::Expression::ArrowFunctionExpression(f)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Ok(None) };
    let awaited = self.expr(member.object())?;
    let done = self.temp();
    let param = match crate::params_of(&f.params).as_slice() {
      [] => None,
      [crate::Param { pattern: js::BindingPattern::BindingIdentifier(name), default: None, rest: false, .. }] => Some(name.name.to_string()),
      _ => return Err(self.residue(call.span, "a `then` callback whose parameter is not one name")),
    };
    let finished = Expr::Var(done.clone());
    let (event, carry) = self.continuation(call.span, param, |this| {
      let stmts = match crate::syntax::arrow_body_of(f) {
        crate::syntax::ArrowBody::Block(b) => b.body,
        crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          b.stmts([b.return_stmt(Some(b.copy(e)), e.span())])
        }
      };
      let ran = this.async_to(stmts, Some(finished.clone()))?;
      let resumed = Expr::Var("$resume".to_owned());
      let rejected = Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), finished.clone()), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(false)))])), Entry::Field("value".to_owned(), resumed.clone().field("value"))]) };
      Ok(Expr::Ternary(Box::new(resumed.field("ok")), Box::new(ran), Box::new(Expr::Block(vec![rejected]))))
    })?;
    let then = Stmt::Act { action: snapfire_fsr_ir::host::Act::Then.name().to_owned(), input: Expr::Object(vec![Entry::Field("deferred".to_owned(), awaited), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]) };
    Ok(Some(Expr::Block(vec![Stmt::Let { name: done.clone(), expr: self.new_deferred() }, then, Stmt::Return(Expr::Var(done))])))
  }

  /// A fresh deferred, `{ "$deferred": id }`.
  fn new_deferred(&self) -> Expr {
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)])
  }

  /// Ink's `suspendTerminal(f)` in a terminal page: `f` run as an async function, then a [`snapfire_fsr_ir::host::Act::Suspend`] asking the terminal to give up the screen until `f`'s promise settles. The call's own promise settles as `f`'s does once the screen is back.
  fn suspend_call(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return Ok(None) };
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    let suspends = match self.scope.iter().rev().find(|(n, _)| *n == id.name.as_str()) {
      Some((_, bound)) => crate::terminal::primitive_ref(bound).as_deref() == Some("suspendTerminal"),
      None => self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) == Some(&meaning::Meaning::Library(meaning::Construct::Suspend)),
    };
    if !suspends {
      return Ok(None);
    }
    let Some(js::Expression::ArrowFunctionExpression(f)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.residue(call.span, "`suspendTerminal` of something other than an arrow written in place")) };
    let inner = self.async_value(f)?;
    let (until, settles) = (self.temp(), self.temp());
    let suspend = Stmt::Act { action: snapfire_fsr_ir::host::Act::Suspend.name().to_owned(), input: Expr::Object(vec![Entry::Field("until".to_owned(), Expr::Var(until.clone())), Entry::Field("settles".to_owned(), Expr::Var(settles.clone()))]) };
    Ok(Some(Expr::Block(vec![Stmt::Let { name: until, expr: inner }, Stmt::Let { name: settles.clone(), expr: self.new_deferred() }, suspend, Stmt::Return(Expr::Var(settles))])))
  }

  /// An async arrow as a value: its statements run as an async function. The promise it returns settles with what the function returns; it rejects with what an awaited promise the function does not catch rejects with.
  pub(crate) fn async_value(&mut self, f: &js::ArrowFunctionExpression) -> Lowered<Expr> {
    let done = self.temp();
    let ran = match crate::syntax::arrow_body_of(f) {
      crate::syntax::ArrowBody::Block(b) => self.async_to(&b.body, Some(Expr::Var(done.clone())))?,
      crate::syntax::ArrowBody::Expr(e) => {
        let b = self.build();
        self.async_to(&[b.return_stmt(Some(b.copy(e)), e.span())], Some(Expr::Var(done.clone())))?
      }
    };
    Ok(Expr::Block(vec![Stmt::Let { name: done.clone(), expr: self.new_deferred() }, Stmt::Expr(ran), Stmt::Return(Expr::Var(done))]))
  }

  /// `(async () => { … })()`: an async arrow called where it is written, in a terminal page.
  pub(crate) fn async_iife<'c>(&self, call: &'c js::CallExpression<'c>) -> Option<&'c js::ArrowFunctionExpression<'c>> {
    let callee = &call.callee;
    match crate::meaning::bare(callee) {
      js::Expression::ArrowFunctionExpression(arrow) if self.reaches(Reach::Later) && arrow.r#async && call.arguments.is_empty() && crate::params_of(&arrow.params).is_empty() => Some(arrow),
      _ => None,
    }
  }

  /// An async function's statements run as far as its first `await`, which hands the awaited promise a continuation, [`snapfire_fsr_ir::host::Act::Then`], holding the rest. The rest lowers as a handler named [`snapfire_fsr_ir::host::Event::Resume`] in [`Lowerer::continuations`]; the names it reads that are not module cells are carried to it with the act.
  pub(crate) fn async_stmts(&mut self, stmts: &[js::Statement]) -> Lowered<Expr> {
    self.async_to(stmts, None)
  }

  /// [`Lowerer::async_stmts`] for a function whose promise is `done`: what it returns settles it; an awaited promise that rejects rejects it.
  pub(crate) fn async_to(&mut self, stmts: &[js::Statement], done: Option<Expr>) -> Lowered<Expr> {
    let floor = self.async_floor.replace(self.scope.len());
    let tail = self.loop_tail.take();
    let lowered = self.async_caught(stmts, done, None);
    self.async_floor = floor;
    self.loop_tail = tail;
    lowered
  }

  /// [`Lowerer::async_to`] inside `catch`, where an awaited promise among the statements it covers that rejects runs the `catch` instead.
  fn async_caught(&mut self, stmts: &[js::Statement], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let depth = self.scope.len();
    let lowered = self.async_to_in(stmts, done, catch);
    self.scope.truncate(depth);
    lowered
  }

  /// A loop that awaits. Its head is a continuation named for the loop, run through `$run` with the live locals carried: when the test holds it runs one pass, the body and the step, whose every way through ends by running the head again; otherwise it runs the statements after the loop. The locals the head reads are carried by value, so a `let` the body writes reaches the next pass and the statements after.
  fn awaiting_loop_in(&mut self, stmts: &[js::Statement], at: usize, looped: AwaitingLoop, done: Option<Expr>, inside: Option<AsyncCatch>) -> Lowered<Expr> {
    if looped.exits_deep {
      return Err(self.residue(looped.span, "a `break` inside a loop that awaits or a `continue` or `return` deeper than an `if` at the top of its body"));
    }
    if looped.over.as_ref().is_some_and(|over| self.kind_of(over) == Some(Kind::Map)) {
      return Err(self.residue(looped.span, "`for...of` over a `Map` in a loop that awaits; iterate `[...map.entries()]`"));
    }
    let before = self.before_await(&stmts[..at])?;
    let init = self.before_await(&looped.init)?;
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.parsed.file, crate::lo(looped.span))).name();
    // Every variable in scope is carried, the function's promise among them: a pass may read or write any of them and a copy of one it does not touch costs nothing.
    let mut carried: Vec<String> = self.scope.iter().filter_map(|(name, expr)| matches!(expr, Expr::Var(var) if var == name).then(|| name.clone())).collect();
    if let Some(done) = &done {
      done.free_vars(&mut carried);
    }
    carried.retain(|name| name != "$resume" && name != "$event" && !self.cells.iter().any(|(cell, _)| cell == name));
    carried.sort();
    carried.dedup();
    let carry = Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect());
    let depth = self.scope.len();
    let outer_tail = self.loop_tail.replace(LoopTail { event: event.clone(), carry: carry.clone(), done: done.clone() });
    let covered = inside.clone().map(|catch| AsyncCatch { covers: looped.pass.len() + catch.covers - at - 1, ..catch });
    let pass = self.expr(&looped.test).and_then(|cond| self.async_caught(&looped.pass, None, covered).map(|pass| (cond, pass)));
    self.loop_tail = outer_tail;
    let (cond, pass) = pass?;
    let after = inside.map(|catch| AsyncCatch { covers: catch.covers - at - 1, ..catch }).filter(|catch| catch.covers > 0);
    let rest = self.async_caught(&stmts[at + 1..], done, after)?;
    self.scope.truncate(depth);
    let resume = "$resume".to_owned();
    let mut body = vec![Stmt::Let { name: resume.clone(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("carry").field(name.clone()) });
    }
    body.push(Stmt::If { cond, then: vec![Stmt::Expr(pass)], r#else: vec![Stmt::Expr(rest)] });
    body.push(Stmt::Return(Expr::Object(Vec::new())));
    if !self.continuations.iter().any(|held| held.event == event) {
      self.continuations.push(snapfire_fsr_ir::ast::Handler { event: event.clone(), body });
    }
    let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]);
    let mut entry = Vec::new();
    splice(before, &mut entry);
    splice(init, &mut entry);
    entry.push(Stmt::Act { action: snapfire_fsr_ir::host::Act::Run.name().to_owned(), input });
    Ok(Expr::Block(entry))
  }

  /// The statements before an `await`, as `let`s and effects whose names stay in scope for the awaited value and the continuation.
  fn before_await(&mut self, stmts: &[js::Statement]) -> Lowered<Expr> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
      out.push(self.stmt(stmt)?);
    }
    Ok(Expr::Block(out))
  }

  fn async_to_in(&mut self, stmts: &[js::Statement], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let settle = |done: &Expr, ok: bool, value: Expr| Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), done.clone()), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))])), Entry::Field("value".to_owned(), value)]) };
    let in_pass = self.loop_tail.is_some();
    let Some(at) = stmts.iter().position(|s| awaits(s) || (in_pass && if_ends_in_return(s))) else {
      if let Some(LoopTail { event, carry, done: promise }) = self.loop_tail.clone() {
        // A `return` in the pass settles the function's promise with its value and the loop is done; any other end of the pass runs the head again.
        if let Some(at) = stmts.iter().position(|s| matches!(s, js::Statement::ReturnStatement(_))) {
          let js::Statement::ReturnStatement(ret) = &stmts[at] else { unreachable!("found a return") };
          let Some(promise) = promise else { return Err(self.residue(ret.span, "a `return` inside a loop that awaits, in a function with no promise to settle")) };
          let ran = self.before_await(&stmts[..at])?;
          let value = match &ret.argument {
            Some(arg) => self.expr(arg)?,
            None => Expr::Lit(Lit::Undefined),
          };
          return Ok(Expr::Block(vec![Stmt::Expr(ran), settle(&promise, true, value)]));
        }
        let ran = self.before_await(stmts)?;
        let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]);
        return Ok(Expr::Block(vec![Stmt::Expr(ran), Stmt::Act { action: snapfire_fsr_ir::host::Act::Run.name().to_owned(), input }]));
      }
      let floor = self.async_floor.unwrap_or(self.scope.len());
      let value = crate::component::block_to_expr_from(self, stmts, floor)?;
      return Ok(match &done {
        None => value,
        Some(done) => {
          let (returned, thrown) = (self.temp(), self.temp());
          let ran = vec![Stmt::Let { name: returned.clone(), expr: value }, settle(done, true, Expr::Var(returned))];
          Expr::Block(vec![Stmt::Try { body: ran, name: Some(thrown.clone()), handler: Some(vec![settle(done, false, Expr::Var(thrown))]), finally: Vec::new() }])
        }
      });
    };
    let inside = catch.filter(|catch| at < catch.covers);
    if let Some(looped) = awaiting_loop(self.parsed, &stmts[at]) {
      return self.awaiting_loop_in(stmts, at, looped, done, inside);
    }
    if let js::Statement::TryStatement(t) = &stmts[at] {
      if inside.is_some() {
        return Err(self.residue(t.span, "a `try` that awaits inside another"));
      }
      let b = self.build();
      let finally: Vec<js::Statement<'a>> = t.finalizer.iter().flat_map(|f| f.body.iter().map(|s| b.copy(s))).collect();
      if !finally.is_empty() && (t.block.body.iter().any(crate::component::holds_return) || t.handler.iter().any(|h| h.body.body.iter().any(crate::component::holds_return))) {
        return Err(self.residue(t.span, "a `return` inside a `try` whose `finally` follows an `await`"));
      }
      let before = self.before_await(&stmts[..at])?;
      let after: Vec<js::Statement<'a>> = finally.iter().map(|s| b.copy(s)).chain(stmts[at + 1..].iter().map(|s| b.copy(s))).collect();
      let rethrown = |covers: usize| {
        let name = format!("$finally{}", crate::lo(t.span));
        let thrown = js::Statement::new_throw_statement(t.span, b.ident(&name, t.span), &b.ast());
        AsyncCatch { param: Some(name), body: b.stmts(finally.iter().map(|s| b.copy(s)).chain(std::iter::once(thrown))), covers, inner: None }
      };
      let catch = match &t.handler {
        Some(handler) => {
          let param = match handler.param.as_ref().map(|p| &p.pattern) {
            None => None,
            Some(js::BindingPattern::BindingIdentifier(name)) => Some(name.name.to_string()),
            Some(other) => return Err(self.residue(other.span(), "a `catch` parameter that is not a name")),
          };
          let inner = (!finally.is_empty()).then(|| Box::new(rethrown(handler.body.body.len())));
          AsyncCatch { param, body: b.stmts(handler.body.body.iter().chain(&after).map(|s| b.copy(s))), covers: t.block.body.len(), inner }
        }
        None => rethrown(t.block.body.len()),
      };
      let body: Vec<js::Statement<'a>> = t.block.body.iter().chain(&after).map(|s| b.copy(s)).collect();
      let ran = self.async_caught(&body, done, Some(catch))?;
      return Ok(spliced(before, vec![Stmt::Expr(ran)]));
    }
    if let js::Statement::IfStatement(branch) = &stmts[at] {
      let before = self.before_await(&stmts[..at])?;
      let cond = self.expr(&branch.test)?;
      let b = self.build();
      let arm = |this: &mut Self, arm: Option<&js::Statement>| -> Lowered<Expr> {
        let mut joined: Vec<js::Statement<'a>> = match arm {
          Some(js::Statement::BlockStatement(block)) => block.body.iter().map(|s| b.copy(s)).collect(),
          Some(other) => vec![b.copy(other)],
          None => Vec::new(),
        };
        let covered = inside.clone().map(|catch| AsyncCatch { covers: joined.len() + catch.covers - at - 1, ..catch });
        joined.extend(stmts[at + 1..].iter().map(|s| b.copy(s)));
        this.async_caught(&joined, done.clone(), covered)
      };
      let then = arm(self, Some(&branch.consequent))?;
      let otherwise = arm(self, branch.alternate.as_ref())?;
      return Ok(spliced(before, vec![Stmt::If { cond, then: vec![Stmt::Expr(then)], r#else: vec![Stmt::Expr(otherwise)] }]));
    }
    let (awaited, bound, assigned) = match &stmts[at] {
      js::Statement::ExpressionStatement(e) => match crate::meaning::bare(&e.expression) {
        js::Expression::AwaitExpression(a) => (&a.argument, None, None),
        js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign && matches!(crate::meaning::bare(&assign.right), js::Expression::AwaitExpression(_)) => {
          let js::Expression::AwaitExpression(a) = crate::meaning::bare(&assign.right) else { unreachable!("matched an await") };
          let hidden = format!("$await{}", crate::lo(e.span));
          let b = self.build();
          let value = b.ident(&hidden, e.span);
          if let js::Expression::Identifier(id) = &value {
            self.parsed.scope.mark_top(id);
          }
          let assigned = b.expr_stmt(js::Expression::new_assignment_expression(assign.span, assign.operator, b.copy(&assign.left), value, &b.ast()));
          (&a.argument, Some(hidden), Some(assigned))
        }
        _ => return Err(self.residue(e.span, "an `await` inside an expression; await as a statement, a `const`'s value or an assignment")),
      },
      js::Statement::VariableDeclaration(var) => match (var.declarations.as_slice(), var.declarations.first().and_then(|d| d.init.as_ref()).map(crate::meaning::bare)) {
        ([decl], Some(js::Expression::AwaitExpression(a))) => match &decl.id {
          js::BindingPattern::BindingIdentifier(name) => (&a.argument, Some(name.name.to_string()), None),
          _ => return Err(self.residue(decl.span, "an awaited value bound to a pattern")),
        },
        _ => return Err(self.residue(var.span, "an `await` inside an expression; await as a statement, a `const`'s value or an assignment")),
      },
      other => return Err(self.residue(other.span(), "an `await` inside a `switch`, a `do...while`, a `for...in` or a bare block")),
    };
    let before = self.before_await(&stmts[..at])?;
    let deferred = self.expr(awaited)?;
    let prepended = assigned.is_some() as usize;
    let b = self.build();
    let joined: Vec<js::Statement<'a>> = assigned.into_iter().chain(stmts[at + 1..].iter().map(|s| b.copy(s))).collect();
    let rest = &joined[..];
    let (event, carry) = self.continuation(stmts[at].span(), bound, |this| {
      let resumed = Expr::Var("$resume".to_owned());
      let covered = inside.clone().map(|catch| AsyncCatch { covers: catch.covers + prepended - at - 1, ..catch }).filter(|catch| catch.covers > 0);
      let ran = this.async_caught(rest, done.clone(), covered)?;
      let rejected = match (&inside, &done) {
        (Some(catch), _) => {
          let depth = this.scope.len();
          if let Some(param) = &catch.param {
            this.scope.push((param.clone(), resumed.clone().field("value")));
          }
          let caught = this.async_caught(&catch.body, done.clone(), catch.inner.clone().map(|inner| *inner));
          this.scope.truncate(depth);
          caught?
        }
        (None, Some(done)) => Expr::Block(vec![settle(done, false, resumed.clone().field("value"))]),
        (None, None) => return Ok(ran),
      };
      Ok(Expr::Ternary(Box::new(resumed.field("ok")), Box::new(ran), Box::new(rejected)))
    })?;
    let then = Stmt::Act { action: snapfire_fsr_ir::host::Act::Then.name().to_owned(), input: Expr::Object(vec![Entry::Field("deferred".to_owned(), deferred), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]) };
    Ok(spliced(before, vec![then]))
  }

  /// Code the host runs later, the rest of an async function or a timer's callback, as a handler named [`snapfire_fsr_ir::host::Event::Resume`] for `at` in [`Lowerer::continuations`]. `bound` names the settled value it is handed. Every other name it reads that is not a module cell is carried to it. Returns the event and the carry the act that schedules it passes.
  fn continuation(&mut self, at: Span, bound: Option<String>, lower: impl FnOnce(&mut Self) -> Lowered<Expr>) -> Lowered<(String, Expr)> {
    let resume = "$resume".to_owned();
    let depth = self.scope.len();
    // The settled value is a variable of the continuation, not the expression `$resume.value`: a later continuation carries the variable, where the expression would read its own settled value.
    if let Some(name) = &bound {
      self.scope.push((name.clone(), Expr::Var(name.clone())));
    }
    let rest = lower(self);
    self.scope.truncate(depth);
    let rest = rest?;
    let mut free = Vec::new();
    rest.free_vars(&mut free);
    free.sort();
    free.dedup();
    let carried: Vec<String> = free.into_iter().filter(|name| *name != resume && bound.as_ref() != Some(name) && !self.cells.iter().any(|(cell, _)| cell == name)).collect();
    let mut body = vec![Stmt::Let { name: resume.clone(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("carry").field(name.clone()) });
    }
    if let Some(name) = &bound {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("value") });
    }
    body.push(Stmt::Expr(rest));
    body.push(Stmt::Return(Expr::Object(Vec::new())));
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.parsed.file, crate::lo(at))).name();
    if !self.continuations.iter().any(|held| held.event == event) {
      self.continuations.push(snapfire_fsr_ir::ast::Handler { event: event.clone(), body });
    }
    let carry = Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect());
    Ok((event, carry))
  }

  /// In a promise's executor, `resolve(v)` or `reject(e)` as an [`snapfire_fsr_ir::host::Act::Settle`] and `setTimeout(resolve, ms)` as a timer that settles; `None` for any other call.
  fn settling(&mut self, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return Ok(None) };
    let settler = |this: &Self, e: &js::Expression| matches!(crate::meaning::bare(e), js::Expression::Identifier(id) if this.settlers.iter().any(|s| *s == id.name.as_str()));
    if settler(self, callee) {
      let value = match call.arguments.first() {
        Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      let settle = self.expr(callee)?;
      let input = Expr::Object(vec![Entry::Field("settle".to_owned(), settle), Entry::Field("value".to_owned(), value)]);
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input }));
    }
    if id.name.as_str() == "setTimeout" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved && self.reaches(Reach::Later) {
      if let [callback, delay] = call.arguments.as_slice() {
        if settler(self, crate::syntax::arg_expr(callback)) {
          let input = Expr::Object(vec![Entry::Field("settle".to_owned(), self.expr(crate::syntax::arg_expr(callback))?), Entry::Field("ms".to_owned(), self.expr(crate::syntax::arg_expr(delay))?)]);
          return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input }));
        }
        if let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(crate::syntax::arg_expr(callback)) {
          let ms = self.expr(crate::syntax::arg_expr(delay))?;
          let (event, carry) = self.continuation(call.span, None, |this| match crate::syntax::arrow_body_of(arrow) {
            crate::syntax::ArrowBody::Block(b) => this.async_stmts(&b.body),
            crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          this.async_stmts(&[b.expr_stmt(b.copy(e))])
        }
          })?;
          let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("ms".to_owned(), ms), Entry::Field("repeats".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("carry".to_owned(), carry)]);
          return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input }));
        }
      }
    }
    Ok(None)
  }

  /// `x = e`, `x += e` and their kin on a local; `x.field = e` rebinds `x` to a copy with that field.
  /// The handle of the cell an assignment writes, when it writes a name held in one or a path into one.
  pub(crate) fn cell_of_target(&self, target: &js::AssignmentTarget) -> Option<Expr> {
    let id = match target {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => &**id,
      other => write_root(other.as_member_expression()?)?.0,
    };
    self.cells.iter().rev().find(|(name, _)| *name == id.name.as_str()).map(|(_, handle)| handle.clone())
  }

  fn local_assign(&mut self, assign: &js::AssignmentExpression) -> Lowered<Stmt> {
    let name = self.local_target(&assign.left).expect("checked");
    let b = syntax::Build::new(self.parsed.arena());
    let read = syntax::target_expr(b, &assign.left).expect("local_target answers for a name or a path");
    let value = match syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
      None => self.expr(&assign.right)?,
      Some(computed) => self.expr(&computed)?,
    };
    let Some(member) = (&read).as_member_expression() else {
      return Ok(Stmt::Set { name, expr: value });
    };
    if let (Some(Kind::Url), js::Expression::Identifier(_), crate::syntax::MemberProp::Ident(part)) = (self.kind_of(member.object()), member.object(), crate::syntax::prop(member)) {
      let part = part.name.to_string();
      if part == "origin" || !URL_PARTS.contains(&part.as_str()) && part != "href" {
        return Err(self.residue(member.span(), format!("a write to `.{part}` of a `URL`")));
      }
      return Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str(&part), value] }, name });
    }
    let (_, path) = write_root(member).expect("local_target found the root");
    let expr = self.written_at(Expr::Var(name.clone()), &path, value)?;
    Ok(Stmt::Set { name, expr })
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` a local collection: the local, which kind it is and the method.
  /// `held` after `call` changes it, a collection of `kind`.
  pub(crate) fn collection_written(&mut self, call: &js::CallExpression, kind: Kind, method: &str, held: Expr) -> Lowered<Expr> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.arguments.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes {} argument{}", i + 1, if i == 0 { "" } else { "s" })))?;
      this.expr(crate::syntax::arg_expr(a))
    };
    let written = match (kind, method) {
      (Kind::Map, "set") => Expr::Object(vec![Entry::Spread(held), Entry::Computed(Expr::Builtin { name: Builtin::MapKey, args: vec![arg(self, 0)?] }, arg(self, 1)?)]),
      (Kind::Map, "delete") => Expr::Builtin { name: Builtin::Omit, args: vec![held, Expr::Builtin { name: Builtin::MapKey, args: vec![arg(self, 0)?] }] },
      (Kind::Set, "add") => Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Array(vec![Entry::Spread(held), Entry::Item(arg(self, 0)?)])] },
      (Kind::Set, "delete") => {
        let item = self.temp();
        let kept = Expr::Compare(CompareOp::Ne, Box::new(Expr::Var(item.clone())), Box::new(arg(self, 0)?));
        Expr::Filter(Box::new(held), Box::new(Expr::Lambda { params: vec![item], body: Box::new(kept) }))
      }
      (Kind::Map, "clear") => Expr::Object(Vec::new()),
      (Kind::Set, "clear") => Expr::Array(Vec::new()),
      (Kind::Query, "set" | "append") => Expr::Builtin { name: if method == "set" { Builtin::FormSet } else { Builtin::FormAppend }, args: vec![held, Expr::Str(Box::new(arg(self, 0)?)), arg(self, 1)?] },
      (Kind::Query, "delete") => Expr::Builtin { name: Builtin::FormDelete, args: vec![held, Expr::Str(Box::new(arg(self, 0)?))] },
      (_, other) => return Err(self.residue(call.span, format!("`.{other}()` of a local `{}` as a statement", kind.name()))),
    };
    Ok(kind.made(written))
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` held in a cell: the cell's handle, the collection's kind and the method.
  fn cell_collection_write(&self, call: &js::CallExpression) -> Option<(Expr, Kind, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let handle = self.cells.iter().rev().find(|(name, _)| *name == obj.name.as_str()).map(|(_, handle)| handle.clone())?;
    let kind = self.kind_of(member.object())?;
    let method = self.member_name(member)?;
    matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear").then_some((handle, kind, method))
  }

  /// `url.searchParams.set(k, v)`, `.append(k, v)` or `.delete(k)` with `url` a local `URL`: the local and the method.
  fn local_url_search_write(&self, call: &js::CallExpression) -> Option<(String, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let Some(params) = (member.object()).as_member_expression() else { return None };
    let (js::Expression::Identifier(url), crate::syntax::MemberProp::Ident(prop)) = (params.object(), crate::syntax::prop(params)) else { return None };
    let method = self.member_name(member)?;
    (prop.name.as_str() == "searchParams" && self.kind_of(params.object()) == Some(Kind::Url) && self.is_local(url.name.as_str()) && matches!(method.as_str(), "set" | "append" | "delete")).then(|| (url.name.to_string(), method))
  }

  /// `m.set(k, v)` and its kin on a name bound nowhere here, which may be a module-level collection the set binds and retries with: the name and its span.
  fn unbound_collection_write(&self, call: &js::CallExpression) -> Option<(String, Span)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let method = self.member_name(member)?;
    if !matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") || self.parsed.ctxt(obj) == crate::Ctxt::Unresolved {
      return None;
    }
    let name = obj.name.as_str();
    let bound = self.scope.iter().any(|(n, _)| n == name) || self.globals.iter().any(|(n, _)| n == name) || self.cells.iter().any(|(n, _)| n == name);
    (!bound).then(|| (name.to_owned(), obj.span))
  }

  fn local_collection_write(&self, call: &js::CallExpression) -> Option<(String, Kind, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let kind = self.kind_of(member.object())?;
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") && self.is_local(obj.name.as_str())).then(|| (obj.name.to_string(), kind, method))
  }

  /// `x.push(...)` or `x.unshift(...)` with `x` a local: the local and the method.
  fn local_push(&self, call: &js::CallExpression) -> Option<(String, String)> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    let js::Expression::Identifier(obj) = member.object() else { return None };
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "push" | "unshift") && self.is_local(obj.name.as_str())).then(|| (obj.name.to_string(), method))
  }

  /// `session.extend(...)` or `ctx.session.extend(...)`.
  fn is_session_extend(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return false };
    if self.member_name(member).as_deref() != Some("extend") {
      return false;
    }
    match member.object() {
      js::Expression::Identifier(id) => self.root_of(id) == Some(Root::Session),
      via_expr if via_expr.is_member_expression() => {
let via = via_expr.as_member_expression().expect("matched as a member");
        let js::Expression::Identifier(id) = via.object() else { return false };
        self.root_of(id) == Some(Root::Ctx) && self.member_name(via).as_deref() == Some("session")
      }
      _ => false,
    }
  }

  fn is_ident_call(&self, call: &js::CallExpression, name: &str) -> bool {
    matches!(&call.callee, js::Expression::Identifier(id) if id.name.as_str() == name)
  }

  /// `session.key`, `session.key.sub`, `session.key[expr]` or the same
  /// through `ctx.session`. Returns the key and the path beneath it.
  fn session_target(&mut self, member: &js::MemberExpression) -> Lowered<(String, Vec<Expr>)> {
    let mut chain = vec![member];
    let mut current = member.object();
    let root_ident = loop {
      match current {
        js::Expression::ChainExpression(o) => match o.expression.as_member_expression() {
          Some(m) => {
            chain.push(m);
            current = m.object();
          }
          None => return Err(self.residue(o.expression.span(), "a call inside a session path")),
        },
        js::Expression::Identifier(id) => break id,
        m_expr if m_expr.is_member_expression() => {
          let m = m_expr.as_member_expression().expect("matched as a member");
          chain.push(m);
          current = m.object();
        }
        other => return Err(self.residue(other.span(), "a session write must start at `session`")),
      }
    };
    chain.reverse();
    let mut steps = chain.into_iter();
    let root = self.root_of(root_ident);
    let first = match root {
      Some(Root::Session) => steps.next(),
      Some(Root::Ctx) => {
        let via = steps.next().ok_or_else(|| self.residue(member.span(), "a write to `ctx` itself"))?;
        if self.member_name(via).as_deref() != Some("session") {
          return Err(self.residue(via.span(), "a write to something other than the session"));
        }
        steps.next()
      }
      _ => return Err(self.residue(root_ident.span, "a session write must start at `session`")),
    };
    let Some(first) = first else {
      return Err(self.residue(member.span(), "a write to the whole session; write one key"));
    };
    let key = self.member_name(first).ok_or_else(|| self.residue(first.span(), "the session key must be a name"))?;
    let mut path = Vec::new();
    for step in steps {
      path.push(match crate::syntax::prop(step) {
        crate::syntax::MemberProp::Ident(id) => Expr::Lit(Lit::Str(id.name.to_string())),
        crate::syntax::MemberProp::Computed(c) => self.expr(c)?,
        crate::syntax::MemberProp::Private(p) => return Err(self.residue(p.span, "a private name")),
      });
    }
    Ok((key, path))
  }

  fn member_name(&self, member: &js::MemberExpression) -> Option<String> {
    match crate::syntax::prop(member) {
      crate::syntax::MemberProp::Ident(id) => Some(id.name.to_string()),
      _ => None,
    }
  }

  /// Whether `call` is middleware's `next(...)` or `ctx.next(...)`.
  fn calls_next(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    match crate::meaning::bare(callee) {
      js::Expression::Identifier(id) => self.root_of(id) == Some(Root::Next),
      m_expr if m_expr.is_member_expression() => { let m = m_expr.as_member_expression().expect("matched as a member"); self.middleware && matches!((m.object(), crate::syntax::prop(m)), (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) if self.root_of(obj) == Some(Root::Ctx) && prop.name.as_str() == "next") },
      _ => false,
    }
  }

  /// `response.headers.get(name)` or `.has(name)` on a `Response`; `None` for any other call.
  fn response_header_read(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let Some((response, op, args)) = self.response_headers_call(call) else { return Ok(None) };
    if !matches!(op.as_str(), "get" | "has") {
      return Err(self.residue(call.span, format!("`headers.{op}` inside an expression; it is a statement")));
    }
    let [name] = args else { return Err(self.residue(call.span, format!("`headers.{op}` takes a header name"))) };
    let mut lowered = vec![self.expr(response)?, self.expr(crate::syntax::arg_expr(name))?];
    if op == "has" {
      lowered.push(Expr::lit_str("has"));
    }
    Ok(Some(Expr::Builtin { name: Builtin::ResponseHeaderGet, args: lowered }))
  }

  /// A call `response.headers.<op>(...)` with `op` one of a `Headers`' `get`, `has`, `set`, `append` and `delete`, on anything not known to be another kind: the response, the operation and the arguments. A value that is not a `Response` fails where it runs.
  fn response_headers_call<'c, 'e>(&self, call: &'c js::CallExpression<'e>) -> Option<(&'c js::Expression<'e>, String, &'c [js::Argument<'e>])> {
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let crate::syntax::MemberProp::Ident(op) = crate::syntax::prop(method) else { return None };
    let Some(headers) = (crate::meaning::bare(method.object())).as_member_expression() else { return None };
    let crate::syntax::MemberProp::Ident(field) = crate::syntax::prop(headers) else { return None };
    let known = matches!(op.name.as_str(), "get" | "has" | "set" | "append" | "delete");
    (field.name.as_str() == "headers" && known && !matches!(self.kind_of(headers.object()), Some(kind) if kind != Kind::Response)).then(|| (headers.object(), op.name.to_string(), call.arguments.as_slice()))
  }

  fn root_of(&self, id: &js::IdentifierReference) -> Option<Root> {
    let name = id.name.as_str();
    self.roots.iter().rev().find(|(n, _)| n == name).map(|(_, r)| r.clone())
  }

  /// `expr` as a value JavaScript's conversion to a primitive reads the same. A `Map` or a `Set` holds an object or an array at run time, which would convert as one, so its text is decided here where its kind is known.
  fn as_primitive(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    let text = match self.kind_of(expr) {
      Some(Kind::Map) => "[object Map]",
      Some(Kind::Set) => "[object Set]",
      _ => return self.expr(expr),
    };
    if matches!(crate::meaning::bare(expr), js::Expression::Identifier(_)) {
      return Ok(Expr::lit_str(text));
    }
    let held = self.expr(expr)?;
    let name = self.temp();
    Ok(Expr::Block(vec![Stmt::Let { name, expr: held }, Stmt::Return(Expr::lit_str(text))]))
  }

  /// Whether `expr` changes an object or an array a name reaches: a field written, a mutating method or `Object.assign`.
  pub(crate) fn object_write(&self, expr: &js::Expression) -> bool {
    let reached = |target: &js::Expression| -> bool {
      let root = match crate::meaning::bare(target) {
        js::Expression::Identifier(id) => id,
        member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match write_root(member) {
          Some((id, _)) => id,
          None => return false,
        } },
        _ => return false,
      };
      // A module constant, not module state: that one lowers to a cell or a store key.
      let global = || self.globals.iter().any(|(name, value)| *name == root.name.as_str() && matches!(value, Expr::Const(_) | Expr::Object(_) | Expr::Array(_)));
      let bound = self.scope.iter().any(|(name, _)| *name == root.name.as_str()) || global();
      bound && self.root_of(root).is_none() && !matches!(self.kind_of(target), Some(Kind::Url | Kind::Query | Kind::Response | Kind::Date | Kind::Scope))
    };
    // A write into what an expression answers, `f()[0].n = 2`, has no name to check.
    let written = |target: &js::Expression| -> bool {
      let mut at = crate::meaning::bare(target);
      while let Some(member) = (at).as_member_expression() {
        at = crate::meaning::bare(member.object());
      }
      match at {
        js::Expression::ThisExpression(_) => false,
        js::Expression::Identifier(id) if id.name.as_str() == crate::classes::THIS => false,
        js::Expression::Identifier(_) => reached(target),
        _ => true,
      }
    };
    match crate::meaning::bare(expr) {
      js::Expression::AssignmentExpression(assign) => assign.left.as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::UpdateExpression(update) => update.argument.as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => (crate::meaning::bare(&unary.argument)).as_member_expression().is_some_and(|m| written(m.object())),
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return false };
        match self.member_name(m).as_deref() {
          Some("push" | "unshift" | "pop" | "shift" | "reverse" | "sort") => reached(m.object()) && !matches!(self.kind_of(m.object()), Some(Kind::Map | Kind::Set)),
          Some("set" | "add" | "delete" | "clear") => reached(m.object()) && matches!(self.kind_of(m.object()), Some(Kind::Map | Kind::Set)),
          Some("assign") => matches!(crate::meaning::bare(m.object()), js::Expression::Identifier(id) if id.name.as_str() == "Object") && call.arguments.first().is_some_and(|first| !crate::syntax::is_spread(first) && reached(crate::syntax::arg_expr(first))),
          _ => false,
        }
      }
      _ => false,
    }
  }

  /// [`Lowerer::object_write`]'s `expr` as what it answers, its object held once and overwritten through the handle.
  pub(crate) fn heap_write(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    let overwrite = |target: &str, next: Expr| Stmt::Expr(Expr::Builtin { name: Builtin::Overwrite, args: vec![Expr::Var(target.to_owned()), next] });
    let held = |this: &mut Self, target: &js::Expression, lets: &mut Body| -> Lowered<String> {
      let value = this.expr(target)?;
      let name = this.temp();
      lets.push(Stmt::Let { name: name.clone(), expr: value });
      Ok(name)
    };
    let mut lets = Vec::new();
    let answer = match crate::meaning::bare(expr) {
      js::Expression::AssignmentExpression(assign) => {
        let Some(m) = assign.left.as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let b = syntax::Build::new(self.parsed.arena());
        let value = match syntax::compound(b, assign.operator, js::Expression::from(b.copy(m)), b.copy(&assign.right), assign.span) {
          None => self.expr(&assign.right)?,
          Some(computed) => self.expr(&computed)?,
        };
        let name = self.temp();
        lets.push(Stmt::Let { name: name.clone(), expr: value });
        let next = self.heap_set(&target, m, Expr::Var(name.clone()), &mut lets)?;
        lets.push(overwrite(&target, next));
        Expr::Var(name)
      }
      js::Expression::UpdateExpression(update) => {
        let Some(m) = update.argument.as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let b = self.build();
        let before = self.expr(&js::Expression::new_unary_expression(update.span, js::UnaryOperator::UnaryPlus, b.updated(&update.argument), &b.ast()))?;
        let old = self.temp();
        lets.push(Stmt::Let { name: old.clone(), expr: before });
        let op = if update.operator == js::UpdateOperator::Increment { ArithOp::Add } else { ArithOp::Sub };
        let after = Expr::Arith(op, Box::new(Expr::Var(old.clone())), Box::new(Expr::Lit(Lit::Float(1.0))));
        let name = self.temp();
        lets.push(Stmt::Let { name: name.clone(), expr: after });
        let next = self.heap_set(&target, m, Expr::Var(name.clone()), &mut lets)?;
        lets.push(overwrite(&target, next));
        Expr::Var(if update.prefix { name } else { old })
      }
      js::Expression::UnaryExpression(unary) => {
        let Some(m) = (crate::meaning::bare(&unary.argument)).as_member_expression() else { unreachable!() };
        let target = held(self, m.object(), &mut lets)?;
        let key = match self.member_name(m) {
          Some(field) => Expr::lit_str(&field),
          None => {
            let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(m) else { return Err(self.residue(m.span(), "`delete` of a private field")) };
            self.expr(key)?
          }
        };
        lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Omit, args: vec![Expr::Var(target.clone()), key] }));
        Expr::Lit(Lit::Bool(true))
      }
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { unreachable!() };
        let method = self.member_name(m).expect("object_write read the name");
        if method == "assign" {
          let target = held(self, &crate::syntax::arg_expr(&call.arguments[0]), &mut lets)?;
          let mut entries = vec![Entry::Spread(Expr::Var(target.clone()))];
          for source in &call.arguments[1..] {
            entries.push(Entry::Spread(self.expr(crate::syntax::arg_expr(source))?));
          }
          lets.push(overwrite(&target, Expr::Object(entries)));
          Expr::Var(target)
        } else {
          let kind = self.kind_of(m.object());
          let target = held(self, m.object(), &mut lets)?;
          let read = Expr::Var(target.clone());
          match (kind, method.as_str()) {
            (Some(kind @ (Kind::Map | Kind::Set)), _) => {
              let next = self.collection_written(call, kind, &method, read.clone())?;
              lets.push(overwrite(&target, next));
              if method == "delete" { Expr::Lit(Lit::Bool(true)) } else { read }
            }
            (_, "push" | "unshift") => {
              let mut items = Vec::with_capacity(call.arguments.len() + 1);
              for arg in &call.arguments {
                let value = self.expr(crate::syntax::arg_expr(arg))?;
                items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
              }
              if method == "push" { items.insert(0, Entry::Spread(read.clone())) } else { items.push(Entry::Spread(read.clone())) }
              lets.push(overwrite(&target, Expr::Array(items)));
              Expr::Length(Box::new(read))
            }
            (_, "pop" | "shift") => {
              let (at, kept) = if method == "pop" { (-1, vec![read.clone(), Expr::Lit(Lit::Int(0)), Expr::Lit(Lit::Int(-1))]) } else { (0, vec![read.clone(), Expr::Lit(Lit::Int(1))]) };
              let name = self.temp();
              lets.push(Stmt::Let { name: name.clone(), expr: Expr::Builtin { name: Builtin::At, args: vec![read.clone(), Expr::Lit(Lit::Int(at))] } });
              lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Slice, args: kept }));
              Expr::Var(name)
            }
            (_, "reverse") => {
              lets.push(overwrite(&target, Expr::Builtin { name: Builtin::Reverse, args: vec![read.clone()] }));
              read
            }
            _ => {
              let by = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
                None => Expr::Lit(Lit::Null),
                Some(js::Expression::ArrowFunctionExpression(arrow)) => self.lambda(arrow)?,
                Some(other) => self.expr(other)?,
              };
              lets.push(overwrite(&target, Expr::Sort(Box::new(read.clone()), Box::new(by))));
              read
            }
          }
        }
      }
      _ => unreachable!("object_write answers for these"),
    };
    lets.push(Stmt::Return(answer));
    Ok(Expr::Block(lets))
  }

  fn heap_set(&mut self, target: &str, member: &js::MemberExpression, value: Expr, lets: &mut Body) -> Lowered<Expr> {
    if let Some(field) = self.member_name(member) {
      return Ok(Expr::Object(vec![Entry::Spread(Expr::Var(target.to_owned())), Entry::Field(field, value)]));
    }
    let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(member) else { return Err(self.residue(member.span(), "a write through a private field")) };
    let key = self.expr(key)?;
    let name = self.temp();
    lets.push(Stmt::Let { name: name.clone(), expr: key });
    Ok(Expr::Builtin { name: Builtin::SetAt, args: vec![Expr::Var(target.to_owned()), Expr::Var(name), value] })
  }

  pub(crate) fn expr(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    if !self.premade.is_empty() {
      let span = expr.span();
      if let Some(made) = self.premade.get(&(crate::lo(span), crate::hi(span))) {
        return Ok(made.clone());
      }
    }
    if self.object_write(expr) {
      if heap_pass() {
        return self.heap_write(expr);
      }
      want_heap();
    }
    if heap_pass() {
      if let js::Expression::CallExpression(call) = crate::meaning::bare(expr) {
        {
      let callee = &call.callee;
          if let Some(m) = (crate::meaning::bare(callee)).as_member_expression() {
            if self.member_name(m).as_deref() == Some("forEach") && call.arguments.len() == 1 {
              let over = self.expr(m.object())?;
              let f = self.expr(&crate::syntax::arg_expr(&call.arguments[0]))?;
              let ran = self.temp();
              return Ok(Expr::Block(vec![Stmt::Let { name: ran, expr: Expr::Map(Box::new(over), Box::new(f)) }, Stmt::Return(Expr::Lit(Lit::Undefined))]));
            }
          }
        }
      }
    }
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.expr(&p.expression),
      js::Expression::AwaitExpression(a) => self.expr(&a.argument),
      js::Expression::CallExpression(_) if self.promise_all(expr).is_some() => {
        let list = self.promise_all(expr).expect("checked by the guard");
        self.expr(list)
      }
      js::Expression::TSAsExpression(a) => self.expr(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.expr(&a.expression),
      js::Expression::TSSatisfiesExpression(a) => self.expr(&a.expression),
      js::Expression::TSTypeAssertion(a) => self.expr(&a.expression),

      js::Expression::Identifier(id) => self.ident(id),
      lit if lit.is_literal() => self.lit(lit),
      js::Expression::TaggedTemplateExpression(tagged) if (crate::meaning::bare(&tagged.tag)).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "String" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved) && self.member_name(m).as_deref() == Some("raw")) => {
        // `String.raw` writes each piece as it stands in the source, escapes and all.
        let mut parts = Vec::new();
        for (i, quasi) in tagged.quasi.quasis.iter().enumerate() {
          if !quasi.value.raw.is_empty() {
            parts.push(Expr::Lit(Lit::Str(quasi.value.raw.to_string())));
          }
          if let Some(e) = tagged.quasi.expressions.get(i) {
            parts.push(self.expr(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expression::TemplateLiteral(tpl) => {
        let mut parts = Vec::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
          let text = quasi.value.cooked.map(crate::lossy).unwrap_or_else(|| quasi.value.raw.to_string());
          if !text.is_empty() {
            parts.push(Expr::Lit(Lit::Str(text)));
          }
          if let Some(e) = tpl.expressions.get(i) {
            parts.push(self.as_primitive(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expression::ObjectExpression(obj) => {
        let mut entries = Vec::new();
        for prop in &obj.properties {
          entries.push(match prop {
            js::ObjectPropertyKind::SpreadProperty(s) => Entry::Spread(self.expr(&s.argument)?),
            js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
              crate::syntax::Prop::Shorthand(id) => Entry::Field(id.name.to_string(), self.ident(id)?),
              crate::syntax::Prop::KeyValue(kv) => match kv.key.as_expression() {
                Some(key) if kv.computed => Entry::Computed(self.as_primitive(key)?, self.expr(&kv.value)?),
                _ => {
                  let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a numeric property name"))?;
                  Entry::Field(key, self.expr(&kv.value)?)
                }
              },
              other => return Err(self.residue(other.span(), "an accessor in an object literal")),
            },
          });
        }
        Ok(Expr::Object(entries))
      }
      js::Expression::ArrayExpression(arr) => {
        let mut entries = Vec::new();
        for elem in &arr.elements {
          let (spread, item) = match elem {
            js::ArrayExpressionElement::Elision(_) => continue,
            js::ArrayExpressionElement::SpreadElement(s) => (true, &s.argument),
            other => (false, other.to_expression()),
          };
          if let (true, Some(length)) = (spread, self.array_of_length(item)) {
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            entries.push(Entry::Spread(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$hole".to_owned()], body: Box::new(Expr::Lit(Lit::Undefined)) }))));
            continue;
          }
          let value = self.expr(item)?;
          entries.push(match spread {
            true if self.kind_of(item) == Some(Kind::Map) => Entry::Spread(Expr::Entries(Box::new(value))),
            true => Entry::Spread(value),
            false => Entry::Item(value),
          });
        }
        Ok(Expr::Array(entries))
      }
      js::Expression::UnaryExpression(u) => match u.operator {
        js::UnaryOperator::LogicalNot => Ok(Expr::Not(Box::new(self.expr(&u.argument)?))),
        js::UnaryOperator::UnaryNegation => match &u.argument {
          js::Expression::NumericLiteral(n) => Ok(Expr::Lit(Lit::Float(-n.value))),
          js::Expression::BigIntLiteral(b) => Ok(Expr::Lit(Lit::Int(-self.bigint(b)?))),
          other => Ok(Expr::Arith(ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(self.expr(other)?))),
        },
        js::UnaryOperator::UnaryPlus => Ok(Expr::Num(Box::new(self.as_primitive(&u.argument)?))),
        js::UnaryOperator::BitwiseNot if self.kind_of(&u.argument) == Some(Kind::BigInt) => Err(self.residue(u.span, "the `~` operator on a `bigint`")),
        js::UnaryOperator::BitwiseNot => Ok(Expr::Arith(ArithOp::BitXor, Box::new(self.expr(&u.argument)?), Box::new(Expr::Lit(Lit::Float(-1.0))))),
        js::UnaryOperator::Delete => Err(self.residue(u.span, "`delete` inside an expression")),
        js::UnaryOperator::Typeof => Ok(match self.kind_of(&u.argument) {
          Some(Kind::BigInt) => Expr::lit_str("bigint"),
          Some(Kind::Map | Kind::Set | Kind::Date | Kind::Query | Kind::Url | Kind::Response | Kind::Scope) => Expr::lit_str("object"),
          Some(Kind::Browser) | None => Expr::Builtin { name: Builtin::TypeOf, args: vec![self.expr(&u.argument)?] },
        }),
        other => Err(self.residue(u.span, format!("the `{}` operator", other.as_str()))),
      },
      js::Expression::BinaryExpression(bin) if bin.operator == js::BinaryOperator::Instanceof => self.instance_of(bin),
      js::Expression::BinaryExpression(bin) if is_bitwise(bin.operator) && (self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt)) => Err(self.residue(bin.span, format!("the `{}` operator on a `bigint`", bin.operator.as_str()))),
      js::Expression::LogicalExpression(logic) => {
        let l = Box::new(self.expr(&logic.left)?);
        let r = Box::new(self.expr(&logic.right)?);
        Ok(match logic.operator {
          js::LogicalOperator::And => Expr::Logic(LogicOp::And, l, r),
          js::LogicalOperator::Or => Expr::Logic(LogicOp::Or, l, r),
          js::LogicalOperator::Coalesce => Expr::Coalesce(l, r),
        })
      }
      js::Expression::BinaryExpression(bin) => {
        let collection = |this: &Self, e: &js::Expression| matches!(this.kind_of(e), Some(Kind::Map | Kind::Set));
        let two_objects = matches!(bin.operator, js::BinaryOperator::Equality | js::BinaryOperator::Inequality) && collection(self, &bin.left) && collection(self, &bin.right);
        let converts = !two_objects && matches!(bin.operator, js::BinaryOperator::Addition | js::BinaryOperator::Subtraction | js::BinaryOperator::Multiplication | js::BinaryOperator::Division | js::BinaryOperator::Remainder | js::BinaryOperator::Exponential | js::BinaryOperator::Equality | js::BinaryOperator::Inequality | js::BinaryOperator::LessThan | js::BinaryOperator::LessEqualThan | js::BinaryOperator::GreaterThan | js::BinaryOperator::GreaterEqualThan);
        let l = Box::new(if converts { self.as_primitive(&bin.left)? } else { self.expr(&bin.left)? });
        let r = Box::new(if converts { self.as_primitive(&bin.right)? } else { self.expr(&bin.right)? });
        Ok(match bin.operator {
          js::BinaryOperator::Addition => Expr::Arith(ArithOp::Add, l, r),
          js::BinaryOperator::Subtraction => Expr::Arith(ArithOp::Sub, l, r),
          js::BinaryOperator::Multiplication => Expr::Arith(ArithOp::Mul, l, r),
          js::BinaryOperator::Division if self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt) => Expr::Arith(ArithOp::Quot, l, r),
          js::BinaryOperator::Division => Expr::Arith(ArithOp::Div, l, r),
          js::BinaryOperator::Remainder => Expr::Arith(ArithOp::Rem, l, r),
          js::BinaryOperator::Exponential => Expr::Arith(ArithOp::Pow, l, r),
          js::BinaryOperator::Equality | js::BinaryOperator::Inequality if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) || matches!(*l, Expr::Lit(Lit::Null | Lit::Undefined)) => {
            let other = if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) { l } else { r };
            let nullish = Expr::Builtin { name: Builtin::IsNullish, args: vec![*other] };
            if bin.operator == js::BinaryOperator::Equality { nullish } else { Expr::Not(Box::new(nullish)) }
          }
          js::BinaryOperator::Equality => Expr::Builtin { name: Builtin::LooseEq, args: vec![*l, *r] },
          js::BinaryOperator::Inequality => Expr::Not(Box::new(Expr::Builtin { name: Builtin::LooseEq, args: vec![*l, *r] })),
          js::BinaryOperator::StrictEquality => Expr::Compare(CompareOp::Eq, l, r),
          js::BinaryOperator::StrictInequality => Expr::Compare(CompareOp::Ne, l, r),
          js::BinaryOperator::LessThan => Expr::Compare(CompareOp::Lt, l, r),
          js::BinaryOperator::LessEqualThan => Expr::Compare(CompareOp::Le, l, r),
          js::BinaryOperator::GreaterThan => Expr::Compare(CompareOp::Gt, l, r),
          js::BinaryOperator::GreaterEqualThan => Expr::Compare(CompareOp::Ge, l, r),
          js::BinaryOperator::In => Expr::Builtin { name: Builtin::HasKey, args: vec![*r, *l] },
          js::BinaryOperator::BitwiseAnd => Expr::Arith(ArithOp::BitAnd, l, r),
          js::BinaryOperator::BitwiseOR => Expr::Arith(ArithOp::BitOr, l, r),
          js::BinaryOperator::BitwiseXOR => Expr::Arith(ArithOp::BitXor, l, r),
          js::BinaryOperator::ShiftLeft => Expr::Arith(ArithOp::Shl, l, r),
          js::BinaryOperator::ShiftRight => Expr::Arith(ArithOp::Shr, l, r),
          js::BinaryOperator::ShiftRightZeroFill => Expr::Arith(ArithOp::UShr, l, r),
          other => return Err(self.residue(bin.span, format!("the `{}` operator", other.as_str()))),
        })
      }
      js::Expression::ConditionalExpression(c) => Ok(Expr::Ternary(
        Box::new(self.expr(&c.test)?),
        Box::new(self.expr(&c.consequent)?),
        Box::new(self.expr(&c.alternate)?),
      )),
      member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); self.member(member) },
      js::Expression::CallExpression(call) => self.call(call),
      js::Expression::ArrowFunctionExpression(arrow) => self.lambda(arrow),
      js::Expression::ChainExpression(o) => match &o.expression {
        js::ChainElement::CallExpression(call) if call.optional => Err(self.residue_with(call.span, "an optional call of a function value", "write `f ? f(x) : null` or call a helper the build can follow")),
        js::ChainElement::CallExpression(call) => self.optional_call(call),
        js::ChainElement::TSNonNullExpression(e) => self.expr(&e.expression),
        member => self.member(member.as_member_expression().expect("a chain element is a call, a `!` or a member")),
      },
      js::Expression::NewExpression(n) => self.new_expr(n),
      js::Expression::FunctionExpression(f) => self.function_expr(f),
      js::Expression::AssignmentExpression(a) if self.cell_of_target(&a.left).is_some() => match self.effect(expr)? {
        Stmt::Expr(write) => Ok(write),
        _ => Err(self.residue(a.span, "an assignment inside an expression")),
      },
      js::Expression::AssignmentExpression(a) => Err(self.residue(a.span, "an assignment inside an expression")),
      js::Expression::ThisExpression(_) if self.scope.iter().any(|(n, _)| n == "this") => Ok(self.scope.iter().rev().find(|(n, _)| n == "this").map(|(_, e)| e.clone()).expect("checked")),
      other => {
        let (what, hint) = describe_expr(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  /// A residue for a value only the browser knows, marked so a consumer that can leave it to the browser lowers null in its place.
  pub(crate) fn browser_only(&mut self, span: Span, reason: impl Into<String>) -> Residue {
    let reason = reason.into();
    self.browser_value = Some(reason.clone());
    self.residue_with(span, reason, "as text under an element the browser fills it in when the component hydrates; anywhere else compute it on the server, from `ctx.now` in the loader or a prop")
  }

  pub(crate) fn ident(&mut self, id: &js::IdentifierReference) -> Lowered<Expr> {
    let name = id.name.as_str();
    if let Some((_, bound)) = self.scope.iter().rev().find(|(n, _)| n == name) {
      if self.kinds.iter().rev().any(|(n, held, kind)| n == name && held == bound && *kind == Kind::Browser) {
        return Err(self.browser_only(id.span, format!("`{name}`, a value only the browser knows")));
      }
      return Ok(bound.clone());
    }
    if let Some((_, bound)) = self.globals.iter().rev().find(|(n, _)| n == name) {
      return Ok(bound.clone());
    }
    if self.reaches(Reach::Process) && self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) == Some(&meaning::Meaning::Library(meaning::Construct::Suspend)) {
      return Ok(Expr::Object(vec![Entry::Field(crate::terminal::PRIMITIVE.to_owned(), Expr::lit_str("suspendTerminal"))]));
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::NamedKey(key))) = self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) {
      return Ok(Expr::lit_str(*key));
    }
    match self.root_of(id) {
      Some(Root::Input) => Ok(Expr::Input),
      Some(Root::Now) => Ok(Expr::Now),
      Some(Root::Locale) => Ok(Expr::Locale),
      Some(Root::Locales) => Ok(Expr::Locales),
      Some(Root::Path) => Ok(Expr::Path),
      Some(Root::Host) => Ok(Expr::Host),
      Some(Root::Origin) => Ok(Expr::Origin),
      Some(Root::Address) => Ok(Expr::Address),
      Some(Root::Identity) => Ok(Expr::Identity(Vec::new())),
      Some(Root::Params) => Ok(Expr::ParamsAll),
      Some(Root::Query) => Ok(Expr::QueryAll),
      Some(Root::Session | Root::Config) => Err(self.residue(id.span, format!("`{name}` as a whole; read one of its fields"))),
      Some(Root::Services) => Err(self.residue(id.span, "`services` as a value; call a method on it")),
      Some(Root::Native) => Err(self.residue(id.span, "`native` as a value; call a method on it")),
      Some(Root::Ctx) => Err(self.residue(id.span, "`ctx` as a value; read one of its fields")),
      Some(Root::Next) => Err(self.residue(id.span, "`next` as a value; call it")),
      None => match name {
        "undefined" => Ok(Expr::Lit(Lit::Undefined)),
        "Infinity" => Ok(Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(1.0))), Box::new(Expr::Lit(Lit::Float(0.0))))),
        "NaN" => Ok(Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(Expr::Lit(Lit::Float(0.0))))),
        _ if self.browser_functions.iter().any(|f| f == name) => Ok(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)),
        _ if self.after_body && self.declared_below.iter().any(|(declared, _)| declared == name) => {
          let (_, bound) = self.declared_below.iter().rev().find(|(declared, _)| declared == name).cloned().expect("checked");
          self.read_below.push((bound.clone(), id.span));
          Ok(Expr::Var(bound))
        }
        "process" if self.parsed.ctxt(id) == crate::Ctxt::Unresolved => Err(self.residue_with(id.span, "`process`, which is not FSR's", NOT_FSR)),
        _ => {
          self.unbound = Some(name.to_owned());
          match crate::component::find_import(self.parsed, name) {
            Some((source, _)) if node_module(&source) => Err(self.residue_with(id.span, format!("`{name}` from `{source}`, which is not FSR's"), NOT_FSR)),
            Some((source, export)) => match meaning::module_absent(&source, &export) {
              Some(why) => Err(self.residue(id.span, why)),
              None => Err(self.residue(id.span, format!("`{name}` comes from `{source}`, which the build cannot follow"))),
            },
            None => Err(self.residue(id.span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body"))),
          }
        }
      },
    }
  }

  /// A literal: a string, a number, a bigint, a boolean, `null` or a regular expression.
  fn lit(&self, lit: &js::Expression) -> Lowered<Expr> {
    Ok(Expr::Lit(match lit {
      js::Expression::StringLiteral(s) => Lit::Str(crate::lossy(s.value)),
      js::Expression::NumericLiteral(n) => Lit::Float(n.value),
      js::Expression::BigIntLiteral(b) => Lit::Int(self.bigint(b)?),
      js::Expression::BooleanLiteral(b) => Lit::Bool(b.value),
      js::Expression::NullLiteral(_) => Lit::Null,
      js::Expression::RegExpLiteral(re) => {
        let raw = re.raw.as_ref().map(|raw| raw.as_str()).unwrap_or_default();
        let flags = raw.rsplit_once('/').map(|(_, flags)| flags).unwrap_or_default();
        return regex_value(self, re.regex.pattern.text.as_str(), flags, re.span);
      }
      other => return Err(self.residue(other.span(), "a literal the build does not read")),
    }))
  }

  fn bigint(&self, b: &js::BigIntLiteral) -> Lowered<i128> {
    b.value
      .to_string()
      .parse::<i128>()
      .map_err(|_| self.residue(b.span, "a bigint literal outside 128 bits"))
  }

  /// A member read. Context roots become reads; anything else is a field.
  fn member(&mut self, member: &js::MemberExpression) -> Lowered<Expr> {
    if let Some(compare) = self.collator_compare(member)? {
      return Ok(compare);
    }
    if let (js::Expression::Identifier(id), Some(key)) = (member.object(), self.member_name(member)) {
      if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.meanings.as_ref().and_then(|m| m.of_ident(id)) == Some(&crate::meaning::Meaning::Config(None)) {
        return Ok(Expr::Config(key));
      }
    }
    if let Some(read) = self.class_static_read(member)? {
      return Ok(read);
    }

    let prop = |this: &mut Self| -> Lowered<Result<String, Expr>> {
      Ok(match crate::syntax::prop(member) {
        crate::syntax::MemberProp::Ident(id) => Ok(id.name.to_string()),
        crate::syntax::MemberProp::Computed(c) => match c {
          js::Expression::StringLiteral(s) => Ok(crate::lossy(s.value)),
          other => Err(this.expr(other)?),
        },
        crate::syntax::MemberProp::Private(p) => return Err(this.residue(p.span, "a private name")),
      })
    };

    if let js::Expression::Identifier(id) = member.object() {
      if id.name.as_str() == "Math" && !self.scope.iter().any(|(n, _)| n == "Math") && !self.globals.iter().any(|(n, _)| n == "Math") {
        if let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) {
          use std::f64::consts;
          let value = match name.name.as_str() {
            "PI" => Some(consts::PI),
            "E" => Some(consts::E),
            "LN2" => Some(consts::LN_2),
            "LN10" => Some(consts::LN_10),
            "LOG2E" => Some(consts::LOG2_E),
            "LOG10E" => Some(consts::LOG10_E),
            "SQRT2" => Some(consts::SQRT_2),
            "SQRT1_2" => Some(consts::FRAC_1_SQRT_2),
            _ => None,
          };
          if let Some(value) = value {
            return Ok(Expr::Lit(Lit::Float(value)));
          }
        }
      }
      if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
        match self.root_of(id) {
          Some(Root::Params) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed param name")) };
            return Ok(Expr::Param(name));
          }
          Some(Root::Query) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed query key")) };
            return Ok(Expr::Query(name));
          }
          Some(Root::Session) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed session key")) };
            return Ok(self.session_read(name));
          }
          Some(Root::Identity) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed identity field")) };
            return Ok(Expr::Identity(vec![name]));
          }
          Some(Root::Config) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed config key")) };
            return Ok(Expr::Config(name));
          }
          Some(Root::Ctx) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed context field")) };
            return match name.as_str() {
              "input" => Ok(Expr::Input),
              "now" => Ok(Expr::Now),
              "locale" => Ok(Expr::Locale),
              "locales" => Ok(Expr::Locales),
              "path" => Ok(Expr::Path),
              "host" => Ok(Expr::Host),
              "origin" => Ok(Expr::Origin),
              "address" => Ok(Expr::Address),
              "identity" => Ok(Expr::Identity(Vec::new())),
              "params" | "query" | "session" | "config" => Err(self.residue(member.span(), format!("`ctx.{name}` as a whole; read one of its fields"))),
              "services" => Err(self.residue(member.span(), "`ctx.services` as a value; call a method on it")),
              "native" => Err(self.residue(member.span(), "`ctx.native` as a value; call a method on it")),
              _ => Err(self.residue(member.span(), format!("`{name}` is not a field of the context"))),
            };
          }
          Some(Root::Services) => return Err(self.residue(member.span(), "a service as a value; call a method on it")),
          Some(Root::Native) => return Err(self.residue(member.span(), "a native module as a value; call a method on it")),
          _ => {}
        }
      }
    }

    if let Some(inner) = (member.object()).as_member_expression() {
      if let js::Expression::Identifier(id) = inner.object() {
        if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
          if let Some(Root::Ctx) = self.root_of(id) {
            let via = self.member_name(inner);
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed context field")) };
            match via.as_deref() {
              Some("params") => return Ok(Expr::Param(name)),
              Some("query") => return Ok(Expr::Query(name)),
              Some("session") => return Ok(self.session_read(name)),
              Some("identity") => return Ok(Expr::Identity(vec![name])),
              _ => {}
            }
          }
          if let Some(Root::Identity) = self.root_of(id) {
            let Some(first) = self.member_name(inner) else { return Err(self.residue(inner.span(), "a computed identity field")) };
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span(), "a computed identity field")) };
            return Ok(Expr::Identity(vec![first, name]));
          }
        }
      }
    }

    if self.member_kind(member) == Some(Kind::Browser) {
      let shown = match (member.object(), crate::syntax::prop(member)) {
        (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(prop)) => format!("`{}.{}`", obj.name, prop.name),
        _ => "a member".to_owned(),
      };
      return Err(self.browser_only(member.span(), format!("{shown}, a value only the browser knows")));
    }
    let collection = self.kind_of(member.object());
    let target = self.expr(member.object())?;
    if collection == Some(Kind::Response) {
      return match prop(self)? {
        Ok(name) if matches!(name.as_str(), "status" | "ok") => Ok(Expr::Builtin { name: Builtin::ResponseMember, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span(), format!("`.{name}` of a `Response`; `status`, `ok` and `headers.get` are read"))),
        Err(_) => Err(self.residue(member.span(), "a computed member of a `Response`")),
      };
    }
    if collection == Some(Kind::Url) {
      return match prop(self)? {
        Ok(name) if name == "href" => Ok(target),
        Ok(name) if name == "searchParams" => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str("search")] }),
        Ok(name) if URL_PARTS.contains(&name.as_str()) => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span(), format!("`.{name}` of a `URL`"))),
        Err(_) => Err(self.residue(member.span(), "a computed part of a `URL`")),
      };
    }
    match prop(self)? {
      Ok(name) if name == "length" => Ok(Expr::Length(Box::new(target))),
      Ok(name) if name == "size" && collection.is_some_and(|kind| kind != Kind::Query) => Ok(Expr::Length(Box::new(target))),
      Ok(name) if collection.is_none() && self.may_be_getter(&name) && !crate::classes::plain_root(&target) => {
        let t = self.temp();
        Ok(crate::classes::getter_dispatch(t, target, &name))
      }
      Ok(name) => Ok(Expr::Field(Box::new(target), name)),
      Err(key) => Ok(Expr::Index(Box::new(target), Box::new(key))),
    }
  }

  /// `use(X)` or `useContext(X)`, however the call names them, with `X` a context: the nearest provider's value, else the default. `None` for any other call.
  fn context_read(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    let Some(meanings) = self.meanings.clone() else { return Ok(None) };
    let Some(hook) = meanings.of_expr(callee).and_then(|meant| meant.hook()) else { return Ok(None) };
    if hook != "use" && hook != "useContext" {
      return Ok(None);
    }
    match call.arguments.first().and_then(|a| meanings.of_expr(crate::syntax::arg_expr(a))) {
      Some(meaning::Meaning::Context { id, default, .. }) => Ok(Some(Expr::Coalesce(Box::new(Expr::Context(id)), Box::new(default)))),
      _ => Err(match hook {
        "use" => self.residue(call.span, "`use` of something other than a `createContext` value; a promise is awaited in the browser"),
        _ => self.residue(call.span, "`useContext` of something the build cannot follow to a `createContext`"),
      }),
    }
  }

  /// The length of `Array(n)` or `new Array(n)` with `Array` the global.
  fn array_of_length<'s, 'e>(&self, expr: &'s js::Expression<'e>) -> Option<&'s js::Expression<'e>> {
    let (callee, args): (&'s js::Expression<'e>, &'s [js::Argument<'e>]) = match strip_parens(expr) {
      js::Expression::CallExpression(call) => (&call.callee, &call.arguments[..]),
      js::Expression::NewExpression(made) => (&made.callee, &made.arguments[..]),
      _ => return None,
    };
    let js::Expression::Identifier(id) = strip_parens(callee) else { return None };
    let global = self.meanings.as_ref().is_none_or(|meanings| meanings.ctxt(id) == Ctxt::Unresolved);
    match args {
      [length] if id.name.as_str() == "Array" && global && !crate::syntax::is_spread(length) => Some(crate::syntax::arg_expr(length)),
      _ => None,
    }
  }

  /// What React's server renderer returns for a hook whose value does not wait on the browser: `useDeferredValue`'s initial value when given and its value otherwise, `useFormStatus`'s status of no form pending, `useLinkStatus`'s of no link pending and `useSyncExternalStore`'s server snapshot.
  pub(crate) fn server_hook_value(&mut self, hook: &str, call: &js::CallExpression) -> Lowered<Expr> {
    match hook {
      "useDeferredValue" => match call.arguments.get(1).or(call.arguments.first()) {
        Some(a) => self.expr(crate::syntax::arg_expr(a)),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      "useFormStatus" => Ok(Expr::Object(["pending", "data", "method", "action"].map(|key| Entry::Field(key.to_owned(), Expr::Lit(if key == "pending" { Lit::Bool(false) } else { Lit::Null }))).to_vec())),
      "useLinkStatus" => Ok(Expr::Object(vec![Entry::Field("pending".to_owned(), Expr::Lit(Lit::Bool(false)))])),
      _ => match call.arguments.get(2) {
        Some(server) => Ok(Expr::Apply { f: Box::new(self.expr(crate::syntax::arg_expr(server))?), args: Vec::new() }),
        None => Err(self.residue(call.span, "`useSyncExternalStore` without `getServerSnapshot`, which React's server renderer refuses too")),
      },
    }
  }

  pub(crate) fn call(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    if let Some(ok) = self.settled_promise(call) {
      let value = match call.arguments.as_slice() {
        [] => Expr::Lit(Lit::Undefined),
        [value] if !crate::syntax::is_spread(value) => self.expr(crate::syntax::arg_expr(value))?,
        _ => return Err(self.residue(call.span, "`Promise.resolve` or `Promise.reject` with more than one value")),
      };
      return Ok(self.settled(ok, value));
    }
    if self.calls_next(call) {
      let request = match call.arguments.as_slice() {
        [] => Expr::Lit(Lit::Undefined),
        [request] => self.expr(crate::syntax::arg_expr(request))?,
        _ => return Err(self.residue(call.span, "`next` takes one request")),
      };
      return Ok(Expr::Next(Box::new(request)));
    }
    if let Some(read) = self.response_header_read(call)? {
      return Ok(read);
    }
    if let (Some(reader), callee) = (&self.reader, &call.callee) {
      if matches!(callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == Ctxt::Local(*reader)) {
        let named = call.arguments.first().ok_or_else(|| self.residue(call.span, "a compute's `read` takes the source key"))?;
        return match self.expr(&crate::syntax::arg_expr(&named))? {
          Expr::Lit(Lit::Str(key)) => Ok(Expr::Store(key)),
          _ => Err(self.residue(crate::syntax::arg_expr(&named).span(), "a source key the build cannot read")),
        };
      }
    }
    if let Some(suspended) = self.suspend_call(call)? {
      return Ok(suspended);
    }
    if let (true, callee, Some(js::Expression::StringLiteral(id))) = (self.reaches(Reach::Process), &call.callee, call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a)))) {
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClientAction)) {
        return Ok(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::ACT_REF.to_owned(), Expr::lit_str(crate::lossy(id.value)))]));
      }
    }
    if self.reaches(Reach::Process) && self.spawns(call) {
      return self.spawn_value(call);
    }
    if let Some(chained) = self.then_call(call)? {
      return Ok(chained);
    }
    {
      let callee = &call.callee;
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::Emit)) {
        let mut args = Vec::with_capacity(2);
        for a in call.arguments.iter().take(2) {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        return Ok(Expr::Builtin { name: Builtin::Emit, args });
      }
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.store_call()) == Some("set") {
        let [key, value] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`set` takes a key and a value")) };
        let Expr::Lit(Lit::Str(key)) = self.expr(crate::syntax::arg_expr(key))? else { return Err(self.residue(crate::syntax::arg_expr(key).span(), "a store key that is not a string the build can read")) };
        return Ok(Expr::WriteStore(key, Box::new(self.expr(crate::syntax::arg_expr(value))?)));
      }
    }
    let callee = &call.callee;
    if let Some(called) = self.class_call(call, callee)? {
      return Ok(called);
    }
    if let Some(called) = self.instance_call(call, callee)? {
      return Ok(called);
    }
    if let Some(read) = self.context_read(call, callee)? {
      return Ok(read);
    }
    if let Some(hook @ ("useDeferredValue" | "useFormStatus" | "useSyncExternalStore")) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.hook()) {
      return self.server_hook_value(hook, call);
    }
    if let Some(module) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.std()).filter(|module| *module != "native") {
      if module != "t" {
        return Err(self.residue(callee.span(), format!("`{module}` from the standard library is a module; call one of its members")));
      }
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(self.candidate(call.span, Expr::Ext { module: "i18n".to_owned(), name: "t".to_owned(), args }));
    }
    let element_builtin = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::CloneElement)) => Some(Builtin::CloneElement),
      Some(meaning::Meaning::Library(meaning::Construct::IsValidElement)) => Some(Builtin::IsElement),
      _ => None,
    };
    if let Some(name) = element_builtin {
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(Expr::Builtin { name, args });
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::SvelteStore(store))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      return self.svelte_store(store, call);
    }
    if let Some(called) = self.std_call(call, callee)? {
      return Ok(called);
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::Extension(key))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      let (module, member) = key.split_once('.').expect("an extension is `module.member`");
      let reach = standard_reach(module, member).expect("a package maps to a standard member");
      if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
        return Err(self.residue(call.span, format!("a call to `{key}`, which {needs}")));
      }
      self.check_reach(call.span, key, reach)?;
      let mut args = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        if crate::syntax::is_spread(a) {
          return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("a spread into `{key}`")));
        }
        args.push(self.expr(crate::syntax::arg_expr(a))?);
      }
      return Ok(Expr::Ext { module: module.to_owned(), name: member.to_owned(), args });
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClassNames)) {
      let mut items = Vec::with_capacity(call.arguments.len());
      for a in &call.arguments {
        items.push(Entry::Item(self.expr(crate::syntax::arg_expr(a))?));
      }
      return Ok(Expr::Builtin { name: Builtin::ClassNames, args: vec![Expr::Array(items)] });
    }
    if let Some(member) = (callee).as_member_expression() {
      if let Some(module) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(member.object())).and_then(|meant| meant.std()) {
        let method = self.member_name(member).ok_or_else(|| self.residue(member.span(), "a computed method name"))?;
        let key = format!("{module}.{method}");
        let reach = standard_reach(module, &method).ok_or_else(|| self.residue(member.span(), format!("`{key}` is not a member of the standard library")))?;
        self.check_reach(call.span, &key, reach)?;
        let mut args = Vec::with_capacity(call.arguments.len());
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        let expr = Expr::Ext { module: module.to_owned(), name: method, args };
        return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
      }
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::StoreKey)) {
      let named = call.arguments.first().ok_or_else(|| self.residue(call.span, "`key` takes the key's name"))?;
      return self.expr(&crate::syntax::arg_expr(&named));
    }
    let math = (callee).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "Math"));
    let applied = matches!(callee, js::Expression::Identifier(id) if self.scope.iter().any(|(n, _)| *n == id.name.as_str()) || self.local_fns.iter().any(|(n, _)| *n == id.name.as_str()) || self.globals.iter().any(|(n, _)| *n == id.name.as_str()));
    for arg in &call.arguments {
      if crate::syntax::is_spread(arg) && !math && !applied {
        if let js::Expression::Identifier(id) = callee {
          self.unbound = Some(id.name.to_string());
        }
        return Err(self.residue(crate::syntax::arg_expr(arg).span(), "a spread argument"));
      }
    }

    if let js::Expression::Identifier(id) = callee {
      let name = id.name.as_str();
      if !self.scope.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() {
        if let Some(function) = self.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| *f) {
          let f = Box::new(self.function_expr(&function)?);
          return self.applied(f, call);
        }
        if let Some((_, f)) = self.globals.iter().rev().find(|(n, _)| n == name) {
          let f = Box::new(f.clone());
          if let Expr::Ext { module, name: member, args: declared } = &*f {
            if declared.is_empty() {
              let key = format!("{module}.{member}");
              let reach = self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r).unwrap_or(Reach::Render);
              self.check_reach(call.span, &key, reach)?;
              let mut args = Vec::with_capacity(call.arguments.len());
              for a in &call.arguments {
                args.push(self.expr(crate::syntax::arg_expr(a))?);
              }
              let expr = Expr::Ext { module: module.clone(), name: member.clone(), args };
              return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
            }
          }
          if !matches!(&*f, Expr::Lambda { .. } | Expr::Fix { .. }) && !matches!(&*f, Expr::Var(v) if v == name) {
            return Err(self.residue(id.span, format!("`{name}` is not a function")));
          }
          if self.on_render_path() && !self.reaches(Reach::Terminal) {
            if let Some(key) = self.body_extension_in(&f) {
              if self.reads_only_the_date_clock(&f) {
                return Err(self.browser_only(call.span, format!("`{name}` reads the current time on a render path, which the server and the browser read at different moments")));
              }
              self.reach_violation = true;
              return Err(self.residue(call.span, format!("`{name}` calls `{key}` on a render path")));
            }
          }
          let applied = self.applied(f, call)?;
          return Ok(self.candidate(call.span, applied));
        }
        let one = |this: &mut Self| -> Lowered<Box<Expr>> {
          let a = call.arguments.first().ok_or_else(|| this.residue(call.span, format!("`{name}` takes one argument")))?;
          Ok(Box::new(this.expr(crate::syntax::arg_expr(a))?))
        };
        return match name {
          "String" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Str(Box::new(self.as_primitive(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "String" => Ok(Expr::Str(one(self)?)),
          "Number" if matches!(call.arguments.as_slice(), [only] if !crate::syntax::is_spread(only)) => Ok(Expr::Num(Box::new(self.as_primitive(&crate::syntax::arg_expr(&call.arguments[0]))?))),
          "Number" => Ok(Expr::Num(one(self)?)),
          "BigInt" => Ok(Expr::BigInt(one(self)?)),
          "Boolean" => Ok(Expr::Not(Box::new(Expr::Not(one(self)?)))),
          "encodeURIComponent" => Ok(Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*one(self)?] }),
          "decodeURIComponent" => Ok(Expr::Builtin { name: Builtin::DecodeUriComponent, args: vec![*one(self)?] }),
          "isNaN" => Ok(Expr::Builtin { name: Builtin::IsNaN, args: vec![*one(self)?] }),
          "btoa" | "atob" => Ok(Expr::Ext { module: "encoding".to_owned(), name: name.to_owned(), args: vec![*one(self)?] }),
          "structuredClone" if call.arguments.len() <= 1 => { let value = *one(self)?; Ok(if heap_pass() { Expr::Builtin { name: Builtin::CopyDeep, args: vec![value] } } else { value }) }
          "parseInt" => {
            let mut args = vec![*one(self)?];
            if let Some(radix) = call.arguments.get(1) {
              args.push(self.expr(crate::syntax::arg_expr(radix))?);
            }
            Ok(Expr::Builtin { name: Builtin::ParseInt, args })
          }
          name if ERROR_CONSTRUCTORS.contains(&name) => {
            let message = match call.arguments.first() {
              Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
              None => Expr::lit_str(""),
            };
            Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)]))
          }
          "fail" => Err(self.residue(call.span, "`fail` inside an expression; it is a statement")),
          "redirect" if self.parsed.names_server(&call.callee, "redirect") => self.redirect_to(call),
          "after" if self.parsed.names_server(&call.callee, "after") => self.after_of(call),
          "$state" | "$derived" if self.parsed.file.ends_with(".svelte.ts") || self.parsed.file.ends_with(".svelte.js") => match call.arguments.first() {
            Some(_) => Ok(*one(self)?),
            None => Ok(Expr::Lit(Lit::Null)),
          },
          _ => {
            self.unbound = Some(name.to_owned());
            if let Some((source, _)) = crate::component::find_import(self.parsed, name).filter(|(source, _)| node_module(source)) {
              return Err(self.residue_with(id.span, format!("`{name}` from `{source}`, which is not FSR's"), NOT_FSR));
            }
            Err(self.residue_with(
              id.span,
              format!("a call to `{name}`, which the build cannot follow"),
              "the build reads a helper declared at module level, or imported from a file under the app; a bare package import is not followed, and neither is a name nothing declares",
            ))
          }
        };
      }
    }

    let Some(member) = (callee).as_member_expression() else {
      if matches!(crate::meaning::bare(callee), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) {
        return Err(self.residue_with(
          callee.span(),
          "a function called where it is written",
          "the build does not run a function to produce a value; write the value as an expression or move the steps into a named function the body calls",
        ));
      }
      let f = Box::new(self.expr(callee)?);
      return self.applied(f, call);
    };
    if let Some(applied) = self.function_member(member, call)? {
      return Ok(applied);
    }
    let method = self.member_name(member).ok_or_else(|| self.residue(member.span(), "a computed method name"))?;
    if self.kind_of(member.object()) == Some(Kind::Scope) {
      return self.scope_call(&method, member, call);
    }

    if let js::Expression::Identifier(obj) = member.object() {
      let global = obj.name.as_str();
      if !self.scope.iter().any(|(n, _)| n == global) && !self.globals.iter().any(|(n, _)| n == global) {
        match global {
          "Object" => {
            let a = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`Object.{method}` takes one argument")))?;
            let target = Box::new(self.expr(crate::syntax::arg_expr(a))?);
            return match method.as_str() {
              "entries" => Ok(Expr::Entries(target)),
              "fromEntries" => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![*target] }),
              "keys" => Ok(Expr::Keys(target)),
              "values" => Ok(Expr::Values(target)),
              _ => Err(self.residue(member.span(), format!("`Object.{method}`"))),
            };
          }
          "Math" if crate::JS_MATH.contains(&method.as_str()) => {
            let mut args = vec![Expr::lit_str(method.as_str())];
            for a in &call.arguments {
              if crate::syntax::is_spread(a) {
                return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
              }
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            return Ok(Expr::Builtin { name: Builtin::Math, args });
          }
          "Number" if method == "parseInt" => {
            let Some(first) = call.arguments.first() else { return Err(self.residue(call.span, "`Number.parseInt` takes a string and an optional radix")) };
            let mut args = vec![self.expr(crate::syntax::arg_expr(first))?];
            if let Some(radix) = call.arguments.get(1) {
              args.push(self.expr(crate::syntax::arg_expr(radix))?);
            }
            return Ok(Expr::Builtin { name: Builtin::ParseInt, args });
          }
          "Number" if matches!(method.as_str(), "isInteger" | "isSafeInteger" | "isFinite" | "isNaN") => {
            let Some(arg) = call.arguments.first() else { return Err(self.residue(call.span, format!("`Number.{method}` takes one argument"))) };
            let x = self.expr(crate::syntax::arg_expr(arg))?;
            let both = |a: Expr, b: Expr| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b));
            let compare = |op: CompareOp, a: Expr, b: Expr| Expr::Compare(op, Box::new(a), Box::new(b));
            let number = compare(CompareOp::Eq, Expr::Builtin { name: Builtin::TypeOf, args: vec![x.clone()] }, Expr::lit_str("number"));
            let finite = compare(CompareOp::Eq, Expr::Arith(ArithOp::Sub, Box::new(x.clone()), Box::new(x.clone())), Expr::Lit(Lit::Int(0)));
            let whole = compare(CompareOp::Eq, Expr::Builtin { name: Builtin::Trunc, args: vec![x.clone()] }, x.clone());
            return Ok(match method.as_str() {
              "isNaN" => both(number, Expr::Builtin { name: Builtin::IsNaN, args: vec![x] }),
              "isFinite" => both(number, finite),
              "isInteger" => both(number, both(finite, whole)),
              _ => both(number, both(finite, both(whole, compare(CompareOp::Le, Expr::Builtin { name: Builtin::Abs, args: vec![x] }, Expr::Lit(Lit::Int(9_007_199_254_740_991)))))),
            });
          }
          "Math" if method == "random" && self.reaches(Reach::Terminal) && call.arguments.is_empty() => {
            let bytes = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Float(6.0))] };
            let whole = Expr::Builtin { name: Builtin::ParseInt, args: vec![bytes, Expr::Lit(Lit::Float(16.0))] };
            return Ok(Expr::Arith(ArithOp::Div, Box::new(whole), Box::new(Expr::Lit(Lit::Float(281_474_976_710_656.0)))));
          }
          "Math" => {
            let name = match method.as_str() {
              "round" => Builtin::Round,
              "floor" => Builtin::Floor,
              "ceil" => Builtin::Ceil,
              "abs" => Builtin::Abs,
              "min" => Builtin::Min,
              "max" => Builtin::Max,
              "pow" => Builtin::Pow,
              "sqrt" => Builtin::Sqrt,
              "trunc" => Builtin::Trunc,
              "sign" => Builtin::Sign,
              _ => return Err(self.residue(member.span(), format!("`Math.{method}`"))),
            };
            if matches!(name, Builtin::Min | Builtin::Max) && call.arguments.iter().any(|a| crate::syntax::is_spread(a)) {
              let mut entries = Vec::with_capacity(call.arguments.len());
              for a in &call.arguments {
                let value = self.expr(crate::syntax::arg_expr(a))?;
                entries.push(if crate::syntax::is_spread(a) { Entry::Spread(value) } else { Entry::Item(value) });
              }
              let name = if name == Builtin::Min { Builtin::MinOf } else { Builtin::MaxOf };
              return Ok(Expr::Builtin { name, args: vec![Expr::Array(entries)] });
            }
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              if crate::syntax::is_spread(a) {
                return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
              }
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            return Ok(Expr::Builtin { name, args });
          }
          "Response" if matches!(method.as_str(), "json" | "redirect") => {
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            let name = if method == "json" { Builtin::ResponseJson } else { Builtin::ResponseRedirect };
            return Ok(Expr::Builtin { name, args });
          }
          "JSON" if method == "parse" => {
            let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`JSON.parse` takes the text alone; a reviver is not lowered")) };
            return Ok(Expr::Builtin { name: Builtin::JsonParse, args: vec![self.expr(crate::syntax::arg_expr(arg))?] });
          }
          "JSON" if method == "stringify" => {
            if call.arguments.len() > 3 || call.arguments.get(1).is_some_and(|a| !matches!(crate::syntax::arg_expr(a), js::Expression::NullLiteral(_))) {
              return Err(self.residue(call.span, "`JSON.stringify` with a replacer; it takes a value, `null` and an indent"));
            }
            let mut args = Vec::with_capacity(call.arguments.len());
            for a in &call.arguments {
              args.push(self.expr(crate::syntax::arg_expr(a))?);
            }
            return Ok(Expr::Builtin { name: Builtin::Json, args });
          }
          "Array" if method == "isArray" => {
            let [arg] = call.arguments.as_slice() else { return Err(self.residue(call.span, "`Array.isArray` takes one argument")) };
            return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(crate::syntax::arg_expr(arg))?] });
          }
          "Array" if method == "from" && call.arguments.first().is_some_and(|a| !matches!(crate::syntax::arg_expr(a), js::Expression::ObjectExpression(_))) => {
            let source = &crate::syntax::arg_expr(&call.arguments[0]);
            let mut items = self.expr(source)?;
            items = match self.kind_of(source) {
              Some(Kind::Map) => Expr::Entries(Box::new(items)),
              _ => Expr::Array(vec![Entry::Spread(items)]),
            };
            return match call.arguments.get(1) {
              None => Ok(items),
              Some(f) => {
                let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&f) else {
                  return Err(self.residue(crate::syntax::arg_expr(&f).span(), "`Array.from` takes an arrow function written in place"));
                };
                Ok(Expr::Map(Box::new(items), Box::new(self.lambda(arrow)?)))
              }
            };
          }
          "crypto" if method == "randomUUID" => {
            if self.on_render_path() && !self.reaches(Reach::Terminal) {
              return Err(self.browser_only(call.span, "`crypto.randomUUID()` on a render path, which gives the server and the browser different ids"));
            }
            return Ok(Expr::Ext { module: "crypto".to_owned(), name: "uuid".to_owned(), args: Vec::new() });
          }
          "performance" if method == "now" => {
            if self.on_render_path() && !self.reaches(Reach::Terminal) {
              return Err(self.browser_only(call.span, "the current time on a render path, which the server and the browser read at different moments"));
            }
            return Ok(Expr::Num(Box::new(Expr::Ext { module: "time".to_owned(), name: "monotonic".to_owned(), args: Vec::new() })));
          }
          "Date" => {
            return match method.as_str() {
              "now" => self.date_now(call.span),
              "parse" => {
                let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`Date.parse` without a string"))?;
                Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![self.expr(crate::syntax::arg_expr(arg))?] })
              }
              other => Err(self.residue(member.span(), format!("`Date.{other}`"))),
            };
          }
          "Array" if method == "from" => {
            let (Some(shape), Some(f)) = (call.arguments.first(), call.arguments.get(1)) else {
              return Err(self.residue(call.span, "`Array.from` takes `{ length }` and a function"));
            };
            let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(shape) else {
              return Err(self.residue(crate::syntax::arg_expr(shape).span(), "`Array.from` over something other than `{ length: n }`"));
            };
            let length = obj.properties.iter().find_map(|p| match p {
              js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
                crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("length") => Some(&kv.value),
                _ => None,
              },
              _ => None,
            });
            let Some(length) = length else { return Err(self.residue(crate::syntax::arg_expr(shape).span(), "`Array.from` over something other than `{ length: n }`")) };
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&f) else {
              return Err(self.residue(crate::syntax::arg_expr(&f).span(), "`Array.from` takes an arrow function written in place"));
            };
            return Ok(Expr::Map(range, Box::new(self.lambda(arrow)?)));
          }
          _ => {}
        }
      }
    }

    if let Some((service, _via_ctx)) = self.service_of(member.object()) {
      let args = self.object_args(call, "service")?;
      return Ok(Expr::Call { service, method, args });
    }

    if let Some((module, _via_ctx)) = self.native_of(member.object()) {
      let args = self.object_args(call, "native")?;
      // `sync` is set by the build from the Rust signature, which this reader cannot see.
      return Ok(Expr::NativeCall { module, method, args, sync: false });
    }

    if method == "fill" {
      if let Some(length) = self.array_of_length(member.object()) {
        let length = self.expr(length)?;
        let value = match call.arguments.first() {
          Some(arg) if call.arguments.len() == 1 => self.expr(crate::syntax::arg_expr(arg))?,
          Some(_) => return Err(self.residue(call.span, "`fill` with a start or an end")),
          None => Expr::Lit(Lit::Null),
        };
        let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![length] });
        return Ok(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$filled".to_owned()], body: Box::new(value) })));
      }
    }
    if method == "segment" || method == "containing" {
      let (segmenter, text) = match (method.as_str(), strip_parens(member.object())) {
        ("segment", obj) => (self.formatter_of(obj), call.arguments.first().map(crate::syntax::arg_expr)),
        ("containing", js::Expression::CallExpression(inner)) => match strip_parens(&inner.callee).as_member_expression() {
          Some(m) if matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "segment") => (self.formatter_of(m.object()), inner.arguments.first().map(crate::syntax::arg_expr)),
          _ => (None, None),
        },
        _ => (None, None),
      };
      if let Some(made) = segmenter.filter(|made| platform_object(made) == Some("Segmenter")) {
        self.grapheme_segmenter(&made)?;
        let text = text.ok_or_else(|| self.residue(call.span, "`segment` without a string"))?;
        let mut args = vec![self.expr(text)?];
        if method == "containing" {
          let at = call.arguments.first().ok_or_else(|| self.residue(call.span, "`containing` without an index"))?;
          args.push(self.expr(crate::syntax::arg_expr(at))?);
        }
        return Ok(Expr::Builtin { name: Builtin::Graphemes, args });
      }
    }
    if matches!(method.as_str(), "encode" | "decode") {
      if let Some(made) = self.formatter_of(member.object()) {
        match (platform_object(&made), method.as_str()) {
          (Some("TextEncoder"), "encode") | (Some("TextDecoder"), "decode") => {
            if method == "decode" {
              let args = &made.arguments[..];
              let utf8 = match args.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
                None => true,
                Some(js::Expression::StringLiteral(s)) => matches!(crate::lossy(s.value).to_ascii_lowercase().as_str(), "utf-8" | "utf8" | "unicode-1-1-utf-8"),
                Some(_) => false,
              };
              if !utf8 || args.len() > 1 {
                return Err(self.residue_with(made.span, "a `TextDecoder` with a label or options", "`new TextDecoder()` decodes UTF-8, which the build reads"));
              }
            }
            let value = match call.arguments.first() {
              Some(a) => self.expr(crate::syntax::arg_expr(a))?,
              None => Expr::lit_str(""),
            };
            return Ok(Expr::Ext { module: "encoding".to_owned(), name: method.clone(), args: vec![value] });
          }
          _ => {}
        }
      }
    }
    if matches!(method.as_str(), "format" | "select" | "compare") {
      if let Some(made) = self.formatter_of(member.object()) {
        let called = match (platform_object(&made), method.as_str()) {
          (Some(formatter @ ("NumberFormat" | "DateTimeFormat" | "ListFormat" | "RelativeTimeFormat")), "format") | (Some(formatter @ "PluralRules"), "select") | (Some(formatter @ "Collator"), "compare") => Some(formatter),
          _ => None,
        };
        if let Some(formatter) = called {
          let mut values = Vec::with_capacity(call.arguments.len());
          for a in &call.arguments {
            values.push(self.expr(crate::syntax::arg_expr(a))?);
          }
          let expr = self.intl_call(formatter, &made.arguments, values, made.span)?;
          return Ok(self.candidate(call.span, expr));
        }
      }
    }
    match self.kind_of(member.object()) {
      Some(Kind::Map) => {
        let target = self.expr(member.object())?;
        let key = |this: &mut Self| -> Lowered<Expr> {
          let arg = call.arguments.first().ok_or_else(|| this.residue(call.span, format!("`{method}` without a key")))?;
          Ok(Expr::Builtin { name: Builtin::MapKey, args: vec![this.expr(crate::syntax::arg_expr(arg))?] })
        };
        return match method.as_str() {
          "get" => Ok(Expr::Index(Box::new(target), Box::new(key(self)?))),
          "has" => Ok(Expr::Builtin { name: Builtin::HasKey, args: vec![target, key(self)?] }),
          "keys" => Ok(Expr::Keys(Box::new(target))),
          "values" => Ok(Expr::Values(Box::new(target))),
          "entries" => Ok(Expr::Entries(Box::new(target))),
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `Map`"))),
        };
      }
      Some(Kind::Set) => {
        let target = self.expr(member.object())?;
        return match method.as_str() {
          "has" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`has` without a value"))?;
            Ok(Expr::Builtin { name: Builtin::Includes, args: vec![target, self.expr(crate::syntax::arg_expr(arg))?] })
          }
          "values" | "keys" => Ok(target),
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `Set`"))),
        };
      }
      Some(Kind::Query) if method != "toString" => {
        let target = self.expr(member.object())?;
        return match method.as_str() {
          "get" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, "`get` without a key"))?;
            Ok(Expr::Builtin { name: Builtin::FormGet, args: vec![target, Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(arg))?))] })
          }
          "has" | "getAll" => {
            let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without a key")))?;
            let name = if method == "has" { Builtin::FormHas } else { Builtin::FormGetAll };
            Ok(Expr::Builtin { name, args: vec![target, Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(arg))?))] })
          }
          "entries" => Ok(Expr::Builtin { name: Builtin::FormEntries, args: vec![target] }),
          other => Err(self.residue(member.span(), format!("`.{other}()` of a `URLSearchParams`"))),
        };
      }
      _ => {}
    }

    let target = Box::new(self.expr(member.object())?);
    let lambda = |this: &mut Self, i: usize| -> Lowered<Box<Expr>> {
      let a = call.arguments.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes a function")))?;
      if let js::Expression::Identifier(id) = crate::syntax::arg_expr(a) {
        let name = id.name.as_str();
        if name == "Boolean" && !this.scope.iter().any(|(n, _)| n == name) && this.root_of(id).is_none() && !this.globals.iter().any(|(n, _)| n == name) {
          let item = this.temp();
          return Ok(Box::new(Expr::Lambda { params: vec![item.clone()], body: Box::new(Expr::Not(Box::new(Expr::Not(Box::new(Expr::Var(item)))))) }));
        }
        if let Some(function) = this.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| *f) {
          return this.function_expr(&function).map(Box::new);
        }
        let bound = this.scope.iter().rev().find(|(n, _)| n == name).or_else(|| this.globals.iter().rev().find(|(n, _)| n == name)).map(|(_, e)| e.clone());
        match &bound {
          Some(Expr::Lambda { .. } | Expr::Fix { .. }) => return this.expr(crate::syntax::arg_expr(a)).map(Box::new),
          Some(Expr::Var(v)) if v == name => return this.expr(crate::syntax::arg_expr(a)).map(Box::new),
          Some(_) => {}
          None if this.root_of(id).is_none() => {
            this.unbound = Some(name.to_owned());
            return Err(this.residue(crate::syntax::arg_expr(a).span(), format!("`{method}` takes a function; `{name}` is not bound here")));
          }
          None => {}
        }
      }
      if let Some(m) = (strip_parens(crate::syntax::arg_expr(a))).as_member_expression() {
        if let Some(compare) = this.collator_compare(m)? {
          return Ok(Box::new(compare));
        }
      }
      let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(a) else {
        return Err(this.residue(crate::syntax::arg_expr(a).span(), format!("`{method}` takes an arrow function written in place or the name of a function")));
      };
      let f = this.lambda(arrow)?;
      if let Expr::Lambda { body, .. } = &f {
        if matches!(&**body, Expr::Builtin { name: Builtin::WriteBack, .. }) {
          let written = crate::component::written_params(&crate::params_of(&arrow.params), match crate::syntax::arrow_body_of(arrow) {
            crate::syntax::ArrowBody::Block(b) => b.body,
            crate::syntax::ArrowBody::Expr(_) => &[],
          });
          let name = written.first().map(|(_, p)| p.clone()).unwrap_or_default();
          return Err(this.residue_with(arrow.span, format!("a write to a field of `{name}`, which this body did not declare: `{method}` would change the item in place"), "return a new value from the callback"));
        }
      }
      Ok(Box::new(f))
    };
    match method.as_str() {
      "map" => Ok(Expr::Map(target, lambda(self, 0)?)),
      "filter" => Ok(Expr::Filter(target, lambda(self, 0)?)),
      "find" => Ok(Expr::Find(target, lambda(self, 0)?)),
      "findIndex" => Ok(Expr::FindIndex(target, lambda(self, 0)?)),
      "some" => Ok(Expr::Some(target, lambda(self, 0)?)),
      "every" => Ok(Expr::Every(target, lambda(self, 0)?)),
      "flatMap" => Ok(Expr::FlatMap(target, lambda(self, 0)?)),
      "flat" if call.arguments.is_empty() || matches!(&*crate::syntax::arg_expr(&call.arguments[0]), js::Expression::NumericLiteral(n) if n.value == 1.0) => Ok(Expr::FlatMap(target, Box::new(Expr::Lambda { params: vec!["$item".to_owned()], body: Box::new(Expr::Var("$item".to_owned())) }))),
      "toString" if call.arguments.is_empty() => Ok(Expr::Str(target)),
      "toString" => Ok(Expr::Builtin { name: Builtin::Radix, args: vec![*target, self.expr(&crate::syntax::arg_expr(&call.arguments[0]))?] }),
      "getTime" | "valueOf" => Ok(*target),
      "toISOString" | "toJSON" => Ok(Expr::Builtin { name: Builtin::IsoString, args: vec![*target] }),
      "getUTCFullYear" | "getUTCMonth" | "getUTCDate" | "getUTCDay" | "getUTCHours" | "getUTCMinutes" | "getUTCSeconds" | "getUTCMilliseconds" | "getMilliseconds" => {
        let part = match method.as_str() {
          "getUTCFullYear" => "year",
          "getUTCMonth" => "month",
          "getUTCDate" => "date",
          "getUTCDay" => "day",
          "getUTCHours" => "hours",
          "getUTCMinutes" => "minutes",
          "getUTCSeconds" => "seconds",
          _ => "milliseconds",
        };
        Ok(Expr::Builtin { name: Builtin::DatePart, args: vec![*target, Expr::lit_str(part)] })
      }
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" if self.reaches(Reach::Terminal) && call.arguments.is_empty() => {
        let part = match method.as_str() {
          "getFullYear" => "year",
          "getMonth" => "month",
          "getDate" => "date",
          "getDay" => "day",
          "getHours" => "hours",
          "getMinutes" => "minutes",
          _ => "seconds",
        };
        let at = self.temp();
        let offset = Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![Expr::Var(at.clone())] };
        let local = Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(at.clone())), Box::new(Expr::Arith(ArithOp::Mul, Box::new(offset), Box::new(Expr::Lit(Lit::Int(60_000))))));
        let body = Expr::Builtin { name: Builtin::DatePart, args: vec![local, Expr::lit_str(part)] };
        Ok(Expr::Apply { f: Box::new(Expr::Lambda { params: vec![at], body: Box::new(body) }), args: vec![*target] })
      }
      "getTimezoneOffset" if self.reaches(Reach::Terminal) => Ok(Expr::Ext { module: "time".to_owned(), name: "localOffset".to_owned(), args: vec![*target] }),
      "toLocaleTimeString" if (self.reaches(Reach::Terminal) || !self.on_render_path()) && call.arguments.len() <= 1 => {
        let locale = match call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
          None => Expr::Lit(Lit::Null),
          Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => Expr::Lit(Lit::Null),
          Some(js::Expression::StringLiteral(tag)) => Expr::lit_str(crate::lossy(tag.value)),
          Some(other) => return Err(self.residue_with(other.span(), "a locale that is not written in place", "write the locale, `\"en-US\"`; `undefined` takes the system's")),
        };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "localTime".to_owned(), args: vec![*target, locale] })
      }
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTimezoneOffset" | "toLocaleTimeString" | "toTimeString" | "toDateString" => {
        let utc = method.replacen("get", "getUTC", 1);
        let mut residue = self.browser_only(member.span(), format!("`.{method}()`, which reads the viewer's time zone"));
        residue.hint = Some(format!("as text under an element the browser fills it in when the component hydrates; the server has no viewer, so `.{utc}()` and `toLocaleDateString(undefined, {{ dateStyle }})` read UTC, which both agree on"));
        Err(residue)
      }
      "localeCompare" => {
        let other = match call.arguments.first() {
          Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
          None => return Err(self.residue(call.span, "`localeCompare` without a string to compare with")),
        };
        self.intl_call("Collator", call.arguments.get(1..).unwrap_or_default(), vec![*target, other], call.span)
      }
      "toLocaleDateString" => {
        let made = &call.arguments[..call.arguments.len().min(2)];
        let expr = self.intl_format("DateTimeFormat", made, *target, call.span)?;
        Ok(self.candidate(call.span, expr))
      }
      "toSorted" | "sort" | "toReversed" | "reverse" => {
        if matches!(method.as_str(), "sort" | "reverse") && !fresh_array(member.object()) {
          return Err(self.residue_with(member.span(), format!("`.{method}()` of an array something else holds, which it rewrites in place"), format!("`[...items].{method}()` or `items.slice().{method}()` gives the same order without the write")));
        }
        if method.contains("everse") {
          return Ok(Expr::Builtin { name: Builtin::Reverse, args: vec![*target] });
        }
        let f = match call.arguments.first() {
          None => Box::new(Expr::Lit(Lit::Null)),
          Some(_) => lambda(self, 0)?,
        };
        Ok(Expr::Sort(target, f))
      }
      "slice" | "at" | "indexOf" | "lastIndexOf" | "charAt" | "charCodeAt" | "codePointAt" | "concat" | "padStart" | "padEnd" | "substring" => {
        let name = match method.as_str() {
          "slice" => Builtin::Slice,
          "at" => Builtin::At,
          "indexOf" => Builtin::IndexOf,
          "lastIndexOf" => Builtin::LastIndexOf,
          "charAt" => Builtin::CharAt,
          "charCodeAt" => Builtin::CharCodeAt,
          "codePointAt" => Builtin::CodePointAt,
          "concat" => Builtin::Concat,
          "padStart" => Builtin::PadStart,
          "padEnd" => Builtin::PadEnd,
          _ => Builtin::Substring,
        };
        let mut args = vec![*target];
        for a in &call.arguments {
          if crate::syntax::is_spread(a) {
            return Err(self.residue(crate::syntax::arg_expr(a).span(), "a spread argument"));
          }
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
      "reduce" => {
        let rewritten = match (call.arguments.first().map(|a| strip_parens(crate::syntax::arg_expr(a))), call.arguments.get(1).map(|a| strip_parens(crate::syntax::arg_expr(a)))) {
          (Some(js::Expression::ArrowFunctionExpression(arrow)), Some(js::Expression::ObjectExpression(_))) => fresh_accumulator(self.build(), arrow),
          _ => None,
        };
        let f = match rewritten {
          Some(arrow) => Box::new(self.lambda(&arrow)?),
          None => lambda(self, 0)?,
        };
        let Some(init) = call.arguments.get(1) else {
          let items = self.temp();
          let whole = Expr::Var(items.clone());
          let f = match *f {
            Expr::Lambda { params, body } if params.len() > 2 => {
              let (index, array) = (self.temp(), self.temp());
              let mut binds = vec![Stmt::Let { name: params[2].clone(), expr: Expr::Arith(ArithOp::Add, Box::new(Expr::Var(index.clone())), Box::new(Expr::Lit(Lit::Int(1)))) }];
              if let Some(all) = params.get(3) {
                binds.push(Stmt::Let { name: all.clone(), expr: whole.clone() });
              }
              binds.push(Stmt::Return(*body));
              Box::new(Expr::Lambda { params: vec![params[0].clone(), params[1].clone(), index, array], body: Box::new(Expr::Block(binds)) })
            }
            f => Box::new(f),
          };
          let rest = Expr::Builtin { name: Builtin::Slice, args: vec![whole.clone(), Expr::Lit(Lit::Int(1))] };
          let reduced = Expr::Reduce(Box::new(rest), Box::new(Expr::Index(Box::new(whole.clone()), Box::new(Expr::Lit(Lit::Int(0))))), f);
          let empty = Expr::Builtin { name: Builtin::Throw, args: vec![Expr::lit_str("Reduce of empty array with no initial value")] };
          return Ok(Expr::Block(vec![Stmt::Let { name: items, expr: *target }, Stmt::Return(Expr::Ternary(Box::new(Expr::Length(Box::new(whole))), Box::new(reduced), Box::new(empty)))]));
        };
        let init = Box::new(self.expr(crate::syntax::arg_expr(init))?);
        Ok(Expr::Reduce(target, init, f))
      }
      "trimStart" | "trimEnd" | "trimLeft" | "trimRight" if call.arguments.is_empty() => {
        let side = if matches!(method.as_str(), "trimStart" | "trimLeft") { "start" } else { "end" };
        Ok(Expr::Builtin { name: Builtin::Trim, args: vec![*target, Expr::lit_str(side)] })
      }
      "toLocaleString" => {
        let formatted = self.intl_format("NumberFormat", &call.arguments, *target, call.span)?;
        Ok(self.candidate(call.span, formatted))
      }
      "toFixed" | "repeat" | "join" | "trim" | "toUpperCase" | "toLowerCase" | "includes" => {
        let name = match method.as_str() {
          "toFixed" => Builtin::ToFixed,
          "repeat" => Builtin::Repeat,
          "join" => Builtin::Join,
          "trim" => Builtin::Trim,
          "toUpperCase" => Builtin::Upper,
          "toLowerCase" => Builtin::Lower,
          _ => Builtin::Includes,
        };
        let mut args = vec![*target];
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
      "test" | "match" | "matchAll" | "search" => {
        let arg = call.arguments.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without an argument")))?;
        let arg = self.expr(crate::syntax::arg_expr(arg))?;
        Ok(match method.as_str() {
          "test" => Expr::Builtin { name: Builtin::RegexTest, args: vec![*target, arg] },
          "match" => Expr::Builtin { name: Builtin::Match, args: vec![*target, arg] },
          "matchAll" => Expr::Builtin { name: Builtin::MatchAll, args: vec![*target, arg] },
          _ => Expr::Builtin { name: Builtin::Search, args: vec![*target, arg] },
        })
      }
      "replace" | "replaceAll" if call.arguments.len() == 2 && matches!(&*crate::syntax::arg_expr(&call.arguments[1]), js::Expression::ArrowFunctionExpression(_)) => {
        let js::Expression::ArrowFunctionExpression(arrow) = &*crate::syntax::arg_expr(&call.arguments[1]) else { unreachable!("checked") };
        let pattern = match (&*crate::syntax::arg_expr(&call.arguments[0]), method.as_str()) {
          (js::Expression::StringLiteral(s), "replaceAll") => regex_value(self, &escape_regex(crate::lossy(s.value).as_ref()), "g", call.span)?,
          (js::Expression::RegExpLiteral(re), "replaceAll") if !re.regex.flags.contains(js::RegExpFlags::G) => return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on")),
          (pattern, "replaceAll") if !matches!(pattern, js::Expression::RegExpLiteral(_)) => return Err(self.residue(call.span, "`replaceAll` of a pattern the build cannot read with a function")),
          (pattern, _) => self.expr(pattern)?,
        };
        let f = self.lambda(arrow)?;
        Ok(Expr::ReplaceWith(target, Box::new(pattern), Box::new(f)))
      }
      "replaceAll" => {
        let [from, to] = call.arguments.as_slice() else { return Err(self.residue(call.span, format!("`replaceAll` takes 2 arguments, got {}", call.arguments.len()))) };
        let to = self.expr(crate::syntax::arg_expr(to))?;
        match crate::syntax::arg_expr(from) {
          js::Expression::RegExpLiteral(re) => {
            if !re.regex.flags.contains(js::RegExpFlags::G) {
              return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on"));
            }
            let pattern = self.expr(crate::syntax::arg_expr(from))?;
            Ok(Expr::Builtin { name: Builtin::Replace, args: vec![*target, pattern, to] })
          }
          other => Ok(Expr::Builtin { name: Builtin::ReplaceAll, args: vec![*target, self.expr(other)?, to] }),
        }
      }
      "split" | "startsWith" | "endsWith" | "replace" => {
        let (name, arity) = match method.as_str() {
          "split" => (Builtin::Split, 1),
          "startsWith" => (Builtin::StartsWith, 1),
          "endsWith" => (Builtin::EndsWith, 1),
          _ => (Builtin::Replace, 2),
        };
        // The `position` that `startsWith` and `endsWith` take in JavaScript
        // would otherwise lower to an argument the interpreter ignores.
        let limited = name == Builtin::Split && call.arguments.len() == 2;
        if call.arguments.len() != arity && !limited {
          let plural = if arity == 1 { "" } else { "s" };
          return Err(self.residue(call.span, format!("`{method}` takes {arity} argument{plural}, got {}", call.arguments.len())));
        }
        let mut args = vec![*target];
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Builtin { name, args })
      }
      other if !STANDARD_METHODS.contains(&other) => {
        let mut args = Vec::with_capacity(call.arguments.len());
        for a in &call.arguments {
          args.push(self.expr(crate::syntax::arg_expr(a))?);
        }
        Ok(Expr::Apply { f: Box::new(Expr::Field(target, other.to_owned())), args })
      }
      other => Err(self.residue_with(
        member.span(),
        format!("`.{other}()`, which is not a builtin"),
        "the builtins are `map`, `filter`, `find`, `findIndex`, `some`, `every`, `reduce`, `flatMap`, `flat`, `toSorted`, `toReversed`, `slice`, `at`, `indexOf`, `lastIndexOf`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `join`, `includes`, `trim`, `repeat`, `toFixed`, `toString`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `substring`, `split`, `startsWith`, `endsWith`, `replace`, `replaceAll`, `match`, `matchAll`, `search`, `test`, `localeCompare`, `toLocaleString`, `trimStart`, `trimEnd`, `valueOf`, `getTime`, `toISOString` and `toJSON`, with `sort` and `reverse` on a fresh array; anything else goes in a module-level helper the build can read",
      )),
    }
  }

  /// The one object literal a service or native method takes, as named
  /// arguments. `kind` names the caller in a diagnostic.
  fn object_args(&mut self, call: &js::CallExpression, kind: &str) -> Lowered<Vec<(String, Expr)>> {
    let mut args = Vec::new();
    if let Some(a) = call.arguments.first() {
      let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(a) else {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("{kind} arguments must be an object literal")));
      };
      for prop in &obj.properties {
        match prop {
          js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
            crate::syntax::Prop::Shorthand(id) => args.push((id.name.to_string(), self.ident(id)?)),
            crate::syntax::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed argument name"))?;
              args.push((key, self.expr(&kv.value)?));
            }
            other => return Err(self.residue(other.span(), "a method in the arguments")),
          },
          js::ObjectPropertyKind::SpreadProperty(s) => return Err(self.residue(s.argument.span(), format!("a spread into {kind} arguments"))),
        }
      }
    }
    if call.arguments.len() > 1 {
      return Err(self.residue(call.span, format!("a {kind} method takes one object")));
    }
    Ok(args)
  }

  /// `native.<name>` or `ctx.native.<name>`.
  fn native_of(&self, obj: &js::Expression) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Native, "native")
  }

  /// `services.<name>` or `ctx.services.<name>`.
  fn service_of(&self, obj: &js::Expression) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Services, "services")
  }

  /// `<root>.<name>` bare or `ctx.<field>.<name>`; the bool says which.
  fn rooted_at(&self, obj: &js::Expression, root: Root, field: &str) -> Option<(String, bool)> {
    let Some(m) = (obj).as_member_expression() else { return None };
    let name = self.member_name(m)?;
    match m.object() {
      js::Expression::Identifier(id) if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) => match self.root_of(id) {
        Some(held) if held == root => Some((name, false)),
        _ => None,
      },
      inner_expr if inner_expr.is_member_expression() => {
let inner = inner_expr.as_member_expression().expect("matched as a member");
        let js::Expression::Identifier(id) = inner.object() else { return None };
        if self.scope.iter().any(|(n, _)| n == id.name.as_str()) {
          return None;
        }
        match (self.root_of(id), self.member_name(inner).as_deref()) {
          (Some(Root::Ctx), Some(via)) if via == field => Some((name, true)),
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// An arrow function applied by a builtin. Its body is one expression or a
  /// block that only returns one. Destructured parameters read as fields and
  /// indexes of the positional parameter.
  /// `new Date(x)`, `new Map(pairs)`, `new Set(items)` and `new URLSearchParams(x)`; anything else made with `new` is residue.
  /// `Some(true)` for the global `Promise.resolve(…)` and `Some(false)` for `Promise.reject(…)` in a page whose host resumes a handler after its step.
  fn settled_promise(&self, call: &js::CallExpression) -> Option<bool> {
    let callee = &call.callee;
    let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let (js::Expression::Identifier(promise), crate::syntax::MemberProp::Ident(method)) = (m.object(), crate::syntax::prop(m)) else { return None };
    if !self.reaches(Reach::Later) || self.parsed.ctxt(promise) != crate::Ctxt::Unresolved || promise.name.as_str() != "Promise" {
      return None;
    }
    match method.name.as_str() {
      "resolve" => Some(true),
      "reject" => Some(false),
      _ => None,
    }
  }

  /// A fresh deferred settled at once with `value`, fulfilled when `ok` and rejected otherwise, as `new Promise` settles one its executor settles.
  fn settled(&mut self, ok: bool, value: Expr) -> Expr {
    let deferred = self.temp();
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let settle = Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), Expr::Var(deferred.clone())), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))]);
    Expr::Block(vec![
      Stmt::Let { name: deferred.clone(), expr: Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)]) },
      Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), settle), Entry::Field("value".to_owned(), value)]) },
      Stmt::Return(Expr::Var(deferred)),
    ])
  }

  /// `new Promise((resolve, reject) => …)` in a terminal page: a fresh deferred, the executor run at once with `resolve` and `reject` held as [`snapfire_fsr_ir::host::SETTLES`] values on it, then the deferred.
  fn terminal_promise(&mut self, n: &js::NewExpression, args: &[js::Argument]) -> Lowered<Expr> {
    let Some(js::Expression::ArrowFunctionExpression(executor)) = args.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.residue(n.span, "a `Promise` whose executor is not an arrow written in place")) };
    let deferred = self.temp();
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let mut body = vec![Stmt::Let { name: deferred.clone(), expr: Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)]) }];
    let depth = self.scope.len();
    let settlers = self.settlers.len();
    for (param, ok) in crate::params_of(&executor.params).into_iter().zip([true, false]) {
      let (js::BindingPattern::BindingIdentifier(name), None, false) = (param.pattern, param.default, param.rest) else { return Err(self.residue(param.span, "a `Promise` executor's parameter that is not a name")) };
      let settle = Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), Expr::Var(deferred.clone())), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))]);
      self.scope.push((name.name.to_string(), settle));
      self.settlers.push(name.name.to_string());
    }
    let ran = match crate::syntax::arrow_body_of(executor) {
      crate::syntax::ArrowBody::Block(b) => crate::component::block_to_expr(self, b.body),
      crate::syntax::ArrowBody::Expr(e) => self.effect(e).map(|stmt| Expr::Block(vec![stmt])),
    };
    self.scope.truncate(depth);
    self.settlers.truncate(settlers);
    body.push(Stmt::Expr(ran?));
    body.push(Stmt::Return(Expr::Var(deferred)));
    Ok(Expr::Block(body))
  }

  fn new_expr(&mut self, n: &js::NewExpression) -> Lowered<Expr> {
    // A `Scope` starts with no value; the cell made for it holds what `run` and `set` set.
    if matches!(&n.callee, js::Expression::Identifier(id) if id.name.as_str() == "Scope") {
      return Ok(Expr::Lit(Lit::Undefined));
    }
    let args = &n.arguments[..];
    if let Some(spread) = args.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.residue(crate::syntax::arg_expr(spread).span(), "a spread argument"));
    }
    let svelte = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(&n.callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::SvelteClass(class))) => Some(class),
      _ => None,
    };
    if let Some("Spring" | "Tween") = svelte {
      let value = match args.first() {
        Some(a) => self.expr(crate::syntax::arg_expr(a))?,
        None => Expr::Lit(Lit::Null),
      };
      return Ok(Expr::Object(vec![Entry::Field("current".to_owned(), value.clone()), Entry::Field("target".to_owned(), value)]));
    }
    if self.reaches(Reach::Later) && matches!(&n.callee, js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && id.name.as_str() == "Promise") {
      return self.terminal_promise(n, &args);
    }
    if let js::Expression::Identifier(id) = &n.callee {
      if self.parsed.ctxt(id) == crate::Ctxt::Unresolved && ERROR_CONSTRUCTORS.contains(&id.name.as_str()) {
        let message = match args.first() {
          Some(a) => Expr::Str(Box::new(self.expr(crate::syntax::arg_expr(a))?)),
          None => Expr::lit_str(""),
        };
        return Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(id.name.as_str())), Entry::Field("message".to_owned(), message)]));
      }
    }
    let global = match &n.callee {
      _ if svelte.is_some() => svelte.expect("checked").to_owned(),
      _ if matches!(platform_object(n), Some("TextEncoder" | "TextDecoder")) => return Err(self.residue_with(n.span, "a `TextEncoder` or `TextDecoder` kept as a value", "call its method where it is made or on a name bound to it: `new TextEncoder().encode(s)`")),
      _ if platform_object(n).is_some() => return Err(self.residue_with(n.span, "an `Intl` formatter kept as a value", "call `.format(x)` on it where it is made, `new Intl.NumberFormat(undefined, options).format(n)`")),
      js::Expression::Identifier(id) if self.parsed.ctxt(id) == crate::Ctxt::Unresolved => id.name.to_string(),
      other => match self.class_new(n)? {
        Some(made) => return Ok(made),
        None => return Err(self.residue(other.span(), "`new` of something other than a platform global or a class the build reads")),
      },
    };
    let first = match args.first() {
      Some(a) => Some(self.expr(crate::syntax::arg_expr(a))?),
      None => None,
    };
    match (global.as_str(), first) {
      ("Date", None) => self.date_now(n.span),
      ("Date", Some(x)) if args.len() == 1 => Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![x] }),
      ("Date", Some(_)) => Err(self.residue_with(n.span, "`new Date(year, month, ...)`, which reads the viewer's time zone", "an ISO 8601 string ending in `Z` names the same instant everywhere, `new Date(\"2026-10-05T00:00:00Z\")`")),
      ("Map", None) => Ok(Kind::Map.made(Expr::Object(Vec::new()))),
      ("Map", Some(pairs)) => Ok(Kind::Map.made(Expr::Builtin { name: Builtin::MapOf, args: vec![pairs] })),
      ("Set", None) => Ok(Kind::Set.made(Expr::Array(Vec::new()))),
      ("Set", Some(items)) => Ok(Kind::Set.made(Expr::Builtin { name: Builtin::Unique, args: vec![items] })),
      ("Response", body) => {
        let mut args = vec![body.unwrap_or(Expr::Lit(Lit::Undefined))];
        if let Some(init) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(init))?);
        }
        Ok(Expr::Builtin { name: Builtin::Response, args })
      }
      ("Request", Some(url)) => Ok(Expr::Builtin { name: Builtin::RequestNew, args: vec![url] }),
      ("Uint8Array", source) => Ok(Expr::Builtin { name: Builtin::Uint8Array, args: source.into_iter().collect() }),
      ("URLSearchParams", None) => Ok(Expr::lit_str("")),
      ("URLSearchParams", Some(x)) => Ok(Expr::Builtin { name: Builtin::FormEncode, args: vec![x] }),
      ("URL", Some(x)) => {
        let mut args = vec![x];
        if let Some(base) = n.arguments.get(1) {
          args.push(self.expr(crate::syntax::arg_expr(base))?);
        }
        Ok(Expr::Builtin { name: Builtin::UrlParse, args })
      }
      ("RegExp", Some(_)) if matches!(crate::syntax::arg_expr(&args[0]), js::Expression::RegExpLiteral(_)) => Err(self.residue(n.span, "`new RegExp` of a regular expression")),
      ("RegExp", Some(pattern)) => {
        let flags = match args.get(1) {
          Some(a) => Some(crate::syntax::arg_expr(a)),
          None => None,
        };
        if let (js::Expression::StringLiteral(p), None | Some(js::Expression::StringLiteral(_))) = (crate::syntax::arg_expr(&args[0]), flags) {
          let flags = match flags {
            Some(js::Expression::StringLiteral(f)) => crate::lossy(f.value),
            _ => String::new(),
          };
          return regex_value(self, crate::lossy(p.value).as_ref(), &flags, n.span);
        }
        let flags = match flags {
          Some(f) => Expr::Str(Box::new(self.expr(f)?)),
          None => Expr::lit_str(""),
        };
        Ok(Expr::Object(vec![
          Entry::Field(snapfire_fsr_ir::jsregex::PATTERN.to_owned(), Expr::Str(Box::new(pattern))),
          Entry::Field(snapfire_fsr_ir::jsregex::FLAGS.to_owned(), flags),
        ]))
      }
      (other, _) => Err(self.residue(n.span, format!("`new {other}`"))),
    }
  }

  /// `Date.now()` or `new Date()`: the request's clock in a body; on a render path the server and the browser would each read their own moment.
  pub(crate) fn date_now(&mut self, span: Span) -> Lowered<Expr> {
    if self.on_render_path() && !self.reaches(Reach::Terminal) {
      return Err(self.browser_only(span, "the current time on a render path, which the server and the browser read at different moments"));
    }
    Ok(Expr::Num(Box::new(Expr::Ext { module: "time".to_owned(), name: "now".to_owned(), args: Vec::new() })))
  }

  /// Refuses an `Intl.Segmenter` the build does not split as the browser would: one outside a terminal, whose browser may hold other Unicode tables, and one whose granularity is not `"grapheme"`.
  fn grapheme_segmenter(&mut self, made: &js::NewExpression) -> Lowered<()> {
    if !self.reaches(Reach::Terminal) {
      return Err(self.browser_only(made.span, "`Intl.Segmenter`, which splits by the browser's own Unicode tables"));
    }
    let granularity = made.arguments.get(1).map(|options| match crate::syntax::arg_expr(options) {
      js::Expression::ObjectExpression(obj) => obj.properties.iter().find_map(|prop| match prop {
        js::ObjectPropertyKind::ObjectProperty(prop) => match crate::syntax::prop_of(prop) {
          crate::syntax::Prop::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("granularity") => Some(match &kv.value {
            js::Expression::StringLiteral(s) => crate::lossy(s.value),
            _ => String::new(),
          }),
          _ => None,
        },
        _ => None,
      }),
      _ => Some(String::new()),
    });
    match granularity.flatten().as_deref() {
      None | Some("grapheme") => Ok(()),
      Some(_) => Err(self.residue_with(made.span, "an `Intl.Segmenter` whose granularity is not `\"grapheme\"`", "a terminal splits text into graphemes; write `{ granularity: \"grapheme\" }`")),
    }
  }

  /// `collator.compare` as a value, a function of two strings, when `collator` is an `Intl.Collator`.
  fn collator_compare(&mut self, member: &js::MemberExpression) -> Lowered<Option<Expr>> {
    if !matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "compare") {
      return Ok(None);
    }
    let Some(made) = self.formatter_of(member.object()).filter(|made| platform_object(made) == Some("Collator")) else { return Ok(None) };
    let (a, b) = (self.temp(), self.temp());
    let body = self.intl_call("Collator", &made.arguments, vec![Expr::Var(a.clone()), Expr::Var(b.clone())], made.span)?;
    Ok(Some(Expr::Lambda { params: vec![a, b], body: Box::new(body) }))
  }

  /// A formatter's call as the `intl` member that answers it the same way under the locale written where it is made, else the request's: `format` of `Intl.NumberFormat`, `Intl.DateTimeFormat`, `Intl.ListFormat` and `Intl.RelativeTimeFormat`, `select` of `Intl.PluralRules` and `compare` of `Intl.Collator`.
  fn intl_call(&mut self, formatter: &str, made: &[js::Argument], mut values: Vec<Expr>, span: Span) -> Lowered<Expr> {
    let wants = match formatter {
      "Collator" | "RelativeTimeFormat" => 2,
      _ => 1,
    };
    if values.len() < wants {
      return Err(self.residue(span, format!("`Intl.{formatter}` called with {} of its {wants} arguments", values.len())));
    }
    values.truncate(wants);
    if matches!(formatter, "NumberFormat" | "DateTimeFormat" | "ListFormat") {
      return self.intl_format(formatter, made, values.remove(0), span);
    }
    let (locale, options) = self.intl_made(formatter, made)?;
    let (name, allowed): (&str, &[(&str, &[&str])]) = match formatter {
      "PluralRules" => ("plural", &[("type", &["cardinal", "ordinal"])]),
      "Collator" => ("compare", &[("sensitivity", &["base", "accent", "case", "variant"]), ("numeric", &[]), ("usage", &["sort"])]),
      _ => ("relative", &[("numeric", &["always", "auto"]), ("style", &["long", "short", "narrow"])]),
    };
    let mut entries: Vec<Entry> = locale.into_iter().collect();
    entries.extend(self.intl_options(formatter, &options, allowed, span)?);
    values.push(Expr::Object(entries));
    Ok(Expr::Ext { module: "intl".to_owned(), name: name.to_owned(), args: values })
  }

  /// The locale and the options a formatter is made with: the locale `None` for the request's.
  fn intl_made<'m>(&mut self, formatter: &str, made: &'m [js::Argument<'m>]) -> Lowered<(Option<Entry>, Vec<(String, &'m js::Expression<'m>)>)> {
    let locale = match made.first().map(|a| strip_parens(crate::syntax::arg_expr(a))) {
      None => None,
      Some(js::Expression::Identifier(id)) if id.name.as_str() == "undefined" => None,
      Some(js::Expression::StringLiteral(s)) => Some(Expr::lit_str(crate::lossy(s.value))),
      Some(other) => Some(self.expr(other)?),
    };
    let mut options: Vec<(String, &'m js::Expression<'m>)> = Vec::new();
    if let Some(given) = made.get(1) {
      let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(given) else { return Err(self.residue(crate::syntax::arg_expr(given).span(), format!("`Intl.{formatter}` options that are not an object literal"))) };
      for prop in &obj.properties {
        let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.residue(obj.span, format!("a spread in `Intl.{formatter}` options"))) };
        let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.residue(obj.span, format!("an `Intl.{formatter}` option written other than `key: value`"))) };
        let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), format!("a computed `Intl.{formatter}` option")))?;
        options.push((key, &kv.value));
      }
    }
    Ok((locale.map(|tag| Entry::Field("locale".to_owned(), tag)), options))
  }

  /// `options` as entries, each key one `allowed` names; a string written in place must be one of the key's values when it lists any.
  fn intl_options(&mut self, formatter: &str, options: &[(String, &js::Expression)], allowed: &[(&str, &[&str])], span: Span) -> Lowered<Vec<Entry>> {
    let mut entries = Vec::new();
    for (key, v) in options {
      let Some((_, values)) = allowed.iter().find(|(k, _)| k == key) else {
        let read = allowed.iter().map(|(k, _)| format!("`{k}`")).collect::<Vec<_>>();
        return Err(self.residue(span, format!("the `Intl.{formatter}` option `{key}`; {} are read", list_and(&read))));
      };
      if let js::Expression::StringLiteral(s) = v {
        let text = crate::lossy(s.value);
        if !values.is_empty() && !values.contains(&text.as_str()) {
          let read = values.iter().map(|v| format!("`\"{v}\"`")).collect::<Vec<_>>();
          return Err(self.residue(v.span(), format!("`{key}: \"{text}\"` in `Intl.{formatter}`; {} are read", list_and(&read))));
        }
      }
      entries.push(Entry::Field(key.clone(), self.expr(v)?));
    }
    Ok(entries)
  }

  /// `new Intl.NumberFormat(locale, options).format(value)`, `new Intl.DateTimeFormat(locale, options).format(value)` and `new Intl.ListFormat(locale, options).format(items)`.
  fn intl_format(&mut self, formatter: &str, made: &[js::Argument], value: Expr, span: Span) -> Lowered<Expr> {
    let (locale_entry, options) = self.intl_made(formatter, made)?;
    let literal = |options: &[(String, &js::Expression)], key: &str| options.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
      js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
      _ => None,
    });
    match formatter {
      "NumberFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        if literal(&options, "style").as_deref() == Some("currency") {
          let Some((_, code)) = options.iter().find(|(k, _)| k == "currency") else { return Err(self.residue(span, "a currency style with no `currency`")) };
          let code = self.expr(code)?;
          // The browser's default display is the symbol.
          let display = match literal(&options, "currencyDisplay").as_deref() {
            None | Some("symbol") => "symbol",
            Some("code") => "code",
            Some("narrowSymbol") => "narrowSymbol",
            Some(other) => return Err(self.residue(span, format!("`currencyDisplay: \"{other}\"`; code, symbol and narrowSymbol are read"))),
          };
          entries.push(Entry::Field("display".to_owned(), Expr::lit_str(display)));
          for (key, v) in &options {
            match key.as_str() {
              "style" | "currency" | "currencyDisplay" => {}
              "minimumFractionDigits" | "maximumFractionDigits" => entries.push(Entry::Field(key.clone(), self.expr(v)?)),
              other => return Err(self.residue(span, format!("the `Intl.NumberFormat` option `{other}` with a currency; `minimumFractionDigits` and `maximumFractionDigits` are read"))),
            }
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "currency".to_owned(), args: vec![value, code, Expr::Object(entries)] });
        }
        let allowed: &[(&str, &[&str])] = &[
          ("style", &["decimal", "percent"]),
          ("minimumFractionDigits", &[]),
          ("maximumFractionDigits", &[]),
          ("minimumIntegerDigits", &[]),
          ("useGrouping", &["auto", "always", "min2"]),
          ("signDisplay", &["auto", "always", "exceptZero", "negative", "never"]),
        ];
        entries.extend(self.intl_options("NumberFormat", &options, allowed, span)?);
        Ok(Expr::Ext { module: "intl".to_owned(), name: "number".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      "ListFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        entries.extend(self.intl_options("ListFormat", &options, &[("type", &["conjunction", "disjunction", "unit"]), ("style", &["long", "short", "narrow"])], span)?);
        Ok(Expr::Ext { module: "intl".to_owned(), name: "list".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      _ => {
        let zone = options.iter().find(|(k, _)| k == "timeZone");
        if zone.is_some() && literal(&options, "timeZone").as_deref() != Some("UTC") {
          return Err(self.residue(span, "a `timeZone` other than `\"UTC\"`"));
        }
        if let Some((_, time_style)) = options.iter().find(|(k, _)| k == "timeStyle") {
          if zone.is_none() {
            return Err(self.browser_only(span, "a time of day without `timeZone: \"UTC\"`, which the browser formats in the viewer's time zone"));
          }
          if let js::Expression::StringLiteral(s) = time_style {
            let text = crate::lossy(s.value);
            if !matches!(text.as_str(), "short" | "medium") {
              return Err(self.residue(time_style.span(), format!("`timeStyle: \"{text}\"`, which names the time zone; `timeStyle: \"short\"` and `timeStyle: \"medium\"` are read")));
            }
          }
          let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
          for (key, v) in &options {
            match key.as_str() {
              "timeZone" => {}
              "timeStyle" => entries.push(Entry::Field("timeStyle".to_owned(), self.expr(v)?)),
              "dateStyle" => entries.push(Entry::Field("style".to_owned(), self.expr(v)?)),
              other => return Err(self.residue(span, format!("the `Intl.DateTimeFormat` option `{other}` beside `timeStyle`; `dateStyle` and `timeZone: \"UTC\"` are read"))),
            }
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(entries)] });
        }
        if let Some(style) = literal(&options, "dateStyle") {
          if let Some((other, _)) = options.iter().find(|(k, _)| !matches!(k.as_str(), "dateStyle" | "timeZone")) {
            return Err(self.residue(span, format!("the `Intl.DateTimeFormat` option `{other}` beside `dateStyle`; `timeStyle` and `timeZone: \"UTC\"` are read")));
          }
          return Ok(match locale_entry {
            None => Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::lit_str(style)] },
            Some(locale) => Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(vec![Entry::Field("style".to_owned(), Expr::lit_str(style)), locale])] },
          });
        }
        // The parts the server formats through a field set: a text month or
        // weekday, a numeric year and day.
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        for (key, v) in &options {
          let text = match v {
            js::Expression::StringLiteral(s) => crate::lossy(s.value),
            _ => return Err(self.residue(v.span(), format!("a `{key}` that is not written in place"))),
          };
          let read = match (key.as_str(), text.as_str()) {
            ("timeZone", _) => continue,
            ("weekday" | "month", "long" | "short" | "narrow") => true,
            ("year" | "day", "numeric") => true,
            ("month", "numeric" | "2-digit") => return Err(self.residue_with(v.span(), "a numeric month", "`month: \"short\"` or a `dateStyle`; the server has no digits that agree with the browser's")),
            ("weekday" | "month" | "year" | "day", _) => false,
            _ => return Err(self.residue(span, format!("the date formatting option `{key}`; `dateStyle`, `timeStyle`, `weekday`, `year`, `month`, `day` and `timeZone: \"UTC\"` are read"))),
          };
          if !read {
            return Err(self.residue(v.span(), format!("`{key}: \"{text}\"`")));
          }
          entries.push(Entry::Field(key.clone(), Expr::lit_str(text)));
        }
        if !entries.iter().any(|e| matches!(e, Entry::Field(k, _) if matches!(k.as_str(), "weekday" | "year" | "month" | "day"))) {
          return Err(self.residue_with(span, "a date formatted without a `dateStyle` or a part", "`{ dateStyle: \"medium\" }` or `{ weekday: \"long\" }` names a format the server and the browser write the same way"));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
    }
  }

  /// `a?.b.c(x)`: the call as written, `null` where an optional link's object is null or undefined.
  fn optional_call(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let b = self.build();
    let mut guards = Vec::new();
    let callee = strip_optional(b, &call.callee, &mut guards);
    // A call on a value known to be null short-circuits, as JavaScript does, whatever the method.
    for guard in &guards {
      if let js::Expression::Identifier(id) = strip_parens(guard) {
        if matches!(self.scope.iter().rev().find(|(n, _)| n == id.name.as_str()), Some((_, Expr::Lit(Lit::Null)))) {
          return Ok(Expr::Lit(Lit::Null));
        }
      }
    }
    let plain = js::CallExpression::new(call.span, callee, None, b.copy(&call.arguments), false, &b.ast());
    let mut out = self.call(&plain)?;
    for guard in guards.iter().rev() {
      let object = self.expr(guard)?;
      let missing = Expr::Builtin { name: Builtin::IsNullish, args: vec![object] };
      out = Expr::Ternary(Box::new(missing), Box::new(Expr::Lit(Lit::Undefined)), Box::new(out));
    }
    Ok(out)
  }

  /// A `function` expression or a method as the lambda it is; a generator is residue.
  /// `scope.get()`, `scope.run(value, fn, ...args)` and `scope.set(value)` on a `Scope` a module holds as a cell: the cell read; the cell set to `value` around a call of `fn` with the value before put back after, the call's result answered; the cell set. A web page has no process to scope, so there the storage is residue.
  fn scope_call(&mut self, method: &str, member: &js::MemberExpression, call: &js::CallExpression) -> Lowered<Expr> {
    let js::Expression::Identifier(id) = crate::meaning::bare(member.object()) else { return Err(self.residue(member.span(), "a `Scope` reached other than by its name")) };
    let name = id.name.to_string();
    let Some((_, handle)) = self.cells.iter().find(|(cell, _)| *cell == name).cloned() else {
      return Err(self.residue(member.span(), format!("`{name}`, a `Scope`, outside a terminal page; a web page has no process to scope")));
    };
    match (method, call.arguments.as_slice()) {
      ("get", []) => Ok(Expr::ReadCell(Box::new(handle))),
      ("set", [value]) => Ok(Expr::WriteCell(Box::new(handle), Box::new(self.expr(syntax::arg_expr(value))?))),
      ("run", [value, f, rest @ ..]) => {
        let value = self.expr(syntax::arg_expr(value))?;
        let f = Box::new(self.expr(syntax::arg_expr(f))?);
        let mut args = Vec::with_capacity(rest.len());
        for a in rest {
          args.push(self.expr(syntax::arg_expr(a))?);
        }
        let (before, result) = (self.temp(), self.temp());
        Ok(Expr::Block(vec![
          Stmt::Let { name: before.clone(), expr: Expr::ReadCell(Box::new(handle.clone())) },
          Stmt::Expr(Expr::WriteCell(Box::new(handle.clone()), Box::new(value))),
          Stmt::Let { name: result.clone(), expr: Expr::Apply { f, args } },
          Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(Expr::Var(before)))),
          Stmt::Return(Expr::Var(result)),
        ]))
      }
      _ => Err(self.residue(call.span, format!("`.{method}()` of a `Scope`; `get`, `run` and `set` are read"))),
    }
  }

  /// `object.name(args)` or `object[key](args)` where `object` lowers to an object literal whose field (every field, for a computed key) is a function value: that field applied to the arguments. A computed key on an object of no known kind is applied as well, since the build cannot name the method. `None` for any other member.
  fn function_member(&mut self, member: &js::MemberExpression, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    // Only a name or a chain of fields is looked at: lowering a call here and again on the method path would count its hoists twice.
    let mut at = crate::meaning::bare(member.object());
    while let Some(inner) = at.as_member_expression() {
      at = crate::meaning::bare(inner.object());
    }
    if !matches!(at, js::Expression::Identifier(_)) {
      return Ok(None);
    }
    let depth = self.scope.len();
    let object = self.expr(member.object());
    self.scope.truncate(depth);
    let Ok(object) = object else { return Ok(None) };
    if let syntax::MemberProp::Ident(name) = syntax::prop(member) {
      if self.holds_method(member.object(), name.name.as_str()) {
        return self.applied(Box::new(object.field(name.name.to_string())), call).map(Some);
      }
    }
    let Expr::Object(entries) = &object else {
      // A computed key on a value of no known kind: whatever the key names is applied, as JavaScript applies it.
      if let (syntax::MemberProp::Computed(key), None) = (syntax::prop(member), self.kind_of(member.object())) {
        let key = self.expr(key)?;
        let callee = Expr::Index(Box::new(object.clone()), Box::new(key));
        return self.applied(Box::new(callee), call).map(Some);
      }
      return Ok(None);
    };
    let is_function = |e: &Expr| matches!(e, Expr::Lambda { .. } | Expr::Fix { .. });
    let callee = match syntax::prop(member) {
      syntax::MemberProp::Ident(name) => {
        let name = name.name.to_string();
        if !entries.iter().any(|entry| matches!(entry, Entry::Field(field, value) if *field == name && is_function(value))) {
          return Ok(None);
        }
        object.clone().field(name)
      }
      syntax::MemberProp::Computed(key) => {
        if entries.is_empty() || !entries.iter().all(|entry| matches!(entry, Entry::Field(_, value) if is_function(value))) {
          return Ok(None);
        }
        let key = self.expr(key)?;
        Expr::Index(Box::new(object.clone()), Box::new(key))
      }
      syntax::MemberProp::Private(_) => return Ok(None),
    };
    self.applied(Box::new(callee), call).map(Some)
  }

  /// `f` applied to `call`'s arguments: [`Expr::Apply`], or [`Expr::ApplyOf`] over one array when an argument is spread.
  fn applied(&mut self, f: Box<Expr>, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      let spread = matches!(a, js::Argument::SpreadElement(_));
      let value = syntax::arg_expr(a);
      // A helper's `Ctx["services"]` parameter is the services root inside it, so its slot carries nothing.
      let arg = if !spread && self.is_services_root(value) { Expr::Lit(Lit::Null) } else { self.expr(value)? };
      args.push((spread, arg));
    }
    if args.iter().all(|(spread, _)| !spread) {
      return Ok(Expr::Apply { f, args: args.into_iter().map(|(_, e)| e).collect() });
    }
    let entries = args.into_iter().map(|(spread, e)| if spread { Entry::Spread(e) } else { Entry::Item(e) }).collect();
    Ok(Expr::ApplyOf(f, Box::new(Expr::Array(entries))))
  }

  pub(crate) fn function_expr(&mut self, f: &js::Function) -> Lowered<Expr> {
    if f.generator {
      return Err(self.residue(f.span, "a generator function"));
    }
    let depth = self.scope.len();
    let outer = std::mem::take(&mut self.breakable);
    let lowered = self.function_body(f, depth);
    self.breakable = outer;
    lowered
  }

  fn function_body(&mut self, f: &js::Function, depth: usize) -> Lowered<Expr> {
    let pats = params_of(&f.params);
    let params = match crate::component::bind_params(self, &pats) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let roots = self.bind_services_params(&pats, depth);
    let body = match &f.body {
      Some(body) => crate::component::block_to_expr(self, &body.statements),
      None => Ok(Expr::Lit(Lit::Null)),
    };
    self.roots.truncate(roots);
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }

  /// Each parameter typed `Ctx["services"]` leaves the scope and becomes the services root, so `services.<name>.<method>(...)` inside the function is the same static call a loader makes. Returns the roots' depth to restore.
  pub(crate) fn bind_services_params(&mut self, pats: &[Param<'_>], depth: usize) -> usize {
    let roots = self.roots.len();
    for name in pats.iter().copied().filter_map(services_param) {
      let mut at = depth;
      while at < self.scope.len() {
        if self.scope[at].0 == name {
          self.scope.remove(at);
        } else {
          at += 1;
        }
      }
      self.roots.push((name, Root::Services));
    }
    roots
  }

  /// `services` or `ctx.services` naming the root rather than a local.
  fn is_services_root(&self, e: &js::Expression) -> bool {
    let e = crate::meaning::bare(e);
    if let js::Expression::Identifier(id) = e {
      return !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.root_of(id) == Some(Root::Services);
    }
    match e.as_member_expression() {
      Some(m) => match m.object() {
        js::Expression::Identifier(id) => !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.root_of(id) == Some(Root::Ctx) && self.member_name(m).as_deref() == Some("services"),
        _ => false,
      },
      None => false,
    }
  }

  pub(crate) fn lambda(&mut self, arrow: &js::ArrowFunctionExpression) -> Lowered<Expr> {
    let depth = self.scope.len();
    let pats = params_of(&arrow.params);
    let params = match crate::component::bind_params(self, &pats) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let roots = self.bind_services_params(&pats, depth);
    let defaults = crate::component::held_frame(self, |this| crate::component::settle(this, depth)).0;
    let body = match arrow_body(arrow) {
      FunctionBody::Expr(e) => self.expr(e),
      FunctionBody::Block(stmts) if !writes_a_member_of(stmts, &params) => crate::component::block_to_expr_from(self, stmts, depth),
      FunctionBody::Block(stmts) => {
        let written = crate::component::written_params(&pats, stmts);
        crate::component::packing_returns(self, stmts, &written).and_then(|packed| crate::component::block_to_expr_from(self, &packed, depth)).map(|packed| crate::component::write_back(packed, &written))
      }
    };
    self.roots.truncate(roots);
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(crate::component::sequenced(defaults, body?)) })
  }
}

/// The name of a parameter typed `Ctx["services"]`, with or without `Ctx`'s type arguments.
fn services_param(param: Param<'_>) -> Option<String> {
  let binding = param.ident()?;
  let js::TSType::TSIndexedAccessType(indexed) = &param.type_annotation?.type_annotation else { return None };
  let js::TSType::TSTypeReference(object) = &indexed.object_type else { return None };
  let js::TSTypeName::IdentifierReference(ctx) = &object.type_name else { return None };
  let js::TSType::TSLiteralType(index) = &indexed.index_type else { return None };
  let js::TSLiteral::StringLiteral(key) = &index.literal else { return None };
  (ctx.name.as_str() == "Ctx" && key.value == "services").then(|| binding.name.to_string())
}

/// A regular expression literal as the value the regex builtins read, refused here when the interpreter could not match it as JavaScript does.
fn regex_value(lowerer: &Lowerer<'_>, pattern: &str, flags: &str, span: Span) -> Lowered<Expr> {
  if let Err(why) = snapfire_fsr_ir::jsregex::compiled(pattern, flags) {
    return Err(lowerer.residue_with(span, format!("a regular expression with {why}"), "write it without that construct; the server refuses a pattern that can backtrack without bound and a flag its engine lacks"));
  }
  Ok(Expr::Object(vec![
    Entry::Field(snapfire_fsr_ir::jsregex::PATTERN.to_owned(), Expr::lit_str(pattern)),
    Entry::Field(snapfire_fsr_ir::jsregex::FLAGS.to_owned(), Expr::lit_str(flags)),
  ]))
}

/// A string as the source of a regular expression that matches it literally.
fn escape_regex(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for c in text.chars() {
    if "\\^$.*+?()[]{}|/-".contains(c) {
      out.push('\\');
    }
    out.push(c);
  }
  out
}

/// `e` with each optional member link made plain, the object of each link pushed to `guards`, outermost first.
fn strip_optional<'a>(b: syntax::Build<'a>, e: &js::Expression<'_>, guards: &mut Vec<js::Expression<'a>>) -> js::Expression<'a> {
  match e {
    js::Expression::ChainExpression(chain) => match chain.expression.as_member_expression() {
      Some(m) => strip_member(b, m, guards),
      None => b.copy(e),
    },
    js::Expression::ParenthesizedExpression(p) => strip_optional(b, &p.expression, guards),
    other => match other.as_member_expression() {
      Some(m) => strip_member(b, m, guards),
      None => chained(b, other),
    },
  }
}

fn strip_member<'a>(b: syntax::Build<'a>, m: &js::MemberExpression<'_>, guards: &mut Vec<js::Expression<'a>>) -> js::Expression<'a> {
  let obj = strip_optional(b, m.object(), guards);
  if m.optional() {
    guards.push(b.copy(&obj));
  }
  let ast = b.ast();
  match m {
    js::MemberExpression::StaticMemberExpression(s) => js::Expression::new_static_member_expression(s.span, obj, b.copy(&s.property), false, &ast),
    js::MemberExpression::ComputedMemberExpression(c) => js::Expression::new_computed_member_expression(c.span, obj, b.copy(&c.expression), false, &ast),
    js::MemberExpression::PrivateFieldExpression(p) => js::Expression::new_private_field_expression(p.span, obj, b.copy(&p.field), false, &ast),
  }
}

/// A copy of `e`, which sits inside an optional chain, wrapped in a chain of its own when it holds an optional link, so it reads as the chain it was part of.
fn chained<'a>(b: syntax::Build<'a>, e: &js::Expression<'_>) -> js::Expression<'a> {
  fn optional(e: &js::Expression) -> bool {
    match e {
      js::Expression::CallExpression(call) => call.optional || optional(&call.callee),
      other => other.as_member_expression().is_some_and(|m| m.optional() || optional(m.object())),
    }
  }
  if !optional(e) {
    return b.copy(e);
  }
  let span = e.span();
  let element = match b.copy(e) {
    js::Expression::CallExpression(call) => js::ChainElement::CallExpression(call),
    js::Expression::StaticMemberExpression(m) => js::ChainElement::StaticMemberExpression(m),
    js::Expression::ComputedMemberExpression(m) => js::ChainElement::ComputedMemberExpression(m),
    js::Expression::PrivateFieldExpression(m) => js::ChainElement::PrivateFieldExpression(m),
    other => return other,
  };
  js::Expression::new_chain_expression(span, element, &b.ast())
}

/// The kind `e` makes in place: a `new Map(...)`, `new Set(...)`, `new URLSearchParams(...)`, `new URL(...)` or `new Date(...)`, a bigint literal or `BigInt(...)`.
pub(crate) fn made_kind(e: &js::Expression) -> Option<Kind> {
  match e {
    js::Expression::ParenthesizedExpression(p) => made_kind(&p.expression),
    js::Expression::TSAsExpression(a) => made_kind(&a.expression),
    js::Expression::TSNonNullExpression(a) => made_kind(&a.expression),
    js::Expression::NewExpression(n) => match &n.callee {
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Map" | "SvelteMap") => Some(Kind::Map),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Set" | "SvelteSet") => Some(Kind::Set),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "URLSearchParams" | "SvelteURLSearchParams") => Some(Kind::Query),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "URL" | "SvelteURL") => Some(Kind::Url),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Date" | "SvelteDate") => Some(Kind::Date),
      js::Expression::Identifier(id) if id.name.as_str() == "Response" => Some(Kind::Response),
      js::Expression::Identifier(id) if id.name.as_str() == "Scope" => Some(Kind::Scope),
      _ => None,
    },
    js::Expression::BigIntLiteral(_) => Some(Kind::BigInt),
    js::Expression::CallExpression(call) => match &call.callee {
      js::Expression::Identifier(id) if id.name.as_str() == "BigInt" => Some(Kind::BigInt),
      js::Expression::StaticMemberExpression(m) if matches!(&m.object, js::Expression::Identifier(obj) if obj.name.as_str() == "Response") && matches!(m.property.name.as_str(), "json" | "redirect") => Some(Kind::Response),
      _ => None,
    },
    _ => None,
  }
}

/// Whether `f` is a lambda whose block body writes a name it neither takes nor declares: a helper writing a module-level `let`.
pub(crate) fn writes_outer(f: &Expr) -> bool {
  !outer_set_targets(f).is_empty()
}

/// The names a lambda sets that it neither takes nor declares, module-level `let`s.
pub(crate) fn outer_set_targets(f: &Expr) -> Vec<String> {
  let Expr::Lambda { params, body } = f else { return Vec::new() };
  let Expr::Block(body) = &**body else { return Vec::new() };
  let mut declared: Vec<String> = Vec::new();
  fn lets(body: &[Stmt], out: &mut Vec<String>) {
    for stmt in body {
      match stmt {
        Stmt::Let { name, .. } => out.push(name.clone()),
        Stmt::If { then, r#else, .. } => {
          lets(then, out);
          lets(r#else, out);
        }
        Stmt::ForOf { name, body, .. } => {
          out.push(name.clone());
          lets(body, out);
        }
        _ => {}
      }
    }
  }
  lets(body, &mut declared);
  snapfire_fsr_ir::ast::set_targets(body).into_iter().filter(|name| !params.contains(name) && !declared.contains(name)).collect()
}

/// The prop a component binding reads, `$props.key` or that with a default, so a kind read through it is a kind relied on.
fn prop_key(bound: &Expr) -> Option<String> {
  match bound {
    Expr::Field(target, key) if **target == Expr::Var("$props".to_owned()) => Some(key.clone()),
    Expr::Coalesce(read, _) => prop_key(read),
    _ => None,
  }
}

/// Whether `body` writes or extends the session anywhere in it.
fn writes_session(body: &[Stmt]) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } => true,
    Stmt::If { then, r#else, .. } => writes_session(then) || writes_session(r#else),
    Stmt::ForOf { body, .. } => writes_session(body),
    Stmt::While { body, step, .. } => writes_session(body) || writes_session(step),
    Stmt::Try { body, handler, finally, .. } => writes_session(body) || handler.as_deref().is_some_and(writes_session) || writes_session(finally),
    Stmt::Let { .. } | Stmt::Set { .. } | Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Guard { .. } | Stmt::Act { .. } | Stmt::Expr(_) => false,
  })
}

pub(crate) fn strip_parens<'e, 'a>(e: &'e js::Expression<'a>) -> &'e js::Expression<'a> {
  match e {
    js::Expression::ParenthesizedExpression(p) => strip_parens(&p.expression),
    other => other,
  }
}

/// The text of a template element, cooked where it has a cooked value.
pub(crate) fn quasi_text(quasi: &js::TemplateElement) -> String {
  quasi.value.cooked.map(lossy).unwrap_or_else(|| quasi.value.raw.to_string())
}

/// The text of a string literal or of a template literal with no `${}` part.
fn string_literal(e: &js::Expression) -> Option<String> {
  match crate::meaning::bare(e) {
    js::Expression::StringLiteral(s) => Some(lossy(s.value)),
    js::Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => Some(quasi_text(&tpl.quasis[0])),
    _ => None,
  }
}

/// Which platform constructor `n` calls whose methods the build lowers: the `Intl` formatters `NumberFormat`, `DateTimeFormat`, `ListFormat`, `Segmenter`, `PluralRules`, `Collator` and `RelativeTimeFormat`, then `TextEncoder` and `TextDecoder`.
pub(crate) fn platform_object(n: &js::NewExpression) -> Option<&'static str> {
  if let js::Expression::Identifier(id) = &n.callee {
    return match id.name.as_str() {
      "TextEncoder" => Some("TextEncoder"),
      "TextDecoder" => Some("TextDecoder"),
      _ => None,
    };
  }
  let js::Expression::StaticMemberExpression(m) = &n.callee else { return None };
  let js::Expression::Identifier(ns) = &m.object else { return None };
  if ns.name.as_str() != "Intl" {
    return None;
  }
  match m.property.name.as_str() {
    "NumberFormat" => Some("NumberFormat"),
    "DateTimeFormat" => Some("DateTimeFormat"),
    "ListFormat" => Some("ListFormat"),
    "Segmenter" => Some("Segmenter"),
    "PluralRules" => Some("PluralRules"),
    "Collator" => Some("Collator"),
    "RelativeTimeFormat" => Some("RelativeTimeFormat"),
    _ => None,
  }
}

/// `a`, `a and b` or `a, b and c`.
fn list_and(items: &[String]) -> String {
  match items {
    [] => String::new(),
    [one] => one.clone(),
    [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
  }
}

/// Whether a statement of an async function holds an `await`: `await x;` and `const v = await x;` [`Lowerer::async_stmts`] splits at; one deeper it refuses.
/// The `catch` of a `try` an async function awaits in: its parameter, its statements followed by those after the `try`, and how many of the statements lowering the `try` covers.
#[derive(Clone)]
struct AsyncCatch<'a> {
  param: Option<String>,
  body: &'a [js::Statement<'a>],
  covers: usize,
  /// The catch over `body`'s first statements: a `finally` that runs before what the body throws goes on.
  inner: Option<Box<AsyncCatch<'a>>>,
}

/// The local a member write changes and the members from it outward: `x.a[i].b` is `x` with `.a`, `[i]` and `.b`.
pub(crate) fn write_root<'e>(member: &'e js::MemberExpression<'e>) -> Option<(&'e js::IdentifierReference<'e>, Vec<&'e js::MemberExpression<'e>>)> {
  let mut path = vec![member];
  let mut at = member;
  loop {
    let object = crate::meaning::bare(at.object());
    match object {
      js::Expression::Identifier(id) => {
        path.reverse();
        return Some((id, path));
      }
      other => match other.as_member_expression() {
        Some(inner) => {
          path.push(inner);
          at = inner;
        }
        None => return None,
      },
    }
  }
}

/// `first`'s statements, when it is a block, followed by `rest` in one block, so a `let` among them stays bound for what follows; a nested block would end its `let`s with it.
fn spliced(first: Expr, rest: Vec<Stmt>) -> Expr {
  let mut out = Vec::with_capacity(rest.len() + 1);
  splice(first, &mut out);
  out.extend(rest);
  Expr::Block(out)
}

fn splice(expr: Expr, out: &mut Vec<Stmt>) {
  match expr {
    Expr::Block(stmts) => out.extend(stmts),
    other => out.push(Stmt::Expr(other)),
  }
}

/// A loop that awaits, taken apart for [`Lowerer::awaiting_loop_in`]: the statements that set it up, its test, one pass (the body, then the step) and what a `for...of` runs over.
struct AwaitingLoop<'a> {
  init: Vec<js::Statement<'a>>,
  test: js::Expression<'a>,
  pass: Vec<js::Statement<'a>>,
  over: Option<js::Expression<'a>>,
  span: Span,
  /// Whether the body leaves the loop from somewhere the pass cannot end: a `break` or a `continue` or `return` deeper than an `if` at its top.
  exits_deep: bool,
}

/// While a pass of a loop that awaits lowers: see [`Lowerer::loop_tail`].
#[derive(Clone)]
pub(crate) struct LoopTail {
  event: String,
  carry: Expr,
  done: Option<Expr>,
}

/// Where `span` starts as a source-map position: one past the byte offset, the first byte of the module being position 1.
pub(crate) fn lo(span: Span) -> u32 {
  span.start + 1
}

/// Where `span` ends as a source-map position, as [`lo`] counts.
pub(crate) fn hi(span: Span) -> u32 {
  span.end + 1
}

/// The statements a loop or branch body holds: a block's own, or the one statement.
fn body_stmts<'a>(b: syntax::Build<'a>, body: &js::Statement<'_>) -> Vec<js::Statement<'a>> {
  match body {
    js::Statement::BlockStatement(block) => block.body.iter().map(|s| b.copy(s)).collect(),
    other => vec![b.copy(other)],
  }
}

/// `for...of` over an array, a counted `for` or a `while` that awaits in its body, as an [`AwaitingLoop`]: `for (const x of xs)` becomes `const $items = xs; let $i = 0;` with the test `$i < $items.length`, a pass starting `const x = $items[$i]` and ending `$i++`.
fn awaiting_loop<'a>(parsed: &'a Parsed<'a>, stmt: &js::Statement<'_>) -> Option<AwaitingLoop<'a>> {
  if !awaits(stmt) {
    return None;
  }
  let b = syntax::Build::new(parsed.arena());
  let ast = b.ast();
  let span = stmt.span();
  let ident = |name: &str| {
    let made = b.ident(name, span);
    if let js::Expression::Identifier(id) = &made {
      parsed.scope.mark_top(id);
    }
    made
  };
  let binding = |name: &str| {
    let made = b.binding(name, span);
    if let js::BindingPattern::BindingIdentifier(id) = &made {
      parsed.scope.mark_top_binding(id);
    }
    made
  };
  let decl = |kind: js::VariableDeclarationKind, name: js::BindingPattern<'a>, init: js::Expression<'a>| {
    js::Statement::new_variable_declaration(span, kind, b.vec([js::VariableDeclarator::new(span, name, None, Some(init), false, &ast)]), false, &ast)
  };
  match stmt {
    js::Statement::ForOfStatement(for_of) => {
      let js::ForStatementLeft::VariableDeclaration(left) = &for_of.left else { return None };
      let first = left.declarations.first()?;
      let pattern = b.copy(&first.id);
      let (items, index) = (format!("$items{}", lo(span)), format!("$i{}", lo(span)));
      let init = vec![
        decl(js::VariableDeclarationKind::Const, binding(&items), b.copy(&for_of.right)),
        decl(js::VariableDeclarationKind::Let, binding(&index), js::Expression::new_numeric_literal(span, 0.0, None, oxc_syntax::number::NumberBase::Decimal, &ast)),
      ];
      let length = js::Expression::new_static_member_expression(span, ident(&items), js::IdentifierName::new(span, b.str("length"), &ast), false, &ast);
      let test = js::Expression::new_binary_expression(span, ident(&index), js::BinaryOperator::LessThan, length, &ast);
      let item = js::Expression::new_computed_member_expression(span, ident(&items), ident(&index), false, &ast);
      let mut body = vec![decl(js::VariableDeclarationKind::Const, pattern, item)];
      let inner = body_stmts(b, &for_of.body);
      let exits_deep = exits_deeper(&inner);
      body.extend(inner);
      let target = js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(&index), &ast);
      if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &target {
        parsed.scope.mark_top(id);
      }
      let step = vec![b.expr_stmt(js::Expression::new_update_expression(span, js::UpdateOperator::Increment, false, target, &ast))];
      Some(AwaitingLoop { init, test, pass: continued(b, body, &step), over: Some(b.copy(&for_of.right)), span, exits_deep })
    }
    js::Statement::ForStatement(counted) => {
      let init = match &counted.init {
        Some(js::ForStatementInit::VariableDeclaration(d)) => vec![js::Statement::VariableDeclaration(b.copy(d))],
        Some(other) => vec![b.expr_stmt(b.copy(other.to_expression()))],
        None => Vec::new(),
      };
      let test = counted.test.as_ref().map(|t| b.copy(t)).unwrap_or_else(|| b.boolean(true, span));
      let step: Vec<js::Statement<'a>> = counted.update.iter().map(|update| b.expr_stmt(b.copy(update))).collect();
      let body = body_stmts(b, &counted.body);
      Some(AwaitingLoop { init, test, exits_deep: exits_deeper(&body), pass: continued(b, body, &step), over: None, span })
    }
    js::Statement::WhileStatement(looped) => {
      let body = body_stmts(b, &looped.body);
      Some(AwaitingLoop { init: Vec::new(), test: b.copy(&looped.test), exits_deep: exits_deeper(&body), pass: continued(b, body, &[]), over: None, span })
    }
    _ => None,
  }
}

/// `body` with `step` at the end of every way through it, a `continue` or `return` at the body's top or ending an `if` there taken as that way's end: `if (c) continue; rest` becomes `if (c) { step } else { rest; step }` and `if (c) return v; rest` becomes `if (c) { return v } else { rest; step }`.
fn continued<'a>(b: syntax::Build<'a>, body: Vec<js::Statement<'a>>, step: &[js::Statement<'a>]) -> Vec<js::Statement<'a>> {
  let mut out = Vec::with_capacity(body.len() + step.len());
  let mut stmts = body.into_iter();
  while let Some(stmt) = stmts.next() {
    match stmt {
      js::Statement::ContinueStatement(_) => {
        out.extend(step.iter().map(|s| b.copy(s)));
        return out;
      }
      ret @ js::Statement::ReturnStatement(_) => {
        out.push(ret);
        return out;
      }
      js::Statement::IfStatement(branch) if branch.alternate.is_none() && (ends_in(&branch.consequent, Exit::Continue) || ends_in(&branch.consequent, Exit::Return)) => {
        let branch = branch.unbox();
        let span = branch.span;
        let returns = ends_in(&branch.consequent, Exit::Return);
        let mut taken: Vec<js::Statement<'a>> = match branch.consequent {
          js::Statement::BlockStatement(block) => block.unbox().body.into_iter().collect(),
          other => vec![other],
        };
        if !returns {
          taken.pop();
          taken.extend(step.iter().map(|s| b.copy(s)));
        }
        let rest = continued(b, stmts.collect(), step);
        out.push(js::Statement::new_if_statement(span, branch.test, b.block(taken, span), Some(b.block(rest, span)), &b.ast()));
        return out;
      }
      other => out.push(other),
    }
  }
  out.extend(step.iter().map(|s| b.copy(s)));
  out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
  Continue,
  Return,
}

/// Whether `stmt` is the exit or a block ending in it.
fn ends_in(stmt: &js::Statement, exit: Exit) -> bool {
  let is = |s: &js::Statement| match exit {
    Exit::Continue => matches!(s, js::Statement::ContinueStatement(_)),
    Exit::Return => matches!(s, js::Statement::ReturnStatement(_)),
  };
  match stmt {
    js::Statement::BlockStatement(block) => block.body.last().is_some_and(is),
    other => is(other),
  }
}

/// Whether an `if` ends either branch in a `return`, which splits a pass of a loop that awaits as an `await` does.
fn if_ends_in_return(stmt: &js::Statement) -> bool {
  matches!(stmt, js::Statement::IfStatement(branch) if ends_in(&branch.consequent, Exit::Return) || branch.alternate.as_ref().is_some_and(|alt| ends_in(alt, Exit::Return)))
}

/// Whether `body` leaves the loop from somewhere a pass cannot end: a `break` anywhere or a `continue` or `return` other than at the body's top or ending an `if` there.
fn exits_deeper(body: &[js::Statement]) -> bool {
  body.iter().any(|stmt| match stmt {
    js::Statement::ContinueStatement(_) | js::Statement::ReturnStatement(_) => false,
    js::Statement::IfStatement(branch) if branch.alternate.is_none() && (ends_in(&branch.consequent, Exit::Continue) || ends_in(&branch.consequent, Exit::Return)) => match &branch.consequent {
      js::Statement::BlockStatement(block) => block.body[..block.body.len() - 1].iter().any(loop_exits),
      _ => false,
    },
    other => loop_exits(other),
  })
}

/// Whether `stmt` leaves the loop around it with `break`, `continue` or `return`, outside any function it declares.
fn loop_exits(stmt: &js::Statement) -> bool {
  struct Exits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Exits {
    fn visit_break_statement(&mut self, _: &js::BreakStatement<'a>) {
      self.0 = true;
    }
    fn visit_continue_statement(&mut self, _: &js::ContinueStatement<'a>) {
      self.0 = true;
    }
    fn visit_return_statement(&mut self, _: &js::ReturnStatement<'a>) {
      self.0 = true;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Exits(false);
  oxc_ast_visit::Visit::visit_statement(&mut found, stmt);
  found.0
}

pub(crate) fn awaits(stmt: &js::Statement) -> bool {
  struct Awaits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Awaits {
    fn visit_await_expression(&mut self, _: &js::AwaitExpression<'a>) {
      self.0 = true;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Awaits(false);
  oxc_ast_visit::Visit::visit_statement(&mut found, stmt);
  found.0
}

/// Whether `e` builds a new array, which `sort` and `reverse` may rewrite without another holder seeing it.
fn fresh_array(e: &js::Expression) -> bool {
  match e {
    js::Expression::ArrayExpression(_) => true,
    js::Expression::ParenthesizedExpression(p) => fresh_array(&p.expression),
    js::Expression::CallExpression(call) => match &call.callee {
      js::Expression::StaticMemberExpression(m) => matches!(m.property.name.as_str(), "slice" | "map" | "filter" | "concat" | "toSorted" | "toReversed" | "flat" | "flatMap" | "split" | "keys" | "values" | "entries" | "from"),
      _ => false,
    },
    _ => false,
  }
}

fn root_named(name: &str) -> Option<Root> {
  Some(match name {
    "params" => Root::Params,
    "query" => Root::Query,
    "session" => Root::Session,
    "services" => Root::Services,
    "native" => Root::Native,
    "identity" => Root::Identity,
    "locale" => Root::Locale,
    "locales" => Root::Locales,
    "path" => Root::Path,
    "host" => Root::Host,
    "origin" => Root::Origin,
    "address" => Root::Address,
    "config" => Root::Config,
    "input" => Root::Input,
    "now" => Root::Now,
    _ => return None,
  })
}

/// The `Math` functions `Builtin::Math` computes as V8 does.
pub(crate) const JS_MATH: [&str; 19] = ["sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "asinh", "acosh", "atanh", "exp", "expm1", "log", "log2", "log10", "log1p", "cbrt"];

/// The object at the root of a chain of members: `a` of `a.b[c].d`.
pub(crate) fn member_root<'e>(member: &'e js::MemberExpression<'e>) -> &'e js::Expression<'e> {
  let mut root = member.object();
  while let Some(inner) = root.as_member_expression() {
    root = inner.object();
  }
  root
}

/// Whether `stmts` write a field of one of `params`, an object the caller holds too, so the parameters stay unwritable; rebinding a parameter itself is a local write.
pub(crate) fn writes_a_member_of(stmts: &[js::Statement], params: &[String]) -> bool {
  if heap_pass() {
    return false;
  }
  struct Writes<'p>(&'p [String], bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Writes<'_> {
    fn visit_simple_assignment_target(&mut self, target: &js::SimpleAssignmentTarget<'a>) {
      if let Some(member) = target.as_member_expression() {
        if matches!(member_root(member), js::Expression::Identifier(id) if self.0.iter().any(|p| p == id.name.as_str())) {
          self.1 = true;
        }
      }
      oxc_ast_visit::walk::walk_simple_assignment_target(self, target);
    }
  }
  let mut writes = Writes(params, false);
  stmts.iter().for_each(|stmt| oxc_ast_visit::Visit::visit_statement(&mut writes, stmt));
  writes.1
}

/// A `reduce` callback that writes its accumulator and answers it, `(acc[k] = v, acc)` or `{ acc.k = v; return acc; }`, as one that answers `({ ...acc, [k]: v })`. Only for an accumulator that starts as an object literal, which nothing else holds, so a fresh object answers what the writes would; only the first write may read the accumulator, since a later one would see the earlier write.
fn fresh_accumulator<'a>(b: syntax::Build<'a>, arrow: &js::ArrowFunctionExpression<'_>) -> Option<js::ArrowFunctionExpression<'a>> {
  let acc = first_param(&arrow.params)?.ident()?.name.as_str();
  let mut writes: Vec<&js::AssignmentExpression> = Vec::new();
  let answers_acc = |e: &js::Expression| matches!(strip_parens(e), js::Expression::Identifier(id) if id.name.as_str() == acc);
  match arrow_body(arrow) {
    FunctionBody::Expr(e) => {
      let js::Expression::SequenceExpression(seq) = strip_parens(e) else { return None };
      let (last, rest) = seq.expressions.split_last()?;
      if !answers_acc(last) {
        return None;
      }
      for e in rest {
        let js::Expression::AssignmentExpression(assign) = strip_parens(e) else { return None };
        writes.push(assign);
      }
    }
    FunctionBody::Block(stmts) => {
      let (last, rest) = stmts.split_last()?;
      let js::Statement::ReturnStatement(ret) = last else { return None };
      if !ret.argument.as_ref().is_some_and(|arg| answers_acc(arg)) {
        return None;
      }
      for stmt in rest {
        let js::Statement::ExpressionStatement(stmt) = stmt else { return None };
        let js::Expression::AssignmentExpression(assign) = strip_parens(&stmt.expression) else { return None };
        writes.push(assign);
      }
    }
  }
  struct Reads<'n>(&'n str, bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Reads<'_> {
    fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
      if id.name.as_str() == self.0 {
        self.1 = true;
      }
    }
    fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
      if id.name.as_str() == self.0 {
        self.1 = true;
      }
    }
  }
  let ast = b.ast();
  let mut props = vec![js::ObjectPropertyKind::new_spread_property(arrow.span, b.ident(acc, arrow.span), &ast)];
  for (i, assign) in writes.iter().enumerate() {
    if assign.operator != js::AssignmentOperator::Assign {
      return None;
    }
    let Some(member) = assign.left.as_member_expression() else { return None };
    if !matches!(member.object(), js::Expression::Identifier(id) if id.name.as_str() == acc) {
      return None;
    }
    let (key, computed) = match syntax::prop(member) {
      syntax::MemberProp::Ident(name) => (js::PropertyKey::StaticIdentifier(b.boxed(b.copy(name))), false),
      syntax::MemberProp::Computed(computed) => (js::PropertyKey::from(b.copy(computed)), true),
      syntax::MemberProp::Private(_) => return None,
    };
    if i > 0 {
      let mut reads = Reads(acc, false);
      oxc_ast_visit::Visit::visit_expression(&mut reads, &assign.right);
      if let syntax::MemberProp::Computed(computed) = syntax::prop(member) {
        oxc_ast_visit::Visit::visit_expression(&mut reads, computed);
      }
      if reads.1 {
        return None;
      }
    }
    props.push(js::ObjectPropertyKind::new_object_property(assign.span, js::PropertyKind::Init, key, b.copy(&assign.right), false, false, computed, &ast));
  }
  let object = js::Expression::new_object_expression(arrow.span, b.vec(props), &ast);
  let mut made = b.copy(arrow);
  made.body = js::ArrowFunctionBody::from(b.paren(object, arrow.span));
  Some(made)
}

/// The name a property key spells: an identifier, a string or a number written out, `None` for one computed.
pub(crate) fn prop_name(key: &js::PropertyKey, computed: bool) -> Option<String> {
  if computed {
    return None;
  }
  match key {
    js::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
    js::PropertyKey::StringLiteral(s) => Some(lossy(s.value)),
    js::PropertyKey::NumericLiteral(n) => Some(snapfire_fsr_ir::interp::number_key(n.value)),
    _ => None,
  }
}

/// What a `break` leaves: a loop or a `switch` arm the statement path reads as an `if`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Breakable {
  Loop,
  Switch,
}

/// What the construct is and the rewrite that does the same thing in the IR.
fn describe_stmt(stmt: &js::Statement) -> (&'static str, &'static str) {
  match stmt {
    js::Statement::WhileStatement(_) | js::Statement::DoWhileStatement(_) => ("`while`, a loop whose length the build cannot know", "a body loops over data it already has: `map`, `filter`, `reduce`, `find` and `for...of`"),
    js::Statement::ForStatement(_) | js::Statement::ForInStatement(_) => ("a `for` loop other than `for...of`", "`for...of` over a list, or `map` and `filter`, which the interpreter runs directly"),
    js::Statement::SwitchStatement(_) => ("`switch`", "a chain of `if`, or a ternary when every arm is an expression"),
    js::Statement::FunctionDeclaration(_) => ("a function declared inside a body", "declare it at module level and call it; the build follows a module-level helper"),
    js::Statement::ClassDeclaration(_) => ("a class outside module level", "declare the class at module level or use an object literal"),
    js::Statement::BreakStatement(_) | js::Statement::ContinueStatement(_) => ("`break` or `continue`", "`find` stops at the first match and `filter` selects, so neither needs to leave a loop early"),
    js::Statement::LabeledStatement(_) => ("a label", "a `for...of` in a body runs over every item; `find` and `filter` express the same intent"),
    js::Statement::WithStatement(_) => ("`with`", "read what the body needs from `ctx` by name"),
    js::Statement::DebuggerStatement(_) => ("`debugger`", "the body runs in Rust, where there is nothing to attach to; `fsr test` replays it instead"),
    js::Statement::EmptyStatement(_) => ("an empty statement", "remove the stray `;`"),
    js::Statement::BlockStatement(_) | js::Statement::IfStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::ReturnStatement(_) | js::Statement::ExpressionStatement(_) | js::Statement::ThrowStatement(_) | js::Statement::TryStatement(_) => unreachable!("`stmt` lowers these itself"),
    _ => ("a declaration the build does not read", "a body declares values with `const` or `let`"),
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
fn describe_expr(expr: &js::Expression) -> (&'static str, &'static str) {
  match expr {
    js::Expression::ThisExpression(_) => ("`this`, and a body has no receiver", "read what the body needs from `ctx`"),
    js::Expression::ClassExpression(_) => ("a class outside module level", "declare the class at module level or use an object literal"),
    js::Expression::SequenceExpression(_) => ("a comma expression", "one expression per statement"),
    js::Expression::UpdateExpression(_) => ("`++` or `--` on something this body did not declare", "declare a local with `let` and write that"),
    js::Expression::YieldExpression(_) => ("`yield`", "a body runs once and returns; there is no generator in the serving path"),
    js::Expression::TaggedTemplateExpression(_) => ("a tagged template", "a plain template literal, or a module-level helper the build can follow"),
    js::Expression::JSXElement(_) | js::Expression::JSXFragment(_) => ("JSX in a body", "a body returns data; the page it feeds is where the markup goes"),
    js::Expression::ImportMeta(_) | js::Expression::NewTarget(_) => ("`import.meta`", "the build resolves every import, so nothing reads them while serving"),
    _ => ("an expression outside the IR", "section 2 of snapfire_fsr_lower's API reference lists what a body may say"),
  }
}

#[cfg(test)]
mod declares_unexported_tests {
  #[test]
  fn a_local_component_is_declared_and_unexported() {
    let source = "function Strong({ children }: { children: string }) { return <strong>{children}</strong>; }\nexport function Shown() { return <Strong>x</Strong>; }\nexport default function Page() { return <Shown />; }\n";
    assert!(super::declares_unexported("a.tsx", source, "Strong"));
    assert!(!super::declares_unexported("a.tsx", source, "Shown"), "an exported component needs nothing");
    assert!(!super::declares_unexported("a.tsx", source, "Missing"), "a name the file does not declare is not this case");
  }
}


impl Lowerer<'_> {
  /// A `svelte/store` call, a store being the value it holds when the server
  /// subscribes: `writable(v)`, `readable(v)`, `readonly(s)`, `get(s)` and
  /// `svelte/motion`'s `spring(v)` and `tweened(v)` the value, `derived(source,
  /// fn)` `fn` of the source's value. `crossfade` makes the pair of transitions
  /// only the browser runs and `createEventDispatcher` the function only a
  /// handler calls. A start function
  /// or a `derived` callback that sets the value runs on subscribe, so either
  /// is refused.
  /// The context a key names, under `framework`: a string key as `framework:` and the string, a name as `framework:` and the file and name it is declared under, followed through its import.
  pub(crate) fn context_key(&self, framework: &str, key: &js::Expression) -> Lowered<String> {
    match crate::meaning::bare(key) {
      js::Expression::StringLiteral(s) => Ok(format!("{framework}:{}", lossy(s.value))),
      js::Expression::Identifier(id) => Ok(match self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)).and_then(|meant| meant.origin()) {
        Some((declared, name)) => format!("{framework}:{declared}#{name}"),
        None => format!("{framework}:{}", id.name),
      }),
      other => Err(self.residue(other.span(), "a context key that is not a string or a name")),
    }
  }

  fn svelte_store(&mut self, store: &str, call: &js::CallExpression) -> Lowered<Expr> {
    let arg = |i: usize| call.arguments.get(i).map(syntax::arg_expr);
    match store {
      "writable" | "readable" => {
        if call.arguments.len() > 1 {
          return Err(self.residue(call.span, format!("a `{store}` store with a start function, which sets its value when it is subscribed")));
        }
        match arg(0) {
          Some(value) => self.expr(value),
          None => Ok(Expr::Lit(Lit::Null)),
        }
      }
      "spring" | "tweened" => match arg(0) {
        Some(value) => self.expr(value),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      "createEventDispatcher" => Ok(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)),
      "getContext" => {
        let key = arg(0).ok_or_else(|| self.residue(call.span, "`getContext` without a key"))?;
        let key = self.context_key("svelte", key)?;
        Ok(match self.provided.iter().rev().find(|(provided, _)| *provided == key) {
          Some((_, value)) => value.clone(),
          None => Expr::Context(key),
        })
      }
      "setContext" => Err(self.residue(call.span, "`setContext` inside an expression; it is a statement of the script")),
      "crossfade" => {
        let browser = || Entry::Item(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER));
        Ok(Expr::Array(vec![browser(), browser()]))
      }
      "readonly" | "get" => match arg(0) {
        Some(value) => self.expr(value),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      _ => {
        let (Some(source), Some(callback)) = (arg(0), arg(1)) else {
          return Err(self.residue(call.span, "a `derived` store without its source and callback"));
        };
        let params = match strip_parens(callback) {
          js::Expression::ArrowFunctionExpression(arrow) => params_of(&arrow.params).len(),
          js::Expression::FunctionExpression(f) => params_of(&f.params).len(),
          _ => return Err(self.residue(callback.span(), "a `derived` callback that is not a function written out")),
        };
        if params != 1 {
          return Err(self.residue(callback.span(), "a `derived` callback that sets its value, which runs when the store is subscribed"));
        }
        let f = self.expr(callback)?;
        let source = self.expr(source)?;
        Ok(Expr::Apply { f: Box::new(f), args: vec![source] })
      }
    }
  }
}
