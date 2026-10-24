//! A `file.stat` result read as FSR answers it: `isFile` and `isDirectory` are
//! fields, so the `s.isFile()` a Node stat was read with is written `s.isFile`.

use std::collections::BTreeSet;

use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Stats)]
}

/// The member of the standard module's `file` that answers a stat.
const STAT: &[&str] = &["stat"];

/// The methods of a Node stat that FSR's `file.stat` answers as fields.
const FIELDS: &[&str] = &["isFile", "isDirectory"];

/// The names of a Node stat FSR's `file.stat` has no field for.
const MISSING: &[&str] = &["isSymbolicLink", "isBlockDevice", "isCharacterDevice", "isFIFO", "isSocket", "mtime", "atime", "ctime", "birthtime", "atimeMs", "ctimeMs", "birthtimeMs", "mode", "uid", "gid", "ino", "dev", "nlink", "rdev", "blksize", "blocks"];

/// `isFile()` and `isDirectory()` on a `stat` result as the fields they are. A result that is passed on or read for a name FSR's `file.stat` has no field for is noted, since `fsr migrate` cannot follow it.
struct Stats;

impl Pass for Stats {
  fn name(&self) -> &'static str {
    "stats"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("stat") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let (functions, namespaces) = imported(&parsed);
      if functions.is_empty() && namespaces.is_empty() {
        continue;
      }
      let mut results = Results { functions: &functions, namespaces: &namespaces, names: BTreeSet::new() };
      parsed.module.visit_with(&mut results);
      let mut uses = Uses { parsed: &parsed, text: &text, functions: &functions, namespaces: &namespaces, results: &results.names, edits: Edits::default(), notes: Vec::new() };
      parsed.module.visit_with(&mut uses);
      let Uses { edits, notes, .. } = uses;
      for (line, note) in notes {
        report.note(&path, line, note);
      }
      let rewritten = edits.apply(&text);
      if rewritten != text {
        tree.insert(path, rewritten);
      }
    }
  }
}

/// The names `parsed` imports a `stat` function under and the names it imports a module that exports one under.
fn imported(parsed: &Parsed) -> (BTreeSet<String>, Vec<(String, &'static [&'static str])>) {
  let mut namespaces = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.src.value.to_string_lossy() != crate::packages::STD {
      continue;
    }
    for specifier in &import.specifiers {
      if let js::ImportSpecifier::Named(n) = specifier {
        let imported = match &n.imported {
          Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
          Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
          None => n.local.sym.to_string(),
        };
        if imported == "file" {
          namespaces.push((n.local.sym.to_string(), STAT));
        }
      }
    }
  }
  (BTreeSet::new(), namespaces)
}

fn bare(e: &js::Expr) -> &js::Expr {
  match e {
    js::Expr::Paren(p) => bare(&p.expr),
    js::Expr::Await(a) => bare(&a.arg),
    js::Expr::TsAs(a) => bare(&a.expr),
    js::Expr::TsNonNull(n) => bare(&n.expr),
    other => other,
  }
}

/// Whether `e`, past any `await` or parentheses, calls a `stat` function.
fn stats(functions: &BTreeSet<String>, namespaces: &[(String, &[&str])], e: &js::Expr) -> bool {
  let js::Expr::Call(call) = bare(e) else { return false };
  let js::Callee::Expr(callee) = &call.callee else { return false };
  match bare(callee) {
    js::Expr::Ident(i) => functions.contains(&*i.sym),
    js::Expr::Member(m) => match (bare(&m.obj), &m.prop) {
      (js::Expr::Ident(object), js::MemberProp::Ident(prop)) => namespaces.iter().any(|(name, exports)| *name == *object.sym && exports.contains(&&*prop.sym)),
      _ => false,
    },
    _ => false,
  }
}

/// The names a file declares a `stat` result under, `const s = await stat(path)`.
struct Results<'a> {
  functions: &'a BTreeSet<String>,
  namespaces: &'a [(String, &'static [&'static str])],
  names: BTreeSet<String>,
}

impl Visit for Results<'_> {
  fn visit_var_declarator(&mut self, n: &js::VarDeclarator) {
    if let (js::Pat::Ident(name), Some(init)) = (&n.name, n.init.as_deref()) {
      if stats(self.functions, self.namespaces, init) {
        self.names.insert(name.id.sym.to_string());
      }
    }
    n.visit_children_with(self);
  }
}

struct Uses<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  functions: &'a BTreeSet<String>,
  namespaces: &'a [(String, &'static [&'static str])],
  results: &'a BTreeSet<String>,
  edits: Edits,
  notes: Vec<(usize, String)>,
}

impl Uses<'_> {
  fn written(&self, span: swc_core::common::Span) -> &str {
    &self.text[self.parsed.range(span)]
  }

  /// Whether `e` is a `stat` result: a call of a `stat` function or a name declared from one.
  fn result(&self, e: &js::Expr) -> bool {
    match bare(e) {
      js::Expr::Ident(i) => self.results.contains(&*i.sym),
      _ => stats(self.functions, self.namespaces, e),
    }
  }

  fn note(&mut self, span: swc_core::common::Span, note: String) {
    self.notes.push((self.parsed.line(span), note));
  }
}

impl Visit for Uses<'_> {
  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_var_declarator(&mut self, n: &js::VarDeclarator) {
    n.init.visit_with(self);
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &n.callee {
      if let js::Expr::Member(m) = &**callee {
        if let js::MemberProp::Ident(prop) = &m.prop {
          if self.result(&m.obj) && FIELDS.contains(&&*prop.sym) && n.args.is_empty() {
            let field = self.written(m.span).to_owned();
            self.edits.replace(self.parsed.range(n.span), field);
            m.obj.visit_with(self);
            return;
          }
        }
      }
    }
    if n.args.iter().any(|arg| matches!(bare(&arg.expr), js::Expr::Ident(i) if self.results.contains(&*i.sym))) {
      let written = self.written(n.span).to_owned();
      self.note(n.span, format!("`{written}`: the `stat` result is passed on, so `fsr migrate` cannot rewrite an `isFile()` or `isDirectory()` call on it to the field FSR's `file.stat` answers"));
    }
    n.visit_children_with(self);
  }

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if let js::MemberProp::Ident(prop) = &n.prop {
      if self.result(&n.obj) && MISSING.contains(&&*prop.sym) {
        let written = self.written(n.span).to_owned();
        self.note(n.span, format!("`{written}`: FSR's `file.stat` answers `isFile`, `isDirectory`, `size` and `mtimeMs` only"));
      }
    }
    n.obj.visit_with(self);
    if let js::MemberProp::Computed(c) = &n.prop {
      c.visit_with(self);
    }
  }
}
