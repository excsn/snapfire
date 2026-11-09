use super::*;

/// The act running `join`, when there is one.
pub(super) fn run_join(join: Option<&Join>, walk: &mut HandlerWalk) {
  if let Some(join) = join {
    walk.act(snapfire_fsr_ir::host::Act::Run.name().to_owned(), Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(join.event.clone())), Entry::Field("carry".to_owned(), join.carry.clone())]));
  }
}

/// A branch's statements: a block's, or the one statement it is.
pub(super) fn arm<'s>(stmt: &'s js::Statement<'s>) -> &'s [js::Statement<'s>] {
  match stmt {
    js::Statement::BlockStatement(block) => &block.body,
    other => std::slice::from_ref(other),
  }
}

pub(crate) fn switch_arms<'s>(lowerer: &Lowerer<'_>, sw: &'s js::SwitchStatement<'s>) -> Lowered<SwitchArms<'s>> {
  let mut arms = Vec::new();
  let mut default = None;
  let mut tests: Vec<&js::Expression> = Vec::new();
  let last = sw.cases.len().saturating_sub(1);
  for (i, case) in sw.cases.iter().enumerate() {
    let is_default = case.test.is_none();
    if is_default && i != last {
      return Err(lowerer.residue(case.span, "a `default` before the last `case`"));
    }
    if let Some(test) = &case.test {
      tests.push(test);
    }
    let cons: &[js::Statement] = match case.consequent.as_slice() {
      [js::Statement::BlockStatement(block)] => &block.body,
      other => other,
    };
    if cons.is_empty() {
      continue;
    }
    let body: &[js::Statement] = match cons.last() {
      Some(js::Statement::BreakStatement(b)) if b.label.is_none() => &cons[..cons.len() - 1],
      Some(js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_)) => cons,
      Some(js::Statement::ContinueStatement(c)) if c.label.is_none() && lowerer.breakable.contains(&crate::Breakable::Loop) => cons,
      _ if i == last => cons,
      _ => return Err(lowerer.residue_with(case.span, "a `case` that falls through into the next", "end it with `break` or `return`")),
    };
    if let Some(inner) = body.iter().find(|s| holds_break(s)) {
      return Err(lowerer.residue(inner.span(), "a `break` before the end of its `case`"));
    }
    if is_default {
      default = Some(body);
      tests.clear();
    } else {
      arms.push((std::mem::take(&mut tests), body));
    }
  }
  Ok((arms, default))
}

/// `throw Error(message)`, `throw new Error(message)` or `throw value` as a render that fails with the message.
pub(crate) fn throw_expr(lowerer: &mut Lowerer<'_>, thrown: &js::ThrowStatement) -> Lowered<Expr> {
  fn error_message<'e>(lowerer: &Lowerer<'_>, callee: &js::Expression, args: &'e [js::Argument<'e>]) -> Option<(String, Option<&'e js::Expression<'e>>)> {
    match crate::meaning::bare(callee) {
    js::Expression::Identifier(id) if crate::ERROR_CONSTRUCTORS.contains(&id.name.as_str()) && lowerer.meanings.as_ref().is_none_or(|m| m.ctxt(id) == crate::Ctxt::Unresolved) => Some((id.name.to_string(), args.first().map(crate::syntax::arg_expr))),
      _ => None,
    }
  }
  let message = match crate::meaning::bare(&thrown.argument) {
    js::Expression::CallExpression(call) => error_message(lowerer, &call.callee, &call.arguments),
    js::Expression::NewExpression(new) => error_message(lowerer, &new.callee, &new.arguments),
    _ => None,
  };
  let (message, error) = match message {
    Some((name, arg)) => {
      let message = match arg {
        Some(arg) => lowerer.expr(&arg)?,
        None => Expr::lit_str(""),
      };
      let error = Expr::Object(vec![Entry::Field("name".to_owned(), Expr::lit_str(name)), Entry::Field("message".to_owned(), Expr::Str(Box::new(message.clone())))]);
      (message, Some(error))
    }
    None => {
      let value = lowerer.expr(&thrown.argument)?;
      if matches!(value, Expr::Lit(_) | Expr::Template(..)) {
        (value, None)
      } else {
        let t = lowerer.temp();
        let read = Expr::Var(t.clone());
        let message = Expr::Coalesce(Box::new(Expr::Field(Box::new(read.clone()), "message".to_owned())), Box::new(read.clone()));
        return Ok(Expr::Block(vec![Stmt::Let { name: t, expr: value }, Stmt::Return(Expr::Builtin { name: Builtin::Throw, args: vec![message, read] })]));
      }
    }
  };
  Ok(Expr::Builtin { name: Builtin::Throw, args: std::iter::once(message).chain(error).collect() })
}

/// A `try` statement as a block whose value is how it ended: `lower` gives each of its blocks a value, a `catch` binds its parameter and a `finally` whose value is not `null` replaces the others'.
pub(super) fn try_paths<'t>(lowerer: &mut Lowerer<'_>, t: &'t js::TryStatement<'t>, lower: &mut dyn FnMut(&mut Lowerer<'_>, Vec<&'t js::Statement<'t>>) -> Lowered<Expr>) -> Lowered<Expr> {
  let body = lower(lowerer, t.block.body.iter().collect())?;
  let (name, handler) = match &t.handler {
    None => (None, None),
    Some(catch) => {
      let depth = lowerer.scope.len();
      let name = catch_param(lowerer, catch.param.as_ref());
      let handler = name.and_then(|name| Ok((name, lower(lowerer, catch.body.body.iter().collect())?)));
      lowerer.scope.truncate(depth);
      let (name, handler) = handler?;
      (name, Some(vec![Stmt::Return(handler)]))
    }
  };
  let finally = match &t.finalizer {
    None => Vec::new(),
    Some(finalizer) => {
      let ended = lower(lowerer, finalizer.body.iter().collect())?;
      let held = lowerer.temp();
      let returned = Expr::Compare(CompareOp::Ne, Box::new(Expr::Var(held.clone())), Box::new(Expr::Lit(Lit::Null)));
      vec![Stmt::Let { name: held.clone(), expr: ended }, Stmt::If { cond: returned, then: vec![Stmt::Return(Expr::Var(held))], r#else: Vec::new() }]
    }
  };
  Ok(Expr::Block(vec![Stmt::Try { body: vec![Stmt::Return(body)], name, handler, finally }]))
}

/// The `return`s a `try` holds outside the functions inside it.
pub(super) fn returns_in<'e>(t: &'e js::TryStatement<'e>) -> Vec<&'e js::ReturnStatement<'e>> {
  struct Returns<'t>(Vec<&'t js::ReturnStatement<'t>>);
  fn walk<'t>(stmt: &'t js::Statement<'t>, out: &mut Returns<'t>) {
    match stmt {
      js::Statement::ReturnStatement(ret) => out.0.push(ret),
      js::Statement::BlockStatement(b) => b.body.iter().for_each(|s| walk(s, out)),
      js::Statement::IfStatement(i) => {
        walk(&i.consequent, out);
        if let Some(alt) = &i.alternate {
          walk(alt, out);
        }
      }
      js::Statement::SwitchStatement(sw) => sw.cases.iter().flat_map(|c| &c.consequent).for_each(|s| walk(s, out)),
      js::Statement::ForOfStatement(f) => walk(&f.body, out),
      js::Statement::TryStatement(t) => {
        t.block.body.iter().for_each(|s| walk(s, out));
        t.handler.iter().flat_map(|c| &c.body.body).for_each(|s| walk(s, out));
        t.finalizer.iter().flat_map(|f| &f.body).for_each(|s| walk(s, out));
      }
      _ => {}
    }
  }
  let mut out = Returns(Vec::new());
  t.block.body.iter().for_each(|s| walk(s, &mut out));
  t.handler.iter().flat_map(|c| &c.body.body).for_each(|s| walk(s, &mut out));
  t.finalizer.iter().flat_map(|f| &f.body).for_each(|s| walk(s, &mut out));
  out.0
}

/// `run` inside a frame of its own for the `let`s a statement holds, which it answers with what `run` answered.
pub(crate) fn held_frame<T>(lowerer: &mut Lowerer<'_>, run: impl FnOnce(&mut Lowerer<'_>) -> T) -> (Vec<Stmt>, T) {
  lowerer.held.push(Vec::new());
  lowerer.held_reach.push(lowerer.rebind_reach.len());
  let result = run(lowerer);
  lowerer.held_reach.pop();
  (lowerer.held.pop().expect("the statement's frame"), result)
}

/// A `try` statement that does not `return`, run as statements over the locals it writes: each is a variable for the run, bound to where it stood before. The run's value is an object holding where each ended. Answers the run and the names it wrote.
pub(super) fn try_run(lowerer: &mut Lowerer<'_>, t: &js::TryStatement, writable: &Writable<'_>) -> Lowered<(Expr, Vec<String>)> {
  let mut writes = Writes::default();
  t.walk(&mut writes);
  let Writes { written, declared } = writes;
  let written: Vec<String> = written.into_iter().filter(|name| !declared.contains(name) && !lowerer.cells.iter().any(|(cell, _)| cell == name)).collect();
  let mut run = Vec::new();
  for name in &written {
    if !writable.local(lowerer, name) {
      return Err(lowerer.residue_with(t.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
    }
    run.push(Stmt::Let { name: name.clone(), expr: current(lowerer, name) });
  }
  let depth = lowerer.scope.len();
  for name in &written {
    lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
  }
  let lowered = lowerer.try_stmt(t);
  lowerer.scope.truncate(depth);
  run.push(lowered?);
  run.push(Stmt::Return(Expr::Object(written.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect())));
  Ok((Expr::Block(run), written))
}

/// A `catch` parameter put in scope: a name as itself, a pattern read from a temporary that holds what was thrown.
pub(crate) fn catch_param(lowerer: &mut Lowerer<'_>, param: Option<&js::CatchParameter>) -> Lowered<Option<String>> {
  match param.map(|p| &p.pattern) {
    None => Ok(None),
    Some(js::BindingPattern::BindingIdentifier(id)) => {
      let name = id.name.to_string();
      lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
      Ok(Some(name))
    }
    Some(pattern) => {
      let name = lowerer.temp();
      bind_pattern(lowerer, pattern, Expr::Var(name.clone()))?;
      Ok(Some(name))
    }
  }
}

pub(super) fn holds_break(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::BreakStatement(_) => true,
    js::Statement::BlockStatement(b) => b.body.iter().any(holds_break),
    js::Statement::IfStatement(i) => holds_break(&i.consequent) || i.alternate.as_ref().is_some_and(holds_break),
    _ => false,
  }
}

/// Whether a statement returns or throws somewhere inside it, either of which ends the path it is on.
pub(crate) fn holds_return(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::ReturnStatement(_) | js::Statement::ThrowStatement(_) => true,
    js::Statement::BlockStatement(b) => b.body.iter().any(holds_return),
    js::Statement::IfStatement(i) => holds_return(&i.consequent) || i.alternate.as_ref().is_some_and(holds_return),
    js::Statement::TryStatement(t) => t.block.body.iter().chain(t.handler.iter().flat_map(|c| &c.body.body)).chain(t.finalizer.iter().flat_map(|f| &f.body)).any(holds_return),
    js::Statement::SwitchStatement(sw) => sw.cases.iter().any(|c| c.consequent.iter().any(holds_return)),
    _ => false,
  }
}
