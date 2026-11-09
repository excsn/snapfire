//! What a component awaits while it renders, moved to where FSR reads data:
//! the route's loader, which runs before anything renders.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, ArrayExpressionElement, BindingPattern, Declaration, ExportDefaultDeclarationKind, Expression, ImportDeclarationSpecifier, LogicalOperator, MemberExpression, ModuleExportName, Statement, UnaryOperator};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};
use oxc_syntax::scope::ScopeFlags;

use crate::edit::Edits;
use crate::items::{describe, import_text, line_range, local, tidy, Item, Reads};
use crate::relocate::{dirname, normalize, relative};
use crate::syntax::{argument, import_specifiers, is_script, jsx_ident, lossy, params, parse, Node, Param, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(RouteLift), Box::new(ComponentLift), Box::new(crate::meta::GeneratedMeta)]
}

/// The default export of `module` when it is a function: its declaration
/// and its name.
pub(crate) fn default_function<'p, 'a>(parsed: &'p Parsed<'a>) -> Option<(&'p js::Function<'a>, Option<String>)> {
  let mut named = None;
  for item in parsed.body() {
    let Statement::ExportDefaultDeclaration(e) = item else { continue };
    match &e.declaration {
      ExportDefaultDeclarationKind::FunctionDeclaration(f) => return Some((f, f.id.as_ref().map(|i| i.name.to_string()))),
      ExportDefaultDeclarationKind::Identifier(i) => named = Some(i.name.to_string()),
      _ => {}
    }
  }
  let named = named?;
  parsed.body().iter().find_map(|item| match item {
    Statement::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name == named.as_str()) => Some((&**f, Some(named.clone()))),
    _ => None,
  })
}

pub(crate) fn route_kind(path: &str) -> Option<&'static str> {
  let file = path.strip_prefix("app/routes/")?.rsplit('/').next()?;
  if file.matches('.').count() != 1 || !is_script(file) {
    return None;
  }
  match file.split('.').next()? {
    "page" => Some("page"),
    "layout" => Some("layout"),
    _ => None,
  }
}

/// The source id FSR gives the route whose page is at `path`: its
/// directories under `app/routes/` joined with `.`, a parameter as `$name`,
/// `$root` for the top.
pub(crate) fn route_id(path: &str) -> String {
  let dir = dirname(path.strip_prefix("app/routes/").unwrap_or(path));
  let parts: Vec<String> = dir
    .split('/')
    .filter(|p| !p.is_empty())
    .map(|p| match p.strip_prefix("[...").or_else(|| p.strip_prefix('[')).and_then(|p| p.strip_suffix(']')) {
      Some(param) => format!("${param}"),
      None => p.to_owned(),
    })
    .collect();
  if parts.is_empty() { "$root".to_owned() } else { parts.join(".") }
}

/// The context type a loader at `path` takes: `Ctx<"/product/{id}">` for a
/// page's loader, whose pattern types `params`; `Ctx` for a layout's or one
/// under a catch-all.
pub(crate) fn ctx_type(path: &str) -> String {
  if !path.rsplit('/').next().is_some_and(|f| f.starts_with("page.")) {
    return "Ctx".to_owned();
  }
  let dir = dirname(path.strip_prefix("app/routes/").unwrap_or(path));
  let mut pattern = String::new();
  for part in dir.split('/').filter(|p| !p.is_empty()) {
    if part.starts_with('(') && part.ends_with(')') {
      continue;
    }
    if part.starts_with("[...") || part.starts_with("[[") {
      return "Ctx".to_owned();
    }
    pattern.push('/');
    match part.strip_prefix('[').and_then(|p| p.strip_suffix(']')) {
      Some(param) => pattern.push_str(&format!("{{{param}}}")),
      None => pattern.push_str(part),
    }
  }
  if pattern.is_empty() {
    pattern.push('/');
  }
  format!("Ctx<{pattern:?}>")
}

/// The props type `fsr build` generates for a source id, as `props_name` in
/// `cli/src/lib.rs` names it: `product.$id` is `ProductIdProps`.
pub(crate) fn props_name(id: &str) -> String {
  let mut name = String::new();
  for part in id.split(['.', '-', '_']) {
    let mut chars = part.trim_start_matches(['$', '(']).trim_end_matches(')').chars();
    if let Some(first) = chars.next() {
      name.extend(first.to_uppercase());
      name.push_str(chars.as_str());
    }
  }
  if name.starts_with(|c: char| c.is_ascii_digit()) {
    name.insert_str(0, "Route");
  }
  name + "Props"
}

/// Whether `node` holds JSX, an `await` or a `return` outside the functions
/// inside it.
#[derive(Default)]
pub(crate) struct Holds {
  pub(crate) jsx: bool,
  pub(crate) awaits: bool,
  pub(crate) returns: bool,
  depth: usize,
}

impl<'a> Visit<'a> for Holds {
  fn visit_jsx_element(&mut self, _: &js::JSXElement<'a>) {
    self.jsx = true;
  }

  fn visit_jsx_fragment(&mut self, _: &js::JSXFragment<'a>) {
    self.jsx = true;
  }

  fn visit_await_expression(&mut self, n: &js::AwaitExpression<'a>) {
    if self.depth == 0 {
      self.awaits = true;
    }
    walk::walk_await_expression(self, n);
  }

  fn visit_return_statement(&mut self, n: &js::ReturnStatement<'a>) {
    if self.depth == 0 {
      self.returns = true;
    }
    walk::walk_return_statement(self, n);
  }

  fn visit_function(&mut self, n: &js::Function<'a>, flags: ScopeFlags) {
    self.depth += 1;
    walk::walk_function(self, n, flags);
    self.depth -= 1;
  }

  fn visit_arrow_function_expression(&mut self, n: &js::ArrowFunctionExpression<'a>) {
    self.depth += 1;
    walk::walk_arrow_function_expression(self, n);
    self.depth -= 1;
  }
}

pub(crate) fn holds<'a>(node: &impl Node<'a>) -> Holds {
  let mut h = Holds::default();
  node.accept(&mut h);
  h
}

pub(crate) fn reads<'a>(node: &impl Node<'a>) -> BTreeSet<String> {
  let mut r = Reads { names: BTreeSet::new() };
  node.accept(&mut r);
  r.names
}

pub(crate) fn binds(stmt: &Statement) -> Vec<String> {
  match stmt {
    Statement::VariableDeclaration(v) => {
      let mut out = Vec::new();
      for d in &v.declarations {
        crate::items::pat_names(&d.id, &mut out);
      }
      out
    }
    Statement::FunctionDeclaration(f) => f.id.iter().map(|id| id.name.to_string()).collect(),
    _ => Vec::new(),
  }
}

/// The module-level items `needed` reaches, imports aside, followed through
/// what each of them reads.
pub(crate) fn closure(items: &[Item], needed: &mut BTreeSet<String>) -> BTreeSet<usize> {
  let mut taken = BTreeSet::new();
  loop {
    let before = taken.len();
    for (n, item) in items.iter().enumerate() {
      if !item.import && !taken.contains(&n) && item.binds.iter().any(|b| needed.contains(b)) {
        taken.insert(n);
        needed.extend(item.reads.iter().cloned());
      }
    }
    if taken.len() == before {
      return taken;
    }
  }
}

/// The imports of `parsed` binding `needed`, written for a module at `to`
/// where they were written for one at `from`.
pub(crate) fn imports_for(text: &str, parsed: &Parsed, needed: &BTreeSet<String>, from: &str, to: &str) -> String {
  let mut out = String::new();
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    let kept: Vec<&ImportDeclarationSpecifier> = import_specifiers(import).iter().filter(|s| needed.contains(&local(s))).collect();
    if kept.is_empty() {
      continue;
    }
    let mut line = import_text(text, parsed, import, &kept);
    let spec = lossy(import.source.value);
    if spec.starts_with("./") || spec.starts_with("../") {
      if let Some(target) = normalize(dirname(from), &spec) {
        let rebased = relative(dirname(to), &target);
        line = line.replace(&format!("\"{spec}\""), &format!("\"{rebased}\"")).replace(&format!("'{spec}'"), &format!("'{rebased}'"));
      }
    }
    out.push_str(&line);
  }
  out
}

/// `await new Promise((resolve) => setTimeout(resolve, n))` and its kin: a
/// delay a demo adds to make waiting visible, which changes nothing a render
/// shows.
fn is_delay(stmt: &Statement) -> bool {
  let Statement::ExpressionStatement(e) = stmt else { return false };
  let Expression::AwaitExpression(a) = &e.expression else { return false };
  let Expression::NewExpression(n) = &a.argument else { return false };
  let Expression::Identifier(callee) = &n.callee else { return false };
  if callee.name != "Promise" {
    return false;
  }
  let mut found = Calls { names: Vec::new() };
  found.visit_new_expression(n);
  found.names.iter().any(|c| c == "setTimeout")
}

struct Calls {
  names: Vec<String>,
}

impl<'a> Visit<'a> for Calls {
  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let Expression::Identifier(i) = &n.callee {
      self.names.push(i.name.to_string());
    }
    walk::walk_call_expression(self, n);
  }
}

/// `text` with every artificial delay among `stmts` taken out, each a note.
fn without_delays<'a>(text: &str, parsed: &Parsed, node: &impl Node<'a>, base: usize, edits: &mut Edits, path: &str, report: &mut Report) {
  let mut found = Delays { found: Vec::new() };
  node.accept(&mut found);
  for span in found.found {
    let range = parsed.range(span);
    report.note(path, parsed.line(span), "an artificial delay was dropped: it changes when data arrives, not what renders");
    edits.replace(shift(line_range(text, range), base), "");
  }
}

struct Delays {
  found: Vec<Span>,
}

impl<'a> Visit<'a> for Delays {
  fn visit_statement(&mut self, n: &Statement<'a>) {
    if is_delay(n) {
      self.found.push(n.span());
      return;
    }
    walk::walk_statement(self, n);
  }
}

/// The module-level items `taken`, as a loader holds them: artificial
/// delays dropped and a function that memoizes in a module-level `Map` read
/// as the call it memoizes, since a loader runs once per request and FSR
/// caches what it returns. A `Map` nothing else copied reads is left out.
pub(crate) fn copied(text: &str, parsed: &Parsed, items: &[Item], taken: &BTreeSet<usize>, path: &str, report: &mut Report) -> String {
  let mut memo_maps = BTreeSet::new();
  let mut bodies: BTreeMap<usize, String> = BTreeMap::new();
  for &n in taken {
    let item = &parsed.body()[n];
    let function = match item {
      Statement::FunctionDeclaration(f) => &**f,
      Statement::ExportDeclaration(e) => match &e.declaration {
        Declaration::FunctionDeclaration(f) => &**f,
        _ => continue,
      },
      _ => continue,
    };
    if let Some((map, call)) = memoized(text, parsed, function) {
      let body = function.body.as_ref().map(|b| parsed.range(b.span)).unwrap_or_default();
      let range = items[n].range.clone();
      let mut e = Edits::default();
      e.replace(shift(body, range.start), format!("{{\n  return {call};\n}}"));
      bodies.insert(n, e.apply(&text[range]));
      memo_maps.insert(map);
    }
  }
  let mut out = String::new();
  for &n in taken {
    let item = &items[n];
    if item.binds.len() == 1 && memo_maps.contains(&item.binds[0]) && !taken.iter().any(|&m| m != n && !bodies.contains_key(&m) && items[m].reads.contains(&item.binds[0])) {
      continue;
    }
    let copy = match bodies.get(&n) {
      Some(body) => body.clone(),
      None => {
        let mut e = Edits::default();
        without_delays(text, parsed, &parsed.body()[n], item.range.start, &mut e, path, report);
        e.apply(&text[item.range.clone()])
      }
    };
    let exported = matches!(parsed.body()[n], Statement::ExportDeclaration(_));
    out.push_str(if exported { copy.strip_prefix("export ").unwrap_or(&copy) } else { &copy });
    out.push_str("\n\n");
  }
  out
}

/// The functions `needed` names that `parsed` imports from a module of the
/// tree and that memoize in a `Map`, copied for the loader at `to` with what
/// they read in their own module: the imports those copies need and the
/// copies. Each is taken out of `needed`, so the loader does not import it.
fn inline_memoized(tree: &Tree, parsed: &Parsed, needed: &mut BTreeSet<String>, from: &str, to: &str, report: &mut Report) -> (String, String) {
  let mut imports = String::new();
  let mut decls = String::new();
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    let spec = lossy(import.source.value);
    let base = if let Some(rest) = spec.strip_prefix("@src/") {
      Some(format!("app/src/{rest}"))
    } else if spec.starts_with("./") || spec.starts_with("../") {
      normalize(dirname(from), &spec)
    } else {
      None
    };
    let Some(base) = base else { continue };
    let Some(target) = std::iter::once(base.clone()).chain(crate::syntax::SCRIPT_EXTENSIONS.iter().map(|e| format!("{base}{e}"))).find(|c| tree.contains(c)) else { continue };
    let Some(target_text) = tree.text(&target) else { continue };
    let allocator = Allocator::default();
    let Ok(target_parsed) = parse(&allocator, &target, target_text) else { continue };
    for s in import_specifiers(import) {
      let ImportDeclarationSpecifier::ImportSpecifier(named) = s else { continue };
      let local_name = named.local.name.to_string();
      if !needed.contains(&local_name) {
        continue;
      }
      let imported = match &named.imported {
        ModuleExportName::IdentifierName(i) => i.name.to_string(),
        ModuleExportName::IdentifierReference(i) => i.name.to_string(),
        ModuleExportName::StringLiteral(s) => lossy(s.value),
      };
      let memo = target_parsed.body().iter().any(|item| match item {
        Statement::ExportDeclaration(e) => match &e.declaration {
          Declaration::FunctionDeclaration(f) => f.id.as_ref().is_some_and(|id| id.name == imported.as_str()) && memoized(target_text, &target_parsed, f).is_some(),
          _ => false,
        },
        _ => false,
      });
      if !memo {
        continue;
      }
      let items: Vec<Item> = target_parsed.body().iter().map(|item| describe(item, &target_parsed)).collect();
      let mut wanted: BTreeSet<String> = [imported.clone()].into_iter().collect();
      let taken = closure(&items, &mut wanted);
      decls.push_str(&copied(target_text, &target_parsed, &items, &taken, &target, report));
      if local_name != imported {
        decls.push_str(&format!("const {local_name} = {imported};\n\n"));
      }
      imports.push_str(&imports_for(target_text, &target_parsed, &wanted, &target, to));
      needed.remove(&local_name);
    }
  }
  (imports, decls)
}

/// `if (!cache.has(k)) cache.set(k, f(k)); return cache.get(k);` with
/// `cache` a module-level `Map`: the map's name and `f(k)`.
fn memoized(text: &str, parsed: &Parsed, function: &js::Function) -> Option<(String, String)> {
  let body = function.body.as_ref()?;
  let [Statement::IfStatement(check), Statement::ReturnStatement(ret)] = body.statements.as_slice() else { return None };
  let Expression::UnaryExpression(not) = &check.test else { return None };
  if not.operator != UnaryOperator::LogicalNot {
    return None;
  }
  let (map, "has", has_args) = method_call(&not.argument)? else { return None };
  let set = match &check.consequent {
    Statement::ExpressionStatement(e) => &e.expression,
    Statement::BlockStatement(b) => match b.body.as_slice() {
      [Statement::ExpressionStatement(e)] => &e.expression,
      _ => return None,
    },
    _ => return None,
  };
  let (set_map, "set", set_args) = method_call(set)? else { return None };
  let (get_map, "get", get_args) = method_call(ret.argument.as_ref()?)? else { return None };
  let [key, value] = set_args else { return None };
  let same_key = |args: &[js::Argument]| args.len() == 1 && text[parsed.range(argument(&args[0]).span())] == text[parsed.range(argument(key).span())];
  if check.alternate.is_some() || map != set_map || map != get_map || !same_key(has_args) || !same_key(get_args) {
    return None;
  }
  Some((map.to_owned(), text[parsed.range(argument(value).span())].to_owned()))
}

/// `map.method(args)` with `map` a plain name.
fn method_call<'e, 'a>(e: &'e Expression<'a>) -> Option<(&'e str, &'e str, &'e [js::Argument<'a>])> {
  let Expression::CallExpression(call) = e else { return None };
  let Expression::StaticMemberExpression(m) = &call.callee else { return None };
  let Expression::Identifier(obj) = &m.object else { return None };
  Some((obj.name.as_str(), m.property.name.as_str(), &call.arguments))
}

fn shift(range: Range<usize>, by: usize) -> Range<usize> {
  range.start - by..range.end - by
}

/// An async `page` or `layout`: the statements before its final `return`
/// that hold no markup become the loader's `load`, `await params` read as
/// `params`, `searchParams` as `query` and `notFound()` as a `not_found`
/// failure; the component, no longer async, receives what its markup reads
/// as props.
struct RouteLift;

impl Pass for RouteLift {
  fn name(&self) -> &'static str {
    "lift-route"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let routes: Vec<(String, &'static str)> = tree.paths().filter_map(|p| route_kind(p).map(|k| (p.to_owned(), k))).collect();
    for (path, kind) in routes {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("async") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let Some((function, _)) = default_function(&parsed) else { continue };
      if !function.r#async {
        continue;
      }
      let loader_path = format!("{}/{kind}.loader.ts", dirname(&path));
      let existing = tree.text(&loader_path).unwrap_or_default().to_owned();
      if existing.contains("function load") || existing.contains("const load") {
        report.note(&path, parsed.line(function.span), "the route's loader already has a `load`, so what this component awaits was left in it");
        continue;
      }
      match lift_route(tree, &path, &text, &parsed, function, &loader_path, report) {
        Some((page, load, fails)) => {
          let mut loader = String::new();
          if fails {
            loader.push_str("import { fail } from \"@snapfire/fsr/host\";\n");
          }
          loader.push_str(&load.imports);
          loader.push('\n');
          loader.push_str(&existing);
          if !existing.is_empty() {
            loader.push('\n');
          }
          loader.push_str(&load.body);
          if !load.head.is_empty() {
            match crate::scripts::with_head(&loader, &loader_path, &load.head) {
              Some(with) => loader = with,
              None => report.note(&path, 0, "the route's `meta` is not an object literal the resource hints could join, so they were left for you"),
            }
          }
          tree.insert(loader_path, tidy(&loader));
          tree.insert(path, tidy(&page));
        }
        None => {}
      }
    }
  }
}

struct Load {
  imports: String,
  body: String,
  /// `meta` head rows for the resource hints the component made.
  head: Vec<String>,
}

/// The `linkTag` row a `preconnect(href)` or `prefetchDNS(href)` from `react-dom` is.
fn resource_hint(stmt: &Statement, text: &str, parsed: &Parsed) -> Option<(String, String)> {
  let Statement::ExpressionStatement(e) = stmt else { return None };
  let Expression::CallExpression(call) = &e.expression else { return None };
  let Expression::Identifier(name) = &call.callee else { return None };
  let rel = match name.name.as_str() {
    "preconnect" => "preconnect",
    "prefetchDNS" => "dns-prefetch",
    _ => return None,
  };
  let [href] = &call.arguments[..] else { return None };
  if matches!(href, js::Argument::SpreadElement(_)) || !imports_from(parsed, &name.name, "react-dom") {
    return None;
  }
  Some((name.name.to_string(), format!("linkTag(\"{rel}\", {})", &text[parsed.range(href.span())])))
}

/// Whether the module imports `local` from `module`.
fn imports_from(parsed: &Parsed, local: &str, module: &str) -> bool {
  parsed.body().iter().any(|item| match item {
    Statement::ImportDeclaration(i) => i.source.value == module && import_specifiers(i).iter().any(|s| matches!(s, ImportDeclarationSpecifier::ImportSpecifier(n) if n.local.name == local)),
    _ => false,
  })
}

fn lift_route(tree: &Tree, path: &str, text: &str, parsed: &Parsed, function: &js::Function, loader_path: &str, report: &mut Report) -> Option<(String, Load, bool)> {
  let body = function.body.as_ref()?;
  let line = parsed.line(function.span);
  let Some((last, before)) = body.statements.split_last() else { return None };
  if !matches!(last, Statement::ReturnStatement(_)) {
    report.note(path, line, "the async component does not end in a `return`, so what it awaits was left in it");
    return None;
  }
  let formals = params(&function.params);
  let props = match formals.first().and_then(Param::plain) {
    Some(BindingPattern::BindingIdentifier(i)) => Some(i.name.to_string()),
    _ => None,
  };
  let mut moved: Vec<&Statement> = Vec::new();
  let mut kept: Vec<&Statement> = Vec::new();
  let mut kept_binds: BTreeSet<String> = props.iter().cloned().collect();
  for stmt in before {
    let h = holds(stmt);
    if h.returns {
      report.note(path, parsed.line(stmt.span()), "the async component returns early, so what it awaits was left in it");
      return None;
    }
    let mut r = RequestReads { props: props.clone(), names: BTreeSet::new() };
    stmt.accept(&mut r);
    if h.jsx || r.names.iter().any(|n| kept_binds.contains(n)) {
      kept_binds.extend(binds(stmt));
      kept.push(stmt);
    } else {
      moved.push(stmt);
    }
  }
  if let Some(stmt) = kept.iter().find(|s| holds(**s).awaits) {
    report.note(path, parsed.line(stmt.span()), "an `await` here reads what the markup computes, so what the component awaits was left in it");
    return None;
  }
  if holds(last).awaits {
    report.note(path, parsed.line(last.span()), "the component awaits in its `return`, so what it awaits was left in it");
    return None;
  }

  let param_names = param_names(function);
  let mut later_reads: BTreeSet<String> = reads(last);
  for stmt in &kept {
    later_reads.extend(reads(*stmt));
  }
  let moved_binds: Vec<String> = moved.iter().flat_map(|s| binds(s)).collect();
  let returned: Vec<String> = moved_binds.iter().filter(|b| later_reads.contains(*b)).cloned().collect();
  let mut moved_reads: BTreeSet<String> = BTreeSet::new();
  for stmt in &moved {
    moved_reads.extend(reads(*stmt));
    let mut r = RequestReads { props: props.clone(), names: BTreeSet::new() };
    stmt.accept(&mut r);
    moved_reads.extend(r.names);
  }
  let reads_params = moved_reads.contains("params") || later_reads.contains("params");
  let reads_query = moved_reads.contains("searchParams") || later_reads.contains("searchParams");

  let items: Vec<Item> = parsed.body().iter().map(|item| describe(item, parsed)).collect();
  let mut needed = moved_reads.clone();
  let taken = closure(&items, &mut needed);

  let mut fails = false;
  let mut statements = String::new();
  let mut head = Vec::new();
  let mut left_out = BTreeSet::new();
  for stmt in &moved {
    if binds_request_params(stmt, props.as_deref()) {
      continue;
    }
    if let Some((hint, row)) = resource_hint(stmt, text, parsed) {
      left_out.insert(hint);
      head.push(row);
      continue;
    }
    let range = parsed.range(stmt.span());
    let mut edits = Edits::default();
    let mut found = ContextUses { props: props.clone(), found: Vec::new(), parsed, text };
    stmt.accept(&mut found);
    for (span, with) in found.found {
      if with.starts_with("fail(") {
        fails = true;
        left_out.insert("notFound".to_owned());
      }
      edits.replace(shift(parsed.range(span), range.start), with);
    }
    without_delays(text, parsed, *stmt, range.start, &mut edits, path, report);
    if found_redirect(stmt) {
      report.note(path, parsed.line(stmt.span()), "a `redirect()` in the loader belongs in `middleware.ts`, which answers `Response.redirect(to, 307)` before the route runs");
    }
    statements.push_str("  ");
    statements.push_str(&edits.apply(&text[range]));
    statements.push('\n');
  }
  let mut fields: Vec<String> = Vec::new();
  let mut called: Vec<(Range<usize>, String)> = Vec::new();
  for name in &returned {
    let mut uses = CallUses { name, parsed, text, calls: Vec::new(), other: false };
    for stmt in kept.iter().copied().chain(std::iter::once(last)) {
      stmt.accept(&mut uses);
    }
    if uses.other || uses.calls.is_empty() {
      fields.push(name.clone());
      continue;
    }
    let mut props: Vec<(String, String)> = Vec::new();
    for (range, call) in uses.calls {
      let prop = match props.iter().find(|(_, c)| *c == call) {
        Some((prop, _)) => prop.clone(),
        None => {
          let prop = call_prop(name, &call, &props);
          fields.push(format!("{prop}: {call}"));
          props.push((prop.clone(), call));
          prop
        }
      };
      called.push((range, prop));
    }
  }
  if later_reads.contains("params") && !fields.iter().any(|f| f == "params") {
    fields.push("params".to_owned());
  }
  if later_reads.contains("searchParams") {
    fields.push("searchParams: query".to_owned());
  }
  let mut ctx = Vec::new();
  if reads_params {
    ctx.push("params");
  }
  if reads_query {
    ctx.push("query");
  }
  let typed_ctx = !ctx.is_empty();
  let ctx = if ctx.is_empty() { String::new() } else { format!("{{ {} }}: {}", ctx.join(", "), ctx_type(loader_path)) };
  let (inlined_imports, inlined) = inline_memoized(tree, parsed, &mut needed, path, loader_path, report);
  let decls = format!("{inlined}{}", copied(text, parsed, &items, &taken, path, report));
  let body_text = format!("{decls}export async function load({ctx}) {{\n{statements}  return {{ {} }};\n}}\n", fields.join(", "));
  let ctx_import = if typed_ctx { "import type { Ctx } from \"@snapfire/fsr/host\";\n" } else { "" };
  let imports = format!("{ctx_import}{inlined_imports}{}", imports_for(text, parsed, &needed.difference(&left_out).cloned().collect(), path, loader_path));

  let mut page = Edits::default();
  for (range, prop) in called {
    page.replace(range, prop);
  }
  for stmt in &moved {
    page.replace(line_range(text, parsed.range(stmt.span())), "");
  }
  let function_text = &text[parsed.range(function.span)];
  if let Some(at) = function_text.find("async") {
    let start = parsed.range(function.span).start + at;
    page.replace(start..start + "async ".len(), "");
  } else if let Some(at) = text[..parsed.range(function.span).start].rfind("async ") {
    page.replace(at..at + "async ".len(), "");
  }
  let mut prop_names: Vec<String> = fields.iter().map(|f| f.split(':').next().unwrap_or(f).trim().to_owned()).collect();
  prop_names.extend(param_names.iter().filter(|p| !["params", "searchParams"].contains(&p.as_str())).cloned());
  let props_type = (route_kind(path) == Some("page") && (path.ends_with(".tsx") || path.ends_with(".ts"))).then(|| props_name(&route_id(path)));
  let typed = props_type.as_deref().map(|t| format!(": {t}")).unwrap_or_default();
  let new_param = if prop_names.is_empty() { String::new() } else { format!("{{ {} }}{typed}", prop_names.join(", ")) };
  match (formals.first(), &props) {
    (Some(param), Some(name)) => {
      page.replace(parsed.range(param.span()), format!("{name}{typed}"));
      if let (Some(body), false) = (&function.body, fields.is_empty()) {
        let open = parsed.range(body.span).start + 1;
        let names: Vec<String> = fields.iter().map(|f| f.split(':').next().unwrap_or(f).trim().to_owned()).collect();
        page.replace(open..open, format!("\n  const {{ {} }} = {name};", names.join(", ")));
      }
    }
    (Some(param), None) => page.replace(parsed.range(param.span()), new_param),
    (None, _) => {
      let open = text[parsed.range(function.span)].find('(').map(|i| parsed.range(function.span).start + i + 1);
      if let Some(open) = open {
        page.replace(open..open, new_param);
      }
    }
  }
  let mut page = crate::items::prune(path, &page.apply(text), &needed);
  if let Some(props_type) = &props_type {
    page.insert_str(0, &format!("import type {{ {props_type} }} from \"@generated/client\";\n"));
  }
  Some((page, Load { imports, body: body_text, head }, fails))
}

/// Where a statement calls `name` with literal arguments and whether it uses `name` any other way.
struct CallUses<'a> {
  name: &'a str,
  parsed: &'a Parsed<'a>,
  text: &'a str,
  calls: Vec<(Range<usize>, String)>,
  other: bool,
}

impl<'a> Visit<'a> for CallUses<'_> {
  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if matches!(&n.callee, Expression::Identifier(i) if i.name == self.name) {
      if n.arguments.iter().all(|a| matches!(a, js::Argument::StringLiteral(_) | js::Argument::NumericLiteral(_) | js::Argument::BooleanLiteral(_))) {
        let range = self.parsed.range(n.span);
        return self.calls.push((range.clone(), self.text[range].to_owned()));
      }
      self.other = true;
    }
    walk::walk_call_expression(self, n);
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

/// The prop a call's result is passed as: `t("Action.Close")` is `tActionClose`, numbered when two calls spell the same.
fn call_prop(name: &str, call: &str, taken: &[(String, String)]) -> String {
  let args = call.find('(').map_or("", |at| &call[at..]);
  let mut prop = name.to_owned();
  let mut upper = true;
  for c in args.chars() {
    if c.is_ascii_alphanumeric() {
      prop.push(if upper { c.to_ascii_uppercase() } else { c });
      upper = false;
    } else {
      upper = true;
    }
  }
  let mut out = prop.clone();
  let mut n = 2;
  while taken.iter().any(|(p, _)| *p == out) {
    out = format!("{prop}{n}");
    n += 1;
  }
  out
}

fn param_names(function: &js::Function) -> Vec<String> {
  let mut out = Vec::new();
  if let Some(BindingPattern::ObjectPattern(o)) = params(&function.params).first().and_then(Param::plain) {
    for p in &o.properties {
      if let (js::PropertyKey::StaticIdentifier(k), false) = (&p.key, p.computed) {
        out.push(k.name.to_string());
      }
    }
  }
  out
}

/// `const params = await params` and `const params = props.params`: a
/// binding the loader's own `params` already is.
fn binds_request_params(stmt: &Statement, props: Option<&str>) -> bool {
  let Statement::VariableDeclaration(v) = stmt else { return false };
  let [d] = v.declarations.as_slice() else { return false };
  let (BindingPattern::BindingIdentifier(name), Some(init)) = (&d.id, &d.init) else { return false };
  if name.name != "params" {
    return false;
  }
  let init = match init {
    Expression::AwaitExpression(a) => &a.argument,
    other => other,
  };
  matches!(init, Expression::Identifier(i) if i.name == "params") || request_field(props, init) == Some("params")
}

fn found_redirect(stmt: &Statement) -> bool {
  let mut c = Calls { names: Vec::new() };
  c.visit_statement(stmt);
  c.names.iter().any(|n| n == "redirect" || n == "permanentRedirect")
}

/// Next's request reads, written as FSR's: `await params` as `params`,
/// `searchParams` as `query` and `notFound()` as a failure.
pub(crate) struct ContextUses<'a> {
  /// The name the component binds its whole props to, when it does.
  pub(crate) props: Option<String>,
  pub(crate) found: Vec<(Span, String)>,
  pub(crate) parsed: &'a Parsed<'a>,
  pub(crate) text: &'a str,
}

impl ContextUses<'_> {
  /// `params` or `query` for `props.params` or `props.searchParams`.
  fn request_field(&self, e: &Expression) -> Option<&'static str> {
    request_field(self.props.as_deref(), e)
  }
}

pub(crate) fn request_field(props: Option<&str>, e: &Expression) -> Option<&'static str> {
  request_member(props, e.as_member_expression()?)
}

/// `params` or `query` for the member `props.params` or `props.searchParams`.
fn request_member(props: Option<&str>, m: &MemberExpression) -> Option<&'static str> {
  let MemberExpression::StaticMemberExpression(m) = m else { return None };
  let Expression::Identifier(obj) = &m.object else { return None };
  if Some(obj.name.as_str()) != props {
    return None;
  }
  match m.property.name.as_str() {
    "params" => Some("params"),
    "searchParams" => Some("query"),
    _ => None,
  }
}

/// The names a statement reads, `props.params` and `props.searchParams`
/// counted as the request's `params` and `searchParams` rather than as a read
/// of `props`.
pub(crate) struct RequestReads {
  pub(crate) props: Option<String>,
  pub(crate) names: BTreeSet<String>,
}

impl<'a> Visit<'a> for RequestReads {
  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    match request_member(self.props.as_deref(), n) {
      Some("params") => {
        self.names.insert("params".to_owned());
      }
      Some(_) => {
        self.names.insert("searchParams".to_owned());
      }
      None => walk::walk_member_expression(self, n),
    }
  }

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    self.names.insert(n.name.to_string());
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    self.names.insert(n.name.to_string());
  }
}

impl ContextUses<'_> {
  /// `params` or `query` for `params`, `searchParams` or either read off the props.
  fn request_value(&self, e: &Expression) -> Option<&'static str> {
    if let Expression::Identifier(i) = e {
      return match i.name.as_str() {
        "params" => Some("params"),
        "searchParams" => Some("query"),
        _ => None,
      };
    }
    self.request_field(e)
  }

  fn searched(&mut self, name: &str, span: Span) {
    if name == "searchParams" {
      self.found.push((span, "query".to_owned()));
    }
  }
}

impl<'a> Visit<'a> for ContextUses<'_> {
  fn visit_await_expression(&mut self, n: &js::AwaitExpression<'a>) {
    if let Some(field) = self.request_field(&n.argument) {
      return self.found.push((n.span, field.to_owned()));
    }
    if let Expression::Identifier(i) = &n.argument {
      match i.name.as_str() {
        "params" => return self.found.push((n.span, "params".to_owned())),
        "searchParams" => return self.found.push((n.span, "query".to_owned())),
        _ => {}
      }
    }
    walk::walk_await_expression(self, n);
  }

  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    if let Some(field) = request_member(self.props.as_deref(), n) {
      return self.found.push((n.span(), field.to_owned()));
    }
    walk::walk_member_expression(self, n);
  }

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    self.searched(&n.name, n.span);
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    self.searched(&n.name, n.span);
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let (Expression::StaticMemberExpression(m), [arg]) = (&n.callee, &n.arguments[..]) {
      if let (false, Some(field)) = (matches!(arg, js::Argument::SpreadElement(_)), self.request_value(&m.object)) {
        if m.property.name == "then" {
          let f = &self.text[self.parsed.range(arg.span())];
          let f = if matches!(arg, js::Argument::Identifier(_)) || arg.is_member_expression() { f.to_owned() } else { format!("({f})") };
          return self.found.push((n.span, format!("{f}({field})")));
        }
      }
    }
    if let Expression::Identifier(i) = &n.callee {
      if i.name == "notFound" && n.arguments.is_empty() {
        return self.found.push((n.span, "fail(\"not_found\", \"not found\")".to_owned()));
      }
    }
    walk::walk_call_expression(self, n);
  }
}

/// A component outside the route modules that awaits data while it
/// renders: each `const x = await f(...)` or `const x = use(f(...))` whose
/// arguments read only what the module binds at its top moves to the loader
/// of every page that places the component, which seeds the result in the
/// store; the component reads it back with `useStore`.
struct ComponentLift;

/// One read a component makes that the lift moves.
struct Read {
  name: String,
  expr: String,
  stmt: Range<usize>,
}

impl Pass for ComponentLift {
  fn name(&self) -> &'static str {
    "lift-component"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let modules: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p) && route_kind(p).is_none() && !p.contains(".loader.")).map(str::to_owned).collect();
    let graph = imports_graph(tree);
    let mut keys: Vec<(String, String)> = Vec::new();
    for path in modules {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("await") && !text.contains("use(") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let items: Vec<Item> = parsed.body().iter().map(|item| describe(item, &parsed)).collect();
      let module_names: BTreeSet<String> = items.iter().flat_map(|i| i.binds.iter().cloned()).collect();
      let mut edits = Edits::default();
      let mut lifted: Vec<(String, Vec<Read>, &js::Function)> = Vec::new();
      for item in parsed.body() {
        let Some((name, function)) = component(item) else { continue };
        let Some(reads) = component_reads(&text, &parsed, function, &module_names, &path, report) else { continue };
        if reads.is_empty() {
          continue;
        }
        if function.r#async {
          let span = parsed.range(item.span());
          if let Some(at) = text[span.clone()].find("async ") {
            edits.replace(span.start + at..span.start + at + "async ".len(), "");
          }
        }
        lifted.push((name, reads, function));
      }
      if lifted.is_empty() {
        continue;
      }
      let pages: Vec<String> = pages_reaching(&graph, &path);
      if pages.is_empty() {
        report.note(&path, 0, "a component awaits data but no page places it, so it was left as it is");
        continue;
      }
      for (component, _, _) in &lifted {
        if !placed_unconditionally(tree, component) {
          report.note(&path, 0, format!("`{component}` renders only under a condition, so its data now loads with the page before any render reads it"));
        }
      }
      let mut needed: BTreeSet<String> = BTreeSet::new();
      let mut loads = String::new();
      let mut seeds = Vec::new();
      let mut reading: BTreeMap<(usize, usize), Vec<String>> = BTreeMap::new();
      let mut fields: Vec<String> = Vec::new();
      let mut file_keys: Vec<(String, String)> = Vec::new();
      for (component, reads, function) in &lifted {
        for read in reads {
          let key_id = format!("{}/{}", component.to_lowercase(), read.name);
          let key_name = format!("{}{}", lower_first(component), upper_first(&read.name));
          let wrapped_allocator = Allocator::default();
          let wrapped = parse(&wrapped_allocator, "x.ts", &format!("({})", read.expr)).ok();
          if let Some(w) = wrapped {
            needed.extend(reads_of(&w));
          }
          loads.push_str(&format!("  const {key_name} = await {};\n", read.expr));
          let mut uses = CallUses { name: &read.name, parsed: &parsed, text: &text, calls: Vec::new(), other: false };
          for stmt in function.body.iter().flat_map(|b| &b.statements).filter(|s| parsed.range(s.span()) != read.stmt) {
            stmt.accept(&mut uses);
          }
          if uses.other || uses.calls.is_empty() {
            fields.push(key_name.clone());
            seeds.push(format!("\"{key_id}\": data.{key_name}"));
            reading.entry((read.stmt.start, read.stmt.end)).or_default().push(format!("const [{}] = useStore({key_name}, null);", read.name));
            file_keys.push((key_name, key_id));
            continue;
          }
          let mut props: Vec<(String, String)> = Vec::new();
          for (range, call) in uses.calls {
            if let Some((prop, _)) = props.iter().find(|(_, c)| *c == call) {
              edits.replace(range, prop.clone());
              continue;
            }
            let prop = call_prop(&read.name, &call, &props);
            let called_name = format!("{}{}", lower_first(component), upper_first(&prop));
            let called_id = format!("{}/{prop}", component.to_lowercase());
            loads.push_str(&format!("  const {called_name} = {key_name}{};\n", &call[read.name.len()..]));
            fields.push(called_name.clone());
            seeds.push(format!("\"{called_id}\": data.{called_name}"));
            reading.entry((read.stmt.start, read.stmt.end)).or_default().push(format!("const [{prop}] = useStore({called_name}, null);"));
            file_keys.push((called_name, called_id));
            edits.replace(range, prop.clone());
            props.push((prop, call));
          }
        }
      }
      let key_names: Vec<String> = fields.clone();
      for ((start, end), lines) in reading {
        let indent = Some(&text[text[..start].rfind('\n').map_or(0, |n| n + 1)..start]).filter(|i| i.chars().all(char::is_whitespace)).unwrap_or(" ");
        edits.replace(start..end, lines.join(&format!("\n{indent}")));
      }
      let taken = closure(&items, &mut needed);
      let own = copied(&text, &parsed, &items, &taken, &path, report);
      let mut loaders: Vec<(String, String)> = Vec::new();
      let mut refused = None;
      for page in &pages {
        let loader_path = format!("{}/page.loader.ts", dirname(page));
        let existing = tree.text(&loader_path).unwrap_or_default().to_owned();
        let mut needed_here = needed.clone();
        let (inlined_imports, inlined) = inline_memoized(tree, &parsed, &mut needed_here, &path, &loader_path, report);
        let decls = format!("{inlined}{own}");
        let imports: String = format!("{inlined_imports}{}", imports_for(&text, &parsed, &needed_here, &path, &loader_path)).lines().filter(|l| !existing.lines().any(|e| e == *l)).map(|l| format!("{l}\n")).collect();
        let store = format!("export const store = ({{ data }}) => ({{ {} }});\n", seeds.join(", "));
        let has_load = existing.contains("function load") || existing.contains("const load");
        let loader = if has_load || existing.contains("export const store") {
          match merged_load(&existing, &loader_path, &loads, &fields) {
            Some(merged) if !existing.contains("export const store") => format!("{imports}\n{decls}{merged}\n{store}"),
            _ => {
              refused = Some(loader_path);
              break;
            }
          }
        } else {
          format!("{imports}\n{existing}\n{decls}export async function load() {{\n{loads}  return {{ {} }};\n}}\n\n{store}", fields.join(", "))
        };
        loaders.push((loader_path, loader));
      }
      if let Some(loader_path) = refused {
        report.note(&path, 0, format!("`{loader_path}` has a `store` or a `load` that does not end in returning an object, so the data this component awaits was left in it"));
        continue;
      }
      for (loader_path, loader) in loaders {
        tree.insert(loader_path, tidy(&loader));
      }
      keys.extend(file_keys);
      let header = format!("import {{ useStore }} from \"@snapfire/fsr/client/react\";\nimport {{ {} }} from \"@src/store\";\n", key_names.join(", "));
      let mut removable = needed.clone();
      removable.insert("use".to_owned());
      let body = crate::items::prune(&path, &edits.apply(&text), &removable);
      tree.insert(path.clone(), tidy(&format!("{header}{body}")));
    }
    if keys.is_empty() {
      return;
    }
    let mut store = tree.text("app/src/store.ts").unwrap_or_default().to_owned();
    if !store.contains("@snapfire/fsr/client/store") {
      store.insert_str(0, "import { key } from \"@snapfire/fsr/client/store\";\n\n");
    }
    for (name, id) in keys {
      store.push_str(&format!("export const {name} = key<any>(\"{id}\");\n"));
    }
    tree.insert("app/src/store.ts", store);
  }
}

/// `existing` with `loads` run before the final `return { ... }` of its `load` and `fields` joined to what it returns; `None` for any other `load`.
fn merged_load(existing: &str, path: &str, loads: &str, fields: &[String]) -> Option<String> {
  let allocator = Allocator::default();
  let parsed = parse(&allocator, path, existing).ok()?;
  let function = parsed.body().iter().find_map(|item| match item {
    Statement::ExportDeclaration(e) => match &e.declaration {
      Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name == "load") => Some(f),
      _ => None,
    },
    _ => None,
  })?;
  let Statement::ReturnStatement(ret) = function.body.as_ref()?.statements.last()? else { return None };
  let Expression::ObjectExpression(object) = ret.argument.as_ref()? else { return None };
  let at = parsed.range(ret.span).start;
  let mut edits = Edits::default();
  edits.replace(at..at, format!("{}\n  ", loads.trim_end().trim_start()));
  match object.properties.last() {
    Some(last) => {
      let end = parsed.range(last.span()).end;
      edits.replace(end..end, format!(", {}", fields.join(", ")));
    }
    None => edits.replace(parsed.range(object.span), format!("{{ {} }}", fields.join(", "))),
  }
  Some(edits.apply(existing))
}

fn reads_of(parsed: &Parsed) -> BTreeSet<String> {
  reads(&parsed.program)
}

fn lower_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn upper_first(s: &str) -> String {
  let mut c = s.chars();
  c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A top-level function whose name starts with a capital: a component.
fn component<'p, 'a>(item: &'p Statement<'a>) -> Option<(String, &'p js::Function<'a>)> {
  let function = match item {
    Statement::FunctionDeclaration(f) => &**f,
    Statement::ExportDeclaration(e) => match &e.declaration {
      Declaration::FunctionDeclaration(f) => &**f,
      _ => return None,
    },
    Statement::ExportDefaultDeclaration(e) => match &e.declaration {
      ExportDefaultDeclarationKind::FunctionDeclaration(f) => &**f,
      _ => return None,
    },
    _ => return None,
  };
  let ident = function.id.as_ref()?;
  ident.name.chars().next().filter(|c| c.is_uppercase())?;
  Some((ident.name.to_string(), function))
}

/// The reads a component makes that the lift can move: `None` when it
/// awaits something the lift cannot, so the component is left whole.
fn component_reads(text: &str, parsed: &Parsed, function: &js::Function, module_names: &BTreeSet<String>, path: &str, report: &mut Report) -> Option<Vec<Read>> {
  let body = function.body.as_ref()?;
  let mut out = Vec::new();
  for stmt in &body.statements {
    let Statement::VariableDeclaration(var) = stmt else {
      if function.r#async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits in a statement the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let [decl] = var.declarations.as_slice() else { continue };
    if let Some(pairs) = awaited_all(&decl.id, decl.init.as_ref()) {
      for (name, call) in pairs {
        let free = reads(call);
        if let Some(local) = free.iter().find(|n| !module_names.contains(*n) && !GLOBALS.contains(&n.as_str())) {
          report.note(path, parsed.line(stmt.span()), format!("the read depends on `{local}`, which is not known before the component renders, so it stays in the browser"));
          return None;
        }
        out.push(Read { name, expr: text[parsed.range(call.span())].to_owned(), stmt: parsed.range(stmt.span()) });
      }
      continue;
    }
    let (BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, &decl.init) else {
      if function.r#async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits into a pattern the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let call = match init {
      Expression::AwaitExpression(a) => Some(&a.argument),
      Expression::CallExpression(c) => match &c.callee {
        Expression::Identifier(i) if i.name == "use" => c.arguments.first().map(argument),
        _ => None,
      },
      _ => None,
    };
    let Some(call) = call else {
      if function.r#async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits inside an expression the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let free = reads(call);
    if let Some(local) = free.iter().find(|n| !module_names.contains(*n) && !GLOBALS.contains(&n.as_str())) {
      report.note(path, parsed.line(stmt.span()), format!("the read depends on `{local}`, which is not known before the component renders, so it stays in the browser"));
      if function.r#async {
        return None;
      }
      continue;
    }
    out.push(Read { name: name.name.to_string(), expr: text[parsed.range(call.span())].to_owned(), stmt: parsed.range(stmt.span()) });
  }
  Some(out)
}

/// Each name of `const [a, b] = await Promise.all([f(), g()])` with the read it binds.
fn awaited_all<'e, 'a>(pattern: &BindingPattern, init: Option<&'e Expression<'a>>) -> Option<Vec<(String, &'e Expression<'a>)>> {
  let (BindingPattern::ArrayPattern(names), Some(Expression::AwaitExpression(awaited))) = (pattern, init) else { return None };
  let Expression::CallExpression(all) = &awaited.argument else { return None };
  let Expression::StaticMemberExpression(m) = &all.callee else { return None };
  let Expression::Identifier(promise) = &m.object else { return None };
  if promise.name != "Promise" || m.property.name != "all" {
    return None;
  }
  let [arg] = &all.arguments[..] else { return None };
  let js::Argument::ArrayExpression(reads) = arg else { return None };
  let mut out = Vec::new();
  for (at, name) in names.elements.iter().enumerate() {
    let Some(name) = name else { continue };
    let BindingPattern::BindingIdentifier(name) = name else { return None };
    let read = match reads.elements.get(at)? {
      ArrayExpressionElement::SpreadElement(_) | ArrayExpressionElement::Elision(_) => return None,
      read => read.to_expression(),
    };
    out.push((name.name.to_string(), read));
  }
  Some(out)
}

const GLOBALS: &[&str] = &["undefined", "String", "Number", "Math", "JSON", "Date", "Boolean", "Array", "Object", "Promise"];

/// Whether some module places `<component>` outside every `&&`, `||`,
/// ternary and `if`, so that the render reaching it is the first one.
fn placed_unconditionally(tree: &Tree, component: &str) -> bool {
  for path in tree.paths().filter(|p| p.starts_with("app/") && is_script(p)) {
    let Some(text) = tree.text(path) else { continue };
    if !text.contains(component) {
      continue;
    }
    let allocator = Allocator::default();
    let Ok(parsed) = parse(&allocator, path, text) else { continue };
    let mut placements = Placements { name: component, depth: 0, unconditional: false };
    placements.visit_program(&parsed.program);
    if placements.unconditional {
      return true;
    }
  }
  false
}

struct Placements<'a> {
  name: &'a str,
  depth: usize,
  unconditional: bool,
}

impl<'a> Visit<'a> for Placements<'_> {
  fn visit_jsx_opening_element(&mut self, n: &js::JSXOpeningElement<'a>) {
    if let Some((name, _)) = jsx_ident(&n.name) {
      if name == self.name && self.depth == 0 {
        self.unconditional = true;
      }
    }
    walk::walk_jsx_opening_element(self, n);
  }

  fn visit_conditional_expression(&mut self, n: &js::ConditionalExpression<'a>) {
    self.visit_expression(&n.test);
    self.depth += 1;
    self.visit_expression(&n.consequent);
    self.visit_expression(&n.alternate);
    self.depth -= 1;
  }

  fn visit_logical_expression(&mut self, n: &js::LogicalExpression<'a>) {
    self.visit_expression(&n.left);
    let conditional = matches!(n.operator, LogicalOperator::And | LogicalOperator::Or | LogicalOperator::Coalesce);
    self.depth += conditional as usize;
    self.visit_expression(&n.right);
    self.depth -= conditional as usize;
  }

  fn visit_if_statement(&mut self, n: &js::IfStatement<'a>) {
    self.visit_expression(&n.test);
    self.depth += 1;
    self.visit_statement(&n.consequent);
    if let Some(alt) = &n.alternate {
      self.visit_statement(alt);
    }
    self.depth -= 1;
  }
}

/// Each script's imports resolved to the scripts they name, relative
/// specifiers and `@src/` alike.
fn imports_graph(tree: &Tree) -> BTreeMap<String, Vec<String>> {
  let mut graph = BTreeMap::new();
  for path in tree.paths().filter(|p| p.starts_with("app/") && is_script(p)) {
    let Some(text) = tree.text(path) else { continue };
    let allocator = Allocator::default();
    let Ok(parsed) = parse(&allocator, path, text) else { continue };
    let mut targets = Vec::new();
    for spec in crate::syntax::specifiers(&parsed) {
      let base = if let Some(rest) = spec.value.strip_prefix("@src/") {
        Some(format!("app/src/{rest}"))
      } else if spec.value.starts_with("./") || spec.value.starts_with("../") {
        normalize(dirname(path), &spec.value)
      } else {
        None
      };
      let Some(base) = base else { continue };
      let found = std::iter::once(base.clone())
        .chain(crate::syntax::SCRIPT_EXTENSIONS.iter().map(|e| format!("{base}{e}")))
        .chain(crate::syntax::SCRIPT_EXTENSIONS.iter().map(|e| format!("{base}/index{e}")))
        .find(|c| tree.contains(c));
      if let Some(found) = found {
        targets.push(found);
      }
    }
    graph.insert(path.to_owned(), targets);
  }
  graph
}

/// The pages whose imports reach `module`.
fn pages_reaching(graph: &BTreeMap<String, Vec<String>>, module: &str) -> Vec<String> {
  let mut out = Vec::new();
  for page in graph.keys().filter(|p| route_kind(p) == Some("page")) {
    let mut seen = BTreeSet::new();
    let mut stack = vec![page.clone()];
    while let Some(at) = stack.pop() {
      if !seen.insert(at.clone()) {
        continue;
      }
      if at == module {
        out.push(page.clone());
        break;
      }
      stack.extend(graph.get(&at).into_iter().flatten().cloned());
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use crate::{lift, Tree};

  fn tree(files: &[(&str, &str)]) -> Tree {
    let mut t = Tree::new();
    for (p, s) in files {
      t.insert(*p, *s);
    }
    t
  }

  #[test]
  fn a_page_is_typed_with_the_props_the_build_names_for_its_route() {
    use super::{props_name, route_id};
    let named = |path: &str| props_name(&route_id(path));
    assert_eq!(named("app/routes/page.tsx"), "RootProps");
    assert_eq!(named("app/routes/cart/page.tsx"), "CartProps");
    assert_eq!(named("app/routes/product/[id]/page.tsx"), "ProductIdProps");
    assert_eq!(named("app/routes/admin/users/page.tsx"), "AdminUsersProps");
    assert_eq!(named("app/routes/(shop)/cart/page.tsx"), "ShopCartProps");
    assert_eq!(named("app/routes/030-intro/page.tsx"), "Route030IntroProps");
    assert_eq!(named("app/routes/docs/[...rest]/page.tsx"), "DocsRestProps");
  }

  #[test]
  fn an_async_page_awaits_in_its_loader_and_receives_the_results() {
    let page = "import prisma from \"../src/lib/prisma\";\nimport { Product } from \"../src/components/Product\";\n\nexport default async function Home({ searchParams }) {\n  const { q } = await searchParams;\n  const products = await prisma.product.findMany({ where: { q } });\n  if (!products) notFound();\n  return (\n    <main>\n      {products.map((product) => (\n        <Product key={product.id} product={product} />\n      ))}\n    </main>\n  );\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/lib/prisma.ts", "export default {};\n"), ("app/src/components/Product.tsx", "export function Product() { return null; }\n")]);
    let (out, report) = lift(&input);
    assert_eq!(
      out.text("app/routes/page.loader.ts").unwrap(),
      "import { fail } from \"@snapfire/fsr/host\";\nimport type { Ctx } from \"@snapfire/fsr/host\";\nimport prisma from \"../src/lib/prisma\";\n\nexport async function load({ query }: Ctx<\"/\">) {\n  const { q } = query;\n  const products = await prisma.product.findMany({ where: { q } });\n  if (!products) fail(\"not_found\", \"not found\");\n  return { products };\n}\n"
    );
    assert_eq!(
      out.text("app/routes/page.tsx").unwrap(),
      "import type { RootProps } from \"@generated/client\";\nimport { Product } from \"../src/components/Product\";\n\nexport default function Home({ products }: RootProps) {\n  return (\n    <main>\n      {products.map((product) => (\n        <Product key={product.id} product={product} />\n      ))}\n    </main>\n  );\n}\n"
    );
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn then_on_the_request_params_calls_its_function_with_them() {
    let page = "import { clean } from \"../src/clean\";\n\nexport default async function Home({ params, searchParams }) {\n  const [q, slug] = await Promise.all([searchParams.then(clean), params.then((p) => p.slug)]);\n  return <p>{q.term}{slug}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/clean.ts", "export const clean = (q) => q;\n")]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("await Promise.all([clean(query), ((p) => p.slug)(params)])"), "{loader}");
    assert!(loader.contains("load({ params, query }"), "{loader}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_not_found_the_lift_rewrites_is_not_imported_by_the_loader() {
    let page = "import { notFound } from \"next/navigation\";\nimport { getPage } from \"../src/cms\";\n\nexport default async function Home() {\n  const page = await getPage();\n  if (page === null) notFound();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/cms.ts", "export const getPage = async () => ({ title: \"t\" });\n")]);
    let (out, _) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("fail(\"not_found\", \"not found\")") && !loader.contains("notFound"), "{loader}");
    assert!(!out.text("app/routes/page.tsx").unwrap().contains("notFound"));
  }

  #[test]
  fn a_function_the_page_only_calls_with_literals_is_called_in_the_loader() {
    let page = "import { getTranslations } from \"../src/i18n\";\nimport Blocks from \"../src/Blocks\";\n\nexport default async function Home() {\n  const [t, tPlay] = await Promise.all([getTranslations(\"Common\"), getTranslations(\"Landing\")]);\n  return <Blocks labels={{ play: tPlay(\"Label.Play\"), close: t(\"Action.Close\") }} title={t(\"Action.Close\")} />;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/i18n.ts", "export const getTranslations = async (ns) => (k) => k;\n"), ("app/src/Blocks.tsx", "export default function Blocks() { return null; }\n")]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("return { tActionClose: t(\"Action.Close\"), tPlayLabelPlay: tPlay(\"Label.Play\") };"), "{loader}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(page.contains("function Home({ tActionClose, tPlayLabelPlay }: RootProps)"), "{page}");
    assert!(page.contains("labels={{ play: tPlayLabelPlay, close: tActionClose }} title={tActionClose}"), "{page}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_function_the_page_also_passes_whole_is_returned_as_it_is() {
    let page = "import { getTranslations } from \"../src/i18n\";\nimport Blocks from \"../src/Blocks\";\n\nexport default async function Home() {\n  const t = await getTranslations(\"Common\");\n  return <Blocks t={t} title={t(\"Title\")} />;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/i18n.ts", "export const getTranslations = async (ns) => (k) => k;\n"), ("app/src/Blocks.tsx", "export default function Blocks() { return null; }\n")]);
    let (out, _) = lift(&input);
    assert!(out.text("app/routes/page.loader.ts").unwrap().contains("return { t };"));
  }

  #[test]
  fn a_preconnect_the_page_makes_is_a_head_row_of_its_meta() {
    let page = "import { preconnect } from \"react-dom\";\nimport { mediaUrl, getPage } from \"../src/cms\";\n\nexport default async function Home() {\n  preconnect(mediaUrl.href);\n  const page = await getPage();\n  return <p>{page.title}</p>;\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/cms.ts", "export const mediaUrl = new URL(\"https://media.example\");\nexport const getPage = async () => ({ title: \"t\" });\n")]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("import { linkTag } from \"@snapfire/fsr/head\";"), "{loader}");
    assert!(loader.contains("import { mediaUrl, getPage } from \"../src/cms\";"), "{loader}");
    assert!(loader.contains("export const meta = () => ({ head: [linkTag(\"preconnect\", mediaUrl.href)] });"), "{loader}");
    assert!(!loader.contains("react-dom") && !loader.contains("  preconnect("), "{loader}");
    let page = out.text("app/routes/page.tsx").unwrap();
    assert!(!page.contains("react-dom") && !page.contains("preconnect"), "{page}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_function_memoizing_in_a_map_is_read_as_the_call_it_memoizes() {
    let data = "let cache = new Map();\n\nexport function fetchData(url) {\n  if (!cache.has(url)) {\n    cache.set(url, getData(url));\n  }\n  return cache.get(url);\n}\n\nasync function getData(url) {\n  return [url];\n}\n";
    let app = "import { use } from 'react';\nimport { fetchData } from './data.js';\n\nexport default function Albums() {\n  const albums = use(fetchData('/albums'));\n  return <ul>{albums.map(a => <li key={a}>{a}</li>)}</ul>;\n}\n";
    let page = "import App from \"@src/App.js\";\n\nexport default function Page() {\n  return <App />;\n}\n";
    let input = tree(&[("app/src/data.js", data), ("app/src/App.js", app), ("app/routes/page.tsx", page)]);
    let (out, _) = lift(&input);
    assert_eq!(
      out.text("app/routes/page.loader.ts").unwrap(),
      "function fetchData(url) {\n  return getData(url);\n}\n\nasync function getData(url) {\n  return [url];\n}\n\nexport async function load() {\n  const albumsAlbums = await fetchData('/albums');\n  return { albumsAlbums };\n}\n\nexport const store = ({ data }) => ({ \"albums/albums\": data.albumsAlbums });\n"
    );
    assert_eq!(out.text("app/src/App.js").unwrap(), "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { albumsAlbums } from \"@src/store\";\n\nexport default function Albums() {\n  const [albums] = useStore(albumsAlbums, null);\n  return <ul>{albums.map(a => <li key={a}>{a}</li>)}</ul>;\n}\n");
  }

  #[test]
  fn a_component_placed_only_under_a_condition_is_noted_as_loading_eagerly() {
    let posts = "export default async function Posts() {\n  const posts = await getPosts();\n  return <ul>{posts}</ul>;\n}\n\nasync function getPosts() {\n  return [];\n}\n";
    let app = "import { useState } from 'react';\nimport Posts from './Posts';\n\nexport default function App() {\n  const [tab, setTab] = useState('about');\n  return <div>{tab === 'posts' && <Posts />}</div>;\n}\n";
    let page = "import App from \"@src/App.js\";\n\nexport default function Page() {\n  return <App />;\n}\n";
    let input = tree(&[("app/src/Posts.js", posts), ("app/src/App.js", app), ("app/routes/page.tsx", page)]);
    let (_, report) = lift(&input);
    assert!(report.notes.iter().any(|n| n.message == "`Posts` renders only under a condition, so its data now loads with the page before any render reads it"), "{:?}", report.notes);
  }

  #[test]
  fn a_layout_taking_its_props_whole_reads_params_from_the_loader() {
    let layout = "import { getDictionary } from \"../../src/dictionaries\";\n\nexport default async function Root(props: { params: Promise<{ lang: string }>; children: React.ReactNode }) {\n  const params = await props.params;\n  const { children } = props;\n  const dict = await getDictionary(params.lang);\n  return <div lang={params.lang}>{dict.title}{children}</div>;\n}\n";
    let input = tree(&[("app/routes/[lang]/layout.tsx", layout), ("app/src/dictionaries.ts", "export async function getDictionary() {}\n")]);
    let (out, report) = lift(&input);
    assert_eq!(
      out.text("app/routes/[lang]/layout.loader.ts").unwrap(),
      "import type { Ctx } from \"@snapfire/fsr/host\";\nimport { getDictionary } from \"../../src/dictionaries\";\n\nexport async function load({ params }: Ctx) {\n  const dict = await getDictionary(params.lang);\n  return { params, dict };\n}\n"
    );
    assert_eq!(
      out.text("app/routes/[lang]/layout.tsx").unwrap(),
      "export default function Root(props) {\n  const { params, dict } = props;\n  const { children } = props;\n  return <div lang={params.lang}>{dict.title}{children}</div>;\n}\n"
    );
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_component_awaiting_a_read_known_before_render_reads_it_from_the_store() {
    let albums = "async function fetchAlbums() {\n  await new Promise(resolve => setTimeout(resolve, 1000));\n  return ['Abbey Road', 'Let It Be'];\n}\n\nexport default async function Albums() {\n  const albums = await fetchAlbums();\n  return <ul>{albums.map(a => <li key={a}>{a}</li>)}</ul>;\n}\n";
    let app = "import Albums from './Albums';\n\nexport default function App() {\n  return <Albums />;\n}\n";
    let page = "import App from \"@src/App.js\";\n\nexport default function Page() {\n  return <App />;\n}\n";
    let input = tree(&[("app/src/Albums.js", albums), ("app/src/App.js", app), ("app/routes/page.tsx", page)]);
    let (out, report) = lift(&input);
    assert_eq!(
      out.text("app/routes/page.loader.ts").unwrap(),
      "async function fetchAlbums() {\n  return ['Abbey Road', 'Let It Be'];\n}\n\nexport async function load() {\n  const albumsAlbums = await fetchAlbums();\n  return { albumsAlbums };\n}\n\nexport const store = ({ data }) => ({ \"albums/albums\": data.albumsAlbums });\n"
    );
    assert_eq!(
      out.text("app/src/Albums.js").unwrap(),
      "import { useStore } from \"@snapfire/fsr/client/react\";\nimport { albumsAlbums } from \"@src/store\";\n\nexport default function Albums() {\n  const [albums] = useStore(albumsAlbums, null);\n  return <ul>{albums.map(a => <li key={a}>{a}</li>)}</ul>;\n}\n"
    );
    assert_eq!(out.text("app/src/store.ts").unwrap(), "import { key } from \"@snapfire/fsr/client/store\";\n\nexport const albumsAlbums = key<any>(\"albums/albums\");\n");
    assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  }

  #[test]
  fn a_function_a_component_awaits_and_only_calls_with_literals_seeds_each_result() {
    let nav = "import { getTranslations } from \"./i18n\";\n\nexport default async function Nav() {\n  const [t, tMenu] = await Promise.all([getTranslations(\"Common\"), getTranslations(\"Menu\")]);\n  const label = t(\"Label.Navigation\");\n  return <nav aria-label={label}><button title={tMenu(\"Label.Open\")}>{t(\"Label.Navigation\")}</button></nav>;\n}\n";
    let page = "import Nav from \"@src/Nav\";\n\nexport default function Page() {\n  return <Nav />;\n}\n";
    let input = tree(&[("app/src/Nav.tsx", nav), ("app/src/i18n.ts", "export const getTranslations = async (ns) => (k) => k;\n"), ("app/routes/page.tsx", page)]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("  const navT = await getTranslations(\"Common\");\n  const navTLabelNavigation = navT(\"Label.Navigation\");\n  const navTMenu = await getTranslations(\"Menu\");\n  const navTMenuLabelOpen = navTMenu(\"Label.Open\");\n  return { navTLabelNavigation, navTMenuLabelOpen };"), "{loader}");
    assert!(loader.contains("\"nav/tLabelNavigation\": data.navTLabelNavigation, \"nav/tMenuLabelOpen\": data.navTMenuLabelOpen"), "{loader}");
    let nav = out.text("app/src/Nav.tsx").unwrap();
    assert!(nav.contains("  const [tLabelNavigation] = useStore(navTLabelNavigation, null);\n  const [tMenuLabelOpen] = useStore(navTMenuLabelOpen, null);\n  const label = tLabelNavigation;\n"), "{nav}");
    assert!(nav.contains("title={tMenuLabelOpen}>{tLabelNavigation}</button>"), "{nav}");
    assert!(!nav.contains("getTranslations"), "{nav}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_component_beneath_a_page_with_a_loader_joins_its_reads_to_that_load() {
    let nav = "import { getLabels } from \"./data\";\n\nexport default async function Nav({ links }) {\n  const labels = await getLabels();\n  return <nav aria-label={labels.title}>{links.length}</nav>;\n}\n";
    let page = "import Nav from \"@src/Nav\";\nimport { getPage } from \"@src/data\";\n\nexport default async function Page() {\n  const page = await getPage();\n  return <Nav links={page.links} />;\n}\n";
    let data = "export const getLabels = async () => ({ title: \"t\" });\nexport const getPage = async () => ({ links: [] });\n";
    let input = tree(&[("app/src/Nav.tsx", nav), ("app/src/data.ts", data), ("app/routes/page.tsx", page)]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("  const page = await getPage();\n  const navLabels = await getLabels();\n  return { page, navLabels };"), "{loader}");
    assert!(loader.contains("export const store = ({ data }) => ({ \"nav/labels\": data.navLabels });"), "{loader}");
    assert_eq!(loader.matches("from \"../src/data\"").count() + loader.matches("from \"@src/data\"").count(), loader.lines().filter(|l| l.starts_with("import") && l.contains("data\"")).count(), "{loader}");
    assert!(out.text("app/src/Nav.tsx").unwrap().contains("const [labels] = useStore(navLabels, null);"));
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }

  #[test]
  fn a_component_beneath_a_load_that_cannot_take_its_reads_is_left_whole() {
    let nav = "import { getLabels } from \"./data\";\n\nexport default async function Nav() {\n  const labels = await getLabels();\n  return <nav aria-label={labels.title} />;\n}\n";
    let page = "import Nav from \"@src/Nav\";\n\nexport default function Page() {\n  return <Nav />;\n}\n";
    let loader = "export async function load() {\n  if (Math.random() > 2) return { a: 1 };\n  return null;\n}\n";
    let input = tree(&[("app/src/Nav.tsx", nav), ("app/src/data.ts", "export const getLabels = async () => ({ title: \"t\" });\n"), ("app/routes/page.tsx", page), ("app/routes/page.loader.ts", loader)]);
    let (out, report) = lift(&input);
    assert_eq!(out.text("app/src/Nav.tsx").unwrap(), nav);
    assert_eq!(out.text("app/routes/page.loader.ts").unwrap(), loader);
    assert!(out.text("app/src/store.ts").is_none());
    assert!(report.notes.iter().any(|n| n.message.contains("does not end in returning an object")), "{:?}", report.notes);
  }

  #[test]
  fn a_component_awaiting_promise_all_into_names_reads_each_from_the_store() {
    let nav = "import { getLinks, getLabels } from \"./data\";\n\nexport default async function Nav() {\n  const [links, labels] = await Promise.all([getLinks(), getLabels(\"nav\")]);\n  return <nav aria-label={labels.title}>{links.length}</nav>;\n}\n";
    let page = "import Nav from \"@src/Nav\";\n\nexport default function Page() {\n  return <Nav />;\n}\n";
    let data = "export const getLinks = async () => [];\nexport const getLabels = async (ns) => ({ title: ns });\n";
    let input = tree(&[("app/src/Nav.tsx", nav), ("app/src/data.ts", data), ("app/routes/page.tsx", page)]);
    let (out, report) = lift(&input);
    let loader = out.text("app/routes/page.loader.ts").unwrap();
    assert!(loader.contains("  const navLinks = await getLinks();\n  const navLabels = await getLabels(\"nav\");\n  return { navLinks, navLabels };"), "{loader}");
    assert!(loader.contains("export const store = ({ data }) => ({ \"nav/links\": data.navLinks, \"nav/labels\": data.navLabels });"), "{loader}");
    let nav = out.text("app/src/Nav.tsx").unwrap();
    assert!(nav.contains("export default function Nav() {\n  const [links] = useStore(navLinks, null);\n  const [labels] = useStore(navLabels, null);\n  return"), "{nav}");
    assert!(!nav.contains("Promise.all") && !nav.contains("./data"), "{nav}");
    assert!(report.notes.is_empty(), "{:?}", report.notes);
  }
}
