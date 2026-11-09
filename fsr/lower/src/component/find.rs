use super::*;

impl<'p> From<(Vec<crate::Param<'p>>, FunctionBody<'p>)> for HandlerFn<'p> {
  fn from((params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>)) -> Self {
    Self { params, body, hook: None }
  }
}

/// The alias `name` is in `parsed`, when it is one: `island` must come from
/// the client's React package and the options are string literals.
pub(super) fn island_alias_of(parsed: &Parsed, meanings: &crate::meaning::Meanings, name: &str) -> Result<Option<IslandAlias>, (Span, String)> {
  let Some(Global::Const(js::Expression::CallExpression(call))) = find_value(parsed, name) else { return Ok(None) };
  let callee = &call.callee;
  if meanings.of_expr(callee).and_then(|meant| meant.placement()) != Some("island") {
    return Ok(None);
  }
  let Some(js::Expression::Identifier(target)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) else {
    return Err((call.span, "`island(...)` takes a component name first".to_owned()));
  };
  let mut when = None;
  let mut mode = None;
  if let Some(options) = call.arguments.get(1) {
    let js::Expression::ObjectExpression(obj) = crate::syntax::arg_expr(options) else { return Err((options.span(), "`island(...)` options must be an object literal".to_owned())) };
    for prop in &obj.properties {
      let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err((options.span(), "a spread in `island(...)` options".to_owned())) };
      let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err((options.span(), "`island(...)` options are `when` and `mode`".to_owned())) };
      let value = match &kv.value {
        js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
        _ => None,
      };
      match prop_name(&kv.key, kv.computed).as_deref() {
        Some("when") => match value.as_deref() {
          Some("load" | "visible" | "idle") => when = value,
          _ => return Err((options.span(), "an island's `when` is \"load\", \"visible\" or \"idle\", written out".to_owned())),
        },
        Some("mode") => match value.as_deref() {
          Some("browser") => mode = None,
          Some("server") => mode = value,
          _ => return Err((options.span(), "an island's `mode` is \"browser\" or \"server\", written out".to_owned())),
        },
        _ => return Err((options.span(), "`island(...)` options are `when` and `mode`".to_owned())),
      }
    }
  }
  Ok(Some(IslandAlias { target: target.name.to_string(), when, mode }))
}

/// The action id of `actions.<path>(input)`, when `actions` is the object the
/// build generates: the property path is the id, `$root.blip` for
/// `actions.$root.blip`. The generated module re-exports the client library's
/// `action` under another name, so there is nothing here to follow by import.
/// Whether `source`, imported from `file`, is the client module the build generates.
pub(super) fn is_generated_client(file: &str, source: &str) -> bool {
  crate::resolve_specifier(file, source).is_some_and(|joined| {
    let stem = joined.rsplit_once('.').filter(|(_, ext)| matches!(*ext, "ts" | "js")).map(|(stem, _)| stem).unwrap_or(&joined);
    stem.ends_with("generated/client")
  })
}

pub(crate) fn generated_action_of(parsed: &Parsed, callee: &js::Expression) -> Option<String> {
  let mut path: Vec<String> = Vec::new();
  let mut cursor = callee;
  loop {
    match cursor {
      member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
        let crate::syntax::MemberProp::Ident(name) = crate::syntax::prop(member) else { return None };
        path.push(name.name.to_string());
        cursor = member.object();
      }
      js::Expression::Identifier(root) => {
        if parsed.meanings.get()?.of_ident(root) != Some(&crate::meaning::Meaning::Library(crate::meaning::Construct::GeneratedActions)) {
          return None;
        }
        path.reverse();
        return (!path.is_empty()).then(|| path.join("."));
      }
      _ => return None,
    }
  }
}

/// `const save = action("desk.save")` at module scope, `action` imported from
/// the client library, when `name` is such a `save`: the action's id.
pub(crate) fn action_alias_of(parsed: &Parsed, name: &str) -> Option<String> {
  let Some(Global::Const(js::Expression::CallExpression(call))) = find_value(parsed, name) else { return None };
  let callee = &call.callee;
  if parsed.meanings.get().and_then(|meanings| meanings.of_expr(callee)) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::ClientAction)) {
    return None;
  }
  match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
    Some(js::Expression::StringLiteral(id)) => Some(crate::lossy(id.value)),
    _ => None,
  }
}

/// `Date.now` named rather than called, as `useState(Date.now)` passes it to be called once.
pub(super) fn is_date_now(member: &js::MemberExpression) -> bool {
  matches!((member.object(), crate::syntax::prop(member)), (js::Expression::Identifier(date), crate::syntax::MemberProp::Ident(now)) if date.name == "Date" && now.name == "now")
}

pub(crate) fn arrow_body<'a>(arrow: &'a js::ArrowFunctionExpression<'a>) -> FunctionBody<'a> {
  match crate::syntax::arrow_body_of(arrow) {
    crate::syntax::ArrowBody::Block(b) => FunctionBody::Block(b.body),
    crate::syntax::ArrowBody::Expr(e) => FunctionBody::Expr(e),
  }
}

/// The exported or declared function named `export`; `default` is the
/// default export.
/// The name `export default name;` exports.
pub(super) fn default_ident<'a>(parsed: &'a Parsed<'a>) -> Option<&'a js::IdentifierReference<'a>> {
  parsed.module().body.iter().find_map(|item| match crate::syntax::top(item) {
    crate::syntax::Top::DefaultExpr(js::Expression::Identifier(id)) => Some(&**id),
    _ => None,
  })
}

/// `export { orig as export } from "source"`: the source, the name it exports there and the specifier's span.
pub(super) fn reexported(parsed: &Parsed, export: &str) -> Option<(String, String, Span)> {
  parsed.module().body.iter().find_map(|item| match item {
    js::Statement::ExportFromDeclaration(e) => {
      let source = crate::lossy(e.source.value);
      e.specifiers.iter().find_map(|n| (crate::schema::export_name(&n.exported) == export).then(|| (source.clone(), crate::schema::export_name(&n.local), n.span)))
    }
    _ => None,
  })
}

pub(crate) fn find_function<'a>(parsed: &'a Parsed<'a>, export: &str) -> Option<Found<'a>> {
  for item in &parsed.module().body {
    match crate::syntax::top(item) {
      crate::syntax::Top::DefaultFunction(f) if export == "default" => {
        let body = f.body.as_ref()?;
        return Some(Found::Declared(&f.params, &body.statements, body.span));
      }
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match e {
        js::Expression::ArrowFunctionExpression(arrow) => return Some(Found::Arrow(arrow)),
        js::Expression::Identifier(id) => return find_function(parsed, id.name.as_str()),
        js::Expression::CallExpression(call) => return unwrapped(parsed, call),
        _ => {}
      },
      crate::syntax::Top::Decl(decl) => {
        if let Some(found) = decl_function(parsed, decl, export) {
          return Some(found);
        }
      }
      _ => {}
    }
  }
  None
}

/// The React export `expr` names: `memo` imported from `react` under any name, or `React.memo` through its default or namespace import.
pub(super) fn react_export(parsed: &Parsed, expr: &js::Expression) -> Option<String> {
  let imports = parsed.module().body.iter().filter_map(|item| match item {
    js::Statement::ImportDeclaration(import) if crate::lossy(import.source.value) == "react" => Some(import),
    _ => None,
  });
  match crate::meaning::bare(expr) {
    js::Expression::Identifier(id) => imports.flat_map(|import| import.specifiers.iter().flatten()).find_map(|spec| match spec {
      js::ImportDeclarationSpecifier::ImportSpecifier(named) if named.local.name == id.name => Some(crate::schema::export_name(&named.imported)),
      _ => None,
    }),
    js::Expression::StaticMemberExpression(member) => {
      let js::Expression::Identifier(object) = crate::meaning::bare(&member.object) else { return None };
      let mut imports = imports;
      imports
        .any(|import| import.specifiers.iter().flatten().any(|spec| matches!(spec, js::ImportDeclarationSpecifier::ImportDefaultSpecifier(d) if d.local.name == object.name) || matches!(spec, js::ImportDeclarationSpecifier::ImportNamespaceSpecifier(n) if n.local.name == object.name)))
        .then(|| member.property.name.to_string())
    }
    _ => None,
  }
}

/// The function `memo(f)` or `forwardRef(f)` wraps, through any nesting of the two.
pub(super) fn unwrapped<'a>(parsed: &'a Parsed<'a>, call: &'a js::CallExpression<'a>) -> Option<Found<'a>> {
  if !matches!(react_export(parsed, &call.callee).as_deref(), Some("memo" | "forwardRef")) {
    return None;
  }
  match crate::meaning::bare(crate::syntax::arg_expr(call.arguments.first()?)) {
    js::Expression::ArrowFunctionExpression(arrow) => Some(Found::Arrow(arrow)),
    js::Expression::FunctionExpression(f) => {
      let body = f.body.as_ref()?;
      Some(Found::Declared(&f.params, &body.statements, body.span))
    }
    js::Expression::CallExpression(inner) => unwrapped(parsed, inner),
    js::Expression::Identifier(id) => find_function(parsed, id.name.as_str()),
    _ => None,
  }
}

/// Whether `export` is wrapped in `forwardRef`, whose function takes the ref as its second parameter.
pub(super) fn forwards_ref(parsed: &Parsed, export: &str) -> bool {
  fn wraps(parsed: &Parsed, expr: &js::Expression) -> bool {
    let js::Expression::CallExpression(call) = crate::meaning::bare(expr) else { return false };
    match react_export(parsed, &call.callee).as_deref() {
      Some("forwardRef") => true,
      Some("memo") => call.arguments.first().is_some_and(|arg| wraps(parsed, crate::syntax::arg_expr(arg))),
      _ => false,
    }
  }
  parsed.module().body.iter().any(|item| {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultExpr(e) if export == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => forwards_ref(parsed, id.name.as_str()),
          other => wraps(parsed, other),
        };
      }
      crate::syntax::Top::Decl(decl) => decl,
      _ => return false,
    };
    let js::Declaration::VariableDeclaration(var) = decl else { return false };
    var.declarations.iter().any(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == export) && d.init.as_ref().is_some_and(|init| wraps(parsed, init)))
  })
}

/// Whether `class` extends React's `Component` or `PureComponent`.
pub(super) fn extends_react(parsed: &Parsed, class: &js::Class) -> bool {
  class.heritage.as_ref().is_some_and(|base| matches!(react_export(parsed, &base.expression).as_deref(), Some("Component" | "PureComponent")))
}

/// `export` when it is a class extending React's `Component` or `PureComponent` with a `render()` method, and that method's body.
pub(super) fn find_class<'a>(parsed: &'a Parsed<'a>, export: &str) -> Option<(&'a js::Class<'a>, Found<'a>)> {
  let render = |class: &'a js::Class<'a>| {
    class.body.body.iter().find_map(|member| match member {
      js::ClassElement::MethodDefinition(m) if !m.r#static && prop_name(&m.key, m.computed).as_deref() == Some("render") => {
        let body = m.value.body.as_ref()?;
        Some((class, Found::Declared(&m.value.params, &body.statements, body.span)))
      }
      _ => None,
    })
  };
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultClass(c) if export == "default" => match extends_react(parsed, c) {
        true => return render(c),
        false => continue,
      },
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match crate::meaning::bare(e) {
        js::Expression::Identifier(id) => return find_class(parsed, id.name.as_str()),
        js::Expression::ClassExpression(c) if extends_react(parsed, c) => return render(c),
        _ => continue,
      },
      crate::syntax::Top::Decl(decl) => decl,
      _ => continue,
    };
    if let js::Declaration::ClassDeclaration(c) = decl {
      if c.id.as_ref().is_some_and(|id| id.name.as_str() == export) && extends_react(parsed, c) {
        return render(c);
      }
    }
  }
  None
}

/// The span of `export` when it is a class extending React's `Component` or `PureComponent`.
pub(super) fn class_component_span(parsed: &Parsed, export: &str) -> Option<Span> {
  for item in &parsed.module().body {
    let decl = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultClass(c) if export == "default" => match extends_react(parsed, c) {
        true => return Some(c.span),
        false => continue,
      },
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match crate::meaning::bare(e) {
        js::Expression::Identifier(id) => return class_component_span(parsed, id.name.as_str()),
        js::Expression::ClassExpression(c) if extends_react(parsed, c) => return Some(c.span),
        _ => continue,
      },
      crate::syntax::Top::Decl(decl) => decl,
      _ => continue,
    };
    if let js::Declaration::ClassDeclaration(c) = decl {
      if c.id.as_ref().is_some_and(|id| id.name.as_str() == export) && extends_react(parsed, c) {
        return Some(c.span);
      }
    }
  }
  None
}

/// Where an export the build cannot read as a function is made and what made
/// it: a call another library wraps a component in (`observer(...)`) or a
/// tagged template (`styled.h1`).
pub(super) fn made_component(parsed: &Parsed, export: &str) -> Option<(Span, String)> {
  let made = |e: &js::Expression| match crate::meaning::bare(e) {
    js::Expression::CallExpression(call) => {
      let name = match crate::meaning::bare(&call.callee) {
        js::Expression::Identifier(id) => id.name.to_string(),
        js::Expression::StaticMemberExpression(m) => match &m.object {
          js::Expression::Identifier(obj) => format!("{}.{}", obj.name, m.property.name),
          _ => return None,
        },
        _ => return None,
      };
      Some((call.span, format!("a component made by `{name}`, which the build does not lower")))
    }
    js::Expression::TaggedTemplateExpression(t) => Some((t.span, "a component made by a tagged template, which the build does not lower".to_owned())),
    _ => None,
  };
  for item in &parsed.module().body {
    let var = match crate::syntax::top(item) {
      crate::syntax::Top::DefaultExpr(e) if export == "default" => {
        return match crate::meaning::bare(e) {
          js::Expression::Identifier(id) => made_component(parsed, id.name.as_str()),
          other => made(other),
        };
      }
      crate::syntax::Top::Decl(js::Declaration::VariableDeclaration(var)) => var,
      _ => continue,
    };
    for decl in &var.declarations {
      match (&decl.id, &decl.init) {
        (js::BindingPattern::BindingIdentifier(name), Some(init)) if name.name.as_str() == export => return made(init),
        (pattern @ (js::BindingPattern::ObjectPattern(_) | js::BindingPattern::ArrayPattern(_)), Some(init)) if binds(pattern, export) => return made(init),
        _ => {}
      }
    }
  }
  None
}

/// Whether a destructuring pattern declares `name`.
pub(super) fn binds(pattern: &js::BindingPattern, name: &str) -> bool {
  let mut names = Vec::new();
  crate::pattern_names(pattern, &mut names);
  names.iter().any(|id| id.name.as_str() == name)
}

/// The span of `export`'s function when it is `async`, found the way [`find_function`] finds it.
pub(crate) fn async_function_span(parsed: &Parsed, export: &str) -> Option<Span> {
  for item in &parsed.module().body {
    match crate::syntax::top(item) {
      crate::syntax::Top::DefaultFunction(f) if export == "default" => return f.r#async.then_some(f.span),
      crate::syntax::Top::DefaultExpr(e) if export == "default" => match e {
        js::Expression::ArrowFunctionExpression(arrow) => return arrow.r#async.then_some(arrow.span),
        js::Expression::Identifier(id) => return async_function_span(parsed, id.name.as_str()),
        _ => {}
      },
      crate::syntax::Top::Decl(decl) => {
        if let Some(span) = decl_async_span(decl, export) {
          return Some(span);
        }
      }
      _ => {}
    }
  }
  None
}

pub(super) fn decl_async_span(decl: &js::Declaration, name: &str) -> Option<Span> {
  match decl {
    js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => f.r#async.then_some(f.span),
    js::Declaration::VariableDeclaration(var) => {
      let decl = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name))?;
      match decl.init.as_ref()? {
        js::Expression::ArrowFunctionExpression(arrow) => arrow.r#async.then_some(arrow.span),
        _ => None,
      }
    }
    _ => None,
  }
}

/// The specifier a `lazy(() => import("x"))` or `defineAsyncComponent(() => import("x"))` loads, from an arrow whose body is the import or returns it.
pub(super) fn lazy_import_source(call: &js::CallExpression) -> Option<String> {
  let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(call.arguments.first()?) else { return None };
  let expr: &js::Expression = match crate::syntax::arrow_body_of(arrow) {
    crate::syntax::ArrowBody::Block(body) => match body.body {
      [js::Statement::ReturnStatement(ret)] => ret.argument.as_ref()?,
      _ => return None,
    },
    crate::syntax::ArrowBody::Expr(e) => e,
  };
  let js::Expression::ImportExpression(import) = crate::meaning::bare(expr) else { return None };
  match &import.source {
    js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
    _ => None,
  }
}

/// The span of `export default tree(...)` with `tree` from FSR's React
/// module or its dialect, a spelling the build refuses.
pub(super) fn tree_call(parsed: &Parsed, meanings: &crate::meaning::Meanings) -> Option<Span> {
  parsed.module().body.iter().find_map(|item| {
    let crate::syntax::Top::DefaultExpr(js::Expression::CallExpression(call)) = crate::syntax::top(item) else { return None };
    (meanings.of_expr(&call.callee).and_then(|meant| meant.placement()) == Some("tree")).then_some(call.span)
  })
}

pub(super) fn decl_function<'a>(parsed: &'a Parsed<'a>, decl: &'a js::Declaration<'a>, name: &str) -> Option<Found<'a>> {
  match decl {
    js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => {
      let body = f.body.as_ref()?;
      Some(Found::Declared(&f.params, &body.statements, body.span))
    }
    js::Declaration::VariableDeclaration(var) => {
      let decl = var.declarations.iter().find(|d| matches!(&d.id, js::BindingPattern::BindingIdentifier(id) if id.name.as_str() == name))?;
      match decl.init.as_ref()? {
        js::Expression::ArrowFunctionExpression(arrow) => Some(Found::Arrow(arrow)),
        js::Expression::CallExpression(call) => unwrapped(parsed, call),
        _ => None,
      }
    }
    _ => None,
  }
}

/// A module-level value: a `const` with an initialiser or a function.
/// The `type X<…> = …` a module item declares, exported or not.
pub(super) fn type_alias_decl<'e>(item: &'e js::Statement<'e>) -> Option<&'e js::TSTypeAliasDeclaration<'e>> {
  match crate::syntax::top(item) {
    crate::syntax::Top::Decl(js::Declaration::TSTypeAliasDeclaration(a)) => Some(a),
    _ => None,
  }
}

/// The return type a module-level function `name` declares: a `function` or a `const` holding an arrow or a function expression.
pub(super) fn declared_return<'a>(module: &'a js::Program<'a>, name: &str) -> Option<&'a js::TSType<'a>> {
  module.body.iter().find_map(|item| {
    let crate::syntax::Top::Decl(decl) = crate::syntax::top(item) else { return None };
    match decl {
      js::Declaration::FunctionDeclaration(f) if f.id.as_ref().is_some_and(|id| id.name.as_str() == name) => f.return_type.as_deref().map(|t| &t.type_annotation),
      js::Declaration::VariableDeclaration(v) => v.declarations.iter().find_map(|d| match (&d.id, d.init.as_ref().map(crate::strip_parens)) {
        (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::ArrowFunctionExpression(a))) if id.name.as_str() == name => a.return_type.as_deref().map(|t| &t.type_annotation),
        (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::FunctionExpression(f))) if id.name.as_str() == name => f.return_type.as_deref().map(|t| &t.type_annotation),
        _ => None,
      }),
      _ => None,
    }
  })
}
