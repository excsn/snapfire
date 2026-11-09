use std::collections::BTreeSet;
use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, BindingPattern, Declaration, ImportDeclarationSpecifier, JSXElementName, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::GetSpan;

use crate::edit::Edits;
use crate::syntax::{import_specifiers, parse, Parsed};

/// `import { name } from "module"` taken out of `text`; `name` alone when
/// the import names more.
pub(crate) fn drop_named_import(text: &str, module: &str, name: &str) -> Option<String> {
  let allocator = Allocator::default();
  let parsed = parse(&allocator, "x.tsx", text).ok()?;
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if import.source.value != module {
      continue;
    }
    let kept: Vec<&ImportDeclarationSpecifier> = import_specifiers(import).iter().filter(|s| local(s) != name).collect();
    if kept.len() == import_specifiers(import).len() {
      continue;
    }
    let mut edits = Edits::default();
    edits.replace(line_range(text, parsed.range(import.span)), if kept.is_empty() { String::new() } else { import_text(text, &parsed, import, &kept) });
    return Some(edits.apply(text));
  }
  None
}

pub(crate) fn local(s: &ImportDeclarationSpecifier) -> String {
  s.local().name.to_string()
}

/// The import `import` written again with only `kept`, ending in a newline.
pub(crate) fn import_text(text: &str, parsed: &Parsed, import: &js::ImportDeclaration, kept: &[&ImportDeclarationSpecifier]) -> String {
  let mut default = None;
  let mut namespace = None;
  let mut named = Vec::new();
  for s in kept {
    match s {
      ImportDeclarationSpecifier::ImportDefaultSpecifier(_) => default = Some(&text[parsed.range(s.span())]),
      ImportDeclarationSpecifier::ImportNamespaceSpecifier(_) => namespace = Some(&text[parsed.range(s.span())]),
      ImportDeclarationSpecifier::ImportSpecifier(_) => named.push(&text[parsed.range(s.span())]),
    }
  }
  let mut clauses = Vec::new();
  clauses.extend(default.map(str::to_owned));
  clauses.extend(namespace.map(str::to_owned));
  if !named.is_empty() {
    clauses.push(format!("{{ {} }}", named.join(", ")));
  }
  let keyword = if import.import_kind.is_type() { "import type" } else { "import" };
  format!("{keyword} {} from {};\n", clauses.join(", "), &text[parsed.range(import.source.span)])
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

pub(crate) fn describe(item: &Statement, parsed: &Parsed) -> Item {
  let range = parsed.range(item.span());
  let mut reads = Reads { names: BTreeSet::new() };
  let mut binds = Vec::new();
  let mut data = None;
  let mut import = false;
  let decl_binds = |decl: &Declaration, binds: &mut Vec<String>| match decl {
    Declaration::FunctionDeclaration(f) => binds.extend(f.id.as_ref().map(|id| id.name.to_string())),
    Declaration::ClassDeclaration(c) => binds.extend(c.id.as_ref().map(|id| id.name.to_string())),
    Declaration::VariableDeclaration(v) => {
      for d in &v.declarations {
        let mut names = Vec::new();
        pat_names(&d.id, &mut names);
        binds.extend(names);
      }
    }
    Declaration::TSInterfaceDeclaration(i) => binds.push(i.id.name.to_string()),
    Declaration::TSTypeAliasDeclaration(t) => binds.push(t.id.name.to_string()),
    Declaration::TSEnumDeclaration(e) => binds.push(e.id.name.to_string()),
    _ => {}
  };
  match item {
    Statement::ImportDeclaration(i) => {
      import = true;
      binds.extend(import_specifiers(i).iter().map(local));
    }
    Statement::ExportDeclaration(e) => {
      decl_binds(&e.declaration, &mut binds);
      data = binds.iter().find_map(|b| crate::pages::DATA_FUNCTIONS.iter().find(|d| **d == b.as_str()).copied());
      reads.visit_declaration(&e.declaration);
    }
    other => match other.as_declaration() {
      Some(d) => {
        decl_binds(d, &mut binds);
        reads.visit_declaration(d);
      }
      None => reads.visit_statement(other),
    },
  }
  Item { range, binds, reads: reads.names, data, import }
}

pub(crate) fn pat_names(pat: &BindingPattern, out: &mut Vec<String>) {
  match pat {
    BindingPattern::BindingIdentifier(i) => out.push(i.name.to_string()),
    BindingPattern::ArrayPattern(a) => {
      a.elements.iter().flatten().for_each(|p| pat_names(p, out));
      if let Some(rest) = &a.rest {
        pat_names(&rest.argument, out);
      }
    }
    BindingPattern::ObjectPattern(o) => {
      for p in &o.properties {
        pat_names(&p.value, out);
      }
      if let Some(rest) = &o.rest {
        pat_names(&rest.argument, out);
      }
    }
    BindingPattern::AssignmentPattern(a) => pat_names(&a.left, out),
  }
}

/// The names an item reads or binds: identifiers, the name of a JSX element and the name of a referenced type.
pub(crate) struct Reads {
  pub names: BTreeSet<String>,
}

impl<'a> Visit<'a> for Reads {
  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    self.names.insert(n.name.to_string());
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    self.names.insert(n.name.to_string());
  }

  fn visit_jsx_element_name(&mut self, n: &JSXElementName<'a>) {
    if let JSXElementName::Identifier(i) = n {
      self.names.insert(i.name.to_string());
    }
    walk::walk_jsx_element_name(self, n);
  }
}

/// `text` without the module-level declarations and import specifiers that
/// bind a name in `removable` nothing else in the module reads, repeated
/// until none is left. A name outside `removable` is never taken out, so a
/// declaration the author left unread stays.
pub(crate) fn prune(path: &str, text: &str, removable: &BTreeSet<String>) -> String {
  let mut text = text.to_owned();
  loop {
    let allocator = Allocator::default();
    let Ok(parsed) = parse(&allocator, path, &text) else { return text };
    let items: Vec<Item> = parsed.body().iter().map(|item| describe(item, &parsed)).collect();
    let read_by_others = |n: usize, name: &String| items.iter().enumerate().any(|(m, other)| m != n && !other.import && other.reads.contains(name));
    let mut edits = Edits::default();
    for (n, item) in items.iter().enumerate() {
      if item.import || item.binds.is_empty() || item.data.is_some() {
        continue;
      }
      if item.binds.iter().all(|b| removable.contains(b) && !read_by_others(n, b)) && !exported(&parsed.body()[n]) {
        edits.replace(line_range(&text, item.range.clone()), "");
      }
    }
    for (n, item) in parsed.body().iter().enumerate() {
      let Statement::ImportDeclaration(import) = item else { continue };
      let specifiers = import_specifiers(import);
      if specifiers.is_empty() {
        continue;
      }
      let kept: Vec<&ImportDeclarationSpecifier> = specifiers.iter().filter(|s| !removable.contains(&local(s)) || read_by_others(n, &local(s))).collect();
      if kept.len() < specifiers.len() {
        edits.replace(line_range(&text, parsed.range(import.span)), if kept.is_empty() { String::new() } else { import_text(&text, &parsed, import, &kept) });
      }
    }
    if edits.is_empty() {
      return text;
    }
    text = edits.apply(&text);
  }
}

fn exported(item: &Statement) -> bool {
  item.is_module_declaration()
}
