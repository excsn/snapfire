//! Extraction: the state, the handlers and the effects a page or a layout
//! holds, moved with the smallest part of its markup that uses them into an
//! island module beside it, so that the page itself is composition.

use std::collections::BTreeSet;
use std::ops::Range;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::component::{find_function, Found};
use crate::Parsed;

/// A page with its browser half moved out.
pub struct Extraction {
  /// The page's source as composition, placing the island where the moved markup stood.
  pub page: String,
  /// The island module's file, beside the page's.
  pub island_file: String,
  pub island: String,
  /// The names the island took from the page's body, for the report.
  pub holds: Vec<String>,
}

/// What a page that cannot be split keeps: the reason, for the report.
pub struct Kept(pub String);

/// The name the page places the island under.
const PLACED: &str = "SfIsland";

/// Hooks whose bindings only a framework keeps. `useLocale` and `useMemo`
/// lower to values the server computes.
fn browser_hook(name: &str) -> bool {
  name.len() > 3 && name.starts_with("use") && name[3..].starts_with(|c: char| c.is_ascii_uppercase()) && !matches!(name, "useLocale" | "useMemo")
}

fn hook_name(expr: &js::Expr) -> Option<&str> {
  let js::Expr::Call(call) = expr else { return None };
  let js::Callee::Expr(callee) = &call.callee else { return None };
  match &**callee {
    js::Expr::Ident(id) => Some(id.sym.as_ref()),
    js::Expr::Member(member) => match &member.prop {
      js::MemberProp::Ident(prop) => Some(prop.sym.as_ref()),
      _ => None,
    },
    _ => None,
  }
}

fn bound(pat: &js::Pat, out: &mut Vec<String>) {
  match pat {
    js::Pat::Ident(id) => out.push(id.id.sym.to_string()),
    js::Pat::Array(array) => array.elems.iter().flatten().for_each(|p| bound(p, out)),
    js::Pat::Object(object) => {
      for prop in &object.props {
        match prop {
          js::ObjectPatProp::KeyValue(kv) => bound(&kv.value, out),
          js::ObjectPatProp::Assign(assign) => out.push(assign.key.id.sym.to_string()),
          js::ObjectPatProp::Rest(rest) => bound(&rest.arg, out),
        }
      }
    }
    js::Pat::Assign(assign) => bound(&assign.left, out),
    js::Pat::Rest(rest) => bound(&rest.arg, out),
    _ => {}
  }
}

/// Every identifier a node reads or binds, which is a superset of what it reads.
#[derive(Default)]
struct Names(BTreeSet<String>);

impl Visit for Names {
  fn visit_ident(&mut self, ident: &js::Ident) {
    self.0.insert(ident.sym.to_string());
  }
}

fn names_of<N: VisitWith<Names>>(node: &N) -> BTreeSet<String> {
  let mut names = Names::default();
  node.visit_with(&mut names);
  names.0
}

fn is_event(attr: &js::JSXAttr) -> bool {
  let js::JSXAttrName::Ident(name) = &attr.name else { return false };
  let name = name.sym.as_ref();
  let event = name.len() > 2 && name.starts_with("on") && name[2..].starts_with(|c: char| c.is_ascii_uppercase());
  event && matches!(&attr.value, Some(js::JSXAttrValue::JSXExprContainer(_)))
}

/// Where in the markup the browser half is used: for each use, the elements
/// around it from the outermost in, each with whether a function inside the
/// markup encloses it, whose parameters the island would not have.
struct Uses<'a> {
  browser: &'a BTreeSet<String>,
  stack: Vec<(Span, bool)>,
  depth: usize,
  found: Vec<Vec<(Span, bool)>>,
}

impl Visit for Uses<'_> {
  fn visit_jsx_element(&mut self, el: &js::JSXElement) {
    self.stack.push((el.span, self.depth > 0));
    el.visit_children_with(self);
    self.stack.pop();
  }

  fn visit_jsx_attr(&mut self, attr: &js::JSXAttr) {
    if is_event(attr) {
      self.found.push(self.stack.clone());
    }
    attr.visit_children_with(self);
  }

  fn visit_ident(&mut self, ident: &js::Ident) {
    if self.browser.contains(ident.sym.as_ref()) {
      self.found.push(self.stack.clone());
    }
  }

  fn visit_arrow_expr(&mut self, arrow: &js::ArrowExpr) {
    self.depth += 1;
    arrow.visit_children_with(self);
    self.depth -= 1;
  }

  fn visit_function(&mut self, function: &js::Function) {
    self.depth += 1;
    function.visit_children_with(self);
    self.depth -= 1;
  }
}

/// A statement of the component's body and what it binds.
struct Line<'a> {
  stmt: &'a js::Stmt,
  names: Vec<String>,
  browser: bool,
  function: bool,
}

/// Splits `file`'s default export, a page or a layout, into composition and
/// an island. `None` when it holds nothing a framework has to run. `slots` are
/// the layout's slot props, which composition fills and an island cannot take.
pub(crate) fn extract(parsed: &Parsed, file: &str, slots: &[String]) -> Result<Option<Extraction>, Kept> {
  let Some(source) = parsed.cm.files().first().map(|f| f.src.to_string()) else { return Ok(None) };
  let Some(found) = find_function(parsed, "default") else { return Ok(None) };
  let (params, stmts, ret): (Vec<js::Pat>, &[js::Stmt], &js::Expr) = match &found {
    Found::Declared(params, stmts, _) => {
      let Some(ret) = stmts.iter().rev().find_map(|s| match s {
        js::Stmt::Return(ret) => ret.arg.as_deref(),
        _ => None,
      }) else {
        return Ok(None);
      };
      (params.clone(), stmts, ret)
    }
    Found::Arrow(arrow) => match &*arrow.body {
      js::ArrowFunctionBody::Expr(e) => (arrow.params.clone(), &[][..], &**e),
      js::ArrowFunctionBody::FunctionBody(body) => {
        let Some(ret) = body.stmts.iter().rev().find_map(|s| match s {
          js::Stmt::Return(ret) => ret.arg.as_deref(),
          _ => None,
        }) else {
          return Ok(None);
        };
        (arrow.params.clone(), &body.stmts[..], ret)
      }
    },
  };

  let mut lines: Vec<Line> = Vec::new();
  for stmt in stmts {
    match stmt {
      js::Stmt::Return(_) => break,
      js::Stmt::Decl(js::Decl::Var(var)) => {
        let mut names = Vec::new();
        let mut browser = false;
        let mut function = false;
        for decl in &var.decls {
          bound(&decl.name, &mut names);
          match decl.init.as_deref() {
            Some(init) if hook_name(init).is_some_and(browser_hook) => browser = true,
            Some(js::Expr::Arrow(_) | js::Expr::Fn(_)) => function = true,
            _ => {}
          }
        }
        lines.push(Line { stmt, names, browser, function });
      }
      js::Stmt::Decl(js::Decl::Fn(f)) => lines.push(Line { stmt, names: vec![f.ident.sym.to_string()], browser: false, function: true }),
      js::Stmt::Expr(e) if hook_name(&e.expr).is_some_and(browser_hook) => lines.push(Line { stmt, names: Vec::new(), browser: true, function: false }),
      other => lines.push(Line { stmt: other, names: Vec::new(), browser: false, function: false }),
    }
  }
  let early = stmts.iter().any(|s| matches!(s, js::Stmt::If(_)));

  // A function an event attribute names runs in the browser, and so does
  // anything that reads what the browser holds.
  let mut handlers = Names::default();
  {
    struct Events<'a>(&'a mut Names);
    impl Visit for Events<'_> {
      fn visit_jsx_attr(&mut self, attr: &js::JSXAttr) {
        if is_event(attr) {
          attr.value.visit_with(&mut *self.0);
        }
      }
    }
    ret.visit_with(&mut Events(&mut handlers));
  }
  let mut browser: BTreeSet<String> = BTreeSet::new();
  loop {
    let mut grew = false;
    for line in &mut lines {
      if line.browser {
        continue;
      }
      let reads = names_of(line.stmt);
      let named = line.function && line.names.iter().any(|n| handlers.0.contains(n));
      if named || reads.iter().any(|n| browser.contains(n) && !line.names.contains(n)) {
        line.browser = true;
      }
    }
    for line in lines.iter().filter(|l| l.browser) {
      for name in &line.names {
        grew |= browser.insert(name.clone());
      }
    }
    if !grew {
      break;
    }
  }
  let effects = lines.iter().any(|l| l.browser && l.names.is_empty());
  let mut uses = Uses { browser: &browser, stack: Vec::new(), depth: 0, found: Vec::new() };
  ret.visit_with(&mut uses);
  if uses.found.is_empty() && !effects {
    return Ok(None);
  }

  // The unit: the deepest element every use sits in and no function inside
  // the markup encloses; the whole tree when there is none or when an early
  // return chooses between trees.
  let mut unit: Option<Span> = None;
  if !early && !uses.found.is_empty() {
    let first = &uses.found[0];
    let common = (0..first.len()).take_while(|&i| uses.found.iter().all(|stack| stack.get(i).map(|e| e.0) == Some(first[i].0))).count();
    unit = first[..common].iter().rev().find(|(_, inner)| !inner).map(|(span, _)| *span);
  }
  let range_of = |span: Span| parsed.range(span);
  let unit_range: Range<usize> = match (unit, &found) {
    (Some(span), _) => range_of(span),
    (None, _) if early => match &found {
      Found::Declared(_, stmts, _) => range_of(Span::new(stmts.first().map(|s| s.span().lo).unwrap_or_default(), stmts.last().map(|s| s.span().hi).unwrap_or_default())),
      Found::Arrow(arrow) => range_of(arrow.body.span()),
    },
    (None, _) => range_of(ret.span()),
  };
  let whole_body = unit.is_none() && early;

  let moved: Vec<&Line> = lines.iter().filter(|l| l.browser || whole_body).collect();
  let unit_text = &source[unit_range.clone()];
  let moved_text: Vec<&str> = moved.iter().map(|l| &source[range_of(l.stmt.span())]).collect();

  let mut scope: Vec<String> = Vec::new();
  for param in &params {
    bound(param, &mut scope);
  }
  for line in lines.iter().filter(|l| !l.browser && !whole_body) {
    scope.extend(line.names.iter().cloned());
  }
  let mut reads = BTreeSet::new();
  {
    let mut names = Names::default();
    for line in &moved {
      line.stmt.visit_with(&mut names);
    }
    if whole_body {
      if let Found::Declared(_, stmts, _) = &found {
        for stmt in stmts.iter() {
          stmt.visit_with(&mut names);
        }
      }
    } else {
      struct Within<'a> {
        range: Range<usize>,
        parsed: &'a Parsed,
        names: &'a mut Names,
      }
      impl Visit for Within<'_> {
        fn visit_ident(&mut self, ident: &js::Ident) {
          let at = self.parsed.range(ident.span);
          if at.start >= self.range.start && at.end <= self.range.end {
            self.names.0.insert(ident.sym.to_string());
          }
        }
      }
      ret.visit_with(&mut Within { range: unit_range.clone(), parsed, names: &mut names });
    }
    reads.extend(names.0);
  }
  let props: Vec<String> = scope.iter().filter(|n| reads.contains(*n)).cloned().collect::<BTreeSet<_>>().into_iter().collect();
  if let Some(slot) = props.iter().find(|p| slots.contains(p)) {
    return Err(Kept(format!("its state reaches the slot `{slot}`, which composition fills and an island cannot take")));
  }

  // The module's own imports and every module-level item the island reads,
  // the component itself aside.
  let own = default_function_name(parsed);
  let mut items: Vec<(Range<usize>, Vec<String>)> = Vec::new();
  let mut imports: Vec<Range<usize>> = Vec::new();
  for item in &parsed.module.body {
    match item {
      js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) => imports.push(range_of(import.span)),
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(_) | js::ModuleDecl::ExportDefaultExpr(_)) => {}
      js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) => items.push((range_of(export.span), decl_names(&export.decl))),
      js::ModuleItem::Stmt(js::Stmt::Decl(decl)) => items.push((range_of(decl.span()), decl_names(decl))),
      _ => {}
    }
  }
  items.retain(|(_, names)| own.as_ref().is_none_or(|own| !names.contains(own)));
  let mut wanted: BTreeSet<String> = reads.clone();
  let mut taken = vec![false; items.len()];
  loop {
    let mut grew = false;
    for (i, (range, names)) in items.iter().enumerate() {
      if !taken[i] && names.iter().any(|n| wanted.contains(n)) {
        taken[i] = true;
        grew = true;
        let text = &source[range.clone()];
        if let Ok(parsed) = crate::parse_with(file, text, true) {
          wanted.extend(names_of(&parsed.module));
        }
      }
    }
    if !grew {
      break;
    }
  }

  let stem = file.rsplit_once('/').map(|(_, name)| name).unwrap_or(file);
  let stem = stem.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(stem);
  let dir = file.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
  let island_file = format!("{dir}{stem}.island0.tsx");
  let name = format!("{}Island", own.as_deref().unwrap_or("Page"));
  let takes_children = props.iter().any(|p| p == "children");
  let passed: Vec<&String> = props.iter().filter(|p| *p != "children").collect();

  let mut island = String::new();
  for range in &imports {
    island.push_str(&source[range.clone()]);
    island.push('\n');
  }
  for (i, (range, _)) in items.iter().enumerate() {
    if taken[i] {
      island.push_str(&source[range.clone()]);
      island.push('\n');
    }
  }
  let destructured = props.join(", ");
  island.push_str(&format!("\nexport default function {name}({{ {destructured} }}: any) {{\n"));
  if whole_body {
    island.push_str("  ");
    island.push_str(unit_text);
    island.push('\n');
  } else {
    for text in &moved_text {
      island.push_str("  ");
      island.push_str(text);
      island.push('\n');
    }
    island.push_str("  return (\n    ");
    island.push_str(unit_text);
    island.push_str("\n  );\n");
  }
  island.push_str("}\n");

  let attrs: String = passed.iter().map(|p| format!(" {p}={{{p}}}")).collect();
  let placement = match takes_children {
    true => format!("<{PLACED}{attrs}>{{children}}</{PLACED}>"),
    false => format!("<{PLACED}{attrs} />"),
  };
  let mut edits: Vec<(Range<usize>, String)> = Vec::new();
  if whole_body {
    edits.push((unit_range, format!("return {placement};")));
  } else {
    edits.push((unit_range, placement));
    for line in &moved {
      edits.push((range_of(line.stmt.span()), String::new()));
    }
  }
  edits.sort_by(|a, b| b.0.start.cmp(&a.0.start));
  let mut page = source.clone();
  for (range, text) in edits {
    page.replace_range(range, &text);
  }
  page.insert_str(0, &format!("import {PLACED} from \"./{stem}.island0.tsx\";\n"));

  let holds: Vec<String> = moved.iter().flat_map(|l| l.names.iter().cloned()).collect();
  Ok(Some(Extraction { page, island_file, island, holds }))
}

fn decl_names(decl: &js::Decl) -> Vec<String> {
  let mut out = Vec::new();
  match decl {
    js::Decl::Fn(f) => out.push(f.ident.sym.to_string()),
    js::Decl::Class(c) => out.push(c.ident.sym.to_string()),
    js::Decl::Var(var) => var.decls.iter().for_each(|d| bound(&d.name, &mut out)),
    js::Decl::TsInterface(i) => out.push(i.id.sym.to_string()),
    js::Decl::TsTypeAlias(t) => out.push(t.id.sym.to_string()),
    js::Decl::TsEnum(e) => out.push(e.id.sym.to_string()),
    _ => {}
  }
  out
}

/// The name `export default function Name` gives the component, which the island is named after.
fn default_function_name(parsed: &Parsed) -> Option<String> {
  parsed.module.body.iter().find_map(|item| match item {
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultDecl(d)) => match &d.decl {
      js::DefaultDecl::Fn(f) => f.ident.as_ref().map(|id| id.sym.to_string()),
      _ => None,
    },
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDefaultExpr(e)) => match &*e.expr {
      js::Expr::Ident(id) => Some(id.sym.to_string()),
      _ => None,
    },
    _ => None,
  })
}
