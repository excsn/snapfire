use super::*;

/// The expression an attribute's value holds when it is `{expr}`.
pub(super) fn attr_expression<'p>(attr: &'p js::JSXAttribute<'p>) -> Option<&'p js::Expression<'p>> {
  match &attr.value {
    Some(js::JSXAttributeValue::ExpressionContainer(c)) => c.expression.as_expression(),
    _ => None,
  }
}

pub(super) fn attr_name(name: &js::JSXAttributeName) -> String {
  match name {
    js::JSXAttributeName::Identifier(id) => id.name.to_string(),
    js::JSXAttributeName::NamespacedName(n) => format!("{}:{}", n.namespace.name, n.name.name),
  }
}

pub(super) fn is_handler_name(name: &str) -> bool {
  name.len() > 2 && name.starts_with("on") && name.as_bytes()[2].is_ascii_uppercase()
}

/// The markup a `dangerouslySetInnerHTML` value carries, taken from the object
/// when it is written inline and read as a field otherwise.
pub(super) fn inner_html(value: Expr) -> Expr {
  if let Expr::Object(entries) = &value {
    if let [Entry::Field(name, html)] = entries.as_slice() {
      if name == "__html" {
        return html.clone();
      }
    }
  }
  Expr::Field(Box::new(value), "__html".to_owned())
}

/// JSX's whitespace rule: lines are trimmed, blank lines dropped, the rest
/// joined by one space; a single line keeps its inner spacing.
pub(super) fn jsx_text(raw: &str) -> String {
  let decoded = decode_entities(raw);
  if !decoded.contains('\n') {
    return decoded;
  }
  let lines: Vec<&str> = decoded.lines().collect();
  let last = lines.len().saturating_sub(1);
  let mut parts = Vec::new();
  for (i, line) in lines.iter().enumerate() {
    let mut piece: &str = line;
    if i > 0 {
      piece = piece.trim_start();
    }
    if i < last {
      piece = piece.trim_end();
    }
    if !piece.is_empty() {
      parts.push(piece);
    }
  }
  parts.join(" ")
}

pub(super) fn decode_entities(raw: &str) -> String {
  if !raw.contains('&') {
    return raw.to_owned();
  }
  let mut out = String::with_capacity(raw.len());
  let mut rest = raw;
  while let Some(start) = rest.find('&') {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    let decoded = rest.find(';').filter(|&end| end <= 10).and_then(|end| {
      let entity = &rest[1..end];
      let entity = match entity.strip_prefix("#X") {
        Some(hex) => format!("#x{hex}"),
        None => entity.to_owned(),
      };
      oxc_syntax::xml_entities::decode_entity(&entity).map(|c| (c.to_char().unwrap_or(char::REPLACEMENT_CHARACTER), end))
    });
    match decoded {
      Some((c, end)) => {
        out.push(c);
        rest = &rest[end + 1..];
      }
      None => {
        out.push('&');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out
}

/// `xs?.map(f)` with `f` an arrow written in place: the array and the callback.
pub(super) fn optional_map<'e>(expr: &'e js::Expression<'e>) -> Option<(&'e js::Expression<'e>, &'e js::ArrowFunctionExpression<'e>)> {
  let js::Expression::ChainExpression(chain) = expr else { return None };
  let js::ChainElement::CallExpression(call) = &chain.expression else { return None };
  let js::Expression::StaticMemberExpression(member) = &call.callee else { return None };
  if !member.optional || member.property.name != "map" {
    return None;
  }
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  Some((&member.object, arrow))
}

/// `xs.map(f)` with `f` an arrow written in place: the array and the callback.
pub(super) fn map_call<'e>(call: &'e js::CallExpression<'e>) -> Option<(&'e js::Expression<'e>, &'e js::ArrowFunctionExpression<'e>)> {
  let callee = &call.callee;
  let Some(member) = (callee).as_member_expression() else { return None };
  if !matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(id) if id.name.as_str() == "map") {
    return None;
  }
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  Some((member.object(), arrow))
}

/// The name a function declaration statement declares.
pub(super) fn function_name<'s>(stmt: &'s js::Statement<'_>) -> Option<&'s str> {
  match stmt {
    js::Statement::FunctionDeclaration(f) => f.id.as_ref().map(|id| id.name.as_str()),
    _ => None,
  }
}

/// The expressions an array literal lists, a spread's argument among them and a hole left out.
pub(crate) fn array_items<'s, 'a>(arr: &'s js::ArrayExpression<'a>) -> impl Iterator<Item = &'s js::Expression<'a>> {
  arr.elements.iter().filter_map(|elem| match elem {
    js::ArrayExpressionElement::Elision(_) => None,
    js::ArrayExpressionElement::SpreadElement(s) => Some(&s.argument),
    other => Some(other.to_expression()),
  })
}

/// Binds a parameter to `target`, its default applying when `target` is null or absent.
pub(crate) fn bind_param(lowerer: &mut Lowerer<'_>, param: &crate::Param, target: Expr) -> Lowered<()> {
  if param.rest {
    return Err(lowerer.residue(param.span, "a pattern the build does not read"));
  }
  let target = match param.default {
    Some(default) => Expr::Coalesce(Box::new(target), Box::new(lowerer.expr(default)?)),
    None => target,
  };
  bind_pattern(lowerer, param.pattern, target)
}
