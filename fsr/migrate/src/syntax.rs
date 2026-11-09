use std::ops::Range;

use oxc_allocator::Allocator;
use oxc_ast::ast::{self as js, Argument, Expression};
use oxc_ast_visit::{walk, Visit};
use oxc_parser::Parser;
use oxc_span::{SourceType, Span};

pub(crate) const SCRIPT_EXTENSIONS: &[&str] = &[".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".mts", ".cts"];

pub(crate) fn is_script(path: &str) -> bool {
  SCRIPT_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) && !path.ends_with(".d.ts")
}

pub(crate) struct Parsed<'a> {
  pub program: js::Program<'a>,
  /// The arena the program lives in, where a node a pass keeps is cloned.
  pub allocator: &'a Allocator,
  line_starts: Vec<usize>,
}

impl<'a> Parsed<'a> {
  /// The byte range of `span` in the source.
  pub fn range(&self, span: Span) -> Range<usize> {
    span.start as usize..span.end as usize
  }

  pub fn line(&self, span: Span) -> usize {
    line_of(&self.line_starts, span.start as usize)
  }

  /// The top-level statements.
  pub fn body(&self) -> &oxc_allocator::Vec<'a, js::Statement<'a>> {
    &self.program.body
  }
}

fn line_starts(source: &str) -> Vec<usize> {
  std::iter::once(0).chain(source.match_indices('\n').map(|(i, _)| i + 1)).collect()
}

fn line_of(starts: &[usize], at: usize) -> usize {
  starts.partition_point(|&start| start <= at)
}

/// `source` as a module, copied into `allocator`. A `.ts`, `.mts` or `.cts`
/// file is TypeScript without JSX, since there `<T>x` is a type assertion;
/// every other script parses JSX.
pub(crate) fn parse<'a>(allocator: &'a Allocator, file: &str, source: &str) -> Result<Parsed<'a>, String> {
  let source = allocator.alloc_str(source);
  let ts = [".ts", ".mts", ".cts"].iter().any(|ext| file.ends_with(ext));
  let source_type = if ts { SourceType::ts() } else { SourceType::tsx() };
  let parsed = Parser::new(allocator, source, source_type).parse();
  let starts = line_starts(source);
  if let Some(first) = parsed.diagnostics.first() {
    let at = first.labels.first().map_or(0, |label| label.offset() as usize);
    let line = line_of(&starts, at);
    let column = source[starts[line - 1]..at.min(source.len())].chars().count() + 1;
    return Err(format!("{line}:{column}: {}", first.message));
  }
  Ok(Parsed { program: parsed.program, allocator, line_starts: starts })
}

/// A node a visitor can start a walk from.
pub(crate) trait Node<'a>: oxc_span::GetSpan {
  fn accept<V: Visit<'a>>(&self, visitor: &mut V);
}

impl<'a> Node<'a> for Expression<'a> {
  fn accept<V: Visit<'a>>(&self, visitor: &mut V) {
    visitor.visit_expression(self);
  }
}

impl<'a> Node<'a> for js::Statement<'a> {
  fn accept<V: Visit<'a>>(&self, visitor: &mut V) {
    visitor.visit_statement(self);
  }
}

impl<'a> Node<'a> for js::CallExpression<'a> {
  fn accept<V: Visit<'a>>(&self, visitor: &mut V) {
    visitor.visit_call_expression(self);
  }
}

impl<'a> Node<'a> for js::Program<'a> {
  fn accept<V: Visit<'a>>(&self, visitor: &mut V) {
    visitor.visit_program(self);
  }
}

/// A property written `name: value` with an identifier for its key, as its
/// key and value. A shorthand, a method, a getter or a setter is none.
pub(crate) fn key_value<'b, 'a>(prop: &'b js::ObjectPropertyKind<'a>) -> Option<(&'b str, &'b Expression<'a>)> {
  let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return None };
  if prop.kind != js::PropertyKind::Init || prop.method || prop.shorthand {
    return None;
  }
  match &prop.key {
    js::PropertyKey::StaticIdentifier(key) => Some((key.name.as_str(), &prop.value)),
    _ => None,
  }
}

/// A module export name written as an identifier; none for a string.
pub(crate) fn export_ident<'b>(name: &'b js::ModuleExportName<'_>) -> Option<&'b str> {
  match name {
    js::ModuleExportName::IdentifierName(i) => Some(i.name.as_str()),
    js::ModuleExportName::IdentifierReference(i) => Some(i.name.as_str()),
    js::ModuleExportName::StringLiteral(_) => None,
  }
}

/// An import's specifiers, none for `import "module"`.
pub(crate) fn import_specifiers<'b, 'a>(import: &'b js::ImportDeclaration<'a>) -> &'b [js::ImportDeclarationSpecifier<'a>] {
  match &import.specifiers {
    Some(specifiers) => specifiers.as_slice(),
    None => &[],
  }
}

/// A function parameter in the order it is written: a pattern with nothing
/// else, or one with a default value or a rest, which the passes leave alone.
pub(crate) enum Param<'b, 'a> {
  Plain(&'b js::FormalParameter<'a>),
  Other(Span),
}

impl<'b, 'a> Param<'b, 'a> {
  pub(crate) fn span(&self) -> Span {
    match self {
      Param::Plain(p) => p.span,
      Param::Other(span) => *span,
    }
  }

  /// The pattern of a plain parameter.
  pub(crate) fn plain(&self) -> Option<&'b js::BindingPattern<'a>> {
    match self {
      Param::Plain(p) => Some(&p.pattern),
      Param::Other(_) => None,
    }
  }
}

/// The parameters of a function, its rest last.
pub(crate) fn params<'b, 'a>(params: &'b js::FormalParameters<'a>) -> Vec<Param<'b, 'a>> {
  let written = params.items.iter().map(|p| if p.initializer.is_some() { Param::Other(p.span) } else { Param::Plain(p) });
  written.chain(params.rest.iter().map(|rest| Param::Other(rest.span))).collect()
}

/// The span of an object pattern's last property, its rest included.
pub(crate) fn last_property(pattern: &js::ObjectPattern) -> Option<Span> {
  pattern.rest.as_ref().map(|rest| rest.span).or_else(|| pattern.properties.last().map(|p| p.span))
}

/// The expression an argument passes, a spread's included.
pub(crate) fn argument<'b, 'a>(arg: &'b Argument<'a>) -> &'b Expression<'a> {
  match arg {
    Argument::SpreadElement(spread) => &spread.argument,
    other => other.to_expression(),
  }
}

/// A string literal's value as text, each lone surrogate replaced with U+FFFD.
pub(crate) fn lossy(value: oxc_str::JSStr<'_>) -> String {
  match value.as_str() {
    Some(text) => text.to_owned(),
    None => String::from_utf8_lossy(value.as_bytes()).into_owned(),
  }
}

/// A JSX element's name when it is one identifier, a tag such as `div` or a
/// component such as `Box`, with its span.
pub(crate) fn jsx_ident<'n>(name: &'n js::JSXElementName<'_>) -> Option<(&'n str, Span)> {
  match name {
    js::JSXElementName::Identifier(i) => Some((i.name.as_str(), i.span)),
    js::JSXElementName::IdentifierReference(i) => Some((i.name.as_str(), i.span)),
    _ => None,
  }
}

/// A module specifier written as a string literal, with the byte range of
/// its text between the quotes.
pub(crate) struct Specifier {
  pub value: String,
  pub range: Range<usize>,
  pub line: usize,
}

/// Every specifier `parsed` names: imports, re-exports, `import()` and
/// `require()` of a literal and `import("...")` types.
pub(crate) fn specifiers(parsed: &Parsed) -> Vec<Specifier> {
  let mut collect = Collect { parsed, found: Vec::new() };
  collect.visit_program(&parsed.program);
  collect.found.sort_by_key(|s| s.range.start);
  collect.found
}

struct Collect<'p, 'a> {
  parsed: &'p Parsed<'a>,
  found: Vec<Specifier>,
}

impl Collect<'_, '_> {
  fn push(&mut self, s: &js::StringLiteral) {
    let outer = self.parsed.range(s.span);
    self.found.push(Specifier { value: lossy(s.value), range: outer.start + 1..outer.end - 1, line: self.parsed.line(s.span) });
  }
}

impl<'a> Visit<'a> for Collect<'_, '_> {
  fn visit_import_declaration(&mut self, n: &js::ImportDeclaration<'a>) {
    self.push(&n.source);
  }

  fn visit_export_all_declaration(&mut self, n: &js::ExportAllDeclaration<'a>) {
    self.push(&n.source);
  }

  fn visit_export_from_declaration(&mut self, n: &js::ExportFromDeclaration<'a>) {
    self.push(&n.source);
  }

  fn visit_ts_import_type(&mut self, n: &js::TSImportType<'a>) {
    self.push(&n.source);
  }

  fn visit_import_expression(&mut self, n: &js::ImportExpression<'a>) {
    if let Expression::StringLiteral(s) = &n.source {
      self.push(s);
    }
    walk::walk_import_expression(self, n);
  }

  fn visit_call_expression(&mut self, n: &js::CallExpression<'a>) {
    if matches!(&n.callee, Expression::Identifier(i) if i.name == "require") {
      if let Some(Argument::StringLiteral(s)) = n.arguments.first() {
        self.push(s);
      }
    }
    walk::walk_call_expression(self, n);
  }
}
