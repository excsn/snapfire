//! An Ink application: its sources under `app/src/`, a `routes/page.tsx`
//! that renders the component the entry rendered and every import written as
//! an FSR application writes it.

use std::collections::BTreeMap;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::lift::ctx_type;
use crate::next::{read_packages_but, source_path};
use crate::relocate::{dirname, find, normalize, relocate, Aliases};
use crate::syntax::{is_script, parse, SCRIPT_EXTENSIONS};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Layout)]
}

/// Packages a terminal page gets from FSR itself, which the migrated application does not list.
const PROVIDED: &[&str] = &["ink", "react", "react-dom", "ink-spinner", "cli-spinners", "ink-text-input", "ms", "strip-ansi", "string-width", "wrap-ansi"];

/// Moves the sources to `app/src/` and tests to `app/tests/`, drops the entry
/// that called Ink's `render` and writes `app/routes/page.tsx` rendering the
/// component it rendered, with imports rewritten to follow.
struct Layout;

impl Pass for Layout {
  fn name(&self) -> &'static str {
    "ink-layout"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    read_packages_but(tree, report, PROVIDED);
    let root = ["source/", "src/"].into_iter().find(|r| tree.paths().any(|p| p.starts_with(r))).unwrap_or("");
    let entry = entry(tree, root);
    // An entry that declares the component it renders stays, exporting it, with the `render` call taken out.
    let kept_entry = entry.as_ref().and_then(|e| keep_entry(tree, e));
    if let (Some(e), Some(text)) = (&entry, &kept_entry) {
      tree.insert(e.path.clone(), text.clone());
    }
    let mut moves = BTreeMap::new();
    for path in tree.paths() {
      if kept_entry.is_none() && entry.as_ref().is_some_and(|e| e.path == path) {
        continue;
      }
      let inner = path.strip_prefix(root).unwrap_or(path);
      let file = inner.rsplit('/').next().unwrap_or(inner);
      let is_test = is_script(file) && (file.contains(".test.") || file.contains(".spec.") || file.split_once('.').is_some_and(|(stem, _)| stem == "test"));
      if is_test && !inner.contains('/') {
        moves.insert(path.to_owned(), format!("app/tests/{inner}"));
      } else if let Some(to) = source_path(path, root, report) {
        moves.insert(path.to_owned(), to);
      }
    }
    let aliases = Aliases::read(tree);
    let loader = match (&entry, kept_entry.is_some()) {
      (Some(entry), false) => loader_for(tree, &moves, &aliases, entry, report),
      _ => None,
    };
    let page = match &entry {
      Some(entry) => page_for(tree, &moves, &aliases, entry, kept_entry.is_some(), loader.is_some(), report),
      None => {
        report.note("package.json", 0, "no file imports `render` from `ink`, so `routes/page.tsx` was not written");
        None
      }
    };
    *tree = relocate(tree, &moves, &aliases, report);
    if let Some(page) = page {
      tree.insert("app/routes/page.tsx", page);
    }
    if let Some(loader) = loader {
      tree.insert("app/routes/page.loader.ts", loader);
    }
  }
}

/// The file that calls Ink's `render` and what it rendered.
struct Entry {
  path: String,
  line: usize,
  /// The `render` call.
  call: Span,
  /// The component rendered, by the name the entry holds it under.
  component: String,
  /// What `render` passed the component: prop names from an element, or the count of arguments from a call.
  passed: Passed,
}

enum Passed {
  /// `<Root …/>` as written: its source, the prop names on the root, each prop's value as written and every name the tree reads, tags and attribute expressions alike.
  Element { text: String, props: Vec<String>, values: Vec<(String, String)>, names: Vec<String> },
  /// `Root(…)` or `createElement(Root, props)`: the argument count, or the prop names.
  Arguments(usize),
  Props(Vec<String>),
}

/// The shallowest file under `root` that calls Ink's `render`, ties in path order.
fn entry(tree: &Tree, root: &str) -> Option<Entry> {
  let mut found: Option<Entry> = None;
  for path in tree.paths().filter(|p| is_script(p) && (p.starts_with(root) || !p.contains('/'))) {
    if found.as_ref().is_some_and(|f| f.path.matches('/').count() <= path.matches('/').count()) {
      continue;
    }
    let Some(text) = tree.text(path) else { continue };
    let Ok(parsed) = parse(path, text) else { continue };
    let mut calls = Renders { render: None, found: None, parsed: &parsed, text };
    parsed.module.visit_with(&mut calls);
    if let Some((line, call, component, passed)) = calls.found {
      found = Some(Entry { path: path.to_owned(), line, call, component, passed });
    }
  }
  found
}

struct Renders<'a> {
  parsed: &'a crate::syntax::Parsed,
  text: &'a str,
  /// The local name `render` from `ink` is imported under.
  render: Option<String>,
  found: Option<(usize, Span, String, Passed)>,
}

impl Visit for Renders<'_> {
  fn visit_import_decl(&mut self, n: &js::ImportDecl) {
    if n.src.value.as_str() != Some("ink") {
      return;
    }
    for spec in &n.specifiers {
      if let js::ImportSpecifier::Named(named) = spec {
        let imported = named.imported.as_ref().map(|i| match i {
          js::ModuleExportName::Ident(id) => id.sym.to_string(),
          js::ModuleExportName::Str(s) => s.value.to_string_lossy().into_owned(),
        });
        if imported.as_deref().unwrap_or(named.local.sym.as_ref()) == "render" {
          self.render = Some(named.local.sym.to_string());
        }
      }
    }
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if self.found.is_none() {
      if let (Some(render), js::Callee::Expr(callee)) = (&self.render, &n.callee) {
        if matches!(&**callee, js::Expr::Ident(id) if *id.sym == **render) {
          if let Some(arg) = n.args.first() {
            self.found = rendered(self.parsed, self.text, &arg.expr).map(|(component, passed)| (self.parsed.line(n.span()), n.span(), component, passed));
          }
        }
      }
    }
    n.visit_children_with(self);
  }
}

/// The component `render`'s argument renders: `<Name …/>`, `Name(…)` or `React.createElement(Name, props)`, with the root's name as the entry writes it.
fn rendered(parsed: &crate::syntax::Parsed, text: &str, e: &js::Expr) -> Option<(String, Passed)> {
  match e {
    js::Expr::Paren(p) => rendered(parsed, text, &p.expr),
    js::Expr::JSXElement(el) => {
      let root = match &el.opening.name {
        js::JSXElementName::Ident(name) => name.sym.to_string(),
        js::JSXElementName::JSXMemberExpr(m) => jsx_root(&m.obj).to_owned(),
        js::JSXElementName::JSXNamespacedName(_) => return None,
      };
      let props = el
        .opening
        .attrs
        .iter()
        .map(|attr| match attr {
          js::JSXAttrOrSpread::JSXAttr(a) => match &a.name {
            js::JSXAttrName::Ident(id) => id.sym.to_string(),
            js::JSXAttrName::JSXNamespacedName(ns) => format!("{}:{}", ns.ns.sym, ns.name.sym),
          },
          js::JSXAttrOrSpread::SpreadElement(_) => "...".to_owned(),
        })
        .collect();
      let values = el
        .opening
        .attrs
        .iter()
        .filter_map(|attr| match attr {
          js::JSXAttrOrSpread::JSXAttr(a) => {
            let name = match &a.name {
              js::JSXAttrName::Ident(id) => id.sym.to_string(),
              js::JSXAttrName::JSXNamespacedName(ns) => format!("{}:{}", ns.ns.sym, ns.name.sym),
            };
            let value = match &a.value {
              None => "true".to_owned(),
              Some(js::JSXAttrValue::JSXExprContainer(c)) => match &c.expr {
                js::JSXExpr::Expr(e) => text[parsed.range(e.span())].to_owned(),
                js::JSXExpr::JSXEmptyExpr(_) => "undefined".to_owned(),
              },
              Some(other) => text[parsed.range(other.span())].to_owned(),
            };
            Some((name, value))
          }
          js::JSXAttrOrSpread::SpreadElement(_) => None,
        })
        .collect();
      let mut reads = Reads(Vec::new());
      el.visit_with(&mut reads);
      let range = parsed.range(el.span());
      Some((root, Passed::Element { text: text[range].to_owned(), props, values, names: reads.0 }))
    }
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => match &**callee {
        js::Expr::Ident(id) => Some((id.sym.to_string(), Passed::Arguments(call.args.len()))),
        js::Expr::Member(m) if matches!(&m.prop, js::MemberProp::Ident(p) if p.sym.as_ref() == "createElement") => {
          let component = match call.args.first().map(|a| &*a.expr) {
            Some(js::Expr::Ident(id)) => id.sym.to_string(),
            _ => return None,
          };
          let props = match call.args.get(1).map(|a| &*a.expr) {
            Some(js::Expr::Object(obj)) => obj
              .props
              .iter()
              .map(|p| match p {
                js::PropOrSpread::Prop(prop) => match &**prop {
                  js::Prop::Shorthand(id) => id.sym.to_string(),
                  js::Prop::KeyValue(kv) => prop_key(&kv.key),
                  js::Prop::Method(m) => prop_key(&m.key),
                  js::Prop::Getter(g) => prop_key(&g.key),
                  js::Prop::Setter(st) => prop_key(&st.key),
                  js::Prop::Assign(a) => a.key.sym.to_string(),
                },
                js::PropOrSpread::Spread(_) => "...".to_owned(),
              })
              .collect(),
            Some(js::Expr::Lit(js::Lit::Null(_))) | None => Vec::new(),
            Some(_) => vec!["...".to_owned()],
          };
          Some((component, Passed::Props(props)))
        }
        _ => None,
      },
      _ => None,
    },
    _ => None,
  }
}

fn prop_key(key: &js::PropName) -> String {
  match key {
    js::PropName::Ident(id) => id.sym.to_string(),
    js::PropName::Str(s) => s.value.to_string_lossy().into_owned(),
    js::PropName::Num(n) => n.value.to_string(),
    js::PropName::BigInt(b) => b.value.to_string(),
    js::PropName::Computed(_) => "[computed]".to_owned(),
  }
}

fn jsx_root(obj: &js::JSXObject) -> &str {
  match obj {
    js::JSXObject::Ident(id) => id.sym.as_ref(),
    js::JSXObject::JSXMemberExpr(m) => jsx_root(&m.obj),
  }
}

/// Every name a JSX tree reads: each tag's root and every identifier in an attribute or child expression.
struct Reads(Vec<String>);

impl Reads {
  fn note(&mut self, name: &str) {
    if !self.0.iter().any(|n| n == name) {
      self.0.push(name.to_owned());
    }
  }
}

impl Visit for Reads {
  fn visit_jsx_element_name(&mut self, n: &js::JSXElementName) {
    match n {
      js::JSXElementName::Ident(id) => self.note(id.sym.as_ref()),
      js::JSXElementName::JSXMemberExpr(m) => {
        let root = jsx_root(&m.obj).to_owned();
        self.note(&root);
      }
      js::JSXElementName::JSXNamespacedName(_) => {}
    }
  }

  fn visit_ident(&mut self, n: &js::Ident) {
    self.note(n.sym.as_ref());
  }

  fn visit_jsx_attr_name(&mut self, _: &js::JSXAttrName) {}

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    n.obj.visit_with(self);
    if let js::MemberProp::Computed(c) = &n.prop {
      c.visit_with(self);
    }
  }

  fn visit_jsx_closing_element(&mut self, _: &js::JSXClosingElement) {}
}

/// The entry's text with the component it renders exported, the `render` call taken out and `render` dropped from its `ink` import, when the entry declares that component itself; `None` when it imports it.
fn keep_entry(tree: &Tree, entry: &Entry) -> Option<String> {
  let text = tree.text(&entry.path)?;
  let parsed = parse(&entry.path, text).ok()?;
  let mut edits = crate::edit::Edits::default();
  let mut declared = false;
  for item in &parsed.module.body {
    match item {
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => {
        let (name, span) = match decl {
          js::Decl::Fn(f) => (f.ident.sym.to_string(), f.span()),
          js::Decl::Class(c) => (c.ident.sym.to_string(), c.span()),
          js::Decl::Var(v) => match v.decls.first().map(|d| &d.name) {
            Some(js::Pat::Ident(id)) if v.decls.len() == 1 => (id.id.sym.to_string(), v.span()),
            _ => continue,
          },
          _ => continue,
        };
        if name == entry.component {
          let start = parsed.range(span).start;
          edits.replace(start..start, "export ");
          declared = true;
        } else if let js::Decl::Var(v) = decl {
          if v.decls.iter().any(|d| d.init.as_deref().is_some_and(renders)) {
            edits.replace(crate::items::line_range(text, parsed.range(v.span())), String::new());
          }
        }
      }
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => {
        let name = match &export.decl {
          js::Decl::Fn(f) => f.ident.sym.to_string(),
          js::Decl::Class(c) => c.ident.sym.to_string(),
          js::Decl::Var(v) => match v.decls.first().map(|d| &d.name) {
            Some(js::Pat::Ident(id)) => id.id.sym.to_string(),
            _ => continue,
          },
          _ => continue,
        };
        declared |= name == entry.component;
      }
      js::ModuleItem::Stmt(stmt) if holds_render(stmt) => {
        edits.replace(crate::items::line_range(text, parsed.range(stmt.span())), String::new());
      }
      _ => {}
    }
  }
  if !declared {
    return None;
  }
  let text = edits.apply(text);
  let text = crate::items::drop_named_import(&text, "ink", "render").unwrap_or(text);
  Some(format!("{}\n", text.trim_end_matches('\n')))
}

/// Whether a top-level statement calls `render` anywhere in it: the call itself, or an `if` choosing between two.
fn holds_render(stmt: &js::Stmt) -> bool {
  struct Found(bool);
  impl Visit for Found {
    fn visit_call_expr(&mut self, n: &js::CallExpr) {
      if let js::Callee::Expr(callee) = &n.callee {
        if matches!(&**callee, js::Expr::Ident(id) if id.sym.as_ref() == "render") {
          self.0 = true;
        }
      }
      n.visit_children_with(self);
    }
    fn visit_function(&mut self, _: &js::Function) {}
    fn visit_arrow_expr(&mut self, _: &js::ArrowExpr) {}
  }
  let mut found = Found(false);
  stmt.visit_with(&mut found);
  found.0
}

/// Whether `e` is a call of `render`, Ink's or anyone's by that name.
fn renders(e: &js::Expr) -> bool {
  let mut e = e;
  while let js::Expr::Paren(p) = e {
    e = &p.expr;
  }
  let js::Expr::Call(call) = e else { return false };
  let js::Callee::Expr(callee) = &call.callee else { return false };
  matches!(&**callee, js::Expr::Ident(id) if id.sym.as_ref() == "render")
}

/// The entry's static imports: each local name, the specifier it came from and the clause that imports it again.
fn imports_of(parsed: &crate::syntax::Parsed) -> Vec<(String, String, String)> {
  let mut imports: Vec<(String, String, String)> = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(decl)) = item else { continue };
    for spec in &decl.specifiers {
      let (local, clause) = match spec {
        js::ImportSpecifier::Default(d) => {
          let local = d.local.sym.to_string();
          (local.clone(), local)
        }
        js::ImportSpecifier::Named(n) => {
          let imported = n.imported.as_ref().map(|i| match i {
            js::ModuleExportName::Ident(id) => id.sym.to_string(),
            js::ModuleExportName::Str(s) => s.value.to_string_lossy().into_owned(),
          });
          let local = n.local.sym.to_string();
          let clause = match imported {
            Some(imported) if imported != local => format!("{{ {imported} as {local} }}"),
            _ => format!("{{ {local} }}"),
          };
          (local, clause)
        }
        js::ImportSpecifier::Namespace(n) => {
          let local = n.local.sym.to_string();
          (local.clone(), format!("* as {local}"))
        }
      };
      imports.push((local, decl.src.value.to_string_lossy().into_owned(), clause));
    }
  }
  imports
}

/// Where an import of the entry is written from under `app/`: a relative specifier or an alias followed to the file it names and that file's `@src/` module; a package specifier as it stands.
fn follow(tree: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, entry: &Entry, spec: &str) -> Result<String, String> {
  let relative_spec = spec.starts_with("./") || spec.starts_with("../");
  let candidates: Vec<String> = if relative_spec { normalize(dirname(&entry.path), spec).into_iter().collect() } else { aliases.candidates(spec) };
  if candidates.is_empty() {
    return Ok(spec.to_owned());
  }
  let Some((target, _)) = candidates.iter().find_map(|c| find(tree, c)) else { return Err(format!("`{spec}`, which the build cannot follow")) };
  let Some(moved) = moves.get(&target).and_then(|m| m.strip_prefix("app/src/")) else { return Err(format!("`{target}`, which is not under the sources")) };
  let mut module = moved.to_owned();
  if let Some(ext) = SCRIPT_EXTENSIONS.iter().find(|ext| module.ends_with(*ext)) {
    module.truncate(module.len() - ext.len());
  }
  if let Some(dir) = module.strip_suffix("/index") {
    module = dir.to_owned();
  }
  Ok(format!("@src/{module}"))
}

/// The names as prose: `` `a` ``, `` `a` and `b` ``, `` `a`, `b` and `c` ``.
fn listed(names: &[String]) -> String {
  let quoted: Vec<String> = names.iter().map(|n| format!("`{n}`")).collect();
  match quoted.len() {
    0 => String::new(),
    1 => quoted[0].clone(),
    n => format!("{} and {}", quoted[..n - 1].join(", "), quoted[n - 1]),
  }
}

/// The statements before the one holding `call` in the innermost function around it, as byte ranges of `text`, with that function's parameter names and every name those statements read; the module's top-level statements when no function holds the call.
fn enclosing(parsed: &crate::syntax::Parsed, call: Span) -> (Vec<std::ops::Range<usize>>, Vec<String>, Vec<String>) {
  struct Enclosing<'a> {
    parsed: &'a crate::syntax::Parsed,
    call: Span,
    best: Option<(Span, Vec<std::ops::Range<usize>>, Vec<String>, Vec<String>)>,
  }
  impl Enclosing<'_> {
    fn consider(&mut self, span: Span, stmts: &[js::Stmt], params: &[js::Pat]) {
      if span.lo > self.call.lo || span.hi < self.call.hi {
        return;
      }
      if self.best.as_ref().is_some_and(|(best, ..)| best.lo >= span.lo && best.hi <= span.hi) {
        return;
      }
      let before: Vec<&js::Stmt> = stmts.iter().take_while(|s| s.span().hi <= self.call.lo).collect();
      let mut reads = Reads(Vec::new());
      before.iter().for_each(|s| s.visit_with(&mut reads));
      let mut names = Vec::new();
      params.iter().for_each(|p| crate::items::pat_names(p, &mut names));
      self.best = Some((span, before.iter().map(|s| self.parsed.range(s.span())).collect(), names, reads.0));
    }
  }
  impl Visit for Enclosing<'_> {
    fn visit_function(&mut self, f: &js::Function) {
      if let Some(body) = &f.body {
        let params: Vec<js::Pat> = f.params.iter().map(|p| p.pat.clone()).collect();
        self.consider(f.span(), &body.stmts, &params);
      }
      f.visit_children_with(self);
    }
    fn visit_arrow_expr(&mut self, a: &js::ArrowExpr) {
      if let js::ArrowFunctionBody::FunctionBody(body) = &*a.body {
        self.consider(a.span(), &body.stmts, &a.params);
      }
      a.visit_children_with(self);
    }
  }
  let mut search = Enclosing { parsed, call, best: None };
  parsed.module.visit_with(&mut search);
  if let Some((_, before, params, names)) = search.best {
    return (before, params, names);
  }
  let mut before = Vec::new();
  let mut reads = Reads(Vec::new());
  for item in &parsed.module.body {
    let js::ModuleItem::Stmt(stmt) = item else { continue };
    if stmt.span().hi <= call.lo {
      stmt.visit_with(&mut reads);
      before.push(parsed.range(stmt.span()));
    }
  }
  (before, Vec::new(), reads.0)
}

/// `app/routes/page.loader.ts` from the entry: the statements before `render` in the function that rendered, or at the module's top level, then a `return` of what `render` passed the component, each prop as written. The imports those statements and values read come along, written as [`follow`] writes them. `None` when `render` passed nothing or spread something.
fn loader_for(tree: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, entry: &Entry, report: &mut Report) -> Option<String> {
  let Passed::Element { props, values, names: value_names, .. } = &entry.passed else { return None };
  if values.is_empty() || props.iter().any(|p| p == "...") {
    return None;
  }
  let text = tree.text(&entry.path)?;
  let parsed = parse(&entry.path, text).ok()?;
  let (before, params, mut names) = enclosing(&parsed, entry.call);
  names.extend(value_names.iter().cloned());
  let mut imports = Vec::new();
  for (local, spec, clause) in imports_of(&parsed) {
    if matches!(spec.as_str(), "ink" | "react" | "react/jsx-runtime") || local == entry.component || !names.contains(&local) {
      continue;
    }
    match follow(tree, moves, aliases, entry, &spec) {
      Ok(module) => imports.push(format!("import {clause} from \"{module}\";")),
      Err(why) => report.note(&entry.path, entry.line, format!("`{local}` comes from {why}, so `routes/page.loader.ts` does not import it")),
    }
  }
  let mut body = String::new();
  if !params.is_empty() {
    body.push_str(&format!("  const {{ {} }} = params;\n", params.join(", ")));
    report.note(&entry.path, entry.line, format!("the function that rendered took {}, read from `params` in `routes/page.loader.ts`", listed(&params)));
  }
  for range in before {
    let indent = text[..range.start].rsplit('\n').next().map(|l| l.len() - l.trim_start().len()).unwrap_or(0);
    for (i, line) in text[range].lines().enumerate() {
      body.push_str("  ");
      body.push_str(if i == 0 { line.trim_start() } else { line.get(indent..).unwrap_or(line.trim_start()) });
      body.push('\n');
    }
  }
  let fields: Vec<String> = values.iter().map(|(name, value)| if name == value { name.clone() } else { format!("{name}: {value}") }).collect();
  body.push_str(&format!("  return {{ {} }};\n", fields.join(", ")));
  let mut out = String::from("import type { Ctx } from \"@snapfire/fsr\";\n");
  for line in imports {
    out.push_str(&line);
    out.push('\n');
  }
  out.push_str(&format!("\nexport async function load({{ params }}: {}) {{\n{body}}}\n", ctx_type("app/routes/page.loader.ts")));
  Some(out)
}

/// `app/routes/page.tsx` rendering what the entry rendered: the element tree as written when every name it reads is an import of the entry; the root with each prop from `RootProps` when `routes/page.loader.ts` was written (`loaded`); otherwise the root component alone with no props and a note. Each import is rewritten to where its file went; `None` with a note when the root's import cannot be followed.
fn page_for(tree: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, entry: &Entry, kept: bool, loaded: bool, report: &mut Report) -> Option<String> {
  let text = tree.text(&entry.path)?;
  let parsed = parse(&entry.path, text).ok()?;
  let mut imports = imports_of(&parsed);
  if kept {
    imports.push((entry.component.clone(), format!("./{}", entry.path.rsplit('/').next().unwrap_or(&entry.path)), format!("{{ {} }}", entry.component)));
  }
  let follow = |spec: &str, report: &mut Report| -> Result<String, String> {
    let _ = report;
    follow(tree, moves, aliases, entry, spec)
  };
  let Some((_, root_spec, root_clause)) = imports.iter().find(|(local, _, _)| *local == entry.component).cloned() else {
    report.note(&entry.path, entry.line, format!("`render` renders `{}`, which the file does not import, so `routes/page.tsx` was not written", entry.component));
    return None;
  };
  let root_module = match follow(&root_spec, report) {
    Ok(module) => module,
    Err(why) => {
      report.note(&entry.path, entry.line, format!("`render` renders `{}` from {why}, so `routes/page.tsx` was not written", entry.component));
      return None;
    }
  };
  // The tree as written, when every name it reads is an import the build can follow.
  if let Passed::Element { text: element, names, .. } = &entry.passed {
    let needed: Vec<&(String, String, String)> = imports.iter().filter(|(local, _, _)| names.contains(local)).collect();
    let all_imported = names.iter().all(|name| imports.iter().any(|(local, _, _)| local == name));
    let followed: Option<Vec<String>> = all_imported.then(|| needed.iter().map(|(_, spec, clause)| follow(spec, report).ok().map(|module| format!("import {clause} from \"{module}\";"))).collect::<Option<Vec<_>>>()).flatten();
    if let Some(lines) = followed {
      let body = if element.contains('\n') {
        let indent = element.lines().skip(1).filter(|l| !l.trim().is_empty()).map(|l| l.len() - l.trim_start().len()).min().unwrap_or(0);
        let mut out = String::from("  return (\n");
        for (i, line) in element.lines().enumerate() {
          let line = if i == 0 { line.trim_start() } else { line.get(indent..).unwrap_or(line.trim_start()) };
          out.push_str("    ");
          out.push_str(line);
          out.push('\n');
        }
        out.push_str("  );\n");
        out
      } else {
        format!("  return {element};\n")
      };
      return Some(format!("{}\n\nexport default function Page() {{\n{body}}}\n", lines.join("\n")));
    }
  }
  if loaded {
    if let Passed::Element { values, .. } = &entry.passed {
      let names: Vec<String> = values.iter().map(|(name, _)| name.clone()).collect();
      let attrs: Vec<String> = names.iter().map(|name| format!("{name}={{{name}}}")).collect();
      report.note(&entry.path, entry.line, format!("`render` passed `{}` the props {}; `routes/page.loader.ts` returns them from the entry's statements and `routes/page.tsx` gives them to `{}`", entry.component, listed(&names), entry.component));
      return Some(format!("import type {{ RootProps }} from \"@generated/client\";\nimport {root_clause} from \"{root_module}\";\n\nexport default function Page({{ {} }}: RootProps) {{\n  return <{} {} />;\n}}\n", names.join(", "), entry.component, attrs.join(" ")));
    }
  }
  let mut root = entry.component.clone();
  let mut clause = root_clause;
  if root.starts_with(|c: char| c.is_ascii_lowercase()) && clause == root {
    let mut chars = root.chars();
    root = chars.next().map(|c| c.to_ascii_uppercase().to_string() + chars.as_str()).unwrap_or_default();
    clause = root.clone();
  }
  match &entry.passed {
    Passed::Element { props, .. } | Passed::Props(props) if !props.is_empty() => {
      let word = if props.len() == 1 { "prop" } else { "props" };
      report.note(&entry.path, entry.line, format!("`render` passed `{}` the {word} {}, read from the command line; `routes/page.tsx` renders it without them", entry.component, listed(props)));
    }
    Passed::Arguments(n) if *n > 0 => {
      report.note(&entry.path, entry.line, format!("`render` called `{}` with {n} argument{}, read from the command line; `routes/page.tsx` renders it without them", entry.component, if *n == 1 { "" } else { "s" }));
    }
    _ => {}
  }
  Some(format!("import {clause} from \"{root_module}\";\n\nexport default function Page() {{\n  return <{root} />;\n}}\n"))
}
