//! The JavaScript inside an MDX file, read with oxc: the callbacks markdown-rs
//! asks whether an ESM block or an expression is complete, and what an ESM
//! block exports.

use markdown::{MdxExpressionKind, MdxSignal};
use oxc_allocator::Allocator;
use oxc_ast::ast::{BindingPattern, Declaration, ExportDefaultDeclarationKind, ExportSpecifier, ImportOrExportKind, Statement};
use oxc_diagnostics::OxcDiagnostic;
use oxc_parser::Parser;
use oxc_span::{GetSpan, SourceType};

fn signal(message: String, relative: usize, len: usize) -> MdxSignal {
  match relative >= len {
    true => MdxSignal::Eof(message, Box::new("mdx".to_owned()), Box::new("esm".to_owned())),
    false => MdxSignal::Error(message, relative, Box::new("mdx".to_owned()), Box::new("esm".to_owned())),
  }
}

/// A diagnostic's message and the byte offset it points at.
fn located(diagnostic: &OxcDiagnostic) -> (String, usize) {
  let at = diagnostic.labels.first().map(|label| label.offset() as usize).unwrap_or(0);
  (diagnostic.message.to_string(), at)
}

/// Whether `value` is a whole ESM block holding only imports and exports.
pub(crate) fn esm(value: &str) -> MdxSignal {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, value, SourceType::tsx()).parse();
  if let Some(first) = parsed.diagnostics.first() {
    let (message, at) = located(first);
    return signal(message, at, value.len());
  }
  for statement in &parsed.program.body {
    if !statement.is_module_declaration() {
      let at = statement.span().start as usize;
      return MdxSignal::Error("Unexpected statement in code: only import and export statements are supported in ESM".to_owned(), at, Box::new("mdx".to_owned()), Box::new("esm".to_owned()));
    }
  }
  MdxSignal::Ok
}

/// Whether `value` is a whole expression, an empty one or a comment among them.
pub(crate) fn expression(value: &str, kind: &MdxExpressionKind) -> MdxSignal {
  if value.trim().is_empty() || is_comment(value) {
    return MdxSignal::Ok;
  }
  let wrapped = match kind {
    MdxExpressionKind::AttributeExpression => format!("<a {{{value}}} />"),
    _ => format!("({value}\n)"),
  };
  let offset = match kind {
    MdxExpressionKind::AttributeExpression => 4,
    _ => 1,
  };
  let allocator = Allocator::default();
  match Parser::new(&allocator, &wrapped, SourceType::tsx()).parse_expression() {
    Ok(_) => MdxSignal::Ok,
    Err(errors) => {
      let (message, at) = errors.first().map(located).unwrap_or_else(|| ("Unexpected end of expression".to_owned(), wrapped.len()));
      signal(message, at.saturating_sub(offset), value.len())
    }
  }
}

fn is_comment(value: &str) -> bool {
  let trimmed = value.trim();
  trimmed.starts_with("/*") && trimmed.ends_with("*/") && !trimmed[2..trimmed.len() - 2].contains("*/")
}

/// What an ESM block declares that the build reads: the names it exports and
/// its `export default`, as the byte range of the `export default` keywords
/// and what follows them.
#[derive(Debug, Default)]
pub(crate) struct Facts {
  pub exports: Vec<String>,
  pub default: Option<DefaultExport>,
}

#[derive(Debug)]
pub(crate) struct DefaultExport {
  /// The bytes of `export default` itself.
  pub keywords: std::ops::Range<usize>,
  /// A declared function or class keeps its name; anything else is bound to `MDXLayout`.
  pub name: Option<String>,
}

pub(crate) fn facts(value: &str) -> Facts {
  let allocator = Allocator::default();
  let parsed = Parser::new(&allocator, value, SourceType::tsx()).parse();
  if !parsed.diagnostics.is_empty() {
    return Facts::default();
  }
  let named = |specifiers: &[ExportSpecifier], kind: ImportOrExportKind, into: &mut Vec<String>| {
    if kind.is_type() {
      return;
    }
    into.extend(specifiers.iter().map(|spec| spec.exported.name().to_string()));
  };
  let mut facts = Facts::default();
  for statement in &parsed.program.body {
    match statement {
      Statement::ExportDeclaration(export) => match &export.declaration {
        Declaration::VariableDeclaration(var) => {
          for d in &var.declarations {
            if let BindingPattern::BindingIdentifier(id) = &d.id {
              facts.exports.push(id.name.to_string());
            }
          }
        }
        Declaration::FunctionDeclaration(f) => facts.exports.extend(f.id.as_ref().map(|id| id.name.to_string())),
        Declaration::ClassDeclaration(c) => facts.exports.extend(c.id.as_ref().map(|id| id.name.to_string())),
        _ => {}
      },
      Statement::ExportNamedDeclaration(export) => named(&export.specifiers, export.export_kind, &mut facts.exports),
      Statement::ExportFromDeclaration(export) => named(&export.specifiers, export.export_kind, &mut facts.exports),
      Statement::ExportDefaultDeclaration(d) => {
        let begin = d.span.start as usize;
        let (name, body) = match &d.declaration {
          ExportDefaultDeclarationKind::FunctionDeclaration(f) => (f.id.as_ref().map(|id| id.name.to_string()), f.span.start as usize),
          ExportDefaultDeclarationKind::ClassDeclaration(c) => (c.id.as_ref().map(|id| id.name.to_string()), c.span.start as usize),
          ExportDefaultDeclarationKind::TSInterfaceDeclaration(_) => continue,
          other => (None, other.span().start as usize),
        };
        facts.default = Some(DefaultExport { keywords: begin..body, name });
      }
      _ => {}
    }
  }
  facts
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn an_esm_block_holds_imports_and_exports_only() {
    assert!(matches!(esm("import a from \"a\"\nexport const b = 1"), MdxSignal::Ok));
    assert!(matches!(esm("export const a = {"), MdxSignal::Eof(..)));
    assert!(matches!(esm("export const a = 1\nconsole.log(a)"), MdxSignal::Error(_, 19, _, _)));
  }

  #[test]
  fn an_expression_is_complete_or_not() {
    assert!(matches!(expression(" 1 + 1 ", &MdxExpressionKind::Expression), MdxSignal::Ok));
    assert!(matches!(expression("/* note */", &MdxExpressionKind::Expression), MdxSignal::Ok));
    assert!(matches!(expression("a +", &MdxExpressionKind::Expression), MdxSignal::Eof(..)));
    assert!(matches!(expression("...props", &MdxExpressionKind::AttributeExpression), MdxSignal::Ok));
  }

  #[test]
  fn facts_name_the_exports_and_the_default() {
    let facts = facts("export const meta = { title: \"a\" }\nexport function helper() {}\nexport default function Layout({ children }) { return children }");
    assert_eq!(facts.exports, ["meta", "helper"]);
    let default = facts.default.expect("a default");
    assert_eq!(default.name.as_deref(), Some("Layout"));
    let value = "export default (props) => props.children";
    let default = super::facts(value).default.expect("a default");
    assert_eq!(&value[default.keywords], "export default ");
    assert_eq!(default.name, None);
  }
}
