//! CommonJS, which `fsr migrate` does not read: the application's files it
//! would migrate, followed from where the source keeps them, each one that is
//! CommonJS with where that shows.

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, Argument, AssignmentTarget, Expression, ModuleDeclaration};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::syntax::{is_script, lossy, parse};
use crate::{Note, Source, Tree};

/// The directories an Ink application's sources live in, beside the entries `package.json` names.
const INK_DIRS: &[&str] = &["source/", "src/"];
/// The directories a Next.js application's routes live in; a config file or a server entry beside them is not migrated.
const NEXT_DIRS: &[&str] = &["app/", "pages/", "src/app/", "src/pages/"];
const SVELTEKIT_DIRS: &[&str] = &["src/routes/", "src/lib/"];
const NUXT_DIRS: &[&str] = &["pages/", "components/", "layouts/", "composables/", "utils/", "plugins/", "middleware/", "app/"];

/// One refusal per CommonJS file the migration of `source` would read: a script that imports and exports nothing and calls `require` or assigns `module.exports` or `exports.x`, at the first of them. The files are followed by their relative imports from where `source` keeps its code.
pub(crate) fn refusals(tree: &Tree, source: Source) -> Vec<Note> {
  let (mut queue, dirs) = match source {
    Source::Ink => (entries(tree), INK_DIRS),
    Source::NextApp | Source::NextPages => (Vec::new(), NEXT_DIRS),
    Source::SvelteKit => (Vec::new(), SVELTEKIT_DIRS),
    Source::Nuxt => (Vec::new(), NUXT_DIRS),
  };
  queue.extend(tree.paths().filter(|p| dirs.iter().any(|dir| p.starts_with(dir)) && is_script(p)).map(str::to_owned));
  let mut seen: Vec<String> = Vec::new();
  let mut out = Vec::new();
  while let Some(path) = queue.pop() {
    if seen.contains(&path) {
      continue;
    }
    seen.push(path.clone());
    let Some(text) = tree.text(&path) else { continue };
    let allocator = Allocator::default();
    let Ok(parsed) = parse(&allocator, &path, text) else { continue };
    let mut found = Found::default();
    found.visit_program(&parsed.program);
    let dir = path.rsplit_once('/').map(|(dir, _)| format!("{dir}/")).unwrap_or_default();
    for spec in &found.relative {
      if let Some(next) = resolve(tree, &dir, spec) {
        queue.push(next);
      }
    }
    if found.modular {
      continue;
    }
    if let Some((span, what)) = found.commonjs {
      out.push(Note { file: path.clone(), line: parsed.line(span), message: format!("{what}, which is CommonJS; fsr migrate reads ES modules, `import` and `export`") });
    }
  }
  out.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
  out
}

/// The files `package.json` names as `main` or `bin`.
fn entries(tree: &Tree) -> Vec<String> {
  let Some(package) = tree.text("package.json").and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok()) else { return Vec::new() };
  let mut out = Vec::new();
  let mut add = |value: &serde_json::Value| {
    if let Some(path) = value.as_str() {
      out.push(path.trim_start_matches("./").to_owned());
    }
  };
  if let Some(main) = package.get("main") {
    add(main);
  }
  match package.get("bin") {
    Some(serde_json::Value::Object(bins)) => bins.values().for_each(&mut add),
    Some(bin) => add(bin),
    None => {}
  }
  out
}

/// The script `spec` names from `dir`, as Node finds one: the file itself, with a script extension or as a directory's `index`.
fn resolve(tree: &Tree, dir: &str, spec: &str) -> Option<String> {
  let mut parts: Vec<&str> = dir.trim_end_matches('/').split('/').filter(|p| !p.is_empty()).collect();
  for part in spec.split('/') {
    match part {
      "." | "" => {}
      ".." => {
        parts.pop();
      }
      other => parts.push(other),
    }
  }
  let base = parts.join("/");
  let extensions = ["", ".js", ".jsx", ".cjs", ".mjs", ".ts", ".tsx", "/index.js", "/index.jsx", "/index.ts", "/index.tsx"];
  extensions.iter().map(|ext| format!("{base}{ext}")).find(|path| tree.text(path).is_some() && is_script(path))
}

#[derive(Default)]
struct Found {
  /// The module imports or exports.
  modular: bool,
  /// The first CommonJS use and how it reads.
  commonjs: Option<(Span, String)>,
  /// The relative specifiers it loads, by `require`, `importJsx` or `import`.
  relative: Vec<String>,
}

impl Found {
  fn note(&mut self, span: Span, what: String) {
    if self.commonjs.as_ref().is_none_or(|(held, _)| span.start < held.start) {
      self.commonjs = Some((span, what));
    }
  }
}

impl<'a> Visit<'a> for Found {
  fn visit_module_declaration(&mut self, n: &ModuleDeclaration<'a>) {
    self.modular = true;
    if let ModuleDeclaration::ImportDeclaration(import) = n {
      let spec = lossy(import.source.value);
      if spec.starts_with('.') {
        self.relative.push(spec);
      }
    }
    walk::walk_module_declaration(self, n);
  }

  fn visit_ts_import_equals_declaration(&mut self, n: &js::TSImportEqualsDeclaration<'a>) {
    self.modular = true;
    walk::walk_ts_import_equals_declaration(self, n);
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let Expression::Identifier(id) = &n.callee {
      let loads = matches!(id.name.as_str(), "require" | "importJsx");
      if loads {
        if let Some(Argument::StringLiteral(spec)) = n.arguments.first() {
          let spec = lossy(spec.value);
          if spec.starts_with('.') {
            self.relative.push(spec);
          }
        }
      }
      if id.name == "require" {
        self.note(n.span, "`require`".to_owned());
      }
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_assignment_expression(&mut self, n: &js::AssignmentExpression<'a>) {
    if let AssignmentTarget::StaticMemberExpression(member) = &n.left {
      match (&member.object, member.property.name.as_str()) {
        (Expression::Identifier(obj), "exports") if obj.name == "module" => self.note(n.span(), "`module.exports =`".to_owned()),
        (Expression::Identifier(obj), name) if obj.name == "exports" => self.note(n.span(), format!("`exports.{name} =`")),
        _ => {}
      }
    }
    walk::walk_assignment_expression(self, n);
  }
}
