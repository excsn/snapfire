//! A callback that writes fields of the object it is handed, written as one
//! that returns the changed object; the function that calls it is written to
//! take what it returns, `fn(config)` as `config = fn(config)`.

use std::collections::BTreeMap;
use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, ArrowFunctionBody, AssignmentOperator, AssignmentTarget, BindingPattern, Declaration, Expression, ImportDeclarationSpecifier, ModuleExportName, Statement, TSType, VariableDeclarationKind};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::syntax::{argument, is_script, lossy, params, parse, Param, Parsed, SCRIPT_EXTENSIONS};
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
    let allocator = Allocator::default();
    let mut parsed: BTreeMap<String, (String, Parsed)> = BTreeMap::new();
    for path in &paths {
      let Some(text) = tree.text(path).map(str::to_owned) else { continue };
      if let Ok(p) = parse(&allocator, path, &text) {
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
        calls.visit_program(&p.program);
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
        if let Ok(p) = parse(&allocator, &path, &rewritten) {
          parsed.insert(path, (rewritten, p));
        }
      }
    }
  }
}

/// The module functions of `parsed` that call a parameter as a statement on a name.
fn targets_in(path: &str, text: &str, parsed: &Parsed) -> Vec<Target> {
  let mut out = Vec::new();
  for item in parsed.body() {
    let decl = match item {
      Statement::ExportDeclaration(export) => &export.declaration,
      other => match other.as_declaration() {
        Some(decl) => decl,
        None => continue,
      },
    };
    let (name, formals, body): (String, &js::FormalParameters, &js::FunctionBody) = match decl {
      Declaration::FunctionDeclaration(f) => match (&f.id, &f.body) {
        (Some(id), Some(body)) => (id.name.to_string(), &f.params, body),
        _ => continue,
      },
      Declaration::VariableDeclaration(var) => {
        let [declarator] = var.declarations.as_slice() else { continue };
        let (BindingPattern::BindingIdentifier(id), Some(Expression::ArrowFunctionExpression(arrow))) = (&declarator.id, &declarator.init) else { continue };
        let ArrowFunctionBody::FunctionBody(body) = &arrow.body else { continue };
        (id.name.to_string(), &arrow.params, body)
      }
      _ => continue,
    };
    for (index, param) in params(formals).into_iter().enumerate() {
      let Param::Plain(param) = param else { continue };
      let BindingPattern::BindingIdentifier(id) = &param.pattern else { continue };
      let mut uses = Uses { param: &id.name, parsed, calls: Vec::new(), other: None };
      uses.visit_function_body(body);
      if uses.calls.is_empty() {
        continue;
      }
      let blocked = uses.other.map(|span| format!("`{}`", &text[parsed.range(span)])).or_else(|| {
        uses.calls.iter().find(|(_, x)| declared_const(parsed, x)).map(|(_, x)| format!("`{x}`, a `const` it cannot assign"))
      });
      let returns = param.type_annotation.as_deref().and_then(|ann| match &ann.type_annotation {
        TSType::TSFunctionType(f) => {
          let returned = &f.return_type.type_annotation;
          let returns_void = matches!(returned, TSType::TSVoidKeyword(_));
          let first = params(&f.params).into_iter().next().and_then(|p| match p {
            Param::Plain(p) if matches!(p.pattern, BindingPattern::BindingIdentifier(_)) => p.type_annotation.as_deref(),
            _ => None,
          });
          match (returns_void, first) {
            (true, Some(first)) => Some((parsed.range(returned.span()), text[parsed.range(first.type_annotation.span())].to_owned())),
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
  impl<'a> Visit<'a> for Consts<'_> {
    fn visit_variable_declaration(&mut self, var: &js::VariableDeclaration<'a>) {
      if var.kind == VariableDeclarationKind::Const && var.declarations.iter().any(|d| matches!(&d.id, BindingPattern::BindingIdentifier(id) if id.name == self.0)) {
        self.1 = true;
      }
      walk::walk_variable_declaration(self, var);
    }
  }
  let mut found = Consts(name, false);
  found.visit_program(&parsed.program);
  found.1
}

/// The uses of a function parameter: each `param(x);` statement and the first use of another shape.
struct Uses<'a> {
  param: &'a str,
  parsed: &'a Parsed<'a>,
  calls: Vec<(Range<usize>, String)>,
  other: Option<Span>,
}

impl Uses<'_> {
  fn named(&mut self, name: &str, span: Span) {
    if name == self.param && self.other.is_none() {
      self.other = Some(span);
    }
  }
}

impl<'a> Visit<'a> for Uses<'_> {
  fn visit_expression_statement(&mut self, stmt: &js::ExpressionStatement<'a>) {
    if let Expression::CallExpression(call) = &stmt.expression {
      if let (Expression::Identifier(f), [js::Argument::Identifier(x)]) = (&call.callee, call.arguments.as_slice()) {
        if f.name == self.param {
          self.calls.push((self.parsed.range(stmt.span), x.name.to_string()));
          return;
        }
      }
    }
    walk::walk_expression_statement(self, stmt);
  }

  fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
    self.named(&id.name, id.span);
  }

  fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
    self.named(&id.name, id.span);
  }
}

/// The name `parsed`, the file at `path`, holds `target`'s function under: its own name in its own file, else the local of an import of it.
fn local_name(path: &str, parsed: &Parsed, target: &Target) -> Option<String> {
  if path == target.file {
    return Some(target.name.clone());
  }
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if !names(path, &lossy(import.source.value), &target.file) {
      continue;
    }
    for specifier in import.specifiers.iter().flatten() {
      if let ImportDeclarationSpecifier::ImportSpecifier(named) = specifier {
        let imported = match &named.imported {
          ModuleExportName::IdentifierName(i) => i.name.to_string(),
          ModuleExportName::IdentifierReference(i) => i.name.to_string(),
          ModuleExportName::StringLiteral(s) => lossy(s.value),
        };
        if imported == target.name {
          return Some(named.local.name.to_string());
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
  parsed: &'a Parsed<'a>,
  text: &'a str,
  local: &'a str,
  index: usize,
  found: Vec<Passed>,
}

impl<'a> Visit<'a> for Calls<'_> {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if matches!(&call.callee, Expression::Identifier(f) if f.name == self.local) {
      if let Some(arg) = call.arguments.get(self.index) {
        self.found.push(Passed { rewrite: returning(self.parsed, self.text, argument(arg)), line: self.parsed.line(call.span) });
      }
    }
    walk::walk_call_expression(self, call);
  }
}

/// A callback `c => { c.a = x; c.b = y; }` as `c => ({ ...c, a: x, b: y })`: the block's range and its replacement. A statement of another shape is why it cannot, as is a value that reads a field an earlier statement wrote.
fn returning(parsed: &Parsed, text: &str, callback: &Expression) -> Result<(Range<usize>, String), String> {
  let written = |span: Span| text[parsed.range(span)].to_owned();
  let Expression::ArrowFunctionExpression(arrow) = callback else { return Err(format!("`{}`", written(callback.span()))) };
  let formals = params(&arrow.params);
  let ([Param::Plain(param)], ArrowFunctionBody::FunctionBody(body)) = (formals.as_slice(), &arrow.body) else { return Err(format!("`{}`", written(arrow.span))) };
  let BindingPattern::BindingIdentifier(param) = &param.pattern else { return Err(format!("`{}`", written(arrow.span))) };
  let param = param.name.to_string();
  if let Some(directive) = body.directives.first() {
    return Err(format!("`{}`", written(directive.span)));
  }
  let mut fields: Vec<(String, String)> = Vec::new();
  for stmt in &body.statements {
    let shape = || format!("`{}`", written(stmt.span()));
    let Statement::ExpressionStatement(e) = stmt else { return Err(shape()) };
    let Expression::AssignmentExpression(assign) = &e.expression else { return Err(shape()) };
    let AssignmentTarget::StaticMemberExpression(member) = &assign.left else { return Err(shape()) };
    let Expression::Identifier(object) = &member.object else { return Err(shape()) };
    let field = &member.property;
    if assign.operator != AssignmentOperator::Assign || object.name.as_str() != param {
      return Err(shape());
    }
    let mut reads = Reads { param: &param, fields: &fields, found: false };
    reads.visit_expression(&assign.right);
    if reads.found {
      return Err(shape());
    }
    fields.retain(|(name, _)| name.as_str() != field.name.as_str());
    fields.push((field.name.to_string(), written(assign.right.span())));
  }
  let entries: Vec<String> = fields.iter().map(|(name, value)| if name == value { name.clone() } else { format!("{name}: {value}") }).collect();
  let object = if entries.is_empty() { format!("({{ ...{param} }})") } else { format!("({{ ...{param}, {} }})", entries.join(", ")) };
  Ok((parsed.range(body.span), object))
}

/// Whether an expression reads `param.field` for a field already written.
struct Reads<'r> {
  param: &'r str,
  fields: &'r [(String, String)],
  found: bool,
}

impl<'a> Visit<'a> for Reads<'_> {
  fn visit_static_member_expression(&mut self, member: &js::StaticMemberExpression<'a>) {
    if let Expression::Identifier(object) = &member.object {
      if object.name == self.param && self.fields.iter().any(|(name, _)| name.as_str() == member.property.name.as_str()) {
        self.found = true;
      }
    }
    walk::walk_static_member_expression(self, member);
  }
}
