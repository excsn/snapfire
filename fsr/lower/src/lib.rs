//! Reads a TypeScript loader or actions module and lowers each body to the IR.
//! The recognised language is the IR's own; anything outside it is residue,
//! reported with the line and the construct.

pub mod assets;
pub mod component;
mod classes;
mod create_element;
pub mod extract;
mod intl_calls;
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
mod entry;
mod flow;
mod forms;
mod kind;
mod members;
mod parse;
mod lowerer;

pub(crate) use entry::*;
pub(crate) use flow::*;
pub(crate) use forms::*;
pub(crate) use members::*;
pub(crate) use parse::*;

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
/// The typed arrays a body makes as arrays of numbers, each element converted as its type stores it. `Uint8Array` has its own arm, since response bodies and `TextEncoder` read its bytes.
pub(crate) const TYPED_ARRAYS: [&str; 11] = ["Int8Array", "Uint8ClampedArray", "Int16Array", "Uint16Array", "Int32Array", "Uint32Array", "Float16Array", "Float32Array", "Float64Array", "BigInt64Array", "BigUint64Array"];

pub(crate) const ERROR_CONSTRUCTORS: [&str; 9] = ["Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError", "AggregateError", "SuppressedError"];

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
/// The setters of a `Date` a body may call on a local it made, rebinding the local.
pub(crate) const DATE_SETTERS: &[&str] = &["setTime", "setUTCFullYear", "setUTCMonth", "setUTCDate", "setUTCHours", "setUTCMinutes", "setUTCSeconds", "setUTCMilliseconds"];

/// The setters that read this machine's time zone, which a body off a web render path may call the same way.
pub(crate) const LOCAL_DATE_SETTERS: &[&str] = &["setFullYear", "setYear", "setMonth", "setDate", "setHours", "setMinutes", "setSeconds", "setMilliseconds"];

const STANDARD_METHODS: &[&str] = &["flat", "entries", "keys", "values", "forEach", "push", "pop", "shift", "unshift", "splice", "fill", "copyWithin", "findLast", "findLastIndex", "reduceRight", "toSpliced", "with", "normalize", "trimStart", "trimEnd", "hasOwnProperty", "isPrototypeOf", "propertyIsEnumerable", "toPrecision", "toExponential", "then", "catch", "finally", "get", "set", "has", "delete", "add", "clear", "anchor", "bind", "call", "apply", "big", "blink", "bold", "fixed", "fontcolor", "fontsize", "italics", "link", "small", "strike", "sub", "sup", "getYear", "setYear", "setDate", "setFullYear", "setHours", "setMilliseconds", "setMinutes", "setMonth", "setSeconds", "setTime", "setUTCDate", "setUTCFullYear", "setUTCHours", "setUTCMilliseconds", "setUTCMinutes", "setUTCMonth", "setUTCSeconds", "toTemporalInstant", "compile", "union", "intersection", "difference", "symmetricDifference", "isSubsetOf", "isSupersetOf", "isDisjointFrom", "getOrInsert", "getOrInsertComputed"];

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
  static DEFERRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
  static HEAP_ACTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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

/// Whether `source` declares `name` at module level without exporting it, which is a component the build can mount only once the bundle overlay exports it.
pub fn declares_unexported(file: &str, source: &str, name: &str) -> bool {
  let arena = Allocator::default();
  let Ok(parsed) = parse_with(&arena, file, source, jsx_parses(file)) else { return false };
  parsed.local(name).is_some() && !parsed.exports().any(|(exported, _)| exported == name)
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

/// The names a module has bound so far, each with its span and whether a `var` bound it.
struct Bindings(Vec<(String, Span, bool)>);

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
  /// Whether this lowers a module-level constant or function, which a loader and a component may both read.
  pub(crate) module_level: bool,
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
  /// A value of one of `Temporal`'s types, which carries its type at run time.
  Temporal,
  /// A `WeakMap`: a `Map` whose keys are objects, by identity. A body runs to its end without a collection, so nothing it holds goes.
  WeakMap,
  /// A `WeakSet`: a `Set` of objects, by identity.
  WeakSet,
  /// A `WeakRef`: its target, which a body never loses.
  WeakRef,
  /// A `FinalizationRegistry`, whose callback never runs, since nothing is collected while a body runs.
  Registry,
  /// A `Number`, `Boolean` or `String` object made with `new`: an object that reads as the primitive it wraps.
  Wrapper,
  /// An `ArrayBuffer` or a `SharedArrayBuffer`: an array of bytes.
  Buffer,
  /// A `DataView`: the bytes of the buffer it was made over.
  View,
  /// A `DisposableStack` or an `AsyncDisposableStack`: its callbacks, newest last, and whether it was disposed.
  Stack,
  /// A `Proxy`: its target and its handler, whose traps a read, `in`, a write and a `delete` go through.
  Proxy,
  /// An `Intl.Locale`: its fields, with its tag under `$tag`.
  Locale,
}

/// The `Temporal` methods that answer another Temporal value, so a call of one keeps the kind.
pub(crate) const TEMPORAL_MAKERS: &[&str] = &["add", "subtract", "until", "since", "with", "withPlainTime", "withTimeZone", "round", "negated", "abs", "startOfDay", "toInstant", "toPlainDate", "toPlainTime", "toPlainDateTime", "toZonedDateTime", "toZonedDateTimeISO"];

/// The `Temporal` statics that answer a Temporal value.
pub(crate) const TEMPORAL_FROM: &[&str] = &["from", "fromEpochMilliseconds", "fromEpochNanoseconds"];

/// The `Temporal.Now` functions that answer a Temporal value.
pub(crate) const TEMPORAL_NOW: &[&str] = &["instant", "zonedDateTimeISO", "plainDateISO", "plainTimeISO", "plainDateTimeISO"];

/// `Temporal.<type>`'s type name when `e` is one.
pub(crate) fn temporal_type<'a>(e: &'a js::Expression) -> Option<&'a str> {
  let js::Expression::StaticMemberExpression(m) = e.without_parentheses() else { return None };
  match &m.object {
    js::Expression::Identifier(id) if id.name.as_str() == "Temporal" && m.property.name.as_str() != "Now" => Some(m.property.name.as_str()),
    _ => None,
  }
}

/// Whether `e` is `Temporal.Now`.
pub(crate) fn is_temporal_now(e: &js::Expression) -> bool {
  matches!(e.without_parentheses(), js::Expression::StaticMemberExpression(m) if m.property.name.as_str() == "Now" && matches!(&m.object, js::Expression::Identifier(id) if id.name.as_str() == "Temporal"))
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
  Continue,
  Return,
}

/// The `Math` functions `Builtin::Math` computes as V8 does.
pub(crate) const JS_MATH: [&str; 25] = ["sin", "cos", "tan", "asin", "acos", "atan", "atan2", "sinh", "cosh", "tanh", "asinh", "acosh", "atanh", "exp", "expm1", "log", "log2", "log10", "log1p", "cbrt", "hypot", "imul", "clz32", "fround", "f16round"];

/// What a `break` leaves: a loop or a `switch` arm the statement path reads as an `if`.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Breakable {
  Loop,
  Switch,
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
