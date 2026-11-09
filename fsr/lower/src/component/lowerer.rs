use super::*;

impl<'p> Placer<'p> for ComponentLowerer<'_, 'p> {
  fn lowerer(&mut self) -> &mut Lowerer<'p> {
    &mut self.lowerer
  }

  fn residue_at(&self, at: At, message: String) -> Residue {
    match at {
      At::Span(span) => self.lowerer.residue(span, message),
      At::Line(line, column) => Residue { file: self.lowerer.parsed.file.clone(), line, column, message, hint: None, via: Vec::new() },
    }
  }

  fn assets(&self) -> Rc<dyn AssetResolver> {
    self.assets.clone()
  }

  fn head(&mut self, row: HeadRow) {
    self.remember_head(row);
  }
}

impl ComponentLowerer<'_, '_> {
  pub(super) fn slot(&self, name: &str) -> Tmpl {
    if self.layout_root {
      let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
      Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![Tmpl::Slot(name.to_owned())] }
    } else {
      Tmpl::Slot(name.to_owned())
    }
  }

  /// A named slot with the markup to show while the plan leaves it unfilled:
  /// the region holds the segment when `$props.$slots` names the slot and
  /// the fallback otherwise.
  pub(super) fn slot_with_fallback(&self, name: &str, fallback: Vec<Tmpl>) -> Tmpl {
    let filled = Expr::Builtin {
      name: Builtin::Includes,
      args: vec![Expr::Coalesce(Box::new(Expr::Var("$props".to_owned()).field("$slots")), Box::new(Expr::Array(Vec::new()))), Expr::lit_str(name)],
    };
    let inner = Tmpl::If { cond: filled, then: Box::new(Tmpl::Slot(name.to_owned())), r#else: Some(Box::new(Tmpl::Fragment(fallback))) };
    let attrs = if name == "content" { Vec::new() } else { vec![Entry::Field("data-sf-name".to_owned(), Expr::lit_str(name))] };
    Tmpl::Element { tag: "sf-s".to_owned(), attrs, children: vec![inner] }
  }

  /// True when `attrs` spread the props, or their rest, which carry the caller's `children`.
  pub(super) fn spreads_props(&self, attrs: &[js::JSXAttributeItem]) -> bool {
    attrs.iter().any(|attr| match attr {
      js::JSXAttributeItem::SpreadAttribute(spread) => match &spread.argument {
        js::Expression::Identifier(id) => [&self.props_name, &self.props_rest].iter().any(|name| name.as_deref() == Some(id.name.as_str())),
        _ => false,
      },
      js::JSXAttributeItem::Attribute(_) => false,
    })
  }

  /// The slot an expression stands for: `children` or `props.children` is
  /// `content`; in a layout, a prop named after one of its slots is that slot.
  pub(super) fn slot_of_expr(&self, expr: &js::Expression) -> Option<String> {
    match expr {
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        if self.children_name.as_deref() == Some(name) {
          return Some("content".to_owned());
        }
        self.slot_props.iter().find(|(local, _)| local == name).map(|(_, slot)| slot.clone())
      }
      m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
        let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(m) else { return None };
        if self.class.is_some() && prop.name.as_str() == "children" && is_this_props(m.object()) {
          return Some("content".to_owned());
        }
        let js::Expression::Identifier(obj) = m.object() else { return None };
        if self.props_name.as_deref() != Some(obj.name.as_str()) {
          return None;
        }
        let prop = prop.name.as_str();
        if prop == "children" {
          return Some("content".to_owned());
        }
        (self.layout_root && self.slot_names.iter().any(|n| n == prop)).then(|| prop.to_owned())
      }
      _ => None,
    }
  }
}

/// `this.properties`, which a class component's `render()` reads its props through.
pub(super) fn is_this_props(expr: &js::Expression) -> bool {
  (expr).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::ThisExpression(_)) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "props"))
}

/// The hook a call names when its callee is a bare identifier.
/// The hook `expr` calls: a library's by the library's own name, however the
/// file reaches it, and an application's by the name the file calls it. `None`
/// for a call that is no hook, a local that shadows a hook's name among them.
/// A call to one of a library's own hooks, by what its callee means rather than its name: a hook an application or FSR's Ink module declares under the same name is not one.
pub(super) fn library_hook<'a>(meanings: &crate::meaning::Meanings, expr: &'a js::Expression<'a>) -> Option<(&'static str, &'a js::CallExpression<'a>)> {
  let js::Expression::CallExpression(call) = expr else { return None };
  let callee = &call.callee;
  meanings.of_expr(callee).and_then(|meant| meant.hook()).map(|hook| (hook, &**call))
}

pub(super) fn hook_call<'a>(meanings: &crate::meaning::Meanings, expr: &'a js::Expression<'a>) -> Option<(&'a str, &'a js::CallExpression<'a>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(expr) else { return None };
  let callee = &call.callee;
  let meant = meanings.of_expr(callee);
  if let Some(hook) = meant.as_ref().and_then(|meant| meant.hook()) {
    return Some((hook, call));
  }
  let declared_hook = meant.as_ref().and_then(|meant| meant.origin()).is_some_and(|(_, name)| name.starts_with("use"));
  let name = match callee {
    js::Expression::Identifier(id) if matches!(meanings.ctxt(id), crate::Ctxt::Top | crate::Ctxt::Unresolved) => id.name.as_str(),
    member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
      let js::Expression::Identifier(ns) = member.object() else { return None };
      let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) else { return None };
      if !matches!(meanings.of_ident(ns), Some(crate::meaning::Meaning::Namespace(_))) {
        return None;
      }
      name.name.as_str()
    }
    _ => return None,
  };
  (declared_hook || name.starts_with("use")).then_some((name, call))
}
