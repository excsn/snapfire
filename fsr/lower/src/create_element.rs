//! `createElement(type, props, ...children)` from React read as the JSX it
//! stands for, so the lowerer sees one element shape.

use swc_core::common::{Span, Spanned, SyntaxContext};
use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

/// Rewrites every `createElement` call bound to React's into a JSX element,
/// in place. A call whose tag, props or children have no JSX spelling stays a
/// call.
pub(crate) fn desugar(module: &mut js::Module, top: SyntaxContext) {
  let mut named = Vec::new();
  let mut namespaces = Vec::new();
  for item in &module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::Import(import)) = item else { continue };
    if &*import.src.value.to_atom_lossy() != "react" {
      continue;
    }
    for spec in &import.specifiers {
      match spec {
        js::ImportSpecifier::Named(spec) => {
          let imported = match &spec.imported {
            Some(js::ModuleExportName::Ident(id)) => id.sym.to_string(),
            Some(js::ModuleExportName::Str(s)) => s.value.to_atom_lossy().to_string(),
            None => spec.local.sym.to_string(),
          };
          if imported == "createElement" {
            named.push(spec.local.sym.to_string());
          }
        }
        js::ImportSpecifier::Default(spec) => namespaces.push(spec.local.sym.to_string()),
        js::ImportSpecifier::Namespace(spec) => namespaces.push(spec.local.sym.to_string()),
      }
    }
  }
  if named.is_empty() && namespaces.is_empty() {
    return;
  }
  module.visit_mut_with(&mut Desugar { named, namespaces, top });
}

struct Desugar {
  named: Vec<String>,
  namespaces: Vec<String>,
  top: SyntaxContext,
}

impl VisitMut for Desugar {
  fn visit_mut_expr(&mut self, expr: &mut js::Expr) {
    expr.visit_mut_children_with(self);
    let element = match expr {
      js::Expr::Call(call) if self.is_create_element(&call.callee) => element(call),
      _ => None,
    };
    if let Some(element) = element {
      *expr = js::Expr::JSXElement(Box::new(element));
    }
  }
}

impl Desugar {
  fn is_create_element(&self, callee: &js::Callee) -> bool {
    let js::Callee::Expr(callee) = callee else { return false };
    match &**callee {
      js::Expr::Ident(id) => id.ctxt == self.top && self.named.iter().any(|n| **n == *id.sym),
      js::Expr::Member(member) => match (&*member.obj, &member.prop) {
        (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) => obj.ctxt == self.top && self.namespaces.iter().any(|n| **n == *obj.sym) && &*prop.sym == "createElement",
        _ => false,
      },
      _ => false,
    }
  }
}

fn element(call: &js::CallExpr) -> Option<js::JSXElement> {
  let tag = call.args.first()?;
  if tag.spread.is_some() {
    return None;
  }
  let name = tag_name(&tag.expr)?;
  let attrs = match call.args.get(1) {
    None => Vec::new(),
    Some(props) if props.spread.is_some() => return None,
    Some(props) => attrs(&props.expr)?,
  };
  let children: Vec<js::JSXElementChild> = call.args.iter().skip(2).map(child).collect();
  let span = call.span;
  let closing = (!children.is_empty()).then(|| js::JSXClosingElement { span, name: name.clone() });
  Some(js::JSXElement {
    span,
    opening: js::JSXOpeningElement { name, span, attrs, self_closing: children.is_empty(), type_args: None },
    children,
    closing,
  })
}

/// A string tag names an element when it starts lowercase; an identifier
/// names a component when it starts uppercase; `A.B` is a member tag. Any
/// other tag, a variable holding a tag name among them, has no JSX spelling.
fn tag_name(expr: &js::Expr) -> Option<js::JSXElementName> {
  match expr {
    js::Expr::Lit(js::Lit::Str(s)) => {
      let sym = s.value.to_atom_lossy().into_owned();
      sym.chars().next().filter(|c| c.is_ascii_lowercase())?;
      Some(js::JSXElementName::Ident(js::Ident::new(sym, s.span, SyntaxContext::empty())))
    }
    js::Expr::Ident(id) => {
      id.sym.chars().next().filter(|c| c.is_ascii_uppercase())?;
      Some(js::JSXElementName::Ident(id.clone()))
    }
    js::Expr::Member(member) => match (&*member.obj, &member.prop) {
      (js::Expr::Ident(obj), js::MemberProp::Ident(prop)) => Some(js::JSXElementName::JSXMemberExpr(js::JSXMemberExpr { span: member.span, obj: js::JSXObject::Ident(obj.clone()), prop: prop.clone() })),
      _ => None,
    },
    _ => None,
  }
}

fn attrs(expr: &js::Expr) -> Option<Vec<js::JSXAttrOrSpread>> {
  match expr {
    js::Expr::Lit(js::Lit::Null(_)) => Some(Vec::new()),
    js::Expr::Ident(id) if &*id.sym == "undefined" => Some(Vec::new()),
    js::Expr::Ident(_) => Some(vec![spread(expr.span(), expr.clone())]),
    js::Expr::Object(object) => {
      let mut out = Vec::with_capacity(object.props.len());
      for prop in &object.props {
        out.push(match prop {
          js::PropOrSpread::Spread(s) => spread(s.dot3_token, (*s.expr).clone()),
          js::PropOrSpread::Prop(prop) => match &**prop {
            js::Prop::Shorthand(id) => attr(id.span, &id.sym, js::Expr::Ident(id.clone())),
            js::Prop::KeyValue(kv) => {
              let name = match &kv.key {
                js::PropName::Ident(id) => id.sym.to_string(),
                js::PropName::Str(s) => s.value.to_atom_lossy().to_string(),
                _ => return None,
              };
              attr(kv.key.span(), &name, (*kv.value).clone())
            }
            _ => return None,
          },
        });
      }
      Some(out)
    }
    _ => None,
  }
}

fn spread(span: Span, expr: js::Expr) -> js::JSXAttrOrSpread {
  js::JSXAttrOrSpread::SpreadElement(js::SpreadElement { dot3_token: span, expr: Box::new(expr) })
}

fn attr(span: Span, name: &str, value: js::Expr) -> js::JSXAttrOrSpread {
  js::JSXAttrOrSpread::JSXAttr(js::JSXAttr {
    span,
    name: js::JSXAttrName::Ident(js::IdentName { span, sym: name.into() }),
    value: Some(js::JSXAttrValue::JSXExprContainer(container(value))),
  })
}

fn child(arg: &js::ExprOrSpread) -> js::JSXElementChild {
  match &*arg.expr {
    js::Expr::JSXElement(el) if arg.spread.is_none() => js::JSXElementChild::JSXElement(el.clone()),
    js::Expr::JSXFragment(frag) if arg.spread.is_none() => js::JSXElementChild::JSXFragment(frag.clone()),
    expr => js::JSXElementChild::JSXExprContainer(container(expr.clone())),
  }
}

fn container(expr: js::Expr) -> js::JSXExprContainer {
  js::JSXExprContainer { span: expr.span(), expr: js::JSXExpr::Expr(Box::new(expr)) }
}
