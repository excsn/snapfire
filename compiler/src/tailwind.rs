//! Stylesheets that use Tailwind. Such a sheet is run through `snapfire_tailwind` with the
//! classes the driver listed before LightningCSS sees it, so the utilities it asks for are in the
//! output like any rule written by hand.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use snapfire_tailwind::{CompileOptions, Compiler, Error, Loaded, NamedTheme, Root, tailwind_stylesheet};

/// What expanding one stylesheet produced. `reads` holds every file it read besides itself,
/// whether or not it succeeded, so a change to any of them compiles it again.
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

/// The name the root theme also answers to when named themes are given, so an element inside
/// one can take the root's values back.
pub const ROOT_THEME: &str = "default";

/// The named themes in `dir`, one per `<name>.css`, selected by `[<attribute>="<name>"]`, in
/// name order.
pub fn named_themes(dir: &Path, attribute: &str) -> Vec<(PathBuf, NamedTheme)> {
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "css")).collect();
  files.sort();
  files
    .into_iter()
    .filter_map(|file| {
      let name = file.file_stem()?.to_string_lossy().into_owned();
      let css = std::fs::read_to_string(&file).ok()?;
      let from = Some(file.to_string_lossy().into_owned());
      Some((file, NamedTheme { selector: format!("[{attribute}=\"{name}\"]"), css, from }))
    })
    .collect()
}

/// Runs `content`, the stylesheet at `path`, through Tailwind with the classes in `classes` and
/// `extra` and the named themes in `themes`, a directory and the attribute that selects one.
/// `quiet` classes build their utilities too but are never reported as unknown: they come from
/// vendored code, where a string that is not a utility is not the app's typo.
/// An import resolves to one of Tailwind's own stylesheets first, then to a file relative to
/// the stylesheet that names it.
pub fn expand(path: &Path, content: &str, classes: Option<&Path>, extra: &[(String, String)], quiet: &[(String, String)], themes: Option<(PathBuf, &str)>, source_name: &str) -> Expanded {
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
  let mut named = Vec::new();
  let mut root_selectors = Vec::new();
  if let Some((themes_dir, attribute)) = themes {
    root_selectors.push(format!("[{attribute}=\"{ROOT_THEME}\"]"));
    reads.borrow_mut().push(themes_dir.clone());
    for (file, theme) in named_themes(&themes_dir, attribute) {
      reads.borrow_mut().push(file);
      named.push(theme);
    }
  }

  let load = |id: &str, base: &str| -> Result<Loaded, Error> {
    if let Some(loaded) = tailwind_stylesheet(id, base) {
      return Ok(loaded);
    }
    let file = crate::graph::normalise(&Path::new(base).join(id));
    reads.borrow_mut().push(file.clone());
    let content = std::fs::read_to_string(&file).map_err(|e| Error::Message(format!("Could not read `{id}` from {base}: {e}")))?;
    Ok(Loaded { path: file.to_string_lossy().into_owned(), base: file.parent().unwrap_or(Path::new(".")).to_string_lossy().into_owned(), content })
  };

  let options = CompileOptions { base: dir, load_stylesheet: Some(Box::new(load)), from: Some(source_name.to_owned()), themes: named, root_selectors, ..CompileOptions::default() };
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
/// property or an argument to `clsx`, `cn`, `cx`, `classnames`, `twMerge` or Vue's
/// `normalizeClass`. Whole tokens only: a token a template expression touches is left out.
pub fn source_classes(path: &Path, source: &str) -> Vec<String> {
  use oxc_ast_visit::Visit;
  let allocator = oxc_allocator::Allocator::default();
  let source_type = oxc_span::SourceType::from_path(path).unwrap_or_else(|_| oxc_span::SourceType::tsx());
  let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
  let mut consts = Consts::default();
  consts.visit_program(&parsed.program);
  let mut collector = Classes { consts: Some(&consts.table), ..Classes::default() };
  collector.visit_program(&parsed.program);
  let mut found: Vec<String> = collector.found.into_iter().collect();
  found.sort();
  found
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

impl<'a> oxc_ast_visit::Visit<'a> for Consts {
  fn visit_variable_declaration(&mut self, it: &oxc_ast::ast::VariableDeclaration<'a>) {
    if it.kind == oxc_ast::ast::VariableDeclarationKind::Const {
      for declarator in &it.declarations {
        let (oxc_ast::ast::BindingPattern::BindingIdentifier(id), Some(init)) = (&declarator.id, &declarator.init) else { continue };
        let read = |f: &mut dyn FnMut(&mut Classes)| {
          let mut sub = Classes { consts: Some(&self.table), ..Classes::default() };
          f(&mut sub);
          sub.found.into_iter().collect::<Vec<_>>()
        };
        let whole = read(&mut |c| c.expression(init));
        let lookup = read(&mut |c| c.lookup(init));
        let mut fields = Vec::new();
        if let oxc_ast::ast::Expression::ObjectExpression(o) = init.without_parentheses() {
          for property in &o.properties {
            if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = property
              && let Some(key) = p.key.static_name()
            {
              fields.push((key.to_string(), read(&mut |c| c.expression(&p.value))));
            }
          }
        }
        let name = id.name.to_string();
        self.table.whole.insert(name.clone(), whole);
        self.table.lookup.insert(name.clone(), lookup);
        for (key, classes) in fields {
          self.table.fields.insert((name.clone(), key), classes);
        }
      }
    }
    oxc_ast_visit::walk::walk_variable_declaration(self, it);
  }
}

#[derive(Default)]
struct Classes<'t> {
  found: std::collections::BTreeSet<String>,
  consts: Option<&'t ConstTable>,
}

const CLASS_CALLS: &[&str] = &["clsx", "cn", "cx", "classnames", "classNames", "twMerge", "normalizeClass", "_normalizeClass"];

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
  fn lookup(&mut self, table: &oxc_ast::ast::Expression) {
    use oxc_ast::ast::Expression;
    match table {
      Expression::ObjectExpression(o) => {
        for property in &o.properties {
          if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = property {
            self.expression(&p.value);
          }
        }
      }
      Expression::ArrayExpression(_) => self.expression(table),
      Expression::TSAsExpression(e) => self.lookup(&e.expression),
      Expression::TSSatisfiesExpression(e) => self.lookup(&e.expression),
      _ => {}
    }
  }

  fn expression(&mut self, expr: &oxc_ast::ast::Expression) {
    use oxc_ast::ast::Expression;
    match expr {
      Expression::Identifier(id) => self.from_table(|t| t.whole.get(id.name.as_str())),
      Expression::ComputedMemberExpression(m) => {
        if let Expression::Identifier(id) = &m.object {
          self.from_table(|t| t.lookup.get(id.name.as_str()));
        }
      }
      Expression::StaticMemberExpression(m) => {
        if let Expression::Identifier(id) = &m.object {
          let key = (id.name.to_string(), m.property.name.to_string());
          self.from_table(|t| t.fields.get(&key));
        }
      }
      Expression::StringLiteral(s) => self.words(&s.value),
      Expression::TemplateLiteral(t) => {
        let last = t.quasis.len().saturating_sub(1);
        for (i, quasi) in t.quasis.iter().enumerate() {
          let text = quasi.value.cooked.as_ref().map(|c| c.as_str()).unwrap_or(quasi.value.raw.as_str());
          let mut tokens: Vec<&str> = text.split_whitespace().collect();
          if i > 0 && !text.starts_with(char::is_whitespace) && !tokens.is_empty() {
            tokens.remove(0);
          }
          if i < last && !text.ends_with(char::is_whitespace) && !tokens.is_empty() {
            tokens.pop();
          }
          self.found.extend(tokens.into_iter().map(str::to_owned));
        }
        for inner in &t.expressions {
          self.expression(inner);
        }
      }
      Expression::ConditionalExpression(c) => {
        self.expression(&c.consequent);
        self.expression(&c.alternate);
      }
      Expression::LogicalExpression(l) => {
        if l.operator != oxc_ast::ast::LogicalOperator::And {
          self.expression(&l.left);
        }
        self.expression(&l.right);
      }
      Expression::ArrayExpression(a) => {
        for element in &a.elements {
          if let Some(e) = element.as_expression() {
            self.expression(e);
          }
        }
      }
      Expression::ObjectExpression(o) => {
        for property in &o.properties {
          if let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(p) = property {
            match &p.key {
              oxc_ast::ast::PropertyKey::StringLiteral(s) => self.words(&s.value),
              oxc_ast::ast::PropertyKey::StaticIdentifier(id) => self.words(&id.name),
              _ => {}
            }
          }
        }
      }
      Expression::ParenthesizedExpression(p) => self.expression(&p.expression),
      Expression::TSAsExpression(e) => self.expression(&e.expression),
      Expression::TSSatisfiesExpression(e) => self.expression(&e.expression),
      Expression::CallExpression(call) => self.call(call),
      _ => {}
    }
  }

  fn call(&mut self, call: &oxc_ast::ast::CallExpression) {
    let name = match &call.callee {
      oxc_ast::ast::Expression::Identifier(id) => Some(id.name.as_str()),
      oxc_ast::ast::Expression::StaticMemberExpression(m) => Some(m.property.name.as_str()),
      _ => None,
    };
    if name.is_some_and(|n| CLASS_CALLS.contains(&n)) {
      for argument in &call.arguments {
        if let Some(e) = argument.as_expression() {
          self.expression(e);
        }
      }
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for Classes<'_> {
  fn visit_jsx_attribute(&mut self, attribute: &oxc_ast::ast::JSXAttribute<'a>) {
    let named = matches!(&attribute.name, oxc_ast::ast::JSXAttributeName::Identifier(id) if id.name == "class" || id.name == "className");
    if named {
      match &attribute.value {
        Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(s)) => self.words(&s.value),
        Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(c)) => {
          if let Some(e) = c.expression.as_expression() {
            self.expression(e);
          }
        }
        _ => {}
      }
    }
    oxc_ast_visit::walk::walk_jsx_attribute(self, attribute);
  }

  fn visit_object_property(&mut self, property: &oxc_ast::ast::ObjectProperty<'a>) {
    let named = match &property.key {
      oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name == "class" || id.name == "className",
      oxc_ast::ast::PropertyKey::StringLiteral(s) => s.value == "class" || s.value == "className",
      _ => false,
    };
    if named {
      self.expression(&property.value);
    }
    oxc_ast_visit::walk::walk_object_property(self, property);
  }

  fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
    self.call(call);
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

/// The specifiers a script imports: static imports and re-exports, side-effect imports and
/// `import("x")` with a literal argument.
pub fn module_imports(path: &Path, source: &str) -> Vec<String> {
  use oxc_ast_visit::Visit;
  let allocator = oxc_allocator::Allocator::default();
  let source_type = oxc_span::SourceType::from_path(path).unwrap_or_else(|_| oxc_span::SourceType::mjs());
  let parsed = oxc_parser::Parser::new(&allocator, source, source_type).parse();
  let mut collector = Imports::default();
  collector.visit_program(&parsed.program);
  collector.found.into_iter().collect()
}

#[derive(Default)]
struct Imports {
  found: std::collections::BTreeSet<String>,
}

impl<'a> oxc_ast_visit::Visit<'a> for Imports {
  fn visit_import_declaration(&mut self, it: &oxc_ast::ast::ImportDeclaration<'a>) {
    self.found.insert(it.source.value.to_string());
  }

  fn visit_export_all_declaration(&mut self, it: &oxc_ast::ast::ExportAllDeclaration<'a>) {
    self.found.insert(it.source.value.to_string());
  }

  fn visit_export_from_declaration(&mut self, it: &oxc_ast::ast::ExportFromDeclaration<'a>) {
    self.found.insert(it.source.value.to_string());
  }

  fn visit_import_expression(&mut self, it: &oxc_ast::ast::ImportExpression<'a>) {
    if let oxc_ast::ast::Expression::StringLiteral(s) = &it.source {
      self.found.insert(s.value.to_string());
    }
    oxc_ast_visit::walk::walk_import_expression(self, it);
  }
}

/// The file a package's build writes its class list to, beside its modules.
pub const PACKAGE_CLASSES: &str = "tailwind.classes";

/// A vendor tree: the directory the vendored modules sit in and the URL prefix the import map
/// names them under.
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

    let expanded = expand(&sheet, content, Some(&classes), &[], &[], None, "styles/app.css");
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
      let error = expand(sheet, content, None, &[], &[], None, "styles/app.css").css.unwrap_err();
      assert!(error.starts_with(&format!("{line}: ")) && error.contains("@source inline("), "{error}");
    }
    for content in ["@import \"tailwindcss\" source(none);\n", "@import \"tailwindcss\";\n@source inline(\"flex\");\n"] {
      let css = expand(sheet, content, None, &[], &[], None, "styles/app.css").css.unwrap();
      assert!(!css.is_empty(), "{content}");
    }
  }
}
