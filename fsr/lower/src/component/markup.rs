use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// A JSX child or a component's return: a tree when the expression holds
  /// JSX anywhere a render would reach, text otherwise.
  pub(super) fn child_expr(&mut self, expr: &'p js::Expression<'p>) -> Lowered<Tmpl> {
    self.premake_children(expr)?;
    if let Some(name) = self.slot_of_expr(expr) {
      return Ok(self.slot(&name));
    }
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.child_expr(&p.expression),
      js::Expression::JSXElement(el) => self.element(el, false),
      js::Expression::JSXFragment(frag) => Ok(Tmpl::Fragment(self.children(&frag.children)?)),
      js::Expression::StringLiteral(s) => Ok(Tmpl::Text(crate::lossy(s.value))),
      js::Expression::ConditionalExpression(c) if self.holds_markup(&c.consequent) || self.holds_markup(&c.alternate) => {
        let cond = self.lowerer.expr(&c.test)?;
        let then = Box::new(self.child_expr(&c.consequent)?);
        let r#else = Some(Box::new(self.child_expr(&c.alternate)?));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expression::LogicalExpression(bin) if bin.operator == js::LogicalOperator::Coalesce && self.layout_root && self.slot_of_expr(&bin.left).is_some() => {
        let name = self.slot_of_expr(&bin.left).expect("checked by the guard");
        let fallback = self.child_expr(&bin.right)?;
        Ok(self.slot_with_fallback(&name, vec![fallback]))
      }
      js::Expression::LogicalExpression(bin) if bin.operator == js::LogicalOperator::And && self.holds_markup(&bin.right) => {
        let cond = self.lowerer.expr(&bin.left)?;
        let then = Box::new(self.child_expr(&bin.right)?);
        // `c && <b />` is `c` when `c` is falsy, so `0` and `NaN` print as React prints them and `false`, `null` and `""` print nothing.
        let r#else = Some(Box::new(Tmpl::Expr(cond.clone())));
        Ok(Tmpl::If { cond, then, r#else })
      }
      js::Expression::CallExpression(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = map_call(call).expect("checked by the guard");
        self.map_tree(over, arrow)
      }
      js::Expression::ChainExpression(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => {
        let (over, arrow) = optional_map(expr).expect("checked by the guard");
        let missing = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::IsNullish, args: vec![self.lowerer.expr(over)?] };
        let each = self.map_tree(over, arrow)?;
        Ok(Tmpl::If { cond: missing, then: Box::new(Tmpl::Fragment(Vec::new())), r#else: Some(Box::new(each)) })
      }
      js::Expression::CallExpression(_) if self.markup_call(expr).is_some() => {
        let (call, name, params, body) = self.markup_call(expr).expect("checked by the guard");
        self.inline_markup_fn(call, name, &params, body)
      }
      js::Expression::ArrayExpression(array) if self.holds_markup(expr) => {
        let mut items = Vec::new();
        for item in array_items(array) {
          items.push(self.child_expr(item)?);
        }
        Ok(fills::items(items))
      }
      js::Expression::Identifier(id) if id.name.as_str() == "null" || id.name.as_str() == "undefined" => Ok(Tmpl::Fragment(Vec::new())),
      js::Expression::NullLiteral(_) => Ok(Tmpl::Fragment(Vec::new())),
      other => {
        self.premake_expr(other)?;
        Ok(Tmpl::Expr(self.lowerer.expr(other)?))
      }
    }
  }

  /// `over.map(arrow)` where `arrow` returns markup: a `Tmpl::For` over the items.
  pub(super) fn map_tree(&mut self, over: &'p js::Expression<'p>, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<Tmpl> {
    let over = self.lowerer.expr(over)?;
    let depth = self.lowerer.scope.len();
    let params = bind_params(&mut self.lowerer, &crate::params_of(&arrow.params))?;
    let range = self.lowerer.parsed.range(arrow.span);
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.push(range);
    }
    let body = match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Expr(e) => self.child_expr(e),
      crate::syntax::ArrowBody::Block(b) => self.tree_of(&b.body.iter().collect::<Vec<_>>(), depth),
    };
    if let Some(candidates) = &mut self.lowerer.hoisting {
      candidates.open_loops.pop();
    }
    self.lowerer.scope.truncate(depth);
    Ok(Tmpl::For { over, params, body: Box::new(body?) })
  }

  /// Whether a child expression is markup: JSX or a call to a function the component declares that returns some, alone or inside a branch or `&&`.
  pub(super) fn holds_markup(&self, expr: &'p js::Expression<'p>) -> bool {
    match expr {
      js::Expression::ParenthesizedExpression(p) => self.holds_markup(&p.expression),
      js::Expression::ConditionalExpression(c) => self.holds_markup(&c.consequent) || self.holds_markup(&c.alternate),
      js::Expression::LogicalExpression(b) if b.operator == js::LogicalOperator::And => self.holds_markup(&b.right),
      js::Expression::ArrayExpression(array) => array_items(array).any(|item| self.holds_markup(item)),
      js::Expression::JSXElement(_) | js::Expression::JSXFragment(_) => true,
      js::Expression::CallExpression(call) if map_call(call).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      js::Expression::ChainExpression(_) if optional_map(expr).is_some_and(|(_, arrow)| self.arrow_markup(arrow)) => true,
      other => self.markup_call(other).is_some(),
    }
  }

  /// Each `Children.toArray(children)`, `Children.map(children, f)` and `Children.only(children)` under `e` over the component's own children, made ahead as a read of `render::CHILD_VALUES`, which the placement passes.
  pub(super) fn premake_children(&mut self, e: &'p js::Expression<'p>) -> Lowered<()> {
    match e {
      js::Expression::CallExpression(call) if matches!(&call.callee, callee if self.slot_of_expr(callee).as_deref() == Some("content")) => {
        let mut args = Vec::with_capacity(call.arguments.len());
        for arg in &call.arguments {
          self.premake_children(crate::syntax::arg_expr(arg))?;
          args.push(self.lowerer.expr(crate::syntax::arg_expr(arg))?);
        }
        let first = Expr::Builtin { name: Builtin::At, args: vec![Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned()), Expr::Lit(Lit::Int(0))] };
        let span = call.span;
        self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), Expr::Apply { f: Box::new(first), args });
        if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
          self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
        }
        Ok(())
      }
      js::Expression::CallExpression(call) => {
        if let Some((method, rest)) = self.own_children_call(call) {
          let values = Expr::Var(snapfire_fsr_ir::render::CHILD_VALUES.to_owned());
          let made = match (method, rest) {
            ("toArray", []) => Some(values),
            ("only", []) => Some(Expr::Builtin { name: Builtin::At, args: vec![values, Expr::Lit(Lit::Int(0))] }),
            ("map", [f]) => {
              self.premake_expr(crate::syntax::arg_expr(f))?;
              Some(Expr::Map(Box::new(values), Box::new(self.lowerer.expr(crate::syntax::arg_expr(f))?)))
            }
            _ => None,
          };
          if let Some(made) = made {
            let span = call.span;
            self.lowerer.premade.insert((crate::lo(span), crate::hi(span)), made);
            if !self.counts_children.contains(&snapfire_fsr_ir::render::CHILD_VALUES) {
              self.counts_children.push(snapfire_fsr_ir::render::CHILD_VALUES);
            }
            return Ok(());
          }
        }
        if let Some(member) = call.callee.as_member_expression() {
          self.premake_children(member.object())?;
        }
        call.arguments.iter().try_for_each(|arg| self.premake_children(crate::syntax::arg_expr(arg)))
      }
      member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); self.premake_children(member.object()) },
      js::Expression::ParenthesizedExpression(p) => self.premake_children(&p.expression),
      js::Expression::TSAsExpression(a) => self.premake_children(&a.expression),
      js::Expression::ConditionalExpression(c) => {
        self.premake_children(&c.test)?;
        self.premake_children(&c.consequent)?;
        self.premake_children(&c.alternate)
      }
      js::Expression::BinaryExpression(b) => {
        self.premake_children(&b.left)?;
        self.premake_children(&b.right)
      }
      _ => Ok(()),
    }
  }

  /// `Children.<method>(children, ...rest)` over the component's own children.
  pub(super) fn own_children_call<'c, 'e>(&self, call: &'c js::CallExpression<'e>) -> Option<(&'static str, &'c [js::Argument<'e>])> {
    let callee = &call.callee;
    let Some(member) = (callee).as_member_expression() else { return None };
    if self.meanings().of_expr(member.object()) != Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) {
      return None;
    }
    let (first, rest) = call.arguments.split_first()?;
    if crate::syntax::is_spread(first) || self.slot_of_expr(crate::syntax::arg_expr(first)).as_deref() != Some("content") {
      return None;
    }
    let method = match prop_name_of(member).as_deref()? {
      "toArray" => "toArray",
      "only" => "only",
      "map" => "map",
      _ => return None,
    };
    Some((method, rest))
  }

  /// `Children.count(children)` (`render::CHILD_COUNT`) or `Children.toArray(children).length` (`render::CHILD_ITEMS`) over the component's own children.
  pub(super) fn counts_own_children(&self, e: &js::Expression) -> Option<&'static str> {
    let children_call = |call: &js::CallExpression, method: &str| -> bool {
      let callee = &call.callee;
      let Some(member) = (callee).as_member_expression() else { return false };
      prop_name_of(member).as_deref() == Some(method)
        && self.meanings().of_expr(member.object()) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children))
        && matches!(call.arguments.as_slice(), [arg] if !crate::syntax::is_spread(arg) && self.slot_of_expr(crate::syntax::arg_expr(arg)).as_deref() == Some("content"))
    };
    match unwrap_literal(e) {
      js::Expression::CallExpression(call) if children_call(call, "count") => Some(snapfire_fsr_ir::render::CHILD_COUNT),
      other if other.as_member_expression().is_some_and(|member| prop_name_of(member).as_deref() == Some("length") && matches!(unwrap_literal(member.object()), js::Expression::CallExpression(call) if children_call(call, "toArray"))) => Some(snapfire_fsr_ir::render::CHILD_ITEMS),
      _ => None,
    }
  }

  /// Whether an arrow returns markup: its expression holds some or its block returns some.
  pub(super) fn arrow_markup(&self, arrow: &'p js::ArrowFunctionExpression<'p>) -> bool {
    match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Expr(e) => self.holds_markup(e),
      crate::syntax::ArrowBody::Block(b) => self.returns_markup(&b.body),
    }
  }

  /// Whether a function body returns markup on some path: a `return` of markup or of a local a declaration in the body made markup, inside a branch, a block or a `switch` arm included.
  pub(super) fn returns_markup(&self, stmts: &'p [js::Statement]) -> bool {
    let mut locals = Vec::new();
    self.stmts_return_markup(stmts, &mut locals)
  }

  pub(super) fn stmts_return_markup(&self, stmts: &'p [js::Statement], locals: &mut Vec<String>) -> bool {
    stmts.iter().any(|stmt| self.stmt_returns_markup(stmt, locals))
  }

  pub(super) fn stmt_returns_markup(&self, stmt: &'p js::Statement<'p>, locals: &mut Vec<String>) -> bool {
    match stmt {
      js::Statement::VariableDeclaration(var) => {
        for decl in &var.declarations {
          if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
            if self.holds_markup(init) {
              locals.push(id.name.to_string());
            }
          }
        }
        false
      }
      js::Statement::ReturnStatement(r) => r.argument.as_ref().is_some_and(|arg| self.holds_markup(arg) || matches!(unwrap_literal(arg), js::Expression::Identifier(id) if locals.iter().any(|l| l == id.name.as_str()))),
      js::Statement::BlockStatement(b) => self.stmts_return_markup(&b.body, locals),
      js::Statement::IfStatement(branch) => self.stmt_returns_markup(&branch.consequent, locals) || branch.alternate.as_ref().is_some_and(|alt| self.stmt_returns_markup(alt, locals)),
      js::Statement::SwitchStatement(sw) => sw.cases.iter().any(|case| self.stmts_return_markup(&case.consequent, locals)),
      _ => false,
    }
  }

  /// A call by name to a function the component or its module declares, an arrow `const` or a `function`, whose body returns markup on some path.
  pub(super) fn markup_call(&self, expr: &'p js::Expression<'p>) -> Option<(&'p js::CallExpression<'p>, String, Vec<crate::Param<'p>>, FunctionBody<'p>)> {
    let js::Expression::CallExpression(call) = expr else { return None };
    let callee = &call.callee;
    let js::Expression::Identifier(id) = callee else { return None };
    let name = id.name.to_string();
    let (params, body) = match self.handler_fns.get(&name) {
      Some(declared) => (declared.params.clone(), declared.body),
      None if !self.lowerer.scope.iter().any(|(n, _)| *n == name) => match find_function(self.lowerer.parsed, &name)? {
        Found::Declared(params, stmts, _) => (crate::params_of(params), FunctionBody::Block(stmts)),
        Found::Arrow(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
      },
      None => return None,
    };
    if self.checking.borrow().contains(&name) {
      return None;
    }
    self.checking.borrow_mut().push(name.clone());
    let markup = match body {
      FunctionBody::Expr(e) => self.holds_markup(e),
      FunctionBody::Block(stmts) => self.returns_markup(stmts),
    };
    self.checking.borrow_mut().pop();
    markup.then_some((call, name, params, body))
  }

  /// A call of a markup helper inside its own body, as a placement of the helper: a helper taking one object pattern is handed the argument as its props, any other its arguments by position. A helper declared inside the component closes over its locals and is refused.
  pub(super) fn place_recursion(&mut self, call: &'p js::CallExpression<'p>, name: String, params: &[crate::Param]) -> Lowered<Tmpl> {
    if self.handler_fns.contains_key(&name) {
      return Err(self.lowerer.residue(call.span, format!("`{name}` returns markup by calling itself inside the component; declare it at module level so the build can place it")));
    }
    if let Some(spread) = call.arguments.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(spread).span(), format!("a spread argument to `{name}`, which returns markup")));
    }
    let by_props = matches!(params, [crate::Param { pattern: js::BindingPattern::ObjectPattern(_), default: None, rest: false, .. }]);
    let mut props = Vec::with_capacity(params.len());
    for i in 0..params.len() {
      let value = match call.arguments.get(i) {
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Undefined),
      };
      props.push(match by_props {
        true => Entry::Spread(value),
        false => Entry::Field(i.to_string(), value),
      });
    }
    if !by_props && !self.positional_refs.contains(&name) {
      self.positional_refs.push(name.clone());
    }
    let loc = self.lowerer.parsed.loc(call.span.start);
    self.refs.push((name.clone(), loc));
    Ok(Tmpl::Component { module: format!("{}#{name}", self.file), props, children: Vec::new(), id: 0, keyed: false })
  }

  /// A function returning markup, inlined where it is called: each argument is lowered in the caller's scope and bound to its parameter by a `Tmpl::Let` around the function's tree.
  pub(super) fn inline_markup_fn(&mut self, call: &'p js::CallExpression<'p>, name: String, params: &[crate::Param], body: FunctionBody<'p>) -> Lowered<Tmpl> {
    if self.inlining.contains(&name) {
      return self.place_recursion(call, name, params);
    }
    if let Some(spread) = call.arguments.iter().find(|a| crate::syntax::is_spread(a)) {
      return Err(self.lowerer.residue(crate::syntax::arg_expr(spread).span(), format!("a spread argument to `{name}`, which returns markup")));
    }
    let mut args = Vec::with_capacity(params.len());
    for i in 0..params.len() {
      args.push(match call.arguments.get(i) {
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      });
    }
    let depth = self.lowerer.scope.len();
    let mut lets = Vec::with_capacity(params.len());
    for (param, value) in params.iter().zip(args) {
      let held = self.lowerer.temp();
      if let Err(residue) = bind_param(&mut self.lowerer, param, Expr::Var(held.clone())) {
        self.lowerer.scope.truncate(depth);
        return Err(residue);
      }
      lets.push((held, value));
    }
    self.inlining.push(name);
    let tree = match body {
      FunctionBody::Expr(e) => self.child_expr(e),
      FunctionBody::Block(stmts) => self.tree_of(&stmts.iter().collect::<Vec<_>>(), depth),
    };
    self.inlining.pop();
    self.lowerer.scope.truncate(depth);
    let mut tree = tree?;
    for (held, value) in lets.into_iter().rev() {
      tree = Tmpl::Let { name: held, expr: value, then: Box::new(tree) };
    }
    Ok(tree)
  }
}
