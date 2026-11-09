//! Node's `fs`, `path`, `os`, `child_process`, `crypto` and `async_hooks`
//! modules and its `process` global written as FSR's `file`, `path`, `system`,
//! `crypto` and `Scope` from the standard module.

use std::collections::BTreeSet;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, BinaryOperator, BindingPattern, Expression, ImportDeclarationSpecifier, MemberExpression, ModuleExportName, Statement};
use oxc_ast_visit::{walk, Visit};
use oxc_span::{GetSpan, Span};

use crate::edit::Edits;
use crate::items::line_range;
use crate::packages::std_home;
use crate::syntax::{argument, import_specifiers, is_script, lossy, parse, Parsed};
use crate::{Pass, Report, Tree};

pub(crate) fn passes() -> Vec<Box<dyn Pass>> {
  vec![Box::new(Node)]
}

#[derive(Clone, Copy, PartialEq)]
enum Group {
  Fs,
  Path,
  Os,
  ChildProcess,
  Crypto,
  AsyncHooks,
}

fn group(source: &str) -> Option<Group> {
  match source.strip_prefix("node:").unwrap_or(source) {
    "fs" | "fs/promises" => Some(Group::Fs),
    "path" | "path/posix" => Some(Group::Path),
    "os" => Some(Group::Os),
    "child_process" => Some(Group::ChildProcess),
    "crypto" => Some(Group::Crypto),
    "async_hooks" => Some(Group::AsyncHooks),
    _ => None,
  }
}

/// How a Node member is written as FSR's: the standard member, the method on it and how many leading arguments it keeps, `None` for all. `call` is false for a value read such as `EOL`, which is written as a call. `platform` is true for the platform's own global, `crypto.randomUUID`, which needs no import.
struct Mapped {
  member: &'static str,
  method: &'static str,
  keep: Option<usize>,
  call: bool,
  platform: bool,
}

fn mapped(group: Group, name: &str) -> Option<Mapped> {
  let m = |member, method, keep| Some(Mapped { member, method, keep, call: true, platform: false });
  match (group, name) {
    (Group::Fs, "existsSync") => m("file", "exists", Some(1)),
    (Group::Fs, "readFileSync" | "readFile") => m("file", "read", Some(1)),
    (Group::Fs, "writeFileSync" | "writeFile") => m("file", "write", Some(2)),
    (Group::Fs, "mkdirSync" | "mkdir") => m("file", "makeDir", Some(2)),
    (Group::Fs, "readdirSync" | "readdir") => m("file", "list", Some(1)),
    (Group::Fs, "statSync" | "stat") => m("file", "stat", Some(1)),
    (Group::Path, name @ ("join" | "resolve" | "normalize" | "dirname" | "basename" | "extname" | "relative" | "isAbsolute")) => Some(Mapped { member: "path", method: path_method(name), keep: None, call: true, platform: false }),
    (Group::Os, "homedir") => m("system", "home", Some(0)),
    (Group::Os, "platform") => m("system", "platform", Some(0)),
    (Group::Os, "tmpdir") => m("system", "tempDir", Some(0)),
    (Group::Os, "EOL") => Some(Mapped { member: "system", method: "lineEnd", keep: Some(0), call: false, platform: false }),
    (Group::ChildProcess, "spawn") => m("system", "spawn", None),
    (Group::Crypto, "randomUUID") => Some(Mapped { member: "crypto", method: "randomUUID", keep: Some(0), call: true, platform: true }),
    _ => None,
  }
}

fn path_method(name: &str) -> &'static str {
  ["join", "resolve", "normalize", "dirname", "basename", "extname", "relative", "isAbsolute"].into_iter().find(|n| *n == name).expect("listed")
}

/// A name a file holds a Node module under.
struct Held {
  local: String,
  group: Group,
  /// `None` for the module itself, a default or namespace import; the member's name for a named import.
  member: Option<String>,
}

/// Node's modules, a named member or the module, and the `process` global, as the standard module's members: `fs` as `file`, `path` as `path`, `os` and `spawn` as `system`, `randomUUID`, `randomBytes(n).toString(encoding)` and a `createHash` chain as `crypto` and an `AsyncLocalStorage` as a `Scope`. A file with a use of a Node module that has no FSR member is left as written with a note for each such use. A `process` use with no FSR member is noted and the rest of the file is still written.
struct Node;

impl Pass for Node {
  fn name(&self) -> &'static str {
    "node"
  }

  fn run(&self, tree: &mut Tree, report: &mut Report) {
    let paths: Vec<String> = tree.paths().filter(|p| p.starts_with("app/") && is_script(p)).map(str::to_owned).collect();
    for path in paths {
      let Some(text) = tree.text(&path).map(str::to_owned) else { continue };
      let allocator = Allocator::default();
      let Ok(parsed) = parse(&allocator, &path, &text) else { continue };
      let (held, imports) = held(&parsed);
      let mut uses = Uses { parsed: &parsed, text: &text, held: &held, edits: Edits::default(), members: BTreeSet::new(), unmapped: Vec::new(), process: Vec::new(), declared_process: declares(&parsed, "process"), scopes: Vec::new() };
      uses.visit_program(&parsed.program);
      let Uses { edits, members, unmapped, process, .. } = uses;
      for (line, written) in &process {
        report.note(&path, *line, format!("`{written}`: `process` is not FSR's; in a component, `useApp().exit(code)` ends the page and `useStdout().stdout` is the terminal's output, so this is left for the application to write"));
      }
      if !unmapped.is_empty() {
        for (line, written) in unmapped {
          report.note(&path, line, format!("`{written}`: FSR has no member for this use of a Node module, so the file is left as written"));
        }
        continue;
      }
      if members.is_empty() && imports.is_empty() {
        continue;
      }
      let locals: BTreeSet<&str> = held.iter().map(|h| h.local.as_str()).collect();
      let mut names = Names(BTreeSet::new());
      names.visit_program(&parsed.program);
      let taken = |member: &str| names.0.contains(member) && !locals.contains(member);
      let mut specifiers: Vec<String> = Vec::new();
      let mut renamed: Vec<(String, String)> = Vec::new();
      for member in &members {
        if taken(member) {
          specifiers.push(format!("{member} as std_{member}"));
          renamed.push((member.clone(), format!("std_{member}")));
        } else {
          specifiers.push(member.clone());
        }
      }
      let mut by_home: std::collections::BTreeMap<&str, Vec<String>> = std::collections::BTreeMap::new();
      for (member, specifier) in members.iter().zip(&specifiers) {
        by_home.entry(std_home(member)).or_default().push(specifier.clone());
      }
      let import = (!specifiers.is_empty()).then(|| by_home.iter().map(|(home, names)| format!("import {{ {} }} from \"{home}\";", names.join(", "))).collect::<Vec<_>>().join("\n"));
      let mut written = edits.apply(&text);
      for (member, alias) in &renamed {
        written = written.replace(&format!("\u{0}{member}"), alias);
      }
      for member in &members {
        written = written.replace(&format!("\u{0}{member}"), member);
      }
      let reallocator = Allocator::default();
      let Ok(reparsed) = parse(&reallocator, &path, &written) else { continue };
      let mut out = Edits::default();
      let mut import = import;
      for item in reparsed.body() {
        let Statement::ImportDeclaration(i) = item else { continue };
        let source = lossy(i.source.value);
        if group(&source).is_none() && !matches!(source.as_str(), "process" | "node:process") {
          continue;
        }
        let range = reparsed.range(i.span);
        match import.take() {
          Some(line) => out.replace(range, line),
          None => out.replace(line_range(&written, range), ""),
        }
      }
      if let Some(line) = import {
        let at = reparsed.body().first().map(|item| reparsed.range(item.span()).start).unwrap_or(0);
        out.replace(at..at, format!("{line}\n"));
      }
      let finished = out.apply(&written);
      if finished != text {
        tree.insert(path, finished);
      }
    }
  }
}

/// The names `parsed` holds Node's `fs`, `path` and `os` under and their import declarations.
fn held(parsed: &Parsed) -> (Vec<Held>, Vec<Span>) {
  let mut held = Vec::new();
  let mut imports = Vec::new();
  for item in parsed.body() {
    let Statement::ImportDeclaration(import) = item else { continue };
    let Some(group) = group(&lossy(import.source.value)) else { continue };
    if import.import_kind.is_type() {
      continue;
    }
    imports.push(import.span);
    for specifier in import_specifiers(import) {
      match specifier {
        ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => held.push(Held { local: d.local.name.to_string(), group, member: None }),
        ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => held.push(Held { local: n.local.name.to_string(), group, member: None }),
        ImportDeclarationSpecifier::ImportSpecifier(n) => {
          let imported = match &n.imported {
            ModuleExportName::IdentifierName(i) => i.name.to_string(),
            ModuleExportName::IdentifierReference(i) => i.name.to_string(),
            ModuleExportName::StringLiteral(s) => lossy(s.value),
          };
          if imported == "default" {
            held.push(Held { local: n.local.name.to_string(), group, member: None });
          } else {
            held.push(Held { local: n.local.name.to_string(), group, member: Some(imported) });
          }
        }
      }
    }
  }
  (held, imports)
}

/// Whether `parsed` binds `name` in a pattern, a variable, a parameter or a
/// `catch` clause, which then shadows the global. An import, a function and
/// a class do not count.
fn declares(parsed: &Parsed, name: &str) -> bool {
  struct Declares<'n>(&'n str, bool);
  impl<'a> Visit<'a> for Declares<'_> {
    fn visit_binding_pattern(&mut self, pattern: &BindingPattern<'a>) {
      if let BindingPattern::BindingIdentifier(id) = pattern {
        self.1 |= id.name == self.0;
      }
      walk::walk_binding_pattern(self, pattern);
    }
  }
  let mut found = Declares(name, false);
  found.visit_program(&parsed.program);
  found.1
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

/// The marker a rewritten member is written with until the file's imports are known: `\0file.` becomes `file.` or `std_file.`.
fn marked(member: &str, method: &str) -> String {
  format!("\u{0}{member}.{method}")
}

struct Uses<'a> {
  parsed: &'a Parsed<'a>,
  text: &'a str,
  held: &'a [Held],
  edits: Edits,
  members: BTreeSet<String>,
  unmapped: Vec<(usize, String)>,
  process: Vec<(usize, String)>,
  declared_process: bool,
  /// The names an `AsyncLocalStorage` is held under, whose `getStore` and `enterWith` are a `Scope`'s `get` and `set`.
  scopes: Vec<String>,
}

impl Uses<'_> {
  fn written(&self, span: Span) -> String {
    self.text[self.parsed.range(span)].to_owned()
  }

  fn held(&self, name: &str) -> Option<&Held> {
    self.held.iter().find(|h| h.local == name)
  }

  /// The Node member `callee` names: a named import, `module.member` or `module.promises.member`.
  fn member_of(&self, e: &Expression) -> Option<(Group, String)> {
    match e {
      Expression::Identifier(id) => self.named(&id.name),
      other => self.member_of_member(other.as_member_expression()?),
    }
  }

  /// The Node member a named import held under `name` is.
  fn named(&self, name: &str) -> Option<(Group, String)> {
    self.held(name).and_then(|h| h.member.clone().map(|m| (h.group, m)))
  }

  fn member_of_member(&self, m: &MemberExpression) -> Option<(Group, String)> {
    let MemberExpression::StaticMemberExpression(m) = m else { return None };
    let prop = &m.property.name;
    match &m.object {
      Expression::Identifier(id) => self.held(&id.name).filter(|h| h.member.is_none()).map(|h| (h.group, prop.to_string())),
      Expression::StaticMemberExpression(inner) => match &inner.object {
        Expression::Identifier(id) if inner.property.name == "promises" => self.held(&id.name).filter(|h| h.member.is_none() && h.group == Group::Fs).map(|h| (h.group, prop.to_string())),
        _ => None,
      },
      _ => None,
    }
  }

  /// Whether `e` names Node's `AsyncLocalStorage`.
  fn is_storage(&self, e: &Expression) -> bool {
    matches!(self.member_of(e), Some((Group::AsyncHooks, name)) if name == "AsyncLocalStorage")
  }

  /// `randomBytes(n).toString(encoding)` as `crypto.random(n, encoding)`, `hex` left out as the default.
  fn random_bytes(&mut self, n: &js::CallExpression) -> Option<String> {
    let Expression::StaticMemberExpression(m) = &n.callee else { return None };
    if m.property.name != "toString" {
      return None;
    }
    let Expression::CallExpression(inner) = &m.object else { return None };
    if !matches!(self.member_of(&inner.callee), Some((Group::Crypto, name)) if name == "randomBytes") {
      return None;
    }
    let count = self.written(inner.arguments.first()?.span());
    let encoding = match n.arguments.first().map(argument) {
      None => None,
      Some(Expression::StringLiteral(s)) if s.value == "hex" => None,
      Some(Expression::StringLiteral(s)) if s.value == "base64" => Some(self.written(s.span)),
      Some(_) => return None,
    };
    self.members.insert("crypto".to_owned());
    Some(match encoding {
      Some(encoding) => format!("{}({count}, {encoding})", marked("crypto", "random")),
      None => format!("{}({count})", marked("crypto", "random")),
    })
  }

  /// `createHash(algorithm).update(part)….digest(encoding)` as `crypto.digest(algorithm, encoding, part, …)`.
  fn hash_chain(&mut self, n: &js::CallExpression) -> Option<String> {
    let Expression::StaticMemberExpression(m) = &n.callee else { return None };
    if m.property.name != "digest" {
      return None;
    }
    let encoding = self.written(n.arguments.first()?.span());
    let mut parts = Vec::new();
    let mut at = &m.object;
    loop {
      let Expression::CallExpression(call) = at else { return None };
      if matches!(self.member_of(&call.callee), Some((Group::Crypto, name)) if name == "createHash") {
        let algorithm = self.written(call.arguments.first()?.span());
        parts.reverse();
        self.members.insert("crypto".to_owned());
        let mut args = vec![algorithm, encoding];
        args.extend(parts);
        return Some(format!("{}({})", marked("crypto", "digest"), args.join(", ")));
      }
      let Expression::StaticMemberExpression(update) = &call.callee else { return None };
      if update.property.name != "update" {
        return None;
      }
      parts.push(self.written(call.arguments.first()?.span()));
      at = &update.object;
    }
  }

  fn is_process(&self, e: &Expression) -> bool {
    !self.declared_process && matches!(e, Expression::Identifier(id) if id.name == "process")
  }

  /// A `process` read written as FSR's, `None` for one with no FSR member.
  fn process_read(&mut self, m: &MemberExpression) -> Option<String> {
    if let MemberExpression::StaticMemberExpression(s) = m {
      if self.is_process(&s.object) && s.property.name == "platform" {
        return Some(self.system("platform", ""));
      }
    }
    let Expression::StaticMemberExpression(env) = m.object() else { return None };
    if env.property.name != "env" || !self.is_process(&env.object) {
      return None;
    }
    match m {
      MemberExpression::StaticMemberExpression(s) => Some(self.system("env", &format!("\"{}\"", s.property.name))),
      MemberExpression::ComputedMemberExpression(c) => {
        let key = self.written(c.expression.span());
        Some(self.system("env", &key))
      }
      MemberExpression::PrivateFieldExpression(_) => None,
    }
  }

  fn system(&mut self, method: &str, args: &str) -> String {
    self.members.insert("system".to_owned());
    format!("{}({args})", marked("system", method))
  }

  /// Whether `e` reads the platform, which FSR names as Rust does.
  fn reads_platform(&self, e: &Expression) -> bool {
    match e {
      Expression::StaticMemberExpression(m) => self.is_process(&m.object) && m.property.name == "platform",
      Expression::CallExpression(call) => matches!(self.member_of(&call.callee), Some((Group::Os, name)) if name == "platform"),
      _ => false,
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

  fn visit_variable_declarator(&mut self, n: &js::VariableDeclarator<'a>) {
    if let (BindingPattern::BindingIdentifier(name), Some(Expression::NewExpression(made))) = (&n.id, &n.init) {
      if self.is_storage(&made.callee) {
        self.scopes.push(name.name.to_string());
      }
    }
    walk::walk_variable_declarator(self, n);
  }

  fn visit_new_expression(&mut self, n: &js::NewExpression<'a>) {
    if self.is_storage(&n.callee) {
      self.members.insert("Scope".to_owned());
      self.edits.replace(self.parsed.range(n.callee.span()), "\u{0}Scope");
      for arg in &n.arguments {
        self.visit_argument(arg);
      }
      return;
    }
    walk::walk_new_expression(self, n);
  }

  fn visit_ts_type_reference(&mut self, n: &js::TSTypeReference<'a>) {
    if let js::TSTypeName::IdentifierReference(id) = &n.type_name {
      if matches!(self.named(&id.name), Some((Group::AsyncHooks, name)) if name == "AsyncLocalStorage") {
        self.members.insert("Scope".to_owned());
        self.edits.replace(self.parsed.range(id.span), "\u{0}Scope");
      }
    }
    walk::walk_ts_type_reference(self, n);
  }

  fn visit_binary_expression(&mut self, n: &js::BinaryExpression<'a>) {
    if matches!(n.operator, BinaryOperator::StrictEquality | BinaryOperator::StrictInequality | BinaryOperator::Equality | BinaryOperator::Inequality) {
      for (side, other) in [(&n.left, &n.right), (&n.right, &n.left)] {
        if let (true, Expression::StringLiteral(s)) = (self.reads_platform(side), other) {
          let renamed = match lossy(s.value).as_str() {
            "darwin" => Some("macos"),
            "win32" => Some("windows"),
            _ => None,
          };
          if let Some(renamed) = renamed {
            let range = self.parsed.range(s.span);
            let quote = &self.text[range.start..range.start + 1];
            self.edits.replace(range, format!("{quote}{renamed}{quote}"));
          }
        }
      }
    }
    walk::walk_binary_expression(self, n);
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    let callee = &n.callee;
    if let Some(written) = self.random_bytes(n).or_else(|| self.hash_chain(n)) {
      self.edits.replace(self.parsed.range(n.span), written);
      return;
    }
    if let Expression::StaticMemberExpression(m) = callee {
      if let Expression::Identifier(object) = &m.object {
        let renamed = match m.property.name.as_str() {
          "getStore" => Some("get"),
          "enterWith" => Some("set"),
          _ => None,
        };
        if let (true, Some(renamed)) = (self.scopes.iter().any(|s| object.name == s.as_str()), renamed) {
          self.edits.replace(self.parsed.range(m.property.span), renamed);
          for arg in &n.arguments {
            self.visit_argument(arg);
          }
          return;
        }
      }
    }
    if let Some((group, name)) = self.member_of(callee) {
      match mapped(group, &name) {
        Some(m) if m.call => {
          let written = match m.platform {
            true => format!("{}.{}", m.member, m.method),
            false => {
              self.members.insert(m.member.to_owned());
              marked(m.member, m.method)
            }
          };
          self.edits.replace(self.parsed.range(callee.span()), written);
          let args = &n.arguments;
          let keep = m.keep.unwrap_or(usize::MAX).min(args.len());
          if keep < args.len() {
            let from = match keep {
              0 => self.parsed.range(args[0].span()).start,
              k => self.parsed.range(args[k - 1].span()).end,
            };
            let to = self.parsed.range(args[args.len() - 1].span()).end;
            self.edits.replace(from..to, "");
          }
          args.iter().take(keep).for_each(|a| self.visit_argument(a));
        }
        _ => self.unmapped.push((self.parsed.line(n.span), self.written(n.span))),
      }
      return;
    }
    if let Expression::StaticMemberExpression(m) = callee {
      if self.is_process(&m.object) && m.property.name == "cwd" && n.arguments.is_empty() {
        let written = self.system("cwd", "");
        self.edits.replace(self.parsed.range(n.span), written);
        return;
      }
    }
    walk::walk_call_expression(self, n);
  }

  fn visit_member_expression(&mut self, n: &MemberExpression<'a>) {
    if let MemberExpression::StaticMemberExpression(outer) = n {
      if let Expression::CallExpression(call) = &outer.object {
        if let Expression::StaticMemberExpression(m) = &call.callee {
          if self.is_process(&m.object) && m.property.name == "memoryUsage" && outer.property.name == "rss" {
            let written = self.system("memory", "");
            self.edits.replace(self.parsed.range(n.span()), written);
            return;
          }
        }
      }
    }
    if let Some(read) = self.process_read(n) {
      self.edits.replace(self.parsed.range(n.span()), read);
      if let MemberExpression::ComputedMemberExpression(key) = n {
        self.visit_expression(&key.expression);
      }
      return;
    }
    if let Some((group, name)) = self.member_of_member(n) {
      match mapped(group, &name) {
        Some(m) if !m.call => {
          self.members.insert(m.member.to_owned());
          self.edits.replace(self.parsed.range(n.span()), format!("{}()", marked(m.member, m.method)));
        }
        _ => self.unmapped.push((self.parsed.line(n.span()), self.written(n.span()))),
      }
      return;
    }
    if self.is_process(n.object()) {
      self.process.push((self.parsed.line(n.span()), self.written(n.span())));
      return;
    }
    walk::walk_member_expression(self, n);
  }

  fn visit_expression(&mut self, n: &Expression<'a>) {
    if let Expression::Identifier(id) = n {
      if let Some(h) = self.held(&id.name) {
        match h.member.as_deref().and_then(|member| mapped(h.group, member)) {
          Some(m) if !m.call => {
            self.members.insert(m.member.to_owned());
            self.edits.replace(self.parsed.range(id.span), format!("{}()", marked(m.member, m.method)));
          }
          _ => self.unmapped.push((self.parsed.line(id.span), id.name.to_string())),
        }
        return;
      }
      if self.is_process(n) {
        self.process.push((self.parsed.line(id.span), id.name.to_string()));
      }
      return;
    }
    walk::walk_expression(self, n);
  }
}
