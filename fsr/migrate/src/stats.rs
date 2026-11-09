//! A `file.stat` result read as FSR answers it: `isFile` and `isDirectory` are
//! fields, so the `s.isFile()` a Node stat was read with is written `s.isFile`.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, BindingPattern, Expression, ImportDeclarationSpecifier, ModuleExportName, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::Span;

use crate::edit::Edits;
use crate::syntax::{argument, is_script, lossy, parse, Parsed};
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
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let (functions, namespaces) = imported(&parsed);
      if functions.is_empty() && namespaces.is_empty() {
        continue;
      }
      let mut results = Results { functions: &functions, namespaces: &namespaces, names: BTreeSet::new() };
      results.visit_program(&parsed.program);
      let mut uses = Uses { parsed: &parsed, text: &text, functions: &functions, namespaces: &namespaces, results: &results.names, edits: Edits::default(), notes: Vec::new() };
      uses.visit_program(&parsed.program);
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
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if !crate::packages::is_std_source(&lossy(import.source.value)) {
      continue;
    }
    for specifier in import.specifiers.iter().flatten() {
      if let ImportDeclarationSpecifier::ImportSpecifier(n) = specifier {
        let imported = match &n.imported {
          ModuleExportName::IdentifierName(i) => i.name.to_string(),
          ModuleExportName::IdentifierReference(i) => i.name.to_string(),
          ModuleExportName::StringLiteral(s) => lossy(s.value),
        };
        if imported == "file" {
          namespaces.push((n.local.name.to_string(), STAT));
        }
      }
    }
  }
  (BTreeSet::new(), namespaces)
}

fn bare<'e, 'a>(e: &'e Expression<'a>) -> &'e Expression<'a> {
  match e {
    Expression::ParenthesizedExpression(p) => bare(&p.expression),
    Expression::AwaitExpression(a) => bare(&a.argument),
    Expression::TSAsExpression(a) => bare(&a.expression),
    Expression::TSNonNullExpression(n) => bare(&n.expression),
    other => other,
  }
}

/// Whether `e`, past any `await` or parentheses, calls a `stat` function.
fn stats(functions: &BTreeSet<String>, namespaces: &[(String, &[&str])], e: &Expression) -> bool {
  let Expression::CallExpression(call) = bare(e) else { return false };
  match bare(&call.callee) {
    Expression::Identifier(i) => functions.contains(i.name.as_str()),
    Expression::StaticMemberExpression(m) => match bare(&m.object) {
      Expression::Identifier(object) => namespaces.iter().any(|(name, exports)| object.name == name.as_str() && exports.contains(&m.property.name.as_str())),
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

impl<'a> Visit<'a> for Results<'_> {
  fn visit_variable_declarator(&mut self, n: &js::VariableDeclarator<'a>) {
    if let (BindingPattern::BindingIdentifier(name), Some(init)) = (&n.id, &n.init) {
      if stats(self.functions, self.namespaces, init) {
        self.names.insert(name.name.to_string());
      }
    }
    walk::walk_variable_declarator(self, n);
  }
}

struct Uses<'a> {
  parsed: &'a Parsed<'a>,
  text: &'a str,
  functions: &'a BTreeSet<String>,
  namespaces: &'a [(String, &'static [&'static str])],
  results: &'a BTreeSet<String>,
  edits: Edits,
  notes: Vec<(usize, String)>,
}

impl Uses<'_> {
  fn written(&self, span: Span) -> &str {
    &self.text[self.parsed.range(span)]
  }

  /// Whether `e` is a `stat` result: a call of a `stat` function or a name declared from one.
  fn result(&self, e: &Expression) -> bool {
    match bare(e) {
      Expression::Identifier(i) => self.results.contains(i.name.as_str()),
      _ => stats(self.functions, self.namespaces, e),
    }
  }

  fn note(&mut self, span: Span, note: String) {
    self.notes.push((self.parsed.line(span), note));
  }
}

impl<'a> Visit<'a> for Uses<'_> {
  fn visit_import_declaration(&mut self, _: &js::ImportDeclaration<'a>) {}

  fn visit_variable_declarator(&mut self, n: &js::VariableDeclarator<'a>) {
    if let Some(init) = &n.init {
      self.visit_expression(init);
    }
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let Expression::StaticMemberExpression(m) = &n.callee {
      if self.result(&m.object) && FIELDS.contains(&m.property.name.as_str()) && n.arguments.is_empty() {
        let field = self.written(m.span).to_owned();
        self.edits.replace(self.parsed.range(n.span), field);
        self.visit_expression(&m.object);
        return;
      }
    }
    if n.arguments.iter().any(|arg| matches!(bare(argument(arg)), Expression::Identifier(i) if self.results.contains(i.name.as_str()))) {
      let written = self.written(n.span).to_owned();
      self.note(n.span, format!("`{written}`: the `stat` result is passed on, so `fsr migrate` cannot rewrite an `isFile()` or `isDirectory()` call on it to the field FSR's `file.stat` answers"));
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_static_member_expression(&mut self, n: &js::StaticMemberExpression<'a>) {
    if self.result(&n.object) && MISSING.contains(&n.property.name.as_str()) {
      let written = self.written(n.span).to_owned();
      self.note(n.span, format!("`{written}`: FSR's `file.stat` answers `isFile`, `isDirectory`, `size` and `mtimeMs` only"));
    }
    self.visit_expression(&n.object);
  }
}
