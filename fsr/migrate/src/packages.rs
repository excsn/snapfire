//! Packages FSR has a standard member for, written as that member wherever an
//! application imports them. `gray-matter` is `text.frontMatter` and
//! `text.withFrontMatter`; a `pino` or `winston` logger is `log`.

use std::collections::BTreeSet;
use std::ops::Range;

use swc_core::common::Spanned;
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::{line_range, tidy};
use crate::syntax::{is_script, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(GrayMatter), Box::new(Loggers)]
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

/// The name the standard module's `member` is held under in `parsed` and the import to add when none is.
fn std_member(parsed: &Parsed, member: &str) -> (String, Option<String>) {
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
        if imported == member {
          return (n.local.sym.to_string(), None);
        }
      }
    }
  }
  let mut names = Names(BTreeSet::new());
  parsed.module.visit_with(&mut names);
  if names.0.contains(member) {
    (format!("std_{member}"), Some(format!("import {{ {member} as std_{member} }} from \"{STD}\";")))
  } else {
    (member.to_owned(), Some(format!("import {{ {member} }} from \"{STD}\";")))
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
      let (member, import) = std_member(&parsed, "text");
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

/// A `pino` or `winston` logger a file declares or a `child` of one.
struct Logger {
  name: String,
  package: &'static str,
  /// The `child` fields as written inside their braces, a parent's first.
  bindings: Option<String>,
  /// The line of the declaration.
  line: Range<usize>,
}

/// `pino` and `winston` loggers as the `log` member: a level call with the message first and the fields second, a `child`'s fields merged into each call and winston's `http`, `verbose` and `silly` at `debug`, `debug` and `trace`. The options a logger is made with are dropped with a note. A file that uses a logger any other way, exports one or calls pino with a format string is left as written with a note for each use.
struct Loggers;

const LOGGING: &[&str] = &["pino", "winston"];

impl Pass for Loggers {
  fn name(&self) -> &'static str {
    "loggers"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !LOGGING.iter().any(|p| text.contains(p)) {
        continue;
      }
      let Ok(parsed) = parse(&path, &text) else { continue };
      let held: Vec<(&'static str, Held)> = LOGGING.iter().flat_map(|p| held(&parsed, &text, p).into_iter().map(move |h| (*p, h))).collect();
      if held.is_empty() {
        continue;
      }
      let mut made = Made { parsed: &parsed, text: &text, held: &held, loggers: Vec::new(), options: Vec::new(), unmapped: Vec::new() };
      parsed.module.visit_with(&mut made);
      let Made { loggers, options, unmapped, .. } = made;
      let (member, import) = std_member(&parsed, "log");
      let mut calls = Calls { parsed: &parsed, text: &text, held: &held, loggers: &loggers, member: &member, edits: Edits::default(), unmapped };
      parsed.module.visit_with(&mut calls);
      let Calls { mut edits, unmapped, .. } = calls;
      if !unmapped.is_empty() {
        for (line, written) in unmapped {
          report.note(&path, line, format!("`{written}`: this use of a logger has no counterpart in `log`, so the file still imports its package"));
        }
        continue;
      }
      for (line, written) in options {
        report.note(&path, line, format!("`{written}`: the options are dropped; the host's `tracing` filter decides which lines are written"));
      }
      for logger in &loggers {
        edits.replace(logger.line.clone(), "");
      }
      let mut import = import;
      for (_, h) in &held {
        match import.take() {
          Some(line) if !h.dynamic => edits.replace(h.binding.clone(), line),
          taken => {
            import = taken;
            edits.replace(if h.dynamic { h.binding.clone() } else { line_range(&text, h.binding.clone()) }, "");
          }
        }
      }
      if let Some(import) = import {
        let at = parsed.module.body.first().map(|item| parsed.range(item.span()).start).unwrap_or(0);
        edits.replace(at..at, format!("{import}\n"));
      }
      tree.insert(path, tidy(&edits.apply(&text)));
    }
  }
}

fn ident<'e>(e: &'e js::Expr) -> Option<&'e str> {
  match e {
    js::Expr::Ident(i) => Some(&i.sym),
    js::Expr::Paren(p) => ident(&p.expr),
    _ => None,
  }
}

/// `object.name`, with `object` a plain name.
fn member_of<'e>(e: &'e js::Expr) -> Option<(&'e str, &'e str)> {
  let js::Expr::Member(m) = e else { return None };
  let js::MemberProp::Ident(prop) = &m.prop else { return None };
  Some((ident(&m.obj)?, &prop.sym))
}

/// The text inside an object literal's braces, trimmed.
fn inside(written: &str) -> &str {
  written.trim().strip_prefix('{').and_then(|w| w.strip_suffix('}')).unwrap_or(written).trim()
}

/// Finds the loggers a file makes and the uses that rule the file out before any call is read.
struct Made<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  held: &'a [(&'static str, Held)],
  loggers: Vec<Logger>,
  options: Vec<(usize, String)>,
  unmapped: Vec<(usize, String)>,
}

impl Made<'_> {
  fn package_of(&self, name: &str) -> Option<&'static str> {
    self.held.iter().find(|(_, h)| h.local == name).map(|(p, _)| *p)
  }

  /// The package whose logger `call` makes: `pino(...)`, `winston.createLogger(...)` or a named `createLogger(...)`.
  fn makes(&self, call: &js::CallExpr) -> Option<&'static str> {
    let js::Callee::Expr(callee) = &call.callee else { return None };
    if let Some(name) = ident(callee) {
      return self.package_of(name);
    }
    let (object, prop) = member_of(callee)?;
    (prop == "createLogger").then(|| self.package_of(object)).flatten().filter(|p| *p == "winston")
  }
}

impl Visit for Made<'_> {
  fn visit_export_decl(&mut self, n: &js::ExportDecl) {
    let before = self.loggers.len();
    n.visit_children_with(self);
    if self.loggers.len() > before {
      let span = self.parsed.range(n.span);
      self.unmapped.push((self.parsed.line(n.span), self.text[span].to_owned()));
    }
  }

  fn visit_var_decl(&mut self, n: &js::VarDecl) {
    let [declarator] = n.decls.as_slice() else { return n.visit_children_with(self) };
    let (js::Pat::Ident(name), Some(js::Expr::Call(call))) = (&declarator.name, declarator.init.as_deref()) else { return n.visit_children_with(self) };
    let line = line_range(self.text, self.parsed.range(n.span));
    if let Some(package) = self.makes(call) {
      if !call.args.is_empty() {
        self.options.push((self.parsed.line(call.span), self.text[self.parsed.range(call.span)].to_owned()));
      }
      self.loggers.push(Logger { name: name.id.sym.to_string(), package, bindings: None, line });
      return;
    }
    let js::Callee::Expr(callee) = &call.callee else { return };
    let Some((object, "child")) = member_of(callee) else { return n.visit_children_with(self) };
    let Some(parent) = self.loggers.iter().find(|l| l.name == object) else { return };
    let fields = match call.args.as_slice() {
      [arg] if arg.spread.is_none() && matches!(&*arg.expr, js::Expr::Object(_)) => inside(&self.text[self.parsed.range(arg.expr.span())]).to_owned(),
      _ => {
        self.unmapped.push((self.parsed.line(call.span), self.text[self.parsed.range(call.span)].to_owned()));
        return;
      }
    };
    let bindings = match &parent.bindings {
      Some(parent) => format!("{parent}, {fields}"),
      None => fields,
    };
    let package = parent.package;
    self.loggers.push(Logger { name: name.id.sym.to_string(), package, bindings: Some(bindings), line });
  }
}

/// Rewrites each level call on a logger.
struct Calls<'a> {
  parsed: &'a Parsed,
  text: &'a str,
  held: &'a [(&'static str, Held)],
  loggers: &'a [Logger],
  member: &'a str,
  edits: Edits,
  unmapped: Vec<(usize, String)>,
}

impl Calls<'_> {
  fn written(&self, span: swc_core::common::Span) -> &str {
    &self.text[self.parsed.range(span)]
  }

  fn logger(&self, name: &str) -> Option<&Logger> {
    self.loggers.iter().find(|l| l.name == name)
  }

  /// `fields` with the logger's `child` fields before them.
  fn merged(&self, logger: &Logger, fields: Option<&js::Expr>) -> Option<String> {
    match (&logger.bindings, fields) {
      (None, None) => None,
      (None, Some(fields)) => Some(self.written(fields.span()).to_owned()),
      (Some(bound), None) => Some(format!("{{ {bound} }}")),
      (Some(bound), Some(js::Expr::Object(object))) => match inside(self.written(object.span)) {
        "" => Some(format!("{{ {bound} }}")),
        fields => Some(format!("{{ {bound}, {fields} }}")),
      },
      (Some(bound), Some(fields)) => Some(format!("{{ {bound}, ...{} }}", self.written(fields.span()))),
    }
  }

  /// The `log` level a logger's method writes at, `None` for a method that is not a level.
  fn level(package: &str, method: &str) -> Option<&'static str> {
    match (package, method) {
      (_, "info") => Some("info"),
      (_, "warn") => Some("warn"),
      (_, "error") => Some("error"),
      (_, "debug") => Some("debug"),
      ("pino", "trace") => Some("trace"),
      ("pino", "fatal") => Some("fatal"),
      ("winston", "http" | "verbose") => Some("debug"),
      ("winston", "silly") => Some("trace"),
      _ => None,
    }
  }

  /// `call` written as a `log` call, `None` when its arguments have no such form.
  fn rewrite(&self, logger: &Logger, method: &str, call: &js::CallExpr) -> Option<String> {
    let args: Vec<&js::Expr> = call.args.iter().map(|a| a.spread.is_none().then_some(&*a.expr)).collect::<Option<_>>()?;
    let (level, args) = match (logger.package, method) {
      ("winston", "log") => match args.split_first()? {
        (js::Expr::Lit(js::Lit::Str(level)), rest) => (Self::level("winston", &level.value.to_string_lossy())?, rest.to_vec()),
        _ => return None,
      },
      (package, method) => (Self::level(package, method)?, args),
    };
    let textual = |e: &js::Expr| matches!(e, js::Expr::Lit(js::Lit::Str(_)) | js::Expr::Tpl(_));
    let (message, fields) = match (logger.package, args.as_slice()) {
      ("pino", [only]) if matches!(only, js::Expr::Object(_)) => ("\"\"".to_owned(), Some(*only)),
      ("pino", [message]) => (self.written(message.span()).to_owned(), None),
      ("pino", [fields, message]) if !textual(fields) => (self.written(message.span()).to_owned(), Some(*fields)),
      ("winston", [message]) => (self.written(message.span()).to_owned(), None),
      ("winston", [message, fields]) => (self.written(message.span()).to_owned(), Some(*fields)),
      _ => return None,
    };
    Some(match self.merged(logger, fields) {
      Some(fields) => format!("{}.{level}({message}, {fields})", self.member),
      None => format!("{}.{level}({message})", self.member),
    })
  }
}

impl Visit for Calls<'_> {
  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_var_decl(&mut self, n: &js::VarDecl) {
    let start = self.parsed.range(n.span).start;
    if !self.loggers.iter().any(|l| l.line.contains(&start)) {
      n.visit_children_with(self);
    }
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    if let js::Callee::Expr(callee) = &n.callee {
      if let Some((object, method)) = member_of(callee) {
        if let Some(logger) = self.logger(object) {
          match self.rewrite(logger, method, n) {
            Some(written) => self.edits.replace(self.parsed.range(n.span), written),
            None => self.unmapped.push((self.parsed.line(n.span), self.written(n.span).to_owned())),
          }
          return;
        }
      }
    }
    n.visit_children_with(self);
  }

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if ident(&n.obj).is_some_and(|name| self.logger(name).is_some() || self.held.iter().any(|(_, h)| h.local == name)) {
      self.unmapped.push((self.parsed.line(n.span), self.written(n.span).to_owned()));
      return;
    }
    n.visit_children_with(self);
  }

  fn visit_expr(&mut self, n: &js::Expr) {
    if let js::Expr::Ident(i) = n {
      if self.logger(&i.sym).is_some() || self.held.iter().any(|(_, h)| h.local == &*i.sym) {
        self.unmapped.push((self.parsed.line(i.span), i.sym.to_string()));
      }
      return;
    }
    n.visit_children_with(self);
  }
}
