//! What a component awaits while it renders, moved to where FSR reads data:
//! the route's loader, which runs before anything renders.

use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{describe, import_text, line_range, local, tidy, Item, Reads};
use crate::relocate::{dirname, normalize, relative};
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(RouteLift), Box::new(ComponentLift)]
}

/// The default export of `module` when it is a function: its declaration
/// and its name.
fn default_function(module: &js::Module) -> Option<(&js::Function, Option<String>)> {
  let mut named = None;
  for item in &module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(js::ExportDefaultDecl { decl: js::DefaultDecl::Fn(f), .. })) => {
        return Some((&f.function, f.ident.as_ref().map(|i| i.sym.to_string())));
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) => {
        if let js::Expr::Ident(i) = &*e.expr {
          named = Some(i.sym.to_string());
        }
      }
      _ => {}
    }
  }
  let named = named?;
  module.body.iter().find_map(|item| match item {
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Fn(f))) if f.ident.sym == *named => Some((&*f.function, Some(named.clone()))),
    _ => None,
  })
}

fn route_kind(path: &str) -> Option<&'static str> {
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

/// Whether `node` holds JSX, an `await` or a `return` outside the functions
/// inside it.
#[derive(Default)]
struct Holds {
  jsx: bool,
  awaits: bool,
  returns: bool,
  depth: usize,
}

impl Visit for Holds {
  fn visit_jsx_element(&mut self, _: &js::JSXElement) {
    self.jsx = true;
  }

  fn visit_jsx_fragment(&mut self, _: &js::JSXFragment) {
    self.jsx = true;
  }

  fn visit_await_expr(&mut self, n: &js::AwaitExpr) {
    if self.depth == 0 {
      self.awaits = true;
    }
    n.visit_children_with(self);
  }

  fn visit_return_stmt(&mut self, n: &js::ReturnStmt) {
    if self.depth == 0 {
      self.returns = true;
    }
    n.visit_children_with(self);
  }

  fn visit_function(&mut self, n: &js::Function) {
    self.depth += 1;
    n.visit_children_with(self);
    self.depth -= 1;
  }

  fn visit_arrow_expr(&mut self, n: &js::ArrowExpr) {
    self.depth += 1;
    n.visit_children_with(self);
    self.depth -= 1;
  }
}

fn holds(node: &impl VisitWith<Holds>) -> Holds {
  let mut h = Holds::default();
  node.visit_with(&mut h);
  h
}

fn reads(node: &impl VisitWith<Reads>) -> BTreeSet<String> {
  let mut r = Reads { names: BTreeSet::new() };
  node.visit_with(&mut r);
  r.names
}

fn binds(stmt: &js::Stmt) -> Vec<String> {
  match stmt {
    js::Stmt::Decl(js::Decl::Var(v)) => {
      let mut out = Vec::new();
      for d in &v.decls {
        crate::items::pat_names(&d.name, &mut out);
      }
      out
    }
    js::Stmt::Decl(js::Decl::Fn(f)) => vec![f.ident.sym.to_string()],
    _ => Vec::new(),
  }
}

/// The module-level items `needed` reaches, imports aside, followed through
/// what each of them reads.
fn closure(items: &[Item], needed: &mut BTreeSet<String>) -> BTreeSet<usize> {
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
fn imports_for(text: &str, parsed: &Parsed, needed: &BTreeSet<String>, from: &str, to: &str) -> String {
  let mut out = String::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| needed.contains(&local(s))).collect();
    if kept.is_empty() {
      continue;
    }
    let mut line = import_text(text, parsed, import, &kept);
    let spec = import.src.value.to_string_lossy().into_owned();
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
fn is_delay(stmt: &js::Stmt) -> bool {
  let js::Stmt::Expr(e) = stmt else { return false };
  let js::Expr::Await(a) = &*e.expr else { return false };
  let js::Expr::New(n) = &*a.arg else { return false };
  let js::Expr::Ident(callee) = &*n.callee else { return false };
  if &*callee.sym != "Promise" {
    return false;
  }
  let mut found = Calls { names: Vec::new() };
  n.visit_with(&mut found);
  found.names.iter().any(|c| c == "setTimeout")
}

struct Calls {
  names: Vec<String>,
}

impl Visit for Calls {
  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(e) = &n.callee {
      if let js::Expr::Ident(i) = &**e {
        self.names.push(i.sym.to_string());
      }
    }
    n.visit_children_with(self);
  }
}

/// `text` with every artificial delay among `stmts` taken out, each a note.
fn without_delays(text: &str, parsed: &Parsed, node: &impl VisitWith<Delays>, base: usize, edits: &mut Edits, path: &str, report: &mut Report) {
  let mut found = Delays { found: Vec::new() };
  node.visit_with(&mut found);
  for span in found.found {
    let range = parsed.range(span);
    report.note(path, parsed.line(span), "an artificial delay was dropped: it changes when data arrives, not what renders");
    edits.replace(shift(line_range(text, range), base), "");
  }
}

struct Delays {
  found: Vec<Span>,
}

impl Visit for Delays {
  fn visit_stmt(&mut self, n: &js::Stmt) {
    if is_delay(n) {
      self.found.push(n.span());
      return;
    }
    n.visit_children_with(self);
  }
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
      let Ok(parsed) = parse(&path, &text) else { continue };
      let Some((function, _)) = default_function(&parsed.module) else { continue };
      if !function.is_async {
        continue;
      }
      let loader_path = format!("{}/{kind}.loader.ts", dirname(&path));
      let existing = tree.text(&loader_path).unwrap_or_default().to_owned();
      if existing.contains("function load") || existing.contains("const load") {
        report.note(&path, parsed.line(function.span), "the route's loader already has a `load`, so what this component awaits was left in it");
        continue;
      }
      match lift_route(&path, &text, &parsed, function, &loader_path, report) {
        Some((page, load, fails)) => {
          let mut loader = String::new();
          if fails {
            loader.push_str("import { fail } from \"@snapfire/fsr\";\n");
          }
          loader.push_str(&load.imports);
          loader.push('\n');
          loader.push_str(&existing);
          if !existing.is_empty() {
            loader.push('\n');
          }
          loader.push_str(&load.body);
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
}

fn lift_route(path: &str, text: &str, parsed: &Parsed, function: &js::Function, loader_path: &str, report: &mut Report) -> Option<(String, Load, bool)> {
  let body = function.body.as_ref()?;
  let line = parsed.line(function.span);
  let Some((last, before)) = body.stmts.split_last() else { return None };
  if !matches!(last, js::Stmt::Return(_)) {
    report.note(path, line, "the async component does not end in a `return`, so what it awaits was left in it");
    return None;
  }
  let mut moved: Vec<&js::Stmt> = Vec::new();
  let mut kept: Vec<&js::Stmt> = Vec::new();
  let mut kept_binds: BTreeSet<String> = BTreeSet::new();
  for stmt in before {
    let h = holds(stmt);
    if h.returns {
      report.note(path, parsed.line(stmt.span()), "the async component returns early, so what it awaits was left in it");
      return None;
    }
    let r = reads(stmt);
    if h.jsx || r.iter().any(|n| kept_binds.contains(n)) {
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
  }
  let reads_params = moved_reads.contains("params") || later_reads.contains("params");
  let reads_query = moved_reads.contains("searchParams") || later_reads.contains("searchParams");

  let items: Vec<Item> = parsed.module.body.iter().map(|item| describe(item, parsed)).collect();
  let mut needed = moved_reads.clone();
  let taken = closure(&items, &mut needed);

  let mut fails = false;
  let mut statements = String::new();
  for stmt in &moved {
    let range = parsed.range(stmt.span());
    let mut edits = Edits::default();
    let mut found = ContextUses { found: Vec::new() };
    stmt.visit_with(&mut found);
    for (span, with) in found.found {
      if with.starts_with("fail(") {
        fails = true;
      }
      edits.replace(shift(parsed.range(span), range.start), with);
    }
    without_delays(text, parsed, *stmt, range.start, &mut edits, path, report);
    if found_redirect(stmt) {
      report.note(path, parsed.line(stmt.span()), "a `redirect()` in the loader belongs in `middleware.ts`, which answers `{ redirect }` before the route runs");
    }
    statements.push_str("  ");
    statements.push_str(&edits.apply(&text[range]));
    statements.push('\n');
  }
  let mut fields: Vec<String> = returned.clone();
  if later_reads.contains("params") {
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
  let ctx = if ctx.is_empty() { String::new() } else { format!("{{ {} }}", ctx.join(", ")) };
  let mut decls = String::new();
  for (n, item) in items.iter().enumerate() {
    if taken.contains(&n) {
      let mut edits = Edits::default();
      without_delays(text, parsed, &parsed.module.body[n], item.range.start, &mut edits, path, report);
      decls.push_str(&edits.apply(&text[item.range.clone()]));
      decls.push_str("\n\n");
    }
  }
  let body_text = format!("{decls}export async function load({ctx}) {{\n{statements}  return {{ {} }};\n}}\n", fields.join(", "));
  let imports = imports_for(text, parsed, &needed, path, loader_path);

  let mut page = Edits::default();
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
  let mut props: Vec<String> = fields.iter().map(|f| f.split(':').next().unwrap_or(f).trim().to_owned()).collect();
  props.extend(param_names.iter().filter(|p| !["params", "searchParams"].contains(&p.as_str())).cloned());
  let new_param = if props.is_empty() { String::new() } else { format!("{{ {} }}", props.join(", ")) };
  match function.params.first() {
    Some(param) => page.replace(parsed.range(param.span()), new_param),
    None => {
      let open = text[parsed.range(function.span)].find('(').map(|i| parsed.range(function.span).start + i + 1);
      if let Some(open) = open {
        page.replace(open..open, new_param);
      }
    }
  }
  let page = crate::items::prune(path, &page.apply(text), &needed);
  Some((page, Load { imports, body: body_text }, fails))
}

fn param_names(function: &js::Function) -> Vec<String> {
  let mut out = Vec::new();
  if let Some(js::Pat::Object(o)) = function.params.first().map(|p| &p.pat) {
    for p in &o.props {
      match p {
        js::ObjectPatProp::Assign(a) => out.push(a.key.sym.to_string()),
        js::ObjectPatProp::KeyValue(kv) => {
          if let js::PropName::Ident(k) = &kv.key {
            out.push(k.sym.to_string());
          }
        }
        js::ObjectPatProp::Rest(_) => {}
      }
    }
  }
  out
}

fn found_redirect(stmt: &js::Stmt) -> bool {
  let mut c = Calls { names: Vec::new() };
  stmt.visit_with(&mut c);
  c.names.iter().any(|n| n == "redirect" || n == "permanentRedirect")
}

/// Next's request reads, written as FSR's: `await params` as `params`,
/// `searchParams` as `query` and `notFound()` as a failure.
struct ContextUses {
  found: Vec<(Span, String)>,
}

impl Visit for ContextUses {
  fn visit_await_expr(&mut self, n: &js::AwaitExpr) {
    if let js::Expr::Ident(i) = &*n.arg {
      match &*i.sym {
        "params" => return self.found.push((n.span, "params".to_owned())),
        "searchParams" => return self.found.push((n.span, "query".to_owned())),
        _ => {}
      }
    }
    n.visit_children_with(self);
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    if &*n.sym == "searchParams" {
      self.found.push((n.span, "query".to_owned()));
    }
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(e) = &n.callee {
      if let js::Expr::Ident(i) = &**e {
        if &*i.sym == "notFound" && n.args.is_empty() {
          return self.found.push((n.span, "fail(\"not_found\", \"not found\")".to_owned()));
        }
      }
    }
    n.visit_children_with(self);
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
      let Ok(parsed) = parse(&path, &text) else { continue };
      let items: Vec<Item> = parsed.module.body.iter().map(|item| describe(item, &parsed)).collect();
      let module_names: BTreeSet<String> = items.iter().flat_map(|i| i.binds.iter().cloned()).collect();
      let mut edits = Edits::default();
      let mut lifted: Vec<(String, Vec<Read>)> = Vec::new();
      for item in &parsed.module.body {
        let Some((name, function)) = component(item) else { continue };
        let Some(reads) = component_reads(&text, &parsed, function, &module_names, &path, report) else { continue };
        if reads.is_empty() {
          continue;
        }
        if function.is_async {
          let span = parsed.range(item.span());
          if let Some(at) = text[span.clone()].find("async ") {
            edits.replace(span.start + at..span.start + at + "async ".len(), "");
          }
        }
        lifted.push((name, reads));
      }
      if lifted.is_empty() {
        continue;
      }
      let pages: Vec<String> = pages_reaching(&graph, &path);
      if pages.is_empty() {
        report.note(&path, 0, "a component awaits data but no page places it, so it was left as it is");
        continue;
      }
      let mut needed: BTreeSet<String> = BTreeSet::new();
      let mut loads = String::new();
      let mut seeds = Vec::new();
      for (component, reads) in &lifted {
        for read in reads {
          let key_id = format!("{}/{}", component.to_lowercase(), read.name);
          let key_name = format!("{}{}", lower_first(component), upper_first(&read.name));
          let field = key_name.clone();
          let wrapped = parse("x.ts", &format!("({})", read.expr)).ok();
          if let Some(w) = wrapped {
            needed.extend(reads_of(&w));
          }
          loads.push_str(&format!("  const {field} = await {};\n", read.expr));
          seeds.push(format!("\"{key_id}\": data.{field}"));
          edits.replace(read.stmt.clone(), format!("const [{}] = useStore({key_name}, null);", read.name));
          keys.push((key_name, key_id));
        }
      }
      let taken = closure(&items, &mut needed);
      let mut decls = String::new();
      for (n, item) in items.iter().enumerate() {
        if taken.contains(&n) {
          let mut e = Edits::default();
          without_delays(&text, &parsed, &parsed.module.body[n], item.range.start, &mut e, &path, report);
          decls.push_str(&e.apply(&text[item.range.clone()]));
          decls.push_str("\n\n");
        }
      }
      let fields: Vec<String> = lifted.iter().flat_map(|(c, rs)| rs.iter().map(move |r| format!("{}{}", lower_first(c), upper_first(&r.name)))).collect();
      for page in &pages {
        let loader_path = format!("{}/page.loader.ts", dirname(page));
        let existing = tree.text(&loader_path).unwrap_or_default().to_owned();
        if existing.contains("function load") || existing.contains("const load") || existing.contains("export const store") {
          report.note(&path, 0, format!("`{loader_path}` already has a `load` or a `store`, so the data this component awaits was not moved there"));
          continue;
        }
        let imports = imports_for(&text, &parsed, &needed, &path, &loader_path);
        let loader = format!(
          "{imports}\n{existing}\n{decls}export async function load() {{\n{loads}  return {{ {} }};\n}}\n\nexport const store = ({{ data }}) => ({{ {} }});\n",
          fields.join(", "),
          seeds.join(", ")
        );
        tree.insert(loader_path, tidy(&loader));
      }
      let key_names: Vec<String> = lifted.iter().flat_map(|(c, rs)| rs.iter().map(move |r| format!("{}{}", lower_first(c), upper_first(&r.name)))).collect();
      let header = format!("import {{ useStore }} from \"@snapfire/fsr-client/react\";\nimport {{ {} }} from \"@src/store\";\n", key_names.join(", "));
      let mut removable = needed.clone();
      removable.insert("use".to_owned());
      let body = crate::items::prune(&path, &edits.apply(&text), &removable);
      tree.insert(path.clone(), tidy(&format!("{header}{body}")));
    }
    if keys.is_empty() {
      return;
    }
    let mut store = tree.text("app/src/store.ts").unwrap_or_default().to_owned();
    if !store.contains("@snapfire/fsr-client/store") {
      store.insert_str(0, "import { key } from \"@snapfire/fsr-client/store\";\n\n");
    }
    for (name, id) in keys {
      store.push_str(&format!("export const {name} = key<any>(\"{id}\");\n"));
    }
    tree.insert("app/src/store.ts", store);
  }
}

fn reads_of(parsed: &Parsed) -> BTreeSet<String> {
  reads(&parsed.module)
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
fn component(item: &js::ModuleItem) -> Option<(String, &js::Function)> {
  let (ident, function) = match item {
    js::ModuleItem::Stmt(js::Stmt::Decl(js::Decl::Fn(f))) => (&f.ident, &*f.function),
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(js::ExportDecl { decl: js::Decl::Fn(f), .. })) => (&f.ident, &*f.function),
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(js::ExportDefaultDecl { decl: js::DefaultDecl::Fn(f), .. })) => (f.ident.as_ref()?, &*f.function),
    _ => return None,
  };
  ident.sym.chars().next().filter(|c| c.is_uppercase())?;
  Some((ident.sym.to_string(), function))
}

/// The reads a component makes that the lift can move: `None` when it
/// awaits something the lift cannot, so the component is left whole.
fn component_reads(text: &str, parsed: &Parsed, function: &js::Function, module_names: &BTreeSet<String>, path: &str, report: &mut Report) -> Option<Vec<Read>> {
  let body = function.body.as_ref()?;
  let mut out = Vec::new();
  for stmt in &body.stmts {
    let js::Stmt::Decl(js::Decl::Var(var)) = stmt else {
      if function.is_async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits in a statement the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let [decl] = var.decls.as_slice() else { continue };
    let (js::Pat::Ident(name), Some(init)) = (&decl.name, decl.init.as_deref()) else {
      if function.is_async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits into a pattern the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let call = match init {
      js::Expr::Await(a) => Some(&*a.arg),
      js::Expr::Call(c) => match &c.callee {
        js::Callee::Expr(e) if matches!(&**e, js::Expr::Ident(i) if &*i.sym == "use") => c.args.first().map(|a| &*a.expr),
        _ => None,
      },
      _ => None,
    };
    let Some(call) = call else {
      if function.is_async && holds(stmt).awaits {
        report.note(path, parsed.line(stmt.span()), "the component awaits inside an expression the lift does not move, so it was left as it is");
        return None;
      }
      continue;
    };
    let free = reads(call);
    if let Some(local) = free.iter().find(|n| !module_names.contains(*n) && !GLOBALS.contains(&n.as_str())) {
      report.note(path, parsed.line(stmt.span()), format!("the read depends on `{local}`, which is not known before the component renders, so it stays in the browser"));
      if function.is_async {
        return None;
      }
      continue;
    }
    out.push(Read { name: name.id.sym.to_string(), expr: text[parsed.range(call.span())].to_owned(), stmt: parsed.range(stmt.span()) });
  }
  Some(out)
}

const GLOBALS: &[&str] = &["undefined", "String", "Number", "Math", "JSON", "Date", "Boolean", "Array", "Object", "Promise"];

/// Each script's imports resolved to the scripts they name, relative
/// specifiers and `@src/` alike.
fn imports_graph(tree: &Tree) -> BTreeMap<String, Vec<String>> {
  let mut graph = BTreeMap::new();
  for path in tree.paths().filter(|p| p.starts_with("app/") && is_script(p)) {
    let Some(text) = tree.text(path) else { continue };
    let Ok(parsed) = parse(path, text) else { continue };
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
  fn an_async_page_awaits_in_its_loader_and_receives_the_results() {
    let page = "import prisma from \"../src/lib/prisma\";\nimport { Product } from \"../src/components/Product\";\n\nexport default async function Home({ searchParams }) {\n  const { q } = await searchParams;\n  const products = await prisma.product.findMany({ where: { q } });\n  if (!products) notFound();\n  return (\n    <main>\n      {products.map((product) => (\n        <Product key={product.id} product={product} />\n      ))}\n    </main>\n  );\n}\n";
    let input = tree(&[("app/routes/page.tsx", page), ("app/src/lib/prisma.ts", "export default {};\n"), ("app/src/components/Product.tsx", "export function Product() { return null; }\n")]);
    let (out, report) = lift(&input);
    assert_eq!(
      out.text("app/routes/page.loader.ts").unwrap(),
      "import { fail } from \"@snapfire/fsr\";\nimport prisma from \"../src/lib/prisma\";\n\nexport async function load({ query }) {\n  const { q } = query;\n  const products = await prisma.product.findMany({ where: { q } });\n  if (!products) fail(\"not_found\", \"not found\");\n  return { products };\n}\n"
    );
    assert_eq!(
      out.text("app/routes/page.tsx").unwrap(),
      "import { Product } from \"../src/components/Product\";\n\nexport default function Home({ products }) {\n  return (\n    <main>\n      {products.map((product) => (\n        <Product key={product.id} product={product} />\n      ))}\n    </main>\n  );\n}\n"
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
      "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { albumsAlbums } from \"@src/store\";\n\nexport default function Albums() {\n  const [albums] = useStore(albumsAlbums, null);\n  return <ul>{albums.map(a => <li key={a}>{a}</li>)}</ul>;\n}\n"
    );
    assert_eq!(out.text("app/src/store.ts").unwrap(), "import { key } from \"@snapfire/fsr-client/store\";\n\nexport const albumsAlbums = key<any>(\"albums/albums\");\n");
    assert_eq!(report.notes.len(), 1, "{:?}", report.notes);
  }
}
