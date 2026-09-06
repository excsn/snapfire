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

use snapfire_fsr_ir::ast::{Builtin, Component, Entry, Expr, Handler, Lit, Stmt, Tmpl};
use snapfire_fsr_ir::render::{html_attr_name, HANDLER_ATTR, KEY_ATTR, SERVER_MODE, UNLOWERED_ATTR};
use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;

use crate::hoist::{self, Candidates, Hook, Rewrite};
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
  /// Per layout module, the slots its `slots/` directory declares, so a prop
  /// of that name is a region rather than a value.
  pub slots: Vec<(String, Vec<String>)>,
  /// The source rewrites the bundle needs, one per component with a hoist.
  pub rewrites: Vec<Rewrite>,
  /// Per lowered module, whether it is pure: no state, no handler, no island,
  /// no slot, nothing ambient, and every component it renders pure too. A
  /// pure component inside a static subtree is rendered into the chunk.
  pub pure: HashMap<String, bool>,
}

impl ComponentSet {
  pub fn new(app: &Path) -> Self {
    Self { app: app.to_path_buf(), parsed: HashMap::new(), defaults: SessionDefaults::new(), components: Vec::new(), resolving: Vec::new(), layouts: Vec::new(), slots: Vec::new(), rewrites: Vec::new(), pure: HashMap::new() }
  }

  /// Every file with a rewrite, with its rewritten source.
  pub fn rewritten(&self) -> Vec<(String, String)> {
    let mut files: Vec<&str> = Vec::new();
    for rewrite in &self.rewrites {
      if !files.contains(&rewrite.file.as_str()) {
        files.push(&rewrite.file);
      }
    }
    files
      .into_iter()
      .filter_map(|file| {
        let parsed = self.parsed.get(file)?;
        let source = parsed.cm.files().first().map(|f| f.src.to_string())?;
        let rewrites: Vec<&Rewrite> = self.rewrites.iter().filter(|r| r.file == file).collect();
        Some((file.to_owned(), hoist::apply(&source, &rewrites)))
      })
      .collect()
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
    let module = format!("{file}#{export}");
    let ((component, refs), hoisting) = loop {
      let (result, unbound, hoisting) = {
        let parsed = self.parsed[file].clone();
        let function = find_function(&parsed, export).ok_or_else(|| LowerError::MissingExport { file: file.to_owned(), export: export.to_owned() })?;
        let defaults = self.defaults.clone();
        let mut lowerer = Lowerer::new(&parsed, &defaults);
        lowerer.globals = globals.clone();
        lowerer.hoisting = Some(Candidates::default());
        let layout_root = self.layouts.iter().any(|m| *m == module);
        let slot_names = self.slots.iter().find(|(m, _)| *m == module).map(|(_, names)| names.clone()).unwrap_or_default();
        let mut cl = ComponentLowerer { lowerer, file, handlers: Vec::new(), refs: Vec::new(), select_value: None, props_name: None, children_name: None, layout_root, slot_names, slot_props: Vec::new(), state: Vec::new(), hook: None, state_bindings: Vec::new(), setters: Vec::new(), handler_fns: HashMap::new(), lowered_handlers: Vec::new() };
        let result = cl.component(&function);
        let hoisting = cl.lowerer.hoisting.take().map(|candidates| (candidates, std::mem::take(&mut cl.state), cl.hook.take()));
        (result, cl.lowerer.unbound.take(), hoisting)
      };
      match result {
        Ok(done) => break (done, hoisting),
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
    let mut component = Component { body: component.body, render: rewrite_modules(component.render, &modules), state: component.state, handlers: component.handlers };
    if let Some((candidates, state, Some(hook))) = hoisting {
      let kept = hoist::decide(&mut component, &state);
      let pure = state.is_empty() && hoist::static_tree(&component.render, &self.pure);
      let chunks = hoist::chunks(&mut component, &state, &self.pure);
      self.pure.insert(module.clone(), pure);
      if let Some(rewrite) = candidates.rewrite(&kept, &chunks, file, &module, hook) {
        self.rewrites.push(rewrite);
      }
    }
    Ok(component)
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
    Tmpl::Island { module, props, children, when, mode } => Tmpl::Island { module: modules.get(&module).cloned().unwrap_or(module), props, children: children.into_iter().map(|c| rewrite_modules(c, modules)).collect(), when, mode },
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
          return Some(Found::Declared(patterns(&f.function), &body.stmts, body.span));
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
  Declared(Vec<js::Pat>, &'a [js::Stmt], Span),
  Arrow(&'a js::ArrowExpr),
}

fn decl_function<'a>(decl: &'a js::Decl, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Decl::Fn(f) if f.ident.sym.as_ref() == name => {
      let body = f.function.body.as_ref()?;
      Some(Found::Declared(patterns(&f.function), &body.stmts, body.span))
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
pub(crate) fn find_import(parsed: &Parsed, local: &str) -> Option<(String, String)> {
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
  /// The slots a layout's `slots/` directory declares.
  slot_names: Vec<String>,
  /// Destructured props of a layout that name a slot: the local name and the slot.
  slot_props: Vec<(String, String)>,
  /// Bindings the browser can change without a new payload: state, store
  /// and ref hooks. A hoist reading one is not props only.
  state: Vec<String>,
  /// Where the rewrite binds the hoist reader in this component.
  hook: Option<Hook>,
  /// The `useState` and `useStore` bindings, in order: a server-mode island's state.
  state_bindings: Vec<String>,
  /// Each setter and the state it sets.
  setters: Vec<(String, String)>,
  /// Handler functions declared in the component, by name, for `onClick={add}`.
  handler_fns: HashMap<String, (Vec<js::Pat>, FunctionBody<'p>)>,
  /// The handlers lowered so far; an element's `$on:` marker holds an index into it.
  lowered_handlers: Vec<Handler>,
}

impl ComponentLowerer<'_, '_> {
  fn slot(&self, name: &str) -> Tmpl {
    if self.layout_root {
      let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
      Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![Tmpl::Slot(name.to_owned())] }
    } else {
      Tmpl::Slot(name.to_owned())
    }
  }

  /// A named slot with the markup to show while the plan leaves it unfilled:
  /// the region holds the segment when `$props.$slots` names the slot and
  /// the fallback otherwise.
  fn slot_with_fallback(&self, name: &str, fallback: Vec<Tmpl>) -> Tmpl {
    let filled = Expr::Builtin {
      name: Builtin::Includes,
      args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str(name)],
    };
    let inner = Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot(name.to_owned())), r#else: Some(Box::new(Tmpl::Fragment(fallback))) };
    let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
    Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![inner] }
  }

  /// The slot an expression stands for: `children` or `props.children` is
  /// `content`; in a layout, a prop named after one of its slots is that slot.
  fn slot_of_expr(&self, expr: &js::Expr) -> Option<String> {
    match expr {
      js::Expr::Ident(id) => {
        let name = id.sym.as_ref();
        if self.children_name.as_deref() == Some(name) {
          return Some("content".to_owned());
        }
        self.slot_props.iter().find(|(local, _)| local == name).map(|(_, slot)| slot.clone())
      }
      js::Expr::Member(m) => {
        let js::Expr::Ident(obj) = &*m.obj else { return None };
        let js::MemberProp::Ident(prop) = &m.prop else { return None };
        if self.props_name.as_deref() != Some(obj.sym.as_ref()) {
          return None;
        }
        let prop = prop.sym.as_ref();
        if prop == "children" {
          return Some("content".to_owned());
        }
        (self.layout_root && self.slot_names.iter().any(|n| n == prop)).then(|| prop.to_owned())
      }
      _ => None,
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
      Found::Declared(params, stmts, block) => {
        self.hook = Some(Hook::Block { after: self.lowerer.parsed.range(*block).start + 1 });
        (params.clone(), FunctionBody::Block(stmts))
      }
      Found::Arrow(arrow) => {
        self.hook = Some(match &*arrow.body {
          js::ArrowFunctionBody::FunctionBody(b) => Hook::Block { after: self.lowerer.parsed.range(b.span).start + 1 },
          js::ArrowFunctionBody::Expr(e) => Hook::Expression(self.lowerer.parsed.range(e.span())),
        });
        (arrow.params.clone(), arrow_body(arrow))
      }
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
            js::Stmt::Decl(js::Decl::Fn(f)) => {
              self.handlers.push(f.ident.sym.to_string());
              if let Some(body) = &f.function.body {
                self.handler_fns.insert(f.ident.sym.to_string(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
              }
            }
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
    Ok((Component { body: lets, render, state: std::mem::take(&mut self.state_bindings), handlers: std::mem::take(&mut self.lowered_handlers) }, std::mem::take(&mut self.refs)))
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
        if self.layout_root {
          for prop in &obj.props {
            let (local, key) = match prop {
              js::ObjectPatProp::Assign(a) => (a.key.id.sym.to_string(), a.key.id.sym.to_string()),
              js::ObjectPatProp::KeyValue(kv) => match (prop_name(&kv.key), &*kv.value) {
                (Some(key), js::Pat::Ident(local)) => (local.id.sym.to_string(), key),
                _ => continue,
              },
              js::ObjectPatProp::Rest(_) => continue,
            };
            if self.slot_names.contains(&key) {
              self.slot_props.push((local, key));
            }
          }
        }
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))
      }
      other => Err(self.lowerer.residue(other.span(), "the props parameter must be a name or a destructuring")),
    }
  }

  /// `const x = e` as a `let`; `const [x, setX] = useState(e)` as `let x = e`
  /// with `setX` a handler; `const { a, b } = e` as one `let` plus field reads.
  /// A function value, `useCallback` included, is a handler.
  fn let_stmt(&mut self, decl: &'p js::VarDeclarator) -> Lowered<Option<Stmt>> {
    let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
    match &decl.name {
      js::Pat::Ident(name) => {
        let local = name.id.sym.to_string();
        let expr = match hook_call(init) {
          Some(("useCallback", call)) => {
            if let Some(js::Expr::Arrow(arrow)) = call.args.first().map(|a| &*a.expr) {
              self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
            }
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
            self.state.push(local.clone());
            Expr::Object(vec![Entry::Field("current".to_owned(), current)])
          }
          Some(("useStore", _)) => {
            return Err(self.lowerer.residue(decl.span, "`useStore` bound to one name; it is a pair, as `const [x, setX] = useStore(key, initial)`"))
          }
          Some(("useLocale", _)) => Expr::Locale,
          Some((hook, _)) if hook != "useState" => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
          _ if matches!(init, js::Expr::Arrow(_) | js::Expr::Fn(_)) => {
            match init {
              js::Expr::Arrow(arrow) => {
                self.handler_fns.insert(local.clone(), (arrow.params.clone(), arrow_body(arrow)));
              }
              js::Expr::Fn(f) => {
                if let Some(body) = &f.function.body {
                  self.handler_fns.insert(local.clone(), (patterns(&f.function), FunctionBody::Block(&body.stmts)));
                }
              }
              _ => {}
            }
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
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState` or `useStore`"));
        };
        let called = match &call.callee {
          js::Callee::Expr(e) => match &**e {
            js::Expr::Ident(id) => id.sym.to_string(),
            _ => String::new(),
          },
          _ => String::new(),
        };
        if called == "useStore" {
          return self.store_stmt(decl, arr, call);
        }
        if called != "useState" {
          return Err(self.lowerer.residue(decl.span, "an array destructuring of something other than `useState` or `useStore`"));
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
        let setter = names.next().flatten();
        let Some(name) = state else { return Ok(None) };
        if let Some(setter) = setter {
          self.setters.push((setter.clone(), name.clone()));
          self.handlers.push(setter);
        }
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
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

  /// `const [x, setX] = useStore(key, initial)` as `let x = <store key> ?? initial`,
  /// with `setX` a handler. The key must lower to a string: a literal, or a
  /// `key()` from the client's store, wherever it is declared.
  fn store_stmt(&mut self, decl: &js::VarDeclarator, arr: &js::ArrayPat, call: &js::CallExpr) -> Lowered<Option<Stmt>> {
    let Some(first) = call.args.first() else {
      return Err(self.lowerer.residue(decl.span, "`useStore` without a key"));
    };
    let key = match self.lowerer.expr(&first.expr)? {
      Expr::Lit(Lit::Str(key)) => key,
      _ => return Err(self.lowerer.residue(first.expr.span(), "a `useStore` key that is not a string the build can read")),
    };
    let initial = match call.args.get(1) {
      Some(a) => self.lowerer.expr(&a.expr)?,
      None => Expr::Lit(Lit::Null),
    };
    let mut names = arr.elems.iter().map(|e| match e {
      Some(js::Pat::Ident(id)) => Some(id.id.sym.to_string()),
      _ => None,
    });
    let held = names.next().flatten();
    let setter = names.next().flatten();
    let Some(name) = held else { return Ok(None) };
    if let Some(setter) = setter {
      self.setters.push((setter.clone(), name.clone()));
      self.handlers.push(setter);
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(initial)) }))
  }

  /// A JSX child or a component's return: a tree when the expression holds
  /// JSX anywhere a render would reach, text otherwise.
  fn child_expr(&mut self, expr: &'p js::Expr) -> Lowered<Tmpl> {
    if let Some(name) = self.slot_of_expr(expr) {
      return Ok(self.slot(&name));
    }
    match expr {
      js::Expr::Paren(p) => self.child_expr(&p.expr),
      js::Expr::JSXElement(el) => self.element(el, false),
      js::Expr::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expr::Lit(js::Lit::Str(s)) => Ok(Tmpl::Text(s.value.to_atom_lossy().to_string())),
      js::Expr::Cond(c) if holds_jsx(&c.cons) || holds_jsx(&c.alt) => {
        let cond = self.lowerer.expr(&c.test)?;
        let then = Box::new(self.child_expr(&c.cons)?);
        let r#else = Some(Box::new(self.child_expr(&c.alt)?));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expr::Bin(bin) if bin.op == js::BinaryOp::NullishCoalescing && self.layout_root && self.slot_of_expr(&bin.left).is_some() => {
        let name = self.slot_of_expr(&bin.left).expect("checked by the guard");
        let fallback = self.child_expr(&bin.right)?;
        Ok(self.slot_with_fallback(&name, vec![fallback]))
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
        let range = self.lowerer.parsed.range(arrow.span);
        if let Some(candidates) = &mut self.lowerer.hoisting {
          candidates.open_loops.push(range);
        }
        let body = match &*arrow.body {
          js::ArrowFunctionBody::Expr(e) => self.child_expr(e),
          js::ArrowFunctionBody::FunctionBody(b) => self.block_tree(&b.stmts),
        };
        if let Some(candidates) = &mut self.lowerer.hoisting {
          candidates.open_loops.pop();
        }
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::For { over, params, body: Box::new(body?) })
      }
      js::Expr::Ident(id) if id.sym.as_ref() == "null" || id.sym.as_ref() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expr::Lit(js::Lit::Null(_)) => Ok(Tmpl::Fragment(Vec::new())),
      other => Ok(Tmpl::Expr(self.lowerer.expr(other)?)),
    }
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

  /// `as_child` says the element sits directly among JSX children, where a
  /// rewrite of it must be braced, rather than in an expression.
  fn element(&mut self, el: &'p js::JSXElement, as_child: bool) -> Lowered<Tmpl> {
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
    let mut bound = false;
    for attr in &el.opening.attrs {
      let attr = match attr {
        js::JSXAttrOrSpread::JSXAttr(attr) => attr,
        js::JSXAttrOrSpread::SpreadElement(spread) => {
          bound = true;
          attrs.push(Entry::Spread(self.lowerer.expr(&spread.expr)?));
          continue;
        }
      };
      let raw = attr_name(&attr.name);
      if raw == "ref" {
        bound = true;
        continue;
      }
      if is_handler_name(&raw) {
        bound = true;
        let event = raw[2..].to_ascii_lowercase();
        match self.handler_attr(attr) {
          Ok(index) => attrs.push(Entry::Field(format!("{HANDLER_ATTR}{event}"), Expr::Lit(Lit::Int(index as i128)))),
          Err(residue) => {
            if !attrs.iter().any(|e| matches!(e, Entry::Field(n, _) if n == UNLOWERED_ATTR)) {
              attrs.push(Entry::Field(UNLOWERED_ATTR.to_owned(), Expr::lit_str(format!("{}:{}: {}", residue.line, residue.column, residue.message))));
            }
          }
        }
        continue;
      }
      if raw == "key" {
        attrs.push(Entry::Field(KEY_ATTR.to_owned(), self.attr_value(attr)?));
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
    let children = children?;
    if bound {
      attrs.push(Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true))));
    }
    if let Some(candidates) = &mut self.lowerer.hoisting {
      if !children.is_empty() && !el.opening.self_closing {
        let range = self.lowerer.parsed.range(el.span);
        let open = self.lowerer.parsed.range(el.opening.span);
        attrs.push(candidates.chunk(range, open, as_child));
      }
    }
    Ok(Tmpl::Element { tag: name, attrs, children })
  }

  /// True when `name` is imported from the client library's React adapter.
  fn is_client_react_import(&self, name: &str) -> bool {
    find_import(self.lowerer.parsed, name).is_some_and(|(source, _)| source == "@snapfire/fsr-client/react")
  }

  /// `<Island when="visible"><Chart … /></Island>`: the one component child
  /// as an island with that timing.
  fn island_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let mut when = None;
    let mut mode = None;
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Island>`")) };
      match attr_name(&attr.name).as_str() {
        "when" => {
          let value = self.attr_value(attr)?;
          when = Some(self.island_timing(value, attr.span)?);
        }
        "mode" => {
          let value = self.attr_value(attr)?;
          mode = self.island_mode(value, attr.span)?;
        }
        _ => return Err(self.lowerer.residue(attr.span, "`<Island>` takes `when` and `mode` and nothing else")),
      }
    }
    let mut elements = el.children.iter().filter(|c| match c {
      js::JSXElementChild::JSXText(text) => !jsx_text(&text.value.to_atom_lossy()).is_empty(),
      _ => true,
    });
    let (Some(js::JSXElementChild::JSXElement(child)), None) = (elements.next(), elements.next()) else {
      return Err(self.lowerer.residue(el.span, "`<Island>` wraps exactly one component"));
    };
    let lowered = self.element(child, false)?;
    self.island_of(lowered, when, mode, child.span)
  }

  /// `"server"` for an island whose events round-trip to the server; `"browser"`, the default, is `None`.
  fn island_mode(&self, value: Expr, span: Span) -> Lowered<Option<String>> {
    match value {
      Expr::Lit(Lit::Str(mode)) if mode == SERVER_MODE => Ok(Some(mode)),
      Expr::Lit(Lit::Str(mode)) if mode == "browser" => Ok(None),
      _ => Err(self.lowerer.residue(span, "an island's `mode` is \"browser\" or \"server\", written out")),
    }
  }

  /// An `on*` attribute's handler as a lowered body, or why it is not one.
  /// A handler is `const`s and calls to state setters, `e.preventDefault()`
  /// aside; the body returns the state it set.
  fn handler_attr(&mut self, attr: &'p js::JSXAttr) -> Lowered<usize> {
    let Some(js::JSXAttrValue::JSXExprContainer(c)) = &attr.value else { return Err(self.lowerer.residue(attr.span, "a handler that is not an expression")) };
    let js::JSXExpr::Expr(e) = &c.expr else { return Err(self.lowerer.residue(attr.span, "an empty handler")) };
    let body = self.handler_body(e)?;
    self.lowered_handlers.push(Handler { event: attr_name(&attr.name)[2..].to_ascii_lowercase(), body });
    Ok(self.lowered_handlers.len() - 1)
  }

  fn handler_body(&mut self, e: &'p js::Expr) -> Lowered<Vec<Stmt>> {
    match e {
      js::Expr::Paren(p) => self.handler_body(&p.expr),
      js::Expr::Arrow(arrow) => {
        let params = arrow.params.clone();
        let body = arrow_body(arrow);
        self.handler_fn(&params, body, arrow.span)
      }
      js::Expr::Ident(id) => {
        let name = id.sym.to_string();
        let Some((params, body)) = self.handler_fns.get(&name).cloned() else {
          return Err(self.lowerer.residue(id.span, format!("`{name}` is not a handler this component declares")));
        };
        self.handler_fn(&params, body, id.span)
      }
      other => Err(self.lowerer.residue(other.span(), "a handler that is not an arrow or a name")),
    }
  }

  fn handler_fn(&mut self, params: &[js::Pat], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let depth = self.lowerer.scope.len();
    if let Some(first) = params.first() {
      let js::Pat::Ident(id) = first else { return Err(self.lowerer.residue(first.span(), "a handler's event parameter must be a name")) };
      self.lowerer.scope.push((id.id.sym.to_string(), Expr::Var("$event".to_owned())));
    }
    let mut out = Vec::new();
    let mut patch: Vec<Entry> = Vec::new();
    let result = (|| {
      match body {
        FunctionBody::Expr(e) => self.handler_stmt(e, &mut out, &mut patch)?,
        FunctionBody::Block(stmts) => {
          for stmt in stmts {
            match stmt {
              js::Stmt::Expr(e) => self.handler_stmt(&e.expr, &mut out, &mut patch)?,
              js::Stmt::Decl(js::Decl::Var(var)) => {
                for decl in &var.decls {
                  let js::Pat::Ident(name) = &decl.name else { return Err(self.lowerer.residue(decl.span, "a destructuring in a handler")) };
                  let init = decl.init.as_deref().ok_or_else(|| self.lowerer.residue(decl.span, "a declaration without a value"))?;
                  let expr = self.lowerer.expr(init)?;
                  let local = name.id.sym.to_string();
                  self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
                  out.push(Stmt::Let { name: local, expr });
                }
              }
              js::Stmt::Return(r) if r.arg.is_none() => break,
              other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s and calls to state setters")),
            }
          }
        }
      }
      Ok(())
    })();
    self.lowerer.scope.truncate(depth);
    result?;
    if patch.is_empty() {
      return Err(self.lowerer.residue(span, "a handler that sets no state"));
    }
    out.push(Stmt::Return(Expr::Object(patch)));
    Ok(out)
  }

  /// `setX(expr)` or `setX((prev) => expr)` adds to the patch; `e.preventDefault()`
  /// and `e.stopPropagation()` are the browser's; `void f()` or `f()` naming a
  /// declared handler inlines it.
  fn handler_stmt(&mut self, e: &'p js::Expr, out: &mut Vec<Stmt>, patch: &mut Vec<Entry>) -> Lowered<()> {
    match e {
      js::Expr::Paren(p) => self.handler_stmt(&p.expr, out, patch),
      js::Expr::Unary(u) if u.op == js::UnaryOp::Void => self.handler_stmt(&u.arg, out, patch),
      js::Expr::Await(a) => self.handler_stmt(&a.arg, out, patch),
      js::Expr::Call(call) => {
        let js::Callee::Expr(callee) = &call.callee else { return Err(self.lowerer.residue(call.span, "a call a handler cannot make")) };
        match &**callee {
          js::Expr::Ident(id) => {
            let name = id.sym.to_string();
            if let Some((_, state)) = self.setters.iter().find(|(s, _)| *s == name).cloned() {
              let arg = call.args.first().ok_or_else(|| self.lowerer.residue(call.span, format!("`{name}` without a value")))?;
              let value = match &*arg.expr {
                js::Expr::Arrow(arrow) => {
                  let depth = self.lowerer.scope.len();
                  if let Some(js::Pat::Ident(prev)) = arrow.params.first() {
                    self.lowerer.scope.push((prev.id.sym.to_string(), Expr::Var(state.clone())));
                  }
                  let value = match &*arrow.body {
                    js::ArrowFunctionBody::Expr(e) => self.lowerer.expr(e),
                    js::ArrowFunctionBody::FunctionBody(b) => block_to_expr(&mut self.lowerer, &b.stmts),
                  };
                  self.lowerer.scope.truncate(depth);
                  value?
                }
                other => self.lowerer.expr(other)?,
              };
              patch.retain(|entry| !matches!(entry, Entry::Field(n, _) if *n == state));
              patch.push(Entry::Field(state, value));
              return Ok(());
            }
            if let Some((params, body)) = self.handler_fns.get(&name).cloned() {
              let inner = self.handler_fn(&params, body, call.span)?;
              for stmt in inner {
                match stmt {
                  Stmt::Return(Expr::Object(entries)) => {
                    for entry in entries {
                      if let Entry::Field(n, _) = &entry {
                        patch.retain(|held| !matches!(held, Entry::Field(m, _) if m == n));
                      }
                      patch.push(entry);
                    }
                  }
                  other => out.push(other),
                }
              }
              return Ok(());
            }
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not a state setter or a handler this component declares")))
          }
          js::Expr::Member(m) => {
            let method = match &m.prop {
              js::MemberProp::Ident(i) => i.sym.to_string(),
              _ => String::new(),
            };
            if method == "preventDefault" || method == "stopPropagation" {
              return Ok(());
            }
            Err(self.lowerer.residue(call.span, format!("`.{method}()` in a handler; a handler is `const`s and calls to state setters")))
          }
          other => Err(self.lowerer.residue(other.span(), "a call a handler cannot make")),
        }
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s and calls to state setters")),
    }
  }

  /// `const Lazy = island(Chart, { when: "visible" })` at module scope, when
  /// `name` is such a `Lazy`: the component and the timing.
  fn island_alias(&mut self, name: &str) -> Lowered<Option<(String, Option<String>, Option<String>)>> {
    let Some(Global::Const(js::Expr::Call(call))) = find_value(self.lowerer.parsed, name) else { return Ok(None) };
    let js::Callee::Expr(callee) = &call.callee else { return Ok(None) };
    let js::Expr::Ident(callee) = &**callee else { return Ok(None) };
    if callee.sym.as_ref() != "island" || !self.is_client_react_import("island") {
      return Ok(None);
    }
    let Some(js::Expr::Ident(target)) = call.args.first().map(|a| &*a.expr) else {
      return Err(self.lowerer.residue(call.span, "`island(...)` takes a component name first"));
    };
    let mut when = None;
    let mut mode = None;
    if let Some(options) = call.args.get(1) {
      let js::Expr::Object(obj) = &*options.expr else { return Err(self.lowerer.residue(options.span(), "`island(...)` options must be an object literal")) };
      for prop in &obj.props {
        let js::PropOrSpread::Prop(prop) = prop else { return Err(self.lowerer.residue(options.span(), "a spread in `island(...)` options")) };
        let js::Prop::KeyValue(kv) = &**prop else { return Err(self.lowerer.residue(options.span(), "`island(...)` options are `when` and `mode`")) };
        let value = self.lowerer.expr(&kv.value)?;
        match prop_name(&kv.key).as_deref() {
          Some("when") => when = Some(self.island_timing(value, options.span())?),
          Some("mode") => mode = self.island_mode(value, options.span())?,
          _ => return Err(self.lowerer.residue(options.span(), "`island(...)` options are `when` and `mode`")),
        }
      }
    }
    Ok(Some((target.sym.to_string(), when, mode)))
  }

  /// `<Slot name="modal" />` in a layout: the plan child of that name, in a
  /// region navigation fills and empties.
  fn slot_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    if !self.layout_root {
      return Err(self.lowerer.residue(el.span, "`<Slot>` outside a layout"));
    }
    let mut name = None;
    for attr in &el.opening.attrs {
      let js::JSXAttrOrSpread::JSXAttr(attr) = attr else { return Err(self.lowerer.residue(el.span, "a spread on `<Slot>`")) };
      if attr_name(&attr.name) != "name" {
        return Err(self.lowerer.residue(attr.span, "`<Slot>` takes `name` and nothing else"));
      }
      match self.attr_value(attr)? {
        Expr::Lit(Lit::Str(value)) => name = Some(value),
        _ => return Err(self.lowerer.residue(attr.span, "a slot's `name` is written out")),
      }
    }
    let Some(name) = name else { return Err(self.lowerer.residue(el.span, "`<Slot>` needs a `name`")) };
    let fallback = self.children(&el.children)?;
    if fallback.is_empty() {
      return Ok(self.slot(&name));
    }
    Ok(self.slot_with_fallback(&name, fallback))
  }

  /// `<Link href="/x" full into="modal" prefetch="none">` from the client
  /// library: an `<a>` carrying the data attributes the navigator reads.
  fn link_element(&mut self, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    let mut attrs = Vec::new();
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
      if raw == "style" {
        attrs.push(Entry::Field("style".to_owned(), self.style(attr)?));
        continue;
      }
      let value = self.attr_value(attr)?;
      let name = match raw.as_str() {
        "full" => "data-sf-full",
        "into" => "data-sf-into",
        "prefetch" => "data-sf-prefetch",
        "native" => "data-sf-native",
        other => html_attr_name(other),
      };
      attrs.push(Entry::Field(name.to_owned(), value));
    }
    let children = self.children(&el.children)?;
    Ok(Tmpl::Element { tag: "a".to_owned(), attrs, children })
  }

  fn island_timing(&self, value: Expr, span: Span) -> Lowered<String> {
    match value {
      Expr::Lit(Lit::Str(timing)) if matches!(timing.as_str(), "load" | "visible" | "idle") => Ok(timing),
      _ => Err(self.lowerer.residue(span, "an island's `when` is \"load\", \"visible\" or \"idle\", written out")),
    }
  }

  fn island_of(&self, lowered: Tmpl, when: Option<String>, mode: Option<String>, span: Span) -> Lowered<Tmpl> {
    match lowered {
      Tmpl::Component { module, props, children } => Ok(Tmpl::Island { module, props, children, when, mode }),
      _ => Err(self.lowerer.residue(span, "an island must be a component, not an element")),
    }
  }

  fn component_ref(&mut self, name: &str, el: &'p js::JSXElement) -> Lowered<Tmpl> {
    if name == "Fragment" || name == "React.Fragment" {
      return Ok(Tmpl::Fragment(self.children(&el.children)?));
    }
    if name == "Island" && self.is_client_react_import("Island") {
      return self.island_element(el);
    }
    if name == "Slot" && self.is_client_react_import("Slot") {
      return self.slot_element(el);
    }
    if name == "Link" && self.is_client_react_import("Link") {
      return self.link_element(el);
    }
    if let Some((target, when, mode)) = self.island_alias(name)? {
      let lowered = self.component_ref(&target, el)?;
      return self.island_of(lowered, when, mode, el.span);
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
        js::JSXElementChild::JSXElement(el) => out.push(self.element(el, true)?),
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

  /// The attributes the markup prints: without the build's `$` markers.
  fn plain(attrs: &[Entry]) -> Vec<Entry> {
    attrs.iter().filter(|e| !matches!(e, Entry::Field(n, _) if n.starts_with('$'))).cloned().collect()
  }

  fn hoist_ids(component: &Component) -> Vec<u32> {
    let mut ids = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Hoist { id, .. } = e {
        ids.push(*id);
      }
    });
    ids
  }

  fn chunk_ids(tmpl: &Tmpl, out: &mut Vec<(String, u32)>) {
    match tmpl {
      Tmpl::Element { tag, attrs, children } => {
        if let Some(Entry::Field(_, Expr::Lit(Lit::Int(id)))) = attrs.iter().find(|e| matches!(e, Entry::Field(n, _) if n == hoist::CHUNK_ATTR)) {
          out.push((tag.clone(), *id as u32));
        }
        children.iter().for_each(|c| chunk_ids(c, out));
      }
      Tmpl::Fragment(children) | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } => children.iter().for_each(|c| chunk_ids(c, out)),
      Tmpl::If { then, r#else, .. } => {
        chunk_ids(then, out);
        if let Some(e) = r#else {
          chunk_ids(e, out);
        }
      }
      Tmpl::For { body, .. } => chunk_ids(body, out),
      Tmpl::Let { then, .. } => chunk_ids(then, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }

  fn set(files: &[(&str, &str)], module: &str) -> ComponentSet {
    let mut set = ComponentSet::new(&app(files));
    set.lower(module).unwrap();
    set
  }

  #[test]
  fn a_helper_call_on_props_is_hoisted_and_one_on_state_is_not() {
    let files = [
      (
        "routes/cart/page.tsx",
        r#"
import { useState } from "react";
import { useStore } from "@snapfire/fsr-client/react";
import { money } from "@src/ui/money";
export default function Cart({ lines, total }: { lines: { price: number }[]; total: number }) {
  const [qty, setQty] = useState(1);
  const [tip] = useStore("cart/tip", 0);
  const grand = total + tip;
  return (
    <div>
      <b>{money(total)}</b>
      <i>{money(total * qty)}</i>
      <u>{money(grand)}</u>
      <ul>{lines.map((l) => <li key={l.price}>{money(l.price)}{money(l.price * qty)}</li>)}</ul>
      <button onClick={() => setQty(qty + 1)}>{money(1)}</button>
    </div>
  );
}
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\n"),
    ];
    let set = set(&files, "routes/cart/page.tsx#default");
    let component = &set.components[0].1;
    assert_eq!(hoist_ids(component), vec![0, 6, 10], "props only: money(total), money(l.price) and money(1); not qty, tip or grand: {component:?}");
    let mut chunks = Vec::new();
    chunk_ids(&component.render, &mut chunks);
    assert_eq!(chunks, vec![("b".to_owned(), 1)], "the one static element that does work; the others read state, hold a handler or only literals: {component:?}");
    assert_eq!(set.rewrites.len(), 1);
    let rewrite = &set.rewrites[0];
    assert_eq!(rewrite.module, "routes/cart/page.tsx#default");
    assert_eq!(rewrite.sites.len(), 3);
    assert_eq!(rewrite.chunks.len(), 1);
    assert_eq!(rewrite.loops.len(), 1, "the one .map callback holding a survivor");
    let (file, source) = set.rewritten().pop().unwrap();
    assert_eq!(file, "routes/cart/page.tsx");
    assert!(source.starts_with(hoist::IMPORT), "{source}");
    assert!(source.contains("{ const __sfh = __sfUseHoisted(\"routes/cart/page.tsx#default\"); "), "{source}");
    assert!(source.contains("__sfh.c(1, (__sfHtml) => <b dangerouslySetInnerHTML={__sfHtml} />, () => (<b>{__sfh.r(0, () => (money(total)))}</b>))"), "{source}");
    assert!(source.contains("<i>{money(total * qty)}</i>"), "a state read stays a call: {source}");
    assert!(source.contains("<ul>{lines.map(__sfh.l((l) => <li key={l.price}>{__sfh.r(6, () => (money(l.price)))}{money(l.price * qty)}</li>))}</ul>"), "{source}");
    assert!(source.contains("{__sfh.r(10, () => (money(1)))}</button>"), "{source}");
  }

  #[test]
  fn handlers_lower_to_state_patches_and_an_island_takes_a_mode() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { Island, island } from "@snapfire/fsr-client/react";
import { Stepper } from "@src/Stepper";
const Lazy = island(Stepper, { when: "idle", mode: "server" });
export default function Page() {
  return <main><Island mode="server"><Stepper start={1} /></Island><Lazy start={2} /><Island mode="browser"><Stepper start={3} /></Island></main>;
}
"#,
      ),
      (
        "src/Stepper.tsx",
        r#"
import { useState } from "react";
export function Stepper({ start, max = 9 }: { start: number; max?: number }) {
  const [n, setN] = useState(start);
  const [open, setOpen] = useState(false);
  const room = max - n;
  function reset() {
    setN(start);
    setOpen(false);
  }
  return (
    <div key="stepper">
      <button onClick={() => setN(n + 1)} disabled={room === 0}>+</button>
      <button onClick={() => setN((prev) => prev - 1)}>-</button>
      <input value={String(n)} onChange={(e) => { e.preventDefault(); const next = Number(e.target.value); setN(next); }} />
      <button onClick={reset}>reset</button>
      <button onClick={() => void reset()}>reset too</button>
      <button onClick={() => alert(n)}>shout</button>
      <label onClick={() => setOpen(!open)}>{open ? "open" : "closed"}</label>
      <ul>{[1, 2].map((i) => <li key={i}>{i}</li>)}</ul>
    </div>
  );
}
"#,
      ),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let Tmpl::Element { children, .. } = &page.render else { panic!() };
    assert!(matches!(&children[0], Tmpl::Island { mode: Some(m), when: None, .. } if m == "server"), "{:?}", children[0]);
    assert!(matches!(&children[1], Tmpl::Island { mode: Some(m), when: Some(w), .. } if m == "server" && w == "idle"), "{:?}", children[1]);
    assert!(matches!(&children[2], Tmpl::Island { mode: None, .. }), "browser is the default and spells as none: {:?}", children[2]);

    let stepper = &set.components.iter().find(|(m, _)| m == "src/Stepper.tsx#Stepper").unwrap().1;
    assert_eq!(stepper.state, ["n", "open"]);
    assert_eq!(stepper.handlers.len(), 6, "{:?}", stepper.handlers.iter().map(|h| &h.event).collect::<Vec<_>>());
    let events: Vec<&str> = stepper.handlers.iter().map(|h| h.event.as_str()).collect();
    assert_eq!(events, ["click", "click", "change", "click", "click", "click"]);
    assert_eq!(stepper.handlers[0].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Add, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))]);
    assert_eq!(stepper.handlers[1].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::Arith(snapfire_fsr_ir::ArithOp::Sub, Box::new(Expr::var("n")), Box::new(Expr::Lit(Lit::Float(1.0)))))]))], "a functional update reads the state as prev");
    assert_eq!(stepper.handlers[2].body, vec![Stmt::Let { name: "next".to_owned(), expr: Expr::Num(Box::new(Expr::var("$event").field("target").field("value"))) }, Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("next"))]))], "preventDefault is dropped, the event reads through $event");
    let reset = vec![Stmt::Return(Expr::Object(vec![Entry::Field("n".to_owned(), Expr::var("$props").field("start")), Entry::Field("open".to_owned(), Expr::Lit(Lit::Bool(false)))]))];
    assert_eq!(stepper.handlers[3].body, reset, "a named handler by name");
    assert_eq!(stepper.handlers[4].body, reset, "a named handler called");
    assert_eq!(stepper.handlers[5].body, vec![Stmt::Return(Expr::Object(vec![Entry::Field("open".to_owned(), Expr::Not(Box::new(Expr::var("open"))))]))]);

    let Tmpl::Element { attrs, children, .. } = &stepper.render else { panic!() };
    assert!(attrs.contains(&Entry::Field(KEY_ATTR.to_owned(), Expr::lit_str("stepper"))), "{attrs:?}");
    let on = |i: usize| -> Vec<(String, Expr)> {
      let Tmpl::Element { attrs, .. } = &children[i] else { panic!() };
      attrs.iter().filter_map(|e| match e {
        Entry::Field(n, v) if n.starts_with(HANDLER_ATTR) || n == UNLOWERED_ATTR => Some((n.clone(), v.clone())),
        _ => None,
      }).collect()
    };
    assert_eq!(on(0), vec![(format!("{HANDLER_ATTR}click"), Expr::Lit(Lit::Int(0)))]);
    assert_eq!(on(2), vec![(format!("{HANDLER_ATTR}change"), Expr::Lit(Lit::Int(2)))]);
    let shout = on(5);
    assert_eq!(shout.len(), 1);
    assert!(matches!(&shout[0], (n, Expr::Lit(Lit::Str(why))) if n == UNLOWERED_ATTR && why.contains("a call to `alert`")), "{shout:?}");
  }

  #[test]
  fn a_static_subtree_is_a_chunk_and_a_handler_state_read_island_or_impure_component_breaks_it() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { useState } from "react";
import { Island } from "@snapfire/fsr-client/react";
import { Price } from "@src/Price";
import { Counter } from "@src/Counter";
import { Chart } from "@src/Chart";
export default function Page({ items, note }: { items: { id: number; price: number }[]; note: string }) {
  const [open, setOpen] = useState(false);
  return (
    <main>
      <h1>Catalog</h1>
      <ul className="list">
        {items.map((it) => (
          <li key={it.id} title={String(it.id)}>
            <Price cents={it.price} />
          </li>
        ))}
      </ul>
      <section>
        <p>{note}</p>
        <Counter start={1} />
      </section>
      <aside>
        <p>{note}</p>
        <Island when="visible"><Chart series={note} /></Island>
      </aside>
      <div>
        <button onClick={() => setOpen(!open)}>{note}</button>
      </div>
      <footer>{open ? note : ""}</footer>
    </main>
  );
}
"#,
      ),
      ("src/Price.tsx", "export function Price({ cents }: { cents: number }) {\n  return <b>{(cents / 100).toFixed(2)}</b>;\n}\n"),
      ("src/Counter.tsx", "import { useState } from \"react\";\nexport function Counter({ start }: { start: number }) {\n  const [n, setN] = useState(start);\n  return <button onClick={() => setN(n + 1)}>{n}</button>;\n}\n"),
      ("src/Chart.tsx", "export function Chart({ series }: { series: string }) {\n  return <svg><title>{series}</title></svg>;\n}\n"),
    ];
    let set = set(&files, "routes/index/page.tsx#default");
    assert_eq!(set.pure.get("src/Price.tsx#Price"), Some(&true));
    assert_eq!(set.pure.get("src/Counter.tsx#Counter"), Some(&false), "state and a handler");
    assert_eq!(set.pure.get("routes/index/page.tsx#default"), Some(&false));
    let page = &set.components.iter().find(|(m, _)| m == "routes/index/page.tsx#default").unwrap().1;
    let mut chunks = Vec::new();
    chunk_ids(&page.render, &mut chunks);
    let tags: Vec<&str> = chunks.iter().map(|(t, _)| t.as_str()).collect();
    assert_eq!(tags, ["ul", "p", "p"], "the list with its pure Price cards is one chunk; the paragraphs beside the impure Counter and the island are chunks of their own; the h1 is literal, the button is bound and the footer reads state: {chunks:?}");
    let rewrite = set.rewrites.iter().find(|r| r.module == "routes/index/page.tsx#default").unwrap();
    assert_eq!(rewrite.chunks.len(), 3);
    assert!(rewrite.loops.is_empty(), "the loop sits inside the chunk, whose fallback renders it as written");
    let (_, source) = set.rewritten().into_iter().find(|(f, _)| f == "routes/index/page.tsx").unwrap();
    assert!(source.contains("{__sfh.c(2, (__sfHtml) => <ul className=\"list\" dangerouslySetInnerHTML={__sfHtml} />, () => (<ul className=\"list\">"), "{source}");
    assert!(source.contains("      </ul>))}\n"), "a chunk among JSX children is braced: {source}");
    assert!(source.contains("{__sfh.c(3, (__sfHtml) => <p dangerouslySetInnerHTML={__sfHtml} />, () => (<p>{note}</p>))}"), "{source}");
    assert!(source.contains("<button onClick={() => setOpen(!open)}>{note}</button>"), "a bound element is untouched: {source}");
    assert_eq!(source.matches("__sfh.c(").count(), 3, "{source}");
  }

  #[test]
  fn hoists_stay_out_of_lambdas_nested_calls_and_client_components() {
    let files = [
      (
        "routes/index/page.tsx",
        r#"
import { money, wrap } from "@src/ui/money";
export const Page = ({ items, n }: { items: number[]; n: number }) => (
  <p title={items.map((i) => money(i)).join(", ")}>{wrap(money(n))}{items.length.toLocaleString()}</p>
);
"#,
      ),
      ("src/ui/money.ts", "export function money(cents: number): string {\n  return `$${(cents / 100).toFixed(2)}`;\n}\nexport function wrap(s: string): string {\n  return `[${s}]`;\n}\n"),
    ];
    let set = set(&files, "routes/index/page.tsx#Page");
    let component = &set.components[0].1;
    let mut hoisted = Vec::new();
    component.visit(&mut |e| {
      if let Expr::Hoist { id, .. } = e {
        hoisted.push(*id);
      }
    });
    assert_eq!(hoisted, vec![2, 3], "wrap(money(n)) as one hoist, toLocaleString as another; the lambda's money(i) is none: {component:?}");
    let (_, source) = set.rewritten().pop().unwrap();
    assert!(source.contains("=> { const __sfh = __sfUseHoisted(\"routes/index/page.tsx#Page\"); return (("), "an expression body becomes a block: {source}");
    assert!(source.trim_end().ends_with(")); };"), "{source}");
    assert!(source.contains("__sfh.c(4, (__sfHtml) => <p title={items.map((i) => money(i)).join(\", \")} dangerouslySetInnerHTML={__sfHtml} />, () => (<p title={items.map((i) => money(i)).join(\", \")}>"), "the whole paragraph is static, so it is a chunk whose hit keeps the attribute: {source}");
    assert!(source.contains("{__sfh.r(2, () => (wrap(money(n))))}{__sfh.r(3, () => (items.length.toLocaleString()))}</p>))"), "{source}");
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
    assert!(plain(attrs).is_empty(), "key and handlers are dropped: {attrs:?}");
    assert!(attrs.contains(&Entry::Field(hoist::BOUND_ATTR.to_owned(), Expr::Lit(Lit::Bool(true)))), "a handler leaves its mark: {attrs:?}");
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
    let Tmpl::Expr(Expr::Hoist { id: 0, expr }) = &span[0] else { panic!("{:?}", span[0]) };
    assert!(matches!(&**expr, Expr::Apply { f, args } if matches!(**f, Expr::Lambda { .. }) && args.len() == 1), "{expr:?}");
    let Tmpl::Element { attrs, .. } = &children[3] else { panic!() };
    assert_eq!(plain(attrs).len(), 1, "onClick is dropped, disabled stays: {attrs:?}");
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
    assert_eq!(main, &vec![Tmpl::Slot("content".to_owned())]);
    let header = &lowered.iter().find(|(m, _)| m == "src/ui/Header.tsx#Header").unwrap().1;
    let Tmpl::Element { children, .. } = &header.render else { panic!() };
    assert_eq!(children[1], Tmpl::Slot("content".to_owned()), "`props.children` is the slot when props are bound whole");
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

  #[test]
  fn the_island_wrapper_and_the_island_alias_place_a_component_as_an_island() {
    let files = [
      (
        "routes/order/page.tsx",
        r#"
import { Island, island } from "@snapfire/fsr-client/react";
import { Help } from "../../src/ui/Help";
import { Chart } from "../../src/ui/Chart";
const LazyChart = island(Chart, { when: "idle" });
export default function Order({ id }: { id: number }) {
  return (
    <main>
      <Island when="visible">
        <Help id={id} />
      </Island>
      <LazyChart series={[id]} />
      <Island><Help id={0} /></Island>
    </main>
  );
}
"#,
      ),
      ("src/ui/Help.tsx", "export function Help({ id }: { id: number }) {\n  return <p>help {id}</p>;\n}\n"),
      ("src/ui/Chart.tsx", "export function Chart({ series }: { series: number[] }) {\n  return <svg>{series.length}</svg>;\n}\n"),
    ];
    let lowered = lower(&files, "routes/order/page.tsx#default").unwrap();
    let page = &lowered.iter().find(|(m, _)| m == "routes/order/page.tsx#default").unwrap().1;
    let Tmpl::Element { children, .. } = &page.render else { panic!("{:?}", page.render) };
    assert!(matches!(&children[0], Tmpl::Island { module, props, when, .. } if module == "src/ui/Help.tsx#Help" && props.len() == 1 && when.as_deref() == Some("visible")), "{:?}", children[0]);
    assert!(matches!(&children[1], Tmpl::Island { module, props, when, .. } if module == "src/ui/Chart.tsx#Chart" && matches!(&props[0], Entry::Field(name, _) if name == "series") && when.as_deref() == Some("idle")), "{:?}", children[1]);
    assert!(matches!(&children[2], Tmpl::Island { when: None, .. }), "no timing means the registry's: {:?}", children[2]);
    assert!(lowered.iter().any(|(m, _)| m == "src/ui/Help.tsx#Help") && lowered.iter().any(|(m, _)| m == "src/ui/Chart.tsx#Chart"), "an island's component is lowered like any other");
  }

  #[test]
  fn a_layout_places_a_named_slot_by_element_and_by_prop() {
    let files = [
      (
        "routes/layout.tsx",
        "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function Layout({ children, feed, title }: { children: unknown; feed: unknown; title: string }) {\n  return <div><h1>{title}</h1>{children}<aside>{feed}</aside><Slot name=\"modal\" /></div>;\n}\n",
      ),
    ];
    let mut set = ComponentSet::new(&app(&files));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
    set.lower("routes/layout.tsx#default").unwrap();
    let layout = &set.components[0].1;
    let Tmpl::Element { children, .. } = &layout.render else { panic!("{:?}", layout.render) };
    assert_eq!(children[1], Tmpl::Element { tag: "sf-s".to_owned(), attrs: Vec::new(), children: vec![Tmpl::Slot("content".to_owned())] });
    let Tmpl::Element { children: aside, .. } = &children[2] else { panic!("{:?}", children[2]) };
    assert_eq!(aside[0], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("feed"))], children: vec![Tmpl::Slot("feed".to_owned())] }, "a prop named after a slots/ directory is that slot");
    assert_eq!(children[3], Tmpl::Element { tag: "sf-s".to_owned(), attrs: vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str("modal"))], children: vec![Tmpl::Slot("modal".to_owned())] });
  }

  #[test]
  fn a_slot_outside_a_layout_is_residue() {
    let page = [("routes/a/page.tsx", "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <Slot name=\"x\" />;\n}\n")];
    let err = lower(&page, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("outside a layout"), "{err}");
  }

  #[test]
  fn a_slot_placement_carries_its_fallback_by_element_and_by_prop() {
    let files = [(
      "routes/layout.tsx",
      "import { Slot } from \"@snapfire/fsr-client/react\";\nexport default function L({ children, feed }: { children: unknown; feed: unknown }) {\n  return <div>{children}{feed ?? <p>no feed</p>}<Slot name=\"modal\"><p>closed</p></Slot></div>;\n}\n",
    )];
    let mut set = ComponentSet::new(&app(&files));
    set.layouts.push("routes/layout.tsx#default".to_owned());
    set.slots.push(("routes/layout.tsx#default".to_owned(), vec!["feed".to_owned()]));
    set.lower("routes/layout.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &set.components[0].1.render else { panic!() };
    for (child, name, text) in [(&children[1], "feed", "no feed"), (&children[2], "modal", "closed")] {
      let Tmpl::Element { tag, attrs, children: inner } = child else { panic!("{child:?}") };
      assert_eq!(tag, "sf-s");
      assert_eq!(attrs, &vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))]);
      let Tmpl::If { cond, then, r#else } = &inner[0] else { panic!("{:?}", inner[0]) };
      assert!(matches!(cond, Expr::Builtin { name: Builtin::Includes, args } if args[1] == Expr::lit_str(name)), "{cond:?}");
      assert_eq!(**then, Tmpl::Slot(name.to_owned()));
      let Some(fallback) = r#else else { panic!() };
      let Tmpl::Fragment(items) = &**fallback else { panic!("{fallback:?}") };
      let Tmpl::Element { children: p, .. } = &items[0] else { panic!("{:?}", items[0]) };
      assert_eq!(p, &vec![Tmpl::Text(text.to_owned())]);
    }
  }

  #[test]
  fn a_store_read_lowers_to_the_key_with_its_initial_value() {
    let files = [
      (
        "routes/index/page.tsx",
        "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { cartCount } from \"../../src/store\";\nexport default function P() {\n  const [items, setItems] = useStore(cartCount, 0);\n  const [name] = useStore(\"user/name\", \"guest\");\n  return <p onClick={() => setItems(items + 1)}>{name}{items}</p>;\n}\n",
      ),
      ("src/store.ts", "import { key } from \"@snapfire/fsr-client/store\";\nexport const cartCount = key<number>(\"cart/count\");\n"),
    ];
    let lowered = lower(&files, "routes/index/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    assert_eq!(
      component.body,
      vec![
        Stmt::Let { name: "items".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("cart/count".to_owned())), Box::new(Expr::Lit(Lit::Float(0.0)))) },
        Stmt::Let { name: "name".to_owned(), expr: Expr::Coalesce(Box::new(Expr::Store("user/name".to_owned())), Box::new(Expr::lit_str("guest"))) },
      ],
      "a key() through an import and a literal both lower to the key"
    );
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert!(plain(attrs).is_empty(), "the setter is a handler: {attrs:?}");
  }

  #[test]
  fn a_store_key_the_build_cannot_read_is_residue() {
    let files = [(
      "routes/index/page.tsx",
      "import { useStore } from \"@snapfire/fsr-client/react\";\nexport default function P({ id }: { id: string }) {\n  const [n] = useStore(id, 0);\n  return <p>{n}</p>;\n}\n",
    )];
    let err = lower(&files, "routes/index/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("`useStore` key"), "{err}");
  }

  #[test]
  fn use_locale_lowers_to_the_locale_read() {
    let files = [(
      "routes/help/page.tsx",
      "import { useLocale } from \"@snapfire/fsr-client/react\";\nexport default function Help() {\n  const locale = useLocale();\n  return <p lang={locale}>{locale === \"fr_FR\" ? \"Bonjour\" : \"Hello\"}</p>;\n}\n",
    )];
    let lowered = lower(&files, "routes/help/page.tsx#default").unwrap();
    let component = &lowered[0].1;
    assert_eq!(component.body, vec![Stmt::Let { name: "locale".to_owned(), expr: Expr::Locale }]);
    let Tmpl::Element { attrs, .. } = &component.render else { panic!("{:?}", component.render) };
    assert_eq!(plain(attrs), [Entry::Field("lang".to_owned(), Expr::var("locale"))]);
  }

  #[test]
  fn a_link_lowers_to_an_anchor_the_navigator_reads() {
    let page = [("routes/a/page.tsx", "import { Link } from \"@snapfire/fsr-client/react\";\nexport default function A({ id }: { id: number }) {\n  return <p><Link href={`/photo/${id}`} className=\"x\" full>full</Link><Link href=\"/photo/1\" into=\"modal\" prefetch=\"none\">quick</Link></p>;\n}\n")];
    let lowered = lower(&page, "routes/a/page.tsx#default").unwrap();
    let Tmpl::Element { children, .. } = &lowered[0].1.render else { panic!() };
    let Tmpl::Element { tag, attrs, children: text } = &children[0] else { panic!("{:?}", children[0]) };
    assert_eq!(tag, "a");
    assert!(matches!(&attrs[0], Entry::Field(name, _) if name == "href"));
    assert_eq!(attrs[1], Entry::Field("class".to_owned(), Expr::lit_str("x")));
    assert_eq!(attrs[2], Entry::Field("data-sf-full".to_owned(), Expr::Lit(Lit::Bool(true))));
    assert_eq!(text, &vec![Tmpl::Text("full".to_owned())]);
    let Tmpl::Element { attrs, .. } = &children[1] else { panic!("{:?}", children[1]) };
    assert_eq!(attrs[1], Entry::Field("data-sf-into".to_owned(), Expr::lit_str("modal")));
    assert_eq!(attrs[2], Entry::Field("data-sf-prefetch".to_owned(), Expr::lit_str("none")));
  }

  #[test]
  fn an_island_around_an_element_or_with_a_computed_timing_is_residue() {
    let element = [("routes/a/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nexport default function A() {\n  return <Island when=\"visible\"><p>x</p></Island>;\n}\n")];
    let err = lower(&element, "routes/a/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("an island must be a component"), "{err}");
    let timing = [("routes/b/page.tsx", "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Help } from \"../../src/ui/Help\";\nexport default function B({ n }: { n: number }) {\n  return <Island when={n > 0 ? \"visible\" : \"load\"}><Help /></Island>;\n}\n"), ("src/ui/Help.tsx", "export function Help() {\n  return <p>help</p>;\n}\n")];
    let err = lower(&timing, "routes/b/page.tsx#default").unwrap_err().to_string();
    assert!(err.contains("written out"), "{err}");
  }
}
