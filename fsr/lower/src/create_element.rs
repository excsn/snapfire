//! `createElement(type, props, ...children)` from React read as the JSX it
//! stands for, so the lowerer sees one element shape.

use oxc_allocator::Allocator;
use oxc_ast::ast as js;
use oxc_ast_visit::VisitMut;
use oxc_span::{GetSpan, Span};

use crate::scope::Scope;
use crate::syntax::Build;
use crate::Ctxt;

/// Rewrites every `createElement` call bound to React's into a JSX element,
/// in place. A call whose tag, props or children have no JSX spelling stays a
/// call.
pub(crate) fn desugar<'a>(arena: &'a Allocator, scope: &Scope, module: &mut js::Program<'a>) {
  let mut named = Vec::new();
  let mut namespaces = Vec::new();
  for stmt in &module.body {
    let js::Statement::ImportDeclaration(import) = stmt else { continue };
    if crate::lossy(import.source.value) != "react" {
      continue;
    }
    for spec in import.specifiers.iter().flatten() {
      match spec {
        js::ImportDeclarationSpecifier::ImportSpecifier(spec) => {
          let imported = match &spec.imported {
            js::ModuleExportName::StringLiteral(s) => crate::lossy(s.value),
            other => other.name().to_string(),
          };
          if imported == "createElement" {
            named.push(spec.local.name.to_string());
          }
        }
        js::ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => namespaces.push(spec.local.name.to_string()),
        js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => namespaces.push(spec.local.name.to_string()),
      }
    }
  }
  if named.is_empty() && namespaces.is_empty() {
    return;
  }
  Desugar { named, namespaces, scope, b: Build::new(arena) }.visit_program(module);
}

struct Desugar<'s, 'a> {
  named: Vec<String>,
  namespaces: Vec<String>,
  scope: &'s Scope,
  b: Build<'a>,
}

impl<'a> VisitMut<'a> for Desugar<'_, 'a> {
  fn visit_expression(&mut self, expr: &mut js::Expression<'a>) {
    oxc_ast_visit::walk_mut::walk_expression(self, expr);
    let element = match expr {
      js::Expression::CallExpression(call) if self.is_create_element(&call.callee) => element(self.b, call),
      _ => None,
    };
    if let Some(element) = element {
      *expr = js::Expression::JSXElement(self.b.boxed(element));
    }
  }
}

impl Desugar<'_, '_> {
  fn is_create_element(&self, callee: &js::Expression) -> bool {
    match callee {
      js::Expression::Identifier(id) => self.scope.of(id) == Ctxt::Top && self.named.iter().any(|n| n == id.name.as_str()),
      js::Expression::StaticMemberExpression(member) => match &member.object {
        js::Expression::Identifier(obj) => self.scope.of(obj) == Ctxt::Top && self.namespaces.iter().any(|n| n == obj.name.as_str()) && member.property.name.as_str() == "createElement",
        _ => false,
      },
      _ => false,
    }
  }
}

fn element<'a>(b: Build<'a>, call: &js::CallExpression<'a>) -> Option<js::JSXElement<'a>> {
  let tag = call.arguments.first()?.as_expression()?;
  let name = tag_name(b, tag)?;
  let attrs = match call.arguments.get(1) {
    None => Vec::new(),
    Some(js::Argument::SpreadElement(_)) => return None,
    Some(props) => attrs(b, props.to_expression())?,
  };
  let children: Vec<js::JSXChild<'a>> = call.arguments.iter().skip(2).map(|arg| child(b, arg)).collect();
  let span = call.span;
  let ast = b.ast();
  let closing = (!children.is_empty()).then(|| b.boxed(js::JSXClosingElement::new(span, b.copy(&name), &ast)));
  let opening = b.boxed(js::JSXOpeningElement::new(span, name, None::<oxc_allocator::Box<'a, js::TSTypeParameterInstantiation<'a>>>, b.vec(attrs), &ast));
  Some(js::JSXElement::new(span, opening, b.vec(children), closing, &ast))
}

/// A string tag names an element when it starts lowercase; an identifier
/// names a component when it starts uppercase; `A.B` is a member tag. Any
/// other tag, a variable holding a tag name among them, has no JSX spelling.
fn tag_name<'a>(b: Build<'a>, expr: &js::Expression<'a>) -> Option<js::JSXElementName<'a>> {
  let ast = b.ast();
  match expr {
    js::Expression::StringLiteral(s) => {
      let sym = crate::lossy(s.value);
      sym.chars().next().filter(|c| c.is_ascii_lowercase())?;
      Some(js::JSXElementName::Identifier(b.boxed(js::JSXIdentifier::new(s.span, b.str(&sym), &ast))))
    }
    js::Expression::Identifier(id) => {
      id.name.chars().next().filter(|c| c.is_ascii_uppercase())?;
      Some(js::JSXElementName::IdentifierReference(b.boxed(b.copy(&**id))))
    }
    js::Expression::StaticMemberExpression(member) => match &member.object {
      js::Expression::Identifier(obj) => {
        let object = js::JSXMemberExpressionObject::IdentifierReference(b.boxed(b.copy(&**obj)));
        let property = js::JSXIdentifier::new(member.property.span, b.str(member.property.name.as_str()), &ast);
        Some(js::JSXElementName::MemberExpression(b.boxed(js::JSXMemberExpression::new(member.span, object, property, &ast))))
      }
      _ => None,
    },
    _ => None,
  }
}

fn attrs<'a>(b: Build<'a>, expr: &js::Expression<'a>) -> Option<Vec<js::JSXAttributeItem<'a>>> {
  match expr {
    js::Expression::NullLiteral(_) => Some(Vec::new()),
    js::Expression::Identifier(id) if id.name.as_str() == "undefined" => Some(Vec::new()),
    js::Expression::Identifier(_) => Some(vec![spread(b, expr.span(), b.copy(expr))]),
    js::Expression::ObjectExpression(object) => {
      let mut out = Vec::with_capacity(object.properties.len());
      for prop in &object.properties {
        out.push(match prop {
          js::ObjectPropertyKind::SpreadProperty(s) => spread(b, s.span, b.copy(&s.argument)),
          js::ObjectPropertyKind::ObjectProperty(p) if p.kind != js::PropertyKind::Init || p.method || p.computed => return None,
          js::ObjectPropertyKind::ObjectProperty(p) if p.shorthand => {
            let js::Expression::Identifier(id) = &p.value else { return None };
            attr(b, id.span, id.name.as_str(), b.copy(&p.value))
          }
          js::ObjectPropertyKind::ObjectProperty(p) => {
            let name = match &p.key {
              js::PropertyKey::StaticIdentifier(id) => id.name.to_string(),
              js::PropertyKey::StringLiteral(s) => crate::lossy(s.value),
              _ => return None,
            };
            attr(b, p.key.span(), &name, b.copy(&p.value))
          }
        });
      }
      Some(out)
    }
    _ => None,
  }
}

fn spread<'a>(b: Build<'a>, span: Span, expr: js::Expression<'a>) -> js::JSXAttributeItem<'a> {
  js::JSXAttributeItem::SpreadAttribute(b.boxed(js::JSXSpreadAttribute::new(span, expr, &b.ast())))
}

fn attr<'a>(b: Build<'a>, span: Span, name: &str, value: js::Expression<'a>) -> js::JSXAttributeItem<'a> {
  let ast = b.ast();
  let name = js::JSXAttributeName::Identifier(b.boxed(js::JSXIdentifier::new(span, b.str(name), &ast)));
  js::JSXAttributeItem::Attribute(b.boxed(js::JSXAttribute::new(span, name, Some(js::JSXAttributeValue::ExpressionContainer(b.boxed(container(b, value)))), &ast)))
}

fn child<'a>(b: Build<'a>, arg: &js::Argument<'a>) -> js::JSXChild<'a> {
  match arg {
    js::Argument::JSXElement(el) => js::JSXChild::Element(b.boxed(b.copy(&**el))),
    js::Argument::JSXFragment(frag) => js::JSXChild::Fragment(b.boxed(b.copy(&**frag))),
    other => js::JSXChild::ExpressionContainer(b.boxed(container(b, b.copy(crate::syntax::arg_expr(other))))),
  }
}

fn container<'a>(b: Build<'a>, expr: js::Expression<'a>) -> js::JSXExpressionContainer<'a> {
  js::JSXExpressionContainer::new(expr.span(), js::JSXExpression::from(expr), &b.ast())
}
