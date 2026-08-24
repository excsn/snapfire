use std::path::Path;

use anyhow::{Context, Result, anyhow};
use oxc_allocator::Allocator;
use oxc_ast::ast::{Statement, Str, StringLiteral};
use oxc_codegen::Codegen;
use oxc_isolated_declarations::{IsolatedDeclarations, IsolatedDeclarationsOptions};
use oxc_parser::Parser;
use oxc_span::SourceType;

use crate::transforms::resolve_specifier;

/// Emits the `.d.ts` for one TypeScript file.
///
/// Isolated declarations, so a file's exports must be annotated well enough to describe themselves
/// without the rest of the program. That is the same per-file bargain the rest of the compiler makes
/// and the reason this needs no `node_modules`, but it does mean an export whose type can only be
/// inferred across files is an error here rather than something to guess at.
pub fn declare(path: &Path) -> Result<String> {
  let source_text = std::fs::read_to_string(path).with_context(|| format!("Failed to read file: {:?}", path))?;

  let allocator = Allocator::default();
  let source_type = SourceType::from_path(path).map_err(|e| anyhow!("{e:?}"))?;

  let parsed = Parser::new(&allocator, &source_text, source_type).parse();

  if !parsed.diagnostics.is_empty() {
    return Err(anyhow!("{}", describe(&parsed.diagnostics, path, &source_text)));
  }

  let built = IsolatedDeclarations::new(
    &allocator,
    IsolatedDeclarationsOptions { strip_internal: true },
  )
  .build(&parsed.program);

  if !built.diagnostics.is_empty() {
    return Err(anyhow!("{}", describe(&built.diagnostics, path, &source_text)));
  }

  let mut program = built.program;
  let dir = path.parent().unwrap_or_else(|| Path::new("."));

  for statement in &mut program.body {
    let Some(source) = module_source(statement) else {
      continue;
    };

    if !source.value.as_str().starts_with('.') {
      continue;
    }

    let resolved = resolve_specifier(dir, source.value.as_str());

    if resolved == source.value.as_str() {
      continue;
    }

    source.value = Str::from(&*allocator.alloc_str(&resolved));
    source.raw = None;
  }

  Ok(Codegen::new().build(&program).code)
}

/// The specifier a statement imports from, when it has one.
fn module_source<'a, 'ast>(statement: &'a mut Statement<'ast>) -> Option<&'a mut StringLiteral<'ast>> {
  match statement {
    Statement::ImportDeclaration(declaration) => Some(&mut declaration.source),
    Statement::ExportAllDeclaration(declaration) => Some(&mut declaration.source),
    Statement::ExportNamedDeclaration(declaration) => declaration.source.as_mut(),
    _ => None,
  }
}

/// Reports every diagnostic in the same `file:line:column: message` shape the script compiler uses,
/// so a declaration failure reads like a compile failure rather than like a different tool.
fn describe(diagnostics: &[oxc_diagnostics::OxcDiagnostic], path: &Path, source: &str) -> String {
  diagnostics
    .iter()
    .map(|diagnostic| {
      let offset = diagnostic.labels.first().map(|label| label.offset() as usize);

      match offset.map(|offset| position(source, offset)) {
        Some((line, column)) => format!("{}:{}:{}: {}", path.display(), line, column, diagnostic.message),
        None => format!("{}: {}", path.display(), diagnostic.message),
      }
    })
    .collect::<Vec<_>>()
    .join("\n")
}

/// One-based line and column for a byte offset, counting columns in characters so a line with
/// multibyte text does not report a column past its own length.
fn position(source: &str, offset: usize) -> (usize, usize) {
  let consumed = &source[..offset.min(source.len())];
  let line = consumed.matches('\n').count() + 1;
  let column = consumed.rsplit('\n').next().unwrap_or_default().chars().count() + 1;

  (line, column)
}
