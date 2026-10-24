//! CommonJS, which `fsr migrate` does not read: the application's files it
//! would migrate, followed from the entries `package.json` names and the
//! source directories, each one that is CommonJS with where that shows.

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::syntax::{is_script, parse};
use crate::{Note, Tree};

/// The directories an application's sources live in.
const SOURCE_DIRS: [&str; 2] = ["source/", "src/"];

/// One refusal per CommonJS file the application holds: a script that imports and exports nothing and calls `require` or assigns `module.exports` or `exports.x`, at the first of them.
pub(crate) fn refusals(tree: &Tree) -> Vec<Note> {
  let mut queue: Vec<String> = entries(tree);
  queue.extend(tree.paths().filter(|p| SOURCE_DIRS.iter().any(|dir| p.starts_with(dir)) && is_script(p)).map(str::to_owned));
  let mut seen: Vec<String> = Vec::new();
  let mut out = Vec::new();
  while let Some(path) = queue.pop() {
    if seen.contains(&path) {
      continue;
    }
    seen.push(path.clone());
    let Some(text) = tree.text(&path) else { continue };
    let Ok(parsed) = parse(&path, text) else { continue };
    let mut found = Found::default();
    parsed.module.visit_with(&mut found);
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
  commonjs: Option<(swc_core::common::Span, String)>,
  /// The relative specifiers it loads, by `require`, `importJsx` or `import`.
  relative: Vec<String>,
}

impl Found {
  fn note(&mut self, span: swc_core::common::Span, what: String) {
    if self.commonjs.as_ref().is_none_or(|(held, _)| span.lo < held.lo) {
      self.commonjs = Some((span, what));
    }
  }
}

impl Visit for Found {
  fn visit_module_decl(&mut self, n: &js::ModuleDecl) {
    self.modular = true;
    if let js::ModuleDecl::Import(import) = n {
      let spec = import.src.value.to_string_lossy().into_owned();
      if spec.starts_with('.') {
        self.relative.push(spec);
      }
    }
    n.visit_children_with(self);
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &n.callee {
      if let js::Expr::Ident(id) = &**callee {
        let loads = matches!(id.sym.as_ref(), "require" | "importJsx");
        if loads {
          if let Some(js::Expr::Lit(js::Lit::Str(spec))) = n.args.first().map(|a| &*a.expr) {
            let spec = spec.value.to_string_lossy().into_owned();
            if spec.starts_with('.') {
              self.relative.push(spec);
            }
          }
        }
        if id.sym.as_ref() == "require" {
          self.note(n.span, "`require`".to_owned());
        }
      }
    }
    n.visit_children_with(self);
  }

  fn visit_assign_expr(&mut self, n: &js::AssignExpr) {
    if let js::AssignTarget::Simple(js::SimpleAssignTarget::Member(member)) = &n.left {
      let field = match &member.prop {
        js::MemberProp::Ident(prop) => Some(prop.sym.to_string()),
        _ => None,
      };
      match (&*member.obj, field.as_deref()) {
        (js::Expr::Ident(obj), Some("exports")) if obj.sym.as_ref() == "module" => self.note(n.span(), "`module.exports =`".to_owned()),
        (js::Expr::Ident(obj), Some(name)) if obj.sym.as_ref() == "exports" => self.note(n.span(), format!("`exports.{name} =`")),
        _ => {}
      }
    }
    n.visit_children_with(self);
  }
}
