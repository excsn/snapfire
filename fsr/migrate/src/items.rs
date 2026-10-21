use std::ops::Range;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

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
