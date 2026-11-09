//! Packages FSR has a standard member for, written as that member wherever an
//! application imports them. `gray-matter` is `text.frontMatter` and
//! `text.withFrontMatter`; a `pino` or `winston` logger is `log`.

use std::collections::BTreeSet;
use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, Argument, BindingPattern, Expression, ImportDeclarationSpecifier, MemberExpression, ModuleExportName, ObjectPropertyKind, PropertyKey, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::items::{line_range, tidy};
use crate::syntax::{argument, import_specifiers, is_script, key_value, lossy, parse, Parsed};
use crate::{Pass, Report, Tree};
use snapfire_fsr_lower::terminal::REPLACED;

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(GrayMatter), Box::new(Loggers), Box::new(crate::big_text::BigText), Box::new(Terminal), Box::new(Clipboardy)]
}

/// The packages a terminal tool reaches for whose functions FSR's terminal module has, by package and member.
const TERMINAL: &[(&str, &str)] = &[("ms", "ms"), ("strip-ansi", "stripAnsi"), ("string-width", "stringWidth"), ("wrap-ansi", "wrapAnsi")];

/// `ms`, `strip-ansi`, `string-width` and `wrap-ansi` imported by default as the member of `@snapfire/fsr/terminal` that does what each does, under the name the file used; `ink`, `ink-spinner`, `ink-text-input` and `cli-spinners` imported from FSR's module of each, with the names the file imported.
struct Terminal;

impl Pass for Terminal {
  fn name(&self) -> &'static str {
    "terminal packages"
  }

  fn run(&self, tree: &mut Tree, _: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !TERMINAL.iter().map(|(package, _)| package).chain(REPLACED.iter().map(|(package, _)| package)).any(|package| text.contains(package)) {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let mut edits = Edits::default();
      for item in parsed.body() {
        let Statement::ImportDeclaration(import) = item else { continue };
        let source = lossy(import.source.value);
        if let Some((_, fsr)) = REPLACED.iter().find(|(package, _)| *package == source) {
          edits.replace(parsed.range(import.source.span), format!("\"{fsr}\""));
          continue;
        }
        let Some((_, member)) = TERMINAL.iter().find(|(package, _)| *package == source) else { continue };
        let [ImportDeclarationSpecifier::ImportDefaultSpecifier(d)] = import_specifiers(import) else { continue };
        let local = d.local.name.to_string();
        let named = if local == *member { local } else { format!("{member} as {local}") };
        edits.replace(parsed.range(import.span), format!("import {{ {named} }} from \"@snapfire/fsr/terminal\";"));
      }
      let written = edits.apply(&text);
      if written != text {
        tree.insert(path, written);
      }
    }
  }
}

/// Where a migrated terminal application imports the standard library's members from.
pub(crate) const HOST: &str = "@snapfire/fsr/host";
pub(crate) const TERMINAL_ROOT: &str = "@snapfire/fsr/terminal";

/// The path a standard `member` is imported from in a terminal application.
pub(crate) fn std_home(member: &str) -> &'static str {
  match member {
    "system" | "clipboard" | "Scope" => TERMINAL_ROOT,
    _ => HOST,
  }
}

/// Whether `source` is a path a standard member is imported from.
pub(crate) fn is_std_source(source: &str) -> bool {
  source == HOST || source == TERMINAL_ROOT
}

/// `clipboardy`'s `write`, `writeSync`, `read` and `readSync` as `clipboard.write` and `clipboard.read`, which answer at once, so an `await` of one goes with it. A file that uses it any other way is left as written with a note for each use.
struct Clipboardy;

impl Pass for Clipboardy {
  fn name(&self) -> &'static str {
    "clipboardy"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      if !text.contains("clipboardy") {
        continue;
      }
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let held = held(&parsed, &text, "clipboardy");
      if held.is_empty() {
        continue;
      }
      let (member, import) = std_member(&parsed, "clipboard");
      let mut uses = ClipboardUses { parsed: &parsed, text: &text, held: &held, member: &member, edits: Edits::default(), unmapped: Vec::new() };
      uses.visit_program(&parsed.program);
      let ClipboardUses { mut edits, unmapped, .. } = uses;
      if !unmapped.is_empty() {
        for (line, written) in unmapped {
          report.note(&path, line, format!("`{written}`: `clipboardy` has no FSR counterpart for this use, so the file still imports it"));
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
        let at = parsed.body().first().map(|item| parsed.range(item.span()).start).unwrap_or(0);
        edits.replace(at..at, format!("{import}\n"));
      }
      tree.insert(path, edits.apply(&text));
    }
  }
}

struct ClipboardUses<'a> {
  parsed: &'a Parsed<'a>,
  text: &'a str,
  held: &'a [Held],
  member: &'a str,
  edits: Edits,
  /// Each use with no member, by line and as written.
  unmapped: Vec<(usize, String)>,
}

impl ClipboardUses<'_> {
  /// `x.write(text)` or `x.read()` written as the member call, for a call of `clipboardy`'s own methods; `write` and `read` answer a promise, so only where it is awaited or the call is a statement (`settled`).
  fn mapped(&self, call: &js::CallExpression, settled: bool) -> Option<String> {
    let Expression::StaticMemberExpression(m) = &call.callee else { return None };
    let Expression::Identifier(obj) = &m.object else { return None };
    if !self.held.iter().any(|h| obj.name == h.local.as_str() && !h.namespace) {
      return None;
    }
    let args: Vec<&str> = call.arguments.iter().map(|a| &self.text[self.parsed.range(a.span())]).collect();
    match (m.property.name.as_str(), args.as_slice()) {
      ("writeSync", [text]) => Some(format!("{}.write({text})", self.member)),
      ("write", [text]) if settled => Some(format!("{}.write({text})", self.member)),
      ("readSync", []) => Some(format!("{}.read()", self.member)),
      ("read", []) if settled => Some(format!("{}.read()", self.member)),
      _ => None,
    }
  }
}

impl<'a> Visit<'a> for ClipboardUses<'_> {
  fn visit_import_declaration(&mut self, _: &js::ImportDeclaration<'a>) {}

  fn visit_expression_statement(&mut self, n: &js::ExpressionStatement<'a>) {
    if let Expression::CallExpression(call) = &n.expression {
      if let Some(written) = self.mapped(call, true) {
        self.edits.replace(self.parsed.range(call.span), written);
        return;
      }
    }
    walk::walk_expression_statement(self, n);
  }

  fn visit_expression(&mut self, n: &Expression<'a>) {
    let call = match n {
      Expression::AwaitExpression(a) => match &a.argument {
        Expression::CallExpression(call) => Some((a.span, call, true)),
        _ => None,
      },
      Expression::CallExpression(call) => Some((call.span, call, false)),
      _ => None,
    };
    if let Some((span, call, settled)) = call {
      if let Some(written) = self.mapped(call, settled) {
        self.edits.replace(self.parsed.range(span), written);
        return;
      }
    }
    walk::walk_expression(self, n);
  }

  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    if let Expression::Identifier(obj) = n.object() {
      if self.held.iter().any(|h| obj.name == h.local.as_str()) {
        self.unmapped.push((self.parsed.line(n.span()), self.text[self.parsed.range(n.span())].to_owned()));
        return;
      }
    }
    walk::walk_member_expression(self, n);
  }

  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    if self.held.iter().any(|h| n.name == h.local.as_str()) {
      self.unmapped.push((self.parsed.line(n.span), n.name.to_string()));
    }
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    if self.held.iter().any(|h| n.name == h.local.as_str()) {
      self.unmapped.push((self.parsed.line(n.span), n.name.to_string()));
    }
  }
}

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
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if import.source.value != package || import.import_kind.is_type() {
      continue;
    }
    for specifier in import_specifiers(import) {
      let (local, namespace) = match specifier {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => (d.local.name.to_string(), false),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => (n.local.name.to_string(), true),
        ImportDeclarationSpecifier::ImportSpecifier(n) => (n.local.name.to_string(), false),
      };
      out.push(Held { local, namespace, binding: parsed.range(import.span), dynamic: false });
    }
  }
  let mut dynamic = Dynamic { parsed, text, package, found: Vec::new() };
  dynamic.visit_program(&parsed.program);
  out.extend(dynamic.found);
  out
}

struct Dynamic<'a> {
  parsed: &'a Parsed<'a>,
  text: &'a str,
  package: &'a str,
  found: Vec<Held>,
}

impl<'a> Visit<'a> for Dynamic<'_> {
  fn visit_variable_declaration(&mut self, n: &js::VariableDeclaration<'a>) {
    if let [declarator] = n.declarations.as_slice() {
      let imports = declarator.init.as_ref().is_some_and(|init| match init {
        Expression::AwaitExpression(awaited) => matches!(&awaited.argument, Expression::ImportExpression(call) if matches!(&call.source, Expression::StringLiteral(s) if s.value == self.package)),
        _ => false,
      });
      if imports {
        let binding = line_range(self.text, self.parsed.range(n.span));
        match &declarator.id {
          BindingPattern::BindingIdentifier(i) => self.found.push(Held { local: i.name.to_string(), namespace: true, binding, dynamic: true }),
          BindingPattern::ObjectPattern(o) => {
            for prop in &o.properties {
              if prop.shorthand || prop.computed {
                continue;
              }
              if let (PropertyKey::StaticIdentifier(key), BindingPattern::BindingIdentifier(value)) = (&prop.key, &prop.value) {
                if key.name == "default" {
                  self.found.push(Held { local: value.name.to_string(), namespace: false, binding: binding.clone(), dynamic: true });
                }
              }
            }
          }
          _ => {}
        }
        return;
      }
    }
    walk::walk_variable_declaration(self, n);
  }
}

/// The name the standard module's `member` is held under in `parsed` and the import to add when none is.
pub(crate) fn std_member(parsed: &Parsed, member: &str) -> (String, Option<String>) {
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    if !is_std_source(&lossy(import.source.value)) {
      continue;
    }
    for specifier in import_specifiers(import) {
      if let ImportDeclarationSpecifier::ImportSpecifier(n) = specifier {
        let imported = match &n.imported {
          ModuleExportName::IdentifierName(i) => i.name.to_string(),
          ModuleExportName::IdentifierReference(i) => i.name.to_string(),
          ModuleExportName::StringLiteral(s) => lossy(s.value),
        };
        if imported == member {
          return (n.local.name.to_string(), None);
        }
      }
    }
  }
  let mut names = Names(BTreeSet::new());
  names.visit_program(&parsed.program);
  if names.0.contains(member) {
    (format!("std_{member}"), Some(format!("import {{ {member} as std_{member} }} from \"{}\";", std_home(member))))
  } else {
    (member.to_owned(), Some(format!("import {{ {member} }} from \"{}\";", std_home(member))))
  }
}

/// Every name a file writes: identifiers, labels and the tag of a JSX element.
struct Names(BTreeSet<String>);

impl<'a> Visit<'a> for Names {
  fn visit_identifier_reference(&mut self, n: &js::IdentifierReference<'a>) {
    self.0.insert(n.name.to_string());
  }

  fn visit_binding_identifier(&mut self, n: &js::BindingIdentifier<'a>) {
    self.0.insert(n.name.to_string());
  }

  fn visit_label_identifier(&mut self, n: &js::LabelIdentifier<'a>) {
    self.0.insert(n.name.to_string());
  }

  fn visit_jsx_element_name(&mut self, n: &js::JSXElementName<'a>) {
    if let js::JSXElementName::Identifier(i) = n {
      self.0.insert(i.name.to_string());
    }
    walk::walk_jsx_element_name(self, n);
  }
}

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
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let held = held(&parsed, &text, "gray-matter");
      if held.is_empty() {
        continue;
      }
      let (member, import) = std_member(&parsed, "text");
      let mut uses = Uses { parsed: &parsed, text: &text, held: &held, member: &member, edits: Edits::default(), unmapped: Vec::new() };
      uses.visit_program(&parsed.program);
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
        let at = parsed.body().first().map(|item| parsed.range(item.span()).start).unwrap_or(0);
        edits.replace(at..at, format!("{import}\n"));
      }
      tree.insert(path, edits.apply(&text));
    }
  }
}

struct Uses<'a> {
  parsed: &'a Parsed<'a>,
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
  fn is_function(&self, e: &Expression) -> bool {
    match e {
      Expression::Identifier(i) => self.held(&i.name).is_some_and(|h| !h.namespace),
      Expression::StaticMemberExpression(m) => matches!(&m.object, Expression::Identifier(i) if self.held(&i.name).is_some_and(|h| h.namespace)) && m.property.name == "default",
      Expression::ParenthesizedExpression(p) => self.is_function(&p.expression),
      _ => false,
    }
  }

  fn written(&self, span: Span) -> String {
    self.text[self.parsed.range(span)].to_owned()
  }

  /// The `content` of a `{ content }` file literal, as written.
  fn content(&self, arg: &Argument) -> Option<String> {
    let Expression::ObjectExpression(object) = argument(arg) else { return None };
    let [prop] = object.properties.as_slice() else { return None };
    match prop {
      ObjectPropertyKind::ObjectProperty(p) if p.shorthand => match &p.key {
        PropertyKey::StaticIdentifier(k) if k.name == "content" => Some(k.name.to_string()),
        _ => None,
      },
      other => match key_value(other) {
        Some(("content", value)) => Some(self.written(value.span())),
        _ => None,
      },
    }
  }
}

impl<'a> Visit<'a> for Uses<'_> {
  fn visit_import_declaration(&mut self, _: &js::ImportDeclaration<'a>) {}

  fn visit_object_property(&mut self, n: &js::ObjectProperty<'a>) {
    if n.shorthand {
      return;
    }
    walk::walk_object_property(self, n);
  }

  fn visit_variable_declaration(&mut self, n: &js::VariableDeclaration<'a>) {
    let binds = self.held.iter().any(|h| h.dynamic && h.binding.contains(&self.parsed.range(n.span).start));
    if !binds {
      walk::walk_variable_declaration(self, n);
    }
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    let callee = &n.callee;
    if self.is_function(callee) {
      self.edits.replace(self.parsed.range(callee.span()), format!("{}.frontMatter", self.member));
      for arg in &n.arguments {
        self.visit_argument(arg);
      }
      return;
    }
    if let Expression::StaticMemberExpression(m) = callee {
      if self.is_function(&m.object) && m.property.name == "stringify" {
        if let Some(content) = n.arguments.first().and_then(|a| self.content(a)) {
          self.edits.replace(self.parsed.range(callee.span()), format!("{}.withFrontMatter", self.member));
          self.edits.replace(self.parsed.range(n.arguments[0].span()), content);
          for arg in &n.arguments[1..] {
            self.visit_argument(arg);
          }
          return;
        }
        self.unmapped.push((self.parsed.line(n.span), self.written(n.span)));
        return;
      }
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    if self.is_function(n.object()) || matches!(n.object(), Expression::Identifier(i) if self.held(&i.name).is_some()) {
      self.unmapped.push((self.parsed.line(n.span()), self.written(n.span())));
      return;
    }
    walk::walk_member_expression(self, n);
  }

  fn visit_expression(&mut self, n: &Expression<'a>) {
    if let Expression::Identifier(i) = n {
      if self.held(&i.name).is_some() {
        self.unmapped.push((self.parsed.line(i.span), i.name.to_string()));
      }
      return;
    }
    walk::walk_expression(self, n);
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
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let held: Vec<(&'static str, Held)> = LOGGING.iter().flat_map(|p| held(&parsed, &text, p).into_iter().map(move |h| (*p, h))).collect();
      if held.is_empty() {
        continue;
      }
      let mut made = Made { parsed: &parsed, text: &text, held: &held, loggers: Vec::new(), options: Vec::new(), unmapped: Vec::new() };
      made.visit_program(&parsed.program);
      let Made { loggers, options, unmapped, .. } = made;
      let (member, import) = std_member(&parsed, "log");
      let mut calls = Calls { parsed: &parsed, text: &text, held: &held, loggers: &loggers, member: &member, edits: Edits::default(), unmapped };
      calls.visit_program(&parsed.program);
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
        let at = parsed.body().first().map(|item| parsed.range(item.span()).start).unwrap_or(0);
        edits.replace(at..at, format!("{import}\n"));
      }
      tree.insert(path, tidy(&edits.apply(&text)));
    }
  }
}

fn ident<'e>(e: &'e Expression) -> Option<&'e str> {
  match e {
    Expression::Identifier(i) => Some(i.name.as_str()),
    Expression::ParenthesizedExpression(p) => ident(&p.expression),
    _ => None,
  }
}

/// `object.name`, with `object` a plain name.
fn member_of<'e>(e: &'e Expression) -> Option<(&'e str, &'e str)> {
  let Expression::StaticMemberExpression(m) = e else { return None };
  Some((ident(&m.object)?, m.property.name.as_str()))
}

/// The text inside an object literal's braces, trimmed.
fn inside(written: &str) -> &str {
  written.trim().strip_prefix('{').and_then(|w| w.strip_suffix('}')).unwrap_or(written).trim()
}

/// Finds the loggers a file makes and the uses that rule the file out before any call is read.
struct Made<'a> {
  parsed: &'a Parsed<'a>,
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
  fn makes(&self, call: &js::CallExpression) -> Option<&'static str> {
    if let Some(name) = ident(&call.callee) {
      return self.package_of(name);
    }
    let (object, prop) = member_of(&call.callee)?;
    (prop == "createLogger").then(|| self.package_of(object)).flatten().filter(|p| *p == "winston")
  }
}

impl<'a> Visit<'a> for Made<'_> {
  fn visit_export_declaration(&mut self, n: &js::ExportDeclaration<'a>) {
    let before = self.loggers.len();
    walk::walk_export_declaration(self, n);
    if self.loggers.len() > before {
      let span = self.parsed.range(n.span);
      self.unmapped.push((self.parsed.line(n.span), self.text[span].to_owned()));
    }
  }

  fn visit_variable_declaration(&mut self, n: &js::VariableDeclaration<'a>) {
    let [declarator] = n.declarations.as_slice() else { return walk::walk_variable_declaration(self, n) };
    let (BindingPattern::BindingIdentifier(name), Some(Expression::CallExpression(call))) = (&declarator.id, &declarator.init) else { return walk::walk_variable_declaration(self, n) };
    let line = line_range(self.text, self.parsed.range(n.span));
    if let Some(package) = self.makes(call) {
      if !call.arguments.is_empty() {
        self.options.push((self.parsed.line(call.span), self.text[self.parsed.range(call.span)].to_owned()));
      }
      self.loggers.push(Logger { name: name.name.to_string(), package, bindings: None, line });
      return;
    }
    let Some((object, "child")) = member_of(&call.callee) else { return walk::walk_variable_declaration(self, n) };
    let Some(parent) = self.loggers.iter().find(|l| l.name == object) else { return };
    let fields = match call.arguments.as_slice() {
      [Argument::ObjectExpression(object)] => inside(&self.text[self.parsed.range(object.span)]).to_owned(),
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
    self.loggers.push(Logger { name: name.name.to_string(), package, bindings: Some(bindings), line });
  }
}

/// Rewrites each level call on a logger.
struct Calls<'a> {
  parsed: &'a Parsed<'a>,
  text: &'a str,
  held: &'a [(&'static str, Held)],
  loggers: &'a [Logger],
  member: &'a str,
  edits: Edits,
  unmapped: Vec<(usize, String)>,
}

impl Calls<'_> {
  fn written(&self, span: Span) -> &str {
    &self.text[self.parsed.range(span)]
  }

  fn logger(&self, name: &str) -> Option<&Logger> {
    self.loggers.iter().find(|l| l.name == name)
  }

  /// `fields` with the logger's `child` fields before them.
  fn merged(&self, logger: &Logger, fields: Option<&Expression>) -> Option<String> {
    match (&logger.bindings, fields) {
      (None, None) => None,
      (None, Some(fields)) => Some(self.written(fields.span()).to_owned()),
      (Some(bound), None) => Some(format!("{{ {bound} }}")),
      (Some(bound), Some(Expression::ObjectExpression(object))) => match inside(self.written(object.span)) {
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
  fn rewrite(&self, logger: &Logger, method: &str, call: &js::CallExpression) -> Option<String> {
    let args: Vec<&Expression> = call.arguments.iter().map(|a| a.as_expression()).collect::<Option<_>>()?;
    let (level, args) = match (logger.package, method) {
      ("winston", "log") => match args.split_first()? {
        (Expression::StringLiteral(level), rest) => (Self::level("winston", &lossy(level.value))?, rest.to_vec()),
        _ => return None,
      },
      (package, method) => (Self::level(package, method)?, args),
    };
    let textual = |e: &Expression| matches!(e, Expression::StringLiteral(_) | Expression::TemplateLiteral(_));
    let (message, fields) = match (logger.package, args.as_slice()) {
      ("pino", [only]) if matches!(only, Expression::ObjectExpression(_)) => ("\"\"".to_owned(), Some(*only)),
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

impl<'a> Visit<'a> for Calls<'_> {
  fn visit_import_declaration(&mut self, _: &js::ImportDeclaration<'a>) {}

  fn visit_object_property(&mut self, n: &js::ObjectProperty<'a>) {
    if n.shorthand {
      return;
    }
    walk::walk_object_property(self, n);
  }

  fn visit_variable_declaration(&mut self, n: &js::VariableDeclaration<'a>) {
    let start = self.parsed.range(n.span).start;
    if !self.loggers.iter().any(|l| l.line.contains(&start)) {
      walk::walk_variable_declaration(self, n);
    }
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if let Some((object, method)) = member_of(&n.callee) {
      if let Some(logger) = self.logger(object) {
        match self.rewrite(logger, method, n) {
          Some(written) => self.edits.replace(self.parsed.range(n.span), written),
          None => self.unmapped.push((self.parsed.line(n.span), self.written(n.span).to_owned())),
        }
        return;
      }
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    if ident(n.object()).is_some_and(|name| self.logger(name).is_some() || self.held.iter().any(|(_, h)| h.local == name)) {
      self.unmapped.push((self.parsed.line(n.span()), self.written(n.span()).to_owned()));
      return;
    }
    walk::walk_member_expression(self, n);
  }

  fn visit_expression(&mut self, n: &Expression<'a>) {
    if let Expression::Identifier(i) = n {
      if self.logger(&i.name).is_some() || self.held.iter().any(|(_, h)| i.name == h.local.as_str()) {
        self.unmapped.push((self.parsed.line(i.span), i.name.to_string()));
      }
      return;
    }
    walk::walk_expression(self, n);
  }
}
