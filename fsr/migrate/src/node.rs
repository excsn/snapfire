//! Node's `fs`, `path`, `os`, `child_process`, `crypto` and `async_hooks`
//! modules and its `process` global written as FSR's `file`, `path`, `system`,
//! `crypto` and `Scope` from the standard module.

use std::collections::BTreeSet;

use swc_core::common::{Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{Visit, VisitWith};

use crate::edit::Edits;
use crate::items::line_range;
use crate::packages::std_home;
use crate::syntax::{is_script, parse, Parsed};
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

/// How a Node member is written as FSR's: the standard member, the method on it and how many leading arguments it keeps, `None` for all. `call` is false for a value read such as `EOL`, which is written as a call.
struct Mapped {
  member: &'static str,
  method: &'static str,
  keep: Option<usize>,
  call: bool,
}

fn mapped(group: Group, name: &str) -> Option<Mapped> {
  let m = |member, method, keep| Some(Mapped { member, method, keep, call: true });
  match (group, name) {
    (Group::Fs, "existsSync") => m("file", "exists", Some(1)),
    (Group::Fs, "readFileSync" | "readFile") => m("file", "read", Some(1)),
    (Group::Fs, "writeFileSync" | "writeFile") => m("file", "write", Some(2)),
    (Group::Fs, "mkdirSync" | "mkdir") => m("file", "makeDir", Some(2)),
    (Group::Fs, "readdirSync" | "readdir") => m("file", "list", Some(1)),
    (Group::Fs, "statSync" | "stat") => m("file", "stat", Some(1)),
    (Group::Path, name @ ("join" | "resolve" | "normalize" | "dirname" | "basename" | "extname" | "relative" | "isAbsolute")) => Some(Mapped { member: "path", method: path_method(name), keep: None, call: true }),
    (Group::Os, "homedir") => m("system", "home", Some(0)),
    (Group::Os, "platform") => m("system", "platform", Some(0)),
    (Group::Os, "tmpdir") => m("system", "tempDir", Some(0)),
    (Group::Os, "EOL") => Some(Mapped { member: "system", method: "lineEnd", keep: Some(0), call: false }),
    (Group::ChildProcess, "spawn") => m("system", "spawn", None),
    (Group::Crypto, "randomUUID") => m("crypto", "uuid", Some(0)),
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
      let Ok(parsed) = parse(&path, &text) else { continue };
      let (held, imports) = held(&parsed);
      let mut uses = Uses { parsed: &parsed, text: &text, held: &held, edits: Edits::default(), members: BTreeSet::new(), unmapped: Vec::new(), process: Vec::new(), declared_process: declares(&parsed, "process"), scopes: Vec::new() };
      parsed.module.visit_with(&mut uses);
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
      parsed.module.visit_with(&mut names);
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
      let Ok(reparsed) = parse(&path, &written) else { continue };
      let mut out = Edits::default();
      let mut import = import;
      for item in &reparsed.module.body {
        let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(i)) = item else { continue };
        if group(&i.src.value.to_string_lossy()).is_none() && !matches!(i.src.value.to_string_lossy().as_ref(), "process" | "node:process") {
          continue;
        }
        let range = reparsed.range(i.span);
        match import.take() {
          Some(line) => out.replace(range, line),
          None => out.replace(line_range(&written, range), ""),
        }
      }
      if let Some(line) = import {
        let at = reparsed.module.body.first().map(|item| reparsed.range(item.span()).start).unwrap_or(0);
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
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    let Some(group) = group(&import.src.value.to_string_lossy()) else { continue };
    if import.type_only {
      continue;
    }
    imports.push(import.span);
    for specifier in &import.specifiers {
      match specifier {
        js::ImportSpecifier::Default(d) => held.push(Held { local: d.local.sym.to_string(), group, member: None }),
        js::ImportSpecifier::Namespace(n) => held.push(Held { local: n.local.sym.to_string(), group, member: None }),
        js::ImportSpecifier::Named(n) => {
          let imported = match &n.imported {
            Some(js::ModuleExportName::Ident(i)) => i.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_string_lossy().into_owned(),
            None => n.local.sym.to_string(),
          };
          if imported == "default" {
            held.push(Held { local: n.local.sym.to_string(), group, member: None });
          } else {
            held.push(Held { local: n.local.sym.to_string(), group, member: Some(imported) });
          }
        }
      }
    }
  }
  (held, imports)
}

/// Whether `parsed` declares `name` itself, which then shadows the global.
fn declares(parsed: &Parsed, name: &str) -> bool {
  struct Declares<'n>(&'n str, bool);
  impl Visit for Declares<'_> {
    fn visit_binding_ident(&mut self, id: &js::BindingIdent) {
      self.1 |= *id.id.sym == *self.0;
    }
  }
  let mut found = Declares(name, false);
  parsed.module.visit_with(&mut found);
  found.1
}

struct Names(BTreeSet<String>);

impl Visit for Names {
  fn visit_ident(&mut self, n: &js::Ident) {
    self.0.insert(n.sym.to_string());
  }
}

/// The marker a rewritten member is written with until the file's imports are known: `\0file.` becomes `file.` or `std_file.`.
fn marked(member: &str, method: &str) -> String {
  format!("\u{0}{member}.{method}")
}

struct Uses<'a> {
  parsed: &'a Parsed,
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
  fn member_of(&self, e: &js::Expr) -> Option<(Group, String)> {
    match e {
      js::Expr::Ident(id) => self.held(&id.sym).and_then(|h| h.member.clone().map(|m| (h.group, m))),
      js::Expr::Member(m) => {
        let js::MemberProp::Ident(prop) = &m.prop else { return None };
        match &*m.obj {
          js::Expr::Ident(id) => self.held(&id.sym).filter(|h| h.member.is_none()).map(|h| (h.group, prop.sym.to_string())),
          js::Expr::Member(inner) => match (&*inner.obj, &inner.prop) {
            (js::Expr::Ident(id), js::MemberProp::Ident(p)) if &*p.sym == "promises" => self.held(&id.sym).filter(|h| h.member.is_none() && h.group == Group::Fs).map(|h| (h.group, prop.sym.to_string())),
            _ => None,
          },
          _ => None,
        }
      }
      _ => None,
    }
  }

  /// Whether `e` names Node's `AsyncLocalStorage`.
  fn is_storage(&self, e: &js::Expr) -> bool {
    matches!(self.member_of(e), Some((Group::AsyncHooks, name)) if name == "AsyncLocalStorage")
  }

  /// `randomBytes(n).toString(encoding)` as `crypto.random(n, encoding)`, `hex` left out as the default.
  fn random_bytes(&mut self, n: &js::CallExpr) -> Option<String> {
    let js::Callee::Expr(callee) = &n.callee else { return None };
    let js::Expr::Member(m) = &**callee else { return None };
    if !matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "toString") {
      return None;
    }
    let js::Expr::Call(inner) = &*m.obj else { return None };
    let js::Callee::Expr(inner_callee) = &inner.callee else { return None };
    if !matches!(self.member_of(inner_callee), Some((Group::Crypto, name)) if name == "randomBytes") {
      return None;
    }
    let count = self.written(inner.args.first()?.span());
    let encoding = match n.args.first().map(|a| &*a.expr) {
      None => None,
      Some(js::Expr::Lit(js::Lit::Str(s))) if s.value.to_string_lossy() == "hex" => None,
      Some(js::Expr::Lit(js::Lit::Str(s))) if s.value.to_string_lossy() == "base64" => Some(self.written(s.span)),
      Some(_) => return None,
    };
    self.members.insert("crypto".to_owned());
    Some(match encoding {
      Some(encoding) => format!("{}({count}, {encoding})", marked("crypto", "random")),
      None => format!("{}({count})", marked("crypto", "random")),
    })
  }

  /// `createHash(algorithm).update(part)….digest(encoding)` as `crypto.digest(algorithm, encoding, part, …)`.
  fn hash_chain(&mut self, n: &js::CallExpr) -> Option<String> {
    let js::Callee::Expr(callee) = &n.callee else { return None };
    let js::Expr::Member(m) = &**callee else { return None };
    if !matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "digest") {
      return None;
    }
    let encoding = self.written(n.args.first()?.span());
    let mut parts = Vec::new();
    let mut at = &*m.obj;
    loop {
      let js::Expr::Call(call) = at else { return None };
      let js::Callee::Expr(callee) = &call.callee else { return None };
      if matches!(self.member_of(callee), Some((Group::Crypto, name)) if name == "createHash") {
        let algorithm = self.written(call.args.first()?.span());
        parts.reverse();
        self.members.insert("crypto".to_owned());
        let mut args = vec![algorithm, encoding];
        args.extend(parts);
        return Some(format!("{}({})", marked("crypto", "digest"), args.join(", ")));
      }
      let js::Expr::Member(update) = &**callee else { return None };
      if !matches!(&update.prop, js::MemberProp::Ident(p) if &*p.sym == "update") {
        return None;
      }
      parts.push(self.written(call.args.first()?.span()));
      at = &update.obj;
    }
  }

  fn is_process(&self, e: &js::Expr) -> bool {
    !self.declared_process && matches!(e, js::Expr::Ident(id) if &*id.sym == "process")
  }

  /// A `process` read written as FSR's, `None` for one with no FSR member.
  fn process_read(&mut self, e: &js::Expr) -> Option<String> {
    let js::Expr::Member(m) = e else { return None };
    match (&*m.obj, &m.prop) {
      (obj, js::MemberProp::Ident(p)) if self.is_process(obj) && &*p.sym == "platform" => Some(self.system("platform", "")),
      (js::Expr::Member(env), prop) if matches!(&env.prop, js::MemberProp::Ident(e) if &*e.sym == "env") && self.is_process(&env.obj) => match prop {
        js::MemberProp::Ident(name) => Some(self.system("env", &format!("\"{}\"", name.sym))),
        js::MemberProp::Computed(key) => {
          let key = self.written(key.expr.span());
          Some(self.system("env", &key))
        }
        js::MemberProp::PrivateName(_) => None,
      },
      _ => None,
    }
  }

  fn system(&mut self, method: &str, args: &str) -> String {
    self.members.insert("system".to_owned());
    format!("{}({args})", marked("system", method))
  }

  /// Whether `e` reads the platform, which FSR names as Rust does.
  fn reads_platform(&self, e: &js::Expr) -> bool {
    match e {
      js::Expr::Member(m) => self.is_process(&m.obj) && matches!(&m.prop, js::MemberProp::Ident(p) if &*p.sym == "platform"),
      js::Expr::Call(call) => matches!(&call.callee, js::Callee::Expr(callee) if matches!(self.member_of(callee), Some((Group::Os, name)) if name == "platform")),
      _ => false,
    }
  }
}

impl Visit for Uses<'_> {
  fn visit_import_decl(&mut self, _: &js::ImportDecl) {}

  fn visit_var_declarator(&mut self, n: &js::VarDeclarator) {
    if let (js::Pat::Ident(name), Some(js::Expr::New(made))) = (&n.name, n.init.as_deref()) {
      if self.is_storage(&made.callee) {
        self.scopes.push(name.id.sym.to_string());
      }
    }
    n.visit_children_with(self);
  }

  fn visit_new_expr(&mut self, n: &js::NewExpr) {
    if self.is_storage(&n.callee) {
      self.members.insert("Scope".to_owned());
      self.edits.replace(self.parsed.range(n.callee.span()), "\u{0}Scope");
      n.args.visit_with(self);
      return;
    }
    n.visit_children_with(self);
  }

  fn visit_ts_type_ref(&mut self, n: &js::TsTypeRef) {
    if let js::TsEntityName::Ident(id) = &n.type_name {
      if self.is_storage(&js::Expr::Ident(id.clone())) {
        self.members.insert("Scope".to_owned());
        self.edits.replace(self.parsed.range(id.span), "\u{0}Scope");
      }
    }
    n.visit_children_with(self);
  }

  fn visit_bin_expr(&mut self, n: &js::BinExpr) {
    if matches!(n.op, js::BinaryOp::EqEqEq | js::BinaryOp::NotEqEq | js::BinaryOp::EqEq | js::BinaryOp::NotEq) {
      for (side, other) in [(&n.left, &n.right), (&n.right, &n.left)] {
        if let (true, js::Expr::Lit(js::Lit::Str(s))) = (self.reads_platform(side), &**other) {
          let renamed = match s.value.to_string_lossy().as_ref() {
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
    n.visit_children_with(self);
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    let js::Callee::Expr(callee) = &n.callee else { return n.visit_children_with(self) };
    if let Some(written) = self.random_bytes(n).or_else(|| self.hash_chain(n)) {
      self.edits.replace(self.parsed.range(n.span), written);
      return;
    }
    if let js::Expr::Member(m) = &**callee {
      if let (js::Expr::Ident(object), js::MemberProp::Ident(method)) = (&*m.obj, &m.prop) {
        let renamed = match &*method.sym {
          "getStore" => Some("get"),
          "enterWith" => Some("set"),
          _ => None,
        };
        if let (true, Some(renamed)) = (self.scopes.iter().any(|s| *s == *object.sym), renamed) {
          self.edits.replace(self.parsed.range(method.span), renamed);
          n.args.visit_with(self);
          return;
        }
      }
    }
    if let Some((group, name)) = self.member_of(callee) {
      match mapped(group, &name) {
        Some(m) if m.call => {
          self.members.insert(m.member.to_owned());
          self.edits.replace(self.parsed.range(callee.span()), marked(m.member, m.method));
          let keep = m.keep.unwrap_or(usize::MAX).min(n.args.len());
          if keep < n.args.len() {
            let from = match keep {
              0 => self.parsed.range(n.args[0].span()).start,
              k => self.parsed.range(n.args[k - 1].span()).end,
            };
            let to = self.parsed.range(n.args[n.args.len() - 1].span()).end;
            self.edits.replace(from..to, "");
          }
          n.args.iter().take(keep).for_each(|a| a.visit_with(self));
        }
        _ => self.unmapped.push((self.parsed.line(n.span), self.written(n.span))),
      }
      return;
    }
    if let js::Expr::Member(m) = &**callee {
      if let (true, js::MemberProp::Ident(p)) = (self.is_process(&m.obj), &m.prop) {
        if &*p.sym == "cwd" && n.args.is_empty() {
          let written = self.system("cwd", "");
          self.edits.replace(self.parsed.range(n.span), written);
          return;
        }
      }
    }
    n.visit_children_with(self);
  }

  fn visit_member_expr(&mut self, n: &js::MemberExpr) {
    if let (js::Expr::Call(call), js::MemberProp::Ident(p)) = (&*n.obj, &n.prop) {
      if let js::Callee::Expr(callee) = &call.callee {
        if let js::Expr::Member(m) = &**callee {
          if self.is_process(&m.obj) && matches!(&m.prop, js::MemberProp::Ident(f) if &*f.sym == "memoryUsage") && &*p.sym == "rss" {
            let written = self.system("memory", "");
            self.edits.replace(self.parsed.range(n.span), written);
            return;
          }
        }
      }
    }
    if let Some(read) = self.process_read(&js::Expr::Member(n.clone())) {
      self.edits.replace(self.parsed.range(n.span), read);
      if let js::MemberProp::Computed(key) = &n.prop {
        key.visit_with(self);
      }
      return;
    }
    if let Some((group, name)) = self.member_of(&js::Expr::Member(n.clone())) {
      match mapped(group, &name) {
        Some(m) if !m.call => {
          self.members.insert(m.member.to_owned());
          self.edits.replace(self.parsed.range(n.span), format!("{}()", marked(m.member, m.method)));
        }
        _ => self.unmapped.push((self.parsed.line(n.span), self.written(n.span))),
      }
      return;
    }
    if self.is_process(&n.obj) {
      self.process.push((self.parsed.line(n.span), self.written(n.span)));
      return;
    }
    n.visit_children_with(self);
  }

  fn visit_expr(&mut self, n: &js::Expr) {
    if let js::Expr::Ident(id) = n {
      if let Some(h) = self.held(&id.sym) {
        match h.member.as_deref().and_then(|member| mapped(h.group, member)) {
          Some(m) if !m.call => {
            self.members.insert(m.member.to_owned());
            self.edits.replace(self.parsed.range(id.span), format!("{}()", marked(m.member, m.method)));
          }
          _ => self.unmapped.push((self.parsed.line(id.span), id.sym.to_string())),
        }
        return;
      }
      if self.is_process(n) {
        self.process.push((self.parsed.line(id.span), id.sym.to_string()));
      }
      return;
    }
    n.visit_children_with(self);
  }
}
