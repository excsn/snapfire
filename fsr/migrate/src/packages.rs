//! Packages FSR has a standard member for, written as that member wherever an
//! application imports them: `gray-matter` as `text.frontMatter` and
//! `text.withFrontMatter`.

use std::collections::BTreeSet;
use std::ops::Range;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::line_range;
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(GrayMatter)]
}

const STD: &str = "@snapfire/fsr-client/std";

/// A name a file holds a package under: the default export itself or a namespace whose `default` is it.
struct Held {
  local: String,
  namespace: bool,
  /// The import declaration or the line of the `await import()` that binds it.
  binding: Range<usize>,
  dynamic: bool,
}

/// Every name `parsed` binds `package` to, by a static import or an awaited `import()`.
fn held(parsed: &Parsed, text: &str, package: &str) -> Vec<Held> {
  let mut out = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.src.value.to_string_lossy() != package || import.type_only {
      continue;
    }
    for specifier in &import.specifiers {
      let (local, namespace) = match specifier {
        js::ImportSpecifier::Default(d) => (d.local.sym.to_string(), false),
        js::ImportSpecifier::Namespace(n) => (n.local.sym.to_string(), true),
        js::ImportSpecifier::Named(n) => (n.local.sym.to_string(), false),
      };
      out.push(Held { local, namespace, binding: parsed.range(import.span), dynamic: false });
    }
  }
  let mut dynamic = Dynamic { parsed, text, package, found: Vec::new() };
  parsed.module.visit_with(&mut dynamic);
  out.extend(dynamic.found);
  out
}

struct Dynamic<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  package: &'a str,
  found: Vec<Held>,
}

impl Visit for Dynamic<'_> {
  fn visit_var_decl(&mut self, n: &js::VarDecl) {
    if let [declarator] = n.decls.as_slice() {
      let imports = declarator.init.as_deref().is_some_and(|init| match init {
        js::Expr::Await(awaited) => matches!(&*awaited.arg, js::Expr::Call(call) if matches!(call.callee, js::Callee::Import(_)) && matches!(call.args.first().map(|a| &*a.expr), Some(js::Expr::Lit(js::Lit::Str(s))) if s.value.to_string_lossy() == self.package)),
        _ => false,
      });
      if imports {
        let binding = line_range(self.text, self.parsed.range(n.span));
        match &declarator.name {
          js::Pat::Ident(i) => self.found.push(Held { local: i.id.sym.to_string(), namespace: true, binding, dynamic: true }),
          js::Pat::Object(o) => {
            for prop in &o.props {
              if let js::ObjectPatProp::KeyValue(kv) = prop {
                if let (js::PropName::Ident(key), js::Pat::Ident(value)) = (&kv.key, &*kv.value) {
                  if &*key.sym == "default" {
                    self.found.push(Held { local: value.id.sym.to_string(), namespace: false, binding: binding.clone(), dynamic: true });
                  }
                }
              }
            }
          }
          _ => {}
        }
        return;
      }
    }
    n.visit_children_with(self);
  }
}

/// The name `text` from the standard module is held under in `parsed` and the import to add when none is.
fn std_text(parsed: &Parsed) -> (String, Option<String>) {
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if import.src.value.to_string_lossy() != STD {
      continue;
    }
    for specifier in &import.specifiers {
      if let js::ImportSpecifier::Named(n) = specifier {
        let imported = match &n.imported {
          Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
          Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
          None => n.local.sym.to_string(),
        };
        if imported == "text" {
          return (n.local.sym.to_string(), None);
        }
      }
    }
  }
  let mut names = Names(BTreeSet::new());
  parsed.module.visit_with(&mut names);
  if names.0.contains("text") {
    ("std_text".to_owned(), Some(format!("import {{ text as std_text }} from \"{STD}\";")))
  } else {
    ("text".to_owned(), Some(format!("import {{ text }} from \"{STD}\";")))
  }
}

struct Names(BTreeSet<String>);

impl Visit for Names {
  fn visit_ident(&mut self, n: &js::Ident) {
    self.0.insert(n.sym.to_string());
  }
}

/// `gray-matter`'s function as `text.frontMatter` and its `stringify` of a `{ content }` file as `text.withFrontMatter(content, ...)`. A file that uses it any other way is left as written with a note for each use.
struct GrayMatter;

impl Pass for GrayMatter {
  fn name(&self) -> &'static str {
    "gray-matter"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("gray-matter") {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let held = held(&parsed, &text, "gray-matter");
      if held.is_empty() {
        continue;
      }
      let (member, import) = std_text(&parsed);
      let mut uses = Uses { parsed: &parsed, text: &text, held: &held, member: &member, edits: Edits::default(), unmapped: Vec::new() };
      parsed.module.visit_with(&mut uses);
      let Uses { mut edits, unmapped, .. } = uses;
      if !unmapped.is_empty() {
        for (line, written) in unmapped {
          report.note(&path, line, format!("`{written}`: `gray-matter` has no FSR counterpart for this use, so the file still imports it"));
        }
        continue;
      }
      let mut import = import;
      for h in &held {
        if !h.dynamic && import.is_some() {
          edits.replace(h.binding.clone(), import.take().unwrap_or_default());
        } else {
          edits.replace(if h.dynamic { h.binding.clone() } else { line_range(&text, h.binding.clone()) }, "");
        }
      }
      if let Some(import) = import {
        let at = parsed.module.body.first().map(|item| parsed.range(item.span()).start).unwrap_or(0);
        edits.replace(at..at, format!("{import}\n"));
      }
      tree.insert(path, edits.apply(&text));
    }
  }
}

struct Uses<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  held: &'a [Held],
  member: &'a str,
  edits: Edits,
  /// Each use with no member, by line and as written.
  unmapped: Vec<(usize, String)>,
}

impl Uses<'_> {
  fn held(&self, name: &str) -> Option<&Held> {
    self.held.iter().find(|h| h.local == name)
  }

  /// Whether `e` is the package's function: a name holding it or a namespace's `default`.
  fn is_function(&self, e: &js::Expr) -> bool {
    match e {
      js::Expr::Ident(i) => self.held(&i.sym).is_some_and(|h| !h.namespace),
      js::Expr::Member(m) => matches!(&*m.obj, js::Expr::Ident(i) if self.held(&i.sym).is_some_and(|h| h.namespace)) && matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "default"),
      js::Expr::Paren(p) => self.is_function(&p.expr),
      _ => false,
    }
  }

  fn written(&self, span: swc_core::common::Span) -> String {
    self.text[self.parsed.range(span)].to_owned()
  }

  /// The `content` of a `{ content }` file literal, as written.
  fn content(&self, arg: &js::ExprOrSpread) -> Option<String> {
    let js::Expr::Object(object) = &*arg.expr else { return None };
    let [prop] = object.props.as_slice() else { return None };
    match prop {
      js::PropOrSpread::Prop(prop) => match &**prop {
        js::Prop::Shorthand(i) if &*i.sym == "content" => Some(i.sym.to_string()),
        js::Prop::KeyValue(kv) if matches!(&kv.key, js::PropName::Ident(k) if &*k.sym == "content") => Some(self.written(kv.value.span())),
        _ => None,
      },
      js::PropOrSpread::Spread(_) => None,
    }
  }
}

impl Visit for Uses<'_> {
  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_var_decl(&mut self, n: &js::VarDecl) {
    let binds = self.held.iter().any(|h| h.dynamic && h.binding.contains(&self.parsed.range(n.span).start));
    if !binds {
      n.visit_children_with(self);
    }
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    let js::Callee::Expr(callee) = &n.callee else {
      n.visit_children_with(self);
      return;
    };
    if self.is_function(callee) {
      self.edits.replace(self.parsed.range(callee.span()), format!("{}.frontMatter", self.member));
      n.args.visit_with(self);
      return;
    }
    if let js::Expr::Member(m) = &**callee {
      if self.is_function(&m.obj) && matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "stringify") {
        if let Some(content) = n.args.first().and_then(|a| self.content(a)) {
          self.edits.replace(self.parsed.range(callee.span()), format!("{}.withFrontMatter", self.member));
          self.edits.replace(self.parsed.range(n.args[0].span()), content);
          n.args[1..].visit_with(self);
          return;
        }
        self.unmapped.push((self.parsed.line(n.span), self.written(n.span)));
        return;
      }
    }
    n.visit_children_with(self);
  }

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if self.is_function(&n.obj) || matches!(&*n.obj, js::Expr::Ident(i) if self.held(&i.sym).is_some()) {
      self.unmapped.push((self.parsed.line(n.span), self.written(n.span)));
      return;
    }
    n.visit_children_with(self);
  }

  fn visit_expr(&mut self, n: &js::Expr) {
    if let js::Expr::Ident(i) = n {
      if self.held(&i.sym).is_some() {
        self.unmapped.push((self.parsed.line(i.span), i.sym.to_string()));
      }
      return;
    }
    n.visit_children_with(self);
  }
}
