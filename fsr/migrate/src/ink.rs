//! An Ink application: its sources under `app/src/`, a `routes/page.tsx`
//! that renders the component the entry rendered and every import written as
//! an FSR application writes it.

use std::collections::BTreeMap;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

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
    let mut moves = BTreeMap::new();
    for path in tree.paths() {
      if entry.as_ref().is_some_and(|e| e.path == path) {
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
    let page = match &entry {
      Some(entry) => page_for(tree, &moves, &aliases, entry, report),
      None => {
        report.note("package.json", 0, "no file imports `render` from `ink`, so `routes/page.tsx` was not written");
        None
      }
    };
    *tree = relocate(tree, &moves, &aliases, report);
    if let Some(page) = page {
      tree.insert("app/routes/page.tsx", page);
    }
  }
}

/// The file that calls Ink's `render` and what it rendered.
struct Entry {
  path: String,
  line: usize,
  /// The component rendered, by the name the entry holds it under.
  component: String,
  /// What `render` passed the component: prop names from an element, or the count of arguments from a call.
  passed: Passed,
}

enum Passed {
  Props(Vec<String>),
  Arguments(usize),
}

fn entry(tree: &Tree, root: &str) -> Option<Entry> {
  let mut found = None;
  for path in tree.paths().filter(|p| is_script(p) && (p.starts_with(root) || !p.contains('/'))) {
    let Some(text) = tree.text(path) else { continue };
    let Ok(parsed) = parse(path, text) else { continue };
    let mut calls = Renders { render: None, found: None, parsed: &parsed };
    parsed.module.visit_with(&mut calls);
    if let Some((line, component, passed)) = calls.found {
      found = Some(Entry { path: path.to_owned(), line, component, passed });
      break;
    }
  }
  found
}

struct Renders<'a> {
  parsed: &'a crate::syntax::Parsed,
  /// The local name `render` from `ink` is imported under.
  render: Option<String>,
  found: Option<(usize, String, Passed)>,
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
            self.found = rendered(&arg.expr).map(|(component, passed)| (self.parsed.line(n.span()), component, passed));
          }
        }
      }
    }
    n.visit_children_with(self);
  }
}

/// The component `render`'s argument renders: `<Name …/>` or `Name(…)`.
fn rendered(e: &js::Expr) -> Option<(String, Passed)> {
  match e {
    js::Expr::Paren(p) => rendered(&p.expr),
    js::Expr::JSXElement(el) => {
      let js::JSXElementName::Ident(name) = &el.opening.name else { return None };
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
      Some((name.sym.to_string(), Passed::Props(props)))
    }
    js::Expr::Call(call) => match &call.callee {
      js::Callee::Expr(callee) => match &**callee {
        js::Expr::Ident(id) => Some((id.sym.to_string(), Passed::Arguments(call.args.len()))),
        _ => None,
      },
      _ => None,
    },
    _ => None,
  }
}

/// `app/routes/page.tsx` rendering `entry.component` with no props, imported from where the entry imported it; `None` with a note when that import cannot be followed.
fn page_for(tree: &Tree, moves: &BTreeMap<String, String>, aliases: &Aliases, entry: &Entry, report: &mut Report) -> Option<String> {
  let text = tree.text(&entry.path)?;
  let parsed = parse(&entry.path, text).ok()?;
  let mut import = None;
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(decl)) = item else { continue };
    for spec in &decl.specifiers {
      let (local, clause) = match spec {
        js::ImportSpecifier::Default(d) => (d.local.sym.to_string(), entry.component.clone()),
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
        js::ImportSpecifier::Namespace(_) => continue,
      };
      if local == entry.component {
        import = Some((decl.src.value.to_string_lossy().into_owned(), clause));
      }
    }
  }
  let Some((spec, clause)) = import else {
    report.note(&entry.path, entry.line, format!("`render` renders `{}`, which the file does not import, so `routes/page.tsx` was not written", entry.component));
    return None;
  };
  let relative_spec = spec.starts_with("./") || spec.starts_with("../");
  let candidates: Vec<String> = if relative_spec { normalize(dirname(&entry.path), &spec).into_iter().collect() } else { aliases.candidates(&spec) };
  let Some((target, _)) = candidates.iter().find_map(|c| find(tree, c)) else {
    report.note(&entry.path, entry.line, format!("`render` renders `{}` from `{spec}`, which the build cannot follow, so `routes/page.tsx` was not written", entry.component));
    return None;
  };
  let Some(moved) = moves.get(&target).and_then(|m| m.strip_prefix("app/src/")) else {
    report.note(&entry.path, entry.line, format!("`render` renders `{}` from `{target}`, which is not under the sources, so `routes/page.tsx` was not written", entry.component));
    return None;
  };
  let mut module = moved.to_owned();
  if let Some(ext) = SCRIPT_EXTENSIONS.iter().find(|ext| module.ends_with(*ext)) {
    module.truncate(module.len() - ext.len());
  }
  if let Some(dir) = module.strip_suffix("/index") {
    module = dir.to_owned();
  }
  match &entry.passed {
    Passed::Props(props) if !props.is_empty() => {
      let names: Vec<String> = props.iter().map(|p| format!("`{p}`")).collect();
      let (word, list) = match names.len() {
        1 => ("prop", names[0].clone()),
        n => ("props", format!("{} and {}", names[..n - 1].join(", "), names[n - 1])),
      };
      report.note(&entry.path, entry.line, format!("`render` passed `{}` the {word} {list}, read from the command line; `routes/page.tsx` renders it without them", entry.component));
    }
    Passed::Arguments(n) if *n > 0 => {
      report.note(&entry.path, entry.line, format!("`render` called `{}` with {n} argument{}, read from the command line; `routes/page.tsx` renders it without them", entry.component, if *n == 1 { "" } else { "s" }));
    }
    _ => {}
  }
  Some(format!("import {clause} from \"@src/{module}\";\n\nexport default function Page() {{\n  return <{} />;\n}}\n", entry.component))
}
