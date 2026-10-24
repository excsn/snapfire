//! A callback that writes fields of the object it is handed, written as one
//! that returns the changed object; the function that calls it is written to
//! take what it returns, `fn(config)` as `config = fn(config)`.

use std::collections::BTreeMap;
use std::ops::Range;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::syntax::{is_script, parse, Parsed, SCRIPT_EXTENSIONS};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Callbacks)]
}

/// A module function that calls one of its parameters as a statement on a name, `fn(config);`.
struct Target {
  file: String,
  name: String,
  index: usize,
  /// Each `fn(x);` call, its range and `x`.
  calls: Vec<(Range<usize>, String)>,
  /// The `void` of the parameter's function type and the type of that function's first parameter, which it becomes.
  returns: Option<(Range<usize>, String)>,
  line: usize,
  /// Why the function cannot take what the callback returns, when it cannot.
  blocked: Option<String>,
}

/// A call of a target with the callback it passes, by file.
struct Passed {
  /// The callback's block body and what it becomes; why it cannot when it cannot.
  rewrite: Result<(Range<usize>, String), String>,
  line: usize,
}

/// Each callback a module function calls on a name, `fn(config);`, that writes fields of its parameter and nothing else is written to return `{ ...c, field: value }`; the call is written `config = fn(config)`. A function any of whose callers passes a callback of another shape is left as written with a note for each, since the two sides must change together.
struct Callbacks;

impl Pass for Callbacks {
  fn name(&self) -> &'static str {
    "callbacks"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    let mut parsed: BTreeMap<String, (String, Parsed)> = BTreeMap::new();
    for path in &paths {
      let Some(text) = tree.text(path).map(str::to_owned) else { continue };
      if let Ok(p) = parse(path, &text) {
        parsed.insert(path.clone(), (text, p));
      }
    }
    let mut targets = Vec::new();
    for (path, (text, p)) in &parsed {
      targets.extend(targets_in(path, text, p));
    }
    for target in targets {
      let mut passed: Vec<(String, Passed)> = Vec::new();
      for (path, (text, p)) in &parsed {
        let Some(local) = local_name(path, p, &target) else { continue };
        let mut calls = Calls { parsed: p, text, local: &local, index: target.index, found: Vec::new() };
        p.module.visit_with(&mut calls);
        passed.extend(calls.found.into_iter().map(|c| (path.clone(), c)));
      }
      if passed.is_empty() {
        continue;
      }
      let refused: Vec<(&String, &Passed, &String)> = passed.iter().filter_map(|(path, c)| c.rewrite.as_ref().err().map(|why| (path, c, why))).collect();
      if let Some(why) = &target.blocked {
        report.note(&target.file, target.line, format!("`{}` calls its callback on {why}, so `fsr migrate` leaves it and its callers as written", target.name));
        continue;
      }
      if !refused.is_empty() {
        for (path, c, why) in refused {
          report.note(path, c.line, format!("{why}: the callback passed to `{}` does more than write fields of its parameter, so `fsr migrate` leaves `{}` and every callback passed to it as written", target.name, target.name));
        }
        continue;
      }
      let mut edits: BTreeMap<String, Edits> = BTreeMap::new();
      for (path, c) in &passed {
        let (range, written) = c.rewrite.clone().expect("checked");
        edits.entry(path.clone()).or_default().replace(range, written);
      }
      let target_edits = edits.entry(target.file.clone()).or_default();
      for (range, x) in &target.calls {
        target_edits.replace(range.start..range.start, format!("{x} = "));
      }
      if let Some((range, ty)) = &target.returns {
        target_edits.replace(range.clone(), ty.clone());
      }
      for (path, edits) in edits {
        let text = parsed[&path].0.clone();
        let rewritten = edits.apply(&text);
        tree.insert(path.clone(), rewritten.clone());
        if let Ok(p) = parse(&path, &rewritten) {
          parsed.insert(path, (rewritten, p));
        }
      }
    }
  }
}

/// The module functions of `parsed` that call a parameter as a statement on a name.
fn targets_in(path: &str, text: &str, parsed: &Parsed) -> Vec<Target> {
  let mut out = Vec::new();
  for item in &parsed.module.body {
    let decl = match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => decl,
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => &export.decl,
      _ => continue,
    };
    let (name, params, body): (String, Vec<&js::Pat>, &js::FunctionBody) = match decl {
      js::Decl::Fn(f) => match &f.function.body {
        Some(body) => (f.ident.sym.to_string(), f.function.params.iter().map(|p| &p.pat).collect(), body),
        None => continue,
      },
      js::Decl::Var(var) => {
        let [declarator] = var.decls.as_slice() else { continue };
        let (js::Pat::Ident(id), Some(js::Expr::Arrow(arrow))) = (&declarator.name, declarator.init.as_deref()) else { continue };
        let js::ArrowFunctionBody::FunctionBody(body) = &*arrow.body else { continue };
        (id.id.sym.to_string(), arrow.params.iter().collect(), body)
      }
      _ => continue,
    };
    for (index, pat) in params.iter().enumerate() {
      let js::Pat::Ident(param) = pat else { continue };
      let mut uses = Uses { param: &param.id.sym, parsed, calls: Vec::new(), other: None };
      body.visit_with(&mut uses);
      if uses.calls.is_empty() {
        continue;
      }
      let blocked = uses.other.map(|span| format!("`{}`", &text[parsed.range(span)])).or_else(|| {
        uses.calls.iter().find(|(_, x)| declared_const(parsed, x)).map(|(_, x)| format!("`{x}`, a `const` it cannot assign"))
      });
      let returns = param.type_ann.as_deref().and_then(|ann| match &*ann.type_ann {
        js::TsType::TsFnOrConstructorType(js::TsFnOrConstructorType::TsFnType(f)) => {
          let returns_void = matches!(&*f.type_ann.type_ann, js::TsType::TsKeywordType(k) if k.kind == js::TsKeywordTypeKind::TsVoidKeyword);
          let first = f.params.first().and_then(|p| match p {
            js::TsFnParam::Ident(i) => i.type_ann.as_deref(),
            _ => None,
          });
          match (returns_void, first) {
            (true, Some(first)) => Some((parsed.range(f.type_ann.type_ann.span()), text[parsed.range(first.type_ann.span())].to_owned())),
            _ => None,
          }
        }
        _ => None,
      });
      out.push(Target { file: path.to_owned(), name: name.clone(), index, calls: uses.calls, returns, line: parsed.line(body.span), blocked });
    }
  }
  out
}

/// Whether `name` is declared with `const` anywhere in `parsed`.
fn declared_const(parsed: &Parsed, name: &str) -> bool {
  struct Consts<'n>(&'n str, bool);
  impl Visit for Consts<'_> {
    fn visit_var_decl(&mut self, var: &js::VarDecl) {
      if var.kind == js::VarDeclKind::Const && var.decls.iter().any(|d| matches!(&d.name, js::Pat::Ident(id) if *id.id.sym == *self.0)) {
        self.1 = true;
      }
      var.visit_children_with(self);
    }
  }
  let mut found = Consts(name, false);
  parsed.module.visit_with(&mut found);
  found.1
}

/// The uses of a function parameter: each `param(x);` statement and the first use of another shape.
struct Uses<'a> {
  param: &'a str,
  parsed: &'a Parsed,
  calls: Vec<(Range<usize>, String)>,
  other: Option<Span>,
}

impl Visit for Uses<'_> {
  fn visit_expr_stmt(&mut self, stmt: &js::ExprStmt) {
    if let js::Expr::Call(call) = &*stmt.expr {
      if let (js::Callee::Expr(callee), [arg]) = (&call.callee, call.args.as_slice()) {
        if let (js::Expr::Ident(f), None, js::Expr::Ident(x)) = (&**callee, arg.spread, &*arg.expr) {
          if *f.sym == *self.param {
            self.calls.push((self.parsed.range(stmt.span), x.sym.to_string()));
            return;
          }
        }
      }
    }
    stmt.visit_children_with(self);
  }

  fn visit_ident(&mut self, id: &js::Ident) {
    if *id.sym == *self.param && self.other.is_none() {
      self.other = Some(id.span);
    }
  }
}

/// The name `parsed`, the file at `path`, holds `target`'s function under: its own name in its own file, else the local of an import of it.
fn local_name(path: &str, parsed: &Parsed, target: &Target) -> Option<String> {
  if path == target.file {
    return Some(target.name.clone());
  }
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if !names(path, &import.src.value.to_string_lossy(), &target.file) {
      continue;
    }
    for specifier in &import.specifiers {
      if let js::ImportSpecifier::Named(named) = specifier {
        let imported = match &named.imported {
          Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
          Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
          None => named.local.sym.to_string(),
        };
        if imported == target.name {
          return Some(named.local.sym.to_string());
        }
      }
    }
  }
  None
}

/// Whether the relative `specifier` written in `from` names the script `file`, a `.js` written for a `.ts` or `.tsx` source and a directory's `index` included.
fn names(from: &str, specifier: &str, file: &str) -> bool {
  if !specifier.starts_with('.') {
    return false;
  }
  let mut parts: Vec<&str> = from.split('/').collect();
  parts.pop();
  for part in specifier.split('/') {
    match part {
      "." | "" => {}
      ".." => {
        parts.pop();
      }
      part => parts.push(part),
    }
  }
  let joined = parts.join("/");
  let stem = [".js", ".jsx", ".mjs", ".ts", ".tsx"].iter().find_map(|ext| joined.strip_suffix(ext)).unwrap_or(&joined);
  file == joined || SCRIPT_EXTENSIONS.iter().any(|ext| file == format!("{stem}{ext}") || file == format!("{stem}/index{ext}"))
}

/// Each call of the function by `local`, with what its callback at `index` becomes.
struct Calls<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  local: &'a str,
  index: usize,
  found: Vec<Passed>,
}

impl Visit for Calls<'_> {
  fn visit_call_expr(&mut self, call: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &call.callee {
      if matches!(&**callee, js::Expr::Ident(f) if *f.sym == *self.local) {
        if let Some(arg) = call.args.get(self.index) {
          self.found.push(Passed { rewrite: returning(self.parsed, self.text, &arg.expr), line: self.parsed.line(call.span) });
        }
      }
    }
    call.visit_children_with(self);
  }
}

/// A callback `c => { c.a = x; c.b = y; }` as `c => ({ ...c, a: x, b: y })`: the block's range and its replacement. A statement of another shape is why it cannot, as is a value that reads a field an earlier statement wrote.
fn returning(parsed: &Parsed, text: &str, callback: &js::Expr) -> Result<(Range<usize>, String), String> {
  let written = |span: Span| text[parsed.range(span)].to_owned();
  let js::Expr::Arrow(arrow) = callback else { return Err(format!("`{}`", written(callback.span()))) };
  let ([js::Pat::Ident(param)], js::ArrowFunctionBody::FunctionBody(body)) = (arrow.params.as_slice(), &*arrow.body) else { return Err(format!("`{}`", written(arrow.span))) };
  let param = param.id.sym.to_string();
  let mut fields: Vec<(String, String)> = Vec::new();
  for stmt in &body.stmts {
    let shape = || format!("`{}`", written(stmt.span()));
    let js::Stmt::Expr(e) = stmt else { return Err(shape()) };
    let js::Expr::Assign(assign) = &*e.expr else { return Err(shape()) };
    let js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) = &assign.left else { return Err(shape()) };
    let (js::Expr::Ident(object), js::MemberProp::Ident(field)) = (&*member.obj, &member.prop) else { return Err(shape()) };
    if assign.op != js::AssignOp::Assign || *object.sym != *param {
      return Err(shape());
    }
    let mut reads = Reads { param: &param, fields: &fields, found: false };
    assign.right.visit_with(&mut reads);
    if reads.found {
      return Err(shape());
    }
    fields.retain(|(name, _)| *name != *field.sym);
    fields.push((field.sym.to_string(), written(assign.right.span())));
  }
  let entries: Vec<String> = fields.iter().map(|(name, value)| if name == value { name.clone() } else { format!("{name}: {value}") }).collect();
  let object = if entries.is_empty() { format!("({{ ...{param} }})") } else { format!("({{ ...{param}, {} }})", entries.join(", ")) };
  Ok((parsed.range(body.span), object))
}

/// Whether an expression reads `param.field` for a field already written.
struct Reads<'a> {
  param: &'a str,
  fields: &'a [(String, String)],
  found: bool,
}

impl Visit for Reads<'_> {
  fn visit_member_expr(&mut self, member: &js::MemberExpr) {
    if let (js::Expr::Ident(object), js::MemberProp::Ident(field)) = (&*member.obj, &member.prop) {
      if *object.sym == *self.param && self.fields.iter().any(|(name, _)| *name == *field.sym) {
        self.found = true;
      }
    }
    member.visit_children_with(self);
  }
}
