//! Reads a TypeScript loader or actions module and lowers each body to the IR.
//! The recognised language is the one `_private_docs/design/IR.md` states; anything
//! outside it is residue, reported with the line and the construct.

pub mod component;
pub mod schema;
pub mod testing;

use snapfire_fsr_ir::ast::{ArithOp, Body, Builtin, CompareOp, Entry, Expr, Lit, LogicOp, Stmt};
use swc_core::common::{sync::Lrc, FileName, SourceMap, Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

/// Why a body is not IR. `line` and `column` are one-based in the source file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{file}:{line}:{column}: {message}")]
pub struct Residue {
  pub file: String,
  pub line: usize,
  pub column: usize,
  pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LowerError {
  #[error("{file}: {message}")]
  Parse { file: String, message: String },
  #[error("{file}: no exported `{export}`")]
  MissingExport { file: String, export: String },
  #[error(transparent)]
  Residue(#[from] Residue),
}

pub use schema::{read_schema, read_session_defaults, SchemaType};

/// The import aliases every fsr application has, each a prefix and the app
/// directory it stands for. The build writes them into both tsconfigs, snapfirec
/// rewrites them for the browser and the lowerers resolve them here.
pub const ALIASES: &[(&str, &str)] = &[("@app/", ""), ("@routes/", "routes/"), ("@src/", "src/"), ("@schemas/", "schemas/"), ("@generated/", "generated/")];

/// A specifier as a path relative to the app: an alias expanded or a relative
/// specifier joined to `from`'s directory. `None` for a bare specifier.
pub fn resolve_specifier(from: &str, specifier: &str) -> Option<String> {
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
  let function = parsed
    .exports()
    .find_map(|(name, decl)| (name == "load").then_some(decl))
    .ok_or_else(|| LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() })?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, body),
    Exported::Action { .. } => return Err(LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }),
    Exported::Expr(_, e) => return Err(parsed.residue(e.span(), "`load` must be a function with a block body").into()),
    Exported::Other(span) => return Err(parsed.residue(span, "`load` must be a function").into()),
  };
  let mut lowerer = Lowerer::new(&parsed, defaults);
  lowerer.bind_ctx(first)?;
  Ok(lowerer.block(body)?)
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

/// An export whose input is the loader's data, bound as `data`.
fn lower_of_data(file: &str, source: &str, defaults: &SessionDefaults, export: &str) -> Result<Option<Body>, LowerError> {
  let parsed = parse(file, source)?;
  let Some(exported) = parsed.exports().find_map(|(name, decl)| (name == export).then_some(decl)) else { return Ok(None) };
  let mut lowerer = Lowerer::new(&parsed, defaults);
  lowerer.meta = true;
  let body = match exported {
    Exported::Function(first, body) => {
      lowerer.bind_ctx(first)?;
      lowerer.block(body)?
    }
    Exported::Expr(first, expr) => {
      lowerer.bind_ctx(first)?;
      vec![Stmt::Return(lowerer.expr(expr)?)]
    }
    Exported::Action { .. } | Exported::Other(_) => return Err(parsed.residue(parsed.module.span, format!("`{export}` must be a function of `{{ data }}`")).into()),
  };
  Ok(Some(body))
}

/// Lowers every `export const name = action(...)` of an actions module.
pub fn lower_actions(file: &str, source: &str) -> Result<Vec<LoweredAction>, LowerError> {
  lower_actions_with(file, source, &SessionDefaults::new())
}

pub fn lower_actions_with(file: &str, source: &str, defaults: &SessionDefaults) -> Result<Vec<LoweredAction>, LowerError> {
  let parsed = parse(file, source)?;
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    let Exported::Action { input, first, body } = exported else {
      continue;
    };
    let mut lowerer = Lowerer::new(&parsed, defaults);
    lowerer.bind_ctx(first)?;
    out.push(LoweredAction { export: name.to_owned(), input, body: lowerer.block(body)? });
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
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    if !HANDLER_METHODS.contains(&name) {
      continue;
    }
    let (input, first, body) = match exported {
      Exported::Function(first, body) => (None, first, body),
      Exported::Action { input, first, body } => (input, first, body),
      Exported::Expr(_, e) => return Err(parsed.residue(e.span(), format!("`{name}` must be a function with a block body")).into()),
      Exported::Other(span) => return Err(parsed.residue(span, format!("`{name}` must be a function or an `action(...)`")).into()),
    };
    let mut lowerer = Lowerer::new(&parsed, defaults);
    lowerer.bind_ctx(first)?;
    out.push(LoweredHandler { method: name.to_owned(), input, body: lowerer.block(body)? });
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
  let function = parsed
    .exports()
    .find_map(|(name, decl)| (name == "middleware").then_some(decl))
    .ok_or_else(|| LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() })?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, body),
    Exported::Action { .. } => return Err(LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() }),
    Exported::Expr(_, e) => return Err(parsed.residue(e.span(), "`middleware` must be a function with a block body").into()),
    Exported::Other(span) => return Err(parsed.residue(span, "`middleware` must be a function").into()),
  };
  let mut lowerer = Lowerer::new(&parsed, defaults);
  lowerer.middleware = true;
  lowerer.bind_ctx(first)?;
  Ok(lowerer.block(body)?)
}

pub(crate) struct Parsed {
  file: String,
  cm: Lrc<SourceMap>,
  pub(crate) module: js::Module,
}

enum Exported<'a> {
  Function(Option<&'a js::Pat>, &'a [js::Stmt]),
  /// An arrow whose body is one expression.
  Expr(Option<&'a js::Pat>, &'a js::Expr),
  Action { input: Option<String>, first: Option<&'a js::Pat>, body: &'a [js::Stmt] },
  Other(Span),
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
  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    let loc = self.cm.lookup_char_pos(span.lo);
    Residue { file: self.file.clone(), line: loc.line, column: loc.col_display + 1, message: message.into() }
  }

  fn exports(&self) -> impl Iterator<Item = (&str, Exported<'_>)> {
    self.module.body.iter().filter_map(|item| {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else {
        return None;
      };
      match &export.decl {
        js::Decl::Fn(f) => {
          let first = f.function.params.first().map(|p| &p.pat);
          let body = f.function.body.as_ref().map(|b| b.stmts.as_slice()).unwrap_or(&[]);
          Some((f.ident.sym.as_ref(), Exported::Function(first, body)))
        }
        js::Decl::Var(var) => {
          let decl = var.decls.first()?;
          let js::Pat::Ident(name) = &decl.name else { return None };
          let init = decl.init.as_deref()?;
          Some((name.id.sym.as_ref(), classify(init)))
        }
        _ => None,
      }
    })
  }
}

fn classify(init: &js::Expr) -> Exported<'_> {
  match init {
    js::Expr::Arrow(arrow) => match &*arrow.body {
      js::ArrowFunctionBody::FunctionBody(b) => Exported::Function(arrow.params.first(), &b.stmts),
      js::ArrowFunctionBody::Expr(e) => Exported::Expr(arrow.params.first(), e),
    },
    js::Expr::Call(call) => {
      let is_action = matches!(&call.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(id) if id.sym.as_ref() == "action"));
      if !is_action {
        return Exported::Other(call.span);
      }
      let Some(last) = call.args.last() else { return Exported::Other(call.span) };
      match &*last.expr {
        js::Expr::Arrow(arrow) => match &*arrow.body {
          js::ArrowFunctionBody::FunctionBody(b) => {
            let input = call.type_args.as_ref().and_then(|t| t.params.first()).and_then(|t| type_ref_name(t)).or_else(|| arrow.params.first().and_then(action_ctx_input));
            Exported::Action { input, first: arrow.params.first(), body: &b.stmts }
          }
          js::ArrowFunctionBody::Expr(_) => Exported::Other(arrow.span),
        },
        other => Exported::Other(other.span()),
      }
    }
    other => Exported::Other(other.span()),
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
enum Root {
  Params,
  Query,
  Session,
  Services,
  Identity,
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
  pub(crate) scope: Vec<(String, Expr)>,
  /// Module-level names a component lowerer has resolved, read after the scope.
  pub(crate) globals: Vec<(String, Expr)>,
  /// The last name `ident` could not resolve, so a caller that can may bind it and retry.
  pub(crate) unbound: Option<String>,
}

pub(crate) type Lowered<T> = Result<T, Residue>;

impl<'a> Lowerer<'a> {
  pub(crate) fn new(parsed: &'a Parsed, defaults: &'a SessionDefaults) -> Self {
    Self { parsed, defaults, roots: Vec::new(), scope: Vec::new(), globals: Vec::new(), unbound: None, middleware: false, meta: false }
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
      out.push(self.stmt(stmt)?);
    }
    self.scope.truncate(depth);
    Ok(out)
  }

  pub(crate) fn stmt(&mut self, stmt: &js::Stmt) -> Lowered<Stmt> {
    match stmt {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        if var.decls.len() != 1 {
          return Err(self.residue(var.span, "one binding per declaration"));
        }
        let decl = &var.decls[0];
        let js::Pat::Ident(name) = &decl.name else {
          return Err(self.residue(decl.name.span(), "a destructuring declaration; bind the whole value and read its fields"));
        };
        let Some(init) = &decl.init else {
          return Err(self.residue(decl.span, "a declaration without a value"));
        };
        let expr = self.expr(init)?;
        let name = name.id.sym.to_string();
        self.scope.push((name.clone(), Expr::Var(name.clone())));
        Ok(Stmt::Let { name, expr })
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
        let Some(js::Pat::Ident(name)) = decl.decls.first().map(|d| &d.name) else {
          return Err(self.residue(decl.span, "`for...of` over a destructuring"));
        };
        let over = self.expr(&for_of.right)?;
        let name = name.id.sym.to_string();
        self.scope.push((name.clone(), Expr::Var(name.clone())));
        let body = self.branch(&for_of.body)?;
        self.scope.pop();
        Ok(Stmt::ForOf { name, over, body })
      }
      js::Stmt::Return(ret) => match &ret.arg {
        Some(arg) => Ok(Stmt::Return(self.expr(arg)?)),
        None => Ok(Stmt::Return(Expr::Lit(Lit::Null))),
      },
      js::Stmt::Expr(expr_stmt) => self.effect(&expr_stmt.expr),
      js::Stmt::Block(block) => Err(self.residue(block.span, "a bare block")),
      other => Err(self.residue(other.span(), describe_stmt(other))),
    }
  }

  fn branch(&mut self, stmt: &js::Stmt) -> Lowered<Body> {
    match stmt {
      js::Stmt::Block(block) => self.block(&block.stmts),
      single => Ok(vec![self.stmt(single)?]),
    }
  }

  /// `fail("kind", "message")` as a statement, bare or in a one-statement block.
  fn as_fail(&self, stmt: &js::Stmt) -> Option<(Lowered<String>, Lowered<String>)> {
    let inner = match stmt {
      js::Stmt::Block(b) if b.stmts.len() == 1 => &b.stmts[0],
      other => other,
    };
    let js::Stmt::Expr(e) = inner else { return None };
    let js::Expr::Call(call) = &*e.expr else { return None };
    let js::Callee::Expr(callee) = &call.callee else { return None };
    let js::Expr::Ident(id) = &**callee else { return None };
    if id.sym.as_ref() != "fail" {
      return None;
    }
    let arg = |i: usize| -> Lowered<String> {
      let a = call.args.get(i).ok_or_else(|| self.residue(call.span, "`fail` takes a kind and a message"))?;
      match &*a.expr {
        js::Expr::Lit(js::Lit::Str(s)) => Ok(s.value.to_atom_lossy().to_string()),
        other => Err(self.residue(other.span(), "`fail` takes string literals")),
      }
    };
    Some((arg(0), arg(1)))
  }

  /// A statement that is an expression: a session write, a delete, a bare
  /// `fail`, or an awaited call for its effect.
  fn effect(&mut self, expr: &js::Expr) -> Lowered<Stmt> {
    match expr {
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
      other => Ok(Stmt::Expr(self.expr(other)?)),
    }
  }

  fn is_ident_call(&self, call: &js::CallExpr, name: &str) -> bool {
    matches!(&call.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(id) if id.sym.as_ref() == name))
  }

  /// `session.key`, `session.key.sub`, `session.key[expr]`, or the same
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
          entries.push(if elem.spread.is_some() { Entry::Spread(self.expr(&elem.expr)?) } else { Entry::Item(self.expr(&elem.expr)?) });
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
      js::Expr::Arrow(arrow) => Err(self.residue(arrow.span, "a function value; lambdas go to `map`, `filter` and their kin")),
      js::Expr::OptChain(o) => match &*o.base {
        js::OptChainBase::Member(member) => self.member(member),
        js::OptChainBase::Call(call) => Err(self.residue(call.span, "an optional call")),
      },
      js::Expr::New(n) => Err(self.residue(n.span, "`new`")),
      js::Expr::Fn(f) => Err(self.residue(f.function.span, "a `function` expression")),
      js::Expr::Assign(a) => Err(self.residue(a.span, "an assignment inside an expression")),
      other => Err(self.residue(other.span(), describe_expr(other))),
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
      Some(Root::Params | Root::Query | Root::Session | Root::Identity) => Err(self.residue(id.span, format!("`{name}` as a whole; read one of its fields"))),
      Some(Root::Services) => Err(self.residue(id.span, "`services` as a value; call a method on it")),
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
      js::Lit::Regex(_) => return Err(self.residue(span, "a regular expression")),
      js::Lit::JSXText(_) => return Err(self.residue(span, "JSX")),
    }))
  }

  fn bigint(&self, b: &js::BigInt) -> Lowered<i128> {
    b.value
      .to_string()
      .parse::<i128>()
      .map_err(|_| self.residue(b.span, "a bigint literal outside 128 bits"))
  }

  /// True when `name` is imported from the client library's store.
  pub(crate) fn is_store_import(&self, name: &str) -> bool {
    crate::component::find_import(self.parsed, name).is_some_and(|(source, _)| source == "@snapfire/fsr-client/store")
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
          Some(Root::Ctx) => {
            let Ok(name) = prop(self)? else { return Err(self.residue(member.span, "a computed context field")) };
            return match name.as_str() {
              "input" => Ok(Expr::Input),
              "now" => Ok(Expr::Now),
              "params" | "query" | "session" | "identity" => Err(self.residue(member.span, format!("`ctx.{name}` as a whole; read one of its fields"))),
              "services" => Err(self.residue(member.span, "`ctx.services` as a value; call a method on it")),
              _ => Err(self.residue(member.span, format!("`{name}` is not a field of the context"))),
            };
          }
          Some(Root::Services) => return Err(self.residue(member.span, "a service as a value; call a method on it")),
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

    let target = self.expr(&member.obj)?;
    match prop(self)? {
      Ok(name) if name == "length" => Ok(Expr::Length(Box::new(target))),
      Ok(name) => Ok(Expr::Field(Box::new(target), name)),
      Err(key) => Ok(Expr::Index(Box::new(target), Box::new(key))),
    }
  }

  pub(crate) fn call(&mut self, call: &js::CallExpr) -> Lowered<Expr> {
    let js::Callee::Expr(callee) = &call.callee else {
      return Err(self.residue(call.span, "`super` or `import()`"));
    };
    for arg in &call.args {
      if arg.spread.is_some() {
        return Err(self.residue(arg.expr.span(), "a spread argument"));
      }
    }

    if let js::Expr::Ident(id) = &**callee {
      let name = id.sym.as_ref();
      if !self.scope.iter().any(|(n, _)| n == name) && self.root_of(id).is_none() {
        if let Some((_, f)) = self.globals.iter().rev().find(|(n, _)| n == name) {
          let f = Box::new(f.clone());
          if !matches!(*f, Expr::Lambda { .. }) {
            return Err(self.residue(id.span, format!("`{name}` is not a function")));
          }
          let mut args = Vec::with_capacity(call.args.len());
          for a in &call.args {
            args.push(self.expr(&a.expr)?);
          }
          return Ok(Expr::Apply { f, args });
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
          "key" if self.is_store_import(name) => Ok(*one(self)?),
          "fail" => Err(self.residue(call.span, "`fail` inside an expression; it is a statement")),
          _ => {
            self.unbound = Some(name.to_owned());
            Err(self.residue(id.span, format!("a call to `{name}`, which the build cannot follow")))
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
        match global {
          "Object" => {
            let a = call.args.first().ok_or_else(|| self.residue(call.span, format!("`Object.{method}` takes one argument")))?;
            let target = Box::new(self.expr(&a.expr)?);
            return match method.as_str() {
              "entries" => Ok(Expr::Entries(target)),
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
              _ => return Err(self.residue(member.span, format!("`Math.{method}`"))),
            };
            let mut args = Vec::with_capacity(call.args.len());
            for a in &call.args {
              args.push(self.expr(&a.expr)?);
            }
            return Ok(Expr::Builtin { name, args });
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

    if let Some((service, via_ctx)) = self.service_of(&member.obj) {
      let _ = via_ctx;
      let mut args = Vec::new();
      if let Some(a) = call.args.first() {
        let js::Expr::Object(obj) = &*a.expr else {
          return Err(self.residue(a.expr.span(), "service arguments must be an object literal"));
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
            js::PropOrSpread::Spread(s) => return Err(self.residue(s.expr.span(), "a spread into service arguments")),
          }
        }
      }
      if call.args.len() > 1 {
        return Err(self.residue(call.span, "a service method takes one object"));
      }
      return Ok(Expr::Call { service, method, args });
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
      "some" => Ok(Expr::Some(target, lambda(self, 0)?)),
      "every" => Ok(Expr::Every(target, lambda(self, 0)?)),
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
          return Ok(Expr::Builtin { name, args });
        }
        for a in &call.args {
          args.push(self.expr(&a.expr)?);
        }
        Ok(Expr::Builtin { name, args })
      }
      other => Err(self.residue(member.span, format!("`.{other}()`, which is not a builtin"))),
    }
  }

  /// `services.<name>` or `ctx.services.<name>`.
  fn service_of(&self, obj: &js::Expr) -> Option<(String, bool)> {
    let js::Expr::Member(m) = obj else { return None };
    let name = self.member_name(m)?;
    match &*m.obj {
      js::Expr::Ident(id) if !self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) => match self.root_of(id) {
        Some(Root::Services) => Some((name, false)),
        _ => None,
      },
      js::Expr::Member(inner) => {
        let js::Expr::Ident(id) = &*inner.obj else { return None };
        if self.scope.iter().any(|(n, _)| n == id.sym.as_ref()) {
          return None;
        }
        match (self.root_of(id), self.member_name(inner).as_deref()) {
          (Some(Root::Ctx), Some("services")) => Some((name, true)),
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// An arrow function applied by a builtin. Its body is one expression, or a
  /// block that only returns one. Destructured parameters read as fields and
  /// indexes of the positional parameter.
  pub(crate) fn lambda(&mut self, arrow: &js::ArrowExpr) -> Lowered<Expr> {
    let depth = self.scope.len();
    let mut params = Vec::new();
    for (i, pat) in arrow.params.iter().enumerate() {
      let positional = format!("${i}");
      match pat {
        js::Pat::Ident(id) => {
          let name = id.id.sym.to_string();
          self.scope.push((name.clone(), Expr::Var(name.clone())));
          params.push(name);
        }
        js::Pat::Array(arr) => {
          for (j, elem) in arr.elems.iter().enumerate() {
            let Some(js::Pat::Ident(id)) = elem else {
              if elem.is_some() {
                return Err(self.residue(pat.span(), "a nested pattern in a parameter"));
              }
              continue;
            };
            self.scope.push((id.id.sym.to_string(), Expr::Var(positional.clone()).index(Expr::Lit(Lit::Float(j as f64)))));
          }
          params.push(positional);
        }
        js::Pat::Object(obj) => {
          for prop in &obj.props {
            let js::ObjectPatProp::Assign(a) = prop else {
              return Err(self.residue(prop.span(), "a renamed or nested field in a parameter"));
            };
            let name = a.key.id.sym.to_string();
            self.scope.push((name.clone(), Expr::Var(positional.clone()).field(name)));
          }
          params.push(positional);
        }
        other => return Err(self.residue(other.span(), "a parameter pattern")),
      }
    }
    let body = match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => self.expr(e),
      js::ArrowFunctionBody::FunctionBody(b) => match b.stmts.as_slice() {
        [js::Stmt::Return(r)] => match &r.arg {
          Some(arg) => self.expr(arg),
          None => Ok(Expr::Lit(Lit::Null)),
        },
        _ => Err(self.residue(b.span, "a function body with statements; a lambda is one expression")),
      },
    };
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }
}

fn root_named(name: &str) -> Option<Root> {
  Some(match name {
    "params" => Root::Params,
    "query" => Root::Query,
    "session" => Root::Session,
    "services" => Root::Services,
    "identity" => Root::Identity,
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

fn describe_stmt(stmt: &js::Stmt) -> &'static str {
  match stmt {
    js::Stmt::Try(_) => "`try`",
    js::Stmt::Throw(_) => "`throw`; use `fail(kind, message)`",
    js::Stmt::While(_) | js::Stmt::DoWhile(_) => "`while`",
    js::Stmt::For(_) | js::Stmt::ForIn(_) => "a `for` loop other than `for...of`",
    js::Stmt::Switch(_) => "`switch`",
    js::Stmt::Decl(js::Decl::Fn(_)) => "a nested function",
    js::Stmt::Decl(js::Decl::Class(_)) => "a class",
    js::Stmt::Decl(_) => "a declaration the build does not read",
    js::Stmt::Break(_) | js::Stmt::Continue(_) => "`break` or `continue`",
    js::Stmt::Labeled(_) => "a label",
    js::Stmt::With(_) => "`with`",
    js::Stmt::Debugger(_) => "`debugger`",
    js::Stmt::Empty(_) => "an empty statement",
    _ => "a statement outside the IR",
  }
}

fn describe_expr(expr: &js::Expr) -> &'static str {
  match expr {
    js::Expr::This(_) => "`this`",
    js::Expr::Class(_) => "a class",
    js::Expr::Seq(_) => "a comma expression",
    js::Expr::Update(_) => "`++` or `--`",
    js::Expr::Yield(_) => "`yield`",
    js::Expr::TaggedTpl(_) => "a tagged template",
    js::Expr::JSXElement(_) | js::Expr::JSXFragment(_) => "JSX",
    js::Expr::MetaProp(_) => "`import.meta`",
    _ => "an expression outside the IR",
  }
}
