use std::ops::Range;

use swc_core::common::{sync::Lrc, FileName, SourceMap, Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};
use swc_core::ecma::visit::{Visit, VisitWith};

pub(crate) const SCRIPT_EXTENSIONS: &[&str] = &[".tsx", ".ts", ".jsx", ".js", ".mjs", ".cjs", ".mts", ".cts"];

pub(crate) fn is_script(path: &str) -> bool {
  SCRIPT_EXTENSIONS.iter().any(|ext| path.ends_with(ext)) && !path.ends_with(".d.ts")
}

pub(crate) struct Parsed {
  pub module: js::Module,
  cm: Lrc<SourceMap>,
  start: u32,
}

impl Parsed {
  /// The byte range of `span` in the source.
  pub fn range(&self, span: Span) -> Range<usize> {
    (span.lo.0 - self.start) as usize..(span.hi.0 - self.start) as usize
  }

  pub fn line(&self, span: Span) -> usize {
    self.cm.lookup_char_pos(span.lo).line
  }
}

/// `source` as a module. A `.ts`, `.mts` or `.cts` file is TypeScript without
/// JSX, since there `<T>x` is a type assertion; every other script parses JSX.
pub(crate) fn parse(file: &str, source: &str) -> Result<Parsed, String> {
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Custom(file.to_owned())), source.to_owned());
  let tsx = ![".ts", ".mts", ".cts"].iter().any(|ext| file.ends_with(ext));
  let syntax = Syntax::Typescript(TsSyntax { tsx, decorators: true, ..Default::default() });
  let lexer = Lexer::new(syntax, js::EsVersion::latest(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  let module = parser.parse_module().map_err(|e| {
    let loc = cm.lookup_char_pos(e.span().lo);
    format!("{}:{}: {}", loc.line, loc.col_display + 1, e.kind().msg())
  })?;
  Ok(Parsed { module, start: fm.start_pos.0, cm })
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
  parsed.module.visit_with(&mut collect);
  collect.found.sort_by_key(|s| s.range.start);
  collect.found
}

struct Collect<'a> {
  parsed: &'a Parsed,
  found: Vec<Specifier>,
}

impl Collect<'_> {
  fn push(&mut self, s: &js::Str) {
    let outer = self.parsed.range(s.span);
    self.found.push(Specifier { value: s.value.to_string_lossy().into_owned(), range: outer.start + 1..outer.end - 1, line: self.parsed.line(s.span) });
  }
}

impl Visit for Collect<'_> {
  fn visit_import_decl(&mut self, n: &js::ImportDecl) {
    self.push(&n.src);
  }

  fn visit_export_all(&mut self, n: &js::ExportAll) {
    self.push(&n.src);
  }

  fn visit_named_export(&mut self, n: &js::NamedExport) {
    if let Some(src) = &n.src {
      self.push(src);
    }
  }

  fn visit_ts_import_type(&mut self, n: &js::TsImportType) {
    self.push(&n.arg);
  }

  fn visit_call_expr(&mut self, n: &js::CallExpr) {
    let named = match &n.callee {
      js::Callee::Import(_) => true,
      js::Callee::Expr(e) => matches!(&**e, js::Expr::Ident(i) if &*i.sym == "require"),
      js::Callee::Super(_) => false,
    };
    if named {
      if let Some(js::Expr::Lit(js::Lit::Str(s))) = n.args.first().map(|a| &*a.expr) {
        self.push(s);
      }
    }
    n.visit_children_with(self);
  }
}
