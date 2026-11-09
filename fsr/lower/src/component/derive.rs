use super::*;

/// How many nodes an expression is.
pub(super) fn weight(expr: &Expr) -> usize {
  let mut n = 0;
  expr.visit(&mut |_| n += 1);
  n
}

pub(super) fn lower_derive(parsed: &Parsed, defaults: &SessionDefaults, resolved: &Resolved, call: &js::CallExpression, nested: bool) -> Result<LoweredDerive, DeriveFailure> {
  let mut lowerer = Lowerer::new(parsed, defaults).resolved(resolved);
  let refused = |lowerer: &mut Lowerer<'_>, residue: Residue, compute: Option<(String, Vec<String>)>| DeriveFailure { residue, unbound: lowerer.unbound.take(), compute };
  let ([k, s, c], false) = (&call.arguments[..], call.arguments.iter().any(|a| crate::syntax::is_spread(a))) else {
    return Err(refused(&mut lowerer, parsed.residue(call.span, "`derive` takes a key, its sources and a compute"), None));
  };
  let key = match lowerer.expr(crate::syntax::arg_expr(k)) {
    Ok(Expr::Lit(Lit::Str(key))) => key,
    Ok(_) => return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(k).span(), "a derived key the build cannot read; name it with `key()` or a string"), None)),
    Err(residue) => return Err(refused(&mut lowerer, residue, None)),
  };
  let js::Expression::ArrayExpression(list) = crate::syntax::arg_expr(s) else {
    return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(s).span(), "a derive's sources the build cannot read; list them in the call"), None));
  };
  let mut sources = Vec::new();
  for element in &list.elements {
    let Some(element) = element.as_expression() else {
      return Err(refused(&mut lowerer, parsed.residue(list.span, "a derive's sources the build cannot read; list them in the call"), None));
    };
    match lowerer.expr(element) {
      Ok(Expr::Lit(Lit::Str(source))) => sources.push(source),
      Ok(_) => return Err(refused(&mut lowerer, parsed.residue(element.span(), "a derive source the build cannot read; name it with `key()` or a string"), None)),
      Err(residue) => return Err(refused(&mut lowerer, residue, None)),
    }
  }
  let known = Some((key.clone(), sources.clone()));
  if nested {
    return Ok(LoweredDerive { key, sources, body: None, at: None, why: Some("written inside a function, so it runs only when the browser calls that function".to_owned()) });
  }
  let (params, stmts): (Vec<crate::Param>, FunctionBody<'_>) = match crate::syntax::arg_expr(c) {
    js::Expression::ArrowFunctionExpression(arrow) if !arrow.r#async => (crate::params_of(&arrow.params), arrow_body(arrow)),
    js::Expression::FunctionExpression(f) if !f.r#async && !f.generator && f.body.is_some() => (crate::params_of(&f.params), FunctionBody::Block(&f.body.as_ref().unwrap().statements)),
    other => return Err(refused(&mut lowerer, parsed.residue(other.span(), "a compute that is not a function written in the call"), known)),
  };
  lowerer.render_path = true;
  match &params[..] {
    [] => {}
    [param] if param.ident().is_some() => {
      if let crate::Ctxt::Local(read) = parsed.binding_ctxt(param.ident().expect("checked")) {
        lowerer.reader = Some(read);
      }
    }
    _ => return Err(refused(&mut lowerer, parsed.residue(crate::syntax::arg_expr(&c).span(), "a compute whose parameters are not one `read`"), known)),
  }
  let body = match stmts {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(b) => block_to_expr(&mut lowerer, b),
  };
  match body {
    Ok(body) => Ok(LoweredDerive { key, sources, body: Some(body), at: None, why: None }),
    Err(residue) => Err(refused(&mut lowerer, residue, known)),
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for DeriveCalls<'_, '_> {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.meanings.of_expr(&call.callee) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Derive)) {
      self.calls.push(self.build.copy(call));
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for StoreCalls<'_, '_> {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.meanings.of_expr(&call.callee).and_then(|meant| meant.hook()) == Some("useStore") {
      if let Some(first) = call.arguments.first() {
        self.args.push(self.build.copy(crate::syntax::arg_expr(first)));
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

impl ThemeExport {
  pub fn new(name: String, force: bool) -> Self {
    Self { name, force }
  }
}

pub(super) fn unwrap_literal<'e>(expr: &'e js::Expression<'e>) -> &'e js::Expression<'e> {
  match expr {
    js::Expression::ParenthesizedExpression(inner) => unwrap_literal(&inner.expression),
    js::Expression::TSAsExpression(inner) => unwrap_literal(&inner.expression),
    js::Expression::TSSatisfiesExpression(inner) => unwrap_literal(&inner.expression),
    other => other,
  }
}
