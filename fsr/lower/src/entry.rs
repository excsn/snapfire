use super::*;

pub(crate) fn heap_pass() -> bool {
  HEAP_PASS.get()
}

pub(crate) fn want_heap() {
  HEAP_WANTED.set(true);
}

/// `run`, then `run` again with the heap when the first pass wrote an object or could not lower the body.
pub(crate) fn with_heap<T, E: std::fmt::Debug>(mut run: impl FnMut() -> Result<T, E>, mark: impl FnOnce(&mut T)) -> Result<T, E> {
  if HEAP_ACTIVE.get() {
    return run();
  }
  with_heap_fresh(run, mark)
}

/// [`with_heap`] for a program of its own inside another's lowering.
pub(crate) fn with_heap_fresh<T, E: std::fmt::Debug>(mut run: impl FnMut() -> Result<T, E>, mark: impl FnOnce(&mut T)) -> Result<T, E> {
  let (outer_pass, outer_wanted, outer_active) = (HEAP_PASS.replace(false), HEAP_WANTED.replace(false), HEAP_ACTIVE.replace(true));
  let first = run();
  let out = if first.is_ok() && !HEAP_WANTED.get() {
    first
  } else {
    let first_ok = first.is_ok();
    HEAP_PASS.set(true);
    let second = run();
    HEAP_PASS.set(false);
    match second {
      Ok(mut value) => {
        mark(&mut value);
        Ok(value)
      }
      Err(error) if first_ok => Err(error),
      Err(second) => {
        if std::env::var("FSR_HEAP_LOG").is_ok() {
          eprintln!("heap pass refused too: {second:?}");
        }
        first
      }
    }
  };
  HEAP_PASS.set(outer_pass);
  HEAP_WANTED.set(outer_wanted);
  HEAP_ACTIVE.set(outer_active);
  out
}

pub(crate) fn heap_marker() -> Stmt {
  Stmt::Let { name: "$heap".to_owned(), expr: Expr::Builtin { name: Builtin::HeapOn, args: Vec::new() } }
}

pub(crate) fn lower_loader_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  with_heap(|| lower_loader_once(parsed, defaults, resolved), |body: &mut Body| body.insert(0, heap_marker()))
}

pub(crate) fn lower_loader_once(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let Some(function) = parsed.exports().find_map(|(name, decl)| (name == "load").then_some(decl)) else {
    if parsed.exports().any(|(name, _)| ["meta", "store", "defaults", "paths"].contains(&name)) {
      return Ok(vec![Stmt::Return(Expr::Object(Vec::new()))]);
    }
    return Err((LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None));
  };
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err((LowerError::MissingExport { file: file.to_owned(), export: "load".to_owned() }, None)),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`load` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`load` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`load` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

/// An export whose input is the loader's data, bound as `data`.
pub(crate) fn lower_of_data(file: &str, source: &str, defaults: &SessionDefaults, export: &str) -> Result<Option<Body>, LowerError> {
  component::ComponentSet::new(std::path::Path::new("")).with_defaults(defaults.clone()).provide(file, source).lower_of_data(file, export)
}

pub(crate) fn lower_of_data_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, export: &str) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, export, true)
}

pub(crate) fn lower_paths_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Option<Body>, Unresolved> {
  lower_optional_export_in(parsed, defaults, resolved, "paths", false)
}

/// A card image module's `images`, or `generateImageMetadata` as Next names
/// it: a function of the context returning the image list.
pub(crate) fn lower_images_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Option<Body>, Unresolved> {
  match lower_optional_export_in(parsed, defaults, resolved, "images", false)? {
    Some(body) => Ok(Some(body)),
    None => lower_optional_export_in(parsed, defaults, resolved, "generateImageMetadata", false),
  }
}

/// An optional exported function of one parameter: the loader's data as
/// `data` when `of_data`, the context otherwise.
pub(crate) fn lower_optional_export_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, export: &str, of_data: bool) -> Result<Option<Body>, Unresolved> {
  let Some(exported) = parsed.exports().find_map(|(name, decl)| (name == export).then_some(decl)) else { return Ok(None) };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.meta = of_data;
  let takes = if of_data { "`{ data }`" } else { "the context" };
  let result = match exported {
    Exported::Function(first, body) => lowerer.bind_ctx(first).and_then(|()| lowerer.block(body)),
    Exported::Expr(first, expr) => lowerer.bind_ctx(first).and_then(|()| lowerer.expr(expr)).map(|e| vec![Stmt::Return(e)]),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`{export}` is {what}")).into(), None)),
    Exported::Value(value) if of_data && export == "meta" => lowerer.expr(value).map(|e| vec![Stmt::Return(e)]),
    Exported::Action { .. } | Exported::BadAction(_) | Exported::Value(_) | Exported::Other(_) => return Err((parsed.residue(parsed.module().span, format!("`{export}` must be a function of {takes}")).into(), None)),
  };
  result.map(Some).map_err(|r| (r.into(), lowerer.unbound.take()))
}

pub(crate) fn lower_actions_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Vec<LoweredAction>, Unresolved> {
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    let (input, first, body) = match exported {
      Exported::Action { input, first, body } => (input, first, body),
      Exported::BadAction(span) => return Err((parsed.residue(span, format!("`{name}` is an `action(...)` of something other than a function")).into(), None)),
      Exported::Unreadable(span, what) => {
        let hint = "every export of an actions module is an `action(...)`; the build passes over a type or a plain value and refuses what it cannot read";
        return Err((Residue { hint: Some(hint.to_owned()), ..parsed.residue(span, format!("`{name}` is {what}")) }.into(), None));
      }
      _ => continue,
    };
    let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
    lowerer.extends = true;
    lowerer.redirects = true;
    let body = with_heap(|| { let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved); lowerer.extends = true; lowerer.redirects = true; lower_function(&mut lowerer, first, body.clone()).map_err(|r| -> Unresolved { (r.into(), lowerer.unbound.take()) }) }, |body: &mut Body| body.insert(0, heap_marker()))?;
    let _ = &mut lowerer;
    out.push(LoweredAction { export: name.to_owned(), input, body });
  }
  Ok(out)
}

pub(crate) fn lower_handlers_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Vec<LoweredHandler>, Unresolved> {
  let mut out = Vec::new();
  for (name, exported) in parsed.exports() {
    if !HANDLER_METHODS.contains(&name) {
      continue;
    }
    let (input, first, body) = match exported {
      Exported::Function(first, body) => (None, first, FunctionBody::Block(body)),
      Exported::Action { input, first, body } => (input, first, body),
      Exported::Expr(first, e) => (None, first, FunctionBody::Expr(e)),
      Exported::BadAction(span) => return Err((parsed.residue(span, format!("`{name}` is an `action(...)` of something other than a function")).into(), None)),
      Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`{name}` is {what}")).into(), None)),
      Exported::Value(value) => return Err((parsed.residue(value.span(), format!("`{name}` must be a function or an `action(...)`")).into(), None)),
      Exported::Other(span) => return Err((parsed.residue(span, format!("`{name}` must be a function or an `action(...)`")).into(), None)),
    };
    let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
    lowerer.extends = true;
    lowerer.redirects = true;
    let body = lower_function(&mut lowerer, first, body).map_err(|r| (r.into(), lowerer.unbound.take()))?;
    out.push(LoweredHandler { method: name.to_owned(), input, body });
  }
  Ok(out)
}

/// `export function theme(ctx)` of `theme.ts`: the document's theme for one request, with
/// `ctx.theme` the route's own as the plan names it, read as the body's input.
pub(crate) fn lower_theme_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let missing = || (LowerError::MissingExport { file: file.to_owned(), export: "theme".to_owned() }, None);
  let function = parsed.exports().find_map(|(name, decl)| (name == "theme").then_some(decl)).ok_or_else(missing)?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err(missing()),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`theme` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`theme` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`theme` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.theming = true;
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

pub(crate) fn lower_middleware_in(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved) -> Result<Body, Unresolved> {
  let file = &parsed.file;
  let function = parsed
    .exports()
    .find_map(|(name, decl)| (name == "middleware").then_some(decl))
    .ok_or_else(|| (LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() }, None))?;
  let (first, body) = match function {
    Exported::Function(first, body) => (first, FunctionBody::Block(body)),
    Exported::Expr(first, e) => (first, FunctionBody::Expr(e)),
    Exported::Action { .. } => return Err((LowerError::MissingExport { file: file.to_owned(), export: "middleware".to_owned() }, None)),
    Exported::Unreadable(span, what) => return Err((parsed.residue(span, format!("`middleware` is {what}")).into(), None)),
    Exported::Value(value) => return Err((parsed.residue(value.span(), "`middleware` must be a function").into(), None)),
    Exported::Other(span) | Exported::BadAction(span) => return Err((parsed.residue(span, "`middleware` must be a function").into(), None)),
  };
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  lowerer.middleware = true;
  lowerer.extends = true;
  let result = lower_function(&mut lowerer, first, body);
  result.map_err(|r| (r.into(), lowerer.unbound.take()))
}

pub(crate) fn classify_in<'a>(parsed: &'a Parsed<'a>, init: &'a js::Expression<'a>) -> Exported<'a> {
  match init {
    js::Expression::ParenthesizedExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::TSAsExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::TSSatisfiesExpression(inner) => classify_in(parsed, &inner.expression),
    js::Expression::FunctionExpression(f) => Exported::Function(first_param(&f.params), body_of(f)),
    js::Expression::ArrowFunctionExpression(arrow) => match arrow_body(arrow) {
      FunctionBody::Block(stmts) => Exported::Function(first_param(&arrow.params), stmts),
      FunctionBody::Expr(e) => Exported::Expr(first_param(&arrow.params), e),
    },
    js::Expression::CallExpression(call) => {
      if !parsed.names_server(&call.callee, "action") {
        return Exported::Unreadable(call.span, "a call the build does not recognise");
      }
      match call.arguments.last() {
        Some(js::Argument::ArrowFunctionExpression(arrow)) => {
          let first = first_param(&arrow.params);
          Exported::Action { input: action_input(call, first), first, body: arrow_body(arrow) }
        }
        Some(js::Argument::FunctionExpression(f)) => {
          let first = first_param(&f.params);
          Exported::Action { input: action_input(call, first), first, body: FunctionBody::Block(body_of(f)) }
        }
        Some(other) => Exported::BadAction(other.span()),
        None => Exported::BadAction(call.span),
      }
    }
    js::Expression::ObjectExpression(_) => Exported::Value(init),
    other => Exported::Other(other.span()),
  }
}

/// The input type of an `action(...)`: the call's type argument, else what
/// the function's first parameter names through `ActionCtx<T>`.
pub(crate) fn action_input(call: &js::CallExpression, first: Option<Param<'_>>) -> Option<String> {
  call.type_arguments.as_ref().and_then(|t| t.params.first()).and_then(type_ref_name).or_else(|| first.and_then(action_ctx_input))
}

/// Binds the context parameter and lowers a function body: a block as it
/// stands, an expression as the value returned.
pub(crate) fn lower_function<'p>(lowerer: &mut Lowerer<'p>, first: Option<Param<'p>>, body: FunctionBody<'p>) -> Lowered<Body> {
  lowerer.bind_ctx(first)?;
  match body {
    FunctionBody::Block(stmts) => lowerer.block(stmts),
    FunctionBody::Expr(expr) => lowerer.expr(expr).map(|e| vec![Stmt::Return(e)]),
  }
}

pub(crate) fn is_bitwise(op: js::BinaryOperator) -> bool {
  matches!(op, js::BinaryOperator::BitwiseAnd | js::BinaryOperator::BitwiseOR | js::BinaryOperator::BitwiseXOR | js::BinaryOperator::ShiftLeft | js::BinaryOperator::ShiftRight | js::BinaryOperator::ShiftRightZeroFill)
}

/// The name a type reference writes, `T` for `T` or `T<U>`.
pub(crate) fn type_ref_name(ty: &js::TSType) -> Option<String> {
  match ty {
    js::TSType::TSTypeReference(r) => match &r.type_name {
      js::TSTypeName::IdentifierReference(id) => Some(id.name.to_string()),
      _ => None,
    },
    _ => None,
  }
}

/// The input type an action's parameter names: `ActionCtx<AddToCart>` on
/// `ctx` or on the destructuring of it.
pub(crate) fn action_ctx_input(param: Param<'_>) -> Option<String> {
  if param.rest || param.default.is_some() || !matches!(param.pattern, js::BindingPattern::BindingIdentifier(_) | js::BindingPattern::ObjectPattern(_)) {
    return None;
  }
  let ann = param.type_annotation?;
  let js::TSType::TSTypeReference(r) = &ann.type_annotation else { return None };
  match &r.type_name {
    js::TSTypeName::IdentifierReference(id) if id.name.as_str() == "ActionCtx" => {}
    _ => return None,
  }
  r.type_arguments.as_ref().and_then(|p| p.params.first()).and_then(type_ref_name)
}
