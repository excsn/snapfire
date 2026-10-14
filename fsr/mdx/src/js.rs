//! The JavaScript inside an MDX file, read with swc: the callbacks markdown-rs
//! asks whether an ESM block or an expression is complete, and what an ESM
//! block exports.

use markdown::{MdxExpressionKind, MdxSignal};
use swc_core::common::{sync::Lrc, BytePos, FileName, SourceMap, Span, Spanned};
use swc_core::ecma::ast as js;
use swc_core::ecma::parser::{lexer::Lexer, Parser, StringInput, Syntax, TsSyntax};

fn syntax() -> Syntax {
  Syntax::Typescript(TsSyntax { tsx: true, ..Default::default() })
}

fn signal(message: String, relative: usize, len: usize) -> MdxSignal {
  match relative >= len {
    true => MdxSignal::Eof(message, Box::new("mdx".to_owned()), Box::new("esm".to_owned())),
    false => MdxSignal::Error(message, relative, Box::new("mdx".to_owned()), Box::new("esm".to_owned())),
  }
}

fn parse_module(value: &str) -> Result<(js::Module, BytePos), (String, usize)> {
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Anon), value.to_owned());
  let lexer = Lexer::new(syntax(), js::EsVersion::latest(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  let module = parser.parse_module().map_err(|e| (e.kind().msg().to_string(), (e.span().lo.0 - fm.start_pos.0) as usize))?;
  if let Some(e) = parser.take_errors().into_iter().next() {
    return Err((e.kind().msg().to_string(), (e.span().lo.0 - fm.start_pos.0) as usize));
  }
  Ok((module, fm.start_pos))
}

/// Whether `value` is a whole ESM block holding only imports and exports.
pub(crate) fn esm(value: &str) -> MdxSignal {
  let (module, start) = match parse_module(value) {
    Ok(parsed) => parsed,
    Err((message, at)) => return signal(message, at, value.len()),
  };
  for item in &module.body {
    if let js::ModuleItem::Stmt(stmt) = item {
      let at = (stmt.span().lo.0 - start.0) as usize;
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
  let cm: Lrc<SourceMap> = Default::default();
  let fm = cm.new_source_file(Lrc::new(FileName::Anon), wrapped.clone());
  let lexer = Lexer::new(syntax(), js::EsVersion::latest(), StringInput::from(&*fm), None);
  let mut parser = Parser::new_from(lexer);
  match parser.parse_expr() {
    Ok(_) => match parser.take_errors().into_iter().next() {
      None => MdxSignal::Ok,
      Some(e) => signal(e.kind().msg().to_string(), ((e.span().lo.0 - fm.start_pos.0) as usize).saturating_sub(offset), value.len()),
    },
    Err(e) => signal(e.kind().msg().to_string(), ((e.span().lo.0 - fm.start_pos.0) as usize).saturating_sub(offset), value.len()),
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
  let Ok((module, start)) = parse_module(value) else { return Facts::default() };
  let at = |span: Span| (span.lo.0 - start.0) as usize;
  let mut facts = Facts::default();
  for item in &module.body {
    let js::ModuleItem::ModuleDecl(decl) = item else { continue };
    match decl {
      js::ModuleDecl::ExportDecl(export) => match &export.decl {
        js::Decl::Var(var) => {
          for d in &var.decls {
            if let js::Pat::Ident(id) = &d.name {
              facts.exports.push(id.id.sym.to_string());
            }
          }
        }
        js::Decl::Fn(f) => facts.exports.push(f.ident.sym.to_string()),
        js::Decl::Class(c) => facts.exports.push(c.ident.sym.to_string()),
        _ => {}
      },
      js::ModuleDecl::ExportNamed(named) if !named.type_only => {
        for spec in &named.specifiers {
          if let js::ExportSpecifier::Named(spec) = spec {
            let name = match spec.exported.as_ref().unwrap_or(&spec.orig) {
              js::ModuleExportName::Ident(id) => id.sym.to_string(),
              js::ModuleExportName::Str(s) => s.value.to_atom_lossy().to_string(),
            };
            facts.exports.push(name);
          }
        }
      }
      js::ModuleDecl::ExportDefaultDecl(d) => {
        let begin = at(d.span);
        let (name, body) = match &d.decl {
          js::DefaultDecl::Fn(f) => (f.ident.as_ref().map(|i| i.sym.to_string()), at(f.function.span)),
          js::DefaultDecl::Class(c) => (c.ident.as_ref().map(|i| i.sym.to_string()), at(c.class.span)),
          js::DefaultDecl::TsInterfaceDecl(_) => continue,
        };
        facts.default = Some(DefaultExport { keywords: begin..body, name });
      }
      js::ModuleDecl::ExportDefaultExpr(e) => {
        facts.default = Some(DefaultExport { keywords: at(e.span)..at(e.expr.span()), name: None });
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
