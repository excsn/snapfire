use crate::*;

impl<'a> Lowerer<'a> {
  pub(crate) fn stmt(&mut self, stmt: &js::Statement) -> Lowered<Stmt> {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        if var.declarations.len() != 1 {
          return Err(self.residue(var.span, "one binding per declaration"));
        }
        self.declare(&var.declarations[0])
      }
      js::Statement::IfStatement(if_stmt) => {
        if let Some((kind, message)) = self.as_fail(&if_stmt.consequent) {
          if if_stmt.alternate.is_some() {
            return Err(self.residue(if_stmt.span, "an `else` after `fail`"));
          }
          let cond = self.expr(&if_stmt.test)?;
          let (kind, message) = (kind?, message?);
          return Ok(Stmt::Guard { cond, kind, message });
        }
        let cond = self.expr(&if_stmt.test)?;
        let then = self.branch(&if_stmt.consequent)?;
        let r#else = match &if_stmt.alternate {
          Some(alt) => self.branch(alt)?,
          None => Vec::new(),
        };
        Ok(Stmt::If { cond, then, r#else })
      }
      js::Statement::ForOfStatement(for_of) => {
        let js::ForStatementLeft::VariableDeclaration(decl) = &for_of.left else {
          return Err(self.residue(for_of.span, "`for...of` must declare its variable"));
        };
        let Some(pattern) = decl.declarations.first().map(|d| &d.id) else {
          return Err(self.residue(decl.span, "`for...of` must declare its variable"));
        };
        let mut over = self.expr(&for_of.right)?;
        if self.kind_of(&for_of.right) == Some(Kind::Map) {
          over = Expr::Entries(Box::new(over));
        }
        let depth = self.scope.len();
        let name = match pattern {
          js::BindingPattern::BindingIdentifier(name) => {
            let name = name.name.to_string();
            self.scope.push((name.clone(), Expr::Var(name.clone())));
            name
          }
          pattern => {
            let name = self.temp();
            crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
            name
          }
        };
        self.breakable.push(Breakable::Loop);
        let body = self.branch(&for_of.body);
        self.breakable.pop();
        self.scope.truncate(depth);
        Ok(Stmt::ForOf { name, over, body: body? })
      }
      js::Statement::ReturnStatement(ret) if self.markup_base.is_some() => {
        let base = self.markup_base.expect("checked");
        let path = self.markup_returns.len();
        let mut fields = vec![Entry::Field("$path".to_owned(), Expr::Lit(Lit::Int(path as i128)))];
        fields.extend(self.scope[base..].iter().enumerate().map(|(k, (_, expr))| Entry::Field(format!("${k}"), expr.clone())));
        let names = self.scope[base..].iter().map(|(name, _)| name.clone()).collect();
        self.markup_returns.push((crate::lo(ret.span), names));
        Ok(Stmt::Return(Expr::Object(fields)))
      }
      js::Statement::ReturnStatement(ret) => match &ret.argument {
        Some(arg) => Ok(Stmt::Return(self.expr(arg)?)),
        None => Ok(Stmt::Return(Expr::Lit(Lit::Null))),
      },
      js::Statement::ExpressionStatement(expr_stmt) => self.effect(&expr_stmt.expression),
      js::Statement::ThrowStatement(thrown) => Ok(Stmt::Expr(crate::component::throw_expr(self, thrown)?)),
      js::Statement::TryStatement(t) => self.try_stmt(t),
      js::Statement::BreakStatement(b) if b.label.is_none() && self.breakable.last() == Some(&Breakable::Loop) => Ok(Stmt::Break),
      js::Statement::ContinueStatement(c) if c.label.is_none() && self.breakable.contains(&Breakable::Loop) => Ok(Stmt::Continue),
      js::Statement::BlockStatement(block) => Err(self.residue(block.span, "a bare block")),
      other => {
        let (what, hint) = describe_stmt(other);
        Err(self.residue_with(other.span(), what, hint))
      }
    }
  }

  /// `redirect(to)`: the response that sends the caller to `to`, which ends the body where it is a statement. An action or a route handler holds one.
  /// `after(fn)`: the function, written in place with no parameters, to run once the response is written. Its body lowers as a body does; it cannot write the session, whose cookie is already sent.
  pub(crate) fn after_of(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let body = match call.arguments.as_slice() {
      [work] if !crate::syntax::is_spread(work) => match strip_parens(crate::syntax::arg_expr(work)) {
        js::Expression::ArrowFunctionExpression(arrow) if crate::params_of(&arrow.params).is_empty() => match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(block) => self.block(&block.body)?,
          crate::syntax::ArrowBody::Expr(e) => vec![Stmt::Return(self.expr(e)?)],
        },
        js::Expression::FunctionExpression(f) if crate::params_of(&f.params).is_empty() => match &f.body {
          Some(block) => self.block(&block.statements)?,
          None => Vec::new(),
        },
        _ => return Err(self.residue(call.span, "`after` takes a function written in place, with no parameters")),
      },
      _ => return Err(self.residue(call.span, "`after` takes a function written in place, with no parameters")),
    };
    if writes_session(&body) {
      return Err(self.residue(call.span, "a session write inside `after`, which runs once the response and its cookie are sent"));
    }
    Ok(Expr::After(Box::new(Expr::Lambda { params: Vec::new(), body: Box::new(Expr::Block(body)) })))
  }

  pub(crate) fn redirect_to(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    if !self.redirects {
      return Err(self.residue_with(
        call.span,
        "`redirect` outside an action or a route handler",
        "middleware returns `Response.redirect(to)`; a loader cannot send the visitor elsewhere",
      ));
    }
    let [to] = call.arguments.as_slice() else {
      return Err(self.residue(call.span, "`redirect` takes the path to send the caller to"));
    };
    Ok(Expr::Builtin { name: Builtin::RedirectTo, args: vec![self.expr(crate::syntax::arg_expr(to))?] })
  }

  pub(crate) fn try_stmt(&mut self, t: &js::TryStatement) -> Lowered<Stmt> {
    let body = self.block(&t.block.body)?;
    let (name, handler) = match &t.handler {
      None => (None, None),
      Some(catch) => {
        let depth = self.scope.len();
        let name = crate::component::catch_param(self, catch.param.as_ref());
        let handler = name.and_then(|name| Ok((name, self.block(&catch.body.body)?)));
        self.scope.truncate(depth);
        let (name, handler) = handler?;
        (name, Some(handler))
      }
    };
    let finally = match &t.finalizer {
      Some(finalizer) => self.block(&finalizer.body)?,
      None => Vec::new(),
    };
    Ok(Stmt::Try { body, name, handler, finally })
  }

  pub(crate) fn branch(&mut self, stmt: &js::Statement) -> Lowered<Body> {
    match stmt {
      js::Statement::BlockStatement(block) => self.block(&block.body),
      single => self.block(std::slice::from_ref(single)),
    }
  }

  /// `fail("kind", "message")` as a statement, bare or in a one-statement block.
  pub(crate) fn as_fail(&mut self, stmt: &js::Statement) -> Option<(Lowered<String>, Lowered<Expr>)> {
    let inner = match stmt {
      js::Statement::BlockStatement(b) if b.body.len() == 1 => &b.body[0],
      other => other,
    };
    let js::Statement::ExpressionStatement(e) = inner else { return None };
    let js::Expression::CallExpression(call) = &e.expression else { return None };
    if !self.parsed.names_server(&call.callee, "fail") {
      return None;
    }
    let kind = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
      Some(js::Expression::StringLiteral(s)) => Ok(crate::lossy(s.value)),
      Some(other) => Err(self.residue(other.span(), "`fail` takes its kind as a string literal, since the kind is matched at build time")),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    let message = match call.arguments.get(1) {
      Some(a) => self.expr(crate::syntax::arg_expr(a)),
      None => Err(self.residue(call.span, "`fail` takes a kind and a message")),
    };
    Some((kind, message))
  }

  /// A statement that is an expression: a session write, a delete, a bare
  /// `fail` or an awaited call for its effect.
  pub(crate) fn effect(&mut self, expr: &js::Expression) -> Lowered<Stmt> {
    if self.object_write(expr) {
      if heap_pass() {
        return Ok(Stmt::Expr(self.heap_write(expr)?));
      }
      want_heap();
    }
    // A `console` call in a helper's block logs on the server as one in a component's body does.
    if let js::Expression::CallExpression(call) = expr {
      if let Some((response, op, args)) = self.response_headers_call(call) {
        if matches!(op.as_str(), "set" | "append" | "delete") {
          let js::Expression::Identifier(local) = crate::meaning::bare(response) else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let Some((_, Expr::Var(name))) = self.scope.iter().rev().find(|(n, _)| n == local.name.as_str()).cloned() else { return Err(self.residue(call.span, "a header written on a `Response` that is not a local")) };
          let wanted = if op == "delete" { 1 } else { 2 };
          if args.len() != wanted {
            return Err(self.residue(call.span, format!("`headers.{op}` takes {wanted} arguments")));
          }
          let mut lowered = vec![Expr::Var(name.clone()), Expr::lit_str(&op)];
          for arg in args {
            lowered.push(self.expr(crate::syntax::arg_expr(arg))?);
          }
          return Ok(Stmt::Set { name, expr: Expr::Builtin { name: Builtin::ResponseHeaderSet, args: lowered } });
        }
      }
      if let Some(logged) = crate::component::console_call(self, call)? {
        return Ok(Stmt::Expr(logged));
      }
      if let Some(act) = self.settling(call)? {
        return Ok(act);
      }
      if let Some(listen) = self.listening(call)? {
        return Ok(listen);
      }
      if let Some(arrow) = self.async_iife(call) {
        let ran = match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => self.async_stmts(&b.body)?,
          crate::syntax::ArrowBody::Expr(e) => {
            let b = self.build();
            self.async_stmts(&[b.expr_stmt(b.copy(e))])?
          }
        };
        return Ok(Stmt::Expr(ran));
      }
    }
    match expr {
      js::Expression::AssignmentExpression(assign) if self.cell_of_target(&assign.left).is_some() => {
        let handle = self.cell_of_target(&assign.left).expect("checked");
        let b = syntax::Build::new(self.parsed.arena());
        let read = syntax::target_expr(b, &assign.left).expect("cell_of_target answers for a name or a path");
        let value = match syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
          None => self.expr(&assign.right)?,
          Some(computed) => self.expr(&computed)?,
        };
        let value = match &read {
          member_expr if member_expr.is_member_expression() => {
let member = member_expr.as_member_expression().expect("matched as a member");
            let (_, path) = write_root(member).expect("cell_of_target found the root");
            self.written_at(Expr::ReadCell(Box::new(handle.clone())), &path, value)?
          }
          _ => value,
        };
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.cells.iter().any(|(name, _)| *name == id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!() };
        let handle = self.cells.iter().find(|(name, _)| *name == id.name.as_str()).map(|(_, handle)| handle.clone()).expect("checked");
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = self.build();
        let one = b.number(1.0, update.span);
        let value = self.expr(&b.binary(b.updated(&update.argument), op, one, update.span))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(value))))
      }
      js::Expression::AssignmentExpression(assign) if self.local_target(&assign.left).is_some() => self.local_assign(assign),
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.is_local(id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!() };
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = self.build();
        let one = b.number(1.0, update.span);
        let expr = self.expr(&b.binary(b.updated(&update.argument), op, one, update.span))?;
        Ok(Stmt::Set { name: id.name.to_string(), expr })
      }
      js::Expression::CallExpression(call) if self.cell_collection_write(call).is_some() => {
        let (handle, kind, method) = self.cell_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::ReadCell(Box::new(handle.clone())))?;
        Ok(Stmt::Expr(Expr::WriteCell(Box::new(handle), Box::new(expr))))
      }
      js::Expression::CallExpression(call) if self.local_url_search_write(call).is_some() => {
        let (name, method) = self.local_url_search_write(call).expect("checked");
        let search = Expr::Builtin { name: Builtin::UrlPart, args: vec![Expr::Var(name.clone()), Expr::lit_str("search")] };
        let written = self.collection_written(call, Kind::Query, &method, search)?;
        Ok(Stmt::Set { expr: Expr::Builtin { name: Builtin::UrlSet, args: vec![Expr::Var(name.clone()), Expr::lit_str("search"), written] }, name })
      }
      js::Expression::CallExpression(call) if self.local_collection_write(call).is_some() => {
        let (name, kind, method) = self.local_collection_write(call).expect("checked");
        let expr = self.collection_written(call, kind, &method, Expr::Var(name.clone()))?;
        Ok(Stmt::Set { name, expr })
      }
      js::Expression::CallExpression(call) if self.unbound_collection_write(call).is_some() => {
        let (name, span) = self.unbound_collection_write(call).expect("checked");
        self.unbound = Some(name.clone());
        Err(self.residue(span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body")))
      }
      js::Expression::CallExpression(call) if self.local_push(call).is_some() => {
        let (name, method) = self.local_push(call).expect("checked");
        let mut items = Vec::with_capacity(call.arguments.len() + 1);
        for arg in &call.arguments {
          let value = self.expr(crate::syntax::arg_expr(arg))?;
          items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
        }
        let held = Entry::Spread(Expr::Var(name.clone()));
        if method == "push" {
          items.insert(0, held);
        } else {
          items.push(held);
        }
        Ok(Stmt::Set { name, expr: Expr::Array(items) })
      }
      js::Expression::AssignmentExpression(assign) => {
        if assign.operator != js::AssignmentOperator::Assign {
          return Err(self.residue(assign.span, "a compound assignment; write the full expression"));
        }
        let Some(member) = assign.left.as_member_expression() else {
          return Err(self.residue(assign.left.span(), "an assignment to something other than the session"));
        };
        let (key, path) = self.session_target(member)?;
        let value = self.expr(&assign.right)?;
        Ok(Stmt::SessionSet { key, path, value })
      }
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => {
        let member = match &unary.argument {
          js::Expression::ChainExpression(o) => match o.expression.as_member_expression() {
            Some(member) => member,
            None => return Err(self.residue(o.expression.span(), "`delete` of a call")),
          },
          other => match other.as_member_expression() {
            Some(member) => member,
            None => return Err(self.residue(other.span(), "`delete` of something other than a session entry")),
          },
        };
        let (key, path) = self.session_target(member)?;
        Ok(Stmt::SessionDelete { key, path })
      }
      js::Expression::CallExpression(call) if self.parsed.names_server(&call.callee, "redirect") => {
        Ok(Stmt::Return(self.redirect_to(call)?))
      }
      js::Expression::CallExpression(call) if self.parsed.names_server(&call.callee, "after") => Ok(Stmt::Expr(self.after_of(call)?)),
      js::Expression::CallExpression(call) if self.is_ident_call(call, "fail") => {
        let b = syntax::Build::new(self.parsed.arena());
        let stmt = b.expr_stmt(b.copy(expr));
        let (kind, message) = self.as_fail(&stmt).expect("checked");
        Ok(Stmt::Guard { cond: Expr::Lit(Lit::Bool(true)), kind: kind?, message: message? })
      }
      js::Expression::CallExpression(call) if self.is_session_extend(call) => {
        if !self.extends {
          return Err(self.residue_with(
            call.span,
            "`session.extend` outside an action, a route handler or middleware",
            "a loader runs on every navigation, so extending there is a store write per page view; extend from an action, a route handler or middleware, which can decide when",
          ));
        }
        let [arg] = call.arguments.as_slice() else {
          return Err(self.residue(call.span, "`session.extend` takes seconds, one number"));
        };
        Ok(Stmt::SessionExtend { seconds: self.expr(crate::syntax::arg_expr(arg))? })
      }
      other => Ok(Stmt::Expr(self.expr(other)?)),
    }
  }

  /// The argument of the global `Promise.all(...)` or, where no handler resumes later, `Promise.resolve(...)`. A body has no promise values, since a call is its answer, so the argument is what it reads as.
  pub(crate) fn promise_all<'e>(&self, e: &'e js::Expression<'e>) -> Option<&'e js::Expression<'e>> {
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
    let callee = &call.callee;
    let Some(m) = (callee).as_member_expression() else { return None };
    let (js::Expression::Identifier(promise), crate::syntax::MemberProp::Ident(all)) = (m.object(), crate::syntax::prop(m)) else { return None };
    let settled_here = all.name.as_str() == "resolve" && !self.reaches(Reach::Later);
    if self.parsed.ctxt(promise) != crate::Ctxt::Unresolved || promise.name.as_str() != "Promise" || !(all.name.as_str() == "all" || settled_here) {
      return None;
    }
    let [arg] = call.arguments.as_slice() else { return None };
    (!crate::syntax::is_spread(arg)).then_some(crate::syntax::arg_expr(arg))
  }

  /// Whether `name` is a binding the body declared, which a write may rebind.
  pub(crate) fn is_local(&self, name: &str) -> bool {
    match self.scope.iter().rev().find(|(n, _)| n == name) {
      Some((_, bound)) => matches!(bound, Expr::Var(var) if var == name),
      // A module-level `let` a helper writes resolves to its own name, bound by the component that reads it.
      None => matches!(self.globals.iter().rev().find(|(n, _)| n == name), Some((_, Expr::Var(var))) if var == name),
    }
  }

  /// `x++` or `--x` inside an expression, with `x` a local: the old value is held under a temporary, the write becomes a statement before the one holding it and the expression reads the temporary, the new value for a prefix. Only an update the statement reaches unconditionally is lifted; one under a condition or inside a function stays where it is.
  pub(crate) fn lift_updates(&mut self, stmt: &js::Statement, out: &mut Vec<Stmt>) -> Lowered<()> {
    if let js::Statement::ExpressionStatement(e) = stmt {
      if matches!(strip_parens(&e.expression), js::Expression::UpdateExpression(_)) {
        return Ok(());
      }
    }
    for (span, name, prefix, plus) in crate::component::updates_in(stmt) {
      if !self.is_local(&name) {
        // A name bound nowhere here may be a module-level `let`, which the set binds and retries with.
        if !self.scope.iter().any(|(n, _)| *n == name) && !self.globals.iter().any(|(n, _)| *n == name) {
          self.unbound = Some(name.clone());
          return Err(self.residue(span, format!("`{name}` is not bound here; an import the build cannot follow or a name from outside the body")));
        }
        continue;
      }
      let op = if plus { ArithOp::Add } else { ArithOp::Sub };
      let next = Expr::Arith(op, Box::new(Expr::Var(name.clone())), Box::new(Expr::Lit(Lit::Float(1.0))));
      if prefix {
        out.push(Stmt::Set { name: name.clone(), expr: next });
        self.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(name));
      } else {
        let held = self.temp();
        out.push(Stmt::Let { name: held.clone(), expr: Expr::Var(name.clone()) });
        out.push(Stmt::Set { name, expr: next });
        self.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(held));
      }
    }
    Ok(())
  }

  /// The local a write lands on: `x` or `x.field` with `x` declared in the body.
  pub(crate) fn local_target(&self, target: &js::AssignmentTarget) -> Option<String> {
    match target {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.is_local(id.name.as_str()).then(|| id.name.to_string()),
      other => match write_root(other.as_member_expression()?) {
        Some((id, _)) if self.is_local(id.name.as_str()) => Some(id.name.to_string()),
        _ => None,
      },
    }
  }

  /// `current` with what `path` reaches replaced by `value`, each step on the way copied: a field as a spread with the field changed, an index through `SetAt`.
  pub(crate) fn written_at(&mut self, current: Expr, path: &[&js::MemberExpression], value: Expr) -> Lowered<Expr> {
    let Some((step, rest)) = path.split_first() else { return Ok(value) };
    if let Some(field) = self.member_name(step) {
      let inner = self.written_at(current.clone().field(field.clone()), rest, value)?;
      return Ok(Expr::Object(vec![Entry::Spread(current), Entry::Field(field, inner)]));
    }
    let crate::syntax::MemberProp::Computed(key) = crate::syntax::prop(step) else { return Err(self.residue(step.span(), "a write through a private field")) };
    let key = self.expr(key)?;
    let inner = self.written_at(Expr::Index(Box::new(current.clone()), Box::new(key.clone())), rest, value)?;
    Ok(Expr::Builtin { name: Builtin::SetAt, args: vec![current, key, inner] })
  }
}
