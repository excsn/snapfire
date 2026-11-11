use super::*;

impl HandlerWalk {
  /// `stmt`, a write to a cell or a call that makes one, run where the handler reaches it.
  pub(crate) fn effect(&mut self, stmt: Stmt) {
    match &self.reach {
      None => self.out.push(stmt),
      Some(reach) => self.out.push(Stmt::If { cond: reach.clone(), then: vec![stmt], r#else: Vec::new() }),
    }
  }

  pub(crate) fn set(&mut self, state: String, value: Expr) {
    if let Some(in_place) = &mut self.in_place {
      let var = match in_place.iter().find(|(key, _, _)| *key == state) {
        Some((_, var, _)) => var.clone(),
        None => {
          let var = format!("$set{}${state}", in_place.len());
          let start = self.patch.iter().find_map(|entry| match entry {
            Entry::Field(name, held) if *name == state => Some(held.clone()),
            _ => None,
          });
          in_place.push((state.clone(), var.clone(), start.unwrap_or_else(|| Expr::Var(state.clone()))));
          var
        }
      };
      let value = match &self.reach {
        None => value,
        Some(reach) => Expr::Ternary(Box::new(reach.clone()), Box::new(value), Box::new(Expr::Var(var.clone()))),
      };
      self.out.push(Stmt::Set { name: var.clone(), expr: value });
      self.patch.retain(|entry| !matches!(entry, Entry::Field(name, _) if *name == state));
      self.patch.push(Entry::Field(state, Expr::Var(var)));
      return;
    }
    let value = match &self.reach {
      None => value,
      Some(reach) => {
        let prior = self
          .patch
          .iter()
          .find_map(|entry| match entry {
            Entry::Field(name, held) if *name == state => Some(held.clone()),
            _ => None,
          })
          .unwrap_or_else(|| Expr::Var(state.clone()));
        Expr::Ternary(Box::new(reach.clone()), Box::new(value), Box::new(prior))
      }
    };
    self.patch.retain(|entry| !matches!(entry, Entry::Field(name, _) if *name == state));
    self.patch.push(Entry::Field(state, value));
  }

  pub(crate) fn act(&mut self, action: String, input: Expr) {
    let act = Stmt::Act { action, input };
    match &self.reach {
      None => self.out.push(act),
      Some(reach) => self.out.push(Stmt::If { cond: reach.clone(), then: vec![act], r#else: Vec::new() }),
    }
  }
}

pub(crate) fn reach_under(outer: &Option<Expr>, cond: Expr) -> Expr {
  match outer {
    None => cond,
    Some(outer) => Expr::Logic(LogicOp::And, Box::new(outer.clone()), Box::new(cond)),
  }
}

/// Whether a handler's statements do something besides setting state: an act, or a write to a cell.
/// Whether `expr` is a call of a function it holds as a value, a prop among them: a handler reference the parent passed runs that handler.
pub(super) fn calls_a_value(expr: &Expr) -> bool {
  matches!(expr, Expr::Apply { f, .. } if !matches!(**f, Expr::Lambda { .. }))
}

/// `undefined` or `void 0`, which a `return` hands back as a bare `return` does.
pub(super) fn is_undefined(expr: &js::Expression) -> bool {
  match crate::meaning::bare(expr) {
    js::Expression::Identifier(id) => id.name.as_str() == "undefined",
    js::Expression::UnaryExpression(u) => u.operator == js::UnaryOperator::Void && u.argument.is_literal(),
    _ => false,
  }
}

/// Whether `expr` writes the store wherever it is evaluated.
pub(super) fn writes_store(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::WriteStore(..)));
  found
}

pub(super) fn holds_effect(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } | Stmt::Expr(_) => true,
    Stmt::If { then, r#else, .. } => holds_effect(then) || holds_effect(r#else),
    Stmt::Try { body, handler, finally, .. } => holds_effect(body) || handler.as_deref().is_some_and(holds_effect) || holds_effect(finally),
    _ => false,
  })
}

pub(crate) fn holds_act(stmts: &[Stmt]) -> bool {
  stmts.iter().any(|stmt| match stmt {
    Stmt::Act { .. } => true,
    Stmt::If { then, r#else, .. } => holds_act(then) || holds_act(r#else),
    Stmt::Try { body, handler, finally, .. } => holds_act(body) || handler.as_deref().is_some_and(holds_act) || holds_act(finally),
    _ => false,
  })
}

pub(crate) fn block_to_expr(lowerer: &mut Lowerer<'_>, stmts: &[js::Statement]) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  block_to_expr_from(lowerer, stmts, depth)
}

/// `block_to_expr` with the names bound at or above `floor` writable, a function's parameters among them.
pub(crate) fn block_to_expr_from(lowerer: &mut Lowerer<'_>, stmts: &[js::Statement], floor: usize) -> Lowered<Expr> {
  let (depth, cells) = (lowerer.scope.len(), lowerer.cells.len());
  let wrapped = std::mem::replace(&mut lowerer.wrapped_returns, false);
  let refs: Vec<&js::Statement> = stmts.iter().collect();
  let result = block_to_expr_inner(lowerer, &refs, floor);
  lowerer.wrapped_returns = wrapped;
  lowerer.scope.truncate(depth);
  lowerer.cells.truncate(cells);
  result
}

/// Statements as the value each path returns: a branch with a `return` in it is a ternary over the paths through it and what follows, a write to a local rebinds it, a path that ends without a `return` is `null`.
pub(super) fn block_to_expr_inner(lowerer: &mut Lowerer<'_>, stmts: &[&js::Statement], floor: usize) -> Lowered<Expr> {
  let Some((first, rest)) = stmts.split_first() else {
    return Ok(Expr::Lit(Lit::Null));
  };
  fn joined<'s, 'a>(head: &'s js::Statement<'a>, rest: &[&'s js::Statement<'a>]) -> Vec<&'s js::Statement<'a>> {
    let mut out: Vec<&'s js::Statement<'a>> = match head {
      js::Statement::BlockStatement(b) => b.body.iter().collect(),
      other => vec![other],
    };
    out.extend(rest.iter().copied());
    out
  }
  let path = |lowerer: &mut Lowerer<'_>, stmts: Vec<&js::Statement>| -> Lowered<Expr> {
    let depth = lowerer.scope.len();
    let value = block_to_expr_inner(lowerer, &stmts, floor);
    lowerer.scope.truncate(depth);
    value
  };
  match first {
    js::Statement::VariableDeclaration(var) if { let written = function_writes(&lowerer.parsed.scope, rest); var.declarations.iter().any(|decl| matches!(&decl.id, js::BindingPattern::BindingIdentifier(id) if written.contains(&(id.name.to_string(), lowerer.parsed.binding_ctxt(id))))) } => {
      let written = function_writes(&lowerer.parsed.scope, rest);
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        match &decl.id {
          js::BindingPattern::BindingIdentifier(id) if written.contains(&(id.name.to_string(), lowerer.parsed.binding_ctxt(id))) => {
            let handle = lowerer.temp();
            let name = id.name.to_string();
            lets.push(Stmt::Let { name: handle.clone(), expr: Expr::Cell(Box::new(expr)) });
            lowerer.cells.push((name.clone(), Expr::Var(handle.clone())));
            lowerer.scope.push((name.clone(), Expr::ReadCell(Box::new(Expr::Var(handle)))));
            if let Some(init) = decl.init.as_ref() {
              lowerer.note_kind(&name, init);
            }
          }
          pattern => {
            if computed_once(lowerer, &expr) || !plain(&expr) {
              let held = lowerer.temp();
              lets.push(Stmt::Let { name: held.clone(), expr });
              expr = Expr::Var(held);
            }
            bind_pattern(lowerer, pattern, expr)?;
          }
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) if !lowerer.cells.is_empty() => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if let (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::CallExpression(call))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) {
          if lowerer.reaches(Reach::Process) && lowerer.spawns(call) {
            lowerer.spawned.push(id.name.to_string());
          }
        }
        if computed_once(lowerer, &expr) || !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) if var.declarations.iter().filter_map(|decl| decl.init.as_ref()).any(|init| matches!(crate::meaning::bare(init), js::Expression::NewExpression(_) | js::Expression::CallExpression(_))) && lowerer.reaches(Reach::Terminal) => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if let (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::CallExpression(call))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) {
          if lowerer.reaches(Reach::Process) && lowerer.spawns(call) {
            lowerer.spawned.push(id.name.to_string());
          }
        }
        if computed_once(lowerer, &expr) || !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::VariableDeclaration(var) => {
      let mut lets = Vec::new();
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if !plain(&expr) {
          let held = lowerer.temp();
          lets.push(Stmt::Let { name: held.clone(), expr });
          expr = Expr::Var(held);
        }
        let from = lowerer.scope.len();
        bind_pattern(lowerer, &decl.id, expr)?;
        let (held, ()) = held_frame(lowerer, |lowerer| settle(lowerer, from));
        lets.extend(held);
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
          note_alias(lowerer, id.name.as_str(), init, &Writable { floor, frozen: &[], setters: &[] })?;
        }
      }
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(lets, value))
    }
    js::Statement::ReturnStatement(ret) => {
      let value = match &ret.argument {
        Some(arg) => lowerer.expr(arg)?,
        None => Expr::Lit(Lit::Undefined),
      };
      Ok(match lowerer.wrapped_returns {
        true => Expr::Array(vec![Entry::Item(value)]),
        false if ret.argument.is_none() => Expr::Lit(Lit::Null),
        false => value,
      })
    }
    js::Statement::ThrowStatement(thrown) => throw_expr(lowerer, thrown),
    js::Statement::EmptyStatement(_) => block_to_expr_inner(lowerer, rest, floor),
    js::Statement::TryStatement(t) => {
      let outer = std::mem::replace(&mut lowerer.wrapped_returns, true);
      let lowered = try_paths(lowerer, t, &mut |lowerer, stmts| path(lowerer, stmts));
      lowerer.wrapped_returns = outer;
      let ended = lowered?;
      let held = lowerer.temp();
      let after = block_to_expr_inner(lowerer, rest, floor)?;
      let returned = match outer {
        true => Expr::Var(held.clone()),
        false => Expr::Index(Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Int(0)))),
      };
      let fell = Expr::Compare(CompareOp::Eq, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Null)));
      Ok(Expr::Block(vec![Stmt::Let { name: held, expr: ended }, Stmt::Return(Expr::Ternary(Box::new(fell), Box::new(after), Box::new(returned)))]))
    }
    js::Statement::BlockStatement(_) if holds_return(first) => path(lowerer, joined(first, rest)),
    js::Statement::IfStatement(if_stmt) if holds_return(first) => {
      let cond = lowerer.expr(&if_stmt.test)?;
      let then = path(lowerer, joined(&if_stmt.consequent, rest))?;
      let otherwise = match &if_stmt.alternate {
        Some(alt) => path(lowerer, joined(alt, rest))?,
        None => path(lowerer, rest.to_vec())?,
      };
      Ok(Expr::Ternary(Box::new(cond), Box::new(then), Box::new(otherwise)))
    }
    js::Statement::SwitchStatement(sw) if holds_return(first) && { let d = lowerer.expr(&sw.discriminant)?; !plain(&d) } => {
      let disc = lowerer.expr(&sw.discriminant)?;
      let held = lowerer.temp();
      let name = format!("$sw{}", crate::lo(sw.span));
      lowerer.scope.push((name.clone(), Expr::Var(held.clone())));
      let b = lowerer.build();
      let mut replaced = b.copy(&**sw);
      replaced.discriminant = b.ident(&name, sw.span);
      let stmt = js::Statement::SwitchStatement(b.boxed(replaced));
      let mut all: Vec<&js::Statement> = vec![&stmt];
      all.extend(rest.iter().copied());
      let value = block_to_expr_inner(lowerer, &all, floor)?;
      Ok(sequenced(vec![Stmt::Let { name: held, expr: disc }], value))
    }
    js::Statement::SwitchStatement(sw) if holds_return(first) => {
      let disc = lowerer.expr(&sw.discriminant)?;
      let (arms, default) = switch_arms(lowerer, sw)?;
      let mut fallback: Vec<&js::Statement> = default.map(|d| d.iter().collect()).unwrap_or_default();
      fallback.extend(rest.iter().copied());
      let mut value = path(lowerer, fallback)?;
      for (tests, body) in arms.into_iter().rev() {
        let mut cond: Option<Expr> = None;
        for test in tests {
          let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(lowerer.expr(test)?));
          cond = Some(match cond {
            None => eq,
            Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
          });
        }
        let mut stmts: Vec<&js::Statement> = body.iter().collect();
        stmts.extend(rest.iter().copied());
        let then = path(lowerer, stmts)?;
        value = Expr::Ternary(Box::new(cond.unwrap_or(Expr::Lit(Lit::Bool(false)))), Box::new(then), Box::new(value));
      }
      Ok(value)
    }
    js::Statement::ForStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::WhileStatement(_) if returns_from_loop(first) => {
      let looped = loop_return(lowerer.build(), first, rest);
      path(lowerer, looped.iter().collect())
    }
    js::Statement::IfStatement(branch) if lowerer.reaches(Reach::Terminal) && holds_acts(lowerer, first) => {
      let cond = lowerer.expr(&branch.test)?;
      let arm = |lowerer: &mut Lowerer<'_>, arm: &js::Statement| -> Lowered<Expr> {
        let depth = lowerer.scope.len();
        let stmts: &[js::Statement] = match arm {
          js::Statement::BlockStatement(block) => &block.body,
          other => std::slice::from_ref(other),
        };
        let lowered = block_to_expr(lowerer, stmts);
        lowerer.scope.truncate(depth);
        lowered
      };
      let then = arm(lowerer, &branch.consequent)?;
      let otherwise = match &branch.alternate {
        Some(alt) => arm(lowerer, alt)?,
        None => Expr::Lit(Lit::Null),
      };
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::If { cond, then: vec![Stmt::Expr(then)], r#else: vec![Stmt::Expr(otherwise)] }], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(&stmt.expression, js::Expression::CallExpression(call) if lowerer.issues_act(call)) => {
      let act = lowerer.effect(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![act], value))
    }
    js::Statement::ExpressionStatement(stmt) if writes_a_cell(lowerer, &stmt.expression) => {
      let Stmt::Expr(effect) = lowerer.effect(&stmt.expression)? else { return Err(lowerer.residue(stmt.span, "a write to a cell the build cannot order")) };
      let held = lowerer.temp();
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::Let { name: held, expr: effect }], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(crate::meaning::bare(&stmt.expression), js::Expression::CallExpression(call) if console_method(lowerer, call).is_some()) => {
      let logged = lowerer.effect(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![logged], value))
    }
    js::Statement::ExpressionStatement(stmt) if matches!(&stmt.expression, js::Expression::CallExpression(_)) && {
      let depth = lowerer.scope.len();
      let probed = lowerer.expr(&stmt.expression).map(|e| writes_store(&e) || (lowerer.reaches(Reach::Terminal) && effectful(&e))).unwrap_or(false);
      lowerer.scope.truncate(depth);
      probed
    } => {
      let effect = lowerer.expr(&stmt.expression)?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(vec![Stmt::Expr(effect)], value))
    }
    straight => {
      let writable = Writable { floor, frozen: &[], setters: &[] };
      let (held, run) = held_frame(lowerer, |lowerer| rebind_stmt(lowerer, straight, &writable, &mut Vec::new()));
      run?;
      let value = block_to_expr_inner(lowerer, rest, floor)?;
      Ok(sequenced(held, value))
    }
  }
}

/// Whether a statement's expression writes a cell or calls something that may, which it must do in order where the lowerer holds cells.
pub(super) fn writes_a_cell(lowerer: &Lowerer<'_>, expr: &js::Expression) -> bool {
  if lowerer.cells.is_empty() {
    return false;
  }
  let held = |name: &str| lowerer.cells.iter().any(|(cell, _)| cell == name);
  match expr {
    js::Expression::AssignmentExpression(assign) => lowerer.cell_of_target(&assign.left).is_some(),
    js::Expression::UpdateExpression(update) => matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if held(id.name.as_str())),
    js::Expression::CallExpression(call) => {
      let collection_write = call.callee.as_member_expression().is_some_and(|m| {
        matches!(m.object(), js::Expression::Identifier(id) if !held(id.name.as_str()))
          && matches!(lowerer.kind_of(m.object()).map(crate::Kind::collection), Some(crate::Kind::Map | crate::Kind::Set))
          && matches!(prop_name_of(m).as_deref(), Some("set" | "add" | "delete" | "clear"))
      });
      !collection_write
    }
    _ => false,
  }
}

/// Whether evaluating `expr` does something besides answer: it emits, issues an act, writes a cell or the store, writes the session or calls an extension that reaches the process or runs later.
pub(super) fn effectful(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| {
    found |= match e {
      Expr::Builtin { name: Builtin::Emit, .. } | Expr::WriteCell(..) | Expr::WriteStore(..) => true,
      Expr::Ext { module, name, .. } => matches!(snapfire_fsr_ir::standard_reach(module, name), Some(Reach::Process | Reach::Later)),
      Expr::Block(body) => acts(body),
      _ => false,
    };
  });
  found
}

/// Whether `body` holds an act or a session write at any depth of its own statements.
pub(super) fn acts(body: &[Stmt]) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::Act { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::SessionExtend { .. } => true,
    Stmt::If { then, r#else, .. } => acts(then) || acts(r#else),
    Stmt::ForOf { body, .. } => acts(body),
    Stmt::Try { body, handler, finally, .. } => acts(body) || handler.as_deref().is_some_and(acts) || acts(finally),
    _ => false,
  })
}

/// Whether `stmt` holds a call that issues an act, [`Lowerer::issues_act`], outside a function of its own.
pub(super) fn holds_acts(lowerer: &Lowerer<'_>, stmt: &js::Statement) -> bool {
  struct Acts<'l, 'p>(&'l Lowerer<'p>, bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Acts<'_, '_> {
    fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
      self.1 |= self.0.issues_act(call);
      oxc_ast_visit::walk::walk_call_expression(self, call);
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Acts(lowerer, false);
  stmt.walk(&mut found);
  found.1
}

/// Whether a declaration's value must be computed once, where it is declared, rather than wherever it is read: it touches a cell, it issues an act or, in a terminal page, it calls a server-only extension such as the randomness a promise's id is drawn from.
pub(super) fn computed_once(lowerer: &Lowerer<'_>, expr: &Expr) -> bool {
  let mut acts = false;
  expr.visit(&mut |e| {
    if let Expr::Block(body) = e {
      acts |= body.iter().any(|stmt| matches!(stmt, Stmt::Act { .. }));
    }
  });
  acts || touches_cells(expr) || (lowerer.reaches(Reach::Terminal) && lowerer.body_extension_in(expr).is_some())
}

/// Whether an expression reads or writes a cell or calls a function, which may: a value computed from one is taken where it is declared, not where it is used.
pub(super) fn touches_cells(expr: &Expr) -> bool {
  let mut found = false;
  expr.visit(&mut |e| found |= matches!(e, Expr::ReadCell(_) | Expr::WriteCell(..) | Expr::Apply { .. }));
  found
}

/// An object or array literal holding only literals and other such literals: data a body can compare and write by identity, so every read of a module constant holding it must reach the one object. A `$` field is a record the lowerer reads in place, a class or a marker, and is not data.
pub(super) fn data_only(expr: &Expr) -> bool {
  match expr {
    Expr::Object(entries) | Expr::Array(entries) => entries.iter().all(|entry| match entry {
      Entry::Field(name, _) if name.starts_with('$') => false,
      Entry::Field(_, value) | Entry::Item(value) => matches!(value, Expr::Lit(_)) || data_only(value),
      _ => false,
    }),
    _ => false,
  }
}

pub(crate) fn plain(expr: &Expr) -> bool {
  match expr {
    Expr::Object(entries) | Expr::Array(entries) if !entries.iter().any(|entry| matches!(entry, Entry::Field(_, Expr::Lambda { .. } | Expr::Fix { .. }) | Entry::Item(Expr::Lambda { .. } | Expr::Fix { .. }))) => false,
    Expr::Lit(_) | Expr::Var(_) | Expr::Const(_) | Expr::Lambda { .. } | Expr::Fix { .. } | Expr::Handler(_) | Expr::HoistId(_) | Expr::Param(_) | Expr::Query(_) | Expr::ParamsAll | Expr::QueryAll | Expr::Session(_) | Expr::Store(_) | Expr::Context(_) | Expr::Identity(_) | Expr::Locale | Expr::Locales | Expr::Path | Expr::Host | Expr::Origin | Expr::Address | Expr::Config(_) | Expr::Input | Expr::Document => true,
    Expr::Field(e, _) => plain(e),
    Expr::Index(a, b) => plain(a) && plain(b),
    Expr::Object(entries) | Expr::Array(entries) => entries.iter().all(|entry| match entry {
      Entry::Field(_, e) | Entry::Item(e) => plain(e),
      Entry::Baked(_) => true,
      _ => false,
    }),
    _ => false,
  }
}

pub(crate) fn settle(lowerer: &mut Lowerer<'_>, from: usize) {
  for at in from..lowerer.scope.len() {
    let (name, value) = lowerer.scope[at].clone();
    if name.starts_with('$') || plain(&value) || lowerer.cells.iter().any(|(cell, _)| *cell == name) {
      continue;
    }
    let kind = lowerer.kinds.iter().rev().find(|(n, held, _)| *n == name && *held == value).map(|(_, _, kind)| *kind);
    let held = hold_in_reach(lowerer, value);
    lowerer.scope[at].1 = held.clone();
    if let Some(kind) = kind {
      lowerer.kinds.push((name, held, kind));
    }
  }
}

pub(super) fn hold_in_reach(lowerer: &mut Lowerer<'_>, expr: Expr) -> Expr {
  let base = *lowerer.held_reach.last().expect("a frame");
  let reach = lowerer.rebind_reach[base..].iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b)));
  let ran = match reach {
    Some(reach) => Expr::Ternary(Box::new(reach), Box::new(expr), Box::new(Expr::Lit(Lit::Null))),
    None => expr,
  };
  let held = lowerer.temp();
  lowerer.held.last_mut().expect("a frame").push(Stmt::Let { name: held.clone(), expr: ran });
  Expr::Var(held)
}

/// `lets` run in order before `value` is computed.
pub(crate) fn sequenced(mut lets: Vec<Stmt>, value: Expr) -> Expr {
  if let Expr::Block(stmts) = &value {
    if !lets.is_empty() {
      lets.extend(stmts.iter().cloned());
      return Expr::Block(lets);
    }
  }
  if lets.is_empty() {
    return value;
  }
  lets.push(Stmt::Return(value));
  Expr::Block(lets)
}

/// The method of `console.log(...)`, `info`, `debug`, `warn` or `error` on the global `console`; `None` for any other call.
pub(super) fn console_method(lowerer: &Lowerer<'_>, call: &js::CallExpression) -> Option<String> {
  let callee = &call.callee;
  let Some(m) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
  let js::Expression::Identifier(console) = crate::meaning::bare(m.object()) else { return None };
  let global = lowerer.meanings.as_ref().is_none_or(|meanings| meanings.ctxt(console) == crate::Ctxt::Unresolved);
  let method = prop_name_of(m).unwrap_or_default();
  (global && console.name.as_str() == "console" && matches!(method.as_str(), "log" | "info" | "debug" | "warn" | "error")).then_some(method)
}

/// `console.log(...)`, `info`, `debug`, `warn` or `error` on the global `console` as the log it writes on the server, as a server renderer running the component writes it, or as nothing when an argument is one only the browser can compute.
pub(crate) fn console_call(lowerer: &mut Lowerer<'_>, call: &js::CallExpression) -> Lowered<Option<Expr>> {
  let Some(method) = console_method(lowerer, call) else { return Ok(None) };
  let spread = call.arguments.iter().any(|arg| crate::syntax::is_spread(arg));
  let mut values = Vec::with_capacity(call.arguments.len());
  for arg in &call.arguments {
    match lowerer.expr(crate::syntax::arg_expr(arg)) {
      Ok(value) => values.push((crate::syntax::is_spread(arg), value)),
      // An argument the server cannot compute, the viewer's clock or zone among them, leaves the call to the browser, which logs it when the component hydrates; the markup is the same either way.
      Err(residue) => {
        let followable = lowerer.unbound.as_ref().is_some_and(|name| !lowerer.unfollowed.contains(name) && matches!(lowerer.meanings.as_ref().and_then(|m| m.bindings.get(name)), Some(crate::meaning::Meaning::Own { .. })));
        if followable || lowerer.reach_violation {
          return Err(residue);
        }
        lowerer.unbound = None;
        return Ok(Some(Expr::Lit(Lit::Null)));
      }
    }
  }
  if spread {
    let entries = values.into_iter().map(|(spread, value)| if spread { Entry::Spread(value) } else { Entry::Item(value) }).collect();
    return Ok(Some(Expr::Builtin { name: Builtin::LogOf, args: vec![Expr::lit_str(method), Expr::Array(entries)] }));
  }
  let mut args = vec![Expr::lit_str(method)];
  args.extend(values.into_iter().map(|(_, value)| value));
  Ok(Some(Expr::Builtin { name: Builtin::Log, args }))
}

/// `xs.forEach((x, i) => body)` as `for (let i = 0; i < xs.length; i++) { const x = xs[i]; body }`, a bare `return` in the callback a `continue`; `xs` a name or a chain of fields read off one, so reading it each pass reads the same array.
pub(super) fn for_each_loop<'a>(b: crate::syntax::Build<'a>, call: &js::CallExpression, meanings: Option<&crate::meaning::Meanings>) -> Option<js::Statement<'a>> {
  let js::Expression::StaticMemberExpression(m) = crate::meaning::bare(&call.callee) else { return None };
  if m.property.name != "forEach" {
    return None;
  }
  fn steady(expr: &js::Expression) -> bool {
    match crate::meaning::bare(expr) {
      js::Expression::Identifier(_) => true,
      js::Expression::StaticMemberExpression(m) => steady(&m.object),
      _ => false,
    }
  }
  let span = call.span;
  // `Children.forEach(children, f)` walks the values the placement passed, which `ComponentLowerer::premake_for_each` binds.
  let children_of = meanings.and_then(|meanings| meanings.of_expr(&m.object)) == Some(crate::meaning::Meaning::Library(crate::meaning::Construct::Children)) && call.arguments.len() == 2;
  let (over, callback) = if children_of {
    (b.ident(snapfire_fsr_ir::render::CHILD_VALUES, span), &call.arguments[1])
  } else if call.arguments.len() == 1 && steady(&m.object) {
    (b.copy(&m.object), &call.arguments[0])
  } else {
    return None;
  };
  let (params, stmts): (Vec<crate::Param>, Vec<js::Statement<'a>>) = match crate::meaning::bare(crate::syntax::arg_expr(callback)) {
    js::Expression::ArrowFunctionExpression(arrow) => (
      crate::params_of(&arrow.params),
      match crate::syntax::arrow_body_of(arrow) {
        crate::syntax::ArrowBody::Block(body) => body.body.iter().map(|s| b.copy(s)).collect(),
        crate::syntax::ArrowBody::Expr(e) => vec![b.expr_stmt(b.copy(e))],
      },
    ),
    js::Expression::FunctionExpression(f) => (crate::params_of(&f.params), f.body.as_ref()?.statements.iter().map(|s| b.copy(s)).collect()),
    _ => return None,
  };
  let counter = match params.get(1) {
    Some(param) => param.ident()?.name.to_string(),
    None => format!("$each{}", crate::lo(span)),
  };
  fn continued<'a>(b: crate::syntax::Build<'a>, stmts: Vec<js::Statement<'a>>) -> Vec<js::Statement<'a>> {
    stmts
      .into_iter()
      .map(|stmt| match stmt {
        js::Statement::ReturnStatement(ret) if ret.argument.is_none() => js::Statement::new_continue_statement(ret.span, None, &b.ast()),
        js::Statement::IfStatement(branch) => {
          let branch = branch.unbox();
          let arm = |s: js::Statement<'a>| match s {
            js::Statement::BlockStatement(block) => {
              let block = block.unbox();
              b.block(continued(b, block.body.into_iter().collect()), block.span)
            }
            other => continued(b, vec![other]).remove(0),
          };
          js::Statement::new_if_statement(branch.span, branch.test, arm(branch.consequent), branch.alternate.map(arm), &b.ast())
        }
        js::Statement::BlockStatement(block) => {
          let span = block.span;
          b.block(continued(b, block.unbox().body.into_iter().collect()), span)
        }
        other => other,
      })
      .collect()
  }
  let mut body = Vec::new();
  if let Some(item) = params.first() {
    if item.rest {
      return None;
    }
    let read = js::Expression::new_computed_member_expression(span, b.copy(&over), b.ident(&counter, span), false, &b.ast());
    let pattern = match item.default {
      Some(default) => js::BindingPattern::new_assignment_pattern(span, b.copy(item.pattern), b.copy(default), &b.ast()),
      None => b.copy(item.pattern),
    };
    let declarator = js::VariableDeclarator::new(span, pattern, None, Some(read), false, &b.ast());
    body.push(js::Statement::new_variable_declaration(span, js::VariableDeclarationKind::Const, b.vec([declarator]), false, &b.ast()));
  }
  body.extend(continued(b, stmts));
  let length = js::Expression::new_static_member_expression(span, b.copy(&over), js::IdentifierName::new(span, b.str("length"), &b.ast()), false, &b.ast());
  let start = js::VariableDeclarator::new(span, b.binding(&counter, span), None, Some(js::Expression::new_numeric_literal(span, 0.0, None, js::NumberBase::Decimal, &b.ast())), false, &b.ast());
  let init = js::ForStatementInit::new_variable_declaration(span, js::VariableDeclarationKind::Let, b.vec([start]), false, &b.ast());
  let test = js::Expression::new_binary_expression(span, b.ident(&counter, span), js::BinaryOperator::LessThan, length, &b.ast());
  let counted = js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(&counter), &b.ast());
  let update = js::Expression::new_update_expression(span, js::UpdateOperator::Increment, false, counted, &b.ast());
  Some(js::Statement::new_for_statement(span, Some(init), Some(test), Some(update), b.block(body, span), &b.ast()))
}

/// Whether a loop's body returns from the function, outside a nested loop or function.
pub(super) fn returns_from_loop(stmt: &js::Statement) -> bool {
  let body = match stmt {
    js::Statement::ForStatement(f) => &f.body,
    js::Statement::ForOfStatement(f) => &f.body,
    js::Statement::WhileStatement(w) => &w.body,
    _ => return false,
  };
  match &*body {
    js::Statement::BlockStatement(block) => block.body.iter().any(holds_return),
    other => holds_return(other),
  }
}

/// A loop that returns or throws as one that does not: `returned`, `value` and `thrown` declared before it, each `return e` in its body a write of `value` and `returned`, each `throw e` a write of `thrown` and `returned`, the rest of an iteration run only while nothing returned, then `if (thrown) throw thrown; if (returned) return value;` ahead of `rest`.
pub(super) fn loop_return<'a>(b: crate::syntax::Build<'a>, looped: &js::Statement, rest: &[&js::Statement]) -> Vec<js::Statement<'a>> {
  let span = looped.span();
  let returned = format!("$returned{}", crate::lo(span));
  let value = format!("$value{}", crate::lo(span));
  let thrown = format!("$thrown{}", crate::lo(span));
  let ast = b.ast();
  let read = |name: &str| b.ident(name, span);
  let declare = |name: &str, init: js::Expression<'a>| {
    let declarator = js::VariableDeclarator::new(span, b.binding(name, span), None, Some(init), false, &ast);
    js::Statement::new_variable_declaration(span, js::VariableDeclarationKind::Let, b.vec([declarator]), false, &ast)
  };
  let set = |name: &str, to: js::Expression<'a>| {
    let target = js::AssignmentTarget::from(js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(name), &ast));
    b.expr_stmt(js::Expression::new_assignment_expression(span, js::AssignmentOperator::Assign, target, to, &ast))
  };
  let not_returned = || js::Expression::new_unary_expression(span, js::UnaryOperator::LogicalNot, read(&returned), &ast);
  fn rewrite<'a>(b: crate::syntax::Build<'a>, stmts: &[js::Statement], on_return: &dyn Fn(Option<js::Expression<'a>>) -> Vec<js::Statement<'a>>, on_throw: &dyn Fn(js::Expression<'a>) -> Vec<js::Statement<'a>>, unless: &dyn Fn(Vec<js::Statement<'a>>) -> js::Statement<'a>) -> Vec<js::Statement<'a>> {
    let mut out = Vec::new();
    for (at, stmt) in stmts.iter().enumerate() {
      match stmt {
        js::Statement::ReturnStatement(ret) => {
          out.extend(on_return(ret.argument.as_ref().map(|a| b.copy(a))));
          return out;
        }
        js::Statement::ThrowStatement(t) => {
          out.extend(on_throw(b.copy(&t.argument)));
          return out;
        }
        js::Statement::IfStatement(branch) if holds_return(stmt) => {
          let arm = |s: &js::Statement| -> js::Statement<'a> {
            let stmts = match s {
              js::Statement::BlockStatement(block) => block.body.as_slice(),
              other => std::slice::from_ref(other),
            };
            b.block(rewrite(b, stmts, on_return, on_throw, unless), s.span())
          };
          out.push(js::Statement::new_if_statement(branch.span, b.copy(&branch.test), arm(&branch.consequent), branch.alternate.as_ref().map(|a| arm(a)), &b.ast()));
        }
        js::Statement::BlockStatement(block) if holds_return(stmt) => out.push(b.block(rewrite(b, &block.body, on_return, on_throw, unless), block.span)),
        other => {
          out.push(b.copy(other));
          continue;
        }
      }
      let after = rewrite(b, &stmts[at + 1..], on_return, on_throw, unless);
      if !after.is_empty() {
        out.push(unless(after));
      }
      return out;
    }
    out
  }
  let on_return = |arg: Option<js::Expression<'a>>| vec![set(&value, arg.unwrap_or_else(|| read("undefined"))), set(&returned, b.boolean(true, span))];
  let on_throw = |arg: js::Expression<'a>| vec![set(&thrown, arg), set(&returned, b.boolean(true, span))];
  let unless = |stmts: Vec<js::Statement<'a>>| js::Statement::new_if_statement(span, not_returned(), b.block(stmts, span), None, &ast);
  let guard = |body: &js::Statement| -> js::Statement<'a> {
    let stmts = match body {
      js::Statement::BlockStatement(block) => block.body.as_slice(),
      other => std::slice::from_ref(other),
    };
    b.block(vec![unless(rewrite(b, stmts, &on_return, &on_throw, &unless))], span)
  };
  let looped = match looped {
    js::Statement::ForStatement(f) => {
      let mut copy = b.copy(&**f);
      copy.body = guard(&f.body);
      js::Statement::ForStatement(b.boxed(copy))
    }
    js::Statement::ForOfStatement(f) => {
      let mut copy = b.copy(&**f);
      copy.body = guard(&f.body);
      js::Statement::ForOfStatement(b.boxed(copy))
    }
    js::Statement::WhileStatement(w) => {
      let mut copy = b.copy(&**w);
      copy.test = js::Expression::new_logical_expression(span, not_returned(), js::LogicalOperator::And, b.copy(&w.test), &ast);
      copy.body = guard(&w.body);
      js::Statement::WhileStatement(b.boxed(copy))
    }
    other => b.copy(other),
  };
  let mut out = vec![declare(&returned, b.boolean(false, span)), declare(&value, read("undefined")), declare(&thrown, read("undefined")), looped];
  let threw = b.binary(read(&thrown), js::BinaryOperator::StrictInequality, read("undefined"), span);
  out.push(js::Statement::new_if_statement(span, threw, js::Statement::new_throw_statement(span, read(&thrown), &ast), None, &ast));
  out.push(js::Statement::new_if_statement(span, read(&returned), js::Statement::new_return_statement(span, Some(read(&value)), &ast), None, &ast));
  out.extend(rest.iter().map(|s| b.copy(*s)));
  out
}

/// The `return` an `if` with no `else` holds as its only statement, braced or bare.
pub(super) fn early_return<'e>(branch: &'e js::IfStatement<'e>) -> Option<&'e js::ReturnStatement<'e>> {
  if branch.alternate.is_some() {
    return None;
  }
  match &branch.consequent {
    js::Statement::ReturnStatement(ret) => Some(ret),
    js::Statement::BlockStatement(block) => match block.body.as_slice() {
      [js::Statement::ReturnStatement(ret)] => Some(ret),
      _ => None,
    },
    _ => None,
  }
}

/// The `return` or `throw` an `if` with no `else` ends in when it holds nothing else: a hook's early exit.
pub(super) fn early_exit<'e>(branch: &'e js::IfStatement<'e>) -> Option<&'e js::Statement<'e>> {
  if branch.alternate.is_some() {
    return None;
  }
  let only = match &branch.consequent {
    js::Statement::BlockStatement(block) => match block.body.as_slice() {
      [only] => only,
      _ => return None,
    },
    other => other,
  };
  matches!(only, js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_)).then_some(only)
}
