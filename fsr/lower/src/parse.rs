use super::*;

impl<'a> Param<'a> {
  /// The name the parameter binds when it is one name with no default, as a plain `x` or `x: T`.
  pub(crate) fn ident(&self) -> Option<&'a js::BindingIdentifier<'a>> {
    match self.pattern {
      js::BindingPattern::BindingIdentifier(id) if !self.rest && self.default.is_none() => Some(id),
      _ => None,
    }
  }
}

impl<'a> Param<'a> {
  /// A pattern standing where a parameter would, as a declaration binds it: no type, no default, no rest.
  pub(crate) fn of_pattern(pattern: &'a js::BindingPattern<'a>) -> Self {
    Param { pattern, type_annotation: None, default: None, rest: false, span: pattern.span() }
  }
}

/// Each parameter of `params` in order, the rest parameter last.
pub(crate) fn params_of<'a>(params: &'a js::FormalParameters<'a>) -> Vec<Param<'a>> {
  let mut out: Vec<Param<'a>> = params
    .items
    .iter()
    .map(|p| match &p.pattern {
      js::BindingPattern::AssignmentPattern(assign) => Param { pattern: &assign.left, type_annotation: p.type_annotation.as_deref(), default: Some(&assign.right), rest: false, span: p.span },
      pattern => Param { pattern, type_annotation: p.type_annotation.as_deref(), default: p.initializer.as_deref(), rest: false, span: p.span },
    })
    .collect();
  if let Some(rest) = &params.rest {
    out.push(Param { pattern: &rest.rest.argument, type_annotation: rest.type_annotation.as_deref(), default: None, rest: true, span: rest.span });
  }
  out
}

/// The first parameter of `params`.
pub(crate) fn first_param<'a>(params: &'a js::FormalParameters<'a>) -> Option<Param<'a>> {
  params_of(params).into_iter().next()
}

/// The statements of a function's body, none for a declaration without one.
pub(crate) fn body_of<'a>(f: &'a js::Function<'a>) -> &'a [js::Statement<'a>] {
  f.body.as_ref().map(|b| b.statements.as_slice()).unwrap_or(&[])
}

/// A string literal's value, a lone surrogate read as U+FFFD.
pub(crate) fn lossy(value: oxc_str::JSStr<'_>) -> String {
  match value.as_str() {
    Some(text) => text.to_owned(),
    None => value.chars().map(|c| c.to_char().unwrap_or('\u{FFFD}')).collect(),
  }
}

/// Every name a pattern binds, for a declaration shape the build does not
/// otherwise read: the names are reported so nothing is skipped unseen.
pub(crate) fn bound_names<'a>(pat: &'a js::BindingPattern<'a>, out: &mut impl FnMut(&'a str, Span)) {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => out(id.name.as_str(), id.span),
    js::BindingPattern::ArrayPattern(arr) => {
      arr.elements.iter().flatten().for_each(|p| bound_names(p, out));
      if let Some(rest) = &arr.rest {
        bound_names(&rest.argument, out);
      }
    }
    js::BindingPattern::AssignmentPattern(assign) => bound_names(&assign.left, out),
    js::BindingPattern::ObjectPattern(obj) => {
      for prop in &obj.properties {
        bound_names(&prop.value, out);
      }
      if let Some(rest) = &obj.rest {
        bound_names(&rest.argument, out);
      }
    }
  }
}

/// Whether `file` may hold JSX, as TypeScript decides: a `.tsx` or any
/// JavaScript file. A `.ts` cannot, since there `<T>x` is a type assertion.
pub(crate) fn jsx_parses(file: &str) -> bool {
  [".tsx", ".jsx", ".js", ".mjs", ".cjs"].iter().any(|ext| file.ends_with(ext))
}

pub(crate) fn parse<'a>(arena: &'a Allocator, file: &str, source: &str) -> Result<Parsed<'a>, LowerError> {
  parse_with(arena, file, source, false)
}

pub(crate) fn parse_with<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool) -> Result<Parsed<'a>, LowerError> {
  parse_from(arena, file, source, tsx, None)
}

impl Origin {
  pub fn new(source: String, segments: Vec<OriginSegment>) -> Self {
    Self { source, segments }
  }
}

impl OriginSegment {
  pub fn new(generated: usize, len: usize, original: usize, verbatim: bool) -> Self {
    Self { generated, len, original, verbatim }
  }
}

impl Origin {
  /// The one-based line and column in the original text of byte `offset` of the module.
  pub fn position(&self, offset: usize) -> (usize, usize) {
    let index = self.segments.partition_point(|s| s.generated <= offset);
    let original = match index.checked_sub(1).map(|i| self.segments[i]) {
      Some(s) if s.verbatim && offset < s.generated + s.len.max(1) => s.original + (offset - s.generated),
      Some(s) => s.original,
      None => 0,
    };
    let mut end = original.min(self.source.len());
    while !self.source.is_char_boundary(end) {
      end -= 1;
    }
    let before = &self.source[..end];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map(|l| l.chars().count()).unwrap_or(0) + 1;
    (line, column)
  }
}

/// The byte offset each line of `text` starts at.
pub(crate) fn line_starts(text: &str) -> Vec<usize> {
  std::iter::once(0).chain(text.match_indices('\n').map(|(at, _)| at + 1)).collect()
}

/// The one-based line and column of byte `offset` of `text`, the column counted in characters.
pub(crate) fn line_column(text: &str, starts: &[usize], offset: usize) -> (usize, usize) {
  let line = starts.partition_point(|start| *start <= offset).max(1);
  let start = starts[line - 1];
  let mut end = offset.min(text.len());
  while !text.is_char_boundary(end) {
    end -= 1;
  }
  (line, text[start..end.max(start)].chars().count() + 1)
}

/// The one-based line and column a parse error points at, in the file the author wrote.
pub(crate) fn error_at(source: &str, origin: Option<&Origin>, error: &oxc_diagnostics::OxcDiagnostic) -> (usize, usize) {
  let offset = error.labels.first().map(|label| label.offset() as usize).unwrap_or(0);
  match origin {
    Some(origin) => origin.position(offset),
    None => line_column(source, &line_starts(source), offset),
  }
}

pub(crate) fn source_type(tsx: bool) -> oxc_span::SourceType {
  oxc_span::SourceType::ts().with_jsx(tsx)
}

pub(crate) fn parse_from<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool, origin: Option<std::rc::Rc<Origin>>) -> Result<Parsed<'a>, LowerError> {
  parse_into(arena, file, source, tsx, origin, |_, _, _| {})
}

/// [`parse_from`] with `edit` run over the tree once its scopes are known.
pub(crate) fn parse_into<'a>(arena: &'a Allocator, file: &str, source: &str, tsx: bool, origin: Option<std::rc::Rc<Origin>>, edit: impl FnOnce(&'a Allocator, &scope::Scope, &mut js::Program<'a>)) -> Result<Parsed<'a>, LowerError> {
  let parse_error = |error: &oxc_diagnostics::OxcDiagnostic| {
    let (line, column) = error_at(source, origin.as_deref(), error);
    LowerError::Parse { file: file.to_owned(), message: format!("{line}:{column}: {}", error.message) }
  };
  let starts = line_starts(source);
  let at = |span: Span| match &origin {
    Some(origin) => origin.position(span.start as usize),
    None => line_column(source, &starts, span.start as usize),
  };
  let (program, scope) = (|| {
    let scope::Analysed { mut program, scope, errors, .. } = scope::analyse(arena, source, source_type(tsx));
    if let Some(error) = errors.first() {
      return Err(parse_error(error));
    }
    create_element::desugar(arena, &scope, &mut program);
    methods::desugar(&mut program);
    if let Some((name, again, first)) = bound_twice(&program) {
      let ((line, column), (first_line, _)) = (at(again), at(first));
      return Err(LowerError::Residue(Residue {
        file: file.to_owned(),
        line,
        column,
        message: format!("`{name}`, bound twice at the top of the module; its first binding is line {first_line}"),
        hint: Some("JavaScript refuses a module that binds one name twice; rename one of them".to_owned()),
        via: Vec::new(),
      }));
    }
    if let Some((source, fsr, span)) = replaced_import(&program) {
      let (line, column) = at(span);
      return Err(LowerError::Residue(Residue {
        file: file.to_owned(),
        line,
        column,
        message: format!("`{source}`, which is not FSR's"),
        hint: Some(format!("FSR provides its own; import it from `{fsr}`, which `fsr migrate` writes")),
        via: Vec::new(),
      }));
    }
    edit(arena, &scope, &mut program);
    Ok((program, scope))
  })()?;
  Ok(Parsed { file: file.to_owned(), program, arena, scope: std::rc::Rc::new(scope), line_starts: starts, snippets: std::cell::RefCell::new(Vec::new()), meanings: std::cell::OnceCell::new(), origin })
}

/// The specifier `stmt` imports or re-exports from, with the statement's span.
pub(crate) fn module_source<'a>(stmt: &'a js::Statement<'a>) -> Option<(&'a js::StringLiteral<'a>, Span)> {
  match stmt {
    js::Statement::ImportDeclaration(import) => Some((&import.source, import.span)),
    js::Statement::ExportFromDeclaration(export) => Some((&export.source, export.span)),
    js::Statement::ExportAllDeclaration(all) => Some((&all.source, all.span)),
    _ => None,
  }
}

/// The first import or re-export in `module` from an npm package FSR provides its own of ([`terminal::REPLACED`]), with FSR's module and where.
pub(crate) fn replaced_import(module: &js::Program<'_>) -> Option<(String, &'static str, Span)> {
  module.body.iter().find_map(|stmt| {
    let (src, span) = module_source(stmt)?;
    let source = lossy(src.value);
    terminal::REPLACED.iter().find(|(npm, _)| *npm == source).map(|(_, fsr)| (source, *fsr, span))
  })
}

/// The first name `module` binds twice at its top level, with the spans of the second binding and the first: an import, a `let`, `const` or `var`, a function or a class. Two `var`s of one name, a function's overload signatures and `declare` statements bind it once, as JavaScript and TypeScript have it; types, enums and namespaces are a namespace of their own and are left out.
pub(crate) fn bound_twice(module: &js::Program<'_>) -> Option<(String, Span, Span)> {
  let mut bindings = Bindings(Vec::new());
  for stmt in &module.body {
    let twice = match stmt {
      js::Statement::ImportDeclaration(import) if !import.import_kind.is_type() => import.specifiers.iter().flatten().find_map(|specifier| match specifier {
        js::ImportDeclarationSpecifier::ImportSpecifier(n) if n.import_kind.is_type() => None,
        js::ImportDeclarationSpecifier::ImportSpecifier(n) => bindings.bind(&n.local, false),
        js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) => bindings.bind(&d.local, false),
        js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) => bindings.bind(&n.local, false),
      }),
      js::Statement::ExportDeclaration(export) => bindings.decl(&export.declaration),
      js::Statement::ExportDefaultDeclaration(export) => match &export.declaration {
        js::ExportDefaultDeclarationKind::FunctionDeclaration(f) if f.body.is_some() => f.id.as_ref().and_then(|id| bindings.bind(id, false)),
        js::ExportDefaultDeclarationKind::ClassDeclaration(c) => c.id.as_ref().and_then(|id| bindings.bind(id, false)),
        _ => None,
      },
      other => other.as_declaration().and_then(|decl| bindings.decl(decl)),
    };
    if twice.is_some() {
      return twice;
    }
  }
  None
}

impl Bindings {
  /// Binds `id`, answering the name and both spans when it was bound already by anything but two `var`s.
  pub(crate) fn bind(&mut self, id: &js::BindingIdentifier, var: bool) -> Option<(String, Span, Span)> {
    match self.0.iter().find(|(name, _, _)| *name == id.name.as_str()) {
      Some((_, _, true)) if var => None,
      Some((name, first, _)) => Some((name.clone(), id.span, *first)),
      None => {
        self.0.push((id.name.to_string(), id.span, var));
        None
      }
    }
  }

  pub(crate) fn decl(&mut self, decl: &js::Declaration) -> Option<(String, Span, Span)> {
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.body.is_some() && !f.declare => f.id.as_ref().and_then(|id| self.bind(id, false)),
      js::Declaration::ClassDeclaration(c) if !c.declare => c.id.as_ref().and_then(|id| self.bind(id, false)),
      js::Declaration::VariableDeclaration(var) if !var.declare => {
        let is_var = var.kind == js::VariableDeclarationKind::Var;
        let mut names = Vec::new();
        for d in &var.declarations {
          pattern_names(&d.id, &mut names);
        }
        names.iter().find_map(|id| self.bind(id, is_var))
      }
      _ => None,
    }
  }
}

/// Every name `pat` binds, in order.
pub(crate) fn pattern_names<'p>(pat: &'p js::BindingPattern<'p>, out: &mut Vec<&'p js::BindingIdentifier<'p>>) {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => out.push(id),
    js::BindingPattern::ArrayPattern(array) => {
      array.elements.iter().flatten().for_each(|p| pattern_names(p, out));
      if let Some(rest) = &array.rest {
        pattern_names(&rest.argument, out);
      }
    }
    js::BindingPattern::ObjectPattern(object) => {
      for prop in &object.properties {
        pattern_names(&prop.value, out);
      }
      if let Some(rest) = &object.rest {
        pattern_names(&rest.argument, out);
      }
    }
    js::BindingPattern::AssignmentPattern(a) => pattern_names(&a.left, out),
  }
}

/// Moves every span in `expr` on by `by`.
pub(crate) fn shift_spans(expr: &mut js::Expression<'_>, by: u32) {
  struct Shift(u32);
  impl<'a> oxc_ast_visit::VisitMut<'a> for Shift {
    fn visit_span(&mut self, span: &mut Span) {
      *span = Span::new(span.start + self.0, span.end + self.0);
    }
  }
  oxc_ast_visit::VisitMut::visit_expression(&mut Shift(by), expr);
}

impl<'a> Parsed<'a> {
  /// The module.
  pub(crate) fn module(&self) -> &js::Program<'a> {
    &self.program
  }

  /// The arena the module lives in, for the nodes the lowerers write.
  pub(crate) fn arena(&self) -> &'a Allocator {
    self.arena
  }

  /// The module's text.
  pub(crate) fn text(&self) -> &str {
    self.module().source_text
  }

  /// What `id` names where it is written.
  pub(crate) fn ctxt(&self, id: &js::IdentifierReference) -> Ctxt {
    self.scope.of(id)
  }

  /// What a binding binds.
  pub(crate) fn binding_ctxt(&self, id: &js::BindingIdentifier) -> Ctxt {
    self.scope.of_binding(id)
  }

  /// Parses `source` as one TypeScript expression written at `line` and
  /// `column` of this file. Its spans start past the module's text, so a
  /// residue inside it names where the expression sits.
  pub(crate) fn parse_expr_at(&self, source: &str, line: usize, column: usize) -> Result<js::Expression<'a>, String> {
    let padded = format!("{}{}{source}", "\n".repeat(line.saturating_sub(1)), " ".repeat(column.saturating_sub(1)));
    let text = self.arena().alloc_str(&padded);
    let mut expr = oxc_parser::Parser::new(self.arena(), text, oxc_span::SourceType::ts()).parse_expression().map_err(|errors| errors.first().map(|e| e.message.to_string()).unwrap_or_default())?;
    let mut snippets = self.snippets.borrow_mut();
    let base = snippets.last().map(|s| s.base as usize + s.padded.len() + 1).unwrap_or(self.text().len() + 1) as u32;
    shift_spans(&mut expr, base);
    snippets.push(Snippet { base, padded });
    Ok(expr)
  }

  /// The byte range of `span` in the file's text, or in the text of the expression that holds it.
  pub(crate) fn range(&self, span: Span) -> std::ops::Range<usize> {
    let base = self.snippet_base(span.start);
    (span.start - base) as usize..(span.end - base) as usize
  }

  /// Where the text `pos` falls in starts: 0 for the module, a snippet's base for an expression parsed after it.
  pub(crate) fn snippet_base(&self, pos: u32) -> u32 {
    self.snippets.borrow().iter().rev().find(|s| s.base <= pos).map(|s| s.base).unwrap_or(0)
  }

  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    let (line, column) = self.loc(span.start);
    Residue { file: self.file.clone(), line, column, message: message.into(), hint: None, via: Vec::new() }
  }

  /// The one-based line and column `pos` names, in the file the author wrote.
  pub(crate) fn loc(&self, pos: u32) -> (usize, usize) {
    if let Some(snippet) = self.snippets.borrow().iter().rev().find(|s| s.base <= pos) {
      return line_column(&snippet.padded, &line_starts(&snippet.padded), (pos - snippet.base) as usize);
    }
    if let Some(origin) = &self.origin {
      return origin.position(pos as usize);
    }
    line_column(self.text(), &self.line_starts, pos as usize)
  }

  /// The byte offset of a 1-based line and column as [`Parsed::position`] counts them.
  pub(crate) fn offset(&self, line: usize, column: usize) -> Option<usize> {
    let mut at = 0;
    for (i, text) in self.text().split_inclusive('\n').enumerate() {
      if i + 1 == line {
        return Some(at + text.char_indices().nth(column.saturating_sub(1)).map(|(byte, _)| byte).unwrap_or(0));
      }
      at += text.len();
    }
    None
  }

  /// The one-based line and column of a byte offset in the file's text.
  pub(crate) fn position(&self, offset: usize) -> (usize, usize) {
    self.loc(offset as u32)
  }

  pub(crate) fn exports(&self) -> impl Iterator<Item = (&str, Exported<'_>)> {
    let mut out: Vec<(&str, Exported<'_>)> = Vec::new();
    for stmt in &self.module().body {
      match stmt {
        js::Statement::ExportDeclaration(export) => self.declared(&export.declaration, &mut out),
        js::Statement::ExportNamedDeclaration(named) if !named.export_kind.is_type() => {
          for spec in &named.specifiers {
            if spec.export_kind.is_type() {
              continue;
            }
            let orig = match &spec.local {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            let name = match &spec.exported {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            match self.local(orig) {
              Some(exported) => out.push((name, exported)),
              None => out.push((name, Exported::Unreadable(spec.span, "exported from a name this module does not declare"))),
            }
          }
        }
        js::Statement::ExportFromDeclaration(from) if !from.export_kind.is_type() => {
          for spec in &from.specifiers {
            if spec.export_kind.is_type() {
              continue;
            }
            let name = match &spec.exported {
              js::ModuleExportName::IdentifierReference(id) => id.name.as_str(),
              js::ModuleExportName::IdentifierName(id) => id.name.as_str(),
              js::ModuleExportName::StringLiteral(_) => continue,
            };
            if matches!(spec.local, js::ModuleExportName::StringLiteral(_)) {
              continue;
            }
            out.push((name, Exported::Unreadable(spec.span, "exported from a name this module does not declare")));
          }
        }
        _ => {}
      }
    }
    out.into_iter()
  }

  /// Every name a declaration exports, in source order.
  pub(crate) fn declared<'p>(&'p self, decl: &'p js::Declaration<'p>, out: &mut Vec<(&'p str, Exported<'p>)>) {
    match decl {
      js::Declaration::FunctionDeclaration(f) => {
        let Some(id) = &f.id else { return };
        out.push((id.name.as_str(), Exported::Function(first_param(&f.params), body_of(f))));
      }
      js::Declaration::VariableDeclaration(var) => {
        for d in &var.declarations {
          let js::BindingPattern::BindingIdentifier(name) = &d.id else {
            bound_names(&d.id, &mut |name, span| out.push((name, Exported::Unreadable(span, "bound by a destructuring the build does not read"))));
            continue;
          };
          let Some(init) = &d.init else { continue };
          out.push((name.name.as_str(), self.classify(init)));
        }
      }
      _ => {}
    }
  }

  /// What a module-level declaration of `name` is, for an `export { name }`
  /// that stands apart from it.
  pub(crate) fn local(&self, name: &str) -> Option<Exported<'_>> {
    let mut found = Vec::new();
    for stmt in &self.module().body {
      let decl = match stmt {
        js::Statement::ExportDeclaration(export) => &export.declaration,
        other => match other.as_declaration() {
          Some(decl) => decl,
          None => continue,
        },
      };
      self.declared(decl, &mut found);
    }
    found.into_iter().find(|(n, _)| *n == name).map(|(_, e)| e)
  }
}

impl Parsed<'_> {
  /// Whether `callee` names `want` from the server tier, however the module
  /// reaches it; with no meanings resolved, the bare name nothing binds.
  pub(crate) fn names_server(&self, callee: &js::Expression, want: &str) -> bool {
    match self.meanings.get() {
      Some(meanings) => meanings.names_server(callee, want),
      None => matches!(callee, js::Expression::Identifier(id) if self.ctxt(id) == Ctxt::Unresolved && id.name.as_str() == want),
    }
  }

  pub(crate) fn classify<'a>(&'a self, init: &'a js::Expression<'a>) -> Exported<'a> {
    classify_in(self, init)
  }
}
