use std::collections::BTreeSet;
use std::ops::Range;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::syntax::{parse, Parsed};

/// `import { name } from "module"` taken out of `text`; `name` alone when
/// the import names more.
pub(crate) fn drop_named_import(text: &str, module: &str, name: &str) -> Option<String> {
  let parsed = parse("x.tsx", text).ok()?;
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.src.value.to_string_lossy() != module {
      continue;
    }
    let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| local(s) != name).collect();
    if kept.len() == import.specifiers.len() {
      continue;
    }
    let mut edits = Edits::default();
    edits.replace(line_range(text, parsed.range(import.span)), if kept.is_empty() { String::new() } else { import_text(text, &parsed, import, &kept) });
    return Some(edits.apply(text));
  }
  None
}

pub(crate) fn local(s: &js::ImportSpecifier) -> String {
  match s {
    js::ImportSpecifier::Named(n) => n.local.sym.to_string(),
    js::ImportSpecifier::Default(d) => d.local.sym.to_string(),
    js::ImportSpecifier::Namespace(n) => n.local.sym.to_string(),
  }
}

/// The import `import` written again with only `kept`, ending in a newline.
pub(crate) fn import_text(text: &str, parsed: &Parsed, import: &js::ImportDecl, kept: &[&js::ImportSpecifier]) -> String {
  let mut default = None;
  let mut namespace = None;
  let mut named = Vec::new();
  for s in kept {
    match s {
      js::ImportSpecifier::Default(_) => default = Some(&text[parsed.range(s.span())]),
      js::ImportSpecifier::Namespace(_) => namespace = Some(&text[parsed.range(s.span())]),
      js::ImportSpecifier::Named(_) => named.push(&text[parsed.range(s.span())]),
    }
  }
  let mut clauses = Vec::new();
  clauses.extend(default.map(str::to_owned));
  clauses.extend(namespace.map(str::to_owned));
  if !named.is_empty() {
    clauses.push(format!("{{ {} }}", named.join(", ")));
  }
  let keyword = if import.type_only { "import type" } else { "import" };
  format!("{keyword} {} from {};\n", clauses.join(", "), &text[parsed.range(import.src.span)])
}

/// `range` widened to the whole lines it sits on, newline included, when
/// nothing else shares them.
pub(crate) fn line_range(text: &str, range: Range<usize>) -> Range<usize> {
  let start = text[..range.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
  let end = text[range.end..].find('\n').map(|i| range.end + i + 1).unwrap_or(text.len());
  if text[start..range.start].trim().is_empty() && text[range.end..end].trim().is_empty() {
    start..end
  } else {
    range
  }
}

/// `text` with no blank lines before its first line, no run of more than
/// one blank line and one newline at the end, for a module a pass took
/// declarations out of.
pub(crate) fn tidy(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  let mut blank = 0;
  for line in text.lines() {
    if line.trim().is_empty() {
      blank += 1;
      continue;
    }
    if blank > 0 && !out.is_empty() {
      out.push('\n');
    }
    blank = 0;
    out.push_str(line);
    out.push('\n');
  }
  out
}

/// A top-level item: its range, the names it binds and the names it reads.
pub(crate) struct Item {
  pub range: Range<usize>,
  pub binds: Vec<String>,
  pub reads: BTreeSet<String>,
  /// The Next data function the item exports, when it is one.
  pub data: Option<&'static str>,
  pub import: bool,
}

pub(crate) fn describe(item: &js::ModuleItem, parsed: &Parsed) -> Item {
  let range = parsed.range(item.span());
  let mut reads = Reads { names: BTreeSet::new() };
  let mut binds = Vec::new();
  let mut data = None;
  let mut import = false;
  let decl_binds = |decl: &js::Decl, binds: &mut Vec<String>| match decl {
    js::Decl::Fn(f) => binds.push(f.ident.sym.to_string()),
    js::Decl::Class(c) => binds.push(c.ident.sym.to_string()),
    js::Decl::Var(v) => {
      for d in &v.decls {
        let mut names = Vec::new();
        pat_names(&d.name, &mut names);
        binds.extend(names);
      }
    }
    js::Decl::TsInterface(i) => binds.push(i.id.sym.to_string()),
    js::Decl::TsTypeAlias(t) => binds.push(t.id.sym.to_string()),
    js::Decl::TsEnum(e) => binds.push(e.id.sym.to_string()),
    _ => {}
  };
  match item {
    js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(i)) => {
      import = true;
      binds.extend(i.specifiers.iter().map(local));
    }
    js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(e)) => {
      decl_binds(&e.decl, &mut binds);
      data = binds.iter().find_map(|b| crate::pages::DATA_FUNCTIONS.iter().find(|d| **d == b.as_str()).copied());
      e.decl.visit_with(&mut reads);
    }
    js::ModuleItem::Stmt(js::Stmt::Decl(d)) => {
      decl_binds(d, &mut binds);
      d.visit_with(&mut reads);
    }
    other => other.visit_with(&mut reads),
  }
  Item { range, binds, reads: reads.names, data, import }
}

pub(crate) fn pat_names(pat: &js::Pat, out: &mut Vec<String>) {
  match pat {
    js::Pat::Ident(i) => out.push(i.id.sym.to_string()),
    js::Pat::Array(a) => a.elems.iter().flatten().for_each(|p| pat_names(p, out)),
    js::Pat::Object(o) => {
      for p in &o.props {
        match p {
          js::ObjectPatProp::Assign(a) => out.push(a.key.sym.to_string()),
          js::ObjectPatProp::KeyValue(kv) => pat_names(&kv.value, out),
          js::ObjectPatProp::Rest(r) => pat_names(&r.arg, out),
        }
      }
    }
    js::Pat::Rest(r) => pat_names(&r.arg, out),
    js::Pat::Assign(a) => pat_names(&a.left, out),
    _ => {}
  }
}

pub(crate) struct Reads {
  pub names: BTreeSet<String>,
}

impl Visit for Reads {
  fn visit_ident(&mut self, n: &js::Ident) {
    self.names.insert(n.sym.to_string());
  }

  fn visit_jsx_element_name(&mut self, n: &js::JSXElementName) {
    if let js::JSXElementName::Ident(i) = n {
      self.names.insert(i.sym.to_string());
    }
    n.visit_children_with(self);
  }

  fn visit_ts_type_ref(&mut self, n: &js::TsTypeRef) {
    if let js::TsEntityName::Ident(i) = &n.type_name {
      self.names.insert(i.sym.to_string());
    }
    n.visit_children_with(self);
  }
}

/// `text` without the module-level declarations and import specifiers that
/// bind a name in `removable` nothing else in the module reads, repeated
/// until none is left. A name outside `removable` is never taken out, so a
/// declaration the author left unread stays.
pub(crate) fn prune(path: &str, text: &str, removable: &BTreeSet<String>) -> String {
  let mut text = text.to_owned();
  loop {
    let Ok(parsed) = parse(path, &text) else { return text };
    let items: Vec<Item> = parsed.module.body.iter().map(|item| describe(item, &parsed)).collect();
    let read_by_others = |n: usize, name: &String| items.iter().enumerate().any(|(m, other)| m != n && !other.import && other.reads.contains(name));
    let mut edits = Edits::default();
    for (n, item) in items.iter().enumerate() {
      if item.import || item.binds.is_empty() || item.data.is_some() {
        continue;
      }
      if item.binds.iter().all(|b| removable.contains(b) && !read_by_others(n, b)) && !exported(&parsed.module.body[n]) {
        edits.replace(line_range(&text, item.range.clone()), "");
      }
    }
    for (n, item) in parsed.module.body.iter().enumerate() {
      let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
      if import.specifiers.is_empty() {
        continue;
      }
      let kept: Vec<&js::ImportSpecifier> = import.specifiers.iter().filter(|s| !removable.contains(&local(s)) || read_by_others(n, &local(s))).collect();
      if kept.len() < import.specifiers.len() {
        edits.replace(line_range(&text, parsed.range(import.span)), if kept.is_empty() { String::new() } else { import_text(&text, &parsed, import, &kept) });
      }
    }
    if edits.is_empty() {
      return text;
    }
    text = edits.apply(&text);
  }
}

fn exported(item: &js::ModuleItem) -> bool {
  matches!(item, js::ModuleItem::ModuleDecl(_))
}
