//! Reads a `.tsx` module and lowers a component to a render tree: the JSX
//! becomes `Tmpl`, the expressions inside it become the IR and a component it
//! renders is lowered in turn under its own module id. Local imports are
//! followed on first use, so a helper module that also imports a browser
//! library costs nothing until a render reads from it. Event handlers, inner
//! functions, effects and `useState` setters are dropped, since the browser
//! mounts the same module over the output; `useState(x)` reads as `x`, which
//! is what a first render sees, `useMemo(f)` as `f()` and `useRef(x)` as
//! `{ current: x }`. A caller's children become a `Slot` where the callee
//! writes `{children}`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use snapfire_fsr_ir::ast::{Builtin, Component, Entry, Expr, Lit, Stmt, Tmpl};
use snapfire_fsr_ir::render::html_attr_name;
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;

use crate::{Lowered, LowerError, Lowerer, Parsed, Residue, SessionDefaults, parse_with, prop_name};

/// The cursor over one application: parsed files, finished components and the
/// resolution stack that turns recursion into a diagnostic.
pub struct ComponentSet {
  app: PathBuf,
  parsed: HashMap<String, Rc<Parsed>>,
  defaults: SessionDefaults,
  pub components: Vec<(String, Component)>,
  resolving: Vec<String>,
  /// Modules that are layouts: their `children` is the child segment, placed
  /// inside `<sf-s>` so the browser can adopt it without reconciling it.
  pub layouts: Vec<String>,
}

impl ComponentSet {
  pub fn new(app: &Path) -> Self {
    Self { app: app.to_path_buf(), parsed: HashMap::new(), defaults: SessionDefaults::new(), components: Vec::new(), resolving: Vec::new(), layouts: Vec::new() }
  }

  /// Lowers `module` (a `path#export` under the app) and everything it
  /// renders. A module already lowered is not read again.
  pub fn lower(&mut self, module: &str) -> Result<(), LowerError> {
    let (file, export) = module.split_once('#').ok_or_else(|| LowerError::MissingExport { file: module.to_owned(), export: String::new() })?;
    if self.components.iter().any(|(m, _)| m == module) {
      return Ok(());
    }
    if self.resolving.iter().any(|m| m == module) {
      return Err(LowerError::Parse { file: file.to_owned(), message: format!("`{export}` renders itself, which the build cannot unroll") });
    }
    self.load(file)?;
    self.resolving.push(module.to_owned());
    let result = self.lower_loaded(file, export);
    self.resolving.pop();
    let component = result?;
    self.components.push((module.to_owned(), component));
    Ok(())
  }

  fn load(&mut self, file: &str) -> Result<(), LowerError> {
    if self.parsed.contains_key(file) {
      return Ok(());
    }
    let path = self.app.join(file);
    let source = std::fs::read_to_string(&path).map_err(|e| LowerError::Parse { file: file.to_owned(), message: e.to_string() })?;
    let parsed = parse_with(file, &source, file.ends_with(".tsx"))?;
    self.parsed.insert(file.to_owned(), Rc::new(parsed));
    Ok(())
  }

  /// The file an import names relative to the app, through an alias or a
  /// relative path; `None` for a bare or generated specifier the build does
  /// not follow.
  fn resolve_import(&self, from: &str, source: &str) -> Option<String> {
    let joined = crate::resolve_specifier(from, source)?;
    for candidate in [joined.clone(), format!("{joined}.tsx"), format!("{joined}.ts"), format!("{joined}/index.tsx"), format!("{joined}/index.ts")] {
      if candidate.starts_with("generated/") {
        return None;
      }
      if self.app.join(&candidate).is_file() {
        return Some(candidate);
      }
    }
    None
  }

  fn lower_loaded(&mut self, file: &str, export: &str) -> Result<Component, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    let (component, refs) = loop {
      let (result, unbound) = {
        let parsed = self.parsed[file].clone();
        let function = find_function(&parsed, export).ok_or_else(|| LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() })?;
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.globals = globals.clone();
        let layout_root = self.layouts.iter().any(|m| *m == format!("{file}#{export}"));
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, children_name: None, layout_root };
        let result = cl.component(&function);
        (result, cl.lowerer.unbound.take())
      };
      match result {
        Ok(done) => break done,
        Err(residue) => {
          let Some(name) = unbound else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some(expr) = self.global(file, &name)? else { return Err(residue.into()) };
          globals.push((name, expr));
        }
      }
    };
    let mut modules: HashMap<String, String> = HashMap::new();
    for (name, (line, column)) in refs {
      let module = self.component_module(file, &name).map_err(|message| Residue { file: file.to_owned(), line, column, message })?;
      self.lower(&module)?;
      modules.insert(format!("{file}#{name}"), module);
    }
    Ok(Component { body: component.body, render: rewrite_modules(component.render, &modules) })
  }

  /// The module id a capitalised JSX tag names: a function in this file or a
  /// local import, lowered as its own component; `Ns.Name` is `Name` from the
  /// file a namespace import binds as `Ns`.
  fn component_module(&mut self, file: &str, name: &str) -> Result<String, String> {
    let parsed = self.parsed[file].clone();
    if let Some((namespace, member)) = name.split_once('.') {
      let source = find_namespace_import(&parsed, namespace).ok_or_else(|| format!("`{namespace}` is not a namespace import; `<{name}>` needs `import * as {namespace}`"))?;
      let target = self.resolve_import(file, &source).ok_or_else(|| format!("`{namespace}` comes from `{source}`, which the build cannot follow"))?;
      self.load(&target).map_err(|e| e.to_string())?;
      return Ok(format!("{target}#{member}"));
    }
    if find_function(&parsed, name).is_some() {
      return Ok(format!("{file}#{name}"));
    }
    if let Some((source, imported)) = find_import(&parsed, name) {
      let target = self.resolve_import(file, &source).ok_or_else(|| format!("`{name}` comes from `{source}`, which the build cannot follow"))?;
      self.load(&target).map_err(|e| e.to_string())?;
      return Ok(format!("{target}#{imported}"));
    }
    Err(format!("`{name}` is not a component this file declares or imports"))
  }

  /// A module-level name as an expression: a `const` inlined, a function as a
  /// lambda, an import resolved in its own file. `None` when the file has no
  /// such name.
  fn global(&mut self, file: &str, name: &str) -> Result<Option<Expr>, LowerError> {
    let key = format!("{file}#{name}");
    if self.resolving.iter().any(|m| *m == key) {
      return Err(LowerError::Parse { file: file.to_owned(), message: format!("`{name}` is recursive, which the build cannot unroll") });
    }
    let parsed = self.parsed[file].clone();
    if let Some((source, imported)) = find_import(&parsed, name) {
      let Some(target) = self.resolve_import(file, &source) else { return Ok(None) };
      self.load(&target)?;
      return self.global(&target, &imported);
    }
    let Some(item) = find_value(&parsed, name) else { return Ok(None) };
    self.resolving.push(key);
    let result = self.lower_global(file, item);
    self.resolving.pop();
    result.map(Some)
  }

  fn lower_global(&mut self, file: &str, item: Global<'_>) -> Result<Expr, LowerError> {
    let mut globals: Vec<(String, Expr)> = Vec::new();
    loop {
      let parsed = self.parsed[file].clone();
      let defaults = self.defaults.clone();
      let mut lowerer = Lowerer::new(&parsed, &defaults);
      lowerer.globals = globals.clone();
      let result = match &item {
        Global::Const(init) => lowerer.expr(init),
        Global::Function(params, body) => function_to_lambda(&mut lowerer, params, *body),
      };
      match result {
        Ok(expr) => return Ok(expr),
        Err(residue) => {
          let Some(name) = lowerer.unbound.take() else { return Err(residue.into()) };
          if globals.iter().any(|(n, _)| *n == name) {
            return Err(residue.into());
          }
          let Some(expr) = self.global(file, &name)? else { return Err(residue.into()) };
          globals.push((name, expr));
        }
      }
    }
  }
}

/// Points every component reference at the module the set lowered it under.
fn rewrite_modules(tmpl: Tmpl, modules: &HashMap<String, String>) -> Tmpl {
  match tmpl {
    Tmpl::Component { module, props, children } => Tmpl::Component { module: modules.get(&module).cloned().unwrap_or(module), props, children: children.into_iter().map(|c| rewrite_modules(c, modules)).collect() },
    Tmpl::Element { tag, attrs, children } => Tmpl::Element { tag, attrs, children: children.into_iter().map(|c| rewrite_modules(c, modules)).collect() },
    Tmpl::Fragment(children) => Tmpl::Fragment(children.into_iter().map(|c| rewrite_modules(c, modules)).collect()),
    Tmpl::If { cond, then, r#else } => Tmpl::If { cond, then: Box::new(rewrite_modules(*then, modules)), r#else: r#else.map(|e| Box::new(rewrite_modules(*e, modules))) },
    Tmpl::For { over, params, body } => Tmpl::For { over, params, body: Box::new(rewrite_modules(*body, modules)) },
    Tmpl::Let { name, expr, then } => Tmpl::Let { name, expr, then: Box::new(rewrite_modules(*then, modules)) },
    other => other,
  }
}

#[derive(Clone, Copy)]
enum FunctionBody<'a> {
  Block(&'a [js::Stmt]),
  Expr(&'a js::Expr),
}

#[derive(Clone)]
enum Global<'a> {
  Const(&'a js::Expr),
  Function(Vec<js::Pat>, FunctionBody<'a>),
}

fn patterns(function: &js::Function) -> Vec<js::Pat> {
  function.params.iter().map(|p| p.pat.clone()).collect()
}

fn arrow_body(arrow: &js::ArrowExpr) -> FunctionBody<'_> {
  match &*arrow.body {
    js::ArrowFunctionBody::FunctionBody(b) => FunctionBody::Block(&b.stmts),
    js::ArrowFunctionBody::Expr(e) => FunctionBody::Expr(e),
  }
}

/// The exported or declared function named `export`; `default` is the
/// default export.
fn find_function<'a>(parsed: &'a Parsed, export: &str) -> Option<Found<'a>> {
  for item in &parsed.module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) if export == "default" => {
        if let js::DefaultDecl::Fn(f) = &d.decl {
          let body = f.function.body.as_ref()?;
          return Some(Found::Declared(patterns(&f.function), &body.stmts));
        }
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) if export == "default" => {
        if let js::Expr::Arrow(arrow) = &*e.expr {
          return Some(Found::Arrow(arrow));
        }
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => {
        if let Some(found) = decl_function(&export_decl.decl, export) {
          return Some(found);
        }
      }
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => {
        if let Some(found) = decl_function(decl, export) {
          return Some(found);
        }
      }
      _ => {}
    }
  }
  None
}

enum Found<'a> {
  Declared(Vec<js::Pat>, &'a [js::Stmt]),
  Arrow(&'a js::ArrowExpr),
}

fn decl_function<'a>(decl: &'a js::Decl, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
      let body = f.function.body.as_ref()?;
      Some(Found::Declared(patterns(&f.function), &body.stmts))
    }
    js::Decl::Var(var) => {
      let decl = var.decls.iter().find(|d| matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name))?;
      match decl.init.as_deref()? {
        js::Expr::Arrow(arrow) => Some(Found::Arrow(arrow)),
        _ => None,
      }
    }
    _ => None,
  }
}

/// A module-level value: a `const` with an initialiser or a function.
fn find_value<'a>(parsed: &'a Parsed, name: &str) -> Option<Global<'a>> {
  for item in &parsed.module.body {
    let decl = match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export_decl)) => &export_decl.decl,
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      _ => continue,
    };
    match decl {
      js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
        let body = f.function.body.as_ref()?;
        return Some(Global::Function(patterns(&f.function), FunctionBody::Block(&body.stmts)));
      }
      js::Decl::Var(var) => {
        for d in &var.decls {
          if matches!(&d.name, js::Pat::Ident(id) if id.id.sym.as_ref() == name) {
            return match d.init.as_deref()? {
              js::Expr::Arrow(arrow) => Some(Global::Function(arrow.params.clone(), arrow_body(arrow))),
              init => Some(Global::Const(init)),
            };
          }
        }
      }
      _ => {}
    }
  }
  None
}

/// The source of `import * as local`.
fn find_namespace_import(parsed: &Parsed, local: &str) -> Option<String> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.type_only {
      continue;
    }
    for spec in &import.specifiers {
      if let js::ImportSpecifier::Namespace(ns) = spec {
        if ns.local.sym.as_ref() == local {
          return Some(import.src.value.to_atom_lossy().to_string());
        }
      }
    }
  }
  None
}

/// `(source, imported name)` for a value import binding `local`.
fn find_import(parsed: &Parsed, local: &str) -> Option<(String, String)> {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.type_only {
      continue;
    }
    for spec in &import.specifiers {
      match spec {
        js::ImportSpecifier::Named(named) if !named.is_type_only && named.local.sym.as_ref() == local => {
          let imported = match &named.imported {
            Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
            None => local.to_owned(),
          };
          return Some((import.src.value.to_atom_lossy().to_string(), imported));
        }
        js::ImportSpecifier::Default(d) if d.local.sym.as_ref() == local => {
          return Some((import.src.value.to_atom_lossy().to_string(), "default".to_owned()));
        }
        _ => {}
      }
    }
  }
  None
}

/// A module-level function as a lambda: parameters bind as in `Lowerer::lambda`;
/// `const`s inline; `if (c) return a;` chains become ternaries.
fn function_to_lambda(lowerer: &mut Lowerer<'_>, params: &[js::Pat], body: FunctionBody<'_>) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let names = bind_params(lowerer, params)?;
  let result = match body {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(stmts) => block_to_expr(lowerer, stmts),
  };
  lowerer.scope.truncate(depth);
  Ok(Expr::Lambda { params: names, body: Box::new(result?) })
}

fn bind_params(lowerer: &mut Lowerer<'_>, params: &[js::Pat]) -> Lowered<Vec<String>> {
  let mut names = Vec::new();
  for (i, pat) in params.iter().enumerate() {
    let positional = format!("${i}");
    match pat {
      js::Pat::Ident(id) => {
        let name = id.id.sym.to_string();
        lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        names.push(name);
      }
      js::Pat::Assign(assign) => {
        let js::Pat::Ident(id) = &*assign.left else {
          return Err(lowerer.residue(assign.span, "a default on a destructured parameter"));
        };
        let name = id.id.sym.to_string();
        let default = lowerer.expr(&assign.right)?;
        lowerer.scope.push((name.clone(), Expr::Coalesce(Box::new(Expr::Var(name.clone())), Box::new(default))));
        names.push(name);
      }
      js::Pat::Object(obj) => {
        bind_object(lowerer, obj, Expr::Var(positional.clone()))?;
        names.push(positional);
      }
      js::Pat::Array(arr) => {
        for (j, elem) in arr.elems.iter().enumerate() {
          let Some(js::Pat::Ident(id)) = elem else {
            if elem.is_some() {
              return Err(lowerer.residue(pat.span(), "a nested pattern in a parameter"));
            }
            continue;
          };
          lowerer.scope.push((id.id.sym.to_string(), Expr::Var(positional.clone()).index(Expr::Lit(Lit::Int(j as i128)))));
        }
        names.push(positional);
      }
      other => return Err(lowerer.residue(other.span(), "a parameter pattern")),
    }
  }
  Ok(names)
}

/// `{ a, b = x, c: d }` over `target`: each name reads a field, a default
/// applies when the field is null or absent.
fn bind_object(lowerer: &mut Lowerer<'_>, obj: &js::ObjectPat, target: Expr) -> Lowered<()> {
  let mut taken: Vec<String> = Vec::new();
  for prop in &obj.props {
    match prop {
      js::ObjectPatProp::Assign(a) => {
        let name = a.key.id.sym.to_string();
        taken.push(name.clone());
        let read = target.clone().field(name.clone());
        let bound = match &a.value {
          Some(default) => Expr::Coalesce(Box::new(read), Box::new(lowerer.expr(default)?)),
          None => read,
        };
        lowerer.scope.push((name, bound));
      }
      js::ObjectPatProp::KeyValue(kv) => {
        let key = prop_name(&kv.key).ok_or_else(|| lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
        taken.push(key.clone());
        let read = target.clone().field(key);
        match &*kv.value {
          js::Pat::Ident(local) => lowerer.scope.push((local.id.sym.to_string(), read)),
          js::Pat::Assign(assign) => {
            let js::Pat::Ident(local) = &*assign.left else {
              return Err(lowerer.residue(assign.span, "a nested pattern in a parameter"));
            };
            let default = lowerer.expr(&assign.right)?;
            lowerer.scope.push((local.id.sym.to_string(), Expr::Coalesce(Box::new(read), Box::new(default))));
          }
          other => return Err(lowerer.residue(other.span(), "a nested pattern in a parameter")),
        }
      }
      js::ObjectPatProp::Rest(r) => {
        let js::Pat::Ident(id) = &*r.arg else {
          return Err(lowerer.residue(r.span, "a pattern in a rest"));
        };
        let mut args = vec![target.clone()];
        args.extend(taken.iter().map(|key| Expr::lit_str(key.clone())));
        lowerer.scope.push((id.id.sym.to_string(), Expr::Builtin { name: Builtin::Omit, args }));
      }
    }
  }
  Ok(())
}

/// A function body as one expression: `const`s inline into what follows and
/// `if (c) return a;` followed by more becomes `c ? a : rest`.
fn block_to_expr(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let result = block_to_expr_inner(lowerer, stmts);
  lowerer.scope.truncate(depth);
  result
}

fn block_to_expr_inner(lowerer: &mut Lowerer<'_>, stmts: &[js::Stmt]) -> Lowered<Expr> {
  let Some((first, rest)) = stmts.split_first() else {
    return Ok(Expr::Lit(Lit::Null));
  };
  match first {
    js::Stmt::Decl(js::Decl::Var(var)) => {
      if var.decls.len() != 1 {
        return Err(lowerer.residue(var.span, "one binding per declaration"));
      }
      let decl = &var.decls[0];
      let init = decl.init.as_deref().ok_or_else(|| lowerer.residue(decl.span, "a declaration without a value"))?;
      let expr = lowerer.expr(init)?;
      match &decl.name {
        js::Pat::Ident(name) => lowerer.scope.push((name.id.sym.to_string(), expr)),
        js::Pat::Object(obj) => bind_object(lowerer, obj, expr)?,
        other => return Err(lowerer.residue(other.span(), "a destructuring the build does not read")),
      }
      block_to_expr_inner(lowerer, rest)
    }
    js::Stmt::Return(ret) => match &ret.arg {
      Some(arg) => lowerer.expr(arg),
      None => Ok(Expr::Lit(Lit::Null)),
    },
    js::Stmt::If(if_stmt) => {
      let cond = lowerer.expr(&if_stmt.test)?;
      let then = branch_to_expr(lowerer, &if_stmt.cons)?;
      let otherwise = match &if_stmt.alt {
        Some(alt) => branch_to_expr(lowerer, alt)?,
        None => block_to_expr_inner(lowerer, rest)?,
      };
      Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }
    other => Err(lowerer.residue(other.span(), "a statement a helper cannot hold; a helper is `const`s, `if ... return` and a `return`")),
  }
}

fn branch_to_expr(lowerer: &mut Lowerer<'_>, stmt: &js::Stmt) -> Lowered<Expr> {
  match stmt {
    js::Stmt::Block(block) => block_to_expr(lowerer, &block.stmts),
    single => block_to_expr(lowerer, std::slice::from_ref(single)),
  }
}

struct ComponentLowerer<'a, 'p> {
  lowerer: Lowerer<'p>,
  file: &'a str,
  /// Names whose calls the browser owns: inner functions and state setters.
  handlers: Vec<String>,
  /// Capitalised tags met so far, with where, resolved by the set after.
  refs: Vec<(String, (usize, usize))>,
  /// The `value` of the enclosing `<select>`, which its options compare against.
  select_value: Option<Expr>,
  /// The props parameter when bound whole, so `props.children` is the slot.
  props_name: Option<String>,
  /// The local name `children` was destructured to.
  children_name: Option<String>,
  /// A layout: its slot is wrapped in `<sf-s>`.
  layout_root: bool,
}

impl ComponentLowerer<'_, '_> {
  fn slot(&self) -> Tmpl {
    if self.layout_root {
      Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot] }
    } else {
      Tmpl::Slot
    }
  }
}

const EFFECT_HOOKS: &[&str] = &["useEffect", "useLayoutEffect", "useInsertionEffect", "useDebugValue", "useImperativeHandle"];

/// The hook a call names when its callee is a bare identifier.
fn hook_call(expr: &js::Expr) -> Option<(&str, &js::CallExpr)> {
  let js::Expr::Call(call) = expr else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  let js::Expr::Ident(id) = &**callee else { return None };
  let name = id.sym.as_ref();
  name.starts_with("use").then_some((name, call))
}

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  fn component(&mut self, found: &Found<'p>) -> Lowered<(Component, Vec<(String, (usize, usize))>)> {
    let depth = self.lowerer.scope.len();
    let (params, body): (Vec<js::Pat>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts) => (params.clone(), FunctionBody::Block(stmts)),
      Found::Arrow(arrow) => (arrow.params.clone(), arrow_body(arrow)),
    };
    self.bind_props(&params)?;
    let mut lets = Vec::new();
    let render = match body {
      FunctionBody::Expr(e) => self.child_expr(e)?,
      FunctionBody::Block(stmts) => {
        let mut render = None;
        for stmt in stmts {
          match stmt {
            js::Stmt::Return(ret) => {
              let arg = ret.arg.as_deref().ok_or_else(|| self.lowerer.residue(ret.span, "a component must return its tree"))?;
              render = Some(self.child_expr(arg)?);
              break;
            }
            js::Stmt::Decl(js::Decl::Fn(f)) => self.handlers.push(f.ident.sym.to_string()),
            js::Stmt::Expr(e) if hook_call(&e.expr).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {}
            js::Stmt::Decl(js::Decl::Var(var)) => {
              for decl in &var.decls {
                if let Some(stmt) = self.let_stmt(decl)? {
                  lets.push(stmt);
                }
              }
            }
            other => return Err(self.lowerer.residue(other.span(), "a statement a component cannot hold before its `return`")),
          }
        }
        render.ok_or_else(|| self.lowerer.residue(Span::default(), "a component must return its tree"))?
      }
    };
    self.lowerer.scope.truncate(depth);
    Ok((Component { body: lets, render }, std::mem::take(&mut self.refs)))
  }

  fn bind_props(&mut self, params: &[js::Pat]) -> Lowered<()> {
    let Some(first) = params.first() else { return Ok(()) };
    match first {
      js::Pat::Ident(id) => {
        self.props_name = Some(id.id.sym.to_string());
        self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var("$props".to_owned())));
        Ok(())
      }
      js::Pat::Object(obj) => {
        self.children_name = obj.props.iter().find_map(|prop| match prop {
          js::ObjectPatProp::Assign(a) if a.key.id.sym.as_ref() == "children" => Some("children".to_owned()),
          js::ObjectPatProp::KeyValue(kv) if prop_name(&kv.key).as_deref() == Some("children") => match &*kv.value {
            js::Pat::Ident(local) => Some(local.id.sym.to_string()),
            _ => None,
          },
          _ => None,
        });
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))
      }
      other => Err(self.lowerer.residue(other.span(), "the props parameter must be a name or a destructuring")),
    }
  }

  /// `const x = e` as a `let`; `const [x, setX] = useState(e)` as `let x = e`
  /// with `setX` a handler; `const { a, b } = e` as one `let` plus field reads.
  /// A function value, `useCallback` included, is a handler.
  fn let_stmt(&mut self, decl: &js::VarDeclarator) -> Lowered<Option<Stmt>> {
    let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
    match &decl.name {
      js::Pat::Ident(name) => {
        let local = name.id.sym.to_string();
        let expr = match hook_call(init) {
          Some(("useCallback", _)) => {
            self.handlers.push(local);
            return Ok(None);
          }
          Some(("useMemo", call)) => {
            let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) else {
              return Err(self.lowerer.residue(decl.span, "`useMemo` of something other than an arrow"));
            };
            match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
            }
          }
          Some(("useRef", call)) => {
            let current = match call.args.first() {
              Some(a) => self.lowerer.expr(&a.expr)?,
              None => Expr::Lit(Lit::Null),
            };
            Expr::Object(vec![Entry::Field("current".to_owned(), current)])
          }
          Some((hook, _)) if hook != "useState" => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
          _ if matches!(init, js::Expr::Arrow(_) | js::Expr::Fn(_)) => {
            self.handlers.push(local);
            return Ok(None);
          }
          _ => self.lowerer.expr(init)?,
        };
        self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
        Ok(Some(Stmt::Let { name: local, expr }))
      }
      js::Pat::Array(arr) => {
        let js::Expr::Call(call) = init else {
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState`"));
        };
        let is_use_state = matches!(&call.callee, js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(id) if id.sym.as_ref() == "useState"));
        if !is_use_state {
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState`"));
        }
        let expr = match call.args.first() {
          Some(a) => match &*a.expr {
            js::Expr::Arrow(arrow) => match &*arrow.body {
              js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e)?,
              js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts)?,
            },
            e => self.lowerer.expr(e)?,
          },
          None => Expr::Lit(Lit::Null),
        };
        let mut names = arr.elems.iter().map(|e| match e {
          Some(js::Pat::Ident(id)) => Some(id.id.sym.to_string()),
          _ => None,
        });
        let state = names.next().flatten();
        if let Some(Some(setter)) = names.next() {
          self.handlers.push(setter);
        }
        let Some(name) = state else { return Ok(None) };
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        Ok(Some(Stmt::Let { name, expr }))
      }
      js::Pat::Object(obj) => {
        let expr = self.lowerer.expr(init)?;
        let name = format!("$let{}", self.lowerer.scope.len());
        bind_object(&mut self.lowerer, obj, Expr::Var(name.clone()))?;
        Ok(Some(Stmt::Let { name, expr }))
      }
      other => Err(self.lowerer.residue(other.span(), "a declaration pattern the build does not read")),
    }
  }

  /// A JSX child or a component's return: a tree when the expression holds
  /// JSX anywhere a render would reach, text otherwise.
  fn child_expr(&mut self, expr: &'p js::Expr) -> Lowered<Tmpl> {
    match expr {
      js::Expr::Paren(p) => self.child_expr(&p.expr),
      js::Expr::JSXElement(el) => self.element(el),
      js::Expr::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expr::Lit(js::Lit::Str(s)) => Ok(Tmpl::Text(s.value.to_atom_lossy().to_string())),
      js::Expr::Cond(c) if holds_jsx(&c.cons) || holds_jsx(&c.alt) => {
        let cond = self.lowerer.expr(&c.test)?;
        let then = Box::new(self.child_expr(&c.cons)?);
        let r#else = Some(Box::new(self.child_expr(&c.alt)?));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::LogicalAnd && holds_jsx(&bin.right) => {
        let cond = self.lowerer.expr(&bin.left)?;
        let then = Box::new(self.child_expr(&bin.right)?);
        Ok(Tmpl::If { cond, then, r#else: None })
      }
      js::Expr::Call(call) if map_with_jsx(call) => {
        let js::Callee::Expr(callee) = &call.callee else { unreachable!() };
        let js::Expr::Member(member) = &**callee else { unreachable!() };
        let over = self.lowerer.expr(&member.obj)?;
        let js::Expr::Arrow(arrow) = &*call.args[0].expr else { unreachable!() };
        let depth = self.lowerer.scope.len();
        let params = bind_params(&mut self.lowerer, &arrow.params)?;
        let body = match &*arrow.body {
          js::ArrowFunctionBody::Expr(e) => self.child_expr(e),
          js::ArrowFunctionBody::FunctionBody(b) => self.block_tree(&b.stmts),
        };
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::For { over, params, body: Box::new(body?) })
      }
      js::Expr::Ident(id) if id.sym.as_ref() == "null" || id.sym.as_ref() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expr::Ident(id) if self.children_name.as_deref() == Some(id.sym.as_ref()) => Ok(self.slot()),
      js::Expr::Member(m) if self.is_props_children(m) => Ok(self.slot()),
      js::Expr::Lit(js::Lit::Null(_)) => Ok(Tmpl::Fragment(Vec::new())),
      other => Ok(Tmpl::Expr(self.lowerer.expr(other)?)),
    }
  }

  fn is_props_children(&self, member: &js::MemberExpr) -> bool {
    let js::Expr::Ident(obj) = &*member.obj else { return false };
    let js::MemberProp::Ident(prop) = &member.prop else { return false };
    self.props_name.as_deref() == Some(obj.sym.as_ref()) && prop.sym.as_ref() == "children"
  }

  /// A `.map` callback with statements: `const`s then a `return` of a tree.
  fn block_tree(&mut self, stmts: &'p [js::Stmt]) -> Lowered<Tmpl> {
    let Some((first, rest)) = stmts.split_first() else {
      return Ok(Tmpl::Fragment(Vec::new()));
    };
    match first {
      js::Stmt::Decl(js::Decl::Var(var)) => {
        if var.decls.len() != 1 {
          return Err(self.lowerer.residue(var.span, "one binding per declaration"));
        }
        let decl = &var.decls[0];
        let js::Pat::Ident(name) = &decl.name else {
          return Err(self.lowerer.residue(decl.name.span(), "a destructuring declaration; bind the whole value and read its fields"));
        };
        let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
        let expr = self.lowerer.expr(init)?;
        let name = name.id.sym.to_string();
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        let then = self.block_tree(rest)?;
        self.lowerer.scope.pop();
        Ok(Tmpl::Let { name, expr, then: Box::new(then) })
      }
      js::Stmt::Return(ret) => match &ret.arg {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      },
      other => Err(self.lowerer.residue(other.span(), "a statement in a `.map` callback other than `const` and `return`")),
    }
  }

  fn element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let (name, member) = match &el.opening.name {
      js::JSXElementName::Ident(id) => (id.sym.to_string(), false),
      js::JSXElementName::JSXMemberExpr(m) => match &m.obj {
        js::JSXObject::Ident(obj) => (format!("{}.{}", obj.sym, m.prop.sym), true),
        js::JSXObject::JSXMemberExpr(_) => return Err(self.lowerer.residue(m.span, "a member expression as a tag more than one level deep")),
      },
      js::JSXElementName::JSXNamespacedName(n) => return Err(self.lowerer.residue(n.span, "a namespaced tag")),
    };
    let is_component = member || name.chars().next().is_some_and(|c| c.is_ascii_uppercase());
    if is_component {
      return self.component_ref(&name, el);
    }
    let mut attrs = Vec::new();
    let mut select_value = None;
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" || is_handler_name(&raw) {
        continue;
      }
      let value = self.attr_value(attr)?;
      match raw.as_str() {
        "dangerouslySetInnerHTML" => return Err(self.lowerer.residue(attr.span, "`dangerouslySetInnerHTML`")),
        "style" => attrs.push(Entry::Field("style".to_owned(), self.style(attr)?)),
        "value" | "defaultValue" if name == "select" => select_value = Some(value),
        "value" if name == "option" => {
          attrs.push(Entry::Field("value".to_owned(), value.clone()));
          if let Some(selected) = &self.select_value {
            attrs.push(Entry::Field("selected".to_owned(), Expr::Compare(snapfire_fsr_ir::CompareOp::Eq, Box::new(Expr::Str(Box::new(value))), Box::new(Expr::Str(Box::new(selected.clone()))))));
          }
        }
        _ => attrs.push(Entry::Field(html_attr_name(&raw).to_owned(), value)),
      }
    }
    let outer = if name == "select" { std::mem::replace(&mut self.select_value, select_value) } else { self.select_value.take() };
    let children = self.children(&el.children);
    self.select_value = outer;
    Ok(Tmpl::Element { tag: name, attrs, children: children? })
  }

  fn component_ref(&mut self, name: &str, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    let mut props = Vec::new();
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          props.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "key" || raw == "ref" || is_handler_name(&raw) {
        continue;
      }
      if raw == "children" {
        return Err(self.lowerer.residue(attr.span, "`children` as a prop; pass them between the tags"));
      }
      let value = self.attr_value(attr)?;
      props.push(Entry::Field(raw, value));
    }
    let children = self.children(&el.children)?;
    let loc = self.lowerer.parsed.cm.lookup_char_pos(el.span.lo);
    self.refs.push((name.to_owned(), (loc.line, loc.col_display + 1)));
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children })
  }

  fn attr_value(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    match &attr.value {
      None => Ok(Expr::Lit(Lit::Bool(true))),
      Some(js::JSXAttrValue::Str(s)) => Ok(Expr::Lit(Lit::Str(decode_entities(&s.value.to_atom_lossy())))),
      Some(js::JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
        js::JSXExpr::Expr(e) => self.lowerer.expr(e),
        js::JSXExpr::JSXEmptyExpr(e) => Err(self.lowerer.residue(e.span, "an empty attribute expression")),
      },
      Some(other) => Err(self.lowerer.residue(other.span(), "an element as an attribute value")),
    }
  }

  /// `style={{ a: x, bTwo: y }}` as an object keyed by CSS name, which the
  /// renderer serialises the way React does.
  fn style(&mut self, attr: &'p js::JSXAttr) -> Lowered<Expr> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else {
      return self.attr_value(attr);
    };
    let js::JSXExpr::Expr(e) = &c.expr else {
      return Err(self.lowerer.residue(attr.span, "an empty style"));
    };
    let js::Expr::Object(obj) = &**e else {
      return Err(self.lowerer.residue(e.span(), "a style that is not an object literal"));
    };
    let mut entries = Vec::new();
    for prop in &obj.props {
      let (key, value) = match prop {
        js::PropOrSpread::Spread(spread) => {
          entries.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
        js::PropOrSpread::Prop(p) => match &**p {
          js::Prop::KeyValue(kv) => (prop_name(&kv.key).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed style property"))?, self.lowerer.expr(&kv.value)?),
          js::Prop::Shorthand(id) => (id.sym.to_string(), self.lowerer.ident(id)?),
          other => return Err(self.lowerer.residue(other.span(), "a method in a style")),
        },
      };
      entries.push(Entry::Field(css_name(&key), value));
    }
    Ok(Expr::Object(entries))
  }

  fn children(&mut self, children: &'p [js::JSXElementChild]) -> Lowered<Vec<Tmpl>> {
    let mut out = Vec::new();
    for child in children {
      match child {
        js::JSXElementChild::JSXText(text) => {
          let cleaned = jsx_text(&text.value.to_atom_lossy());
          if !cleaned.is_empty() {
            out.push(Tmpl::Text(cleaned));
          }
        }
        js::JSXElementChild::JSXExprContainer(c) => match &c.expr {
          js::JSXExpr::JSXEmptyExpr(_) => {}
          js::JSXExpr::Expr(e) => out.push(self.child_expr(e)?),
        },
        js::JSXElementChild::JSXElement(el) => out.push(self.element(el)?),
        js::JSXElementChild::JSXFragment(frag) => out.push(Tmpl::Fragment(self.children(&frag.children)?)),
        js::JSXElementChild::JSXSpreadChild(s) => return Err(self.lowerer.residue(s.span, "a spread child")),
      }
    }
    Ok(out)
  }
}

fn attr_name(name: &js::JSXAttrName) -> String {
  match name {
    js::JSXAttrName::Ident(id) => id.sym.to_string(),
    js::JSXAttrName::JSXNamespacedName(n) => format!("{}:{}", n.ns.sym, n.name.sym),
  }
}

fn is_handler_name(name: &str) -> bool {
  name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase()
}

fn css_name(key: &str) -> String {
  let mut out = String::with_capacity(key.len() + 4);
  for c in key.chars() {
    if c.is_ascii_uppercase() {
      out.push('-');
      out.push(c.to_ascii_lowercase());
    } else {
      out.push(c);
    }
  }
  out
}

/// JSX's whitespace rule: lines are trimmed, blank lines dropped, the rest
/// joined by one space; a single line keeps its inner spacing.
fn jsx_text(raw: &str) -> String {
  let decoded = decode_entities(raw);
  if !decoded.contains('\n') {
    return decoded;
  }
  let lines: Vec<&str> = decoded.lines().collect();
  let last = lines.len().saturating_sub(1);
  let mut parts = Vec::new();
  for (i, line) in lines.iter().enumerate() {
    let mut piece: &str = line;
    if i > 0 {
      piece = piece.trim_start();
    }
    if i < last {
      piece = piece.trim_end();
    }
    if !piece.is_empty() {
      parts.push(piece);
    }
  }
  parts.join(" ")
}

fn decode_entities(raw: &str) -> String {
  if !raw.contains('&') {
    return raw.to_owned();
  }
  let mut out = String::with_capacity(raw.len());
  let mut rest = raw;
  while let Some(start) = rest.find('&') {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    let Some(end) = rest.find(';').filter(|&e| e <= 10) else {
      out.push('&');
      rest = &rest[1..];
      continue;
    };
    let entity = &rest[1..end];
    let decoded = match entity {
      "amp" => Some('&'),
      "lt" => Some('<'),
      "gt" => Some('>'),
      "quot" => Some('"'),
      "apos" => Some('\''),
      "nbsp" => Some('\u{a0}'),
      "copy" => Some('\u{a9}'),
      "reg" => Some('\u{ae}'),
      "minus" => Some('\u{2212}'),
      "times" => Some('\u{d7}'),
      "middot" => Some('\u{b7}'),
      "hellip" => Some('\u{2026}'),
      "mdash" => Some('\u{2014}'),
      "ndash" => Some('\u{2013}'),
      "rarr" => Some('\u{2192}'),
      "larr" => Some('\u{2190}'),
      "laquo" => Some('\u{ab}'),
      "raquo" => Some('\u{bb}'),
      "bull" => Some('\u{2022}'),
      "trade" => Some('\u{2122}'),
      "euro" => Some('\u{20ac}'),
      "pound" => Some('\u{a3}'),
      "yen" => Some('\u{a5}'),
      "deg" => Some('\u{b0}'),
      _ => entity
        .strip_prefix('#')
        .and_then(|n| match n.strip_prefix('x').or_else(|| n.strip_prefix('X')) {
          Some(hex) => u32::from_str_radix(hex, 16).ok(),
          None => n.parse().ok(),
        })
        .and_then(char::from_u32),
    };
    match decoded {
      Some(c) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

/// Whether a render reaching this expression would meet JSX.
fn holds_jsx(expr: &js::Expr) -> bool {
  match expr {
    js::Expr::JSXElement(_) | js::Expr::JSXFragment(_) => true,
    js::Expr::Paren(p) => holds_jsx(&p.expr),
    js::Expr::Cond(c) => holds_jsx(&c.cons) || holds_jsx(&c.alt),
    js::Expr::Bin(b) if b.op == js::BinaryOp::LogicalAnd => holds_jsx(&b.right),
    js::Expr::Call(call) => map_with_jsx(call),
    _ => false,
  }
}

/// `xs.map((x) => <jsx>)` or with a block whose return is JSX.
fn map_with_jsx(call: &js::CallExpr) -> bool {
  let js::Callee::Expr(callee) = &call.callee else { return false };
  let js::Expr::Member(member) = &**callee else { return false };
  if !matches!(&member.prop, js::MemberProp::Ident(id) if id.sym.as_ref() == "map") {
    return false;
  }
  let Some(first) = call.args.first() else { return false };
  let js::Expr::Arrow(arrow) = &*first.expr else { return false };
  match &*arrow.body {
    js::ArrowFunctionBody::Expr(e) => holds_jsx(e),
    js::ArrowFunctionBody::FunctionBody(b) => b.stmts.iter().any(|s| matches!(s, js::Stmt::Return(r) if r.arg.as_deref().is_some_and(holds_jsx))),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn app(files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("fsr_component_{}_{}", std::process::id(), files.len() + files[0].1.len()));
    let _ = std::fs::remove_dir_all(&dir);
    for (name, source) in files {
      let path = dir.join(name);
      std::fs::create_dir_all(path.parent().unwrap()).unwrap();
      std::fs::write(path, source).unwrap();
    }
    dir
  }

  fn lower(files: &[(&str, &str)], module: &str) -> Result<Vec<(String, Component)>, LowerError> {
    let mut set = ComponentSet::new(&app(files));
    set.lower(module)?;
    Ok(set.components)
  }

  #[test]
  fn jsx_lowers_to_elements_attributes_and_the_three_idioms() {
    let page = r#"
import type { Props } from "../../generated/client";
export default function Page({ items, q = "" }: Props) {
  const n = items.length;
  return (
    <main className="page" aria-label={`Results for ${q}`}>
      <h1>{n} result{n === 1 ? "" : "s"}</h1>
      {n === 0 ? <p>Nothing</p> : null}
      {q && <b>{q}</b>}
      <ul>
        {items.map((it) => (
          <li key={String(it.id)} onClick={() => go(it)}>
            {it.name}
          </li>
        ))}
      </ul>
    </main>
  );
}
"#;
    let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
    assert_eq!(lowered.len(), 1);
    let component = &lowered[0].1;
    assert_eq!(component.body, vec![Stmt::Let { name: "n".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("items"))) }]);
    let Tmpl::Element { tag, attrs, children } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(tag, "main");
    assert_eq!(attrs[0], Entry::Field("class".to_owned(), Expr::lit_str("page")));
    assert!(matches!(&attrs[1], Entry::Field(name, Expr::Template(_)) if name == "aria-label"));
    assert!(matches!(&children[1], Tmpl::If { r#else: Some(e), .. } if matches!(**e, Tmpl::Fragment(ref f) if f.is_empty())));
    assert!(matches!(&children[2], Tmpl::If { r#else: None, .. }));
    let Tmpl::Element { children: ul, .. } = &children[3] else { panic!() };
    let Tmpl::For { params, body, .. } = &ul[0] else { panic!("{:?}", ul[0]) };
    assert_eq!(params, &["it".to_owned()]);
    let Tmpl::Element { attrs, children, .. } = &**body else { panic!() };
    assert!(attrs.is_empty(), "key and handlers are dropped: {attrs:?}");
    assert_eq!(children, &vec![Tmpl::Expr(Expr::var("it").field("name"))]);
  }

  #[test]
  fn helpers_components_and_state_resolve_across_local_modules() {
    let files = [
      (
        "routes/product/page.tsx",
        r#"
import { useState } from "react";
import { money } from "../../src/ui/money";
import { Stars } from "../../src/ui/Stars";
import { actions } from "../../generated/client";
export default function Product({ product }: { product: { price: number; rating: number } }) {
  const [quantity, setQuantity] = useState(1);
  async function add() { await actions.cart.add({ quantity }); }
  return (
    <div style={{ background: product.color, marginTop: 4 }}>
      <Stars rating={product.rating} />
      <span>{money(product.price)}</span>
      <select value={quantity} onChange={(e) => setQuantity(Number(e.target.value))} />
      <button onClick={() => void add()} disabled={product.price === 0}>Add</button>
    </div>
  );
}
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  if (cents === 0) return \"free\";\n  const dollars = cents / 100;\n  return `$${dollars.toFixed(2)}`;\n}\n"),
      ("src/ui/Stars.tsx", "import Swal from \"sweetalert2\";\nexport function Stars({ rating }: { rating: number }) {\n  const full = Math.round(rating);\n  return <span>{\"★\".repeat(full)}</span>;\n}\nexport function toast() { Swal.fire(); }\n"),
    ];
    let lowered = lower(&files, "routes/product/page.tsx#default").unwrap();
    let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(modules, ["src/ui/Stars.tsx#Stars", "routes/product/page.tsx#default"], "the referenced component lands first");
    let page = &lowered[1].1;
    assert_eq!(page.body, vec![Stmt::Let { name: "quantity".to_owned(), expr: Expr::Lit(Lit::Float(1.0)) }], "useState(x) reads as x");
    let Tmpl::Element { attrs, children, .. } = &page.render else { panic!() };
    assert!(matches!(&attrs[0], Entry::Field(name, Expr::Object(entries)) if name == "style" && entries.len() == 2));
    assert!(matches!(&children[0], Tmpl::Component { module, props, .. } if module == "src/ui/Stars.tsx#Stars" && props.len() == 1));
    let Tmpl::Element { children: span, .. } = &children[1] else { panic!() };
    assert!(matches!(&span[0], Tmpl::Expr(Expr::Apply { f, args }) if matches!(**f, Expr::Lambda { .. }) && args.len() == 1), "{:?}", span[0]);
    let Tmpl::Element { attrs, .. } = &children[3] else { panic!() };
    assert_eq!(attrs.len(), 1, "onClick is dropped, disabled stays: {attrs:?}");
  }

  #[test]
  fn children_spreads_and_hooks() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Page } from "../../src/ui/Page";
export default function Catalog({ products, cartCount, q }: { products: { name: string }[]; cartCount: number; q: string }) {
  const [open, setOpen] = useState(false);
  const header = { cartCount, q };
  const count = useMemo(() => products.length, [products]);
  const box = useRef(null);
  const toggle = useCallback(() => setOpen(!open), [open]);
  const focus = () => box.current?.focus();
  useEffect(() => { document.title = q; }, [q]);
  return (
    <Page {...header} className="catalog">
      <h1 {...(open ? { hidden: true } : {})} className="title" ref={box}>{count}</h1>
      {products.map((p) => <p key={p.name}>{p.name}</p>)}
    </Page>
  );
}
"#,
      ),
      (
        "src/ui/Page.tsx",
        r#"
import { Header } from "./Header";
export function Page({ className, children, ...rest }: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header {...rest} /><main className={`page ${className}`}>{children}</main></>;
}
"#,
      ),
      ("src/ui/Header.tsx", "export function Header(props: { cartCount: number; q: string }) {\n  return <header>{props.q}{props.children}</header>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
    let Tmpl::Fragment(parts) = &layout.render else { panic!() };
    let rest = Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$props"), Expr::lit_str("className"), Expr::lit_str("children")] });
    assert!(matches!(&parts[0], Tmpl::Component { props, .. } if props[0] == rest), "the rest is the props without what was named: {:?}", parts[0]);
    let mut files = files;
    files[1].1 = r#"
import { Header } from "./Header";
export function Page(props: { className: string; children: React.ReactNode; cartCount: number; q: string }) {
  return <><Header cartCount={props.cartCount} q={props.q} /><main className={`page ${props.className}`}>{props.children}</main></>;
}
"#;
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let names: Vec<&str> = page.body.iter().map(|s| match s { Stmt::Let { name, .. } => name.as_str(), _ => "" }).collect();
    assert_eq!(names, ["open", "header", "count", "box"], "useCallback and the arrow are handlers, useEffect is dropped");
    assert_eq!(page.body[2], Stmt::Let { name: "count".to_owned(), expr: Expr::Length(Box::new(Expr::var("$props").field("products"))) });
    assert_eq!(page.body[3], Stmt::Let { name: "box".to_owned(), expr: Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]) });
    let Tmpl::Component { module, props, children } = &page.render else { panic!("{:?}", page.render) };
    assert_eq!(module, "src/ui/Page.tsx#Page");
    assert!(matches!(&props[0], Entry::Spread(Expr::Var(v)) if v == "header"));
    assert_eq!(props[1], Entry::Field("className".to_owned(), Expr::lit_str("catalog")));
    let Tmpl::Element { attrs, children: h1, .. } = &children[0] else { panic!("{:?}", children[0]) };
    assert!(matches!(&attrs[0], Entry::Spread(Expr::Ternary(..))));
    assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("title")));
    assert_eq!(h1, &vec![Tmpl::Expr(Expr::var("count"))]);
    assert!(matches!(&children[1], Tmpl::For { .. }));
    let layout = &lowered.iter().find(|(m, _)| m == "src/ui/Page.tsx#Page").unwrap().1;
    let Tmpl::Fragment(parts) = &layout.render else { panic!() };
    let Tmpl::Element { children: main, .. } = &parts[1] else { panic!() };
    assert_eq!(main, &vec![Tmpl::Slot]);
    let header = &lowered.iter().find(|(m, _)| m == "src/ui/Header.tsx#Header").unwrap().1;
    let Tmpl::Element { children, .. } = &header.render else { panic!() };
    assert_eq!(children[1], Tmpl::Slot, "`props.children` is the slot when props are bound whole");
  }

  #[test]
  fn residue_names_the_line_and_the_construct() {
    let page = "export default function Page() {\n  const params = new URLSearchParams();\n  return <a href={params.toString()}>x</a>;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert_eq!(err.to_string(), "routes/index/page.tsx:2:18: `new`");
    let page = "import { Chart } from \"chart-lib\";\nexport default function Page() {\n  return <Chart />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`Chart` comes from `chart-lib`, which the build cannot follow"), "{err}");
  }

  #[test]
  fn jsx_text_follows_the_whitespace_rule_and_decodes_entities() {
    assert_eq!(jsx_text("\n    Hello\n    world\n  "), "Hello world");
    assert_eq!(jsx_text("a  b"), "a  b");
    assert_eq!(jsx_text("&minus; &amp; &#8230; &#x2192; &bogus;"), "\u{2212} & \u{2026} \u{2192} &bogus;");
  }
  #[test]
  fn a_rest_in_a_destructuring_is_the_object_without_the_named_keys() {
    let page = r#"
export default function Page({ title, kind = "note", ...rest }: { title: string; kind?: string; id: number; hidden: boolean }) {
  const { id, ...attrs } = rest;
  return <section data-id={id} {...attrs}>{title}: {kind}</section>;
}
"#;
    let lowered = lower(&[("routes/index/page.tsx", page)], "routes/index/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    let props = Expr::var("$props");
    assert_eq!(component.body, vec![Stmt::Let { name: "$let3".to_owned(), expr: Expr::Builtin { name: Builtin::Omit, args: vec![props.clone(), Expr::lit_str("title"), Expr::lit_str("kind")] } }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(attrs[0], Entry::Field("data-id".to_owned(), Expr::var("$let3").field("id")));
    assert_eq!(attrs[1], Entry::Spread(Expr::Builtin { name: Builtin::Omit, args: vec![Expr::var("$let3"), Expr::lit_str("id")] }));
  }

  #[test]
  fn a_member_expression_tag_names_an_export_of_a_namespace_import() {
    let files = [
      ("routes/index/page.tsx", "import * as Ui from \"../../src/ui\";\nimport * as React from \"react\";\nexport default function Page() {\n  return <React.Fragment><Ui.Card title=\"a\" /></React.Fragment>;\n}\n"),
      ("src/ui/index.tsx", "export function Card({ title }: { title: string }) {\n  return <div className=\"card\">{title}</div>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let modules: Vec<&str> = lowered.iter().map(|(m, _)| m.as_str()).collect();
    assert_eq!(modules, ["src/ui/index.tsx#Card", "routes/index/page.tsx#default"]);
    let Tmpl::Fragment(children) = &lowered[1].1.render else { panic!("{:?}", lowered[1].1.render) };
    assert!(matches!(&children[0], Tmpl::Component { module, .. } if module == "src/ui/index.tsx#Card"));

    let page = "import { ui } from \"../../src/ui\";\nexport default function Page() {\n  return <ui.Card title=\"a\" />;\n}\n";
    let err = lower(&[("routes/index/page.tsx", page), ("src/ui/index.tsx", "export const ui = {};\n")], "routes/index/page.tsx#default").unwrap_err();
    assert!(err.to_string().contains("`ui` is not a namespace import"), "{err}");
  }

}
