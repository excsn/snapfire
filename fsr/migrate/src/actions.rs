//! Next's server actions, the async functions a `"use server"` module
//! exports, as FSR actions: each in the `actions.ts` of the route it belongs
//! to, its input type under `schemas/`, every caller reaching it through the
//! generated `actions`.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, Argument, BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression, ImportDeclarationSpecifier, JSXAttributeItem, JSXAttributeName, JSXAttributeValue, ModuleExportName, Statement, TSType};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;

use crate::edit::Edits;
use crate::items::{line_range, local, prune, tidy};
use crate::lift::{imports_for, route_id};
use crate::relocate::{dirname, normalize};
use crate::syntax::{import_specifiers, is_script, jsx_ident, last_property, lossy, params, parse, specifiers, Param, Parsed, SCRIPT_EXTENSIONS};
use crate::{Pass, Report, Tree};

pub(crate) struct ServerActions;

/// One exported function, rewritten as an action.
struct Converted {
  name: String,
  /// The type its one parameter is annotated with; `None` when it takes none.
  input: Option<String>,
  /// The input type's declaration when the pass wrote it, as for a `FormData`.
  declared: Option<String>,
  text: String,
  /// Whether its body calls `redirect` from `@snapfire/fsr`.
  redirects: bool,
}

impl Pass for ServerActions {
  fn name(&self) -> &'static str {
    "next-server-actions"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let modules: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in modules {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("use server") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      if uses_server(&parsed) {
        convert_module(tree, &path, &text, &parsed, report);
      } else {
        convert_inline(tree, &path, &text, &parsed, report);
      }
    }
  }
}

fn uses_server(parsed: &Parsed) -> bool {
  parsed.program.directives.iter().any(|d| d.expression.value == "use server")
}

/// The route directory a module's actions belong to: the nearest one at or
/// above it holding a page; `app/routes` for a module outside the routes.
fn route_dir(tree: &Tree, path: &str) -> String {
  let mut dir = if path.starts_with("app/routes/") { dirname(path).to_owned() } else { "app/routes".to_owned() };
  while dir != "app/routes" && !SCRIPT_EXTENSIONS.iter().any(|ext| tree.contains(&format!("{dir}/page{ext}"))) {
    dir = dirname(&dir).to_owned();
  }
  dir
}

/// `actions.<id>`, a segment that is no identifier written as a key.
fn accessor(id: &str) -> String {
  let mut out = "actions".to_owned();
  for segment in id.split('.') {
    if is_identifier(segment) {
      out.push('.');
      out.push_str(segment);
    } else {
      out.push_str(&format!("[{segment:?}]"));
    }
  }
  out
}

/// A value an inline action reads from the component around it, which its
/// form posts as a hidden field.
struct Captured {
  name: String,
  ty: &'static str,
}

/// `f` as an action named `name`. Its input is its one typed parameter
/// or the keys it reads from a `FormData` together with what it captured.
/// None with a note when the input cannot be named. `indent` is what the
/// declaration was indented by, taken off every line of the body.
#[allow(clippy::too_many_arguments)]
fn convert_function(f: &js::Function, name: &str, line: usize, indent: &str, captured: &[Captured], path: &str, text: &str, parsed: &Parsed, report: &mut Report) -> Option<Converted> {
  let (Some(body), true) = (&f.body, f.r#async) else { return None };
  let formals = params(&f.params);
  let body_range = parsed.range(body.span);
  let mut edits = Edits::default();
  let mut fields: Vec<(String, &'static str)> = Vec::new();
  let mut typed = None;
  match formals.as_slice() {
    [] => {}
    [param] => match annotated_type(param) {
      Some(t) if t == "FormData" => {
        let reads = match param.plain() {
          Some(BindingPattern::BindingIdentifier(p)) => form_reads(body, &p.name),
          _ => None,
        };
        let Some(reads) = reads else {
          report.note(path, line, format!("`{name}` takes a `FormData` it reads other than by `get(\"key\")`; an FSR action takes the form's fields as its input, typed under `schemas/`, so it was left in place"));
          return None;
        };
        for (span, key) in &reads {
          let range = parsed.range(*span);
          edits.replace(range.start - body_range.start..range.end - body_range.start, format!("input.{key}"));
          if !fields.iter().any(|(k, _)| k == key) {
            fields.push((key.clone(), "string"));
          }
        }
      }
      Some(t) if captured.is_empty() => typed = Some(t),
      Some(_) => {
        report.note(path, line, format!("`{name}` takes a typed input and reads values of the component around it; an FSR action takes one input, so it was left in place"));
        return None;
      }
      None => {
        report.note(path, line, format!("`{name}`'s parameter has no type; an FSR action's input is a type under `schemas/`, so it was left in place"));
        return None;
      }
    },
    _ => {
      report.note(path, line, format!("`{name}` takes more than one argument; an FSR action takes one input, so it was left in place"));
      return None;
    }
  }
  for c in captured {
    if fields.iter().any(|(k, _)| *k == c.name) {
      report.note(path, line, format!("`{name}` reads `{}` from both its form and the component around it, so it was left in place", c.name));
      return None;
    }
    fields.push((c.name.clone(), c.ty));
  }
  if let (Some(BindingPattern::BindingIdentifier(param)), Some(_)) = (formals.first().and_then(Param::plain), &typed) {
    edits.replace(1..1, format!("\n{indent}  const {} = input;", param.name));
  }
  if !captured.is_empty() {
    edits.replace(1..1, format!("\n{indent}  const {{ {} }} = input;", captured.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")));
  }
  if let Some(directive) = body.directives.first().filter(|d| is_use_server(d)) {
    let range = line_range(text, parsed.range(directive.span));
    edits.replace(range.start - body_range.start..range.end - body_range.start, "");
  }
  let mut revalidates = Revalidates { found: Vec::new() };
  revalidates.visit_function_body(body);
  for span in revalidates.found {
    let range = line_range(text, parsed.range(span));
    edits.replace(range.start - body_range.start..range.end - body_range.start, "");
  }
  let mut redirects = Redirects { found: Vec::new() };
  redirects.visit_function_body(body);
  for r in &redirects.found {
    if !r.statement {
      report.note(path, parsed.line(r.call), format!("`{name}` calls `redirect` inside an expression; an FSR action's `redirect` is a statement, so it was left in place"));
      return None;
    }
    let Some(to) = r.to else {
      report.note(path, parsed.line(r.call), format!("`{name}` calls `redirect` without a path, so it was left in place"));
      return None;
    };
    let rel = |range: std::ops::Range<usize>| range.start - body_range.start..range.end - body_range.start;
    if r.permanent {
      edits.replace(rel(parsed.range(r.callee)), "redirect");
    }
    if let Some(returned) = r.returned {
      edits.replace(rel(parsed.range(returned).start..parsed.range(r.call).start), "");
    }
    if let Some(kind) = r.kind {
      edits.replace(rel(parsed.range(to).end..parsed.range(kind).end), "");
    }
  }
  let (input, declared) = match typed {
    Some(t) => (Some(t), None),
    None if fields.is_empty() => (None, None),
    None => {
      let input = format!("{}Input", upper_first(name));
      let declared = format!("export interface {input} {{\n{}}}\n", fields.iter().map(|(k, t)| format!("  {k}: {t};\n")).collect::<String>());
      (Some(input), Some(declared))
    }
  };
  let ctx = match &input {
    Some(t) => format!("{{ input }}: ActionCtx<{t}>"),
    None => "_ctx: ActionCtx".to_owned(),
  };
  let body_text = dedent(&edits.apply(&text[body_range]), indent);
  Some(Converted { name: name.to_owned(), input, declared, text: format!("export const {name} = action(async ({ctx}) => {body_text});\n"), redirects: !redirects.found.is_empty() })
}

/// Each `redirect(...)` or `permanentRedirect(...)` call.
struct Redirects {
  found: Vec<Redirect>,
}

struct Redirect {
  call: Span,
  callee: Span,
  permanent: bool,
  /// The path, its first argument.
  to: Option<Span>,
  /// Next's `RedirectType`, a second argument FSR has no use for.
  kind: Option<Span>,
  /// Whether it stands as a statement, alone or returned.
  statement: bool,
  /// The `return` statement around it.
  returned: Option<Span>,
}

impl Redirects {
  fn record<'a>(&mut self, call: &js::CallExpression<'a>, statement: bool, returned: Option<Span>) -> bool {
    let Expression::Identifier(i) = &call.callee else { return false };
    if i.name != "redirect" && i.name != "permanentRedirect" {
      return false;
    }
    let (to, kind) = match call.arguments.as_slice() {
      [to] => (Some(to.span()), None),
      [to, kind] => (Some(to.span()), Some(kind.span())),
      _ => (None, None),
    };
    self.found.push(Redirect { call: call.span, callee: i.span, permanent: i.name == "permanentRedirect", to, kind, statement, returned });
    for arg in &call.arguments {
      self.visit_argument(arg);
    }
    true
  }
}

impl<'a> Visit<'a> for Redirects {
  fn visit_expression_statement(&mut self, n: &js::ExpressionStatement<'a>) {
    if !matches!(&n.expression, Expression::CallExpression(call) if self.record(call, true, None)) {
      walk::walk_expression_statement(self, n);
    }
  }

  fn visit_return_statement(&mut self, n: &js::ReturnStatement<'a>) {
    if !matches!(&n.argument, Some(Expression::CallExpression(call)) if self.record(call, true, Some(n.span))) {
      walk::walk_return_statement(self, n);
    }
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if !self.record(n, false, None) {
      walk::walk_call_expression(self, n);
    }
  }
}

/// `range` widened up over the comment lines directly above it.
fn with_leading_comments(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
  let mut start = range.start;
  while start > 0 {
    let above = text[..start - 1].rfind('\n').map_or(0, |i| i + 1);
    if !text[above..start].trim_start().starts_with("//") {
      break;
    }
    start = above;
  }
  start..range.end
}

/// The local name `parsed` imports `next/form`'s `Form` under.
fn next_form(parsed: &Parsed) -> Option<String> {
  parsed.body().iter().find_map(|item| match item {
    Statement::ImportDeclaration(i) if i.source.value == "next/form" => import_specifiers(i).iter().find_map(|s| match s {
      ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => Some(d.local.name.to_string()),
      _ => None,
    }),
    _ => None,
  })
}

fn is_use_server(directive: &js::Directive) -> bool {
  directive.expression.value == "use server"
}

/// `text` with `indent` taken off each line after the first that starts with it.
fn dedent(text: &str, indent: &str) -> String {
  if indent.is_empty() {
    return text.to_owned();
  }
  let mut lines = text.split('\n');
  let mut out = lines.next().unwrap_or_default().to_owned();
  for line in lines {
    out.push('\n');
    out.push_str(line.strip_prefix(indent).unwrap_or(line));
  }
  out
}

/// The whitespace a line begins with.
fn indent_at(text: &str, at: usize) -> &str {
  let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
  let line = &text[start..];
  &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

/// The schemas and the route's `actions.ts` for `converted`, which `path`
/// declared: every import of `path` an action still reads goes with it.
/// Answers the route id the actions are reached under.
fn write_actions(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, converted: &[Converted], report: &mut Report) -> (String, String) {
  let dir = route_dir(tree, path);
  let id = route_id(&format!("{dir}/page.tsx"));
  let actions_path = format!("{dir}/actions.ts");
  let inputs: BTreeSet<String> = converted.iter().filter_map(|c| c.input.clone()).collect();

  let mut schemas = String::new();
  for input in &inputs {
    let written = converted.iter().find(|c| c.input.as_ref() == Some(input)).and_then(|c| c.declared.clone());
    match written.or_else(|| declaration(tree, path, parsed, input)) {
      Some(declared) => {
        let file = format!("app/schemas/{}.ts", input.to_lowercase());
        tree.insert(file, declared);
        schemas.push_str(&format!("import type {{ {input} }} from \"@schemas/{}\";\n", input.to_lowercase()));
      }
      None => report.note(path, 0, format!("`{input}` is not declared in a module of the application, so `schemas/{}.ts` was not written", input.to_lowercase())),
    }
  }

  let redirects = converted.iter().any(|c| c.redirects);
  let mut needed = import_locals(parsed, &inputs);
  if redirects {
    needed.retain(|name| name != "redirect" && name != "permanentRedirect");
  }
  let imports = imports_for(text, parsed, &needed, path, &actions_path);
  let existing = if actions_path == path { String::new() } else { tree.text(&actions_path).unwrap_or_default().to_owned() };
  let fsr = if redirects { "action, redirect" } else { "action" };
  let mut actions = format!("import {{ {fsr} }} from \"@snapfire/fsr/host\";\nimport type {{ ActionCtx }} from \"@snapfire/fsr/host\";\n{schemas}{imports}\n{existing}");
  for c in converted {
    actions.push('\n');
    actions.push_str(&c.text);
  }
  let actions = prune(&actions_path, &actions, &needed);
  tree.insert(actions_path.clone(), tidy(&actions));
  (actions_path, id)
}

/// The names `parsed` imports, but for `next/cache` and the input types.
fn import_locals(parsed: &Parsed, inputs: &BTreeSet<String>) -> BTreeSet<String> {
  let mut needed: BTreeSet<String> = BTreeSet::new();
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if import.source.value == "next/cache" {
      continue;
    }
    needed.extend(import_specifiers(import).iter().map(local).filter(|l| !inputs.contains(l)));
  }
  needed
}

fn convert_module(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, report: &mut Report) {
  let mut converted = Vec::new();
  let mut kept = Edits::default();
  for item in parsed.body() {
    let Statement::ExportDeclaration(export) = item else { continue };
    let Declaration::FunctionDeclaration(f) = &export.declaration else { continue };
    let Some(id) = &f.id else { continue };
    let name = id.name.to_string();
    let Some(c) = convert_function(f, &name, parsed.line(item.span()), "", &[], path, text, parsed, report) else { continue };
    converted.push(c);
    kept.replace(line_range(text, parsed.range(item.span())), "");
  }
  if converted.is_empty() {
    return;
  }
  let (actions_path, id) = write_actions(tree, path, text, parsed, &converted, report);
  let dir = dirname(&actions_path).to_owned();
  let inputs: BTreeSet<String> = converted.iter().filter_map(|c| c.input.clone()).collect();
  let needed = import_locals(parsed, &inputs);
  let rest = prune(path, &kept.apply(text), &needed);
  let rest_allocator = Allocator::default();
  let exports_left = parse(&rest_allocator, path, &rest).is_ok_and(|p| p.body().iter().any(|item| matches!(item, Statement::ExportDeclaration(_))));
  // FSR reads every export of `actions.ts` as an action, so what could not be
  // converted moves out of the way when the module was that file.
  let leftover = match (exports_left, path == actions_path) {
    (false, _) => None,
    (true, false) => Some(path.to_owned()),
    (true, true) => Some(format!("{dir}/server-actions.ts")),
  };
  let names: BTreeSet<String> = converted.iter().map(|c| c.name.clone()).collect();
  rewrite_callers(tree, path, &names, &accessor(&id), leftover.as_deref(), report);
  match &leftover {
    Some(target) => {
      if target != path {
        report.note(target, 0, format!("the server actions this pass could not convert moved here from `{path}`, since every export of `actions.ts` is an FSR action"));
      }
      tree.insert(target.clone(), tidy(&rest));
    }
    None if path != actions_path => {
      tree.remove(path);
    }
    None => {}
  }
}

/// The component functions `module` declares at its top level.
fn component_functions<'p, 'a>(parsed: &'p Parsed<'a>) -> Vec<&'p js::Function<'a>> {
  let mut out = Vec::new();
  for item in parsed.body() {
    match item {
      Statement::ExportDefaultDeclaration(e) => {
        if let ExportDefaultDeclarationKind::FunctionDeclaration(f) = &e.declaration {
          out.push(&**f);
        }
      }
      Statement::ExportDeclaration(e) => {
        if let Declaration::FunctionDeclaration(f) = &e.declaration {
          out.push(&**f);
        }
      }
      Statement::FunctionDeclaration(f) => out.push(&**f),
      _ => {}
    }
  }
  out
}

/// Actions a component declares in its own body, `async function f() {
/// "use server"; ... }`, each moved to the route's `actions.ts` with what it
/// reads of the component as its input. Each `<form action={f}>` posts to
/// it with those values and the session's CSRF token as hidden fields.
fn convert_inline(tree: &mut Tree, path: &str, text: &str, parsed: &Parsed, report: &mut Report) {
  let mut converted = Vec::new();
  let mut edits = Edits::default();
  let mut forms: Vec<(&js::Function, String, Vec<Captured>)> = Vec::new();
  let form_component = next_form(parsed);
  for outer in component_functions(parsed) {
    let Some(body) = &outer.body else { continue };
    let mut scope: Vec<String> = Vec::new();
    for p in &outer.params.items {
      crate::items::pat_names(&p.pattern, &mut scope);
    }
    if let Some(rest) = &outer.params.rest {
      crate::items::pat_names(&rest.rest.argument, &mut scope);
    }
    let mut initial: Vec<(String, Option<&Expression>)> = Vec::new();
    for stmt in &body.statements {
      if let Statement::VariableDeclaration(v) = stmt {
        for d in &v.declarations {
          let mut names = Vec::new();
          crate::items::pat_names(&d.id, &mut names);
          let single = matches!(d.id, BindingPattern::BindingIdentifier(_));
          for n in names {
            initial.push((n.clone(), if single { d.init.as_ref() } else { None }));
            scope.push(n);
          }
        }
      }
    }
    for stmt in &body.statements {
      let Statement::FunctionDeclaration(f) = stmt else { continue };
      let Some(inner) = &f.body else { continue };
      if !inner.directives.first().is_some_and(is_use_server) {
        continue;
      }
      let Some(id) = &f.id else { continue };
      let name = id.name.to_string();
      let line = parsed.line(f.span);
      let mut reads = FreeReads { declared: Vec::new(), read: Vec::new() };
      reads.visit_function(f, ScopeFlags::Function);
      let mut captured = Vec::new();
      let mut unknown = None;
      for r in reads.read.iter().filter(|r| scope.contains(r) && !reads.declared.contains(r)) {
        if captured.iter().any(|c: &Captured| &c.name == r) {
          continue;
        }
        match captured_type(outer, initial.iter().find(|(n, _)| n == r).and_then(|(_, init)| *init), r) {
          Some(ty) => captured.push(Captured { name: r.clone(), ty }),
          None => unknown = Some(r.clone()),
        }
      }
      if let Some(r) = unknown {
        report.note(path, line, format!("`{name}` reads `{r}` from the component around it, a value whose type the pass cannot name for a form field, so it was left in place"));
        continue;
      }
      let mut uses = FormUses { name: &name, component: form_component.as_deref(), forms: Vec::new(), other: false };
      uses.visit_function_body(body);
      if uses.other {
        report.note(path, line, format!("`{name}` is used other than as a form's `action`; an FSR action is called as `actions.<id>.{name}(input)`, so it was left in place"));
        continue;
      }
      let indent = indent_at(text, parsed.range(f.span).start).to_owned();
      let Some(c) = convert_function(f, &name, line, &indent, &captured, path, text, parsed, report) else { continue };
      converted.push(c);
      edits.replace(with_leading_comments(text, line_range(text, parsed.range(stmt.span()))), "");
      forms.push((outer, name, captured));
    }
  }
  if converted.is_empty() {
    return;
  }
  let (_, id) = write_actions(tree, path, text, parsed, &converted, report);
  let mut with_token: Vec<Span> = Vec::new();
  for (outer, name, captured) in &forms {
    let Some(body) = &outer.body else { continue };
    let token = csrf_read(outer);
    let mut uses = FormUses { name, component: form_component.as_deref(), forms: Vec::new(), other: false };
    uses.visit_function_body(body);
    for (attr, open_end, renamed) in uses.forms {
      for span in renamed {
        edits.replace(parsed.range(span), "form");
      }
      edits.replace(parsed.range(attr), format!("method=\"post\" action=\"/_sf/action/{id}.{name}\""));
      let child = format!("{}  ", indent_at(text, parsed.range(attr).start));
      let mut hidden = format!("\n{child}<input type=\"hidden\" name=\"_csrf\" value={{{token} ?? \"\"}} />");
      for c in captured {
        hidden.push_str(&format!("\n{child}<input type=\"hidden\" name=\"{}\" value={{{}}} />", c.name, c.name));
      }
      let at = parsed.range(open_end).end;
      edits.replace(at..at, hidden);
    }
    if !with_token.contains(&outer.span) {
      with_token.push(outer.span);
      csrf_prop(outer, text, parsed, &mut edits);
    }
  }
  let page = prune(path, &edits.apply(text), &import_locals(parsed, &BTreeSet::new()));
  tree.insert(path.to_owned(), tidy(&page));
}

/// `number` for a value made by `parseInt`, `Number` or a numeric literal,
/// `string` for a string, a template or a field of the route's `params`;
/// None for anything else.
fn captured_type(outer: &js::Function, init: Option<&Expression>, name: &str) -> Option<&'static str> {
  match init.map(unwrap_await) {
    Some(Expression::NumericLiteral(_)) => Some("number"),
    Some(Expression::StringLiteral(_) | Expression::TemplateLiteral(_)) => Some("string"),
    Some(Expression::BooleanLiteral(_)) => Some("boolean"),
    Some(Expression::CallExpression(call)) => match &call.callee {
      Expression::Identifier(i) if ["parseInt", "parseFloat", "Number"].contains(&i.name.as_str()) => Some("number"),
      Expression::Identifier(i) if i.name == "String" => Some("string"),
      _ => None,
    },
    Some(_) => None,
    None => params_field(outer, name).then_some("string"),
  }
}

fn unwrap_await<'e, 'a>(e: &'e Expression<'a>) -> &'e Expression<'a> {
  match e {
    Expression::AwaitExpression(a) => unwrap_await(&a.argument),
    Expression::ParenthesizedExpression(p) => unwrap_await(&p.expression),
    Expression::TSAsExpression(a) => unwrap_await(&a.expression),
    other => other,
  }
}

/// Whether `name` is bound by `const { name } = await params` in `outer`.
fn params_field(outer: &js::Function, name: &str) -> bool {
  let Some(body) = &outer.body else { return false };
  body.statements.iter().any(|stmt| {
    let Statement::VariableDeclaration(v) = stmt else { return false };
    v.declarations.iter().any(|d| {
      let (BindingPattern::ObjectPattern(_), Some(init)) = (&d.id, &d.init) else { return false };
      let from_params = match unwrap_await(init) {
        Expression::Identifier(i) => i.name == "params",
        Expression::StaticMemberExpression(m) => m.property.name == "params",
        _ => false,
      };
      let mut names = Vec::new();
      crate::items::pat_names(&d.id, &mut names);
      from_params && names.iter().any(|n| n == name)
    })
  })
}

/// How `outer` reads the `csrf_token` prop: by name from a destructuring, or
/// as a field of its props object.
fn csrf_read(outer: &js::Function) -> String {
  match params(&outer.params).first().and_then(Param::plain) {
    Some(BindingPattern::BindingIdentifier(props)) => format!("{}.csrf_token", props.name),
    _ => "csrf_token".to_owned(),
  }
}

/// `csrf_token` added to what `outer` destructures from its props and to
/// their type when it has one.
fn csrf_prop(outer: &js::Function, text: &str, parsed: &Parsed, edits: &mut Edits) {
  let ann = |param: &js::FormalParameter, edits: &mut Edits| {
    if let Some(t) = &param.type_annotation {
      let at = parsed.range(t.span).end;
      edits.replace(at..at, " & { csrf_token?: string }");
    }
  };
  match params(&outer.params).first() {
    None => {
      let start = parsed.range(outer.span).start;
      if let Some(open) = text[start..].find("()") {
        let at = start + open + 1;
        edits.replace(at..at, "{ csrf_token }: { csrf_token?: string }");
      }
    }
    Some(Param::Plain(param)) => match &param.pattern {
      BindingPattern::ObjectPattern(obj) => {
        if let Some(last) = last_property(obj) {
          let at = parsed.range(last).end;
          edits.replace(at..at, ", csrf_token");
        }
        ann(param, edits);
      }
      BindingPattern::BindingIdentifier(_) => ann(param, edits),
      _ => {}
    },
    Some(Param::Other(_)) => {}
  }
}

/// The names a function reads and the names it declares itself.
struct FreeReads {
  declared: Vec<String>,
  read: Vec<String>,
}

impl<'a> Visit<'a> for FreeReads {
  fn visit_binding_pattern(&mut self, n: &BindingPattern<'a>) {
    crate::items::pat_names(n, &mut self.declared);
    walk::walk_binding_pattern(self, n);
  }

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    self.read.push(n.name.to_string());
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    self.read.push(n.name.to_string());
  }
}

/// Each `<form action={name}>` or `next/form`'s `<Form action={name}>` by the
/// attribute's span, the end of its opening tag and the tag names a `Form`
/// renames; `other` when `name` is used any other way.
struct FormUses<'a> {
  name: &'a str,
  /// The local name of `next/form`'s `Form`.
  component: Option<&'a str>,
  forms: Vec<(Span, Span, Vec<Span>)>,
  other: bool,
}

impl FormUses<'_> {
  /// The `action={name}` attribute of `opening` when it is a form's.
  fn action_attr(&self, opening: &js::JSXOpeningElement) -> Option<Span> {
    let (tag, _) = jsx_ident(&opening.name)?;
    if tag != "form" && Some(tag) != self.component {
      return None;
    }
    opening.attributes.iter().find_map(|attr| {
      let JSXAttributeItem::Attribute(a) = attr else { return None };
      let (JSXAttributeName::Identifier(n), Some(JSXAttributeValue::ExpressionContainer(c))) = (&a.name, &a.value) else { return None };
      let e = c.expression.as_expression()?;
      (n.name == "action" && matches!(e, Expression::Identifier(i) if i.name == self.name)).then_some(a.span)
    })
  }
}

impl<'a> Visit<'a> for FormUses<'_> {
  fn visit_function(&mut self, n: &js::Function<'a>, flags: ScopeFlags) {
    if !(n.is_declaration() && n.id.as_ref().is_some_and(|id| id.name == self.name)) {
      walk::walk_function(self, n, flags);
    }
  }

  fn visit_jsx_element(&mut self, n: &js::JSXElement<'a>) {
    let opening = &n.opening_element;
    if let Some(attr) = self.action_attr(opening) {
      let end = Span::new(opening.span.end - 1, opening.span.end);
      let mut renamed = Vec::new();
      if let Some((tag, tag_span)) = jsx_ident(&opening.name) {
        if tag != "form" {
          renamed.push(tag_span);
          if let Some((_, close)) = n.closing_element.as_ref().and_then(|c| jsx_ident(&c.name)) {
            renamed.push(close);
          }
        }
      }
      self.forms.push((attr, end, renamed));
      for other in &opening.attributes {
        match other {
          JSXAttributeItem::Attribute(a) if a.span == attr => {}
          other => self.visit_jsx_attribute_item(other),
        }
      }
      for child in &n.children {
        self.visit_jsx_child(child);
      }
      return;
    }
    walk::walk_jsx_element(self, n);
  }

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    if n.name == self.name {
      self.other = true;
    }
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    if n.name == self.name {
      self.other = true;
    }
  }
}

/// `T` as an interface for `schemas/`: a `type T = { ... }` or an
/// `interface T` declared in the module or in the local module it imports
/// `T` from.
fn declaration(tree: &Tree, path: &str, parsed: &Parsed, name: &str) -> Option<String> {
  let in_module = |p: &Parsed, text: &str| -> Option<String> {
    p.body().iter().find_map(|item| {
      let decl = match item {
        Statement::ExportDeclaration(e) => &e.declaration,
        other => other.as_declaration()?,
      };
      match decl {
        Declaration::TSInterfaceDeclaration(i) if i.id.name == name => Some(format!("export interface {name} {}\n", &text[p.range(i.body.span)])),
        Declaration::TSTypeAliasDeclaration(t) if t.id.name == name => match &t.type_annotation {
          TSType::TSTypeLiteral(lit) => Some(format!("export interface {name} {}\n", &text[p.range(lit.span)])),
          _ => None,
        },
        _ => None,
      }
    })
  };
  if let Some(found) = in_module(parsed, tree.text(path)?) {
    return Some(found);
  }
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if !import_specifiers(import).iter().any(|s| local(s) == name) {
      continue;
    }
    let target = resolve(tree, path, &lossy(import.source.value))?;
    let text = tree.text(&target)?;
    let allocator = Allocator::default();
    let other = parse(&allocator, &target, text).ok()?;
    return in_module(&other, text);
  }
  None
}

fn resolve(tree: &Tree, from: &str, spec: &str) -> Option<String> {
  let base = match spec.strip_prefix("@src/") {
    Some(rest) => format!("app/src/{rest}"),
    None if spec.starts_with("./") || spec.starts_with("../") => normalize(dirname(from), spec)?,
    None => return None,
  };
  std::iter::once(base.clone()).chain(SCRIPT_EXTENSIONS.iter().map(|e| format!("{base}{e}"))).find(|c| tree.contains(c))
}

/// Every module importing one of `names` from `module`: the import dropped
/// for `actions` from `@generated/client`, each call through `accessor`.
fn rewrite_callers(tree: &mut Tree, module: &str, names: &BTreeSet<String>, accessor: &str, leftover: Option<&str>, report: &mut Report) {
  let scripts: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p) && *p != module).map(str::to_owned).collect();
  for path in scripts {
    let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
    let allocator = Allocator::default();
    let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
    if !specifiers(&parsed).iter().any(|s| resolve(tree, &path, &s.value).as_deref() == Some(module)) {
      continue;
    }
    let mut edits = Edits::default();
    let mut locals: Vec<(String, String)> = Vec::new();
    for item in parsed.body() {
      let Statement::ImportDeclaration(import) = item else { continue };
      let spec = lossy(import.source.value);
      if resolve(tree, &path, &spec).as_deref() != Some(module) {
        continue;
      }
      let mut kept = Vec::new();
      for s in import_specifiers(import) {
        let ImportDeclarationSpecifier::ImportSpecifier(n) = s else {
          kept.push(s);
          continue;
        };
        let imported = match &n.imported {
          ModuleExportName::IdentifierName(i) => i.name.to_string(),
          ModuleExportName::IdentifierReference(i) => i.name.to_string(),
          ModuleExportName::StringLiteral(_) => n.local.name.to_string(),
        };
        if names.contains(&imported) {
          locals.push((n.local.name.to_string(), imported));
        } else {
          kept.push(s);
        }
      }
      let line = line_range(&text, parsed.range(import.span));
      let mut replacement = if kept.is_empty() { String::new() } else { crate::items::import_text(&text, &parsed, import, &kept) };
      if let Some(target) = leftover.filter(|t| *t != module && !replacement.is_empty()) {
        let moved = crate::relocate::relative(dirname(&path), target.trim_end_matches(".ts"));
        replacement = replacement.replace(&format!("\"{spec}\""), &format!("\"{moved}\"")).replace(&format!("'{spec}'"), &format!("'{moved}'"));
      }
      edits.replace(line, replacement);
    }
    if locals.is_empty() {
      continue;
    }
    let mut uses = Uses { locals: &locals, calls: Vec::new(), other: Vec::new() };
    uses.visit_program(&parsed.program);
    for (span, imported) in uses.calls {
      edits.replace(parsed.range(span), format!("{accessor}.{imported}"));
    }
    for (span, local) in uses.other {
      report.note(&path, parsed.line(span), format!("`{local}` is used other than called; an FSR action is called as `{accessor}.<name>(input)`, a form posts to `/_sf/action/<id>`"));
    }
    let rewritten = format!("import {{ actions }} from \"@generated/client\";\n{}", edits.apply(&text));
    tree.insert(path, tidy(&rewritten));
  }
}

/// Each `form.get("key")` in `body` by its span, when every use of `form` is
/// one with a key that is an identifier; None otherwise.
fn form_reads(body: &js::FunctionBody, form: &str) -> Option<Vec<(Span, String)>> {
  struct Reads<'f> {
    form: &'f str,
    found: Vec<(Span, String)>,
    other: bool,
  }
  impl<'a> Visit<'a> for Reads<'_> {
    fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
      if let Expression::StaticMemberExpression(m) = &n.callee {
        if let Expression::Identifier(obj) = &m.object {
          if obj.name == self.form && m.property.name == "get" {
            match n.arguments.as_slice() {
              [Argument::StringLiteral(key)] if is_identifier(&lossy(key.value)) => self.found.push((n.span, lossy(key.value))),
              _ => self.other = true,
            }
            return;
          }
        }
      }
      walk::walk_call_expression(self, n);
    }

    fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
      if n.name == self.form {
        self.other = true;
      }
    }

    fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
      if n.name == self.form {
        self.other = true;
      }
    }
  }
  let mut reads = Reads { form, found: Vec::new(), other: false };
  reads.visit_function_body(body);
  (!reads.other).then_some(reads.found)
}

fn is_identifier(s: &str) -> bool {
  s.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$') && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn upper_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn annotated_type(param: &Param) -> Option<String> {
  let Param::Plain(param) = param else { return None };
  if !matches!(param.pattern, BindingPattern::BindingIdentifier(_) | BindingPattern::ObjectPattern(_)) {
    return None;
  }
  match &param.type_annotation.as_ref()?.type_annotation {
    TSType::TSTypeReference(r) => match &r.type_name {
      js::TSTypeName::IdentifierReference(i) => Some(i.name.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// `revalidatePath(...)` and `revalidateTag(...)` statements: an FSR action
/// revalidates the route by default.
struct Revalidates {
  found: Vec<Span>,
}

impl<'a> Visit<'a> for Revalidates {
  fn visit_expression_statement(&mut self, n: &js::ExpressionStatement<'a>) {
    let expr = match &n.expression {
      Expression::AwaitExpression(a) => &a.argument,
      other => other,
    };
    if let Expression::CallExpression(call) = expr {
      if matches!(&call.callee, Expression::Identifier(i) if ["revalidatePath", "revalidateTag"].contains(&i.name.as_str())) {
        self.found.push(n.span);
        return;
      }
    }
    walk::walk_expression_statement(self, n);
  }
}

/// Each call of an imported action by the callee's span; each other use apart.
struct Uses<'a> {
  locals: &'a [(String, String)],
  calls: Vec<(Span, String)>,
  other: Vec<(Span, String)>,
}

impl<'a> Visit<'a> for Uses<'_> {
  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let Expression::Identifier(i) = &n.callee {
      if let Some((_, imported)) = self.locals.iter().find(|(local, _)| i.name == local.as_str()) {
        self.calls.push((i.span, imported.clone()));
        for arg in &n.arguments {
          self.visit_argument(arg);
        }
        return;
      }
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_import_declaration(&mut self, _: &js::ImportDeclaration<'a>) {}

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    if self.locals.iter().any(|(local, _)| n.name == local.as_str()) {
      self.other.push((n.span, n.name.to_string()));
    }
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    if self.locals.iter().any(|(local, _)| n.name == local.as_str()) {
      self.other.push((n.span, n.name.to_string()));
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::{migrate, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  #[test]
  fn a_server_action_is_an_fsr_action_its_input_a_schema_and_its_callers_reach_it_through_actions() {
    let actions = "\"use server\";\n\nimport { kv } from \"@vercel/kv\";\nimport { revalidatePath } from \"next/cache\";\nimport { Feature } from \"./types\";\n\nexport async function upvote(feature: Feature) {\n  await kv.hset(`item:${feature.id}`, feature);\n  revalidatePath(\"/\");\n}\n\nexport async function saveEmail(formData: FormData) {\n  await kv.sadd(\"emails\", formData.getAll(\"email\"));\n}\n";
    let types = "export type Feature = {\n  id: string;\n  title: string;\n};\n";
    let form = "\"use client\";\nimport { upvote } from \"./actions\";\nimport { Feature } from \"./types\";\n\nexport default function Item({ feature }: { feature: Feature }) {\n  return <button onClick={() => upvote(feature)}>up</button>;\n}\n";
    let page = "import Item from \"./item\";\n\nexport default function Page() {\n  return <Item feature={{ id: \"1\", title: \"t\" }} />;\n}\n";
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      ("app/actions.ts", actions),
      ("app/types.ts", types),
      ("app/item.tsx", form),
      ("app/page.tsx", page),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/schemas/feature.ts").unwrap(), "export interface Feature {\n  id: string;\n  title: string;\n}\n");
    assert_eq!(
      out.text("app/routes/actions.ts").unwrap(),
      "import { action } from \"@snapfire/fsr/host\";\nimport type { ActionCtx } from \"@snapfire/fsr/host\";\nimport type { Feature } from \"@schemas/feature\";\nimport { kv } from \"@vercel/kv\";\n\nexport const upvote = action(async ({ input }: ActionCtx<Feature>) => {\n  const feature = input;\n  await kv.hset(`item:${feature.id}`, feature);\n});\n"
    );
    assert!(out.text("app/routes/item.tsx").unwrap().contains("onClick={() => actions.$root.upvote(feature)}"), "{}", out.text("app/routes/item.tsx").unwrap());
    assert!(out.text("app/routes/item.tsx").unwrap().starts_with("import { actions } from \"@generated/client\";\n"));
    assert!(!out.text("app/routes/actions.ts").unwrap().contains("saveEmail"));
    let left = out.text("app/routes/server-actions.ts").expect("the action left in place moves out of actions.ts");
    assert!(left.starts_with("\"use server\";") && left.contains("export async function saveEmail"), "{left}");
    assert!(report.notes.iter().any(|n| n.message.starts_with("`saveEmail` takes a `FormData`")), "{:?}", report.notes);
  }

  #[test]
  fn an_action_reading_its_form_data_by_key_takes_those_keys_as_its_input() {
    let actions = "\"use server\";\n\nimport { db } from \"./db\";\n\nexport async function createUser(formData: FormData) {\n  const name = formData.get(\"name\") as string;\n  const email = formData.get(\"email\") as string;\n  await db.create({ name, email });\n}\n";
    let page = "export default function Page() {\n  return <p>new</p>;\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/actions.ts", actions), ("app/db.ts", "export const db = { create: async (_: object) => {} };\n"), ("app/page.tsx", page)]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/schemas/createuserinput.ts").unwrap(), "export interface CreateUserInput {\n  name: string;\n  email: string;\n}\n");
    assert_eq!(
      out.text("app/routes/actions.ts").unwrap(),
      "import { action } from \"@snapfire/fsr/host\";\nimport type { ActionCtx } from \"@snapfire/fsr/host\";\nimport type { CreateUserInput } from \"@schemas/createuserinput\";\nimport { db } from \"./db\";\n\nexport const createUser = action(async ({ input }: ActionCtx<CreateUserInput>) => {\n  const name = input.name as string;\n  const email = input.email as string;\n  await db.create({ name, email });\n});\n"
    );
    assert!(!report.notes.iter().any(|n| n.message.contains("FormData")), "{:?}", report.notes);
  }

  #[test]
  fn an_action_declared_inside_a_page_is_an_fsr_action_and_its_form_posts_what_it_captured() {
    let page = "import { db } from \"@/lib/db\";\nimport Form from \"next/form\";\nimport { redirect } from \"next/navigation\";\n\nexport default async function Post({ params }: { params: Promise<{ id: string }> }) {\n  const { id } = await params;\n  const postId = parseInt(id);\n  const post = await db.find(postId);\n\n  // Server action to delete the post\n  async function deletePost(formData: FormData) {\n    \"use server\";\n\n    const reason = formData.get(\"reason\") as string;\n    await db.remove(postId, reason);\n    redirect(\"/posts\");\n  }\n\n  return (\n    <Form action={deletePost}>\n      <input name=\"reason\" />\n      <button type=\"submit\">Delete {post.title}</button>\n    </Form>\n  );\n}\n";
    let input = tree(&[
      ("package.json", r#"{ "dependencies": { "next": "latest" } }"#),
      ("app/posts/[id]/page.tsx", page),
      ("lib/db.ts", "export const db = { find: async (_: number) => ({ title: \"t\" }), remove: async (_: number, __: string) => {} };\n"),
      ("tsconfig.json", "{ \"compilerOptions\": { \"paths\": { \"@/*\": [\"./*\"] } } }"),
    ]);
    let (out, report) = migrate(&input);
    assert_eq!(out.text("app/schemas/deletepostinput.ts").unwrap(), "export interface DeletePostInput {\n  reason: string;\n  postId: number;\n}\n");
    assert_eq!(
      out.text("app/routes/posts/[id]/actions.ts").unwrap(),
      "import { action, redirect } from \"@snapfire/fsr/host\";\nimport type { ActionCtx } from \"@snapfire/fsr/host\";\nimport type { DeletePostInput } from \"@schemas/deletepostinput\";\nimport { db } from \"@src/lib/db\";\n\nexport const deletePost = action(async ({ input }: ActionCtx<DeletePostInput>) => {\n  const { postId } = input;\n\n  const reason = input.reason as string;\n  await db.remove(postId, reason);\n  redirect(\"/posts\");\n});\n"
    );
    let page = out.text("app/routes/posts/[id]/page.tsx").unwrap();
    for expected in [
      "<form method=\"post\" action=\"/_sf/action/posts.$id.deletePost\">\n      <input type=\"hidden\" name=\"_csrf\" value={csrf_token ?? \"\"} />\n      <input type=\"hidden\" name=\"postId\" value={postId} />\n      <input name=\"reason\" />",
      "      <button type=\"submit\">Delete {post.title}</button>\n    </form>",
      ": PostsIdProps)",
    ] {
      assert!(page.contains(expected), "missing {expected} in {page}");
    }
    for gone in ["use server", "async function deletePost", "Server action", "next/form", "next/navigation", "Form", "csrf_token?: string"] {
      assert!(!page.contains(gone), "{gone} in {page}");
    }
    let notes: Vec<&str> = report.notes.iter().filter(|n| n.message.contains("deletePost")).map(|n| n.message.as_str()).collect();
    assert!(notes.is_empty(), "{notes:?}");
  }

  #[test]
  fn a_redirect_an_action_makes_as_a_statement_is_fsrs_and_any_other_is_a_note() {
    let module = "\"use server\";\nimport { permanentRedirect, redirect } from \"next/navigation\";\n\nexport async function go(formData: FormData) {\n  const id = formData.get(\"id\");\n  return permanentRedirect(`/p/${id}`);\n}\n\nexport async function maybe(formData: FormData) {\n  const to = formData.get(\"to\");\n  to ? redirect(\"/a\") : null;\n}\n";
    let input = tree(&[("package.json", r#"{ "dependencies": { "next": "latest" } }"#), ("app/page.tsx", "export default function Page() {\n  return <main />;\n}\n"), ("app/actions.ts", module)]);
    let (out, report) = migrate(&input);
    let actions = out.text("app/routes/actions.ts").unwrap();
    assert!(actions.starts_with("import { action, redirect } from \"@snapfire/fsr/host\";\n"), "{actions}");
    assert!(actions.contains("  redirect(`/p/${id}`);\n"), "{actions}");
    assert!(!actions.contains("permanentRedirect") && !actions.contains("return redirect"), "{actions}");
    let notes: Vec<&str> = report.notes.iter().map(|n| n.message.as_str()).collect();
    assert!(notes.iter().any(|n| n.contains("`maybe` calls `redirect` inside an expression")), "{notes:?}");
    assert!(!notes.iter().any(|n| n.contains("`go`")), "{notes:?}");
  }
}
