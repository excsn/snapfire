use crate::*;

impl<'a> Lowerer<'a> {
  /// `a?.b.c(x)`: the call as written, `null` where an optional link's object is null or undefined.
  pub(crate) fn optional_call(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let b = self.build();
    let mut guards = Vec::new();
    let callee = strip_optional(b, &call.callee, &mut guards);
    // A call on a value known to be null short-circuits, as JavaScript does, whatever the method.
    for guard in &guards {
      if let js::Expression::Identifier(id) = strip_parens(guard) {
        if matches!(self.scope.iter().rev().find(|(n, _)| n == id.name.as_str()), Some((_, Expr::Lit(Lit::Null)))) {
          return Ok(Expr::Lit(Lit::Null));
        }
      }
    }
    let plain = js::CallExpression::new(call.span, callee, None, b.copy(&call.arguments), false, &b.ast());
    let mut out = self.call(&plain)?;
    for guard in guards.iter().rev() {
      let object = self.expr(guard)?;
      let missing = Expr::Builtin { name: Builtin::IsNullish, args: vec![object] };
      out = Expr::Ternary(Box::new(missing), Box::new(Expr::Lit(Lit::Undefined)), Box::new(out));
    }
    Ok(out)
  }

  /// A `function` expression or a method as the lambda it is; a generator is residue.
  /// `scope.get()`, `scope.run(value, fn, ...args)` and `scope.set(value)` on a `Scope` a module holds as a cell: the cell read; the cell set to `value` around a call of `fn` with the value before put back after, the call's result answered; the cell set. A web page has no process to scope, so there the storage is residue.
  pub(crate) fn scope_call(&mut self, method: &str, member: &js::MemberExpression, call: &js::CallExpression) -> Lowered<Expr> {
    let js::Expression::Identifier(id) = crate::meaning::bare(member.object()) else { return Err(self.residue(member.span(), "a `Scope` reached other than by its name")) };
    let name = id.name.to_string();
    let Some((_, handle)) = self.cells.iter().find(|(cell, _)| *cell == name).cloned() else {
      return Err(self.residue(member.span(), format!("`{name}`, a `Scope`, outside a terminal page; a web page has no process to scope")));
    };
    match (method, call.arguments.as_slice()) {
      ("get", []) => Ok(Expr::ReadCell(Box::new(handle))),
      ("set", [value]) => Ok(Expr::WriteCell(Box::new(handle), Box::new(self.expr(syntax::arg_expr(value))?))),
      ("run", [value, f, rest @ ..]) => {
        let value = self.expr(syntax::arg_expr(value))?;
        let f = Box::new(self.expr(syntax::arg_expr(f))?);
        let mut args = Vec::with_capacity(rest.len());
        for a in rest {
          args.push(self.expr(syntax::arg_expr(a))?);
        }
        let (before, result) = (self.temp(), self.temp());
        Ok(Expr::Block(vec![
          Stmt::Let { name: before.clone(), expr: Expr::ReadCell(Box::new(handle.clone())) },
          Stmt::Expr(Expr::WriteCell(Box::new(handle.clone()), Box::new(value))),
          Stmt::Let { name: result.clone(), expr: Expr::Apply { f, args } },
          Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(Expr::Var(before)))),
          Stmt::Return(Expr::Var(result)),
        ]))
      }
      _ => Err(self.residue(call.span, format!("`.{method}()` of a `Scope`; `get`, `run` and `set` are read"))),
    }
  }

  /// `object.name(args)` or `object[key](args)` where `object` lowers to an object literal whose field (every field, for a computed key) is a function value: that field applied to the arguments. A computed key on an object of no known kind is applied as well, since the build cannot name the method. `None` for any other member.
  pub(crate) fn function_member(&mut self, member: &js::MemberExpression, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    // Only a name or a chain of fields is looked at: lowering a call here and again on the method path would count its hoists twice.
    let mut at = crate::meaning::bare(member.object());
    while let Some(inner) = at.as_member_expression() {
      at = crate::meaning::bare(inner.object());
    }
    if !matches!(at, js::Expression::Identifier(_)) {
      return Ok(None);
    }
    let depth = self.scope.len();
    let object = self.expr(member.object());
    self.scope.truncate(depth);
    let Ok(object) = object else { return Ok(None) };
    if let syntax::MemberProp::Ident(name) = syntax::prop(member) {
      if self.holds_method(member.object(), name.name.as_str()) {
        return self.applied(Box::new(object.field(name.name.to_string())), call).map(Some);
      }
    }
    let Expr::Object(entries) = &object else {
      // A computed key on a value of no known kind: whatever the key names is applied, as JavaScript applies it.
      if let (syntax::MemberProp::Computed(key), None) = (syntax::prop(member), self.kind_of(member.object())) {
        let key = self.expr(key)?;
        let callee = Expr::Index(Box::new(object.clone()), Box::new(key));
        return self.applied(Box::new(callee), call).map(Some);
      }
      return Ok(None);
    };
    let is_function = |e: &Expr| matches!(e, Expr::Lambda { .. } | Expr::Fix { .. });
    let callee = match syntax::prop(member) {
      syntax::MemberProp::Ident(name) => {
        let name = name.name.to_string();
        if !entries.iter().any(|entry| matches!(entry, Entry::Field(field, value) if *field == name && is_function(value))) {
          return Ok(None);
        }
        object.clone().field(name)
      }
      syntax::MemberProp::Computed(key) => {
        if entries.is_empty() || !entries.iter().all(|entry| matches!(entry, Entry::Field(_, value) if is_function(value))) {
          return Ok(None);
        }
        let key = self.expr(key)?;
        Expr::Index(Box::new(object.clone()), Box::new(key))
      }
      syntax::MemberProp::Private(_) => return Ok(None),
    };
    self.applied(Box::new(callee), call).map(Some)
  }

  /// `f` applied to `call`'s arguments: [`Expr::Apply`], or [`Expr::ApplyOf`] over one array when an argument is spread.
  pub(crate) fn applied(&mut self, f: Box<Expr>, call: &js::CallExpression) -> Lowered<Expr> {
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      let spread = matches!(a, js::Argument::SpreadElement(_));
      let value = syntax::arg_expr(a);
      // A helper's `Ctx["services"]` parameter is the services root inside it, so its slot carries nothing.
      let arg = if !spread && self.is_services_root(value) { Expr::Lit(Lit::Null) } else { self.expr(value)? };
      args.push((spread, arg));
    }
    if args.iter().all(|(spread, _)| !spread) {
      return Ok(Expr::Apply { f, args: args.into_iter().map(|(_, e)| e).collect() });
    }
    let entries = args.into_iter().map(|(spread, e)| if spread { Entry::Spread(e) } else { Entry::Item(e) }).collect();
    Ok(Expr::ApplyOf(f, Box::new(Expr::Array(entries))))
  }

  pub(crate) fn function_expr(&mut self, f: &js::Function) -> Lowered<Expr> {
    if f.generator {
      return Err(self.residue(f.span, "a generator function"));
    }
    let depth = self.scope.len();
    let outer = std::mem::take(&mut self.breakable);
    let lowered = self.function_body(f, depth);
    self.breakable = outer;
    lowered
  }

  pub(crate) fn function_body(&mut self, f: &js::Function, depth: usize) -> Lowered<Expr> {
    let pats = params_of(&f.params);
    let params = match crate::component::bind_params(self, &pats) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let roots = self.bind_services_params(&pats, depth);
    let body = match &f.body {
      Some(body) => crate::component::block_to_expr(self, &body.statements),
      None => Ok(Expr::Lit(Lit::Null)),
    };
    self.roots.truncate(roots);
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(body?) })
  }

  /// Each parameter typed `Ctx["services"]` leaves the scope and becomes the services root, so `services.<name>.<method>(...)` inside the function is the same static call a loader makes. Returns the roots' depth to restore.
  pub(crate) fn bind_services_params(&mut self, pats: &[Param<'_>], depth: usize) -> usize {
    let roots = self.roots.len();
    for name in pats.iter().copied().filter_map(services_param) {
      let mut at = depth;
      while at < self.scope.len() {
        if self.scope[at].0 == name {
          self.scope.remove(at);
        } else {
          at += 1;
        }
      }
      self.roots.push((name, Root::Services));
    }
    roots
  }

  /// `services` or `ctx.services` naming the root rather than a local.
  pub(crate) fn is_services_root(&self, e: &js::Expression) -> bool {
    let e = crate::meaning::bare(e);
    if let js::Expression::Identifier(id) = e {
      return !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.root_of(id) == Some(Root::Services);
    }
    match e.as_member_expression() {
      Some(m) => match m.object() {
        js::Expression::Identifier(id) => !self.scope.iter().any(|(n, _)| n == id.name.as_str()) && self.root_of(id) == Some(Root::Ctx) && self.member_name(m).as_deref() == Some("services"),
        _ => false,
      },
      None => false,
    }
  }

  pub(crate) fn lambda(&mut self, arrow: &js::ArrowFunctionExpression) -> Lowered<Expr> {
    let depth = self.scope.len();
    let pats = params_of(&arrow.params);
    let params = match crate::component::bind_params(self, &pats) {
      Ok(params) => params,
      Err(residue) => {
        self.scope.truncate(depth);
        return Err(residue);
      }
    };
    let roots = self.bind_services_params(&pats, depth);
    let defaults = crate::component::held_frame(self, |this| crate::component::settle(this, depth)).0;
    let body = match arrow_body(arrow) {
      FunctionBody::Expr(e) => self.expr(e),
      FunctionBody::Block(stmts) if !writes_a_member_of(stmts, &params) => crate::component::block_to_expr_from(self, stmts, depth),
      FunctionBody::Block(stmts) => {
        let written = crate::component::written_params(&pats, stmts);
        crate::component::packing_returns(self, stmts, &written).and_then(|packed| crate::component::block_to_expr_from(self, &packed, depth)).map(|packed| crate::component::write_back(packed, &written))
      }
    };
    self.roots.truncate(roots);
    self.scope.truncate(depth);
    Ok(Expr::Lambda { params, body: Box::new(crate::component::sequenced(defaults, body?)) })
  }
}
