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
pub use meaning::COMPONENTS_MODULE;
mod placements;
pub mod schema;
pub mod testing;
pub mod svelte;
pub mod terminal;
pub mod vue;

use component::FunctionBody;
use snapfire_fsr_ir::ast::{ArithOp, Body, Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use snapfire_fsr_ir::{standard_reach, Reach};
use swc_core::common::{sync::Lrc, FileName, Globals, Mark, SourceMap, Span, Spanned, SyntaxContext, GLOBALS};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::ast as js;
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

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

/// JavaScript's error constructors, which a `throw` and a `new` read as `{ name, message }`.
pub(crate) const ERROR_CONSTRUCTORS: [&str; 7] = ["Error", "TypeError", "RangeError", "SyntaxError", "ReferenceError", "EvalError", "URIError"];

/// The import aliases every fsr application has, each a prefix and the app
/// directory it stands for. The build writes them into both tsconfigs, snapfirec
/// rewrites them for the browser and the lowerers resolve them here.
pub const ALIASES: &[(&str, &str)] = &[("@app/", ""), ("@routes/", "routes/"), ("@src/", "src/"), ("@schemas/", "schemas/"), ("@generated/", "generated/"), ("@ext/", "ext/")];

/// The client library's standard library module, whose members lower to
/// `Expr::Ext` and whose `native` declares an application's own pair.
pub const STD_SPECIFIER: &str = "@snapfire/fsr-client/std";

/// Where `key` comes from: the client library's store.
pub const STORE_SPECIFIER: &str = "@snapfire/fsr-client/store";

/// The directory under the app whose modules are extensions: every export
/// lowers or the build fails, where a `native` declaration also lives.
pub const EXT_DIR: &str = "ext";

/// What a body lowerer resolves through the component set: module-level
/// names it could not bind and the native pairs declared so far.
#[derive(Debug, Clone, Default)]
pub(crate) struct Resolved {
  pub globals: Vec<(String, Expr)>,
  pub global_kinds: Vec<(String, Kind)>,
  /// The resolved module-level names that hold an `Intl` formatter, with the `new` that made each.
  pub global_formatters: Vec<(String, js::NewExpr)>,
  pub natives: Vec<(String, Reach)>,
  pub classes: Option<std::rc::Rc<std::cell::RefCell<crate::classes::ClassTable>>>,
  pub type_kinds: Vec<(String, AliasKind)>,
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

/// Methods JavaScript gives strings, arrays, numbers, objects and promises that the build does not lower: a call of one is refused by name rather than read as a call of a function value held in a field.
const STANDARD_METHODS: &[&str] = &["flat", "entries", "keys", "values", "forEach", "push", "pop", "shift", "unshift", "splice", "sort", "reverse", "fill", "copyWithin", "lastIndexOf", "findLast", "findLastIndex", "reduceRight", "toSpliced", "with", "charAt", "charCodeAt", "codePointAt", "normalize", "localeCompare", "match", "matchAll", "search", "trimStart", "trimEnd", "toString", "valueOf", "toLocaleUpperCase", "toLocaleLowerCase", "hasOwnProperty", "isPrototypeOf", "propertyIsEnumerable", "toPrecision", "toExponential", "then", "catch", "finally", "get", "set", "has", "delete", "add", "clear", "replaceAll", "anchor", "at", "bind", "call", "apply"];

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

pub(crate) fn lower_loader_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
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
    Exported::Action { .. } | Exported::BadAction(_) | Exported::Value(_) | Exported::Other(_) => return Err((parsed.residue(parsed.module.span, format!("`{export}` must be a function of {takes}")).into(), None)),
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
  let parsed = parse_with(file, source, true)?;
  let unreadable = |span: Span, message: &str| LowerError::Residue(parsed.residue(span, message));
  let Some(found) = component::find_function(&parsed, "useMDXComponents") else {
    return Err(unreadable(parsed.module.span, "no `useMDXComponents` function; export one returning the map, as Next's `mdx-components.tsx` does"));
  };
  let returned: Option<&js::Expr> = match &found {
    component::Found::Declared(_, stmts, _) => stmts.iter().rev().find_map(|stmt| match stmt {
      js::Stmt::Return(ret) => ret.arg.as_deref(),
      _ => None,
    }),
    component::Found::Arrow(arrow) => match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => Some(e),
      js::ArrowFunctionBody::FunctionBody(b) => b.stmts.iter().rev().find_map(|stmt| match stmt {
        js::Stmt::Return(ret) => ret.arg.as_deref(),
        _ => None,
      }),
    },
  };
  let mut expr = returned.ok_or_else(|| unreadable(parsed.module.span, "`useMDXComponents` returns nothing"))?;
  loop {
    expr = match expr {
      js::Expr::Paren(inner) => &inner.expr,
      js::Expr::TsAs(inner) => &inner.expr,
      js::Expr::TsSatisfies(inner) => &inner.expr,
      _ => break,
    };
  }
  let js::Expr::Object(object) = expr else { return Err(unreadable(expr.span(), "`useMDXComponents` returns something other than an object written out")) };
  let mut out = Vec::new();
  for prop in &object.props {
    match prop {
      js::PropOrSpread::Spread(_) => {}
      js::PropOrSpread::Prop(p) => match &**p {
        js::Prop::Shorthand(id) => out.push((id.sym.to_string(), MappedValue::Name(id.sym.to_string()))),
        js::Prop::KeyValue(kv) => {
          let Some(key) = prop_name(&kv.key) else { return Err(unreadable(kv.key.span(), "a computed key in `useMDXComponents`")) };
          let value = match &*kv.value {
            js::Expr::Ident(id) => MappedValue::Name(id.sym.to_string()),
            other => MappedValue::Expr(parsed.range(other.span())),
          };
          out.push((key, value));
        }
        other => return Err(unreadable(other.span(), "an entry in `useMDXComponents` other than `key: Component`")),
      },
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
    let body = lower_function(&mut lowerer, first, body).map_err(|r| (r.into(), lowerer.unbound.take()))?;
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

pub(crate) struct Parsed {
  pub(crate) file: String,
  cm: Lrc<SourceMap>,
  pub(crate) module: js::Module,
  /// The syntax context swc's resolver gave the module's top-level bindings: an identifier carrying it names an import or a module-level declaration, and one carrying any other names a local.
  pub(crate) top: SyntaxContext,
  /// The syntax context of a name nothing in the module binds.
  pub(crate) unresolved: SyntaxContext,
  /// What the module's names mean, filled when a component set resolves the file.
  pub(crate) meanings: std::cell::OnceCell<std::rc::Rc<meaning::Meanings>>,
  /// The file the module was written from, when it was not written by hand.
  pub(crate) origin: Option<std::rc::Rc<Origin>>,
}

pub(crate) enum Exported<'a> {
  Function(Option<&'a js::Pat>, &'a [js::Stmt]),
  /// An arrow whose body is one expression.
  Expr(Option<&'a js::Pat>, &'a js::Expr),
  /// `action(<function>)`, the function an arrow with either body or a function expression.
  Action { input: Option<String>, first: Option<&'a js::Pat>, body: FunctionBody<'a> },
  /// `action(<something that is not a function>)`, which no lowering accepts.
  BadAction(Span),
  /// An export the build tried to read and could not classify, as against one
  /// it classified as something else. Never skipped in silence.
  Unreadable(Span, &'static str),
  /// An object literal, which `meta` may be when it reads nothing from the route's data.
  Value(&'a js::Expr),
  Other(Span),
}


/// Every name a pattern binds, for a declaration shape the build does not
/// otherwise read: the names are reported so nothing is skipped unseen.
fn bound_names<'a>(pat: &'a js::Pat, out: &mut impl FnMut(&'a str, Span)) {
  match pat {
    js::Pat::Ident(id) => out(id.id.sym.as_ref(), id.id.span),
    js::Pat::Array(arr) => arr.elems.iter().flatten().for_each(|p| bound_names(p, out)),
    js::Pat::Rest(rest) => bound_names(&rest.arg, out),
    js::Pat::Assign(assign) => bound_names(&assign.left, out),
    js::Pat::Object(obj) => {
      for prop in &obj.props {
        match prop {
          js::ObjectPatProp::KeyValue(kv) => bound_names(&kv.value, out),
          js::ObjectPatProp::Assign(a) => out(a.key.id.sym.as_ref(), a.key.id.span),
          js::ObjectPatProp::Rest(rest) => bound_names(&rest.arg, out),
        }
      }
    }
    js::Pat::Invalid(_) | js::Pat::Expr(_) => {}
  }
}

/// Whether `source` declares `name` at module level without exporting it, which is a component the build can mount only once the bundle overlay exports it.
pub fn declares_unexported(file: &str, source: &str, name: &str) -> bool {
  let Ok(parsed) = parse_with(file, source, jsx_parses(file)) else { return false };
  parsed.local(name).is_some() && !parsed.exports().any(|(exported, _)| exported == name)
}

/// Whether `file` may hold JSX, as TypeScript decides: a `.tsx` or any
/// JavaScript file. A `.ts` cannot, since there `<T>x` is a type assertion.
pub(crate) fn jsx_parses(file: &str) -> bool {
  [".tsx", ".jsx", ".js", ".mjs", ".cjs"].iter().any(|ext| file.ends_with(ext))
}

pub(crate) fn parse(file: &str, source: &str) -> Result<Parsed, LowerError> {
  parse_with(file, source, false)
}

pub(crate) fn parse_with(file: &str, source: &str, tsx: bool) -> Result<Parsed, LowerError> {
  parse_from(file, source, tsx, None)
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

pub(crate) fn parse_from(file: &str, source: &str, tsx: bool, origin: Option<std::rc::Rc<Origin>>) -> Result<Parsed, LowerError> {
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Custom(file.to_owned())), source.to_owned());
  let syntax = Syntax::Typescript(TsSyntax { tsx, decorators: true, ..Default::default() });
  let lexer = Lexer::new(syntax, js::EsVersion::latest(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  let module = parser.parse_module().map_err(|e| {
    let (line, column) = match &origin {
      Some(origin) => origin.position((e.span().lo.0 - fm.start_pos.0) as usize),
      None => {
        let loc = cm.lookup_char_pos(e.span().lo);
        (loc.line, loc.col_display + 1)
      }
    };
    LowerError::Parse { file: file.to_owned(), message: format!("{line}:{column}: {}", e.kind().msg()) }
  })?;
  let (module, top, unresolved) = GLOBALS.set(&Globals::new(), || {
    let unresolved = Mark::new();
    let top_level = Mark::new();
    let program = js::Program::Module(module).apply(resolver(unresolved, top_level, true));
    let js::Program::Module(mut module) = program else { unreachable!("a module resolves to a module") };
    let top = SyntaxContext::empty().apply_mark(top_level);
    create_element::desugar(&mut module, top);
    methods::desugar(&mut module);
    (module, top, SyntaxContext::empty().apply_mark(unresolved))
  });
  Ok(Parsed { file: file.to_owned(), cm, module, top, unresolved, meanings: std::cell::OnceCell::new(), origin })
}

impl Parsed {
  /// Parses `source` as one TypeScript expression written at `line` and
  /// `column` of this file. It joins the file's own source map padded to
  /// that position, so a residue inside it names where the expression sits.
  pub(crate) fn parse_expr_at(&self, source: &str, line: usize, column: usize) -> Result<Box<js::Expr>, String> {
    let padded = format!("{}{}{source}", "\n".repeat(line.saturating_sub(1)), " ".repeat(column.saturating_sub(1)));
    let fm = self.cm.new_source_file(Lrc::new(FileName::Custom(self.file.clone())), padded);
    let syntax = Syntax::Typescript(TsSyntax { tsx: false, decorators: false, ..Default::default() });
    let lexer = Lexer::new(syntax, js::EsVersion::latest(), StringInput::from(&*fm), None);
    let mut parser = Parser::new_from(lexer);
    parser.parse_expr().map_err(|e| e.kind().msg().to_string())
  }

  /// The byte range of `span` in the file's text.
  pub(crate) fn range(&self, span: Span) -> std::ops::Range<usize> {
    let lo = self.cm.lookup_byte_offset(span.lo).pos.0 as usize;
    let hi = self.cm.lookup_byte_offset(span.hi).pos.0 as usize;
    lo..hi
  }

  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    let (line, column) = self.loc(span.lo);
    Residue { file: self.file.clone(), line, column, message: message.into(), hint: None, via: Vec::new() }
  }

  /// The one-based line and column `pos` names, in the file the author wrote.
  pub(crate) fn loc(&self, pos: swc_core::common::BytePos) -> (usize, usize) {
    if let Some(origin) = &self.origin {
      let start = self.cm.files().first().map(|f| f.start_pos).unwrap_or_default();
      return origin.position(pos.0.saturating_sub(start.0) as usize);
    }
    let loc = self.cm.lookup_char_pos(pos);
    (loc.line, loc.col_display + 1)
  }

  /// The one-based line and column of a byte offset in the file's text.
  /// The byte offset of a 1-based line and column as [`Parsed::position`] counts them.
  pub(crate) fn offset(&self, line: usize, column: usize) -> Option<usize> {
    let source = self.cm.files().first()?.src.clone();
    let mut at = 0;
    for (i, text) in source.split_inclusive('\n').enumerate() {
      if i + 1 == line {
        return Some(at + text.char_indices().nth(column.saturating_sub(1)).map(|(byte, _)| byte).unwrap_or(0));
      }
      at += text.len();
    }
    None
  }

  pub(crate) fn position(&self, offset: usize) -> (usize, usize) {
    let start = self.cm.files().first().map(|f| f.start_pos).unwrap_or_default();
    self.loc(start + swc_core::common::BytePos(offset as u32))
  }

  pub(crate) fn exports(&self) -> impl Iterator<Item = (&str, Exported<'_>)> {
    let mut out: Vec<(&str, Exported<'_>)> = Vec::new();
    for item in &self.module.body {
      match item {
        js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => self.declared(&export.decl, &mut out),
        js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportNamed(named)) if !named.type_only => {
          for spec in &named.specifiers {
            let js::ExportSpecifier::Named(spec) = spec else { continue };
            if spec.is_type_only {
              continue;
            }
            let js::ModuleExportName::Ident(orig) = &spec.orig else { continue };
            let name = match &spec.exported {
              Some(js::ModuleExportName::Ident(id)) => id.sym.as_ref(),
              Some(js::ModuleExportName::Str(_)) => continue,
              None => orig.sym.as_ref(),
            };
            match self.local(orig.sym.as_ref()).filter(|_| named.src.is_none()) {
              Some(exported) => out.push((name, exported)),
              None => out.push((name, Exported::Unreadable(spec.span, "exported from a name this module does not declare"))),
            }
          }
        }
        _ => {}
      }
    }
    out.into_iter()
  }

  /// Every name a declaration exports, in source order.
  fn declared<'a>(&'a self, decl: &'a js::Decl, out: &mut Vec<(&'a str, Exported<'a>)>) {
    match decl {
      js::Decl::Fn(f) => {
        let first = f.function.params.first().map(|p| &p.pat);
        let body = f.function.body.as_ref().map(|b| b.stmts.as_slice()).unwrap_or(&[]);
        out.push((f.ident.sym.as_ref(), Exported::Function(first, body)));
      }
      js::Decl::Var(var) => {
        for d in &var.decls {
          let js::Pat::Ident(name) = &d.name else {
            bound_names(&d.name, &mut |name, span| out.push((name, Exported::Unreadable(span, "bound by a destructuring the build does not read"))));
            continue;
          };
          let Some(init) = d.init.as_deref() else { continue };
          out.push((name.id.sym.as_ref(), self.classify(init)));
        }
      }
      _ => {}
    }
  }

  /// What a module-level declaration of `name` is, for an `export { name }`
  /// that stands apart from it.
  fn local(&self, name: &str) -> Option<Exported<'_>> {
    let mut found = Vec::new();
    for item in &self.module.body {
      let decl = match item {
        js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
        js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => &export.decl,
        _ => continue,
      };
      self.declared(decl, &mut found);
    }
    found.into_iter().find(|(n, _)| *n == name).map(|(_, e)| e)
  }
}

impl Parsed {
  /// Whether `callee` names `want` from the server tier, however the module
  /// reaches it; with no meanings resolved, the bare name nothing binds.
  pub(crate) fn names_server(&self, callee: &js::Callee, want: &str) -> bool {
    match self.meanings.get() {
      Some(meanings) => meanings.names_server(callee, want),
      None => matches!(callee, js::Callee::Expr(expr) if matches!(&**expr, js::Expr::Ident(id) if id.ctxt == self.unresolved && id.sym.as_ref() == want)),
    }
  }

  fn classify<'a>(&'a self, init: &'a js::Expr) -> Exported<'a> {
    classify_in(self, init)
  }
}

fn classify_in<'a>(parsed: &'a Parsed, init: &'a js::Expr) -> Exported<'a> {
  match init {
    js::Expr::Paren(inner) => classify_in(parsed, &inner.expr),
    js::Expr::TsAs(inner) => classify_in(parsed, &inner.expr),
    js::Expr::TsSatisfies(inner) => classify_in(parsed, &inner.expr),
    js::Expr::Fn(f) => Exported::Function(f.function.params.first().map(|p| &p.pat), f.function.body.as_ref().map(|b| b.stmts.as_slice()).unwrap_or(&[])),
    js::Expr::Arrow(arrow) => match &*arrow.body {
      js::ArrowFunctionBody::FunctionBody(b) => Exported::Function(arrow.params.first(), &b.stmts),
      js::ArrowFunctionBody::Expr(e) => Exported::Expr(arrow.params.first(), e),
    },
    js::Expr::Call(call) => {
      if !parsed.names_server(&call.callee, "action") {
        return Exported::Unreadable(call.span, "a call the build does not recognise");
      }
      match call.args.last().map(|a| &*a.expr) {
        Some(js::Expr::Arrow(arrow)) => {
          let first = arrow.params.first();
          let body = match &*arrow.body {
            js::ArrowFunctionBody::FunctionBody(b) => FunctionBody::Block(&b.stmts),
            js::ArrowFunctionBody::Expr(e) => FunctionBody::Expr(e),
          };
          Exported::Action { input: action_input(call, first), first, body }
        }
        Some(js::Expr::Fn(f)) => {
          let first = f.function.params.first().map(|p| &p.pat);
          let body = f.function.body.as_ref().map(|b| b.stmts.as_slice()).unwrap_or(&[]);
          Exported::Action { input: action_input(call, first), first, body: FunctionBody::Block(body) }
        }
        Some(other) => Exported::BadAction(other.span()),
        None => Exported::BadAction(call.span),
      }
    }
    js::Expr::Object(_) => Exported::Value(init),
    other => Exported::Other(other.span()),
  }
}

/// The input type of an `action(...)`: the call's type argument, else what
/// the function's first parameter names through `ActionCtx<T>`.
fn action_input(call: &js::CallExpr, first: Option<&js::Pat>) -> Option<String> {
  call.type_args.as_ref().and_then(|t| t.params.first()).and_then(|t| type_ref_name(t)).or_else(|| first.and_then(action_ctx_input))
}

/// Binds the context parameter and lowers a function body: a block as it
/// stands, an expression as the value returned.
fn lower_function(lowerer: &mut Lowerer<'_>, first: Option<&js::Pat>, body: FunctionBody<'_>) -> Lowered<Body> {
  lowerer.bind_ctx(first)?;
  match body {
    FunctionBody::Block(stmts) => lowerer.block(stmts),
    FunctionBody::Expr(expr) => lowerer.expr(expr).map(|e| vec![Stmt::Return(e)]),
  }
}

fn is_bitwise(op: js::BinaryOp) -> bool {
  matches!(op, js::BinaryOp::BitAnd | js::BinaryOp::BitOr | js::BinaryOp::BitXor | js::BinaryOp::LShift | js::BinaryOp::RShift | js::BinaryOp::ZeroFillRShift)
}

fn type_ref_name(ty: &js::TsType) -> Option<String> {
  match ty {
    js::TsType::TsTypeRef(r) => match &r.type_name {
      js::TsEntityName::Ident(id) => Some(id.sym.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// The input type an action's parameter names: `ActionCtx<AddToCart>` on
/// `ctx` or on the destructuring of it.
fn action_ctx_input(param: &js::Pat) -> Option<String> {
  let ann = match param {
    js::Pat::Ident(id) => id.type_ann.as_ref()?,
    js::Pat::Object(obj) => obj.type_ann.as_ref()?,
    _ => return None,
  };
  let js::TsType::TsTypeRef(r) = &*ann.type_ann else { return None };
  match &r.type_name {
    js::TsEntityName::Ident(id) if id.sym.as_ref() == "ActionCtx" => {}
    _ => return None,
  }
  r.type_params.as_ref().and_then(|p| p.params.first()).and_then(|t| type_ref_name(t))
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
  parsed: &'a Parsed,
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
  /// The kinds of a component's props, from the props parameter's type or from the placements that pass them, by prop name.
  pub(crate) prop_kinds: Vec<(String, Kind)>,
  /// The props whose kind the lowering read, so a placement passing another kind for one is a conflict rather than a silent wrong constant.
  pub(crate) consulted: std::cell::RefCell<Vec<String>>,
  /// Names bound to an `Intl` formatter kept as a value, with the binding noted against and the `new` that made it, so `f.format(x)` lowers as the inline form would.
  pub(crate) formatters: Vec<(String, Expr, js::NewExpr)>,
  /// The resolved module-level names that hold an `Intl` formatter.
  pub(crate) global_formatters: Vec<(String, js::NewExpr)>,
  /// Why the last residue was a value only the browser knows, set by the site that refused it. A consumer that can leave the value to the browser, text under an element, a prop into a placement, a binding, takes it and lowers null in its place; anywhere else the residue stands.
  pub(crate) browser_value: Option<String>,
  /// The reaches this page may use besides `Render` and `Body`: [`Reach::Terminal`], [`Reach::Later`] and [`Reach::Process`] for a terminal page, none for a web page.
  pub(crate) reaches: Vec<Reach>,
  /// What the module-level names of a component's file mean, set by the component lowerer.
  pub(crate) meanings: Option<std::rc::Rc<meaning::Meanings>>,
  /// The parameter a `derive` compute reads its sources through: a call of it is a store read.
  pub(crate) reader: Option<js::Id>,
  /// A component's setters, dispatchers and inner functions, which only the browser calls. Read as a value on the server one is `render::BROWSER_HANDLER`, a function that renders nothing, as React renders nothing for one.
  pub(crate) browser_functions: Vec<String>,
  /// The functions a component's body declares, by name; a call to one in an expression applies it as a lambda made where the call stands.
  pub(crate) local_fns: Vec<(String, js::Function)>,
  /// The application's functions whose every statement the browser alone runs, so a call to one as a statement does nothing on the server.
  pub(crate) browser_effects: Vec<String>,
  /// Names held in a cell, `Expr::Cell` or `Expr::ModuleCell`, because a function writes them, each with the expression for its handle: a write in a block is a `WriteCell` through it.
  pub(crate) cells: Vec<(String, Expr)>,
  /// One frame per open fold step or function path: the lets a declaration whose value reads the clock or randomness is bound by, so it is read once where it is declared.
  pub(crate) held: Vec<Vec<Stmt>>,
  /// For each frame in `held`, where its conditions start in `rebind_reach`.
  pub(crate) held_reach: Vec<usize>,
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
    }
  }

  /// The variant tag the value travels under into an island, so the browser makes the real type; a `URLSearchParams` is a string on both sides.
  pub(crate) fn tag(self) -> Option<&'static str> {
    use snapfire_fsr_ir::render::typed;
    match self {
      Kind::Map => Some(typed::MAP),
      Kind::Set => Some(typed::SET),
      Kind::Query | Kind::Browser | Kind::Response => None,
      Kind::Url => Some(typed::URL),
      Kind::Date => Some(typed::DATE),
      Kind::BigInt => Some(typed::BIGINT),
    }
  }

  /// The kind a type annotation names: `Map<K, V>`, `Set<T>`, `Date`, `URLSearchParams`, `URL` or `bigint`.
  pub(crate) fn of_type(ty: &js::TsType) -> Option<Kind> {
    Kind::of_type_in(ty, &[])
  }

  /// The kind `ty` names, an alias in `aliases` read through to what it stands for, so `Unique<URL>` for `type Unique<T> = T & Brand` is a `URL`.
  pub(crate) fn of_type_in(ty: &js::TsType, aliases: &[(String, AliasKind)]) -> Option<Kind> {
    match ty {
      js::TsType::TsParenthesizedType(p) => Kind::of_type_in(&p.type_ann, aliases),
      js::TsType::TsUnionOrIntersectionType(js::TsUnionOrIntersectionType::TsIntersectionType(i)) => i.types.iter().find_map(|t| Kind::of_type_in(t, aliases)),
      js::TsType::TsKeywordType(k) if k.kind == js::TsKeywordTypeKind::TsBigIntKeyword => Some(Kind::BigInt),
      js::TsType::TsTypeRef(r) => match &r.type_name {
        js::TsEntityName::Ident(id) => match id.sym.as_ref() {
          "Map" | "ReadonlyMap" => Some(Kind::Map),
          "Set" | "ReadonlySet" => Some(Kind::Set),
          "Date" => Some(Kind::Date),
          "URLSearchParams" => Some(Kind::Query),
          "URL" => Some(Kind::Url),
          name => match aliases.iter().find(|(n, _)| n == name).map(|(_, kind)| *kind)? {
            AliasKind::Fixed(kind) => Some(kind),
            AliasKind::Param(i) => Kind::of_type_in(r.type_params.as_ref()?.params.get(i)?, aliases),
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
  pub(crate) fn new(parsed: &'a Parsed, defaults: &'a SessionDefaults) -> Self {
    Self { parsed, defaults, roots: Vec::new(), scope: Vec::new(), premade: std::collections::HashMap::new(), globals: Vec::new(), global_kinds: Vec::new(), unbound: None, middleware: false, theming: false, meta: false, extends: false, redirects: false, hoisting: None, natives: Vec::new(), in_handler: false, render_path: false, reach_violation: false, temps: 0, kinds: Vec::new(), prop_kinds: Vec::new(), consulted: std::cell::RefCell::new(Vec::new()), formatters: Vec::new(), global_formatters: Vec::new(), browser_value: None, reaches: Vec::new(), meanings: parsed.meanings.get().cloned(), reader: None, browser_functions: Vec::new(), local_fns: Vec::new(), unfollowed: Vec::new(), browser_effects: Vec::new(), provided: Vec::new(), cells: Vec::new(), held: Vec::new(), held_reach: Vec::new(), rebind_reach: Vec::new(), settlers: Vec::new(), continuations: Vec::new(), spawned: Vec::new(), async_floor: None, wrapped_returns: false, markup_base: None, markup_returns: Vec::new(), classes: None, class_parent: None, this_kinds: Vec::new(), dispatching: Vec::new(), type_kinds: Vec::new() }
  }

  pub(crate) fn resolved(mut self, resolved: &Resolved) -> Self {
    self.globals = resolved.globals.clone();
    self.global_kinds = resolved.global_kinds.clone();
    self.global_formatters = resolved.global_formatters.clone();
    self.natives = resolved.natives.clone();
    self.classes = resolved.classes.clone();
    self.type_kinds = resolved.type_kinds.clone();
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

  pub(crate) fn literal(&mut self, expr: &js::Expr) -> Lowered<Expr> {
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
  fn bind_ctx(&mut self, first: Option<&js::Pat>) -> Lowered<()> {
    let Some(first) = first else { return Ok(()) };
    match first {
      js::Pat::Ident(id) => {
        self.roots.push((id.id.sym.to_string(), Root::Ctx));
        Ok(())
      }
      js::Pat::Object(obj) => {
        for prop in &obj.props {
          match prop {
            js::ObjectPatProp::Assign(a) => {
              let name = a.key.id.sym.to_string();
              let root = self.root_named(&name).ok_or_else(|| self.residue(a.span, format!("`{name}` is not a field of the context")))?;
              self.roots.push((name, root));
            }
            js::ObjectPatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key).ok_or_else(|| self.residue(kv.key.span(), "a computed context field"))?;
              let js::Pat::Ident(local) = &*kv.value else {
                return Err(self.residue(kv.value.span(), "a nested destructuring of the context"));
              };
              let root = self.root_named(&key).ok_or_else(|| self.residue(kv.key.span(), format!("`{key}` is not a field of the context")))?;
              self.roots.push((local.id.sym.to_string(), root));
            }
            js::ObjectPatProp::Rest(r) => return Err(self.residue(r.span, "a rest of the context")),
          }
        }
        Ok(())
      }
      other => Err(self.residue(other.span(), "the context parameter must be `ctx` or a destructuring")),
    }
  }

  /// Whether `init` is the context parameter itself, so a declaration of it binds as the parameter's own destructuring does.
  fn is_ctx(&self, init: Option<&js::Expr>) -> bool {
    matches!(init, Some(js::Expr::Ident(id)) if matches!(self.root_of(id), Some(Root::Ctx)))
  }

  /// `params`, `query`, `session` or `config`, by name or as a field of the context: a root read only by key.
  fn keyed_root(&self, init: &js::Expr) -> Option<Root> {
    let keyed = |root: Root| matches!(root, Root::Params | Root::Query | Root::Session | Root::Config).then_some(root);
    match init {
      js::Expr::Paren(p) => self.keyed_root(&p.expr),
      js::Expr::Ident(id) if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) => self.root_of(id).and_then(keyed),
      js::Expr::Member(m) => {
        let (js::Expr::Ident(obj), js::MemberProp::Ident(field)) = (&*m.obj, &m.prop) else { return None };
        if self.scope.iter().any(|(n, _)| n == obj.sym.as_ref()) || !matches!(self.root_of(obj), Some(Root::Ctx)) {
          return None;
        }
        root_named(field.sym.as_ref()).and_then(keyed)
      }
      _ => None,
    }
  }

  /// `const { id, slug: s = "a" } = params`: each name bound to its key's read.
  fn bind_keyed(&mut self, fields: &js::ObjectPat, root: Root) -> Lowered<()> {
    let read = |lowerer: &Self, key: String| match root {
      Root::Params => Expr::Param(key),
      Root::Query => Expr::Query(key),
      Root::Session => lowerer.session_read(key),
      _ => Expr::Config(key),
    };
    for field in &fields.props {
      match field {
        js::ObjectPatProp::Assign(a) => {
          let name = a.key.id.sym.to_string();
          let bound = match &a.value {
            Some(default) => Expr::Coalesce(Box::new(read(self, name.clone())), Box::new(self.expr(default)?)),
            None => read(self, name.clone()),
          };
          self.scope.push((name, bound));
        }
        js::ObjectPatProp::KeyValue(kv) => {
          let key = prop_name(&kv.key).ok_or_else(|| self.residue(kv.key.span(), "a computed key in a pattern"))?;
          let value = read(self, key);
          crate::component::bind_pattern(self, &kv.value, value)?;
        }
        js::ObjectPatProp::Rest(r) => return Err(self.residue(r.span, "a rest of a value read only by key")),
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

  pub(crate) fn block(&mut self, stmts: &[js::Stmt]) -> Lowered<Body> {
    let depth = self.scope.len();
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
      self.lift_updates(stmt, &mut out)?;
      match stmt {
        js::Stmt::Decl(js::Decl::Var(var)) => {
          for decl in &var.decls {
            if self.is_ctx(decl.init.as_deref()) {
              self.bind_ctx(Some(&decl.name))?;
              continue;
            }
            if let (js::Pat::Object(fields), Some(root)) = (&decl.name, decl.init.as_deref().and_then(|init| self.keyed_root(init))) {
              self.bind_keyed(fields, root)?;
              continue;
            }
            out.push(self.declare(decl)?);
          }
        }
        js::Stmt::Switch(sw) => out.extend(self.switch(sw)?),
        stmt => out.push(self.stmt(stmt)?),
      }
    }
    self.scope.truncate(depth);
    Ok(out)
  }

  /// `switch` as its discriminant under a temporary and an `if` chain over the arms, each compared with `===`.
  fn switch(&mut self, sw: &js::SwitchStmt) -> Lowered<Vec<Stmt>> {
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
  pub(crate) fn kind_of(&self, e: &js::Expr) -> Option<Kind> {
    match e {
      js::Expr::Paren(p) => self.kind_of(&p.expr),
      js::Expr::Await(a) => self.kind_of(&a.arg),
      js::Expr::Call(call) if self.calls_next(call) => Some(Kind::Response),
      js::Expr::TsAs(a) => self.kind_of(&a.expr),
      js::Expr::TsNonNull(a) => self.kind_of(&a.expr),
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
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
      js::Expr::Call(call) => match &call.callee {
        js::Callee::Expr(callee) => match &**callee {
          js::Expr::Member(m) if self.kind_of(&m.obj) == Some(Kind::Browser) => Some(Kind::Browser),
          js::Expr::Ident(f) if !self.scope.iter().any(|(n, _)| n == f.sym.as_ref()) => {
            let returned = returned_by(f.sym.as_ref());
            self.global_kinds.iter().find(|(n, _)| *n == returned).map(|(_, kind)| *kind).or_else(|| made_kind(e))
          }
          _ => made_kind(e),
        },
        _ => None,
      },
      js::Expr::Member(m) if self.kind_of(&m.obj) == Some(Kind::Browser) => Some(Kind::Browser),
      js::Expr::Member(m) if self.kind_of(&m.obj) == Some(Kind::Url) && matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "searchParams") => Some(Kind::Query),
      js::Expr::Member(m) if matches!((&*m.obj, &m.prop), (js::Expr::Ident(this), js::MemberProp::Ident(_)) if this.sym.as_ref() == crate::classes::THIS) => {
        let js::MemberProp::Ident(field) = &m.prop else { return None };
        self.this_kinds.iter().find(|(n, _)| n == field.sym.as_ref()).map(|(_, kind)| *kind)
      }
      js::Expr::Member(m) => {
        let js::Expr::Ident(obj) = &*m.obj else { return None };
        let js::MemberProp::Ident(prop) = &m.prop else { return None };
        let (_, bound) = self.scope.iter().rev().find(|(n, _)| n == obj.sym.as_ref())?;
        if *bound != Expr::Var("$props".to_owned()) {
          return None;
        }
        let kind = self.prop_kinds.iter().find(|(k, _)| k == prop.sym.as_ref()).map(|(_, kind)| *kind)?;
        self.consulted.borrow_mut().push(prop.sym.to_string());
        Some(kind)
      }
      other => made_kind(other),
    }
  }

  /// `x instanceof Map`, `Set`, `Date` or `Array`: a constant where the build knows what `x` holds, `Array.isArray` for `Array`.
  fn instance_of(&mut self, bin: &js::BinExpr) -> Lowered<Expr> {
    if let Some(test) = self.class_instance_of(&bin.left, &bin.right)? {
      return Ok(test);
    }
    let js::Expr::Ident(class) = &*bin.right else { return Err(self.residue(bin.span, "`instanceof` of a computed class")) };
    let class = class.sym.as_ref();
    if self.scope.iter().any(|(n, _)| n == class) || self.globals.iter().any(|(n, _)| n == class) {
      return Err(self.residue(bin.span, format!("`instanceof {class}`, a class this file declares")));
    }
    if class == "Array" {
      return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(&bin.left)?] });
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
  pub(crate) fn formatter_of(&self, e: &js::Expr) -> Option<js::NewExpr> {
    match strip_parens(e) {
      js::Expr::TsAs(a) => self.formatter_of(&a.expr),
      js::Expr::TsNonNull(a) => self.formatter_of(&a.expr),
      js::Expr::New(n) if intl_formatter(n).is_some() => Some(n.clone()),
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
        match self.scope.iter().rev().find(|(n, _)| n == name) {
          Some((_, bound)) => self.formatters.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, made)| made.clone()),
          None => self.global_formatters.iter().find(|(n, _)| n == name).map(|(_, made)| made.clone()),
        }
      }
      _ => None,
    }
  }

  /// Binds `name` to an `Intl` formatter `init` makes, so a later `.format(x)` on it lowers as the inline form; the name reads as null anywhere else.
  pub(crate) fn bind_formatter(&mut self, name: &str, init: &js::Expr) -> bool {
    let js::Expr::New(made) = strip_parens(init) else { return false };
    if intl_formatter(made).is_none() {
      return false;
    }
    self.scope.push((name.to_owned(), Expr::Lit(Lit::Null)));
    self.formatters.push((name.to_owned(), Expr::Lit(Lit::Null), made.clone()));
    true
  }

  /// Notes `name`, just bound to `init`, as a collection when `init` is one.
  pub(crate) fn note_kind(&mut self, name: &str, init: &js::Expr) {
    let Some(kind) = self.kind_of(init) else { return };
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()) else { return };
    self.kinds.push((name.to_owned(), bound, kind));
  }

  /// One declarator as a `let`: a name binds the value; a pattern binds it under a temporary and each name it declares reads a part of that.
  fn declare(&mut self, decl: &js::VarDeclarator) -> Lowered<Stmt> {
    let Some(init) = &decl.init else {
      return Err(self.residue(decl.span, "a declaration without a value"));
    };
    if let js::Pat::Ident(name) = &decl.name {
      if self.bind_formatter(name.id.sym.as_ref(), init) {
        return Ok(Stmt::Let { name: name.id.sym.to_string(), expr: Expr::Lit(Lit::Null) });
      }
    }
    let expr = self.expr(init)?;
    let name = match &decl.name {
      js::Pat::Ident(name) => {
        let name = name.id.sym.to_string();
        self.scope.push((name.clone(), Expr::Var(name.clone())));
        self.note_kind(&name, init);
        name
      }
      pattern => {
        let name = self.temp();
        crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
        name
      }
    };
    Ok(Stmt::Let { name, expr })
  }

  pub(crate) fn stmt(&mut self, stmt: &js::Stmt) -> Lowered<Stmt> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        if var.decls.len() != 1 {
          return Err(self.residue(var.span, "one binding per declaration"));
        }
        self.declare(&var.decls[0])
      }
      js::Stmt::If(if_stmt) => {
        if let Some((kind, message)) = self.as_fail(&if_stmt.cons) {
          if if_stmt.alt.is_some() {
            return Err(self.residue(if_stmt.span, "an `else` after `fail`"));
          }
          let cond = self.expr(&if_stmt.test)?;
          let (kind, message) = (kind?, message?);
          return Ok(Stmt::Guard { cond, kind, message });
        }
        let cond = self.expr(&if_stmt.test)?;
        let then = self.branch(&if_stmt.cons)?;
        let r#else = match &if_stmt.alt {
          Some(alt) => self.branch(alt)?,
          None => Vec::new(),
        };
        Ok(Stmt::If { cond, then, r#else })
      }
      js::Stmt::ForOf(for_of) => {
        let js::ForHead::VarDecl(decl) = &for_of.left else {
          return Err(self.residue(for_of.span, "`for...of` must declare its variable"));
        };
        let Some(pattern) = decl.decls.first().map(|d| &d.name) else {
          return Err(self.residue(decl.span, "`for...of` must declare its variable"));
        };
        let mut over = self.expr(&for_of.right)?;
        if self.kind_of(&for_of.right) == Some(Kind::Map) {
          over = Expr::Entries(Box::new(over));
        }
        let depth = self.scope.len();
        let name = match pattern {
          js::Pat::Ident(name) => {
            let name = name.id.sym.to_string();
            self.scope.push((name.clone(), Expr::Var(name.clone())));
            name
          }
          pattern => {
            let name = self.temp();
            crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
            name
          }
        };
        let body = self.branch(&for_of.body);
        self.scope.truncate(depth);
        Ok(Stmt::ForOf { name, over, body: body? })
      }
      js::Stmt::Return(ret) if self.markup_base.is_some() => {
        let base = self.markup_base.expect("checked");
        let path = self.markup_returns.len();
        let mut fields = vec![Entry::Field("$path".to_owned(), Expr::Lit(Lit::Int(path as i128)))];
        fields.extend(self.scope[base..].iter().enumerate().map(|(k, (_, expr))| Entry::Field(format!("${k}"), expr.clone())));
        let names = self.scope[base..].iter().map(|(name, _)| name.clone()).collect();
        self.markup_returns.push((ret.span.lo.0, names));
        Ok(Stmt::Return(Expr::Object(fields)))
      }
      js::Stmt::Return(ret) => match &ret.arg {
        Some(arg) => Ok(Stmt::Return(self.expr(arg)?)),
        None => Ok(Stmt::Return(Expr::Lit(Lit::Null))),
      },
      js::Stmt::Expr(expr_stmt) => self.effect(&expr_stmt.expr),
      js::Stmt::Throw(thrown) => Ok(Stmt::Expr(crate::component::throw_expr(self, thrown)?)),
      js::Stmt::Try(t) => self.try_stmt(t),
      js::Stmt::Block(block) => Err(self.residue(block.span, "a bare block")),
      other => {
        let (what, hint) = describe_stmt(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  /// `redirect(to)`: the response that sends the caller to `to`, which ends the body where it is a statement. An action or a route handler holds one.
  /// `after(fn)`: the function, written in place with no parameters, to run once the response is written. Its body lowers as a body does; it cannot write the session, whose cookie is already sent.
  fn after_of(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    let body = match call.args.as_slice() {
      [work] if work.spread.is_none() => match strip_parens(&work.expr) {
        js::Expr::Arrow(arrow) if arrow.params.is_empty() => match &*arrow.body {
          js::ArrowFunctionBody::FunctionBody(block) => self.block(&block.stmts)?,
          js::ArrowFunctionBody::Expr(e) => vec![Stmt::Return(self.expr(e)?)],
        },
        js::Expr::Fn(f) if f.function.params.is_empty() => match &f.function.body {
          Some(block) => self.block(&block.stmts)?,
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

  fn redirect_to(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    if !self.redirects {
      return Err(self.residue_with(
        call.span,
        "`redirect` outside an action or a route handler",
        "middleware returns `Response.redirect(to)`; a loader cannot send the visitor elsewhere",
      ));
    }
    let [to] = call.args.as_slice() else {
      return Err(self.residue(call.span, "`redirect` takes the path to send the caller to"));
    };
    Ok(Expr::Builtin { name: Builtin::RedirectTo, args: vec![self.expr(&to.expr)?] })
  }

  pub(crate) fn try_stmt(&mut self, t: &js::TryStmt) -> Lowered<Stmt> {
    let body = self.block(&t.block.stmts)?;
    let (name, handler) = match &t.handler {
      None => (None, None),
      Some(catch) => {
        let depth = self.scope.len();
        let name = crate::component::catch_param(self, catch.param.as_ref());
        let handler = name.and_then(|name| Ok((name, self.block(&catch.body.stmts)?)));
        self.scope.truncate(depth);
        let (name, handler) = handler?;
        (name, Some(handler))
      }
    };
    let finally = match &t.finalizer {
      Some(finalizer) => self.block(&finalizer.stmts)?,
      None => Vec::new(),
    };
    Ok(Stmt::Try { body, name, handler, finally })
  }

  fn branch(&mut self, stmt: &js::Stmt) -> Lowered<Body> {
    match stmt {
      js::Stmt::Block(block) => self.block(&block.stmts),
      single => self.block(std::slice::from_ref(single)),
    }
  }

  /// `fail("kind", "message")` as a statement, bare or in a one-statement block.
  fn as_fail(&mut self, stmt: &js::Stmt) -> Option<(Lowered<String>, Lowered<Expr>)> {
    let inner = match stmt {
      js::Stmt::Block(b) if b.stmts.len() == 1 => &b.stmts[0],
      other => other,
    };
    let js::Stmt::Expr(e) = inner else { return None };
    let js::Expr::Call(call) = &*e.expr else { return None };
    if !self.parsed.names_server(&call.callee, "fail") {
      return None;
    }
    let kind = match call.args.first().map(|a| &*a.expr) {
      Some(js::Expr::Lit(js::Lit::Str(s))) => Ok(s.value.to_atom_lossy().to_string()),
      Some(other) => Err(self.residue(other.span(), "`fail` takes its kind as a string literal, since the kind is matched at build time")),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    let message = match call.args.get(1) {
      Some(a) => self.expr(&a.expr),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    Some((kind, message))
  }

  /// A statement that is an expression: a session write, a delete, a bare
  /// `fail` or an awaited call for its effect.
  pub(crate) fn effect(&mut self, expr: &js::Expr) -> Lowered<Stmt> {
    // A `console` call in a helper's block logs on the server as one in a component's body does.
    if let js::Expr::Call(call) = expr {
      if let Some((response, op, args)) = self.response_headers_call(call) {
        if matches!(op.as_str(), "set" | "append" | "delete") {
          let js::Expr::Ident(local) = crate::meaning::bare(response) else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let Some((_, Expr::Var(name))) = self.scope.iter().rev().find(|(n, _)| n == local.sym.as_ref()).cloned() else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let wanted = if op == "delete" { 1 } else { 2 };
          if args.len() != wanted {
            return Err(self.residue(call.span, format!("`headers.{op}` takes {wanted} arguments")));
          }
          let mut lowered = vec![Expr::Var(name.clone()), Expr::lit_str(&op)];
          for arg in args {
            lowered.push(self.expr(&arg.expr)?);
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
      if let Some(exit) = self.process_exit(call)? {
        return Ok(exit);
      }
      if let Some(arrow) = self.async_iife(call) {
        let ran = match &*arrow.body {
          js::ArrowFunctionBody::FunctionBody(b) => self.async_stmts(&b.stmts)?,
          js::ArrowFunctionBody::Expr(e) => self.async_stmts(&[js::Stmt::Expr(js::ExprStmt { span: e.span(), expr: e.clone() })])?,
        };
        return Ok(Stmt::Expr(ran));
      }
    }
    match expr {
      js::Expr::Assign(assign) if self.cell_of_target(&assign.left).is_some() => {
        let handle = self.cell_of_target(&assign.left).expect("checked");
        let read = match &assign.left {
          js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => js::Expr::Ident(id.id.clone()),
          js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) => js::Expr::Member(member.clone()),
          _ => unreachable!("cell_of_target answers for a name or a path"),
        };
        let value = match assign.op.to_update() {
          None => self.expr(&assign.right)?,
          Some(op) => self.expr(&js::Expr::Bin(js::BinExpr { span: assign.span, op, left: Box::new(read.clone()), right: assign.right.clone() }))?,
        };
        let value = match &read {
          js::Expr::Member(member) => {
            let (_, path) = write_root(member).expect("cell_of_target found the root");
            self.written_at(Expr::ReadCell(Box::new(handle.clone())), &path, value)?
          }
          _ => value,
        };
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expr::Update(update) if matches!(&*update.arg, js::Expr::Ident(id) if self.cells.iter().any(|(name, _)| *name == *id.sym)) => {
        let js::Expr::Ident(id) = &*update.arg else { unreachable!() };
        let handle = self.cells.iter().find(|(name, _)| *name == *id.sym).map(|(_, handle)| handle.clone()).expect("checked");
        let op = match update.op {
          js::UpdateOp::PlusPlus => js::BinaryOp::Add,
          js::UpdateOp::MinusMinus => js::BinaryOp::Sub,
        };
        let one = js::Expr::Lit(js::Lit::Num(js::Number { span: update.span, value: 1.0, raw: None }));
        let value = self.expr(&js::Expr::Bin(js::BinExpr { span: update.span, op, left: update.arg.clone(), right: Box::new(one) }))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expr::Assign(assign) if self.local_target(&assign.left).is_some() => self.local_assign(assign),
      js::Expr::Update(update) if matches!(&*update.arg, js::Expr::Ident(id) if self.is_local(id.sym.as_ref())) => {
        let js::Expr::Ident(id) = &*update.arg else { unreachable!() };
        let op = match update.op {
          js::UpdateOp::PlusPlus => js::BinaryOp::Add,
          js::UpdateOp::MinusMinus => js::BinaryOp::Sub,
        };
        let one = js::Expr::Lit(js::Lit::Num(js::Number { span: update.span, value: 1.0, raw: None }));
        let expr = self.expr(&js::Expr::Bin(js::BinExpr { span: update.span, op, left: update.arg.clone(), right: Box::new(one) }))?;
        Ok(Stmt::Set { name: id.sym.to_string(), expr })
      }
      js::Expr::Call(call) if self.cell_collection_write(call).is_some() => {
        let (handle, kind, method) = self.cell_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::ReadCell(Box::new(handle.clone())))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(expr))))
      }
      js::Expr::Call(call) if self.local_url_search_write(call).is_some() => {
        let (name, method) = self.local_url_search_write(call).expect("checked");
        let search = Expr::Builtin { name: Builtin::UrlPart, args: vec![Expr::Var(name.clone()), Expr::lit_str("search")] };
        let written = self.collection_written(call, Kind::Query, &method, search)?;
        Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str("search"), written] }, name })
      }
      js::Expr::Call(call) if self.local_collection_write(call).is_some() => {
        let (name, kind, method) = self.local_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::Var(name.clone()))?;
        Ok(Stmt::Set { name, expr })
      }
      js::Expr::Call(call) if self.unbound_collection_write(call).is_some() => {
        let (name, span) = self.unbound_collection_write(call).expect("checked");
        self.unbound = Some(name.clone());
        Err(self.residue(span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body")))
      }
      js::Expr::Call(call) if self.local_push(call).is_some() => {
        let (name, method) = self.local_push(call).expect("checked");
        let mut items = Vec::with_capacity(call.args.len() + 1);
        for arg in &call.args {
          let value = self.expr(&arg.expr)?;
          items.push(if arg.spread.is_some() { Entry::Spread(value) } else { Entry::Item(value) });
        }
        let held = Entry::Spread(Expr::Var(name.clone()));
        if method == "push" {
          items.insert(0, held);
        } else {
          items.push(held);
        }
        Ok(Stmt::Set { name, expr: Expr::Array(items) })
      }
      js::Expr::Assign(assign) => {
        if assign.op != js::AssignOp::Assign {
          return Err(self.residue(assign.span, "a compound assignment; write the full expression"));
        }
        let js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) = &assign.left else {
          return Err(self.residue(assign.left.span(), "an assignment to something other than the session"));
        };
        let (key, path) = self.session_target(member)?;
        let value = self.expr(&assign.right)?;
        Ok(Stmt::SessionSet { key, path, value })
      }
      js::Expr::Unary(unary) if unary.op == js::UnaryOp::Delete => {
        let member = match &*unary.arg {
          js::Expr::Member(member) => member.clone(),
          js::Expr::OptChain(o) => match &*o.base {
            js::OptChainBase::Member(member) => member.clone(),
            js::OptChainBase::Call(call) => return Err(self.residue(call.span, "`delete` of a call")),
          },
          other => return Err(self.residue(other.span(), "`delete` of something other than a session entry")),
        };
        let (key, path) = self.session_target(&member)?;
        Ok(Stmt::SessionDelete { key, path })
      }
      js::Expr::Call(call) if self.parsed.names_server(&call.callee, "redirect") => {
        Ok(Stmt::Return(self.redirect_to(call)?))
      }
      js::Expr::Call(call) if self.parsed.names_server(&call.callee, "after") => Ok(Stmt::Expr(self.after_of(call)?)),
      js::Expr::Call(call) if self.is_ident_call(call, "fail") => {
        let stmt = js::Stmt::Expr(js::ExprStmt { span: call.span, expr: Box::new(expr.clone()) });
        let (kind, message) = self.as_fail(&stmt).expect("checked");
        Ok(Stmt::Guard { cond: Expr::Lit(Lit::Bool(true)), kind: kind?, message: message? })
      }
      js::Expr::Call(call) if self.is_session_extend(call) => {
        if !self.extends {
          return Err(self.residue_with(
            call.span,
            "`session.extend` outside an action, a route handler or middleware",
            "a loader runs on every navigation, so extending there is a store write per page view; extend from an action, a route handler or middleware, which can decide when",
          ));
        }
        let [arg] = call.args.as_slice() else {
          return Err(self.residue(call.span, "`session.extend` takes seconds, one number"));
        };
        Ok(Stmt::SessionExtend { seconds: self.expr(&arg.expr)? })
      }
      other => Ok(Stmt::Expr(self.expr(other)?)),
    }
  }

  /// The values of `Promise.all([...])`, the global's, each written without a spread: awaiting it is awaiting each of them, which a body runs together.
  fn promise_all<'e>(&self, e: &'e js::Expr) -> Option<Vec<&'e js::Expr>> {
    let js::Expr::Call(call) = crate::meaning::bare(e) else { return None };
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(m) = &**callee else { return None };
    let (js::Expr::Ident(promise), js::MemberProp::Ident(all)) = (&*m.obj, &m.prop) else { return None };
    if promise.ctxt != self.parsed.unresolved || &*promise.sym != "Promise" || &*all.sym != "all" {
      return None;
    }
    let [arg] = call.args.as_slice() else { return None };
    let js::Expr::Array(array) = crate::meaning::bare(&arg.expr) else { return None };
    array.elems.iter().map(|e| e.as_ref().filter(|e| e.spread.is_none()).map(|e| &*e.expr)).collect()
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
  pub(crate) fn lift_updates(&mut self, stmt: &js::Stmt, out: &mut Vec<Stmt>) -> Lowered<()> {
    if let js::Stmt::Expr(e) = stmt {
      if matches!(strip_parens(&e.expr), js::Expr::Update(_)) {
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
        self.premade.insert((span.lo.0, span.hi.0), Expr::Var(name));
      } else {
        let held = self.temp();
        out.push(Stmt::Let { name: held.clone(), expr: Expr::Var(name.clone()) });
        out.push(Stmt::Set { name, expr: next });
        self.premade.insert((span.lo.0, span.hi.0), Expr::Var(held));
      }
    }
    Ok(())
  }

  /// The local a write lands on: `x` or `x.field` with `x` declared in the body.
  fn local_target(&self, target: &js::AssignTarget) -> Option<String> {
    match target {
      js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => self.is_local(id.id.sym.as_ref()).then(|| id.id.sym.to_string()),
      js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => match write_root(m) {
        Some((id, _)) if self.is_local(id.sym.as_ref()) => Some(id.sym.to_string()),
        _ => None,
      },
      _ => None,
    }
  }

  /// `current` with what `path` reaches replaced by `value`, each step on the way copied: a field as a spread with the field changed, an index through `SetAt`.
  pub(crate) fn written_at(&mut self, current: Expr, path: &[&js::MemberExpr], value: Expr) -> Lowered<Expr> {
    let Some((step, rest)) = path.split_first() else { return Ok(value) };
    if let Some(field) = self.member_name(step) {
      let inner = self.written_at(current.clone().field(field.clone()), rest, value)?;
      return Ok(Expr::Object(vec![Entry::Spread(current), Entry::Field(field, inner)]));
    }
    let js::MemberProp::Computed(key) = &step.prop else { return Err(self.residue(step.span, "a write through a private field")) };
    let key = self.expr(&key.expr)?;
    let inner = self.written_at(Expr::Index(Box::new(current.clone()), Box::new(key.clone())), rest, value)?;
    Ok(Expr::Builtin { name: Builtin::SetAt, args: vec![current, key, inner] })
  }

  /// Whether `call` is one [`Lowerer::settling`] makes an act of. An async arrow called in place is one too, since it issues an act at its first `await`.
  pub(crate) fn issues_act(&self, call: &js::CallExpr) -> bool {
    if self.async_iife(call).is_some() || self.listens(call) {
      return true;
    }
    let js::Callee::Expr(callee) = &call.callee else { return false };
    let js::Expr::Ident(id) = crate::meaning::bare(callee) else { return false };
    let settler = |e: &js::Expr| matches!(crate::meaning::bare(e), js::Expr::Ident(id) if self.settlers.iter().any(|s| *s == *id.sym));
    settler(callee) || (self.reaches(Reach::Later) && id.sym.as_ref() == "setTimeout" && id.ctxt == self.parsed.unresolved && matches!(call.args.as_slice(), [callback, _] if settler(&callback.expr) || matches!(crate::meaning::bare(&callback.expr), js::Expr::Arrow(_))))
  }

  /// Whether `e` is Node's `process`, the global or the default import of `node:process`.
  pub(crate) fn is_process(&self, e: &js::Expr) -> bool {
    match crate::meaning::bare(e) {
      js::Expr::Ident(id) if id.ctxt == self.parsed.unresolved => id.sym.as_ref() == "process",
      other => self.meanings.as_ref().and_then(|meanings| meanings.of_expr(other)) == Some(meaning::Meaning::Library(meaning::Construct::NodeProcess)),
    }
  }

  /// `process.env.NAME` or `process.env[name]` in a terminal page: the variable through `terminal.env`, null when it is not set. `process.platform` is `os.platform`. `process.stdout` is the terminal's output as `useStdout` gives it, its `columns` and `rows` the terminal's size.
  fn env_read(&mut self, member: &js::MemberExpr) -> Lowered<Option<Expr>> {
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    if self.is_process(&member.obj) && self.member_name(member).as_deref() == Some("platform") {
      return Ok(Some(Expr::Ext { module: "os".to_owned(), name: "platform".to_owned(), args: Vec::new() }));
    }
    if self.is_process(&member.obj) && self.member_name(member).as_deref() == Some("stdout") {
      let size = |key: &str, fallback: i128| Expr::Coalesce(Box::new(Expr::Store(key.to_owned())), Box::new(Expr::Lit(Lit::Int(fallback))));
      return Ok(Some(Expr::Object(vec![
        Entry::Field("columns".to_owned(), size(crate::TERMINAL_COLUMNS, 80)),
        Entry::Field("rows".to_owned(), size(crate::TERMINAL_ROWS, 24)),
        Entry::Field(snapfire_fsr_ir::host::PROCESS.to_owned(), Expr::lit_str(snapfire_fsr_ir::host::TERMINAL_OUTPUT)),
      ])));
    }
    if let js::Expr::Member(out) = crate::meaning::bare(&member.obj) {
      if matches!(self.member_name(out).as_deref(), Some("stdout" | "stderr")) && self.is_process(&out.obj) {
        let size = match self.member_name(member).as_deref() {
          Some("columns") => (crate::TERMINAL_COLUMNS, 80),
          Some("rows") => (crate::TERMINAL_ROWS, 24),
          _ => return Ok(None),
        };
        return Ok(Some(Expr::Coalesce(Box::new(Expr::Store(size.0.to_owned())), Box::new(Expr::Lit(Lit::Int(size.1))))));
      }
    }
    let js::Expr::Member(env) = crate::meaning::bare(&member.obj) else { return Ok(None) };
    if self.member_name(env).as_deref() != Some("env") || !self.is_process(&env.obj) {
      return Ok(None);
    }
    let name = match &member.prop {
      js::MemberProp::Ident(name) => Expr::lit_str(name.sym.to_string()),
      js::MemberProp::Computed(key) => self.expr(&key.expr)?,
      js::MemberProp::PrivateName(_) => return Ok(None),
    };
    Ok(Some(Expr::Ext { module: "terminal".to_owned(), name: "env".to_owned(), args: vec![name] }))
  }

  /// `process.exit(code?)` in a terminal page: the act [`snapfire_fsr_ir::host::Act::Exit`] with the code, null when none is given.
  pub(crate) fn process_exit(&mut self, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Member(member) = crate::meaning::bare(callee) else { return Ok(None) };
    if !self.reaches(Reach::Process) || !self.is_process(&member.obj) || self.member_name(member).as_deref() != Some("exit") {
      return Ok(None);
    }
    let code = match call.args.first() {
      Some(arg) => self.expr(&arg.expr)?,
      None => Expr::Lit(Lit::Null),
    };
    Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Exit.name().to_owned(), input: code }))
  }

  /// `process.cwd()` in a terminal page: the member `process.cwd`.
  fn process_call(&self, call: &js::CallExpr) -> Option<Expr> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = crate::meaning::bare(callee) else { return None };
    (self.reaches(Reach::Process) && self.is_process(&member.obj) && self.member_name(member).as_deref() == Some("cwd") && call.args.is_empty()).then(|| Expr::Ext { module: "process".to_owned(), name: "cwd".to_owned(), args: Vec::new() })
  }

  /// Whether `call` is `spawn` from `node:child_process`, named or through its default export.
  pub(crate) fn spawns(&self, call: &js::CallExpr) -> bool {
    let js::Callee::Expr(callee) = &call.callee else { return false };
    let meant = |e: &js::Expr| self.meanings.as_ref().and_then(|meanings| meanings.of_expr(e));
    match crate::meaning::bare(callee) {
      js::Expr::Member(member) => meant(&member.obj) == Some(meaning::Meaning::Library(meaning::Construct::ChildProcess("module"))) && self.member_name(member).as_deref() == Some("spawn"),
      other => meant(other) == Some(meaning::Meaning::Library(meaning::Construct::ChildProcess("spawn"))),
    }
  }

  /// `spawn(command, args, options)` in a terminal block: a fresh process id and the act [`snapfire_fsr_ir::host::Act::Spawn`] starting it, whose value is the process, [`snapfire_fsr_ir::host::PROCESS`].
  fn spawn_value(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    let id = self.temp();
    let mut fields = vec![Entry::Field("id".to_owned(), Expr::Var(id.clone()))];
    for (i, key) in ["command", "args", "options"].iter().enumerate() {
      let value = match call.args.get(i) {
        Some(arg) if arg.spread.is_some() => return Err(self.residue(arg.expr.span(), "a spread into `spawn`")),
        Some(arg) => self.expr(&arg.expr)?,
        None => Expr::Lit(Lit::Null),
      };
      fields.push(Entry::Field((*key).to_owned(), value));
    }
    let random = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let spawn = Stmt::Act { action: snapfire_fsr_ir::host::Act::Spawn.name().to_owned(), input: Expr::Object(fields) };
    Ok(Expr::Block(vec![Stmt::Let { name: id.clone(), expr: random }, spawn, Stmt::Return(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::PROCESS.to_owned(), Expr::Var(id))]))]))
  }

  /// The process, the report and the listener of `sub.on(event, f)`, `sub.stdout.on("data", f)` or `sub.stderr.on("data", f)` on a local a block bound to a spawn; also `stdout.on("resize", f)` and `stdout.off("resize", f)` on the terminal's output, `true` for `off`; the report `None` for `setEncoding`, which a terminal takes as read.
  fn listener<'c>(&self, call: &'c js::CallExpr) -> Option<(&'c js::Expr, Option<snapfire_fsr_ir::host::Report>, Option<&'c js::Expr>, bool)> {
    use snapfire_fsr_ir::host::Report;
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(method) = crate::meaning::bare(callee) else { return None };
    let event = call.args.first().and_then(|a| string_literal(&a.expr));
    if let (Some(on @ ("on" | "off")), Some(event)) = (self.member_name(method).as_deref(), event.as_deref()) {
      if event == Report::Resize.name() {
        return Some((&method.obj, Some(Report::Resize), call.args.get(1).map(|a| &*a.expr), on == "off"));
      }
    }
    let (process, stream) = match crate::meaning::bare(&method.obj) {
      process @ js::Expr::Ident(id) if self.spawned.iter().any(|s| *s == *id.sym) => (process, None),
      js::Expr::Member(inner) => match (crate::meaning::bare(&inner.obj), self.member_name(inner).as_deref()) {
        (process @ js::Expr::Ident(id), Some(stream @ ("stdout" | "stderr"))) if self.spawned.iter().any(|s| *s == *id.sym) => (process, Some(if stream == "stdout" { Report::Stdout } else { Report::Stderr })),
        _ => return None,
      },
      _ => return None,
    };
    let report = match (stream, self.member_name(method).as_deref(), event.as_deref()) {
      (Some(_), Some("setEncoding"), _) => return Some((process, None, None, false)),
      (Some(stream), Some("on"), Some("data")) => stream,
      (None, Some("on"), Some(event)) => Report::parse(event)?,
      _ => return None,
    };
    Some((process, Some(report), call.args.get(1).map(|a| &*a.expr), false))
  }

  /// Whether `call` is a listener [`Lowerer::listener`] reads.
  fn listens(&self, call: &js::CallExpr) -> bool {
    self.reaches(Reach::Process) && self.listener(call).is_some()
  }

  /// A listener on a spawned process as the act [`snapfire_fsr_ir::host::Act::Listen`]. Its callback is a continuation with `code` and `signal` bound from an exit's report; a promise's `resolve` or `reject` settles the promise instead. `None` for any other call.
  fn listening(&mut self, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
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
    if matches!(crate::meaning::bare(callback), js::Expr::Ident(id) if self.settlers.iter().any(|s| *s == *id.sym)) {
      fields.push(Entry::Field("settle".to_owned(), self.expr(callback)?));
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }));
    }
    let js::Expr::Arrow(arrow) = crate::meaning::bare(callback) else { return Err(self.residue(callback.span(), "a process listener that is not an arrow written in place or a promise's `resolve` or `reject`")) };
    let params: Vec<String> = arrow.params.iter().map(|p| match p {
      js::Pat::Ident(name) => Ok(name.id.sym.to_string()),
      other => Err(self.residue(other.span(), "a process listener's parameter that is not a name")),
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
      match &*arrow.body {
        js::ArrowFunctionBody::FunctionBody(b) => this.async_stmts(&b.stmts),
        js::ArrowFunctionBody::Expr(e) => this.async_stmts(&[js::Stmt::Expr(js::ExprStmt { span: e.span(), expr: e.clone() })]),
      }
    })?;
    fields.push(Entry::Field("event".to_owned(), Expr::lit_str(event)));
    fields.push(Entry::Field("carry".to_owned(), carry));
    Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }))
  }

  /// `p.then(f)` in a terminal page: a fresh promise and a continuation on `p` running `f` with what `p` resolved to, which settles the fresh one with what `f` returns; one `p` rejects rejects it as well.
  fn then_call(&mut self, call: &js::CallExpr) -> Lowered<Option<Expr>> {
    let (true, js::Callee::Expr(callee)) = (self.reaches(Reach::Later), &call.callee) else { return Ok(None) };
    let js::Expr::Member(member) = crate::meaning::bare(callee) else { return Ok(None) };
    if self.member_name(member).as_deref() != Some("then") || call.args.len() != 1 {
      return Ok(None);
    }
    let Some(js::Expr::Arrow(f)) = call.args.first().map(|a| crate::meaning::bare(&a.expr)) else { return Ok(None) };
    let awaited = self.expr(&member.obj)?;
    let done = self.temp();
    let param = match f.params.as_slice() {
      [] => None,
      [js::Pat::Ident(name)] => Some(name.id.sym.to_string()),
      _ => return Err(self.residue(call.span, "a `then` callback whose parameter is not one name")),
    };
    let finished = Expr::Var(done.clone());
    let (event, carry) = self.continuation(call.span, param, |this| {
      let stmts = match &*f.body {
        js::ArrowFunctionBody::FunctionBody(b) => b.stmts.clone(),
        js::ArrowFunctionBody::Expr(e) => vec![js::Stmt::Return(js::ReturnStmt { span: e.span(), arg: Some(e.clone()) })],
      };
      let ran = this.async_to(&stmts, Some(finished.clone()))?;
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
  fn suspend_call(&mut self, call: &js::CallExpr) -> Lowered<Option<Expr>> {
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(id) = crate::meaning::bare(callee) else { return Ok(None) };
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    let suspends = match self.scope.iter().rev().find(|(n, _)| *n == *id.sym) {
      Some((_, bound)) => crate::terminal::primitive_ref(bound).as_deref() == Some("suspendTerminal"),
      None => self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) == Some(&meaning::Meaning::Library(meaning::Construct::Suspend)),
    };
    if !suspends {
      return Ok(None);
    }
    let Some(js::Expr::Arrow(f)) = call.args.first().map(|a| crate::meaning::bare(&a.expr)) else { return Err(self.residue(call.span, "`suspendTerminal` of something other than an arrow written in place")) };
    let inner = self.async_value(f)?;
    let (until, settles) = (self.temp(), self.temp());
    let suspend = Stmt::Act { action: snapfire_fsr_ir::host::Act::Suspend.name().to_owned(), input: Expr::Object(vec![Entry::Field("until".to_owned(), Expr::Var(until.clone())), Entry::Field("settles".to_owned(), Expr::Var(settles.clone()))]) };
    Ok(Some(Expr::Block(vec![Stmt::Let { name: until, expr: inner }, Stmt::Let { name: settles.clone(), expr: self.new_deferred() }, suspend, Stmt::Return(Expr::Var(settles))])))
  }

  /// An async arrow as a value: its statements run as an async function. The promise it returns settles with what the function returns; it rejects with what an awaited promise the function does not catch rejects with.
  pub(crate) fn async_value(&mut self, f: &js::ArrowExpr) -> Lowered<Expr> {
    let done = self.temp();
    let ran = match &*f.body {
      js::ArrowFunctionBody::FunctionBody(b) => self.async_to(&b.stmts, Some(Expr::Var(done.clone())))?,
      js::ArrowFunctionBody::Expr(e) => self.async_to(&[js::Stmt::Return(js::ReturnStmt { span: e.span(), arg: Some(e.clone()) })], Some(Expr::Var(done.clone())))?,
    };
    Ok(Expr::Block(vec![Stmt::Let { name: done.clone(), expr: self.new_deferred() }, Stmt::Expr(ran), Stmt::Return(Expr::Var(done))]))
  }

  /// `(async () => { … })()`: an async arrow called where it is written, in a terminal page.
  pub(crate) fn async_iife<'c>(&self, call: &'c js::CallExpr) -> Option<&'c js::ArrowExpr> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    match crate::meaning::bare(callee) {
      js::Expr::Arrow(arrow) if self.reaches(Reach::Later) && arrow.is_async && call.args.is_empty() && arrow.params.is_empty() => Some(arrow),
      _ => None,
    }
  }

  /// An async function's statements run as far as its first `await`, which hands the awaited promise a continuation, [`snapfire_fsr_ir::host::Act::Then`], holding the rest. The rest lowers as a handler named [`snapfire_fsr_ir::host::Event::Resume`] in [`Lowerer::continuations`]; the names it reads that are not module cells are carried to it with the act.
  pub(crate) fn async_stmts(&mut self, stmts: &[js::Stmt]) -> Lowered<Expr> {
    self.async_to(stmts, None)
  }

  /// [`Lowerer::async_stmts`] for a function whose promise is `done`: what it returns settles it; an awaited promise that rejects rejects it.
  pub(crate) fn async_to(&mut self, stmts: &[js::Stmt], done: Option<Expr>) -> Lowered<Expr> {
    let floor = self.async_floor.replace(self.scope.len());
    let lowered = self.async_caught(stmts, done, None);
    self.async_floor = floor;
    lowered
  }

  /// [`Lowerer::async_to`] inside `catch`, where an awaited promise among the statements it covers that rejects runs the `catch` instead.
  fn async_caught(&mut self, stmts: &[js::Stmt], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let depth = self.scope.len();
    let lowered = self.async_to_in(stmts, done, catch);
    self.scope.truncate(depth);
    lowered
  }

  /// The statements before an `await`, as `let`s and effects whose names stay in scope for the awaited value and the continuation.
  fn before_await(&mut self, stmts: &[js::Stmt]) -> Lowered<Expr> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
      out.push(self.stmt(stmt)?);
    }
    Ok(Expr::Block(out))
  }

  fn async_to_in(&mut self, stmts: &[js::Stmt], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let settle = |done: &Expr, ok: bool, value: Expr| Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), done.clone()), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))])), Entry::Field("value".to_owned(), value)]) };
    let Some(at) = stmts.iter().position(awaits) else {
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
    if let js::Stmt::Try(t) = &stmts[at] {
      if inside.is_some() {
        return Err(self.residue(t.span, "a `try` that awaits inside another"));
      }
      let finally: Vec<js::Stmt> = t.finalizer.iter().flat_map(|f| f.stmts.iter().cloned()).collect();
      if !finally.is_empty() && (t.block.stmts.iter().any(crate::component::holds_return) || t.handler.iter().any(|h| h.body.stmts.iter().any(crate::component::holds_return))) {
        return Err(self.residue(t.span, "a `return` inside a `try` whose `finally` follows an `await`"));
      }
      let before = self.before_await(&stmts[..at])?;
      let after: Vec<js::Stmt> = finally.iter().chain(&stmts[at + 1..]).cloned().collect();
      let rethrown = |covers: usize| {
        let name = format!("$finally{}", t.span.lo.0);
        let thrown = js::Stmt::Throw(js::ThrowStmt { span: t.span, arg: Box::new(js::Expr::Ident(js::Ident::new_no_ctxt(name.as_str().into(), t.span))) });
        AsyncCatch { param: Some(name), body: finally.iter().cloned().chain(std::iter::once(thrown)).collect(), covers, inner: None }
      };
      let catch = match &t.handler {
        Some(handler) => {
          let param = match &handler.param {
            None => None,
            Some(js::Pat::Ident(name)) => Some(name.id.sym.to_string()),
            Some(other) => return Err(self.residue(other.span(), "a `catch` parameter that is not a name")),
          };
          let inner = (!finally.is_empty()).then(|| Box::new(rethrown(handler.body.stmts.len())));
          AsyncCatch { param, body: handler.body.stmts.iter().chain(&after).cloned().collect(), covers: t.block.stmts.len(), inner }
        }
        None => rethrown(t.block.stmts.len()),
      };
      let body: Vec<js::Stmt> = t.block.stmts.iter().chain(&after).cloned().collect();
      let ran = self.async_caught(&body, done, Some(catch))?;
      return Ok(Expr::Block(vec![Stmt::Expr(before), Stmt::Expr(ran)]));
    }
    if let js::Stmt::If(branch) = &stmts[at] {
      let before = self.before_await(&stmts[..at])?;
      let cond = self.expr(&branch.test)?;
      let arm = |this: &mut Self, arm: Option<&js::Stmt>| -> Lowered<Expr> {
        let mut joined: Vec<js::Stmt> = match arm {
          Some(js::Stmt::Block(block)) => block.stmts.clone(),
          Some(other) => vec![other.clone()],
          None => Vec::new(),
        };
        let covered = inside.clone().map(|catch| AsyncCatch { covers: joined.len() + catch.covers - at - 1, ..catch });
        joined.extend(stmts[at + 1..].iter().cloned());
        this.async_caught(&joined, done.clone(), covered)
      };
      let then = arm(self, Some(&branch.cons))?;
      let otherwise = arm(self, branch.alt.as_deref())?;
      return Ok(Expr::Block(vec![Stmt::Expr(before), Stmt::If { cond, then: vec![Stmt::Expr(then)], r#else: vec![Stmt::Expr(otherwise)] }]));
    }
    let (awaited, bound) = match &stmts[at] {
      js::Stmt::Expr(e) => match crate::meaning::bare(&e.expr) {
        js::Expr::Await(a) => (&*a.arg, None),
        _ => return Err(self.residue(e.span, "an `await` inside an expression; await as a statement or a `const`'s value")),
      },
      js::Stmt::Decl(js::Decl::Var(var)) => match (var.decls.as_slice(), var.decls.first().and_then(|d| d.init.as_deref()).map(crate::meaning::bare)) {
        ([decl], Some(js::Expr::Await(a))) => match &decl.name {
          js::Pat::Ident(name) => (&*a.arg, Some(name.id.sym.to_string())),
          _ => return Err(self.residue(decl.span, "an awaited value bound to a pattern")),
        },
        _ => return Err(self.residue(var.span, "an `await` inside an expression; await as a statement or a `const`'s value")),
      },
      other => return Err(self.residue(other.span(), "an `await` inside a loop or a `switch`")),
    };
    let before = self.before_await(&stmts[..at])?;
    let deferred = self.expr(awaited)?;
    let rest = &stmts[at + 1..];
    let (event, carry) = self.continuation(stmts[at].span(), bound, |this| {
      let resumed = Expr::Var("$resume".to_owned());
      let covered = inside.clone().map(|catch| AsyncCatch { covers: catch.covers - at - 1, ..catch }).filter(|catch| catch.covers > 0);
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
    Ok(Expr::Block(vec![Stmt::Expr(before), then]))
  }

  /// Code the host runs later, the rest of an async function or a timer's callback, as a handler named [`snapfire_fsr_ir::host::Event::Resume`] for `at` in [`Lowerer::continuations`]. `bound` names the settled value it is handed. Every other name it reads that is not a module cell is carried to it. Returns the event and the carry the act that schedules it passes.
  fn continuation(&mut self, at: Span, bound: Option<String>, lower: impl FnOnce(&mut Self) -> Lowered<Expr>) -> Lowered<(String, Expr)> {
    let resume = "$resume".to_owned();
    let depth = self.scope.len();
    if let Some(name) = &bound {
      self.scope.push((name.clone(), Expr::Var(resume.clone()).field("value")));
    }
    let rest = lower(self);
    self.scope.truncate(depth);
    let rest = rest?;
    let mut free = Vec::new();
    rest.free_vars(&mut free);
    free.sort();
    free.dedup();
    let carried: Vec<String> = free.into_iter().filter(|name| *name != resume && !self.cells.iter().any(|(cell, _)| cell == name)).collect();
    let mut body = vec![Stmt::Let { name: resume.clone(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("carry").field(name.clone()) });
    }
    body.push(Stmt::Expr(rest));
    body.push(Stmt::Return(Expr::Object(Vec::new())));
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.parsed.file, at.lo.0)).name();
    if !self.continuations.iter().any(|held| held.event == event) {
      self.continuations.push(snapfire_fsr_ir::ast::Handler { event: event.clone(), body });
    }
    let carry = Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect());
    Ok((event, carry))
  }

  /// In a promise's executor, `resolve(v)` or `reject(e)` as an [`snapfire_fsr_ir::host::Act::Settle`] and `setTimeout(resolve, ms)` as a timer that settles; `None` for any other call.
  fn settling(&mut self, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(id) = crate::meaning::bare(callee) else { return Ok(None) };
    let settler = |this: &Self, e: &js::Expr| matches!(crate::meaning::bare(e), js::Expr::Ident(id) if this.settlers.iter().any(|s| *s == *id.sym));
    if settler(self, callee) {
      let value = match call.args.first() {
        Some(arg) => self.expr(&arg.expr)?,
        None => Expr::Lit(Lit::Null),
      };
      let settle = self.expr(callee)?;
      let input = Expr::Object(vec![Entry::Field("settle".to_owned(), settle), Entry::Field("value".to_owned(), value)]);
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input }));
    }
    if id.sym.as_ref() == "setTimeout" && id.ctxt == self.parsed.unresolved && self.reaches(Reach::Later) {
      if let [callback, delay] = call.args.as_slice() {
        if settler(self, &callback.expr) {
          let input = Expr::Object(vec![Entry::Field("settle".to_owned(), self.expr(&callback.expr)?), Entry::Field("ms".to_owned(), self.expr(&delay.expr)?)]);
          return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input }));
        }
        if let js::Expr::Arrow(arrow) = crate::meaning::bare(&callback.expr) {
          let ms = self.expr(&delay.expr)?;
          let (event, carry) = self.continuation(call.span, None, |this| match &*arrow.body {
            js::ArrowFunctionBody::FunctionBody(b) => this.async_stmts(&b.stmts),
            js::ArrowFunctionBody::Expr(e) => this.async_stmts(&[js::Stmt::Expr(js::ExprStmt { span: e.span(), expr: e.clone() })]),
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
  pub(crate) fn cell_of_target(&self, target: &js::AssignTarget) -> Option<Expr> {
    let id = match target {
      js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => &id.id,
      js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) => write_root(member)?.0,
      _ => return None,
    };
    self.cells.iter().rev().find(|(name, _)| *name == *id.sym).map(|(_, handle)| handle.clone())
  }

  fn local_assign(&mut self, assign: &js::AssignExpr) -> Lowered<Stmt> {
    let name = self.local_target(&assign.left).expect("checked");
    let read = match &assign.left {
      js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => js::Expr::Ident(id.id.clone()),
      js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => js::Expr::Member(m.clone()),
      _ => unreachable!("local_target answers for these two"),
    };
    let value = match assign.op.to_update() {
      None => self.expr(&assign.right)?,
      Some(op) => self.expr(&js::Expr::Bin(js::BinExpr { span: assign.span, op, left: Box::new(read.clone()), right: assign.right.clone() }))?,
    };
    let js::Expr::Member(member) = &read else {
      return Ok(Stmt::Set { name, expr: value });
    };
    if let (Some(Kind::Url), js::Expr::Ident(_), js::MemberProp::Ident(part)) = (self.kind_of(&member.obj), &*member.obj, &member.prop) {
      let part = part.sym.to_string();
      if part == "origin" || !URL_PARTS.contains(&part.as_str()) && part != "href" {
        return Err(self.residue(member.span, format!("a write to `.{part}` of a `URL`")));
      }
      return Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str(&part), value] }, name });
    }
    let (_, path) = write_root(member).expect("local_target found the root");
    let expr = self.written_at(Expr::Var(name.clone()), &path, value)?;
    Ok(Stmt::Set { name, expr })
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` a local collection: the local, which kind it is and the method.
  /// `held` after `call` changes it, a collection of `kind`.
  pub(crate) fn collection_written(&mut self, call: &js::CallExpr, kind: Kind, method: &str, held: Expr) -> Lowered<Expr> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      let a = call.args.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes {} argument{}", i + 1, if i == 0 { "" } else { "s" })))?;
      this.expr(&a.expr)
    };
    Ok(match (kind, method) {
      (Kind::Map, "set") => Expr::Object(vec![Entry::Spread(held), Entry::Computed(Expr::Str(Box::new(arg(self, 0)?)), arg(self, 1)?)]),
      (Kind::Map, "delete") => Expr::Builtin { name: Builtin::Omit, args: vec![held, Expr::Str(Box::new(arg(self, 0)?))] },
      (Kind::Set, "add") => Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Array(vec![Entry::Spread(held), Entry::Item(arg(self, 0)?)])] },
      (Kind::Query, "set" | "append") => Expr::Builtin { name: if method == "set" { Builtin::FormSet } else { Builtin::FormAppend }, args: vec![held, Expr::Str(Box::new(arg(self, 0)?)), arg(self, 1)?] },
      (Kind::Query, "delete") => Expr::Builtin { name: Builtin::FormDelete, args: vec![held, Expr::Str(Box::new(arg(self, 0)?))] },
      (_, other) => return Err(self.residue(call.span, format!("`.{other}()` of a local `{}` as a statement", kind.name()))),
    })
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` held in a cell: the cell's handle, the collection's kind and the method.
  fn cell_collection_write(&self, call: &js::CallExpr) -> Option<(Expr, Kind, String)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Ident(obj) = &*member.obj else { return None };
    let handle = self.cells.iter().rev().find(|(name, _)| *name == *obj.sym).map(|(_, handle)| handle.clone())?;
    let kind = self.kind_of(&member.obj)?;
    let method = self.member_name(member)?;
    matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear").then_some((handle, kind, method))
  }

  /// `url.searchParams.set(k, v)`, `.append(k, v)` or `.delete(k)` with `url` a local `URL`: the local and the method.
  fn local_url_search_write(&self, call: &js::CallExpr) -> Option<(String, String)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Member(params) = &*member.obj else { return None };
    let (js::Expr::Ident(url), js::MemberProp::Ident(prop)) = (&*params.obj, &params.prop) else { return None };
    let method = self.member_name(member)?;
    (&*prop.sym == "searchParams" && self.kind_of(&params.obj) == Some(Kind::Url) && self.is_local(url.sym.as_ref()) && matches!(method.as_str(), "set" | "append" | "delete")).then(|| (url.sym.to_string(), method))
  }

  /// `m.set(k, v)` and its kin on a name bound nowhere here, which may be a module-level collection the set binds and retries with: the name and its span.
  fn unbound_collection_write(&self, call: &js::CallExpr) -> Option<(String, Span)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Ident(obj) = &*member.obj else { return None };
    let method = self.member_name(member)?;
    if !matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") || obj.ctxt == self.parsed.unresolved {
      return None;
    }
    let name = obj.sym.as_ref();
    let bound = self.scope.iter().any(|(n, _)| n == name) || self.globals.iter().any(|(n, _)| n == name) || self.cells.iter().any(|(n, _)| n == name);
    (!bound).then(|| (name.to_owned(), obj.span))
  }

  fn local_collection_write(&self, call: &js::CallExpr) -> Option<(String, Kind, String)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Ident(obj) = &*member.obj else { return None };
    let kind = self.kind_of(&member.obj)?;
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "set" | "delete" | "add" | "append" | "clear") && self.is_local(obj.sym.as_ref())).then(|| (obj.sym.to_string(), kind, method))
  }

  /// `x.push(...)` or `x.unshift(...)` with `x` a local: the local and the method.
  fn local_push(&self, call: &js::CallExpr) -> Option<(String, String)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Ident(obj) = &*member.obj else { return None };
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "push" | "unshift") && self.is_local(obj.sym.as_ref())).then(|| (obj.sym.to_string(), method))
  }

  /// `session.extend(...)` or `ctx.session.extend(...)`.
  fn is_session_extend(&self, call: &js::CallExpr) -> bool {
    let js::Callee::Expr(callee) = &call.callee else { return false };
    let js::Expr::Member(member) = &**callee else { return false };
    if self.member_name(member).as_deref() != Some("extend") {
      return false;
    }
    match &*member.obj {
      js::Expr::Ident(id) => self.root_of(id) == Some(Root::Session),
      js::Expr::Member(via) => {
        let js::Expr::Ident(id) = &*via.obj else { return false };
        self.root_of(id) == Some(Root::Ctx) && self.member_name(via).as_deref() == Some("session")
      }
      _ => false,
    }
  }

  fn is_ident_call(&self, call: &js::CallExpr, name: &str) -> bool {
    matches!(&call.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(id) if id.sym.as_ref() == name))
  }

  /// `session.key`, `session.key.sub`, `session.key[expr]` or the same
  /// through `ctx.session`. Returns the key and the path beneath it.
  fn session_target(&mut self, member: &js::MemberExpr) -> Lowered<(String, Vec<Expr>)> {
    let mut chain = Vec::new();
    let mut current: &js::Expr = &js::Expr::Member(member.clone());
    let root_ident = loop {
      match current {
        js::Expr::Member(m) => {
          chain.push(m);
          current = &m.obj;
        }
        js::Expr::OptChain(o) => match &*o.base {
          js::OptChainBase::Member(m) => {
            chain.push(m);
            current = &m.obj;
          }
          js::OptChainBase::Call(call) => return Err(self.residue(call.span, "a call inside a session path")),
        },
        js::Expr::Ident(id) => break id,
        other => return Err(self.residue(other.span(), "a session write must start at `session`")),
      }
    };
    chain.reverse();
    let mut steps = chain.into_iter();
    let root = self.root_of(root_ident);
    let first = match root {
      Some(Root::Session) => steps.next(),
      Some(Root::Ctx) => {
        let via = steps.next().ok_or_else(|| self.residue(member.span, "a write to `ctx` itself"))?;
        if self.member_name(via).as_deref() != Some("session") {
          return Err(self.residue(via.span, "a write to something other than the session"));
        }
        steps.next()
      }
      _ => return Err(self.residue(root_ident.span, "a session write must start at `session`")),
    };
    let Some(first) = first else {
      return Err(self.residue(member.span, "a write to the whole session; write one key"));
    };
    let key = self.member_name(first).ok_or_else(|| self.residue(first.span, "the session key must be a name"))?;
    let mut path = Vec::new();
    for step in steps {
      path.push(match &step.prop {
        js::MemberProp::Ident(id) => Expr::Lit(Lit::Str(id.sym.to_string())),
        js::MemberProp::Computed(c) => self.expr(&c.expr)?,
        js::MemberProp::PrivateName(p) => return Err(self.residue(p.span, "a private name")),
      });
    }
    Ok((key, path))
  }

  fn member_name(&self, member: &js::MemberExpr) -> Option<String> {
    match &member.prop {
      js::MemberProp::Ident(id) => Some(id.sym.to_string()),
      _ => None,
    }
  }

  /// Whether `call` is middleware's `next(...)` or `ctx.next(...)`.
  fn calls_next(&self, call: &js::CallExpr) -> bool {
    let js::Callee::Expr(callee) = &call.callee else { return false };
    match crate::meaning::bare(callee) {
      js::Expr::Ident(id) => self.root_of(id) == Some(Root::Next),
      js::Expr::Member(m) => self.middleware && matches!((&*m.obj, &m.prop), (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) if self.root_of(obj) == Some(Root::Ctx) && &*prop.sym == "next"),
      _ => false,
    }
  }

  /// `response.headers.get(name)` or `.has(name)` on a `Response`; `None` for any other call.
  fn response_header_read(&mut self, call: &js::CallExpr) -> Lowered<Option<Expr>> {
    let Some((response, op, args)) = self.response_headers_call(call) else { return Ok(None) };
    if !matches!(op.as_str(), "get" | "has") {
      return Err(self.residue(call.span, format!("`headers.{op}` inside an expression; it is a statement")));
    }
    let [name] = args else { return Err(self.residue(call.span, format!("`headers.{op}` takes a header name"))) };
    let mut lowered = vec![self.expr(response)?, self.expr(&name.expr)?];
    if op == "has" {
      lowered.push(Expr::lit_str("has"));
    }
    Ok(Some(Expr::Builtin { name: Builtin::ResponseHeaderGet, args: lowered }))
  }

  /// A call `response.headers.<op>(...)` with `op` one of a `Headers`' `get`, `has`, `set`, `append` and `delete`, on anything not known to be another kind: the response, the operation and the arguments. A value that is not a `Response` fails where it runs.
  fn response_headers_call<'c>(&self, call: &'c js::CallExpr) -> Option<(&'c js::Expr, String, &'c [js::ExprOrSpread])> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(method) = crate::meaning::bare(callee) else { return None };
    let js::MemberProp::Ident(op) = &method.prop else { return None };
    let js::Expr::Member(headers) = crate::meaning::bare(&method.obj) else { return None };
    let js::MemberProp::Ident(field) = &headers.prop else { return None };
    let known = matches!(op.sym.as_ref(), "get" | "has" | "set" | "append" | "delete");
    (&*field.sym == "headers" && known && !matches!(self.kind_of(&headers.obj), Some(kind) if kind != Kind::Response)).then(|| (&*headers.obj, op.sym.to_string(), call.args.as_slice()))
  }

  fn root_of(&self, id: &js::Ident) -> Option<Root> {
    let name = id.sym.as_ref();
    self.roots.iter().rev().find(|(n, _)| n == name).map(|(_, r)| r.clone())
  }

  pub(crate) fn expr(&mut self, expr: &js::Expr) -> Lowered<Expr> {
    if !self.premade.is_empty() {
      let span = expr.span();
      if let Some(made) = self.premade.get(&(span.lo.0, span.hi.0)) {
        return Ok(made.clone());
      }
    }
    match expr {
      js::Expr::Paren(p) => self.expr(&p.expr),
      js::Expr::Await(a) => match self.promise_all(&a.arg) {
        Some(values) => Ok(Expr::Array(values.iter().map(|v| self.expr(v).map(Entry::Item)).collect::<Lowered<_>>()?)),
        None => self.expr(&a.arg),
      },
      js::Expr::TsAs(a) => self.expr(&a.expr),
      js::Expr::TsConstAssertion(a) => self.expr(&a.expr),
      js::Expr::TsNonNull(a) => self.expr(&a.expr),
      js::Expr::TsSatisfies(a) => self.expr(&a.expr),
      js::Expr::TsTypeAssertion(a) => self.expr(&a.expr),

      js::Expr::Ident(id) => self.ident(id),
      js::Expr::Lit(lit) => self.lit(lit, expr.span()),
      js::Expr::TaggedTpl(tagged) if matches!(crate::meaning::bare(&tagged.tag), js::Expr::Member(m) if matches!(&*m.obj, js::Expr::Ident(id) if id.sym.as_ref() == "String" && id.ctxt == self.parsed.unresolved) && self.member_name(m).as_deref() == Some("raw")) => {
        // `String.raw` writes each piece as it stands in the source, escapes and all.
        let mut parts = Vec::new();
        for (i, quasi) in tagged.tpl.quasis.iter().enumerate() {
          if !quasi.raw.is_empty() {
            parts.push(Expr::Lit(Lit::Str(quasi.raw.to_string())));
          }
          if let Some(e) = tagged.tpl.exprs.get(i) {
            parts.push(self.expr(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expr::Tpl(tpl) => {
        let mut parts = Vec::new();
        for (i, quasi) in tpl.quasis.iter().enumerate() {
          let text = quasi
            .cooked
            .as_ref()
            .map(|c| c.to_atom_lossy().to_string())
            .unwrap_or_else(|| quasi.raw.to_string());
          if !text.is_empty() {
            parts.push(Expr::Lit(Lit::Str(text)));
          }
          if let Some(e) = tpl.exprs.get(i) {
            parts.push(self.expr(e)?);
          }
        }
        Ok(Expr::Template(parts))
      }
      js::Expr::Object(obj) => {
        let mut entries = Vec::new();
        for prop in &obj.props {
          entries.push(match prop {
            js::PropOrSpread::Spread(s) => Entry::Spread(self.expr(&s.expr)?),
            js::PropOrSpread::Prop(p) => match &**p {
              js::Prop::Shorthand(id) => Entry::Field(id.sym.to_string(), self.ident(id)?),
              js::Prop::KeyValue(kv) => match &kv.key {
                js::PropName::Computed(c) => Entry::Computed(self.expr(&c.expr)?, self.expr(&kv.value)?),
                key => {
                  let key = prop_name(key).ok_or_else(|| self.residue(key.span(), "a numeric property name"))?;
                  Entry::Field(key, self.expr(&kv.value)?)
                }
              },
              other => return Err(self.residue(other.span(), "an accessor in an object literal")),
            },
          });
        }
        Ok(Expr::Object(entries))
      }
      js::Expr::Array(arr) => {
        let mut entries = Vec::new();
        for elem in arr.elems.iter().flatten() {
          if let (Some(_), Some(length)) = (elem.spread, self.array_of_length(&elem.expr)) {
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            entries.push(Entry::Spread(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$hole".to_owned()], body: Box::new(Expr::Lit(Lit::Undefined)) }))));
            continue;
          }
          let value = self.expr(&elem.expr)?;
          entries.push(match elem.spread {
            Some(_) if self.kind_of(&elem.expr) == Some(Kind::Map) => Entry::Spread(Expr::Entries(Box::new(value))),
            Some(_) => Entry::Spread(value),
            None => Entry::Item(value),
          });
        }
        Ok(Expr::Array(entries))
      }
      js::Expr::Unary(u) => match u.op {
        js::UnaryOp::Bang => Ok(Expr::Not(Box::new(self.expr(&u.arg)?))),
        js::UnaryOp::Minus => match &*u.arg {
          js::Expr::Lit(js::Lit::Num(n)) => Ok(Expr::Lit(Lit::Float(-n.value))),
          js::Expr::Lit(js::Lit::BigInt(b)) => Ok(Expr::Lit(Lit::Int(-self.bigint(b)?))),
          other => Ok(Expr::Arith(ArithOp::Sub, Box::new(Expr::Lit(Lit::Float(0.0))), Box::new(self.expr(other)?))),
        },
        js::UnaryOp::Plus => Ok(Expr::Num(Box::new(self.expr(&u.arg)?))),
        js::UnaryOp::Tilde if self.kind_of(&u.arg) == Some(Kind::BigInt) => Err(self.residue(u.span, "the `~` operator on a `bigint`")),
        js::UnaryOp::Tilde => Ok(Expr::Arith(ArithOp::BitXor, Box::new(self.expr(&u.arg)?), Box::new(Expr::Lit(Lit::Float(-1.0))))),
        js::UnaryOp::Delete => Err(self.residue(u.span, "`delete` inside an expression")),
        js::UnaryOp::TypeOf => Ok(match self.kind_of(&u.arg) {
          Some(Kind::BigInt) => Expr::lit_str("bigint"),
          Some(Kind::Map | Kind::Set | Kind::Date | Kind::Query | Kind::Url | Kind::Response) => Expr::lit_str("object"),
          Some(Kind::Browser) | None => Expr::Builtin { name: Builtin::TypeOf, args: vec![self.expr(&u.arg)?] },
        }),
        other => Err(self.residue(u.span, format!("the `{}` operator", other))),
      },
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::InstanceOf => self.instance_of(bin),
      js::Expr::Bin(bin) if is_bitwise(bin.op) && (self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt)) => Err(self.residue(bin.span, format!("the `{}` operator on a `bigint`", bin.op))),
      js::Expr::Bin(bin) => {
        let l = Box::new(self.expr(&bin.left)?);
        let r = Box::new(self.expr(&bin.right)?);
        Ok(match bin.op {
          js::BinaryOp::Add => Expr::Arith(ArithOp::Add, l, r),
          js::BinaryOp::Sub => Expr::Arith(ArithOp::Sub, l, r),
          js::BinaryOp::Mul => Expr::Arith(ArithOp::Mul, l, r),
          js::BinaryOp::Div if self.kind_of(&bin.left) == Some(Kind::BigInt) || self.kind_of(&bin.right) == Some(Kind::BigInt) => Expr::Arith(ArithOp::Quot, l, r),
          js::BinaryOp::Div => Expr::Arith(ArithOp::Div, l, r),
          js::BinaryOp::Mod => Expr::Arith(ArithOp::Rem, l, r),
          js::BinaryOp::Exp => Expr::Arith(ArithOp::Pow, l, r),
          js::BinaryOp::EqEq | js::BinaryOp::NotEq if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) || matches!(*l, Expr::Lit(Lit::Null | Lit::Undefined)) => {
            let other = if matches!(*r, Expr::Lit(Lit::Null | Lit::Undefined)) { l } else { r };
            let nullish = Expr::Builtin { name: Builtin::IsNullish, args: vec![*other] };
            if bin.op == js::BinaryOp::EqEq { nullish } else { Expr::Not(Box::new(nullish)) }
          }
          js::BinaryOp::EqEqEq | js::BinaryOp::EqEq => Expr::Compare(CompareOp::Eq, l, r),
          js::BinaryOp::NotEqEq | js::BinaryOp::NotEq => Expr::Compare(CompareOp::Ne, l, r),
          js::BinaryOp::Lt => Expr::Compare(CompareOp::Lt, l, r),
          js::BinaryOp::LtEq => Expr::Compare(CompareOp::Le, l, r),
          js::BinaryOp::Gt => Expr::Compare(CompareOp::Gt, l, r),
          js::BinaryOp::GtEq => Expr::Compare(CompareOp::Ge, l, r),
          js::BinaryOp::LogicalAnd => Expr::Logic(LogicOp::And, l, r),
          js::BinaryOp::LogicalOr => Expr::Logic(LogicOp::Or, l, r),
          js::BinaryOp::NullishCoalescing => Expr::Coalesce(l, r),
          js::BinaryOp::In => Expr::Builtin { name: Builtin::HasKey, args: vec![*r, *l] },
          js::BinaryOp::BitAnd => Expr::Arith(ArithOp::BitAnd, l, r),
          js::BinaryOp::BitOr => Expr::Arith(ArithOp::BitOr, l, r),
          js::BinaryOp::BitXor => Expr::Arith(ArithOp::BitXor, l, r),
          js::BinaryOp::LShift => Expr::Arith(ArithOp::Shl, l, r),
          js::BinaryOp::RShift => Expr::Arith(ArithOp::Shr, l, r),
          js::BinaryOp::ZeroFillRShift => Expr::Arith(ArithOp::UShr, l, r),
          other => return Err(self.residue(bin.span, format!("the `{}` operator", other))),
        })
      }
      js::Expr::Cond(c) => Ok(Expr::Ternary(
        Box::new(self.expr(&c.test)?),
        Box::new(self.expr(&c.cons)?),
        Box::new(self.expr(&c.alt)?),
      )),
      js::Expr::Member(member) => self.member(member),
      js::Expr::Call(call) => self.call(call),
      js::Expr::Arrow(arrow) => self.lambda(arrow),
      js::Expr::OptChain(o) => match &*o.base {
        js::OptChainBase::Member(member) => self.member(member),
        js::OptChainBase::Call(call) if o.optional => Err(self.residue_with(call.span, "an optional call of a function value", "the server holds no function values; call a builtin or a helper the build can follow")),
        js::OptChainBase::Call(call) => self.optional_call(call),
      },
      js::Expr::New(n) => self.new_expr(n),
      js::Expr::Fn(f) => self.function_expr(&f.function),
      js::Expr::Assign(a) if self.cell_of_target(&a.left).is_some() => match self.effect(expr)? {
        Stmt::Expr(write) => Ok(write),
        _ => Err(self.residue(a.span, "an assignment inside an expression")),
      },
      js::Expr::Assign(a) => Err(self.residue(a.span, "an assignment inside an expression")),
      js::Expr::This(_) if self.scope.iter().any(|(n, _)| n == "this") => Ok(self.scope.iter().rev().find(|(n, _)| n == "this").map(|(_, e)| e.clone()).expect("checked")),
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

  pub(crate) fn ident(&mut self, id: &js::Ident) -> Lowered<Expr> {
    let name = id.sym.as_ref();
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
        _ => {
          self.unbound = Some(name.to_owned());
          match crate::component::find_import(self.parsed, name) {
            Some((source, _)) => Err(self.residue(id.span, format!("`{name}` comes from `{source}`, which the build cannot follow"))),
            None => Err(self.residue(id.span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body"))),
          }
        }
      },
    }
  }

  fn lit(&self, lit: &js::Lit, span: Span) -> Lowered<Expr> {
    Ok(Expr::Lit(match lit {
      js::Lit::Str(s) => Lit::Str(s.value.to_atom_lossy().to_string()),
      js::Lit::Num(n) => Lit::Float(n.value),
      js::Lit::BigInt(b) => Lit::Int(self.bigint(b)?),
      js::Lit::Bool(b) => Lit::Bool(b.value),
      js::Lit::Null(_) => Lit::Null,
      js::Lit::Regex(re) => return regex_value(self, re.exp.as_ref(), re.flags.as_ref(), span),
      js::Lit::JSXText(_) => return Err(self.residue(span, "JSX")),
    }))
  }

  fn bigint(&self, b: &js::BigInt) -> Lowered<i128> {
    b.value
      .to_string()
      .parse::<i128>()
      .map_err(|_| self.residue(b.span, "a bigint literal outside 128 bits"))
  }

  /// A member read. Context roots become reads; anything else is a field.
  fn member(&mut self, member: &js::MemberExpr) -> Lowered<Expr> {
    if let Some(read) = self.env_read(member)? {
      return Ok(read);
    }
    if let (js::Expr::Ident(id), Some(key)) = (&*member.obj, self.member_name(member)) {
      if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) && self.meanings.as_ref().and_then(|m| m.of_ident(id)) == Some(&crate::meaning::Meaning::Config(None)) {
        return Ok(Expr::Config(key));
      }
    }
    if let Some(read) = self.class_static_read(member)? {
      return Ok(read);
    }

    let prop = |this: &mut Self| -> Lowered<Result<String, Expr>> {
      Ok(match &member.prop {
        js::MemberProp::Ident(id) => Ok(id.sym.to_string()),
        js::MemberProp::Computed(c) => match &*c.expr {
          js::Expr::Lit(js::Lit::Str(s)) => Ok(s.value.to_atom_lossy().to_string()),
          other => Err(this.expr(other)?),
        },
        js::MemberProp::PrivateName(p) => return Err(this.residue(p.span, "a private name")),
      })
    };

    if let js::Expr::Ident(id) = &*member.obj {
      if id.sym.as_ref() == "Math" && !self.scope.iter().any(|(n, _)| n == "Math") && !self.globals.iter().any(|(n, _)| n == "Math") {
        if let js::MemberProp::Ident(name) = &member.prop {
          use std::f64::consts;
          let value = match name.sym.as_ref() {
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
      if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) {
        match self.root_of(id) {
          Some(Root::Params) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed param name")) };
            return Ok(Expr::Param(name));
          }
          Some(Root::Query) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed query key")) };
            return Ok(Expr::Query(name));
          }
          Some(Root::Session) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed session key")) };
            return Ok(self.session_read(name));
          }
          Some(Root::Identity) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed identity field")) };
            return Ok(Expr::Identity(vec![name]));
          }
          Some(Root::Config) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed config key")) };
            return Ok(Expr::Config(name));
          }
          Some(Root::Ctx) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed context field")) };
            return match name.as_str() {
              "input" => Ok(Expr::Input),
              "now" => Ok(Expr::Now),
              "locale" => Ok(Expr::Locale),
              "path" => Ok(Expr::Path),
              "host" => Ok(Expr::Host),
              "origin" => Ok(Expr::Origin),
              "address" => Ok(Expr::Address),
              "identity" => Ok(Expr::Identity(Vec::new())),
              "params" | "query" | "session" | "config" => Err(self.residue(member.span, format!("`ctx.{name}` as a whole; read one of its fields"))),
              "services" => Err(self.residue(member.span, "`ctx.services` as a value; call a method on it")),
              "native" => Err(self.residue(member.span, "`ctx.native` as a value; call a method on it")),
              _ => Err(self.residue(member.span, format!("`{name}` is not a field of the context"))),
            };
          }
          Some(Root::Services) => return Err(self.residue(member.span, "a service as a value; call a method on it")),
          Some(Root::Native) => return Err(self.residue(member.span, "a native module as a value; call a method on it")),
          _ => {}
        }
      }
    }

    if let js::Expr::Member(inner) = &*member.obj {
      if let js::Expr::Ident(id) = &*inner.obj {
        if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) {
          if let Some(Root::Ctx) = self.root_of(id) {
            let via = self.member_name(inner);
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed context field")) };
            match via.as_deref() {
              Some("params") => return Ok(Expr::Param(name)),
              Some("query") => return Ok(Expr::Query(name)),
              Some("session") => return Ok(self.session_read(name)),
              Some("identity") => return Ok(Expr::Identity(vec![name])),
              _ => {}
            }
          }
          if let Some(Root::Identity) = self.root_of(id) {
            let Some(first) = self.member_name(inner) else { return Err(self.residue(inner.span, "a computed identity field")) };
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed identity field")) };
            return Ok(Expr::Identity(vec![first, name]));
          }
        }
      }
    }

    if self.kind_of(&js::Expr::Member(member.clone())) == Some(Kind::Browser) {
      let shown = match (&*member.obj, &member.prop) {
        (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) => format!("`{}.{}`", obj.sym, prop.sym),
        _ => "a member".to_owned(),
      };
      return Err(self.browser_only(member.span, format!("{shown}, a value only the browser knows")));
    }
    let collection = self.kind_of(&member.obj);
    let target = self.expr(&member.obj)?;
    if collection == Some(Kind::Response) {
      return match prop(self)? {
        Ok(name) if matches!(name.as_str(), "status" | "ok") => Ok(Expr::Builtin { name: Builtin::ResponseMember, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span, format!("`.{name}` of a `Response`; `status`, `ok` and `headers.get` are read"))),
        Err(_) => Err(self.residue(member.span, "a computed member of a `Response`")),
      };
    }
    if collection == Some(Kind::Url) {
      return match prop(self)? {
        Ok(name) if name == "href" => Ok(target),
        Ok(name) if name == "searchParams" => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str("search")] }),
        Ok(name) if URL_PARTS.contains(&name.as_str()) => Ok(Expr::Builtin { name: Builtin::UrlPart, args: vec![target, Expr::lit_str(&name)] }),
        Ok(name) => Err(self.residue(member.span, format!("`.{name}` of a `URL`"))),
        Err(_) => Err(self.residue(member.span, "a computed part of a `URL`")),
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
  fn context_read(&mut self, call: &js::CallExpr, callee: &js::Expr) -> Lowered<Option<Expr>> {
    let Some(meanings) = self.meanings.clone() else { return Ok(None) };
    let Some(hook) = meanings.of_expr(callee).and_then(|meant| meant.hook()) else { return Ok(None) };
    if hook != "use" && hook != "useContext" {
      return Ok(None);
    }
    match call.args.first().and_then(|a| meanings.of_expr(&a.expr)) {
      Some(meaning::Meaning::Context { id, default, .. }) => Ok(Some(Expr::Coalesce(Box::new(Expr::Context(id)), Box::new(default)))),
      _ => Err(match hook {
        "use" => self.residue(call.span, "`use` of something other than a `createContext` value; a promise is awaited in the browser"),
        _ => self.residue(call.span, "`useContext` of something the build cannot follow to a `createContext`"),
      }),
    }
  }

  /// The length of `Array(n)` or `new Array(n)` with `Array` the global.
  fn array_of_length<'e>(&self, expr: &'e js::Expr) -> Option<&'e js::Expr> {
    let (callee, args): (&js::Expr, &[js::ExprOrSpread]) = match strip_parens(expr) {
      js::Expr::Call(call) => match &call.callee {
        js::Callee::Expr(callee) => (callee, &call.args),
        _ => return None,
      },
      js::Expr::New(made) => (&made.callee, made.args.as_deref().unwrap_or_default()),
      _ => return None,
    };
    let js::Expr::Ident(id) = strip_parens(callee) else { return None };
    let global = self.meanings.as_ref().is_none_or(|meanings| id.ctxt == meanings.unresolved);
    match args {
      [length] if id.sym.as_ref() == "Array" && global && length.spread.is_none() => Some(&length.expr),
      _ => None,
    }
  }

  /// What React's server renderer returns for a hook whose value does not wait on the browser: `useDeferredValue`'s initial value when given and its value otherwise, `useFormStatus`'s status of no form pending, `useLinkStatus`'s of no link pending and `useSyncExternalStore`'s server snapshot.
  pub(crate) fn server_hook_value(&mut self, hook: &str, call: &js::CallExpr) -> Lowered<Expr> {
    match hook {
      "useDeferredValue" => match call.args.get(1).or(call.args.first()) {
        Some(a) => self.expr(&a.expr),
        None => Ok(Expr::Lit(Lit::Null)),
      },
      "useFormStatus" => Ok(Expr::Object(["pending", "data", "method", "action"].map(|key| Entry::Field(key.to_owned(), Expr::Lit(if key == "pending" { Lit::Bool(false) } else { Lit::Null }))).to_vec())),
      "useLinkStatus" => Ok(Expr::Object(vec![Entry::Field("pending".to_owned(), Expr::Lit(Lit::Bool(false)))])),
      _ => match call.args.get(2) {
        Some(server) => Ok(Expr::Apply { f: Box::new(self.expr(&server.expr)?), args: Vec::new() }),
        None => Err(self.residue(call.span, "`useSyncExternalStore` without `getServerSnapshot`, which React's server renderer refuses too")),
      },
    }
  }

  pub(crate) fn call(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    if self.calls_next(call) {
      let request = match call.args.as_slice() {
        [] => Expr::Lit(Lit::Undefined),
        [request] => self.expr(&request.expr)?,
        _ => return Err(self.residue(call.span, "`next` takes one request")),
      };
      return Ok(Expr::Next(Box::new(request)));
    }
    if let Some(read) = self.response_header_read(call)? {
      return Ok(read);
    }
    if let (Some(reader), js::Callee::Expr(callee)) = (&self.reader, &call.callee) {
      if matches!(&**callee, js::Expr::Ident(id) if id.sym == reader.0 && id.ctxt == reader.1) {
        let named = call.args.first().ok_or_else(|| self.residue(call.span, "a compute's `read` takes the source key"))?;
        return match self.expr(&named.expr)? {
          Expr::Lit(Lit::Str(key)) => Ok(Expr::Store(key)),
          _ => Err(self.residue(named.expr.span(), "a source key the build cannot read")),
        };
      }
    }
    if let Some(suspended) = self.suspend_call(call)? {
      return Ok(suspended);
    }
    if let (true, js::Callee::Expr(callee), Some(js::Expr::Lit(js::Lit::Str(id)))) = (self.reaches(Reach::Process), &call.callee, call.args.first().map(|a| crate::meaning::bare(&a.expr))) {
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClientAction)) {
        return Ok(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::ACT_REF.to_owned(), Expr::lit_str(id.value.to_string_lossy().into_owned()))]));
      }
    }
    if self.reaches(Reach::Process) && self.spawns(call) {
      return self.spawn_value(call);
    }
    if let Some(cwd) = self.process_call(call) {
      return Ok(cwd);
    }
    if let Some(chained) = self.then_call(call)? {
      return Ok(chained);
    }
    if let js::Callee::Expr(callee) = &call.callee {
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::Emit)) {
        let mut args = Vec::with_capacity(2);
        for a in call.args.iter().take(2) {
          args.push(self.expr(&a.expr)?);
        }
        return Ok(Expr::Builtin { name: Builtin::Emit, args });
      }
      if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)).and_then(|meant| meant.store_call()) == Some("set") {
        let [key, value] = call.args.as_slice() else { return Err(self.residue(call.span, "`set` takes a key and a value")) };
        let Expr::Lit(Lit::Str(key)) = self.expr(&key.expr)? else { return Err(self.residue(key.expr.span(), "a store key that is not a string the build can read")) };
        return Ok(Expr::WriteStore(key, Box::new(self.expr(&value.expr)?)));
      }
    }
    let js::Callee::Expr(callee) = &call.callee else {
      return Err(self.residue(call.span, "`super` or `import()`"));
    };
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
      let mut args = Vec::with_capacity(call.args.len());
      for a in &call.args {
        args.push(self.expr(&a.expr)?);
      }
      return Ok(self.candidate(call.span, Expr::Ext { module: "i18n".to_owned(), name: "t".to_owned(), args }));
    }
    let element_builtin = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::CloneElement)) => Some(Builtin::CloneElement),
      Some(meaning::Meaning::Library(meaning::Construct::IsValidElement)) => Some(Builtin::IsElement),
      _ => None,
    };
    if let Some(name) = element_builtin {
      let mut args = Vec::with_capacity(call.args.len());
      for a in &call.args {
        args.push(self.expr(&a.expr)?);
      }
      return Ok(Expr::Builtin { name, args });
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::SvelteStore(store))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      return self.svelte_store(store, call);
    }
    if let Some(meaning::Meaning::Library(meaning::Construct::Extension(key))) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) {
      if key == "crypto.digest" {
        return Err(self.residue(call.span, "`createHash` on its own; it is read as `createHash(algorithm).update(part).digest(encoding)`"));
      }
      let (module, member) = key.split_once('.').expect("an extension is `module.member`");
      let reach = standard_reach(module, member).expect("a package maps to a standard member");
      if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
        return Err(self.residue(call.span, format!("a call to `{key}`, which {needs}")));
      }
      self.check_reach(call.span, key, reach)?;
      let mut args = Vec::with_capacity(call.args.len());
      for a in &call.args {
        if a.spread.is_some() {
          return Err(self.residue(a.expr.span(), format!("a spread into `{key}`")));
        }
        args.push(self.expr(&a.expr)?);
      }
      return Ok(Expr::Ext { module: module.to_owned(), name: member.to_owned(), args });
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::ClassNames)) {
      let mut items = Vec::with_capacity(call.args.len());
      for a in &call.args {
        items.push(Entry::Item(self.expr(&a.expr)?));
      }
      return Ok(Expr::Builtin { name: Builtin::ClassNames, args: vec![Expr::Array(items)] });
    }
    if let js::Expr::Member(member) = &**callee {
      if let Some(module) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(&member.obj)).and_then(|meant| meant.std()) {
        let method = self.member_name(member).ok_or_else(|| self.residue(member.span, "a computed method name"))?;
        let key = format!("{module}.{method}");
        let reach = standard_reach(module, &method).ok_or_else(|| self.residue(member.span, format!("`{key}` is not a member of the standard library")))?;
        self.check_reach(call.span, &key, reach)?;
        let mut args = Vec::with_capacity(call.args.len());
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        let expr = Expr::Ext { module: module.to_owned(), name: method, args };
        return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
      }
    }
    if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) == Some(meaning::Meaning::Library(meaning::Construct::StoreKey)) {
      let named = call.args.first().ok_or_else(|| self.residue(call.span, "`key` takes the key's name"))?;
      return self.expr(&named.expr);
    }
    let math = matches!(&**callee, js::Expr::Member(m) if matches!(&*m.obj, js::Expr::Ident(id) if id.sym.as_ref() == "Math"));
    let applied = matches!(&**callee, js::Expr::Ident(id) if self.scope.iter().any(|(n, _)| *n == *id.sym) || self.local_fns.iter().any(|(n, _)| *n == *id.sym) || self.globals.iter().any(|(n, _)| *n == *id.sym));
    for arg in &call.args {
      if arg.spread.is_some() && !math && !applied {
        if let js::Expr::Ident(id) = &**callee {
          self.unbound = Some(id.sym.to_string());
        }
        return Err(self.residue(arg.expr.span(), "a spread argument"));
      }
    }

    if let js::Expr::Ident(id) = &**callee {
      let name = id.sym.as_ref();
      if !self.scope.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() {
        if let Some(function) = self.local_fns.iter().rev().find(|(n, _)| n == name).map(|(_, f)| f.clone()) {
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
              let mut args = Vec::with_capacity(call.args.len());
              for a in &call.args {
                args.push(self.expr(&a.expr)?);
              }
              let expr = Expr::Ext { module: module.clone(), name: member.clone(), args };
              return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
            }
          }
          if !matches!(*f, Expr::Lambda { .. }) {
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
          let a = call.args.first().ok_or_else(|| this.residue(call.span, format!("`{name}` takes one argument")))?;
          Ok(Box::new(this.expr(&a.expr)?))
        };
        return match name {
          "String" => Ok(Expr::Str(one(self)?)),
          "Number" => Ok(Expr::Num(one(self)?)),
          "BigInt" => Ok(Expr::BigInt(one(self)?)),
          "Boolean" => Ok(Expr::Not(Box::new(Expr::Not(one(self)?)))),
          "encodeURIComponent" => Ok(Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*one(self)?] }),
          "isNaN" => Ok(Expr::Builtin { name: Builtin::IsNaN, args: vec![*one(self)?] }),
          "parseInt" => {
            let mut args = vec![*one(self)?];
            if let Some(radix) = call.args.get(1) {
              args.push(self.expr(&radix.expr)?);
            }
            Ok(Expr::Builtin { name: Builtin::ParseInt, args })
          }
          name if ERROR_CONSTRUCTORS.contains(&name) => {
            let message = match call.args.first() {
              Some(a) => Expr::Str(Box::new(self.expr(&a.expr)?)),
              None => Expr::lit_str(""),
            };
            Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), message)]))
          }
          "fail" => Err(self.residue(call.span, "`fail` inside an expression; it is a statement")),
          "redirect" if self.parsed.names_server(&call.callee, "redirect") => self.redirect_to(call),
          "after" if self.parsed.names_server(&call.callee, "after") => self.after_of(call),
          "$state" | "$derived" if self.parsed.file.ends_with(".svelte.ts") || self.parsed.file.ends_with(".svelte.js") => match call.args.first() {
            Some(_) => Ok(*one(self)?),
            None => Ok(Expr::Lit(Lit::Null)),
          },
          _ => {
            self.unbound = Some(name.to_owned());
            Err(self.residue_with(
              id.span,
              format!("a call to `{name}`, which the build cannot follow"),
              "the build reads a helper declared at module level, or imported from a file under the app; a bare package import is not followed, and neither is a name nothing declares",
            ))
          }
        };
      }
    }

    let js::Expr::Member(member) = &**callee else {
      if matches!(crate::meaning::bare(callee), js::Expr::Arrow(_) | js::Expr::Fn(_)) {
        return Err(self.residue_with(
          callee.span(),
          "a function called where it is written",
          "the build does not run a function to produce a value; write the value as an expression or move the steps into a named function the body calls",
        ));
      }
      let f = Box::new(self.expr(callee)?);
      return self.applied(f, call);
    };
    let method = self.member_name(member).ok_or_else(|| self.residue(member.span, "a computed method name"))?;
    if method == "digest" {
      if let Some(folded) = self.hash_chain(member, call)? {
        return Ok(folded);
      }
    }

    if let js::Expr::Ident(obj) = &*member.obj {
      let global = obj.sym.as_ref();
      if !self.scope.iter().any(|(n, _)| n == global) && !self.globals.iter().any(|(n, _)| n == global) {
        match global {
          "Object" => {
            let a = call.args.first().ok_or_else(|| self.residue(call.span, format!("`Object.{method}` takes one argument")))?;
            let target = Box::new(self.expr(&a.expr)?);
            return match method.as_str() {
              "entries" => Ok(Expr::Entries(target)),
              "fromEntries" => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![*target] }),
              "keys" => Ok(Expr::Keys(target)),
              "values" => Ok(Expr::Values(target)),
              _ => Err(self.residue(member.span, format!("`Object.{method}`"))),
            };
          }
          "Math" if crate::JS_MATH.contains(&method.as_str()) => {
            let mut args = vec![Expr::lit_str(method.as_str())];
            for a in &call.args {
              if a.spread.is_some() {
                return Err(self.residue(a.expr.span(), "a spread argument"));
              }
              args.push(self.expr(&a.expr)?);
            }
            return Ok(Expr::Builtin { name: Builtin::Math, args });
          }
          "Number" if method == "parseInt" => {
            let Some(first) = call.args.first() else { return Err(self.residue(call.span, "`Number.parseInt` takes a string and an optional radix")) };
            let mut args = vec![self.expr(&first.expr)?];
            if let Some(radix) = call.args.get(1) {
              args.push(self.expr(&radix.expr)?);
            }
            return Ok(Expr::Builtin { name: Builtin::ParseInt, args });
          }
          "Number" if matches!(method.as_str(), "isInteger" | "isSafeInteger" | "isFinite" | "isNaN") => {
            let Some(arg) = call.args.first() else { return Err(self.residue(call.span, format!("`Number.{method}` takes one argument"))) };
            let x = self.expr(&arg.expr)?;
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
          "Math" if method == "random" && self.reaches(Reach::Terminal) && call.args.is_empty() => {
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
              _ => return Err(self.residue(member.span, format!("`Math.{method}`"))),
            };
            if matches!(name, Builtin::Min | Builtin::Max) && call.args.iter().any(|a| a.spread.is_some()) {
              let mut entries = Vec::with_capacity(call.args.len());
              for a in &call.args {
                let value = self.expr(&a.expr)?;
                entries.push(if a.spread.is_some() { Entry::Spread(value) } else { Entry::Item(value) });
              }
              let name = if name == Builtin::Min { Builtin::MinOf } else { Builtin::MaxOf };
              return Ok(Expr::Builtin { name, args: vec![Expr::Array(entries)] });
            }
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              if a.spread.is_some() {
                return Err(self.residue(a.expr.span(), "a spread argument"));
              }
              args.push(self.expr(&a.expr)?);
            }
            return Ok(Expr::Builtin { name, args });
          }
          "Response" if matches!(method.as_str(), "json" | "redirect") => {
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              args.push(self.expr(&a.expr)?);
            }
            let name = if method == "json" { Builtin::ResponseJson } else { Builtin::ResponseRedirect };
            return Ok(Expr::Builtin { name, args });
          }
          "JSON" if method == "parse" => {
            let [arg] = call.args.as_slice() else { return Err(self.residue(call.span, "`JSON.parse` takes the text alone; a reviver is not lowered")) };
            return Ok(Expr::Builtin { name: Builtin::JsonParse, args: vec![self.expr(&arg.expr)?] });
          }
          "JSON" if method == "stringify" => {
            if call.args.len() > 3 || call.args.get(1).is_some_and(|a| !matches!(&*a.expr, js::Expr::Lit(js::Lit::Null(_)))) {
              return Err(self.residue(call.span, "`JSON.stringify` with a replacer; it takes a value, `null` and an indent"));
            }
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              args.push(self.expr(&a.expr)?);
            }
            return Ok(Expr::Builtin { name: Builtin::Json, args });
          }
          "Array" if method == "isArray" => {
            let [arg] = call.args.as_slice() else { return Err(self.residue(call.span, "`Array.isArray` takes one argument")) };
            return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(&arg.expr)?] });
          }
          "Array" if method == "from" && call.args.first().is_some_and(|a| !matches!(&*a.expr, js::Expr::Object(_))) => {
            let source = &call.args[0].expr;
            let mut items = self.expr(source)?;
            items = match self.kind_of(source) {
              Some(Kind::Map) => Expr::Entries(Box::new(items)),
              _ => Expr::Array(vec![Entry::Spread(items)]),
            };
            return match call.args.get(1) {
              None => Ok(items),
              Some(f) => {
                let js::Expr::Arrow(arrow) = &*f.expr else {
                  return Err(self.residue(f.expr.span(), "`Array.from` takes an arrow function written in place"));
                };
                Ok(Expr::Map(Box::new(items), Box::new(self.lambda(arrow)?)))
              }
            };
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
                let arg = call.args.first().ok_or_else(|| self.residue(call.span, "`Date.parse` without a string"))?;
                Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![self.expr(&arg.expr)?] })
              }
              other => Err(self.residue(member.span, format!("`Date.{other}`"))),
            };
          }
          "Array" if method == "from" => {
            let (Some(shape), Some(f)) = (call.args.first(), call.args.get(1)) else {
              return Err(self.residue(call.span, "`Array.from` takes `{ length }` and a function"));
            };
            let js::Expr::Object(obj) = &*shape.expr else {
              return Err(self.residue(shape.expr.span(), "`Array.from` over something other than `{ length: n }`"));
            };
            let length = obj.props.iter().find_map(|p| match p {
              js::PropOrSpread::Prop(p) => match &**p {
                js::Prop::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some("length") => Some(&kv.value),
                _ => None,
              },
              _ => None,
            });
            let Some(length) = length else { return Err(self.residue(shape.expr.span(), "`Array.from` over something other than `{ length: n }`")) };
            let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![self.expr(length)?] });
            let js::Expr::Arrow(arrow) = &*f.expr else {
              return Err(self.residue(f.expr.span(), "`Array.from` takes an arrow function written in place"));
            };
            return Ok(Expr::Map(range, Box::new(self.lambda(arrow)?)));
          }
          _ => {}
        }
      }
    }

    if let Some((service, _via_ctx)) = self.service_of(&member.obj) {
      let args = self.object_args(call, "service")?;
      return Ok(Expr::Call { service, method, args });
    }

    if let Some((module, _via_ctx)) = self.native_of(&member.obj) {
      let args = self.object_args(call, "native")?;
      // `sync` is set by the build from the Rust signature, which this reader
      // cannot see; the interpreter answers a sync method without a future
      // either way.
      return Ok(Expr::NativeCall { module, method, args, sync: false });
    }

    if method == "fill" {
      if let Some(length) = self.array_of_length(&member.obj) {
        let length = self.expr(length)?;
        let value = match call.args.first() {
          Some(arg) if call.args.len() == 1 => self.expr(&arg.expr)?,
          Some(_) => return Err(self.residue(call.span, "`fill` with a start or an end")),
          None => Expr::Lit(Lit::Null),
        };
        let range = Box::new(Expr::Builtin { name: Builtin::Range, args: vec![length] });
        return Ok(Expr::Map(range, Box::new(Expr::Lambda { params: vec!["$filled".to_owned()], body: Box::new(value) })));
      }
    }
    if method == "segment" || method == "containing" {
      let (segmenter, text) = match (method.as_str(), strip_parens(&member.obj)) {
        ("segment", obj) => (self.formatter_of(obj), call.args.first().map(|a| (*a.expr).clone())),
        ("containing", js::Expr::Call(inner)) => match &inner.callee {
          js::Callee::Expr(callee) => match strip_parens(callee) {
            js::Expr::Member(m) if matches!(&m.prop, js::MemberProp::Ident(p) if p.sym.as_ref() == "segment") => (self.formatter_of(&m.obj), inner.args.first().map(|a| (*a.expr).clone())),
            _ => (None, None),
          },
          _ => (None, None),
        },
        _ => (None, None),
      };
      if let Some(made) = segmenter.filter(|made| intl_formatter(made) == Some("Segmenter")) {
        self.grapheme_segmenter(&made)?;
        let text = text.ok_or_else(|| self.residue(call.span, "`segment` without a string"))?;
        let mut args = vec![self.expr(&text)?];
        if method == "containing" {
          let at = call.args.first().ok_or_else(|| self.residue(call.span, "`containing` without an index"))?;
          args.push(self.expr(&at.expr)?);
        }
        return Ok(Expr::Builtin { name: Builtin::Graphemes, args });
      }
    }
    if method == "format" {
      if let Some(made) = self.formatter_of(&member.obj) {
        if let Some(formatter) = intl_formatter(&made) {
          let value = call.args.first().ok_or_else(|| self.residue(call.span, "`format` without a value"))?;
          let value = self.expr(&value.expr)?;
          let made_args: Vec<js::ExprOrSpread> = made.args.clone().unwrap_or_default();
          let expr = self.intl_format(formatter, &made_args, value, made.span)?;
          return Ok(self.candidate(call.span, expr));
        }
      }
    }
    match self.kind_of(&member.obj) {
      Some(Kind::Map) => {
        let target = self.expr(&member.obj)?;
        let key = |this: &mut Self| -> Lowered<Expr> {
          let arg = call.args.first().ok_or_else(|| this.residue(call.span, format!("`{method}` without a key")))?;
          Ok(Expr::Str(Box::new(this.expr(&arg.expr)?)))
        };
        return match method.as_str() {
          "get" => Ok(Expr::Index(Box::new(target), Box::new(key(self)?))),
          "has" => Ok(Expr::Builtin { name: Builtin::HasKey, args: vec![target, key(self)?] }),
          "keys" => Ok(Expr::Keys(Box::new(target))),
          "values" => Ok(Expr::Values(Box::new(target))),
          "entries" => Ok(Expr::Entries(Box::new(target))),
          other => Err(self.residue(member.span, format!("`.{other}()` of a `Map`"))),
        };
      }
      Some(Kind::Set) => {
        let target = self.expr(&member.obj)?;
        return match method.as_str() {
          "has" => {
            let arg = call.args.first().ok_or_else(|| self.residue(call.span, "`has` without a value"))?;
            Ok(Expr::Builtin { name: Builtin::Includes, args: vec![target, self.expr(&arg.expr)?] })
          }
          "values" | "keys" => Ok(target),
          other => Err(self.residue(member.span, format!("`.{other}()` of a `Set`"))),
        };
      }
      Some(Kind::Query) if method != "toString" => {
        let target = self.expr(&member.obj)?;
        return match method.as_str() {
          "get" => {
            let arg = call.args.first().ok_or_else(|| self.residue(call.span, "`get` without a key"))?;
            Ok(Expr::Builtin { name: Builtin::FormGet, args: vec![target, Expr::Str(Box::new(self.expr(&arg.expr)?))] })
          }
          "has" | "getAll" => {
            let arg = call.args.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without a key")))?;
            let name = if method == "has" { Builtin::FormHas } else { Builtin::FormGetAll };
            Ok(Expr::Builtin { name, args: vec![target, Expr::Str(Box::new(self.expr(&arg.expr)?))] })
          }
          "entries" => Ok(Expr::Builtin { name: Builtin::FormEntries, args: vec![target] }),
          other => Err(self.residue(member.span, format!("`.{other}()` of a `URLSearchParams`"))),
        };
      }
      _ => {}
    }

    let target = Box::new(self.expr(&member.obj)?);
    let lambda = |this: &mut Self, i: usize| -> Lowered<Box<Expr>> {
      let a = call.args.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes a function")))?;
      if let js::Expr::Ident(id) = &*a.expr {
        let name = id.sym.as_ref();
        if name == "Boolean" && !this.scope.iter().any(|(n, _)| n == name) && this.root_of(id).is_none() && !this.globals.iter().any(|(n, _)| n == name) {
          let item = this.temp();
          return Ok(Box::new(Expr::Lambda { params: vec![item.clone()], body: Box::new(Expr::Not(Box::new(Expr::Not(Box::new(Expr::Var(item)))))) }));
        }
      }
      let js::Expr::Arrow(arrow) = &*a.expr else {
        return Err(this.residue(a.expr.span(), format!("`{method}` takes an arrow function written in place")));
      };
      this.lambda(arrow).map(Box::new)
    };
    match method.as_str() {
      "map" => Ok(Expr::Map(target, lambda(self, 0)?)),
      "filter" => Ok(Expr::Filter(target, lambda(self, 0)?)),
      "find" => Ok(Expr::Find(target, lambda(self, 0)?)),
      "findIndex" => Ok(Expr::FindIndex(target, lambda(self, 0)?)),
      "some" => Ok(Expr::Some(target, lambda(self, 0)?)),
      "every" => Ok(Expr::Every(target, lambda(self, 0)?)),
      "flatMap" => Ok(Expr::FlatMap(target, lambda(self, 0)?)),
      "flat" if call.args.is_empty() || matches!(&*call.args[0].expr, js::Expr::Lit(js::Lit::Num(n)) if n.value == 1.0) => Ok(Expr::FlatMap(target, Box::new(Expr::Lambda { params: vec!["$item".to_owned()], body: Box::new(Expr::Var("$item".to_owned())) }))),
      "toString" if call.args.is_empty() => Ok(Expr::Str(target)),
      "toString" => Ok(Expr::Builtin { name: Builtin::Radix, args: vec![*target, self.expr(&call.args[0].expr)?] }),
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
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" if self.reaches(Reach::Terminal) && call.args.is_empty() => {
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
      "toLocaleTimeString" if self.reaches(Reach::Terminal) && call.args.len() <= 1 => {
        let locale = match call.args.first().map(|a| strip_parens(&a.expr)) {
          None => Expr::Lit(Lit::Null),
          Some(js::Expr::Ident(id)) if id.sym.as_ref() == "undefined" => Expr::Lit(Lit::Null),
          Some(js::Expr::Lit(js::Lit::Str(tag))) => Expr::lit_str(tag.value.to_atom_lossy().to_string()),
          Some(other) => return Err(self.residue_with(other.span(), "a locale that is not written in place", "write the locale, `\"en-US\"`; `undefined` takes the system's")),
        };
        Ok(Expr::Ext { module: "intl".to_owned(), name: "localTime".to_owned(), args: vec![*target, locale] })
      }
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTimezoneOffset" | "toLocaleTimeString" | "toTimeString" | "toDateString" => {
        let utc = method.replacen("get", "getUTC", 1);
        let mut residue = self.browser_only(member.span, format!("`.{method}()`, which reads the viewer's time zone"));
        residue.hint = Some(format!("as text under an element the browser fills it in when the component hydrates; the server has no viewer, so `.{utc}()` and `toLocaleDateString(undefined, {{ dateStyle }})` read UTC, which both agree on"));
        Err(residue)
      }
      "localeCompare" => {
        let other = match call.args.first() {
          Some(arg) => self.expr(&arg.expr)?,
          None => return Err(self.residue(call.span, "`localeCompare` without a string to compare with")),
        };
        if call.args.len() > 2 {
          return Err(self.residue(call.span, "`localeCompare` options; the build reads a locale"));
        }
        let mut entries = Vec::new();
        if let Some(locale) = call.args.get(1) {
          if !matches!(&*locale.expr, js::Expr::Ident(id) if id.sym == "undefined") {
            entries.push(Entry::Field("locale".to_owned(), self.expr(&locale.expr)?));
          }
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "compare".to_owned(), args: vec![*target, other, Expr::Object(entries)] })
      }
      "toLocaleDateString" => {
        let mut made = call.args.clone();
        made.truncate(2);
        let expr = self.intl_format("DateTimeFormat", &made, *target, call.span)?;
        Ok(self.candidate(call.span, expr))
      }
      "toSorted" | "sort" | "toReversed" | "reverse" => {
        if matches!(method.as_str(), "sort" | "reverse") && !fresh_array(&member.obj) {
          return Err(self.residue_with(member.span, format!("`.{method}()` of an array something else holds, which it rewrites in place"), format!("`[...items].{method}()` or `items.slice().{method}()` gives the same order without the write")));
        }
        if method.contains("everse") {
          return Ok(Expr::Builtin { name: Builtin::Reverse, args: vec![*target] });
        }
        let f = match call.args.first() {
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
        for a in &call.args {
          if a.spread.is_some() {
            return Err(self.residue(a.expr.span(), "a spread argument"));
          }
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Builtin { name, args })
      }
      "reduce" => {
        let rewritten = match (call.args.first().map(|a| strip_parens(&a.expr)), call.args.get(1).map(|a| strip_parens(&a.expr))) {
          (Some(js::Expr::Arrow(arrow)), Some(js::Expr::Object(_))) => fresh_accumulator(arrow),
          _ => None,
        };
        let f = match rewritten {
          Some(arrow) => Box::new(self.lambda(&arrow)?),
          None => lambda(self, 0)?,
        };
        let Some(init) = call.args.get(1) else {
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
        let init = Box::new(self.expr(&init.expr)?);
        Ok(Expr::Reduce(target, init, f))
      }
      "trimStart" | "trimEnd" | "trimLeft" | "trimRight" if call.args.is_empty() => {
        let side = if matches!(method.as_str(), "trimStart" | "trimLeft") { "start" } else { "end" };
        Ok(Expr::Builtin { name: Builtin::Trim, args: vec![*target, Expr::lit_str(side)] })
      }
      "toFixed" | "repeat" | "join" | "trim" | "toUpperCase" | "toLowerCase" | "includes" | "toLocaleString" => {
        let name = match method.as_str() {
          "toFixed" => Builtin::ToFixed,
          "repeat" => Builtin::Repeat,
          "join" => Builtin::Join,
          "trim" => Builtin::Trim,
          "toUpperCase" => Builtin::Upper,
          "toLowerCase" => Builtin::Lower,
          "includes" => Builtin::Includes,
          _ => Builtin::LocaleNumber,
        };
        let mut args = vec![*target];
        if name == Builtin::LocaleNumber {
          if let Some(a) = call.args.first() {
            if !matches!(&*a.expr, js::Expr::Lit(js::Lit::Str(s)) if s.value.to_atom_lossy().as_ref() == "en-US") {
              return Err(self.residue(a.expr.span(), "`toLocaleString` with a locale other than \"en-US\""));
            }
          }
          if call.args.len() > 1 {
            let value = args.remove(0);
            let formatted = self.intl_format("NumberFormat", &call.args, value, call.span)?;
            return Ok(self.candidate(call.span, formatted));
          }
          return Ok(self.candidate(call.span, Expr::Builtin { name, args }));
        }
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Builtin { name, args })
      }
      "test" | "match" | "matchAll" | "search" => {
        let arg = call.args.first().ok_or_else(|| self.residue(call.span, format!("`{method}` without an argument")))?;
        let arg = self.expr(&arg.expr)?;
        Ok(match method.as_str() {
          "test" => Expr::Builtin { name: Builtin::RegexTest, args: vec![*target, arg] },
          "match" => Expr::Builtin { name: Builtin::Match, args: vec![*target, arg] },
          "matchAll" => Expr::Builtin { name: Builtin::MatchAll, args: vec![*target, arg] },
          _ => Expr::Builtin { name: Builtin::Search, args: vec![*target, arg] },
        })
      }
      "replace" | "replaceAll" if call.args.len() == 2 && matches!(&*call.args[1].expr, js::Expr::Arrow(_)) => {
        let js::Expr::Arrow(arrow) = &*call.args[1].expr else { unreachable!("checked") };
        let pattern = match (&*call.args[0].expr, method.as_str()) {
          (js::Expr::Lit(js::Lit::Str(s)), "replaceAll") => regex_value(self, &escape_regex(s.value.to_atom_lossy().as_ref()), "g", call.span)?,
          (js::Expr::Lit(js::Lit::Regex(re)), "replaceAll") if !re.flags.contains('g') => return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on")),
          (pattern, "replaceAll") if !matches!(pattern, js::Expr::Lit(js::Lit::Regex(_))) => return Err(self.residue(call.span, "`replaceAll` of a pattern the build cannot read with a function")),
          (pattern, _) => self.expr(pattern)?,
        };
        let f = self.lambda(arrow)?;
        Ok(Expr::ReplaceWith(target, Box::new(pattern), Box::new(f)))
      }
      "replaceAll" => {
        let [from, to] = call.args.as_slice() else { return Err(self.residue(call.span, format!("`replaceAll` takes 2 arguments, got {}", call.args.len()))) };
        let to = self.expr(&to.expr)?;
        match &*from.expr {
          js::Expr::Lit(js::Lit::Regex(re)) => {
            if !re.flags.contains('g') {
              return Err(self.residue(call.span, "`replaceAll` of a regular expression without `g`, which JavaScript throws on"));
            }
            let pattern = self.expr(&from.expr)?;
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
        let limited = name == Builtin::Split && call.args.len() == 2;
        if call.args.len() != arity && !limited {
          let plural = if arity == 1 { "" } else { "s" };
          return Err(self.residue(call.span, format!("`{method}` takes {arity} argument{plural}, got {}", call.args.len())));
        }
        let mut args = vec![*target];
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Builtin { name, args })
      }
      other if !STANDARD_METHODS.contains(&other) => {
        let mut args = Vec::with_capacity(call.args.len());
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Apply { f: Box::new(Expr::Field(target, other.to_owned())), args })
      }
      other => Err(self.residue_with(
        member.span,
        format!("`.{other}()`, which is not a builtin"),
        "the builtins are `map`, `filter`, `find`, `findIndex`, `some`, `every`, `reduce`, `flatMap`, `flat`, `toSorted`, `toReversed`, `slice`, `at`, `indexOf`, `lastIndexOf`, `charAt`, `charCodeAt`, `codePointAt`, `concat`, `join`, `includes`, `trim`, `repeat`, `toFixed`, `toString`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `substring`, `split`, `startsWith`, `endsWith`, `replace`, `replaceAll`, `match`, `matchAll`, `search`, `test`, `localeCompare` and `toLocaleString`; anything else goes in a module-level helper the build can read",
      )),
    }
  }

  /// The one object literal a service or native method takes, as named
  /// arguments. `kind` names the caller in a diagnostic.
  fn object_args(&mut self, call: &js::CallExpr, kind: &str) -> Lowered<Vec<(String, Expr)>> {
    let mut args = Vec::new();
    if let Some(a) = call.args.first() {
      let js::Expr::Object(obj) = &*a.expr else {
        return Err(self.residue(a.expr.span(), format!("{kind} arguments must be an object literal")));
      };
      for prop in &obj.props {
        match prop {
          js::PropOrSpread::Prop(p) => match &**p {
            js::Prop::Shorthand(id) => args.push((id.sym.to_string(), self.ident(id)?)),
            js::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key).ok_or_else(|| self.residue(kv.key.span(), "a computed argument name"))?;
              args.push((key, self.expr(&kv.value)?));
            }
            other => return Err(self.residue(other.span(), "a method in the arguments")),
          },
          js::PropOrSpread::Spread(s) => return Err(self.residue(s.expr.span(), format!("a spread into {kind} arguments"))),
        }
      }
    }
    if call.args.len() > 1 {
      return Err(self.residue(call.span, format!("a {kind} method takes one object")));
    }
    Ok(args)
  }

  /// `native.<name>` or `ctx.native.<name>`.
  fn native_of(&self, obj: &js::Expr) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Native, "native")
  }

  /// `services.<name>` or `ctx.services.<name>`.
  fn service_of(&self, obj: &js::Expr) -> Option<(String, bool)> {
    self.rooted_at(obj, Root::Services, "services")
  }

  /// `<root>.<name>` bare or `ctx.<field>.<name>`; the bool says which.
  fn rooted_at(&self, obj: &js::Expr, root: Root, field: &str) -> Option<(String, bool)> {
    let js::Expr::Member(m) = obj else { return None };
    let name = self.member_name(m)?;
    match &*m.obj {
      js::Expr::Ident(id) if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) => match self.root_of(id) {
        Some(held) if held == root => Some((name, false)),
        _ => None,
      },
      js::Expr::Member(inner) => {
        let js::Expr::Ident(id) = &*inner.obj else { return None };
        if self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) {
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
  /// `new Promise((resolve, reject) => …)` in a terminal page: a fresh deferred, the executor run at once with `resolve` and `reject` held as [`snapfire_fsr_ir::host::SETTLES`] values on it, then the deferred.
  fn terminal_promise(&mut self, n: &js::NewExpr, args: &[js::ExprOrSpread]) -> Lowered<Expr> {
    let Some(js::Expr::Arrow(executor)) = args.first().map(|a| crate::meaning::bare(&a.expr)) else { return Err(self.residue(n.span, "a `Promise` whose executor is not an arrow written in place")) };
    let deferred = self.temp();
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let mut body = vec![Stmt::Let { name: deferred.clone(), expr: Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)]) }];
    let depth = self.scope.len();
    let settlers = self.settlers.len();
    for (param, ok) in executor.params.iter().zip([true, false]) {
      let js::Pat::Ident(name) = param else { return Err(self.residue(param.span(), "a `Promise` executor's parameter that is not a name")) };
      let settle = Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), Expr::Var(deferred.clone())), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))]);
      self.scope.push((name.id.sym.to_string(), settle));
      self.settlers.push(name.id.sym.to_string());
    }
    let ran = match &*executor.body {
      js::ArrowFunctionBody::FunctionBody(b) => crate::component::block_to_expr(self, &b.stmts),
      js::ArrowFunctionBody::Expr(e) => self.effect(e).map(|stmt| Expr::Block(vec![stmt])),
    };
    self.scope.truncate(depth);
    self.settlers.truncate(settlers);
    body.push(Stmt::Expr(ran?));
    body.push(Stmt::Return(Expr::Var(deferred)));
    Ok(Expr::Block(body))
  }

  fn new_expr(&mut self, n: &js::NewExpr) -> Lowered<Expr> {
    let args: Vec<js::ExprOrSpread> = n.args.clone().unwrap_or_default();
    if let Some(spread) = args.iter().find(|a| a.spread.is_some()) {
      return Err(self.residue(spread.expr.span(), "a spread argument"));
    }
    let svelte = match self.meanings.as_ref().and_then(|meanings| meanings.of_expr(&n.callee)) {
      Some(meaning::Meaning::Library(meaning::Construct::SvelteClass(class))) => Some(class),
      _ => None,
    };
    if let Some("Spring" | "Tween") = svelte {
      let value = match args.first() {
        Some(a) => self.expr(&a.expr)?,
        None => Expr::Lit(Lit::Null),
      };
      return Ok(Expr::Object(vec![Entry::Field("current".to_owned(), value.clone()), Entry::Field("target".to_owned(), value)]));
    }
    if self.reaches(Reach::Later) && matches!(&*n.callee, js::Expr::Ident(id) if id.ctxt == self.parsed.unresolved && id.sym.as_ref() == "Promise") {
      return self.terminal_promise(n, &args);
    }
    if let js::Expr::Ident(id) = &*n.callee {
      if id.ctxt == self.parsed.unresolved && ERROR_CONSTRUCTORS.contains(&id.sym.as_ref()) {
        let message = match args.first() {
          Some(a) => Expr::Str(Box::new(self.expr(&a.expr)?)),
          None => Expr::lit_str(""),
        };
        return Ok(Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(id.sym.as_ref())), Entry::Field("message".to_owned(), message)]));
      }
    }
    let global = match &*n.callee {
      _ if svelte.is_some() => svelte.expect("checked").to_owned(),
      js::Expr::Ident(id) if id.ctxt == self.parsed.unresolved => id.sym.to_string(),
      _ if intl_formatter(n).is_some() => return Err(self.residue_with(n.span, "an `Intl` formatter kept as a value", "call `.format(x)` on it where it is made, `new Intl.NumberFormat(undefined, options).format(n)`")),
      other => match self.class_new(n)? {
        Some(made) => return Ok(made),
        None => return Err(self.residue(other.span(), "`new` of something other than `Date`, `Map`, `Set`, `URL`, `URLSearchParams` or a class the build reads")),
      },
    };
    let first = match args.first() {
      Some(a) => Some(self.expr(&a.expr)?),
      None => None,
    };
    match (global.as_str(), first) {
      ("Date", None) => self.date_now(n.span),
      ("Date", Some(x)) if args.len() == 1 => Ok(Expr::Builtin { name: Builtin::DateMs, args: vec![x] }),
      ("Date", Some(_)) => Err(self.residue_with(n.span, "`new Date(year, month, ...)`, which reads the viewer's time zone", "an ISO 8601 string ending in `Z` names the same instant everywhere, `new Date(\"2026-10-05T00:00:00Z\")`")),
      ("Map", None) => Ok(Expr::Object(Vec::new())),
      ("Map", Some(pairs)) => Ok(Expr::Builtin { name: Builtin::FromEntries, args: vec![pairs] }),
      ("Set", None) => Ok(Expr::Array(Vec::new())),
      ("Set", Some(items)) => Ok(Expr::Builtin { name: Builtin::Unique, args: vec![items] }),
      ("Response", body) => {
        let mut args = vec![body.unwrap_or(Expr::Lit(Lit::Undefined))];
        if let Some(init) = n.args.as_deref().and_then(|a| a.get(1)) {
          args.push(self.expr(&init.expr)?);
        }
        Ok(Expr::Builtin { name: Builtin::Response, args })
      }
      ("Request", Some(url)) => Ok(Expr::Builtin { name: Builtin::RequestNew, args: vec![url] }),
      ("Uint8Array", source) => Ok(Expr::Builtin { name: Builtin::Uint8Array, args: source.into_iter().collect() }),
      ("URLSearchParams", None) => Ok(Expr::lit_str("")),
      ("URLSearchParams", Some(x)) => Ok(Expr::Builtin { name: Builtin::FormEncode, args: vec![x] }),
      ("URL", Some(x)) => {
        let mut args = vec![x];
        if let Some(base) = n.args.as_deref().and_then(|a| a.get(1)) {
          args.push(self.expr(&base.expr)?);
        }
        Ok(Expr::Builtin { name: Builtin::UrlParse, args })
      }
      ("RegExp", Some(_)) if matches!(&*args[0].expr, js::Expr::Lit(js::Lit::Regex(_))) => Err(self.residue(n.span, "`new RegExp` of a regular expression")),
      ("RegExp", Some(pattern)) => {
        let flags = match args.get(1) {
          Some(a) => Some(&*a.expr),
          None => None,
        };
        if let (js::Expr::Lit(js::Lit::Str(p)), None | Some(js::Expr::Lit(js::Lit::Str(_)))) = (&*args[0].expr, flags) {
          let flags = match flags {
            Some(js::Expr::Lit(js::Lit::Str(f))) => f.value.to_atom_lossy().to_string(),
            _ => String::new(),
          };
          return regex_value(self, p.value.to_atom_lossy().as_ref(), &flags, n.span);
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
  fn grapheme_segmenter(&mut self, made: &js::NewExpr) -> Lowered<()> {
    if !self.reaches(Reach::Terminal) {
      return Err(self.browser_only(made.span, "`Intl.Segmenter`, which splits by the browser's own Unicode tables"));
    }
    let granularity = made.args.as_ref().and_then(|args| args.get(1)).map(|options| match &*options.expr {
      js::Expr::Object(obj) => obj.props.iter().find_map(|prop| match prop {
        js::PropOrSpread::Prop(prop) => match &**prop {
          js::Prop::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some("granularity") => Some(match &*kv.value {
            js::Expr::Lit(js::Lit::Str(s)) => s.value.to_atom_lossy().to_string(),
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

  /// `new Intl.NumberFormat(locale, options).format(value)` and `new Intl.DateTimeFormat(locale, options).format(value)` as the `intl` member that formats the same way under the request's locale, or under the locale written there.
  fn intl_format(&mut self, formatter: &str, made: &[js::ExprOrSpread], value: Expr, span: Span) -> Lowered<Expr> {
    let locale = match made.first().map(|a| strip_parens(&a.expr)) {
      None => None,
      Some(js::Expr::Ident(id)) if id.sym.as_ref() == "undefined" => None,
      Some(js::Expr::Lit(js::Lit::Str(s))) => Some(Expr::lit_str(s.value.to_atom_lossy().to_string())),
      Some(other) => Some(self.expr(other)?),
    };
    let mut options: Vec<(String, js::Expr)> = Vec::new();
    if let Some(given) = made.get(1) {
      let js::Expr::Object(obj) = &*given.expr else { return Err(self.residue(given.expr.span(), "formatting options that are not an object literal")) };
      for prop in &obj.props {
        let js::PropOrSpread::Prop(prop) = prop else { return Err(self.residue(obj.span, "a spread in formatting options")) };
        let js::Prop::KeyValue(kv) = &**prop else { return Err(self.residue(obj.span, "a formatting option written other than `key: value`")) };
        let key = prop_name(&kv.key).ok_or_else(|| self.residue(kv.key.span(), "a computed formatting option"))?;
        options.push((key, (*kv.value).clone()));
      }
    }
    let literal = |options: &[(String, js::Expr)], key: &str| options.iter().find(|(k, _)| k == key).and_then(|(_, v)| match v {
      js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_atom_lossy().to_string()),
      _ => None,
    });
    let locale_entry = locale.map(|tag| Entry::Field("locale".to_owned(), tag));
    match formatter {
      "NumberFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        if literal(&options, "style").as_deref() == Some("currency") {
          let Some((_, code)) = options.iter().find(|(k, _)| k == "currency") else { return Err(self.residue(span, "a currency style with no `currency`")) };
          let code = self.expr(&code.clone())?;
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
              other => return Err(self.residue(span, format!("the formatting option `{other}` with a currency"))),
            }
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "currency".to_owned(), args: vec![value, code, Expr::Object(entries)] });
        }
        for (key, v) in &options {
          if !matches!(key.as_str(), "minimumFractionDigits" | "maximumFractionDigits") {
            return Err(self.residue(span, format!("the number formatting option `{key}`; `minimumFractionDigits`, `maximumFractionDigits` and a currency are read")));
          }
          entries.push(Entry::Field(key.clone(), self.expr(v)?));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "number".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      "ListFormat" => {
        let mut entries: Vec<Entry> = locale_entry.into_iter().collect();
        for (key, v) in &options {
          if !matches!(key.as_str(), "type" | "style") {
            return Err(self.residue(span, format!("the list formatting option `{key}`; `type` and `style` are read")));
          }
          entries.push(Entry::Field(key.clone(), self.expr(v)?));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "list".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      _ => {
        let utc = |options: &[(String, js::Expr)]| options.iter().all(|(k, _)| k != "timeZone") || literal(options, "timeZone").as_deref() == Some("UTC");
        if !utc(&options) {
          return Err(self.residue(span, "a `timeZone` other than `\"UTC\"`"));
        }
        if let Some(style) = literal(&options, "dateStyle") {
          if let Some((other, _)) = options.iter().find(|(k, _)| !matches!(k.as_str(), "dateStyle" | "timeZone")) {
            return Err(self.residue(span, format!("the date formatting option `{other}` beside `dateStyle`")));
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
            js::Expr::Lit(js::Lit::Str(s)) => s.value.to_atom_lossy().to_string(),
            _ => return Err(self.residue(v.span(), format!("a `{key}` that is not written in place"))),
          };
          let read = match (key.as_str(), text.as_str()) {
            ("timeZone", _) => continue,
            ("weekday" | "month", "long" | "short" | "narrow") => true,
            ("year" | "day", "numeric") => true,
            ("month", "numeric" | "2-digit") => return Err(self.residue_with(v.span(), "a numeric month", "`month: \"short\"` or a `dateStyle`; the server has no digits that agree with the browser's")),
            ("weekday" | "month" | "year" | "day", _) => false,
            _ => return Err(self.residue(span, format!("the date formatting option `{key}`; `dateStyle`, `weekday`, `year`, `month`, `day` and `timeZone: \"UTC\"` are read"))),
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
  fn optional_call(&mut self, call: &js::OptCall) -> Lowered<Expr> {
    let mut guards = Vec::new();
    let callee = strip_optional(&call.callee, &mut guards);
    // A call on a value known to be null short-circuits, as JavaScript does, whatever the method.
    for guard in &guards {
      if let js::Expr::Ident(id) = strip_parens(guard) {
        if matches!(self.scope.iter().rev().find(|(n, _)| n == id.sym.as_ref()), Some((_, Expr::Lit(Lit::Null)))) {
          return Ok(Expr::Lit(Lit::Null));
        }
      }
    }
    let plain = js::CallExpr { span: call.span, ctxt: call.ctxt, callee: js::Callee::Expr(Box::new(callee)), args: call.args.clone(), type_args: None };
    let mut out = self.call(&plain)?;
    for guard in guards.iter().rev() {
      let object = self.expr(guard)?;
      let missing = Expr::Builtin { name: Builtin::IsNullish, args: vec![object] };
      out = Expr::Ternary(Box::new(missing), Box::new(Expr::Lit(Lit::Undefined)), Box::new(out));
    }
    Ok(out)
  }

  /// A `function` expression or a method as the lambda it is; a generator is residue.
  /// `createHash(algorithm).update(part)….digest(encoding)` as one `crypto.digest` call, the parts in order after the algorithm and the encoding; `None` when the chain under `.digest` does not start at `createHash` from `node:crypto`.
  fn hash_chain(&mut self, member: &js::MemberExpr, call: &js::CallExpr) -> Lowered<Option<Expr>> {
    let mut parts: Vec<&js::Expr> = Vec::new();
    let mut receiver: &js::Expr = &member.obj;
    loop {
      let js::Expr::Call(inner) = crate::meaning::bare(receiver) else { return Ok(None) };
      let js::Callee::Expr(callee) = &inner.callee else { return Ok(None) };
      match crate::meaning::bare(callee) {
        js::Expr::Member(m) if self.member_name(m).as_deref() == Some("update") => {
          let Some(part) = inner.args.first() else { return Ok(None) };
          parts.push(&part.expr);
          receiver = &m.obj;
        }
        callee => {
          if self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) != Some(meaning::Meaning::Library(meaning::Construct::Extension("crypto.digest"))) {
            return Ok(None);
          }
          let reach = Reach::Render;
          if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
            return Err(self.residue(call.span, format!("a call to `crypto.digest`, which {needs}")));
          }
          self.check_reach(call.span, "crypto.digest", reach)?;
          let Some(algorithm) = inner.args.first() else { return Err(self.residue(inner.span, "`createHash` without an algorithm")) };
          let Some(encoding) = call.args.first() else { return Err(self.residue(call.span, "`digest()` without an encoding; `hex` or `base64` reads the bytes")) };
          let mut args = vec![self.expr(&algorithm.expr)?, self.expr(&encoding.expr)?];
          for part in parts.iter().rev() {
            args.push(self.expr(part)?);
          }
          return Ok(Some(Expr::Ext { module: "crypto".to_owned(), name: "digest".to_owned(), args }));
        }
      }
    }
  }

  /// `f` applied to `call`'s arguments: [`Expr::Apply`], or [`Expr::ApplyOf`] over one array when an argument is spread.
  fn applied(&mut self, f: Box<Expr>, call: &js::CallExpr) -> Lowered<Expr> {
    let mut args = Vec::with_capacity(call.args.len());
    for a in &call.args {
      args.push((a.spread.is_some(), self.expr(&a.expr)?));
    }
    if args.iter().all(|(spread, _)| !spread) {
      return Ok(Expr::Apply { f, args: args.into_iter().map(|(_, e)| e).collect() });
    }
    let entries = args.into_iter().map(|(spread, e)| if spread { Entry::Spread(e) } else { Entry::Item(e) }).collect();
    Ok(Expr::ApplyOf(f, Box::new(Expr::Array(entries))))
  }

  pub(crate) fn function_expr(&mut self, f: &js::Function) -> Lowered<Expr> {
    if f.is_generator {
      return Err(self.residue(f.span, "a generator function"));
    }
    let depth = self.scope.len();
    let pats: Vec<js::Pat> = f.params.iter().map(|p| p.pat.clone()).collect();
    let params = match crate::component::bind_params(self, &pats) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let body = match &f.body {
      Some(body) => crate::component::block_to_expr(self, &body.stmts),
      None => Ok(Expr::Lit(Lit::Null)),
    };
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }

  pub(crate) fn lambda(&mut self, arrow: &js::ArrowExpr) -> Lowered<Expr> {
    let depth = self.scope.len();
    let params = match crate::component::bind_params(self, &arrow.params) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let body = match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => self.expr(e),
      js::ArrowFunctionBody::FunctionBody(b) if !writes_a_member_of(&b.stmts, &params) => crate::component::block_to_expr_from(self, &b.stmts, depth),
      js::ArrowFunctionBody::FunctionBody(b) => crate::component::block_to_expr(self, &b.stmts),
    };
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }
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
fn strip_optional(e: &js::Expr, guards: &mut Vec<js::Expr>) -> js::Expr {
  match e {
    js::Expr::OptChain(o) => match &*o.base {
      js::OptChainBase::Member(m) => {
        let obj = strip_optional(&m.obj, guards);
        if o.optional {
          guards.push(obj.clone());
        }
        js::Expr::Member(js::MemberExpr { span: m.span, obj: Box::new(obj), prop: m.prop.clone() })
      }
      js::OptChainBase::Call(_) => e.clone(),
    },
    js::Expr::Member(m) => js::Expr::Member(js::MemberExpr { span: m.span, obj: Box::new(strip_optional(&m.obj, guards)), prop: m.prop.clone() }),
    js::Expr::Paren(p) => strip_optional(&p.expr, guards),
    other => other.clone(),
  }
}

/// The kind `e` makes in place: a `new Map(...)`, `new Set(...)`, `new URLSearchParams(...)`, `new URL(...)` or `new Date(...)`, a bigint literal or `BigInt(...)`.
pub(crate) fn made_kind(e: &js::Expr) -> Option<Kind> {
  match e {
    js::Expr::Paren(p) => made_kind(&p.expr),
    js::Expr::TsAs(a) => made_kind(&a.expr),
    js::Expr::TsNonNull(a) => made_kind(&a.expr),
    js::Expr::New(n) => match &*n.callee {
      js::Expr::Ident(id) if matches!(id.sym.as_ref(), "Map" | "SvelteMap") => Some(Kind::Map),
      js::Expr::Ident(id) if matches!(id.sym.as_ref(), "Set" | "SvelteSet") => Some(Kind::Set),
      js::Expr::Ident(id) if matches!(id.sym.as_ref(), "URLSearchParams" | "SvelteURLSearchParams") => Some(Kind::Query),
      js::Expr::Ident(id) if matches!(id.sym.as_ref(), "URL" | "SvelteURL") => Some(Kind::Url),
      js::Expr::Ident(id) if matches!(id.sym.as_ref(), "Date" | "SvelteDate") => Some(Kind::Date),
      js::Expr::Ident(id) if id.sym.as_ref() == "Response" => Some(Kind::Response),
      _ => None,
    },
    js::Expr::Lit(js::Lit::BigInt(_)) => Some(Kind::BigInt),
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => match &**callee {
        js::Expr::Ident(id) if id.sym.as_ref() == "BigInt" => Some(Kind::BigInt),
        js::Expr::Member(m) if matches!((&*m.obj, &m.prop), (js::Expr::Ident(obj), js::MemberProp::Ident(method)) if &*obj.sym == "Response" && matches!(&*method.sym, "json" | "redirect")) => Some(Kind::Response),
        _ => None,
      },
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
    Stmt::Try { body, handler, finally, .. } => writes_session(body) || handler.as_deref().is_some_and(writes_session) || writes_session(finally),
    Stmt::Let { .. } | Stmt::Set { .. } | Stmt::Return(_) | Stmt::Guard { .. } | Stmt::Act { .. } | Stmt::Expr(_) => false,
  })
}

fn strip_parens(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => strip_parens(&p.expr),
    other => other,
  }
}

/// The text of a string literal or of a template literal with no `${}` part.
fn string_literal(e: &js::Expr) -> Option<String> {
  match crate::meaning::bare(e) {
    js::Expr::Lit(js::Lit::Str(s)) => Some(s.value.to_string_lossy().into_owned()),
    js::Expr::Tpl(tpl) if tpl.exprs.is_empty() && tpl.quasis.len() == 1 => {
      let quasi = &tpl.quasis[0];
      Some(quasi.cooked.as_ref().map(|c| c.to_atom_lossy().to_string()).unwrap_or_else(|| quasi.raw.to_string()))
    }
    _ => None,
  }
}

/// `new Intl.NumberFormat(...)`, `new Intl.DateTimeFormat(...)` or `new Intl.ListFormat(...)`: which one.
pub(crate) fn intl_formatter(n: &js::NewExpr) -> Option<&'static str> {
  let js::Expr::Member(m) = &*n.callee else { return None };
  let (js::Expr::Ident(ns), js::MemberProp::Ident(name)) = (&*m.obj, &m.prop) else { return None };
  if ns.sym.as_ref() != "Intl" {
    return None;
  }
  match name.sym.as_ref() {
    "NumberFormat" => Some("NumberFormat"),
    "DateTimeFormat" => Some("DateTimeFormat"),
    "ListFormat" => Some("ListFormat"),
    "Segmenter" => Some("Segmenter"),
    _ => None,
  }
}

/// Whether a statement of an async function holds an `await`: `await x;` and `const v = await x;` [`Lowerer::async_stmts`] splits at; one deeper it refuses.
/// The `catch` of a `try` an async function awaits in: its parameter, its statements followed by those after the `try`, and how many of the statements lowering the `try` covers.
#[derive(Clone)]
struct AsyncCatch {
  param: Option<String>,
  body: Vec<js::Stmt>,
  covers: usize,
  /// The catch over `body`'s first statements: a `finally` that runs before what the body throws goes on.
  inner: Option<Box<AsyncCatch>>,
}

/// The local a member write changes and the members from it outward: `x.a[i].b` is `x` with `.a`, `[i]` and `.b`.
pub(crate) fn write_root(member: &js::MemberExpr) -> Option<(&js::Ident, Vec<&js::MemberExpr>)> {
  let mut path = vec![member];
  let mut at = member;
  loop {
    match crate::meaning::bare(&at.obj) {
      js::Expr::Ident(id) => {
        path.reverse();
        return Some((id, path));
      }
      js::Expr::Member(inner) => {
        path.push(inner);
        at = inner;
      }
      _ => return None,
    }
  }
}

pub(crate) fn awaits(stmt: &js::Stmt) -> bool {
  struct Awaits(bool);
  impl swc_core::ecma::visit::Visit for Awaits {
    fn visit_await_expr(&mut self, _: &js::AwaitExpr) {
      self.0 = true;
    }
    fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
    fn visit_function(&mut self, _: &js::Function) {}
  }
  let mut found = Awaits(false);
  swc_core::ecma::visit::VisitWith::visit_with(stmt, &mut found);
  found.0
}

/// Whether `e` builds a new array, which `sort` and `reverse` may rewrite without another holder seeing it.
fn fresh_array(e: &js::Expr) -> bool {
  match e {
    js::Expr::Array(_) => true,
    js::Expr::Paren(p) => fresh_array(&p.expr),
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => match &**callee {
        js::Expr::Member(m) => matches!(&m.prop, js::MemberProp::Ident(p) if matches!(p.sym.as_ref(), "slice" | "map" | "filter" | "concat" | "toSorted" | "toReversed" | "flatMap" | "split" | "keys" | "values" | "entries" | "from")),
        _ => false,
      },
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

/// Whether `stmts` write a field of one of `params`, an object the caller holds too, so the parameters stay unwritable; rebinding a parameter itself is a local write.
fn writes_a_member_of(stmts: &[js::Stmt], params: &[String]) -> bool {
  use swc_core::ecma::visit::{Visit, VisitWith};
  struct Writes<'a>(&'a [String], bool);
  impl Visit for Writes<'_> {
    fn visit_simple_assign_target(&mut self, target: &js::SimpleAssignTarget) {
      if let js::SimpleAssignTarget::Member(member) = target {
        let mut root = &*member.obj;
        while let js::Expr::Member(inner) = root {
          root = &inner.obj;
        }
        if matches!(root, js::Expr::Ident(id) if self.0.iter().any(|p| *p == *id.sym)) {
          self.1 = true;
        }
      }
      target.visit_children_with(self);
    }
  }
  let mut writes = Writes(params, false);
  stmts.iter().for_each(|stmt| stmt.visit_with(&mut writes));
  writes.1
}

/// A `reduce` callback that writes its accumulator and answers it, `(acc[k] = v, acc)` or `{ acc.k = v; return acc; }`, as one that answers `({ ...acc, [k]: v })`. Only for an accumulator that starts as an object literal, which nothing else holds, so a fresh object answers what the writes would; only the first write may read the accumulator, since a later one would see the earlier write.
fn fresh_accumulator(arrow: &js::ArrowExpr) -> Option<js::ArrowExpr> {
  use swc_core::ecma::visit::{Visit, VisitWith};
  let js::Pat::Ident(acc) = arrow.params.first()? else { return None };
  let acc = acc.id.sym.clone();
  let mut writes: Vec<&js::AssignExpr> = Vec::new();
  let answers_acc = |e: &js::Expr| matches!(strip_parens(e), js::Expr::Ident(id) if id.sym == acc);
  match &*arrow.body {
    js::ArrowFunctionBody::Expr(e) => {
      let js::Expr::Seq(seq) = strip_parens(e) else { return None };
      let (last, rest) = seq.exprs.split_last()?;
      if !answers_acc(last) {
        return None;
      }
      for e in rest {
        let js::Expr::Assign(assign) = strip_parens(e) else { return None };
        writes.push(assign);
      }
    }
    js::ArrowFunctionBody::FunctionBody(block) => {
      let (last, rest) = block.stmts.split_last()?;
      let js::Stmt::Return(js::ReturnStmt { arg: Some(arg), .. }) = last else { return None };
      if !answers_acc(&**arg) {
        return None;
      }
      for stmt in rest {
        let js::Stmt::Expr(js::ExprStmt { expr, .. }) = stmt else { return None };
        let js::Expr::Assign(assign) = strip_parens(&**expr) else { return None };
        writes.push(assign);
      }
    }
  }
  struct Reads<'a>(&'a swc_core::atoms::Atom, bool);
  impl Visit for Reads<'_> {
    fn visit_ident(&mut self, id: &js::Ident) {
      if id.sym == *self.0 {
        self.1 = true;
      }
    }
  }
  let mut props = vec![js::PropOrSpread::Spread(js::SpreadElement { dot3_token: arrow.span, expr: Box::new(js::Expr::Ident(js::Ident::new_no_ctxt(acc.clone(), arrow.span))) })];
  for (i, assign) in writes.iter().enumerate() {
    if assign.op != js::AssignOp::Assign {
      return None;
    }
    let js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) = &assign.left else { return None };
    if !matches!(&*member.obj, js::Expr::Ident(id) if id.sym == acc) {
      return None;
    }
    let key = match &member.prop {
      js::MemberProp::Ident(name) => js::PropName::Ident(name.clone()),
      js::MemberProp::Computed(computed) => js::PropName::Computed(computed.clone()),
      js::MemberProp::PrivateName(_) => return None,
    };
    if i > 0 {
      let mut reads = Reads(&acc, false);
      assign.right.visit_with(&mut reads);
      member.prop.visit_with(&mut reads);
      if reads.1 {
        return None;
      }
    }
    props.push(js::PropOrSpread::Prop(Box::new(js::Prop::KeyValue(js::KeyValueProp { key, value: assign.right.clone() }))));
  }
  let object = js::Expr::Object(js::ObjectLit { span: arrow.span, props });
  Some(js::ArrowExpr { body: Box::new(js::ArrowFunctionBody::Expr(Box::new(js::Expr::Paren(js::ParenExpr { span: arrow.span, expr: Box::new(object) })))), ..arrow.clone() })
}

pub(crate) fn prop_name(name: &js::PropName) -> Option<String> {
  match name {
    js::PropName::Ident(id) => Some(id.sym.to_string()),
    js::PropName::Str(s) => Some(s.value.to_atom_lossy().to_string()),
    js::PropName::Num(n) => Some(snapfire_fsr_ir::interp::number_key(n.value)),
    _ => None,
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
fn describe_stmt(stmt: &js::Stmt) -> (&'static str, &'static str) {
  match stmt {
    js::Stmt::While(_) | js::Stmt::DoWhile(_) => ("`while`, a loop whose length the build cannot know", "a body loops over data it already has: `map`, `filter`, `reduce`, `find` and `for...of`"),
    js::Stmt::For(_) | js::Stmt::ForIn(_) => ("a `for` loop other than `for...of`", "`for...of` over a list, or `map` and `filter`, which the interpreter runs directly"),
    js::Stmt::Switch(_) => ("`switch`", "a chain of `if`, or a ternary when every arm is an expression"),
    js::Stmt::Decl(js::Decl::Fn(_)) => ("a function declared inside a body", "declare it at module level and call it; the build follows a module-level helper"),
    js::Stmt::Decl(js::Decl::Class(_)) => ("a class", "the value model has objects and lists; an object literal carries the same fields"),
    js::Stmt::Decl(_) => ("a declaration the build does not read", "a body declares values with `const`"),
    js::Stmt::Break(_) | js::Stmt::Continue(_) => ("`break` or `continue`", "`find` stops at the first match and `filter` selects, so neither needs to leave a loop early"),
    js::Stmt::Labeled(_) => ("a label", "a body has no loops to jump out of; `find` and `filter` express the same intent"),
    js::Stmt::With(_) => ("`with`", "read what the body needs from `ctx` by name"),
    js::Stmt::Debugger(_) => ("`debugger`", "the body runs in Rust, where there is nothing to attach to; `fsr test` replays it instead"),
    js::Stmt::Empty(_) => ("an empty statement", "remove the stray `;`"),
    _ => ("a statement outside the IR", "the statements a body may use are `const`, `if`, `for...of`, `return`, `fail` and a session write"),
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
fn describe_expr(expr: &js::Expr) -> (&'static str, &'static str) {
  match expr {
    js::Expr::This(_) => ("`this`, and a body has no receiver", "read what the body needs from `ctx`"),
    js::Expr::Class(_) => ("a class", "the value model has objects and lists; an object literal carries the same fields"),
    js::Expr::Seq(_) => ("a comma expression", "one expression per statement"),
    js::Expr::Update(_) => ("`++` or `--`, which mutate", "a body's bindings do not change; bind the new value with `const n = n0 + 1`"),
    js::Expr::Yield(_) => ("`yield`", "a body runs once and returns; there is no generator in the serving path"),
    js::Expr::TaggedTpl(_) => ("a tagged template", "a plain template literal, or a module-level helper the build can follow"),
    js::Expr::JSXElement(_) | js::Expr::JSXFragment(_) => ("JSX in a body", "a body returns data; the page it feeds is where the markup goes"),
    js::Expr::MetaProp(_) => ("`import.meta`", "the build resolves every import, so nothing reads them while serving"),
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
  pub(crate) fn context_key(&self, framework: &str, key: &js::Expr) -> Lowered<String> {
    match crate::meaning::bare(key) {
      js::Expr::Lit(js::Lit::Str(s)) => Ok(format!("{framework}:{}", s.value.to_atom_lossy())),
      js::Expr::Ident(id) => Ok(match self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)).and_then(|meant| meant.origin()) {
        Some((declared, name)) => format!("{framework}:{declared}#{name}"),
        None => format!("{framework}:{}", id.sym),
      }),
      other => Err(self.residue(other.span(), "a context key that is not a string or a name")),
    }
  }

  fn svelte_store(&mut self, store: &str, call: &js::CallExpr) -> Lowered<Expr> {
    let arg = |i: usize| call.args.get(i).map(|a| &*a.expr);
    match store {
      "writable" | "readable" => {
        if call.args.len() > 1 {
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
          js::Expr::Arrow(arrow) => arrow.params.len(),
          js::Expr::Fn(f) => f.function.params.len(),
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
