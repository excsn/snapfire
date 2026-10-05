//! Reads a TypeScript loader or actions module and lowers each body to the IR.
//! The recognised language is the IR's own; anything outside it is residue,
//! reported with the line and the construct.

pub mod assets;
pub mod component;
pub mod extract;
pub mod hoist;
mod placements;
pub mod schema;
pub mod testing;
pub mod vue;

use component::FunctionBody;
use snapfire_fsr_ir::ast::{ArithOp, Body, Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use snapfire_fsr_ir::{standard_reach, Reach};
use swc_core::common::{sync::Lrc, FileName, SourceMap, Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

/// Where a component that does not lower was placed: the file holding the
/// tag, the one-based line and column of the tag, plus the name as written.
/// A residue collects one of these per level it is re-raised through, so the
/// page that stops being server rendered can name the path down to the cause.
#[derive(Debug, Clone, PartialEq, Eq)]
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
  pub natives: Vec<(String, Reach)>,
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
pub struct LoweredAction {
  pub export: String,
  pub input: Option<String>,
  pub body: Body,
}

/// A lowered route handler: the HTTP method it answers, the body and the
/// input type when the export is an `action<T>`.
#[derive(Debug, Clone, PartialEq)]
pub struct LoweredHandler {
  pub method: String,
  pub input: Option<String>,
  pub body: Body,
}

/// The exports of a `route.ts` that are handlers.
pub const HANDLER_METHODS: [&str; 5] = ["GET", "POST", "PUT", "PATCH", "DELETE"];

/// Session keys with the value a body reads when the key is absent, from
/// `export const defaults` in the session schema. A read of such a key lowers
/// to `session.key ?? default`.
pub type SessionDefaults = Vec<(String, Expr)>;

/// Lowers the exported `load` of a loader module.
pub fn lower_loader(file: &str, source: &str) -> Result<Body, LowerError> {
  lower_loader_with(file, source, &SessionDefaults::new())
}

pub fn lower_loader_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Body, LowerError> {
  let parsed = parse(file, source)?;
  lower_loader_in(&parsed, defaults, &Resolved::default()).map_err(|(e, _)| e)
}

pub(crate) fn lower_loader_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let function = parsed
    .exports()
    .find_map(|(name, decl)| (name == "load").then_some(decl))
    .ok_or_else(|| (LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None))?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err((LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None)),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`load` is {what}")).into(), None)),
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

/// Lowers the exported `paths` of a page loader module when there is one: a
/// function of the context returning the parameter sets the route
/// prerenders, one object per path. `None` when the module exports no `paths`.
pub fn lower_paths_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Option<Body>, LowerError> {
  let parsed = parse(file, source)?;
  lower_paths_in(&parsed, defaults, &Resolved::default()).map_err(|(e, _)| e)
}

/// An export whose input is the loader's data, bound as `data`.
fn lower_of_data(file: &str, source: &str, defaults: &SessionDefaults, export: &str) -> Result<Option<Body>, LowerError> {
  let parsed = parse(file, source)?;
  lower_of_data_in(&parsed, defaults, &Resolved::default(), export).map_err(|(e, _)| e)
}

pub(crate) fn lower_of_data_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, export: &str) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, export, true)
}

pub(crate) fn lower_paths_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, "paths", false)
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
    Exported::Action { .. } | Exported::BadAction(_) | Exported::Other(_) => return Err((parsed.residue(parsed.module.span, format!("`{export}` must be a function of {takes}")).into(), None)),
  };
  result.map(Some).map_err(|r| (r.into(), lowerer.unbound.take()))
}

/// Lowers every `export const name = action(...)` of an actions module.
pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError> {
  lower_actions_with(file, source, &SessionDefaults::new())
}

pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError> {
  let parsed = parse(file, source)?;
  lower_actions_in(&parsed, defaults, &Resolved::default()).map_err(|(e, _)| e)
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
  let parsed = parse(file, source)?;
  lower_handlers_in(&parsed, defaults, &Resolved::default()).map_err(|(e, _)| e)
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
      Exported::Other(span) => return Err((parsed.residue(span, format!("`{name}` must be a function or an `action(...)`")).into(), None)),
    };
    let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
    lowerer.extends = true;
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
  let parsed = parse(file, source)?;
  lower_middleware_in(&parsed, defaults, &Resolved::default()).map_err(|(e, _)| e)
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

pub(crate) fn parse(file: &str, source: &str) -> Result<Parsed, LowerError> {
  parse_with(file, source, false)
}

pub(crate) fn parse_with(file: &str, source: &str, tsx: bool) -> Result<Parsed, LowerError> {
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Custom(file.to_owned())), source.to_owned());
  let syntax = Syntax::Typescript(TsSyntax { tsx, decorators: true, ..Default::default() });
  let lexer = Lexer::new(syntax, js::EsVersion::latest(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  let module = parser.parse_module().map_err(|e| {
    let loc = cm.lookup_char_pos(e.span().lo);
    LowerError::Parse { file: file.to_owned(), message: format!("{}:{}: {}", loc.line, loc.col_display + 1, e.kind().msg()) }
  })?;
  Ok(Parsed { file: file.to_owned(), cm, module })
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
    let loc = self.cm.lookup_char_pos(span.lo);
    Residue { file: self.file.clone(), line: loc.line, column: loc.col_display + 1, message: message.into(), hint: None, via: Vec::new() }
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
    let loc = self.cm.lookup_char_pos(start + swc_core::common::BytePos(offset as u32));
    (loc.line, loc.col_display + 1)
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

/// The source `action` and `fail` are written by an application's server tier.
pub(crate) const SERVER_SOURCE: &str = "@snapfire/fsr";

impl Parsed {
  /// Whether `callee` names `want` from the server tier, however the module
  /// spelled it: the bare name, an aliased import or a namespace member.
  pub(crate) fn names_server(&self, callee: &js::Callee, want: &str) -> bool {
    let js::Callee::Expr(expr) = callee else { return false };
    if let js::Expr::Ident(id) = &**expr {
      if id.sym.as_ref() == want {
        return true;
      }
    }
    crate::component::imported_callee(self, expr).is_some_and(|(source, name)| source == SERVER_SOURCE && name == want)
  }

  fn classify<'a>(&'a self, init: &'a js::Expr) -> Exported<'a> {
    classify_in(self, init)
  }
}

fn classify_in<'a>(parsed: &'a Parsed, init: &'a js::Expr) -> Exported<'a> {
  match init {
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
}

pub(crate) struct Lowerer<'a> {
  parsed: &'a Parsed,
  defaults: &'a SessionDefaults,
  roots: Vec<(String, Root)>,
  /// A middleware body reads the request line as `request`, which is its input.
  middleware: bool,
  /// A meta body reads its loader's data as `data`, which is its input.
  meta: bool,
  /// An action, a route handler or middleware may call `session.extend`; a
  /// loader runs on every navigation, so it may not.
  extends: bool,
  pub(crate) scope: Vec<(String, Expr)>,
  /// Module-level names a component lowerer has resolved, read after the scope.
  pub(crate) globals: Vec<(String, Expr)>,
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
  /// Names bound to a `new Map(...)` or a `new Set(...)`, with the binding they were noted against, so `.size`, `.get` and `.has` read the collection only where the name still holds one.
  pub(crate) kinds: Vec<(String, Expr, Collection)>,
}

/// What a `new Map` or a `new Set` lowered to: an object keyed by `String(key)` or an array without repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Collection {
  Map,
  Set,
}

pub(crate) type Lowered<T> = Result<T, Residue>;

impl<'a> Lowerer<'a> {
  pub(crate) fn new(parsed: &'a Parsed, defaults: &'a SessionDefaults) -> Self {
    Self { parsed, defaults, roots: Vec::new(), scope: Vec::new(), globals: Vec::new(), unbound: None, middleware: false, meta: false, extends: false, hoisting: None, natives: Vec::new(), in_handler: false, render_path: false, reach_violation: false, temps: 0, kinds: Vec::new() }
  }

  pub(crate) fn resolved(mut self, resolved: &Resolved) -> Self {
    self.globals = resolved.globals.clone();
    self.natives = resolved.natives.clone();
    self
  }

  /// True while lowering a component's render path: the browser runs it too.
  fn on_render_path(&self) -> bool {
    (self.render_path || self.hoisting.is_some()) && !self.in_handler
  }

  /// Refuses a `body` extension on a render path.
  fn check_reach(&mut self, span: Span, key: &str, reach: Reach) -> Lowered<()> {
    if reach == Reach::Body && self.on_render_path() {
      self.reach_violation = true;
      return Err(self.residue(span, format!("`{key}` on a render path")));
    }
    Ok(())
  }

  /// The first `body` extension a lowered expression calls, by name.
  fn body_extension_in(&self, expr: &Expr) -> Option<String> {
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

  fn root_named(&self, name: &str) -> Option<Root> {
    if self.middleware && name == "request" {
      return Some(Root::Input);
    }
    if self.meta && name == "data" {
      return Some(Root::Input);
    }
    root_named(name)
  }

  fn block(&mut self, stmts: &[js::Stmt]) -> Lowered<Body> {
    let depth = self.scope.len();
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
      match stmt {
        js::Stmt::Decl(js::Decl::Var(var)) => {
          for decl in &var.decls {
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

  /// The collection `e` holds: a `new Map(...)` or `new Set(...)` itself, or a name noted as bound to one.
  pub(crate) fn collection_of(&self, e: &js::Expr) -> Option<Collection> {
    match e {
      js::Expr::Paren(p) => self.collection_of(&p.expr),
      js::Expr::TsAs(a) => self.collection_of(&a.expr),
      js::Expr::TsNonNull(a) => self.collection_of(&a.expr),
      js::Expr::New(n) => match &*n.callee {
        js::Expr::Ident(id) if id.sym.as_ref() == "Map" => Some(Collection::Map),
        js::Expr::Ident(id) if id.sym.as_ref() == "Set" => Some(Collection::Set),
        _ => None,
      },
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
        let bound = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e)?;
        self.kinds.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, kind)| *kind)
      }
      _ => None,
    }
  }

  /// Notes `name`, just bound to `init`, as a collection when `init` is one.
  pub(crate) fn note_kind(&mut self, name: &str, init: &js::Expr) {
    let Some(kind) = self.collection_of(init) else { return };
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()) else { return };
    self.kinds.push((name.to_owned(), bound, kind));
  }

  /// One declarator as a `let`: a name binds the value; a pattern binds it under a temporary and each name it declares reads a part of that.
  fn declare(&mut self, decl: &js::VarDeclarator) -> Lowered<Stmt> {
    let Some(init) = &decl.init else {
      return Err(self.residue(decl.span, "a declaration without a value"));
    };
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
        if self.collection_of(&for_of.right) == Some(Collection::Map) {
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
      js::Stmt::Return(ret) => match &ret.arg {
        Some(arg) => Ok(Stmt::Return(self.expr(arg)?)),
        None => Ok(Stmt::Return(Expr::Lit(Lit::Null))),
      },
      js::Stmt::Expr(expr_stmt) => self.effect(&expr_stmt.expr),
      js::Stmt::Block(block) => Err(self.residue(block.span, "a bare block")),
      other => {
        let (what, hint) = describe_stmt(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
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
  fn effect(&mut self, expr: &js::Expr) -> Lowered<Stmt> {
    match expr {
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
      js::Expr::Call(call) if self.local_collection_write(call).is_some() => {
        let (name, kind, method) = self.local_collection_write(call).expect("checked");
        let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
          let a = call.args.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes {} argument{}", i + 1, if i == 0 { "" } else { "s" })))?;
          this.expr(&a.expr)
        };
        let held = Expr::Var(name.clone());
        let expr = match (kind, method.as_str()) {
          (Collection::Map, "set") => Expr::Object(vec![Entry::Spread(held), Entry::Computed(Expr::Str(Box::new(arg(self, 0)?)), arg(self, 1)?)]),
          (Collection::Map, "delete") => Expr::Builtin { name: Builtin::Omit, args: vec![held, Expr::Str(Box::new(arg(self, 0)?))] },
          (Collection::Set, "add") => Expr::Builtin { name: Builtin::Unique, args: vec![Expr::Array(vec![Entry::Spread(held), Entry::Item(arg(self, 0)?)])] },
          (_, other) => return Err(self.residue(call.span, format!("`.{other}()` of a local `{}` as a statement", if kind == Collection::Map { "Map" } else { "Set" }))),
        };
        Ok(Stmt::Set { name, expr })
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

  /// Whether `name` is a binding the body declared, which a write may rebind.
  fn is_local(&self, name: &str) -> bool {
    matches!(self.scope.iter().rev().find(|(n, _)| n == name), Some((_, Expr::Var(var))) if var == name)
  }

  /// The local a write lands on: `x` or `x.field` with `x` declared in the body.
  fn local_target(&self, target: &js::AssignTarget) -> Option<String> {
    match target {
      js::AssignTarget::Simple(js::SimpleAssignTarget::Ident(id)) => self.is_local(id.id.sym.as_ref()).then(|| id.id.sym.to_string()),
      js::AssignTarget::Simple(js::SimpleAssignTarget::Member(m)) => match &*m.obj {
        js::Expr::Ident(id) if self.is_local(id.sym.as_ref()) => Some(id.sym.to_string()),
        _ => None,
      },
      _ => None,
    }
  }

  /// `x = e`, `x += e` and their kin on a local; `x.field = e` rebinds `x` to a copy with that field.
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
    let Some(field) = self.member_name(member) else {
      return Err(self.residue_with(member.span, "a write at a computed index of a local", "build the new value whole with `map` or a spread and assign that"));
    };
    let expr = Expr::Object(vec![Entry::Spread(Expr::Var(name.clone())), Entry::Field(field, value)]);
    Ok(Stmt::Set { name, expr })
  }

  /// `m.set(k, v)`, `m.delete(k)` or `s.add(x)` with `m` or `s` a local collection: the local, which kind it is and the method.
  fn local_collection_write(&self, call: &js::CallExpr) -> Option<(String, Collection, String)> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Member(member) = &**callee else { return None };
    let js::Expr::Ident(obj) = &*member.obj else { return None };
    let kind = self.collection_of(&member.obj)?;
    let method = self.member_name(member)?;
    (matches!(method.as_str(), "set" | "delete" | "add" | "clear") && self.is_local(obj.sym.as_ref())).then(|| (obj.sym.to_string(), kind, method))
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

  fn root_of(&self, id: &js::Ident) -> Option<Root> {
    let name = id.sym.as_ref();
    self.roots.iter().rev().find(|(n, _)| n == name).map(|(_, r)| r.clone())
  }

  pub(crate) fn expr(&mut self, expr: &js::Expr) -> Lowered<Expr> {
    match expr {
      js::Expr::Paren(p) => self.expr(&p.expr),
      js::Expr::Await(a) => self.expr(&a.arg),
      js::Expr::TsAs(a) => self.expr(&a.expr),
      js::Expr::TsConstAssertion(a) => self.expr(&a.expr),
      js::Expr::TsNonNull(a) => self.expr(&a.expr),
      js::Expr::TsSatisfies(a) => self.expr(&a.expr),
      js::Expr::TsTypeAssertion(a) => self.expr(&a.expr),

      js::Expr::Ident(id) => self.ident(id),
      js::Expr::Lit(lit) => self.lit(lit, expr.span()),
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
              other => return Err(self.residue(other.span(), "a method or accessor in an object literal")),
            },
          });
        }
        Ok(Expr::Object(entries))
      }
      js::Expr::Array(arr) => {
        let mut entries = Vec::new();
        for elem in arr.elems.iter().flatten() {
          let value = self.expr(&elem.expr)?;
          entries.push(match elem.spread {
            Some(_) if self.collection_of(&elem.expr) == Some(Collection::Map) => Entry::Spread(Expr::Entries(Box::new(value))),
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
        js::UnaryOp::Delete => Err(self.residue(u.span, "`delete` inside an expression")),
        other => Err(self.residue(u.span, format!("the `{}` operator", other))),
      },
      js::Expr::Bin(bin) => {
        let l = Box::new(self.expr(&bin.left)?);
        let r = Box::new(self.expr(&bin.right)?);
        Ok(match bin.op {
          js::BinaryOp::Add => Expr::Arith(ArithOp::Add, l, r),
          js::BinaryOp::Sub => Expr::Arith(ArithOp::Sub, l, r),
          js::BinaryOp::Mul => Expr::Arith(ArithOp::Mul, l, r),
          js::BinaryOp::Div => Expr::Arith(ArithOp::Div, l, r),
          js::BinaryOp::Mod => Expr::Arith(ArithOp::Rem, l, r),
          js::BinaryOp::Exp => Expr::Arith(ArithOp::Pow, l, r),
          js::BinaryOp::EqEqEq | js::BinaryOp::EqEq => Expr::Compare(CompareOp::Eq, l, r),
          js::BinaryOp::NotEqEq | js::BinaryOp::NotEq => Expr::Compare(CompareOp::Ne, l, r),
          js::BinaryOp::Lt => Expr::Compare(CompareOp::Lt, l, r),
          js::BinaryOp::LtEq => Expr::Compare(CompareOp::Le, l, r),
          js::BinaryOp::Gt => Expr::Compare(CompareOp::Gt, l, r),
          js::BinaryOp::GtEq => Expr::Compare(CompareOp::Ge, l, r),
          js::BinaryOp::LogicalAnd => Expr::Logic(LogicOp::And, l, r),
          js::BinaryOp::LogicalOr => Expr::Logic(LogicOp::Or, l, r),
          js::BinaryOp::NullishCoalescing => Expr::Coalesce(l, r),
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
      js::Expr::Arrow(arrow) => Err(self.residue_with(
        arrow.span,
        "a function value; lambdas go to `map`, `filter` and their kin",
        "export helpers as bare functions rather than fields of an object, so each call site names one the build can follow",
      )),
      js::Expr::OptChain(o) => match &*o.base {
        js::OptChainBase::Member(member) => self.member(member),
        js::OptChainBase::Call(call) if o.optional => Err(self.residue_with(call.span, "an optional call of a function value", "the server holds no function values; call a builtin or a helper the build can follow")),
        js::OptChainBase::Call(call) => self.optional_call(call),
      },
      js::Expr::New(n) => self.new_expr(n),
      js::Expr::Fn(f) => Err(self.residue(f.function.span, "a `function` expression")),
      js::Expr::Assign(a) => Err(self.residue(a.span, "an assignment inside an expression")),
      other => {
        let (what, hint) = describe_expr(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  pub(crate) fn ident(&mut self, id: &js::Ident) -> Lowered<Expr> {
    let name = id.sym.as_ref();
    if let Some((_, bound)) = self.scope.iter().rev().find(|(n, _)| n == name) {
      return Ok(bound.clone());
    }
    if let Some((_, bound)) = self.globals.iter().rev().find(|(n, _)| n == name) {
      return Ok(bound.clone());
    }
    match self.root_of(id) {
      Some(Root::Input) => Ok(Expr::Input),
      Some(Root::Now) => Ok(Expr::Now),
      Some(Root::Locale) => Ok(Expr::Locale),
      Some(Root::Path) => Ok(Expr::Path),
      Some(Root::Host) => Ok(Expr::Host),
      Some(Root::Origin) => Ok(Expr::Origin),
      Some(Root::Address) => Ok(Expr::Address),
      Some(Root::Params | Root::Query | Root::Session | Root::Identity | Root::Config) => Err(self.residue(id.span, format!("`{name}` as a whole; read one of its fields"))),
      Some(Root::Services) => Err(self.residue(id.span, "`services` as a value; call a method on it")),
      Some(Root::Native) => Err(self.residue(id.span, "`native` as a value; call a method on it")),
      Some(Root::Ctx) => Err(self.residue(id.span, "`ctx` as a value; read one of its fields")),
      None => match name {
        "undefined" => Ok(Expr::Lit(Lit::Null)),
        _ => {
          self.unbound = Some(name.to_owned());
          Err(self.residue(id.span, format!("`{name}` is not bound here; an import the build cannot follow, or a name from outside the body")))
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

  /// True when `name` is the client library's store `key`, however this
  /// module spelled it.
  pub(crate) fn is_store_key(&self, name: &str) -> bool {
    crate::component::imported_as(self.parsed, name, |source| source == STORE_SPECIFIER).as_deref() == Some("key")
  }

  /// A member read. Context roots become reads; anything else is a field.
  fn member(&mut self, member: &js::MemberExpr) -> Lowered<Expr> {
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
              "params" | "query" | "session" | "identity" | "config" => Err(self.residue(member.span, format!("`ctx.{name}` as a whole; read one of its fields"))),
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

    let collection = self.collection_of(&member.obj);
    let target = self.expr(&member.obj)?;
    match prop(self)? {
      Ok(name) if name == "length" => Ok(Expr::Length(Box::new(target))),
      Ok(name) if name == "size" && collection.is_some() => Ok(Expr::Length(Box::new(target))),
      Ok(name) => Ok(Expr::Field(Box::new(target), name)),
      Err(key) => Ok(Expr::Index(Box::new(target), Box::new(key))),
    }
  }

  pub(crate) fn call(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    let js::Callee::Expr(callee) = &call.callee else {
      return Err(self.residue(call.span, "`super` or `import()`"));
    };
    let math = matches!(&**callee, js::Expr::Member(m) if matches!(&*m.obj, js::Expr::Ident(id) if id.sym.as_ref() == "Math"));
    for arg in &call.args {
      if arg.spread.is_some() && !math {
        return Err(self.residue(arg.expr.span(), "a spread argument"));
      }
    }

    if let js::Expr::Ident(id) = &**callee {
      let name = id.sym.as_ref();
      if !self.scope.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() {
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
          if self.on_render_path() {
            if let Some(key) = self.body_extension_in(&f) {
              self.reach_violation = true;
              return Err(self.residue(call.span, format!("`{name}` calls `{key}` on a render path")));
            }
          }
          let mut args = Vec::with_capacity(call.args.len());
          for a in &call.args {
            args.push(self.expr(&a.expr)?);
          }
          return Ok(self.candidate(call.span, Expr::Apply { f, args }));
        }
        if let Some((source, imported)) = crate::component::find_import(self.parsed, name) {
          if source == STD_SPECIFIER {
            if imported != "t" {
              return Err(self.residue(id.span, format!("`{imported}` from the standard library is a module; call one of its members")));
            }
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              args.push(self.expr(&a.expr)?);
            }
            return Ok(self.candidate(call.span, Expr::Ext { module: "i18n".to_owned(), name: "t".to_owned(), args }));
          }
        }
        let one = |this: &mut Self| -> Lowered<Box<Expr>> {
          let a = call.args.first().ok_or_else(|| this.residue(call.span, format!("`{name}` takes one argument")))?;
          Ok(Box::new(this.expr(&a.expr)?))
        };
        return match name {
          "String" => Ok(Expr::Str(one(self)?)),
          "Number" => Ok(Expr::Num(one(self)?)),
          "BigInt" => Ok(Expr::BigInt(one(self)?)),
          "encodeURIComponent" => Ok(Expr::Builtin { name: Builtin::EncodeUriComponent, args: vec![*one(self)?] }),
          _ if self.is_store_key(name) => Ok(*one(self)?),
          "fail" => Err(self.residue(call.span, "`fail` inside an expression; it is a statement")),
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
      return Err(self.residue(callee.span(), "a call to a computed target"));
    };
    let method = self.member_name(member).ok_or_else(|| self.residue(member.span, "a computed method name"))?;

    if let js::Expr::Ident(obj) = &*member.obj {
      let global = obj.sym.as_ref();
      if !self.scope.iter().any(|(n, _)| n == global) && !self.globals.iter().any(|(n, _)| n == global) {
        if let Some((source, imported)) = crate::component::find_import(self.parsed, global) {
          if source == STD_SPECIFIER {
            let key = format!("{imported}.{method}");
            let reach = standard_reach(&imported, &method).ok_or_else(|| self.residue(member.span, format!("`{key}` is not a member of the standard library")))?;
            self.check_reach(call.span, &key, reach)?;
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              args.push(self.expr(&a.expr)?);
            }
            let expr = Expr::Ext { module: imported, name: method, args };
            return Ok(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr });
          }
        }
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
          "Array" if method == "from" && call.args.first().is_some_and(|a| !matches!(&*a.expr, js::Expr::Object(_))) => {
            let source = &call.args[0].expr;
            let mut items = self.expr(source)?;
            items = match self.collection_of(source) {
              Some(Collection::Map) => Expr::Entries(Box::new(items)),
              _ => Expr::Builtin { name: Builtin::Slice, args: vec![items] },
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

    if method == "format" {
      if let js::Expr::New(made) = strip_parens(&member.obj) {
        if let Some(formatter) = intl_formatter(made) {
          let value = call.args.first().ok_or_else(|| self.residue(call.span, "`format` without a value"))?;
          let value = self.expr(&value.expr)?;
          let made_args: Vec<js::ExprOrSpread> = made.args.clone().unwrap_or_default();
          let expr = self.intl_format(formatter, &made_args, value, made.span)?;
          return Ok(self.candidate(call.span, expr));
        }
      }
    }
    match self.collection_of(&member.obj) {
      Some(Collection::Map) => {
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
      Some(Collection::Set) => {
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
      None => {}
    }

    let target = Box::new(self.expr(&member.obj)?);
    let lambda = |this: &mut Self, i: usize| -> Lowered<Box<Expr>> {
      let a = call.args.get(i).ok_or_else(|| this.residue(call.span, format!("`{method}` takes a function")))?;
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
      "toString" if call.args.is_empty() => Ok(Expr::Str(target)),
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
      "getFullYear" | "getMonth" | "getDate" | "getDay" | "getHours" | "getMinutes" | "getSeconds" | "getTimezoneOffset" | "toLocaleTimeString" | "toTimeString" | "toDateString" => {
        let utc = method.replacen("get", "getUTC", 1);
        Err(self.residue_with(member.span, format!("`.{method}()`, which reads the viewer's time zone"), format!("the server has no viewer; `.{utc}()` and `toLocaleDateString(undefined, {{ dateStyle }})` read UTC, which the server and the browser agree on")))
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
      "slice" | "at" | "indexOf" | "concat" | "padStart" | "padEnd" | "substring" => {
        let name = match method.as_str() {
          "slice" => Builtin::Slice,
          "at" => Builtin::At,
          "indexOf" => Builtin::IndexOf,
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
        let f = lambda(self, 0)?;
        let init = call.args.get(1).ok_or_else(|| self.residue(call.span, "`reduce` needs an initial value"))?;
        let init = Box::new(self.expr(&init.expr)?);
        Ok(Expr::Reduce(target, init, f))
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
        // The second argument each of these takes in JavaScript, `limit` for
        // `split` and `position` for the other two, would otherwise lower to an
        // argument the interpreter ignores.
        if call.args.len() != arity {
          let plural = if arity == 1 { "" } else { "s" };
          return Err(self.residue(call.span, format!("`{method}` takes {arity} argument{plural}, got {}", call.args.len())));
        }
        if name == Builtin::Split {
          let empty = call.args.first().is_some_and(|a| matches!(&*a.expr, js::Expr::Lit(js::Lit::Str(s)) if s.value.to_atom_lossy().as_ref().is_empty()));
          if empty {
            return Err(self.residue_with(
              call.args[0].expr.span(),
              "`split` with an empty separator".to_owned(),
              "JavaScript splits one into UTF-16 code units, which the value model holds no half of; `Array.from(s)` and `[...s]` are residue for the same reason",
            ));
          }
        }
        let mut args = vec![*target];
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Builtin { name, args })
      }
      other => Err(self.residue_with(
        member.span,
        format!("`.{other}()`, which is not a builtin"),
        "the builtins are `map`, `filter`, `find`, `findIndex`, `some`, `every`, `reduce`, `flatMap`, `toSorted`, `toReversed`, `slice`, `at`, `indexOf`, `concat`, `join`, `includes`, `trim`, `repeat`, `toFixed`, `toUpperCase`, `toLowerCase`, `padStart`, `padEnd`, `substring`, `split`, `startsWith`, `endsWith`, `replace` and `toLocaleString`; anything else goes in a module-level helper the build can read",
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
  fn new_expr(&mut self, n: &js::NewExpr) -> Lowered<Expr> {
    let args: Vec<js::ExprOrSpread> = n.args.clone().unwrap_or_default();
    if let Some(spread) = args.iter().find(|a| a.spread.is_some()) {
      return Err(self.residue(spread.expr.span(), "a spread argument"));
    }
    let global = match &*n.callee {
      js::Expr::Ident(id) if !self.scope.iter().any(|(name, _)| name == id.sym.as_ref()) && crate::component::find_import(self.parsed, id.sym.as_ref()).is_none() => id.sym.to_string(),
      _ if intl_formatter(n).is_some() => return Err(self.residue_with(n.span, "an `Intl` formatter kept as a value", "call `.format(x)` on it where it is made, `new Intl.NumberFormat(undefined, options).format(n)`")),
      other => return Err(self.residue(other.span(), "`new` of something other than `Date`, `Map`, `Set` or `URLSearchParams`")),
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
      ("URLSearchParams", None) => Ok(Expr::lit_str("")),
      ("URLSearchParams", Some(x)) => Ok(Expr::Builtin { name: Builtin::FormEncode, args: vec![x] }),
      (other, _) => Err(self.residue(n.span, format!("`new {other}`"))),
    }
  }

  /// `Date.now()` or `new Date()`: the request's clock in a body; on a render path the server and the browser would each read their own moment.
  fn date_now(&mut self, span: Span) -> Lowered<Expr> {
    if self.on_render_path() {
      return Err(self.residue_with(span, "the current time on a render path, which the server and the browser read at different moments", "read `ctx.now` in the loader and pass it as a prop"));
    }
    Ok(Expr::Num(Box::new(Expr::Ext { module: "time".to_owned(), name: "now".to_owned(), args: Vec::new() })))
  }

  /// `new Intl.NumberFormat(locale, options).format(value)` and `new Intl.DateTimeFormat(locale, options).format(value)` as the `intl` member that formats the same way under the request's locale.
  fn intl_format(&mut self, formatter: &str, made: &[js::ExprOrSpread], value: Expr, span: Span) -> Lowered<Expr> {
    if let Some(locale) = made.first() {
      if !matches!(&*locale.expr, js::Expr::Ident(id) if id.sym.as_ref() == "undefined") {
        return Err(self.residue_with(locale.expr.span(), "a locale named where it formats", "pass `undefined`: the request's locale formats it, on the server and in the browser alike"));
      }
    }
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
    match formatter {
      "NumberFormat" => {
        if literal(&options, "style").as_deref() == Some("currency") {
          if literal(&options, "currencyDisplay").as_deref() != Some("code") {
            return Err(self.residue_with(span, "a currency formatted with a symbol", "`currencyDisplay: \"code\"` writes the ISO code, which the server and every browser agree on"));
          }
          let Some((_, code)) = options.iter().find(|(k, _)| k == "currency") else { return Err(self.residue(span, "a currency style with no `currency`")) };
          let code = self.expr(&code.clone())?;
          if let Some((other, _)) = options.iter().find(|(k, _)| !matches!(k.as_str(), "style" | "currency" | "currencyDisplay")) {
            return Err(self.residue(span, format!("the formatting option `{other}` with a currency")));
          }
          return Ok(Expr::Ext { module: "intl".to_owned(), name: "currency".to_owned(), args: vec![value, code] });
        }
        let mut entries = Vec::new();
        for (key, v) in &options {
          if !matches!(key.as_str(), "minimumFractionDigits" | "maximumFractionDigits") {
            return Err(self.residue(span, format!("the number formatting option `{key}`; `minimumFractionDigits`, `maximumFractionDigits` and a currency in code form are read")));
          }
          entries.push(Entry::Field(key.clone(), self.expr(v)?));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "number".to_owned(), args: vec![value, Expr::Object(entries)] })
      }
      _ => {
        let Some(style) = literal(&options, "dateStyle") else {
          return Err(self.residue_with(span, "a date formatted without a `dateStyle`", "`{ dateStyle: \"medium\" }` names a format the server and the browser write the same way"));
        };
        if let Some((other, _)) = options.iter().find(|(k, _)| k != "dateStyle" && !(k == "timeZone" && literal(&options, "timeZone").as_deref() == Some("UTC"))) {
          return Err(self.residue(span, format!("the date formatting option `{other}`; `dateStyle` and `timeZone: \"UTC\"` are read")));
        }
        Ok(Expr::Ext { module: "intl".to_owned(), name: "date".to_owned(), args: vec![value, Expr::lit_str(style)] })
      }
    }
  }

  /// `a?.b.c(x)`: the call as written, `null` where an optional link's object is null or undefined.
  fn optional_call(&mut self, call: &js::OptCall) -> Lowered<Expr> {
    let mut guards = Vec::new();
    let callee = strip_optional(&call.callee, &mut guards);
    let plain = js::CallExpr { span: call.span, ctxt: call.ctxt, callee: js::Callee::Expr(Box::new(callee)), args: call.args.clone(), type_args: None };
    let mut out = self.call(&plain)?;
    for guard in guards.iter().rev() {
      let object = self.expr(guard)?;
      let missing = Expr::Compare(CompareOp::Eq, Box::new(object), Box::new(Expr::Lit(Lit::Null)));
      out = Expr::Ternary(Box::new(missing), Box::new(Expr::Lit(Lit::Null)), Box::new(out));
    }
    Ok(out)
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
      js::ArrowFunctionBody::FunctionBody(b) => crate::component::block_to_expr(self, &b.stmts),
    };
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }
}

/// A regular expression literal as the value the regex builtins read, refused here when the interpreter could not match it as JavaScript does.
fn regex_value(lowerer: &Lowerer<'_>, pattern: &str, flags: &str, span: Span) -> Lowered<Expr> {
  if let Err(why) = snapfire_fsr_ir::jsregex::compiled(pattern, flags) {
    return Err(lowerer.residue_with(span, format!("a regular expression with {why}"), "write it without that construct; the server matches it with a different engine than the browser's, so only the shared part is lowered"));
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

fn strip_parens(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => strip_parens(&p.expr),
    other => other,
  }
}

/// `new Intl.NumberFormat(...)` or `new Intl.DateTimeFormat(...)`: which one.
fn intl_formatter(n: &js::NewExpr) -> Option<&'static str> {
  let js::Expr::Member(m) = &*n.callee else { return None };
  let (js::Expr::Ident(ns), js::MemberProp::Ident(name)) = (&*m.obj, &m.prop) else { return None };
  if ns.sym.as_ref() != "Intl" {
    return None;
  }
  match name.sym.as_ref() {
    "NumberFormat" => Some("NumberFormat"),
    "DateTimeFormat" => Some("DateTimeFormat"),
    _ => None,
  }
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

fn prop_name(name: &js::PropName) -> Option<String> {
  match name {
    js::PropName::Ident(id) => Some(id.sym.to_string()),
    js::PropName::Str(s) => Some(s.value.to_atom_lossy().to_string()),
    _ => None,
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
fn describe_stmt(stmt: &js::Stmt) -> (&'static str, &'static str) {
  match stmt {
    js::Stmt::Try(_) => ("`try`, and a body has no exceptions", "a service call that fails fails the body; `fail(kind, message)` is how a body stops on purpose"),
    js::Stmt::Throw(_) => ("`throw`, and a body has no exceptions", "`fail(kind, message)` stops the body and the host maps the kind onto a status"),
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
