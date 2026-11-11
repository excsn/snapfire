use super::*;

/// The name of a parameter typed `Ctx["services"]`, with or without `Ctx`'s type arguments.
pub(crate) fn services_param(param: Param<'_>) -> Option<String> {
  let binding = param.ident()?;
  let js::TSType::TSIndexedAccessType(indexed) = &param.type_annotation?.type_annotation else { return None };
  let js::TSType::TSTypeReference(object) = &indexed.object_type else { return None };
  let js::TSTypeName::IdentifierReference(ctx) = &object.type_name else { return None };
  let js::TSType::TSLiteralType(index) = &indexed.index_type else { return None };
  let js::TSLiteral::StringLiteral(key) = &index.literal else { return None };
  (ctx.name.as_str() == "Ctx" && key.value == "services").then(|| binding.name.to_string())
}

/// A regular expression literal as the value the regex builtins read, refused here when the interpreter could not match it as JavaScript does.
pub(crate) fn regex_value(lowerer: &Lowerer<'_>, pattern: &str, flags: &str, span: Span) -> Lowered<Expr> {
  if let Err(why) = snapfire_fsr_ir::jsregex::compiled(pattern, flags) {
    return Err(lowerer.residue_with(span, format!("a regular expression with {why}"), "write it without that construct; the server refuses a pattern that can backtrack without bound and a flag its engine lacks"));
  }
  Ok(Expr::Object(vec![
    Entry::Field(snapfire_fsr_ir::jsregex::PATTERN.to_owned(), Expr::lit_str(pattern)),
    Entry::Field(snapfire_fsr_ir::jsregex::FLAGS.to_owned(), Expr::lit_str(flags)),
    Entry::Field(snapfire_fsr_ir::jsregex::LAST_INDEX.to_owned(), Expr::Lit(Lit::Float(0.0))),
  ]))
}

/// A string as the source of a regular expression that matches it literally.
pub(crate) fn escape_regex(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for c in text.chars() {
    if "\\^$.*+?()[]{}|/-".contains(c) {
      out.push('\\');
    }
    out.push(c);
  }
  out
}

/// `e` with each optional member link made plain, the object of each link pushed to `guards`, outermost first.
pub(crate) fn strip_optional<'a>(b: syntax::Build<'a>, e: &js::Expression<'_>, guards: &mut Vec<js::Expression<'a>>) -> js::Expression<'a> {
  match e {
    js::Expression::ChainExpression(chain) => match chain.expression.as_member_expression() {
      Some(m) => strip_member(b, m, guards),
      None => b.copy(e),
    },
    js::Expression::ParenthesizedExpression(p) => strip_optional(b, &p.expression, guards),
    other => match other.as_member_expression() {
      Some(m) => strip_member(b, m, guards),
      None => chained(b, other),
    },
  }
}

pub(crate) fn strip_member<'a>(b: syntax::Build<'a>, m: &js::MemberExpression<'_>, guards: &mut Vec<js::Expression<'a>>) -> js::Expression<'a> {
  let obj = strip_optional(b, m.object(), guards);
  if m.optional() {
    guards.push(b.copy(&obj));
  }
  let ast = b.ast();
  match m {
    js::MemberExpression::StaticMemberExpression(s) => js::Expression::new_static_member_expression(s.span, obj, b.copy(&s.property), false, &ast),
    js::MemberExpression::ComputedMemberExpression(c) => js::Expression::new_computed_member_expression(c.span, obj, b.copy(&c.expression), false, &ast),
    js::MemberExpression::PrivateFieldExpression(p) => js::Expression::new_private_field_expression(p.span, obj, b.copy(&p.field), false, &ast),
  }
}

/// A copy of `e`, which sits inside an optional chain, wrapped in a chain of its own when it holds an optional link, so it reads as the chain it was part of.
pub(crate) fn chained<'a>(b: syntax::Build<'a>, e: &js::Expression<'_>) -> js::Expression<'a> {
  fn optional(e: &js::Expression) -> bool {
    match e {
      js::Expression::CallExpression(call) => call.optional || optional(&call.callee),
      other => other.as_member_expression().is_some_and(|m| m.optional() || optional(m.object())),
    }
  }
  if !optional(e) {
    return b.copy(e);
  }
  let span = e.span();
  let element = match b.copy(e) {
    js::Expression::CallExpression(call) => js::ChainElement::CallExpression(call),
    js::Expression::StaticMemberExpression(m) => js::ChainElement::StaticMemberExpression(m),
    js::Expression::ComputedMemberExpression(m) => js::ChainElement::ComputedMemberExpression(m),
    js::Expression::PrivateFieldExpression(m) => js::ChainElement::PrivateFieldExpression(m),
    other => return other,
  };
  js::Expression::new_chain_expression(span, element, &b.ast())
}

/// The kind `e` makes in place: a `new Map(...)`, `new Set(...)`, `new URLSearchParams(...)`, `new URL(...)` or `new Date(...)`, a bigint literal or `BigInt(...)`.
pub(crate) fn made_kind(e: &js::Expression) -> Option<Kind> {
  match e {
    js::Expression::ParenthesizedExpression(p) => made_kind(&p.expression),
    js::Expression::TSAsExpression(a) => made_kind(&a.expression),
    js::Expression::TSNonNullExpression(a) => made_kind(&a.expression),
    js::Expression::NewExpression(n) if intl_member(&n.callee) == Some("Locale") => Some(Kind::Locale),
    js::Expression::NewExpression(n) => match &n.callee {
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Map" | "SvelteMap") => Some(Kind::Map),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Set" | "SvelteSet") => Some(Kind::Set),
      js::Expression::Identifier(id) if id.name.as_str() == "WeakMap" => Some(Kind::WeakMap),
      js::Expression::Identifier(id) if id.name.as_str() == "WeakSet" => Some(Kind::WeakSet),
      js::Expression::Identifier(id) if id.name.as_str() == "WeakRef" => Some(Kind::WeakRef),
      js::Expression::Identifier(id) if id.name.as_str() == "FinalizationRegistry" => Some(Kind::Registry),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Number" | "Boolean" | "String") => Some(Kind::Wrapper),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "ArrayBuffer" | "SharedArrayBuffer") => Some(Kind::Buffer),
      js::Expression::Identifier(id) if id.name.as_str() == "DataView" => Some(Kind::View),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "DisposableStack" | "AsyncDisposableStack") => Some(Kind::Stack),
      js::Expression::Identifier(id) if id.name.as_str() == "Proxy" => Some(Kind::Proxy),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "URLSearchParams" | "SvelteURLSearchParams") => Some(Kind::Query),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "URL" | "SvelteURL") => Some(Kind::Url),
      js::Expression::Identifier(id) if matches!(id.name.as_str(), "Date" | "SvelteDate") => Some(Kind::Date),
      js::Expression::Identifier(id) if id.name.as_str() == "Response" => Some(Kind::Response),
      js::Expression::Identifier(id) if id.name.as_str() == "Scope" => Some(Kind::Scope),
      callee if crate::temporal_type(callee).is_some() => Some(Kind::Temporal),
      _ => None,
    },
    js::Expression::BigIntLiteral(_) => Some(Kind::BigInt),
    js::Expression::CallExpression(call) => match &call.callee {
      js::Expression::Identifier(id) if id.name.as_str() == "BigInt" => Some(Kind::BigInt),
      js::Expression::StaticMemberExpression(m) if matches!(&m.object, js::Expression::Identifier(obj) if obj.name.as_str() == "Response") && matches!(m.property.name.as_str(), "json" | "redirect") => Some(Kind::Response),
      js::Expression::StaticMemberExpression(m) if crate::temporal_type(&m.object).is_some() && crate::TEMPORAL_FROM.contains(&m.property.name.as_str()) => Some(Kind::Temporal),
      js::Expression::StaticMemberExpression(m) if crate::is_temporal_now(&m.object) && crate::TEMPORAL_NOW.contains(&m.property.name.as_str()) => Some(Kind::Temporal),
      _ => None,
    },
    _ => None,
  }
}

/// Whether `f` is a lambda whose block body writes a name it neither takes nor declares: a helper writing a module-level `let`.
pub(crate) fn writes_outer(f: &Expr) -> bool {
  !outer_set_targets(f).is_empty()
}

/// The names a lambda sets that it neither takes nor declares, module-level `let`s.
pub(crate) fn outer_set_targets(f: &Expr) -> Vec<String> {
  let Expr::Lambda { params, body } = f else { return Vec::new() };
  let Expr::Block(body) = &**body else { return Vec::new() };
  let mut declared: Vec<String> = Vec::new();
  fn lets(body: &[Stmt], out: &mut Vec<String>) {
    for stmt in body {
      match stmt {
        Stmt::Let { name, .. } => out.push(name.clone()),
        Stmt::If { then, r#else, .. } => {
          lets(then, out);
          lets(r#else, out);
        }
        Stmt::ForOf { name, body, .. } => {
          out.push(name.clone());
          lets(body, out);
        }
        _ => {}
      }
    }
  }
  lets(body, &mut declared);
  snapfire_fsr_ir::ast::set_targets(body).into_iter().filter(|name| !params.contains(name) && !declared.contains(name)).collect()
}

/// The prop a component binding reads, `$props.key` or that with a default, so a kind read through it is a kind relied on.
pub(crate) fn prop_key(bound: &Expr) -> Option<String> {
  match bound {
    Expr::Field(target, key) if **target == Expr::Var("$props".to_owned()) => Some(key.clone()),
    Expr::Coalesce(read, _) => prop_key(read),
    _ => None,
  }
}

/// Whether `body` writes or extends the session anywhere in it.
pub(crate) fn writes_session(body: &[Stmt]) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } => true,
    Stmt::If { then, r#else, .. } => writes_session(then) || writes_session(r#else),
    Stmt::ForOf { body, .. } => writes_session(body),
    Stmt::While { body, step, .. } => writes_session(body) || writes_session(step),
    Stmt::Try { body, handler, finally, .. } => writes_session(body) || handler.as_deref().is_some_and(writes_session) || writes_session(finally),
    Stmt::Let { .. } | Stmt::Set { .. } | Stmt::Return(_) | Stmt::Break | Stmt::Continue | Stmt::Guard { .. } | Stmt::Act { .. } | Stmt::Expr(_) => false,
  })
}

pub(crate) fn strip_parens<'e, 'a>(e: &'e js::Expression<'a>) -> &'e js::Expression<'a> {
  match e {
    js::Expression::ParenthesizedExpression(p) => strip_parens(&p.expression),
    other => other,
  }
}

/// The text of a template element, cooked where it has a cooked value.
pub(crate) fn quasi_text(quasi: &js::TemplateElement) -> String {
  quasi.value.cooked.map(lossy).unwrap_or_else(|| quasi.value.raw.to_string())
}

/// The text of a string literal or of a template literal with no `${}` part.
pub(crate) fn string_literal(e: &js::Expression) -> Option<String> {
  match crate::meaning::bare(e) {
    js::Expression::StringLiteral(s) => Some(lossy(s.value)),
    js::Expression::TemplateLiteral(tpl) if tpl.expressions.is_empty() && tpl.quasis.len() == 1 => Some(quasi_text(&tpl.quasis[0])),
    _ => None,
  }
}

/// Which platform constructor `n` calls whose methods the build lowers: the `Intl` formatters `NumberFormat`, `DateTimeFormat`, `ListFormat`, `Segmenter`, `PluralRules`, `Collator` and `RelativeTimeFormat`, then `TextEncoder` and `TextDecoder`.
pub(crate) fn platform_object(n: &js::NewExpression) -> Option<&'static str> {
  if let js::Expression::Identifier(id) = &n.callee {
    return match id.name.as_str() {
      "TextEncoder" => Some("TextEncoder"),
      "TextDecoder" => Some("TextDecoder"),
      _ => None,
    };
  }
  let js::Expression::StaticMemberExpression(m) = &n.callee else { return None };
  let js::Expression::Identifier(ns) = &m.object else { return None };
  if ns.name.as_str() != "Intl" {
    return None;
  }
  match m.property.name.as_str() {
    "NumberFormat" => Some("NumberFormat"),
    "DateTimeFormat" => Some("DateTimeFormat"),
    "ListFormat" => Some("ListFormat"),
    "Segmenter" => Some("Segmenter"),
    "PluralRules" => Some("PluralRules"),
    "Collator" => Some("Collator"),
    "RelativeTimeFormat" => Some("RelativeTimeFormat"),
    "DurationFormat" => Some("DurationFormat"),
    _ => None,
  }
}

/// `a`, `a and b` or `a, b and c`.
pub(crate) fn list_and(items: &[String]) -> String {
  match items {
    [] => String::new(),
    [one] => one.clone(),
    [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
  }
}

/// The member of `Intl` `callee` names, `Locale` in `Intl.Locale`.
pub(crate) fn intl_member<'e>(callee: &'e js::Expression) -> Option<&'e str> {
  let js::Expression::StaticMemberExpression(m) = callee else { return None };
  matches!(&m.object, js::Expression::Identifier(id) if id.name.as_str() == "Intl").then(|| m.property.name.as_str())
}
