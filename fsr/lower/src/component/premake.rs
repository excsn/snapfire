use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// `name` as a function only the browser calls: a setter, a dispatcher or an inner function.
  pub(super) fn browser_function(&mut self, name: String) {
    self.lowerer.browser_functions.push(name.clone());
    self.handlers.push(name);
  }

  /// Each JSX-holding expression under `e` outside a function, lowered to
  /// markup ahead of the statement or attribute that holds it, so
  /// `Lowerer::expr` takes it made. `fills` later places it in the tree.
  pub(super) fn premake_expr(&mut self, e: &'p js::Expression<'p>) -> Lowered<()> {
    if let Some(kind) = self.counts_own_children(e) {
      let span = e.span();
      self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Var(kind.to_owned()));
      if !self.counts_children.contains(&kind) {
        self.counts_children.push(kind);
      }
      return Ok(());
    }
    self.premake_children(e)?;
    if let js::Expression::ArrayExpression(a) = e {
      for item in array_items(a) {
        self.premake_expr(item)?;
      }
      return Ok(());
    }
    if self.holds_markup(e) {
      let tmpl = self.child_expr(e)?;
      let span = e.span();
      self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Markup(std::sync::Arc::new(tmpl)));
      return Ok(());
    }
    match e {
      js::Expression::ParenthesizedExpression(p) => self.premake_expr(&p.expression),
      js::Expression::ObjectExpression(o) => {
        for prop in &o.properties {
          if let js::ObjectPropertyKind::ObjectProperty(prop) = prop {
            if let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) {
              self.premake_expr(&kv.value)?;
            }
          }
        }
        Ok(())
      }
      js::Expression::ArrayExpression(a) => {
        for item in array_items(a) {
          self.premake_expr(item)?;
        }
        Ok(())
      }
      js::Expression::ConditionalExpression(c) => {
        self.premake_expr(&c.consequent)?;
        self.premake_expr(&c.alternate)
      }
      js::Expression::BinaryExpression(b) => {
        self.premake_expr(&b.left)?;
        self.premake_expr(&b.right)
      }
      js::Expression::LogicalExpression(l) => {
        self.premake_expr(&l.left)?;
        self.premake_expr(&l.right)
      }
      js::Expression::AssignmentExpression(a) => self.premake_expr(&a.right),
      js::Expression::ArrowFunctionExpression(arrow) => {
        let depth = self.lowerer.scope.len();
        let result = bind_params(&mut self.lowerer, &crate::params_of(&arrow.params)).and_then(|_| match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Expr(e) => self.premake_expr(e),
          crate::syntax::ArrowBody::Block(b) => self.premake_returning(&b.body),
        });
        self.lowerer.scope.truncate(depth);
        result
      }
      js::Expression::CallExpression(call) => {
        {
      let callee = &call.callee;
          if let Some(member) = (callee).as_member_expression() {
            self.premake_expr(member.object())?;
          }
        }
        for arg in &call.arguments {
          self.premake_expr(crate::syntax::arg_expr(arg))?;
        }
        Ok(())
      }
      _ => Ok(()),
    }
  }

  /// [`Self::premake_expr`] over a statement whose expressions reach
  /// `Lowerer::expr`: a declaration, a write or a branch holding them.
  /// Marks the loop at `span` open for the hoist candidates its body holds, with the `push` calls its body makes onto `written`, so the browser's copy keys them by iteration; whether there was a candidate set to mark.
  pub(super) fn open_statement_loop(&mut self, span: Span, body: &js::Statement, written: &[String]) -> bool {
    let range = self.lowerer.parsed.range(span);
    let body_range = self.lowerer.parsed.range(body.span());
    struct Pushes<'w> {
      written: &'w [String],
      args: Vec<Span>,
    }
    impl<'a> oxc_ast_visit::Visit<'a> for Pushes<'_> {
      fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
        {
      let callee = &call.callee;
          if let Some(m) = (callee).as_member_expression() {
            if let (js::Expression::Identifier(id), Some("push")) = (m.object(), prop_name_of(m).as_deref()) {
              if self.written.iter().any(|w| w == id.name.as_str()) {
                self.args.extend(call.arguments.iter().filter(|a| !crate::syntax::is_spread(a)).map(|a| crate::syntax::arg_expr(a).span()));
              }
            }
          }
        }
        oxc_ast_visit::walk::walk_call_expression(self, call);
      }
      fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
      fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
      fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
      fn visit_for_statement(&mut self, _: &js::ForStatement<'a>) {}
    }
    let mut pushes = Pushes { written, args: Vec::new() };
    body.walk(&mut pushes);
    let args: Vec<std::ops::Range<usize>> = pushes.args.into_iter().map(|span| self.lowerer.parsed.range(span)).collect();
    let Some(candidates) = &mut self.lowerer.hoisting else { return false };
    if !candidates.statement_loops.iter().any(|l| l.range == range) {
      candidates.statement_loops.push(hoist::StatementLoop::new(range.clone(), body_range, args));
    }
    candidates.open_loops.push(range);
    true
  }

  pub(super) fn close_statement_loop(&mut self, open: bool) {
    if let (true, Some(candidates)) = (open, &mut self.lowerer.hoisting) {
      candidates.open_loops.pop();
    }
  }

  /// [`Self::premake_stmt`] over a block's statements with each of its declarations bound as it goes, so markup after one reads it; the bindings end with the block.
  /// [`Self::premake_stmt`] for `xs.forEach(f)` under the names the counted loop [`for_each_loop`] reads it as, so the markup its callback pushes and the fold agree.
  pub(super) fn premake_for_each(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let Some(js::Statement::ForStatement(f)) = for_each_loop(self.lowerer.build(), call, self.lowerer.meanings.as_deref()) else { return Ok(()) };
    let over_children = (&call.callee).as_member_expression().is_some_and(|m| self.meanings().of_expr(m.object()) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)));
    if over_children {
      if self.slot_of_expr(&crate::syntax::arg_expr(&call.arguments[0])).as_deref() != Some("content") {
        return Ok(());
      }
      let values = snapfire_fsr_ir::render::CHILD_VALUES;
      if !self.counts_children.contains(&values) {
        self.counts_children.push(values);
      }
      if !self.lowerer.scope.iter().any(|(n, _)| n == values) {
        self.lowerer.scope.push((values.to_owned(), Expr::Var(values.to_owned())));
      }
    }
    let Ok(counted) = Counted::of(&self.lowerer, &f) else { return Ok(()) };
    let names = LoopNames::over(f.span, &f.init, &f.body);
    let callback = if over_children { &call.arguments[1] } else { &call.arguments[0] };
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match crate::meaning::bare(crate::syntax::arg_expr(callback)) {
      js::Expression::ArrowFunctionExpression(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
      js::Expression::FunctionExpression(func) => match &func.body {
        Some(b) => (crate::params_of(&func.params), FunctionBody::Block(&b.statements)),
        None => return Ok(()),
      },
      _ => return Ok(()),
    };
    let js::Statement::BlockStatement(looped) = &f.body else { return Ok(()) };
    let item = looped.body.first().and_then(|s| match s {
      js::Statement::VariableDeclaration(var) if params.first().is_some() => var.declarations.first().and_then(|d| d.init.as_ref()),
      _ => None,
    });
    let depth = self.lowerer.scope.len();
    let result = counted.lowered(&mut self.lowerer, &names.item).and_then(|(_, counter)| {
      for name in &names.written {
        self.lowerer.scope.push((name.clone(), names.read(name)));
      }
      self.lowerer.scope.push((counted.name.clone(), counter));
      if let (Some(head), Some(item)) = (params.first(), item) {
        let value = self.lowerer.expr(item)?;
        bind_param(&mut self.lowerer, head, value)?;
      }
      let open = self.open_statement_loop(f.span, &f.body, &names.written);
      let result = match body {
        FunctionBody::Block(stmts) => self.premake_block(stmts),
        FunctionBody::Expr(e) => self.premake_expr(e),
      };
      self.close_statement_loop(open);
      result
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  pub(super) fn premake_block(&mut self, stmts: &'p [js::Statement]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      self.premake_stmt(stmt)?;
      if let js::Statement::VariableDeclaration(var) = stmt {
        for decl in &var.declarations {
          let value = match decl.init.as_ref() {
            Some(init) => self.premade_value(&decl.id, init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          bind_pattern(&mut self.lowerer, &decl.id, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  /// A declaration's value for the markup [`Self::premake_block`] prepares. A value the build cannot follow outside the application's own code, an awaited browser call among them, is bound as a variable: the pass that lowers the function itself decides what becomes of it.
  pub(super) fn premade_value(&mut self, name: &js::BindingPattern, init: &'p js::Expression<'p>) -> Lowered<Expr> {
    match self.lowerer.expr(init) {
      Ok(value) => Ok(value),
      Err(residue) if self.unbound_is_followable() || self.lowerer.reach_violation => Err(residue),
      Err(_) => {
        self.lowerer.unbound = None;
        Ok(match name {
          js::BindingPattern::BindingIdentifier(id) => Expr::Var(id.name.to_string()),
          _ => Expr::Lit(Lit::Undefined),
        })
      }
    }
  }

  /// [`Self::premake_block`] over a function's body, whose `return`s hold values the way its declarations do.
  pub(super) fn premake_returning(&mut self, stmts: &'p [js::Statement]) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let result = stmts.iter().try_for_each(|stmt| {
      match stmt {
        js::Statement::ReturnStatement(r) => {
          if let Some(arg) = r.argument.as_ref() {
            self.premake_expr(arg)?;
          }
        }
        other => self.premake_stmt(other)?,
      }
      if let js::Statement::VariableDeclaration(var) = stmt {
        for decl in &var.declarations {
          let value = match decl.init.as_ref() {
            Some(init) => self.premade_value(&decl.id, init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          bind_pattern(&mut self.lowerer, &decl.id, value)?;
        }
      }
      Ok(())
    });
    self.lowerer.scope.truncate(depth);
    result
  }

  pub(super) fn premake_stmt(&mut self, stmt: &'p js::Statement<'p>) -> Lowered<()> {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        for decl in &var.declarations {
          if let Some(init) = decl.init.as_ref() {
            self.premake_expr(init)?;
          }
        }
        Ok(())
      }
      js::Statement::ExpressionStatement(e) => match &e.expression {
        js::Expression::CallExpression(call) if for_each_loop(self.lowerer.build(), call, self.lowerer.meanings.as_deref()).is_some() => self.premake_for_each(call),
        other => self.premake_expr(other),
      },
      js::Statement::IfStatement(branch) => {
        self.premake_stmt(&branch.consequent)?;
        match branch.alternate.as_ref() {
          Some(alt) => self.premake_stmt(alt),
          None => Ok(()),
        }
      }
      js::Statement::BlockStatement(b) => self.premake_block(&b.body),
      js::Statement::TryStatement(t) => self.premake_stmt_try(t),
      js::Statement::SwitchStatement(sw) => {
        for case in &sw.cases {
          self.premake_block(&case.consequent)?;
        }
        Ok(())
      }
      js::Statement::ForOfStatement(for_of) => {
        let Some(head) = LoopNames::head(for_of) else { return Ok(()) };
        let names = LoopNames::of(for_of);
        let depth = self.lowerer.scope.len();
        for name in &names.written {
          self.lowerer.scope.push((name.clone(), names.read(name)));
        }
        let open = self.open_statement_loop(for_of.span, &for_of.body, &names.written);
        let result = bind_pattern(&mut self.lowerer, head, Expr::Var(names.item.clone())).and_then(|_| self.premake_stmt(&for_of.body));
        self.close_statement_loop(open);
        self.lowerer.scope.truncate(depth);
        result
      }
      js::Statement::ForStatement(f) => {
        let Ok(counted) = Counted::of(&self.lowerer, f) else { return Ok(()) };
        let names = LoopNames::over(f.span, &f.init, &f.body);
        let depth = self.lowerer.scope.len();
        let result = counted.lowered(&mut self.lowerer, &names.item).and_then(|(_, counter)| {
          for name in &names.written {
            self.lowerer.scope.push((name.clone(), names.read(name)));
          }
          self.lowerer.scope.push((counted.name.clone(), counter));
          let open = self.open_statement_loop(f.span, &f.body, &names.written);
          let result = self.premake_stmt(&f.body);
          self.close_statement_loop(open);
          result
        });
        self.lowerer.scope.truncate(depth);
        result
      }
      _ => Ok(()),
    }
  }
}
