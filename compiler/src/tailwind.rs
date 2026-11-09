//! Stylesheets that use Tailwind. Such a sheet is run through `snapfire_tailwind` with the
//! classes the driver listed before LightningCSS sees it, so the utilities it asks for are in the
//! output like any rule written by hand.

use std::cell::RefCell;

use swc_core::ecma::visit::VisitWith;
use std::path::{Path, PathBuf};

use snapfire_tailwind::{CompileOptions, Compiler, Error, Loaded, Root, tailwind_stylesheet};

/// What expanding one stylesheet produced. `reads` holds every file it read besides itself,
/// whether or not it succeeded, so a change to any of them compiles it again.
#[non_exhaustive]
pub struct Expanded {
  pub css: Result<String, String>,
  pub reads: Vec<PathBuf>,
  pub warnings: Vec<String>,
  /// Each listed class that is not a utility and that the stylesheet does not define, with the
  /// modules using it.
  pub unknown: Vec<(String, String)>,
}

const AT_RULES: &[&str] = &["@tailwind", "@theme", "@apply", "@utility", "@custom-variant", "@variant", "@reference", "@source", "@plugin", "@config"];
const FUNCTIONS: &[&str] = &["--theme(", "--spacing(", "--alpha(", "theme("];

/// Whether `css` uses Tailwind: it imports `tailwindcss`, uses one of its at-rules or calls one
/// of its functions.
pub fn uses_tailwind(css: &str) -> bool {
  css.lines().any(|line| {
    let line = line.trim_start();
    (line.starts_with("@import") && line.contains("tailwindcss")) || AT_RULES.iter().any(|at| line.starts_with(at) && line[at.len()..].starts_with([' ', ';', '{', '('])) || FUNCTIONS.iter().any(|f| line.contains(f))
  })
}

/// The classes file a driver writes: one class per line, optionally followed by a tab and the
/// modules that use it, separated by spaces.
pub fn read_classes(path: &Path) -> Vec<(String, String)> {
  std::fs::read_to_string(path)
    .unwrap_or_default()
    .lines()
    .filter(|line| !line.trim().is_empty())
    .map(|line| match line.split_once('\t') {
      Some((class, modules)) => (class.to_owned(), modules.trim().to_owned()),
      None => (line.trim().to_owned(), String::new()),
    })
    .collect()
}

/// Runs `content`, the stylesheet at `path`, through Tailwind with the classes in `classes` and
/// `extra`.
/// `quiet` classes build their utilities too but are never reported as unknown: they come from
/// vendored code, where a string that is not a utility is not the app's typo.
/// An import resolves to one of Tailwind's own stylesheets first, then to a file relative to
/// the stylesheet that names it.
pub fn expand(path: &Path, content: &str, classes: Option<&Path>, extra: &[(String, String)], quiet: &[(String, String)], source_name: &str) -> Expanded {
  let reads = RefCell::new(Vec::new());
  let mut listed = match classes {
    Some(file) => {
      reads.borrow_mut().push(file.to_path_buf());
      read_classes(file)
    }
    None => Vec::new(),
  };
  let known: std::collections::HashSet<String> = listed.iter().map(|(class, _)| class.clone()).collect();
  listed.extend(extra.iter().filter(|(class, _)| !known.contains(class)).cloned());
  let dir = path.parent().unwrap_or_else(|| Path::new(".")).to_string_lossy().into_owned();

  let load = |id: &str, base: &str| -> Result<Loaded, Error> {
    if let Some(loaded) = tailwind_stylesheet(id, base) {
      return Ok(loaded);
    }
    let file = crate::graph::normalise(&Path::new(base).join(id));
    reads.borrow_mut().push(file.clone());
    let content = std::fs::read_to_string(&file).map_err(|e| Error::Message(format!("Could not read `{id}` from {base}: {e}")))?;
    Ok(Loaded::new(file.to_string_lossy().into_owned(), file.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned(), content))
  };

  let options = CompileOptions::default().base(dir).load_stylesheet(Some(Box::new(load))).from(Some(source_name.to_owned()));
  let compiled = Compiler::new(content, options).and_then(|compiler| unread_sources(&compiler).map_or(Ok(compiler), Err)).map(|mut compiler| {
    let css = compiler.build(listed.iter().chain(quiet).map(|(class, _)| class.as_str()));
    let invalid = compiler.invalid_candidates();
    let defined = defined_classes(&css);
    let unknown: Vec<(String, String)> = listed.iter().filter(|(class, _)| invalid.binary_search(class).is_ok() && !defined.contains(class.as_str())).cloned().collect();
    let warnings = compiler.warnings().into_iter().map(|w| format!("{source_name}: {w}")).collect();
    (css, warnings, unknown)
  });

  let reads = reads.into_inner();
  match compiled {
    Ok((css, warnings, unknown)) => Expanded { css: Ok(css), reads, warnings, unknown },
    Err(e) => Expanded { css: Err(e.to_string()), reads, warnings: Vec::new(), unknown: Vec::new() },
  }
}

/// A `@source` path or a `source("dir")` on the import: snapfirec reads classes from the sources
/// it compiles and the classes file, never from files a stylesheet names, so a class only such a
/// file holds would build no utility.
fn unread_sources(compiler: &Compiler) -> Option<Error> {
  let remedy = "snapfirec reads classes from the sources it compiles and from --tailwind-classes, not from files a stylesheet names; list the classes with `@source inline(\"...\")` instead";
  if let Some(source) = compiler.sources().first() {
    let line = if source.negated { format!("@source not \"{}\"", source.pattern) } else { format!("@source \"{}\"", source.pattern) };
    return Some(Error::Message(format!("`{line}`: {remedy}")));
  }
  if let Some(Root::Path { pattern, .. }) = compiler.root() {
    return Some(Error::Message(format!("`source(\"{pattern}\")`: {remedy}")));
  }
  None
}

/// The class names a stylesheet's selectors name, so a class an author wrote a rule for is not
/// mistaken for a misspelt utility. Declarations and strings are skipped.
pub fn defined_classes(css: &str) -> std::collections::BTreeSet<&str> {
  let mut defined = std::collections::BTreeSet::new();
  let bytes = css.as_bytes();
  let mut in_declaration = false;
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' | b';' | b'}' => in_declaration = declaration_follows(&bytes[i + 1..]),
      b'"' | b'\'' => {
        let quote = bytes[i];
        i += 1;
        while i < bytes.len() && bytes[i] != quote {
          i += if bytes[i] == b'\\' { 2 } else { 1 };
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i = css[i + 2..].find("*/").map_or(bytes.len(), |end| i + 2 + end + 1);
      }
      b'.' if !in_declaration && bytes.get(i + 1).is_some_and(|b| b.is_ascii_alphabetic() || *b == b'_' || *b == b'-') => {
        let start = i + 1;
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'-' || bytes[end] == b'_') {
          end += 1;
        }
        defined.insert(&css[start..end]);
        i = end;
        continue;
      }
      _ => {}
    }
    i += 1;
  }
  defined
}

/// Whether the text after a `{` starts with a declaration rather than a nested rule, so a
/// number like `.5` in a value is not read as a class.
fn declaration_follows(rest: &[u8]) -> bool {
  let text = String::from_utf8_lossy(&rest[..rest.len().min(200)]);
  let head = text.trim_start();
  let until = head.find([';', '{', '}']).unwrap_or(head.len());
  let first = &head[..until];
  first.contains(':') && !head[until..].starts_with('{')
}

/// The class names a script names where a class goes: a `class` or `className` attribute or
/// property, an argument to `clsx`, `cn`, `cx`, `classnames`, `twMerge` or Vue's
/// `normalizeClass` or what Svelte's compiled output hands its runtime's `from_html` and
/// `set_class`. Whole tokens only: a token a template expression touches is left out.
pub fn source_classes(path: &Path, source: &str) -> Vec<String> {
  let Some(program) = parse(path, source, true) else { return Vec::new() };
  let mut consts = Consts::default();
  program.visit_with(&mut consts);
  let mut collector = Classes { consts: Some(&consts.table), aliases: helper_aliases(&program), svelte: svelte_runtimes(&program), ..Classes::default() };
  program.visit_with(&mut collector);
  let mut found: Vec<String> = collector.found.into_iter().collect();
  found.sort();
  found
}

/// `source` parsed by its extension: TypeScript for `.ts`, TSX for `.tsx` and anything unknown,
/// JavaScript with JSX for `.js`, `.mjs`, `.cjs` and `.jsx`. A parse that fails reads as nothing.
fn parse(path: &Path, source: &str, unknown_is_tsx: bool) -> Option<swc_core::ecma::ast::Program> {
  use swc_core::common::{FileName, SourceMap, sync::Lrc};
  use swc_core::ecma::ast::EsVersion;
  use swc_core::ecma::parser::{EsSyntax, Parser, StringInput, Syntax, TsSyntax, lexer::Lexer};
  let extension = path.extension().and_then(|e| e.to_str()).unwrap_or_default();
  let syntax = match extension {
    "ts" | "mts" | "cts" => Syntax::Typescript(TsSyntax { decorators: true, ..Default::default() }),
    "tsx" => Syntax::Typescript(TsSyntax { tsx: true, decorators: true, ..Default::default() }),
    "js" | "mjs" | "cjs" | "jsx" => Syntax::Es(EsSyntax { jsx: true, decorators: true, ..Default::default() }),
    _ if unknown_is_tsx => Syntax::Typescript(TsSyntax { tsx: true, decorators: true, ..Default::default() }),
    _ => Syntax::Es(EsSyntax { jsx: true, ..Default::default() }),
  };
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Anon), source.to_owned());
  let lexer = Lexer::new(syntax, EsVersion::latest(), StringInput::from(&*fm), None);
  Parser::new_from(lexer).parse_program().ok()
}

/// A string literal's text, `None` when it holds a lone surrogate.
fn text(value: &swc_core::atoms::Wtf8Atom) -> Option<&str> {
  value.as_wtf8().as_str()
}

/// What each `const` holds read as classes, by name: as a whole, as a lookup table indexed by
/// any key and field by field. Read in declaration order, so a constant naming an earlier one
/// reads what that one holds.
#[derive(Default)]
struct ConstTable {
  whole: std::collections::HashMap<String, Vec<String>>,
  lookup: std::collections::HashMap<String, Vec<String>>,
  fields: std::collections::HashMap<(String, String), Vec<String>>,
}

#[derive(Default)]
struct Consts {
  table: ConstTable,
}

/// A property key's name when it is written out: an identifier, a string or a number.
fn static_name(key: &swc_core::ecma::ast::PropName) -> Option<String> {
  use swc_core::ecma::ast::PropName;
  match key {
    PropName::Ident(id) => Some(id.sym.to_string()),
    PropName::Str(s) => text(&s.value).map(str::to_owned),
    PropName::Num(n) => Some(n.value.to_string()),
    _ => None,
  }
}

/// The values of an object literal's `key: value` properties.
fn values(object: &swc_core::ecma::ast::ObjectLit) -> impl Iterator<Item = &swc_core::ecma::ast::Expr> {
  use swc_core::ecma::ast::{Prop, PropOrSpread};
  object.props.iter().filter_map(|property| match property {
    PropOrSpread::Prop(prop) => match &**prop {
      Prop::KeyValue(kv) => Some(&*kv.value),
      _ => None,
    },
    _ => None,
  })
}

/// The keys of an object literal's own properties, shorthand and methods included.
fn property_keys(object: &swc_core::ecma::ast::ObjectLit) -> Vec<String> {
  use swc_core::ecma::ast::{Prop, PropOrSpread};
  let mut out = Vec::new();
  for property in &object.props {
    let PropOrSpread::Prop(prop) = property else { continue };
    let key = match &**prop {
      Prop::KeyValue(kv) => static_name(&kv.key),
      Prop::Shorthand(id) => Some(id.sym.to_string()),
      Prop::Method(m) => static_name(&m.key),
      Prop::Getter(g) => static_name(&g.key),
      Prop::Setter(s) => static_name(&s.key),
      _ => None,
    };
    out.extend(key);
  }
  out
}

fn without_parentheses(expr: &swc_core::ecma::ast::Expr) -> &swc_core::ecma::ast::Expr {
  match expr {
    swc_core::ecma::ast::Expr::Paren(p) => without_parentheses(&p.expr),
    other => other,
  }
}

impl swc_core::ecma::visit::Visit for Consts {
  fn visit_var_decl(&mut self, it: &swc_core::ecma::ast::VarDecl) {
    use swc_core::ecma::ast::{Expr, Pat, VarDeclKind};
    if it.kind == VarDeclKind::Const {
      for declarator in &it.decls {
        let (Pat::Ident(id), Some(init)) = (&declarator.name, &declarator.init) else { continue };
        let read = |f: &mut dyn FnMut(&mut Classes)| {
          let mut sub = Classes { consts: Some(&self.table), ..Classes::default() };
          f(&mut sub);
          sub.found.into_iter().collect::<Vec<_>>()
        };
        let whole = read(&mut |c| c.expression(init));
        let lookup = read(&mut |c| c.lookup(init));
        let mut fields = Vec::new();
        if let Expr::Object(o) = without_parentheses(init) {
          for property in &o.props {
            let swc_core::ecma::ast::PropOrSpread::Prop(prop) = property else { continue };
            let (key, value): (Option<String>, std::borrow::Cow<'_, Expr>) = match &**prop {
              swc_core::ecma::ast::Prop::KeyValue(kv) => (static_name(&kv.key), std::borrow::Cow::Borrowed(&*kv.value)),
              swc_core::ecma::ast::Prop::Shorthand(name) => (Some(name.sym.to_string()), std::borrow::Cow::Owned(Expr::Ident(name.clone()))),
              _ => continue,
            };
            if let Some(key) = key {
              fields.push((key, read(&mut |c| c.expression(&value))));
            }
          }
        }
        let name = id.id.sym.to_string();
        self.table.whole.insert(name.clone(), whole);
        self.table.lookup.insert(name.clone(), lookup);
        for (key, classes) in fields {
          self.table.fields.insert((name.clone(), key), classes);
        }
      }
    }
    it.visit_children_with(self);
  }
}

#[derive(Default)]
struct Classes<'t> {
  found: std::collections::BTreeSet<String>,
  consts: Option<&'t ConstTable>,
  /// Local names an import binds a class helper to, `e` in `import { cn as e } from "./cn.js"`.
  aliases: std::collections::HashSet<String>,
  /// Local names Svelte's client runtime is imported under, `$` in compiled output.
  svelte: std::collections::HashSet<String>,
}

/// The import declarations of a module program.
fn imports(program: &swc_core::ecma::ast::Program) -> impl Iterator<Item = &swc_core::ecma::ast::ImportDecl> {
  use swc_core::ecma::ast::{ModuleDecl, ModuleItem, Program};
  let body: &[ModuleItem] = match program {
    Program::Module(m) => &m.body,
    _ => &[],
  };
  body.iter().filter_map(|item| match item {
    ModuleItem::ModuleDecl(ModuleDecl::Import(import)) => Some(import),
    _ => None,
  })
}

/// The names a program imports `svelte/internal/client` under as a namespace.
fn svelte_runtimes(program: &swc_core::ecma::ast::Program) -> std::collections::HashSet<String> {
  use swc_core::ecma::ast::ImportSpecifier;
  let mut out = std::collections::HashSet::new();
  for import in imports(program) {
    if text(&import.src.value) != Some("svelte/internal/client") {
      continue;
    }
    for specifier in &import.specifiers {
      if let ImportSpecifier::Namespace(s) = specifier {
        out.insert(s.local.sym.to_string());
      }
    }
  }
  out
}

/// The packages whose default export joins classes.
const CLASS_PACKAGES: &[&str] = &["clsx", "clsx/lite", "classnames", "tailwind-merge"];

/// The local names a program's imports bind a class helper to: a named import of one of
/// [`CLASS_CALLS`] under any local name or a default import from one of [`CLASS_PACKAGES`].
fn helper_aliases(program: &swc_core::ecma::ast::Program) -> std::collections::HashSet<String> {
  use swc_core::ecma::ast::{ImportSpecifier, ModuleExportName};
  let mut out = std::collections::HashSet::new();
  for import in imports(program) {
    for specifier in &import.specifiers {
      match specifier {
        ImportSpecifier::Named(s) => {
          let imported = match &s.imported {
            Some(ModuleExportName::Ident(n)) => n.sym.to_string(),
            Some(ModuleExportName::Str(n)) => text(&n.value).unwrap_or_default().to_owned(),
            None => s.local.sym.to_string(),
          };
          if CLASS_CALLS.contains(&imported.as_str()) {
            out.insert(s.local.sym.to_string());
          }
        }
        ImportSpecifier::Default(s) if CLASS_PACKAGES.contains(&text(&import.src.value).unwrap_or_default()) => {
          out.insert(s.local.sym.to_string());
        }
        _ => {}
      }
    }
  }
  out
}

const CLASS_CALLS: &[&str] = &["clsx", "cn", "cx", "classnames", "classNames", "twMerge", "twJoin", "normalizeClass", "_normalizeClass"];

/// A call's arguments that are expressions, spreads left out.
fn argument(call: &swc_core::ecma::ast::CallExpr, i: usize) -> Option<&swc_core::ecma::ast::Expr> {
  call.args.get(i).filter(|a| a.spread.is_none()).map(|a| &*a.expr)
}

impl Classes<'_> {
  fn words(&mut self, text: &str) {
    self.found.extend(text.split_whitespace().map(str::to_owned));
  }

  fn from_table(&mut self, pick: impl FnOnce(&ConstTable) -> Option<&Vec<String>>) {
    if let Some(classes) = self.consts.and_then(pick) {
      self.found.extend(classes.iter().cloned());
    }
  }

  /// The values of a lookup object or the elements of a lookup array: what `table[key]` can be.
  fn lookup(&mut self, table: &swc_core::ecma::ast::Expr) {
    use swc_core::ecma::ast::Expr;
    match table {
      Expr::Object(o) => {
        for value in values(o) {
          self.expression(value);
        }
        for property in &o.props {
          if let swc_core::ecma::ast::PropOrSpread::Prop(prop) = property
            && let swc_core::ecma::ast::Prop::Shorthand(name) = &**prop
          {
            self.expression(&Expr::Ident(name.clone()));
          }
        }
      }
      Expr::Array(_) => self.expression(table),
      Expr::TsAs(e) => self.lookup(&e.expr),
      Expr::TsConstAssertion(e) => self.lookup(&e.expr),
      Expr::TsSatisfies(e) => self.lookup(&e.expr),
      _ => {}
    }
  }

  fn expression(&mut self, expr: &swc_core::ecma::ast::Expr) {
    use swc_core::ecma::ast::{BinaryOp, Expr, Lit, MemberProp};
    match expr {
      Expr::Ident(id) => self.from_table(|t| t.whole.get(id.sym.as_str())),
      Expr::Member(m) => {
        if let Expr::Ident(id) = &*m.obj {
          match &m.prop {
            MemberProp::Computed(_) => self.from_table(|t| t.lookup.get(id.sym.as_str())),
            MemberProp::Ident(property) => {
              let key = (id.sym.to_string(), property.sym.to_string());
              self.from_table(|t| t.fields.get(&key));
            }
            _ => {}
          }
        }
      }
      Expr::Lit(Lit::Str(s)) => self.words(text(&s.value).unwrap_or_default()),
      Expr::Tpl(t) => {
        let last = t.quasis.len().saturating_sub(1);
        for (i, quasi) in t.quasis.iter().enumerate() {
          let text = quasi.cooked.as_ref().and_then(text).unwrap_or(quasi.raw.as_str());
          let mut tokens: Vec<&str> = text.split_whitespace().collect();
          if i > 0 && !text.starts_with(char::is_whitespace) && !tokens.is_empty() {
            tokens.remove(0);
          }
          if i < last && !text.ends_with(char::is_whitespace) && !tokens.is_empty() {
            tokens.pop();
          }
          self.found.extend(tokens.into_iter().map(str::to_owned));
        }
        for inner in &t.exprs {
          self.expression(inner);
        }
      }
      Expr::Cond(c) => {
        self.expression(&c.cons);
        self.expression(&c.alt);
      }
      Expr::Bin(b) if matches!(b.op, BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing) => {
        if b.op != BinaryOp::LogicalAnd {
          self.expression(&b.left);
        }
        self.expression(&b.right);
      }
      Expr::Array(a) => {
        for element in a.elems.iter().flatten() {
          if element.spread.is_none() {
            self.expression(&element.expr);
          }
        }
      }
      Expr::Object(o) => {
        for key in property_keys(o) {
          self.words(&key);
        }
      }
      Expr::Paren(p) => self.expression(&p.expr),
      Expr::TsAs(e) => self.expression(&e.expr),
      Expr::TsConstAssertion(e) => self.expression(&e.expr),
      Expr::TsSatisfies(e) => self.expression(&e.expr),
      Expr::Call(call) => self.call(call),
      _ => {}
    }
  }

  fn call(&mut self, call: &swc_core::ecma::ast::CallExpr) {
    use swc_core::ecma::ast::{Callee, Expr, MemberProp};
    let Callee::Expr(callee) = &call.callee else { return };
    if let Expr::Member(m) = &**callee
      && let Expr::Ident(object) = &*m.obj
      && let MemberProp::Ident(property) = &m.prop
      && self.svelte.contains(object.sym.as_str())
    {
      self.svelte_call(property.sym.as_str(), call);
    }
    let name = match &**callee {
      Expr::Ident(id) => Some(id.sym.as_str()),
      Expr::Member(m) => match &m.prop {
        MemberProp::Ident(property) => Some(property.sym.as_str()),
        _ => None,
      },
      _ => None,
    };
    if name.is_some_and(|n| CLASS_CALLS.contains(&n) || self.aliases.contains(n)) {
      for argument in &call.args {
        if argument.spread.is_none() {
          self.expression(&argument.expr);
        }
      }
    }
  }
}

impl Classes<'_> {
  /// Svelte's compiled output: `set_class(element, flags, value, hash, previous, directives)`
  /// sets `value` and toggles each key of `directives`, and `from_html` and its siblings hold
  /// the template's static markup, whose `class` attributes are classes.
  fn svelte_call(&mut self, name: &str, call: &swc_core::ecma::ast::CallExpr) {
    use swc_core::ecma::ast::{Expr, Lit};
    match name {
      "set_class" => {
        for i in [2, 5] {
          if let Some(e) = argument(call, i) {
            self.expression(e);
          }
        }
      }
      "from_html" | "from_svg" | "from_mathml" => {
        let markup = match argument(call, 0) {
          Some(Expr::Lit(Lit::Str(s))) => text(&s.value).map(str::to_owned),
          Some(Expr::Tpl(t)) if t.exprs.is_empty() => t.quasis.first().map(|q| q.cooked.as_ref().and_then(text).unwrap_or(q.raw.as_str()).to_owned()),
          _ => None,
        };
        let Some(markup) = markup else { return };
        let mut rest = markup.as_str();
        while let Some(at) = rest.find("class=\"") {
          let attribute = rest[..at].ends_with(char::is_whitespace);
          let value = &rest[at + 7..];
          let end = value.find('"').unwrap_or(value.len());
          if attribute {
            self.words(&unescape(&value[..end]));
          }
          rest = &value[end..];
        }
      }
      _ => {}
    }
  }
}

/// An attribute value as the browser reads it. Svelte's compiled markup writes
/// `&` as `&amp;` and `"` as `&quot;`; a numeric reference stands for its character.
fn unescape(value: &str) -> std::borrow::Cow<'_, str> {
  if !value.contains('&') {
    return std::borrow::Cow::Borrowed(value);
  }
  let mut out = String::with_capacity(value.len());
  let mut rest = value;
  while let Some(at) = rest.find('&') {
    out.push_str(&rest[..at]);
    rest = &rest[at..];
    let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
      let name = &rest[1..end];
      let ch = match name {
        "amp" => Some('&'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "lt" => Some('<'),
        "gt" => Some('>'),
        _ => match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
          Some(hex) => u32::from_str_radix(hex, 16).ok().and_then(char::from_u32),
          None => name.strip_prefix('#').and_then(|dec| dec.parse::<u32>().ok()).and_then(char::from_u32),
        },
      };
      ch.map(|ch| (ch, end))
    });
    match decoded {
      Some((ch, end)) => {
        out.push(ch);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  std::borrow::Cow::Owned(out)
}

impl swc_core::ecma::visit::Visit for Classes<'_> {
  fn visit_jsx_attr(&mut self, attribute: &swc_core::ecma::ast::JSXAttr) {
    use swc_core::ecma::ast::{JSXAttrName, JSXAttrValue, JSXExpr};
    let named = matches!(&attribute.name, JSXAttrName::Ident(id) if id.sym == "class" || id.sym == "className");
    if named {
      match &attribute.value {
        Some(JSXAttrValue::Str(s)) => self.words(text(&s.value).unwrap_or_default()),
        Some(JSXAttrValue::JSXExprContainer(c)) => {
          if let JSXExpr::Expr(e) = &c.expr {
            self.expression(e);
          }
        }
        _ => {}
      }
    }
    attribute.visit_children_with(self);
  }

  fn visit_prop(&mut self, property: &swc_core::ecma::ast::Prop) {
    use swc_core::ecma::ast::{Expr, Prop, PropName};
    let is_class = |name: &str| name == "class" || name == "className";
    match property {
      Prop::KeyValue(kv) => {
        let named = match &kv.key {
          PropName::Ident(id) => is_class(&id.sym),
          PropName::Str(s) => text(&s.value).is_some_and(is_class),
          _ => false,
        };
        if named {
          self.expression(&kv.value);
        }
      }
      Prop::Shorthand(id) if is_class(&id.sym) => self.expression(&Expr::Ident(id.clone())),
      _ => {}
    }
    property.visit_children_with(self);
  }

  fn visit_call_expr(&mut self, call: &swc_core::ecma::ast::CallExpr) {
    self.call(call);
    call.visit_children_with(self);
  }
}

/// The specifiers a script imports: static imports and re-exports, side-effect imports and
/// `import("x")` with a literal argument.
pub fn module_imports(path: &Path, source: &str) -> Vec<String> {
  let Some(program) = parse(path, source, false) else { return Vec::new() };
  let mut collector = Imports::default();
  program.visit_with(&mut collector);
  collector.found.into_iter().collect()
}

#[derive(Default)]
struct Imports {
  found: std::collections::BTreeSet<String>,
}

impl Imports {
  fn add(&mut self, source: &swc_core::ecma::ast::Str) {
    self.found.extend(text(&source.value).map(str::to_owned));
  }
}

impl swc_core::ecma::visit::Visit for Imports {
  fn visit_import_decl(&mut self, it: &swc_core::ecma::ast::ImportDecl) {
    self.add(&it.src);
  }

  fn visit_export_all(&mut self, it: &swc_core::ecma::ast::ExportAll) {
    self.add(&it.src);
  }

  fn visit_named_export(&mut self, it: &swc_core::ecma::ast::NamedExport) {
    if let Some(source) = &it.src {
      self.add(source);
    }
  }

  fn visit_call_expr(&mut self, it: &swc_core::ecma::ast::CallExpr) {
    use swc_core::ecma::ast::{Callee, Expr, Lit};
    if matches!(it.callee, Callee::Import(_))
      && let Some(Expr::Lit(Lit::Str(s))) = argument(it, 0)
    {
      self.add(s);
    }
    it.visit_children_with(self);
  }
}

/// The file a package's build writes its class list to, beside its modules.
pub const PACKAGE_CLASSES: &str = "tailwind.classes";

/// A vendor tree: the directory the vendored modules sit in and the URL prefix the import map
/// names them under.
#[non_exhaustive]
pub struct Vendor<'a> {
  pub dir: &'a Path,
  pub base: &'a str,
  pub map: &'a crate::importmap::ImportMap,
}

impl Vendor<'_> {
  fn file_of(&self, url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix(self.base.trim_end_matches('/'))?.strip_prefix('/')?;
    Some(crate::graph::normalise(&self.dir.join(rest)))
  }

  /// The directory holding a package's class list at or above `file`, inside the tree.
  fn listing_package(&self, file: &Path) -> Option<PathBuf> {
    let mut dir = file.parent()?;
    while dir.starts_with(self.dir) && dir != self.dir {
      if dir.join(PACKAGE_CLASSES).is_file() {
        return Some(dir.to_path_buf());
      }
      dir = dir.parent()?;
    }
    None
  }

  /// The classes of every vendored module `specifiers` reach, each with the vendored path it
  /// came from. A package that ships a class list is read from the list and its modules are
  /// not parsed; any other module is read for classes where a class goes and its imports are
  /// followed, bare ones through the import map.
  pub fn classes(&self, specifiers: &[String]) -> Vec<(String, String)> {
    let mut queue: Vec<PathBuf> = specifiers.iter().filter_map(|s| self.map.url_of(s)).filter_map(|url| self.file_of(&url)).collect();
    let mut seen_files = std::collections::BTreeSet::new();
    let mut seen_packages = std::collections::BTreeSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    let label = |path: &Path| path.strip_prefix(self.dir.parent().unwrap_or(self.dir)).unwrap_or(path).to_string_lossy().replace('\\', "/");
    while let Some(file) = queue.pop() {
      if !seen_files.insert(file.clone()) {
        continue;
      }
      if let Some(package) = self.listing_package(&file) {
        if seen_packages.insert(package.clone()) {
          let listed = package.join(PACKAGE_CLASSES);
          out.extend(read_classes(&listed).into_iter().map(|(class, _)| (class, label(&listed))));
        }
        continue;
      }
      let Ok(text) = std::fs::read_to_string(&file) else { continue };
      out.extend(source_classes(&file, &text).into_iter().map(|class| (class, label(&file))));
      for specifier in module_imports(&file, &text) {
        let next = if specifier.starts_with("./") || specifier.starts_with("../") {
          file.parent().map(|dir| crate::graph::normalise(&dir.join(&specifier)))
        } else if specifier.starts_with('/') {
          self.file_of(&specifier)
        } else {
          self.map.url_of(&specifier).and_then(|url| self.file_of(&url))
        };
        queue.extend(next);
      }
    }
    out
  }
}

#[cfg(test)]
mod svelte_properties;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn reads_classes_where_a_class_goes() {
    let tsx = "import { clsx } from \"clsx\";\nexport function Clock({ on, tone }: { on: boolean; tone: string }) {\n  return <p className={clsx(\"font-mono text-sm\", on && \"text-emerald-600\", { \"animate-pulse\": on })}><span className={`rounded px-2 bg-${tone}-100 ${on ? \"ring-2\" : \"ring-0\"}`}>x</span><b class=\"font-bold\">y</b></p>;\n}\n";
    let found = source_classes(Path::new("src/ui/Clock.tsx"), tsx);
    assert_eq!(found, ["animate-pulse", "font-bold", "font-mono", "px-2", "ring-0", "ring-2", "rounded", "text-emerald-600", "text-sm"]);
    let vue = "import { normalizeClass as _normalizeClass, createElementVNode as _createElementVNode } from \"vue\";\nexport function render(_ctx) {\n  return _createElementVNode(\"div\", { class: _normalizeClass([\"card\", { \"card-on\": _ctx.on }]) }, [_createElementVNode(\"p\", { class: \"p-4 text-lg\" })]);\n}\n";
    assert_eq!(source_classes(Path::new("src/Box.vue.js"), vue), ["card", "card-on", "p-4", "text-lg"]);
  }

  #[test]
  fn reads_the_classes_svelte_compiles_into_its_template_and_its_class_calls() {
    let svelte = "import * as $ from 'svelte/internal/client';\nvar root = $.from_html(`<div data-class=\"not-read\" class=\"p-4 text-lg\"><p>a</p> <p>b</p> <p>c</p> <p>d</p></div>`);\nexport default function Tw($$anchor, $$props) {\n  var div = root();\n  var p = $.child(div);\n  let classes;\n  $.template_effect(() => {\n    $.set_class(p, 1, $.clsx($$props.on ? \"font-bold\" : \"font-light\"));\n    $.set_class(p, 1, $.clsx([\"rounded\", { \"ring-2\": $$props.on }]));\n    classes = $.set_class(p, 1, 'base-x', null, classes, { 'text-red-500': $$props.on, underline: !$$props.on });\n    $.set_class(p, 1, `m-2 ${$$props.tone}`);\n  });\n}\n";
    assert_eq!(source_classes(Path::new("src/Tw.svelte"), svelte), ["base-x", "font-bold", "font-light", "m-2", "p-4", "ring-2", "rounded", "text-lg", "text-red-500", "underline"]);
    let unrelated = "import { set_class, from_html } from \"./mine.js\";\nset_class(el, 1, \"not-a-class\");\nfrom_html(`<p class=\"nor-this\"></p>`);\n";
    assert!(source_classes(Path::new("src/mine.js"), unrelated).is_empty(), "only Svelte's own runtime is read this way");
  }

  #[test]
  fn recognises_a_tailwind_stylesheet() {
    assert!(uses_tailwind("@import \"tailwindcss\";\n"));
    assert!(uses_tailwind("@import 'tailwindcss/theme.css' layer(theme);"));
    assert!(uses_tailwind(".a {\n  @apply flex;\n}"));
    assert!(uses_tailwind("@theme {\n  --color-brand: red;\n}"));
    assert!(uses_tailwind(".a { margin: --spacing(4); }"));
    assert!(!uses_tailwind(":root {\n  --bg: #fff;\n}\n@media (width > 40rem) {\n  .a { color: red; }\n}"));
    assert!(!uses_tailwind("@import \"./base.css\";\n.themed { color: var(--theme-ink); }"));
  }

  #[test]
  fn reads_the_classes_a_constant_holds_where_a_class_goes() {
    let tsx = "const tones = { violet: \"bg-violet-100 text-violet-800\", teal: \"bg-teal-100\" } as const;\nconst base = \"inline-flex rounded-full\";\nconst sizes = [\"text-xs\", \"text-sm\"];\nconst unused = \"not-a-class-anywhere\";\nexport function Badge({ tone, big }: { tone: keyof typeof tones; big: boolean }) {\n  return <span className={cn(base, tones[tone], sizes[big ? 1 : 0])}><i className={tones.teal} /></span>;\n}\n";
    let found = source_classes(Path::new("src/Badge.tsx"), tsx);
    assert_eq!(found, ["bg-teal-100", "bg-violet-100", "inline-flex", "rounded-full", "text-sm", "text-violet-800", "text-xs"]);
  }

  #[test]
  fn a_constant_that_names_another_reads_what_that_one_holds() {
    let tsx = "const ring = \"ring-1 ring-inset\";\nconst tones = { violet: `${ring} bg-violet-100` };\nexport const B = ({ t }: { t: \"violet\" }) => <b className={tones[t]} />;\n";
    assert_eq!(source_classes(Path::new("src/B.tsx"), tsx), ["bg-violet-100", "ring-1", "ring-inset"]);
  }

  #[test]
  fn a_helper_imported_under_another_name_still_counts() {
    let js = "import{cn as e}from\"./cn.min.js\";import t from\"clsx\";import{twJoin as j}from\"tailwind-merge\";import{other as o}from\"./x.js\";export function B(){return[e(\"bg-violet-100\",\"px-2\"),t(\"ring-1\"),j(\"gap-1\"),o(\"not-a-class\")]}";
    assert_eq!(source_classes(Path::new("vendor/@acme/ui/index.min.js"), js), ["bg-violet-100", "gap-1", "px-2", "ring-1"]);
  }

  #[test]
  fn reads_the_classes_selectors_name() {
    let css = ".a, .b-c:hover > .d_e { margin: .5rem; background: url(x.png); }\n@media (width > 1px) {\n  .f { color: red; }\n}\n/* .g */\n.h { content: \".i\"; &.j { color: red; } }";
    let defined: Vec<&str> = defined_classes(css).into_iter().collect();
    assert_eq!(defined, ["a", "b-c", "d_e", "f", "h", "j"]);
  }

  #[test]
  fn expands_the_listed_classes_and_names_the_unknown_ones() {
    let dir = std::env::temp_dir().join(format!("snapfirec-tailwind-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let classes = dir.join("classes");
    std::fs::write(&classes, "flex\troutes/page.tsx#default\nbg-brand\troutes/page.tsx#default\nflx\troutes/layout.tsx#default\nhero\troutes/page.tsx#default\n").unwrap();
    std::fs::write(dir.join("brand.css"), "@theme {\n  --color-brand: #123456;\n}\n").unwrap();
    let sheet = dir.join("app.css");
    let content = "@import \"tailwindcss/utilities\";\n@import \"./brand.css\";\n.hero {\n  margin: 0 .5rem;\n}\n";

    let expanded = expand(&sheet, content, Some(&classes), &[], &[], "styles/app.css");
    let css = expanded.css.unwrap();
    assert!(css.contains(".flex {\n  display: flex;\n}"), "{css}");
    assert!(css.contains("background-color: var(--color-brand)"), "{css}");
    assert!(expanded.warnings.is_empty(), "{:?}", expanded.warnings);
    assert_eq!(expanded.unknown, [("flx".to_owned(), "routes/layout.tsx#default".to_owned())]);
    assert_eq!(expanded.reads, [classes, dir.join("brand.css")]);
    std::fs::remove_dir_all(&dir).unwrap();
  }

  #[test]
  fn refuses_a_stylesheet_that_names_files_for_classes() {
    let sheet = Path::new("styles/app.css");
    for (content, line) in [
      ("@import \"tailwindcss\";\n@source \"../node_modules/acme-ui\";\n", "`@source \"../node_modules/acme-ui\"`"),
      ("@import \"tailwindcss\";\n@source not \"./legacy\";\n", "`@source not \"./legacy\"`"),
      ("@import \"tailwindcss\" source(\"../src\");\n", "`source(\"../src\")`"),
    ] {
      let error = expand(sheet, content, None, &[], &[], "styles/app.css").css.unwrap_err();
      assert!(error.starts_with(&format!("{line}: ")) && error.contains("@source inline("), "{error}");
    }
    for content in ["@import \"tailwindcss\" source(none);\n", "@import \"tailwindcss\";\n@source inline(\"flex\");\n"] {
      let css = expand(sheet, content, None, &[], &[], "styles/app.css").css.unwrap();
      assert!(!css.is_empty(), "{content}");
    }
  }
}
