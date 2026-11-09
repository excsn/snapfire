use crate::*;

impl<'a> Lowerer<'a> {
  /// Whether `call` is one [`Lowerer::settling`] makes an act of. An async arrow called in place is one too, since it issues an act at its first `await`.
  pub(crate) fn issues_act(&self, call: &js::CallExpression) -> bool {
    if self.async_iife(call).is_some() || self.listens(call) {
      return true;
    }
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return false };
    let settler = |e: &js::Expression| matches!(crate::meaning::bare(e), js::Expression::Identifier(id) if self.settlers.iter().any(|s| *s == id.name.as_str()));
    settler(callee) || (self.reaches(Reach::Later) && id.name.as_str() == "setTimeout" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved && matches!(call.arguments.as_slice(), [callback, _] if settler(crate::syntax::arg_expr(callback)) || matches!(crate::meaning::bare(crate::syntax::arg_expr(callback)), js::Expression::ArrowFunctionExpression(_))))
  }

  /// Whether `call` is `system.spawn` from the standard module.
  pub(crate) fn spawns(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    self.meanings.as_ref().is_some_and(|meanings| meanings.names_spawn(callee))
  }

  /// `spawn(command, args, options)` in a terminal block: a fresh process id and the act [`snapfire_fsr_ir::host::Act::Spawn`] starting it, whose value is the process, [`snapfire_fsr_ir::host::PROCESS`].
  pub(crate) fn spawn_value(&mut self, call: &js::CallExpression) -> Lowered<Expr> {
    let id = self.temp();
    let mut fields = vec![Entry::Field("id".to_owned(), Expr::Var(id.clone()))];
    for (i, key) in ["command", "args", "options"].iter().enumerate() {
      let value = match call.arguments.get(i) {
        Some(arg) if crate::syntax::is_spread(arg) => return Err(self.residue(crate::syntax::arg_expr(arg).span(), "a spread into `spawn`")),
        Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      fields.push(Entry::Field((*key).to_owned(), value));
    }
    let random = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    let spawn = Stmt::Act { action: snapfire_fsr_ir::host::Act::Spawn.name().to_owned(), input: Expr::Object(fields) };
    Ok(Expr::Block(vec![Stmt::Let { name: id.clone(), expr: random }, spawn, Stmt::Return(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::PROCESS.to_owned(), Expr::Var(id))]))]))
  }

  /// The process, the report and the listener of `sub.on(event, f)`, `sub.stdout.on("data", f)` or `sub.stderr.on("data", f)` on a local a block bound to a spawn; also `stdout.on("resize", f)` and `stdout.off("resize", f)` on the terminal's output, `true` for `off`; the report `None` for `setEncoding`, which a terminal takes as read.
  pub(crate) fn listener<'c>(&self, call: &'c js::CallExpression<'c>) -> Option<(&'c js::Expression<'c>, Option<snapfire_fsr_ir::host::Report>, Option<&'c js::Expression<'c>>, bool)> {
    use snapfire_fsr_ir::host::Report;
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
    let event = call.arguments.first().and_then(|a| string_literal(crate::syntax::arg_expr(a)));
    if let (Some(on @ ("on" | "off")), Some(event)) = (self.member_name(method).as_deref(), event.as_deref()) {
      if event == Report::Resize.name() {
        return Some((method.object(), Some(Report::Resize), call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), on == "off"));
      }
    }
    let (process, stream) = match crate::meaning::bare(method.object()) {
      process @ js::Expression::Identifier(id) if self.spawned.iter().any(|s| *s == id.name.as_str()) => (process, None),
      inner_expr if inner_expr.is_member_expression() => { let inner = inner_expr.as_member_expression().expect("matched as a member"); match (crate::meaning::bare(inner.object()), self.member_name(inner).as_deref()) {
        (process @ js::Expression::Identifier(id), Some(stream @ ("stdout" | "stderr"))) if self.spawned.iter().any(|s| *s == id.name.as_str()) => (process, Some(if stream == "stdout" { Report::Stdout } else { Report::Stderr })),
        _ => return None,
      } },
      _ => return None,
    };
    let report = match (stream, self.member_name(method).as_deref(), event.as_deref()) {
      (Some(_), Some("setEncoding"), _) => return Some((process, None, None, false)),
      (Some(stream), Some("on"), Some("data")) => stream,
      (None, Some("on"), Some(event)) => Report::parse(event)?,
      _ => return None,
    };
    Some((process, Some(report), call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), false))
  }

  /// Whether `call` is a listener [`Lowerer::listener`] reads.
  pub(crate) fn listens(&self, call: &js::CallExpression) -> bool {
    self.reaches(Reach::Process) && self.listener(call).is_some()
  }

  /// A listener on a spawned process as the act [`snapfire_fsr_ir::host::Act::Listen`]. Its callback is a continuation with `code` and `signal` bound from an exit's report; a promise's `resolve` or `reject` settles the promise instead. `None` for any other call.
  pub(crate) fn listening(&mut self, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    let Some((process, report, callback, removes)) = self.listener(call) else { return Ok(None) };
    let Some(report) = report else { return Ok(Some(Stmt::Expr(Expr::Lit(Lit::Null)))) };
    let Some(callback) = callback else { return Err(self.residue(call.span, "a process listener without a function")) };
    if removes {
      return Err(self.residue(call.span, "an `off` outside a component's handler"));
    }
    let process = self.expr(process)?;
    let mut fields = vec![Entry::Field("process".to_owned(), process), Entry::Field("report".to_owned(), Expr::lit_str(report.name()))];
    if matches!(crate::meaning::bare(callback), js::Expression::Identifier(id) if self.settlers.iter().any(|s| *s == id.name.as_str())) {
      fields.push(Entry::Field("settle".to_owned(), self.expr(callback)?));
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }));
    }
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(callback) else { return Err(self.residue(callback.span(), "a process listener that is not an arrow written in place or a promise's `resolve` or `reject`")) };
    let params: Vec<String> = crate::params_of(&arrow.params).iter().map(|p| match (p.pattern, p.default.is_none() && !p.rest) {
      (js::BindingPattern::BindingIdentifier(name), true) => Ok(name.name.to_string()),
      _ => Err(self.residue(p.span, "a process listener's parameter that is not a name")),
    }).collect::<Lowered<_>>()?;
    let exits = matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close);
    let (event, carry) = self.continuation(call.span, None, |this| {
      let value = Expr::Var("$resume".to_owned()).field("value");
      for (i, param) in params.iter().enumerate() {
        let bound = match (exits, i) {
          (true, 0) => value.clone().field("code"),
          (true, _) => value.clone().field("signal"),
          (false, _) => value.clone(),
        };
        this.scope.push((param.clone(), bound));
      }
      match crate::syntax::arrow_body_of(arrow) {
        crate::syntax::ArrowBody::Block(b) => this.async_stmts(&b.body),
        crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          this.async_stmts(&[b.expr_stmt(b.copy(e))])
        }
      }
    })?;
    fields.push(Entry::Field("event".to_owned(), Expr::lit_str(event)));
    fields.push(Entry::Field("carry".to_owned(), carry));
    Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Listen.name().to_owned(), input: Expr::Object(fields) }))
  }

  /// `p.then(f)` in a terminal page: a fresh promise and a continuation on `p` running `f` with what `p` resolved to, which settles the fresh one with what `f` returns; one `p` rejects rejects it as well.
  pub(crate) fn then_call(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let (true, callee) = (self.reaches(Reach::Later), &call.callee) else { return Ok(None) };
    let Some(member) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(None) };
    if self.member_name(member).as_deref() != Some("then") || call.arguments.len() != 1 {
      return Ok(None);
    }
    let Some(js::Expression::ArrowFunctionExpression(f)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Ok(None) };
    let awaited = self.expr(member.object())?;
    let done = self.temp();
    let param = match crate::params_of(&f.params).as_slice() {
      [] => None,
      [crate::Param { pattern: js::BindingPattern::BindingIdentifier(name), default: None, rest: false, .. }] => Some(name.name.to_string()),
      _ => return Err(self.residue(call.span, "a `then` callback whose parameter is not one name")),
    };
    let finished = Expr::Var(done.clone());
    let (event, carry) = self.continuation(call.span, param, |this| {
      let stmts = match crate::syntax::arrow_body_of(f) {
        crate::syntax::ArrowBody::Block(b) => b.body,
        crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          b.stmts([b.return_stmt(Some(b.copy(e)), e.span())])
        }
      };
      let ran = this.async_to(stmts, Some(finished.clone()))?;
      let resumed = Expr::Var("$resume".to_owned());
      let rejected = Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), finished.clone()), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(false)))])), Entry::Field("value".to_owned(), resumed.clone().field("value"))]) };
      Ok(Expr::Ternary(Box::new(resumed.field("ok")), Box::new(ran), Box::new(Expr::Block(vec![rejected]))))
    })?;
    let then = Stmt::Act { action: snapfire_fsr_ir::host::Act::Then.name().to_owned(), input: Expr::Object(vec![Entry::Field("deferred".to_owned(), awaited), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]) };
    Ok(Some(Expr::Block(vec![Stmt::Let { name: done.clone(), expr: self.new_deferred() }, then, Stmt::Return(Expr::Var(done))])))
  }

  /// A fresh deferred, `{ "$deferred": id }`.
  pub(crate) fn new_deferred(&self) -> Expr {
    let id = Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] };
    Expr::Object(vec![Entry::Field(snapfire_fsr_ir::render::DEFERRED.to_owned(), id)])
  }

  /// Ink's `suspendTerminal(f)` in a terminal page: `f` run as an async function, then a [`snapfire_fsr_ir::host::Act::Suspend`] asking the terminal to give up the screen until `f`'s promise settles. The call's own promise settles as `f`'s does once the screen is back.
  pub(crate) fn suspend_call(&mut self, call: &js::CallExpression) -> Lowered<Option<Expr>> {
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return Ok(None) };
    if !self.reaches(Reach::Process) {
      return Ok(None);
    }
    let suspends = match self.scope.iter().rev().find(|(n, _)| *n == id.name.as_str()) {
      Some((_, bound)) => crate::terminal::primitive_ref(bound).as_deref() == Some("suspendTerminal"),
      None => self.meanings.as_ref().and_then(|meanings| meanings.of_ident(id)) == Some(&meaning::Meaning::Library(meaning::Construct::Suspend)),
    };
    if !suspends {
      return Ok(None);
    }
    let Some(js::Expression::ArrowFunctionExpression(f)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.residue(call.span, "`suspendTerminal` of something other than an arrow written in place")) };
    let inner = self.async_value(f)?;
    let (until, settles) = (self.temp(), self.temp());
    let suspend = Stmt::Act { action: snapfire_fsr_ir::host::Act::Suspend.name().to_owned(), input: Expr::Object(vec![Entry::Field("until".to_owned(), Expr::Var(until.clone())), Entry::Field("settles".to_owned(), Expr::Var(settles.clone()))]) };
    Ok(Some(Expr::Block(vec![Stmt::Let { name: until, expr: inner }, Stmt::Let { name: settles.clone(), expr: self.new_deferred() }, suspend, Stmt::Return(Expr::Var(settles))])))
  }

  /// An async arrow as a value: its statements run as an async function. The promise it returns settles with what the function returns; it rejects with what an awaited promise the function does not catch rejects with.
  pub(crate) fn async_value(&mut self, f: &js::ArrowFunctionExpression) -> Lowered<Expr> {
    let done = self.temp();
    let ran = match crate::syntax::arrow_body_of(f) {
      crate::syntax::ArrowBody::Block(b) => self.async_to(&b.body, Some(Expr::Var(done.clone())))?,
      crate::syntax::ArrowBody::Expr(e) => {
        let b = self.build();
        self.async_to(&[b.return_stmt(Some(b.copy(e)), e.span())], Some(Expr::Var(done.clone())))?
      }
    };
    Ok(Expr::Block(vec![Stmt::Let { name: done.clone(), expr: self.new_deferred() }, Stmt::Expr(ran), Stmt::Return(Expr::Var(done))]))
  }

  /// `(async () => { … })()`: an async arrow called where it is written, in a terminal page.
  pub(crate) fn async_iife<'c>(&self, call: &'c js::CallExpression<'c>) -> Option<&'c js::ArrowFunctionExpression<'c>> {
    let callee = &call.callee;
    match crate::meaning::bare(callee) {
      js::Expression::ArrowFunctionExpression(arrow) if self.reaches(Reach::Later) && arrow.r#async && call.arguments.is_empty() && crate::params_of(&arrow.params).is_empty() => Some(arrow),
      _ => None,
    }
  }

  /// An async function's statements run as far as its first `await`, which hands the awaited promise a continuation, [`snapfire_fsr_ir::host::Act::Then`], holding the rest. The rest lowers as a handler named [`snapfire_fsr_ir::host::Event::Resume`] in [`Lowerer::continuations`]; the names it reads that are not module cells are carried to it with the act.
  pub(crate) fn async_stmts(&mut self, stmts: &[js::Statement]) -> Lowered<Expr> {
    self.async_to(stmts, None)
  }

  /// [`Lowerer::async_stmts`] for a function whose promise is `done`: what it returns settles it; an awaited promise that rejects rejects it.
  pub(crate) fn async_to(&mut self, stmts: &[js::Statement], done: Option<Expr>) -> Lowered<Expr> {
    let floor = self.async_floor.replace(self.scope.len());
    let tail = self.loop_tail.take();
    let lowered = self.async_caught(stmts, done, None);
    self.async_floor = floor;
    self.loop_tail = tail;
    lowered
  }

  /// [`Lowerer::async_to`] inside `catch`, where an awaited promise among the statements it covers that rejects runs the `catch` instead.
  pub(crate) fn async_caught(&mut self, stmts: &[js::Statement], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let depth = self.scope.len();
    let lowered = self.async_to_in(stmts, done, catch);
    self.scope.truncate(depth);
    lowered
  }

  /// A loop that awaits. Its head is a continuation named for the loop, run through `$run` with the live locals carried: when the test holds it runs one pass, the body and the step, whose every way through ends by running the head again; otherwise it runs the statements after the loop. The locals the head reads are carried by value, so a `let` the body writes reaches the next pass and the statements after.
  pub(crate) fn awaiting_loop_in(&mut self, stmts: &[js::Statement], at: usize, looped: AwaitingLoop, done: Option<Expr>, inside: Option<AsyncCatch>) -> Lowered<Expr> {
    if looped.exits_deep {
      return Err(self.residue(looped.span, "a `break` inside a loop that awaits or a `continue` or `return` deeper than an `if` at the top of its body"));
    }
    if looped.over.as_ref().is_some_and(|over| self.kind_of(over) == Some(Kind::Map)) {
      return Err(self.residue(looped.span, "`for...of` over a `Map` in a loop that awaits; iterate `[...map.entries()]`"));
    }
    let before = self.before_await(&stmts[..at])?;
    let init = self.before_await(&looped.init)?;
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.parsed.file, crate::lo(looped.span))).name();
    // Every variable in scope is carried, the function's promise among them: a pass may read or write any of them and a copy of one it does not touch costs nothing.
    let mut carried: Vec<String> = self.scope.iter().filter_map(|(name, expr)| matches!(expr, Expr::Var(var) if var == name).then(|| name.clone())).collect();
    if let Some(done) = &done {
      done.free_vars(&mut carried);
    }
    carried.retain(|name| name != "$resume" && name != "$event" && !self.cells.iter().any(|(cell, _)| cell == name));
    carried.sort();
    carried.dedup();
    let carry = Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect());
    let depth = self.scope.len();
    let outer_tail = self.loop_tail.replace(LoopTail { event: event.clone(), carry: carry.clone(), done: done.clone() });
    let covered = inside.clone().map(|catch| AsyncCatch { covers: looped.pass.len() + catch.covers - at - 1, ..catch });
    let pass = self.expr(&looped.test).and_then(|cond| self.async_caught(&looped.pass, None, covered).map(|pass| (cond, pass)));
    self.loop_tail = outer_tail;
    let (cond, pass) = pass?;
    let after = inside.map(|catch| AsyncCatch { covers: catch.covers - at - 1, ..catch }).filter(|catch| catch.covers > 0);
    let rest = self.async_caught(&stmts[at + 1..], done, after)?;
    self.scope.truncate(depth);
    let resume = "$resume".to_owned();
    let mut body = vec![Stmt::Let { name: resume.clone(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("carry").field(name.clone()) });
    }
    body.push(Stmt::If { cond, then: vec![Stmt::Expr(pass)], r#else: vec![Stmt::Expr(rest)] });
    body.push(Stmt::Return(Expr::Object(Vec::new())));
    if !self.continuations.iter().any(|held| held.event == event) {
      self.continuations.push(snapfire_fsr_ir::ast::Handler { event: event.clone(), body });
    }
    let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]);
    let mut entry = Vec::new();
    splice(before, &mut entry);
    splice(init, &mut entry);
    entry.push(Stmt::Act { action: snapfire_fsr_ir::host::Act::Run.name().to_owned(), input });
    Ok(Expr::Block(entry))
  }

  /// The statements before an `await`, as `let`s and effects whose names stay in scope for the awaited value and the continuation.
  pub(crate) fn before_await(&mut self, stmts: &[js::Statement]) -> Lowered<Expr> {
    let mut out = Vec::with_capacity(stmts.len());
    for stmt in stmts {
      out.push(self.stmt(stmt)?);
    }
    Ok(Expr::Block(out))
  }

  pub(crate) fn async_to_in(&mut self, stmts: &[js::Statement], done: Option<Expr>, catch: Option<AsyncCatch>) -> Lowered<Expr> {
    let settle = |done: &Expr, ok: bool, value: Expr| Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input: Expr::Object(vec![Entry::Field("settle".to_owned(), Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::SETTLES.to_owned(), done.clone()), Entry::Field("ok".to_owned(), Expr::Lit(Lit::Bool(ok)))])), Entry::Field("value".to_owned(), value)]) };
    let in_pass = self.loop_tail.is_some();
    let Some(at) = stmts.iter().position(|s| awaits(s) || (in_pass && if_ends_in_return(s))) else {
      if let Some(LoopTail { event, carry, done: promise }) = self.loop_tail.clone() {
        // A `return` in the pass settles the function's promise with its value and the loop is done; any other end of the pass runs the head again.
        if let Some(at) = stmts.iter().position(|s| matches!(s, js::Statement::ReturnStatement(_))) {
          let js::Statement::ReturnStatement(ret) = &stmts[at] else { unreachable!("found a return") };
          let Some(promise) = promise else { return Err(self.residue(ret.span, "a `return` inside a loop that awaits, in a function with no promise to settle")) };
          let ran = self.before_await(&stmts[..at])?;
          let value = match &ret.argument {
            Some(arg) => self.expr(arg)?,
            None => Expr::Lit(Lit::Undefined),
          };
          return Ok(Expr::Block(vec![Stmt::Expr(ran), settle(&promise, true, value)]));
        }
        let ran = self.before_await(stmts)?;
        let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]);
        return Ok(Expr::Block(vec![Stmt::Expr(ran), Stmt::Act { action: snapfire_fsr_ir::host::Act::Run.name().to_owned(), input }]));
      }
      let floor = self.async_floor.unwrap_or(self.scope.len());
      let value = crate::component::block_to_expr_from(self, stmts, floor)?;
      return Ok(match &done {
        None => value,
        Some(done) => {
          let (returned, thrown) = (self.temp(), self.temp());
          let ran = vec![Stmt::Let { name: returned.clone(), expr: value }, settle(done, true, Expr::Var(returned))];
          Expr::Block(vec![Stmt::Try { body: ran, name: Some(thrown.clone()), handler: Some(vec![settle(done, false, Expr::Var(thrown))]), finally: Vec::new() }])
        }
      });
    };
    let inside = catch.filter(|catch| at < catch.covers);
    if let Some(looped) = awaiting_loop(self.parsed, &stmts[at]) {
      return self.awaiting_loop_in(stmts, at, looped, done, inside);
    }
    if let js::Statement::TryStatement(t) = &stmts[at] {
      if inside.is_some() {
        return Err(self.residue(t.span, "a `try` that awaits inside another"));
      }
      let b = self.build();
      let finally: Vec<js::Statement<'a>> = t.finalizer.iter().flat_map(|f| f.body.iter().map(|s| b.copy(s))).collect();
      if !finally.is_empty() && (t.block.body.iter().any(crate::component::holds_return) || t.handler.iter().any(|h| h.body.body.iter().any(crate::component::holds_return))) {
        return Err(self.residue(t.span, "a `return` inside a `try` whose `finally` follows an `await`"));
      }
      let before = self.before_await(&stmts[..at])?;
      let after: Vec<js::Statement<'a>> = finally.iter().map(|s| b.copy(s)).chain(stmts[at + 1..].iter().map(|s| b.copy(s))).collect();
      let rethrown = |covers: usize| {
        let name = format!("$finally{}", crate::lo(t.span));
        let thrown = js::Statement::new_throw_statement(t.span, b.ident(&name, t.span), &b.ast());
        AsyncCatch { param: Some(name), body: b.stmts(finally.iter().map(|s| b.copy(s)).chain(std::iter::once(thrown))), covers, inner: None }
      };
      let catch = match &t.handler {
        Some(handler) => {
          let param = match handler.param.as_ref().map(|p| &p.pattern) {
            None => None,
            Some(js::BindingPattern::BindingIdentifier(name)) => Some(name.name.to_string()),
            Some(other) => return Err(self.residue(other.span(), "a `catch` parameter that is not a name")),
          };
          let inner = (!finally.is_empty()).then(|| Box::new(rethrown(handler.body.body.len())));
          AsyncCatch { param, body: b.stmts(handler.body.body.iter().chain(&after).map(|s| b.copy(s))), covers: t.block.body.len(), inner }
        }
        None => rethrown(t.block.body.len()),
      };
      let body: Vec<js::Statement<'a>> = t.block.body.iter().chain(&after).map(|s| b.copy(s)).collect();
      let ran = self.async_caught(&body, done, Some(catch))?;
      return Ok(spliced(before, vec![Stmt::Expr(ran)]));
    }
    if let js::Statement::IfStatement(branch) = &stmts[at] {
      let before = self.before_await(&stmts[..at])?;
      let cond = self.expr(&branch.test)?;
      let b = self.build();
      let arm = |this: &mut Self, arm: Option<&js::Statement>| -> Lowered<Expr> {
        let mut joined: Vec<js::Statement<'a>> = match arm {
          Some(js::Statement::BlockStatement(block)) => block.body.iter().map(|s| b.copy(s)).collect(),
          Some(other) => vec![b.copy(other)],
          None => Vec::new(),
        };
        let covered = inside.clone().map(|catch| AsyncCatch { covers: joined.len() + catch.covers - at - 1, ..catch });
        joined.extend(stmts[at + 1..].iter().map(|s| b.copy(s)));
        this.async_caught(&joined, done.clone(), covered)
      };
      let then = arm(self, Some(&branch.consequent))?;
      let otherwise = arm(self, branch.alternate.as_ref())?;
      return Ok(spliced(before, vec![Stmt::If { cond, then: vec![Stmt::Expr(then)], r#else: vec![Stmt::Expr(otherwise)] }]));
    }
    let (awaited, bound, assigned) = match &stmts[at] {
      js::Statement::ExpressionStatement(e) => match crate::meaning::bare(&e.expression) {
        js::Expression::AwaitExpression(a) => (&a.argument, None, None),
        js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign && matches!(crate::meaning::bare(&assign.right), js::Expression::AwaitExpression(_)) => {
          let js::Expression::AwaitExpression(a) = crate::meaning::bare(&assign.right) else { unreachable!("matched an await") };
          let hidden = format!("$await{}", crate::lo(e.span));
          let b = self.build();
          let value = b.ident(&hidden, e.span);
          if let js::Expression::Identifier(id) = &value {
            self.parsed.scope.mark_top(id);
          }
          let assigned = b.expr_stmt(js::Expression::new_assignment_expression(assign.span, assign.operator, b.copy(&assign.left), value, &b.ast()));
          (&a.argument, Some(hidden), Some(assigned))
        }
        _ => return Err(self.residue(e.span, "an `await` inside an expression; await as a statement, a `const`'s value or an assignment")),
      },
      js::Statement::VariableDeclaration(var) => match (var.declarations.as_slice(), var.declarations.first().and_then(|d| d.init.as_ref()).map(crate::meaning::bare)) {
        ([decl], Some(js::Expression::AwaitExpression(a))) => match &decl.id {
          js::BindingPattern::BindingIdentifier(name) => (&a.argument, Some(name.name.to_string()), None),
          _ => return Err(self.residue(decl.span, "an awaited value bound to a pattern")),
        },
        _ => return Err(self.residue(var.span, "an `await` inside an expression; await as a statement, a `const`'s value or an assignment")),
      },
      other => return Err(self.residue(other.span(), "an `await` inside a `switch`, a `do...while`, a `for...in` or a bare block")),
    };
    let before = self.before_await(&stmts[..at])?;
    let deferred = self.expr(awaited)?;
    let prepended = assigned.is_some() as usize;
    let b = self.build();
    let joined: Vec<js::Statement<'a>> = assigned.into_iter().chain(stmts[at + 1..].iter().map(|s| b.copy(s))).collect();
    let rest = &joined[..];
    let (event, carry) = self.continuation(stmts[at].span(), bound, |this| {
      let resumed = Expr::Var("$resume".to_owned());
      let covered = inside.clone().map(|catch| AsyncCatch { covers: catch.covers + prepended - at - 1, ..catch }).filter(|catch| catch.covers > 0);
      let ran = this.async_caught(rest, done.clone(), covered)?;
      let rejected = match (&inside, &done) {
        (Some(catch), _) => {
          let depth = this.scope.len();
          if let Some(param) = &catch.param {
            this.scope.push((param.clone(), resumed.clone().field("value")));
          }
          let caught = this.async_caught(&catch.body, done.clone(), catch.inner.clone().map(|inner| *inner));
          this.scope.truncate(depth);
          caught?
        }
        (None, Some(done)) => Expr::Block(vec![settle(done, false, resumed.clone().field("value"))]),
        (None, None) => return Ok(ran),
      };
      Ok(Expr::Ternary(Box::new(resumed.field("ok")), Box::new(ran), Box::new(rejected)))
    })?;
    let then = Stmt::Act { action: snapfire_fsr_ir::host::Act::Then.name().to_owned(), input: Expr::Object(vec![Entry::Field("deferred".to_owned(), deferred), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]) };
    Ok(spliced(before, vec![then]))
  }

  /// Code the host runs later, the rest of an async function or a timer's callback, as a handler named [`snapfire_fsr_ir::host::Event::Resume`] for `at` in [`Lowerer::continuations`]. `bound` names the settled value it is handed. Every other name it reads that is not a module cell is carried to it. Returns the event and the carry the act that schedules it passes.
  pub(crate) fn continuation(&mut self, at: Span, bound: Option<String>, lower: impl FnOnce(&mut Self) -> Lowered<Expr>) -> Lowered<(String, Expr)> {
    let resume = "$resume".to_owned();
    let depth = self.scope.len();
    // The settled value is a variable of the continuation, not the expression `$resume.value`: a later continuation carries the variable, where the expression would read its own settled value.
    if let Some(name) = &bound {
      self.scope.push((name.clone(), Expr::Var(name.clone())));
    }
    let rest = lower(self);
    self.scope.truncate(depth);
    let rest = rest?;
    let mut free = Vec::new();
    rest.free_vars(&mut free);
    free.sort();
    free.dedup();
    let carried: Vec<String> = free.into_iter().filter(|name| *name != resume && bound.as_ref() != Some(name) && !self.cells.iter().any(|(cell, _)| cell == name)).collect();
    let mut body = vec![Stmt::Let { name: resume.clone(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("carry").field(name.clone()) });
    }
    if let Some(name) = &bound {
      body.push(Stmt::Let { name: name.clone(), expr: Expr::Var(resume.clone()).field("value") });
    }
    body.push(Stmt::Expr(rest));
    body.push(Stmt::Return(Expr::Object(Vec::new())));
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.parsed.file, crate::lo(at))).name();
    if !self.continuations.iter().any(|held| held.event == event) {
      self.continuations.push(snapfire_fsr_ir::ast::Handler { event: event.clone(), body });
    }
    let carry = Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), Expr::Var(name.clone()))).collect());
    Ok((event, carry))
  }

  /// In a promise's executor, `resolve(v)` or `reject(e)` as an [`snapfire_fsr_ir::host::Act::Settle`] and `setTimeout(resolve, ms)` as a timer that settles; `None` for any other call.
  pub(crate) fn settling(&mut self, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let callee = &call.callee;
    let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return Ok(None) };
    let settler = |this: &Self, e: &js::Expression| matches!(crate::meaning::bare(e), js::Expression::Identifier(id) if this.settlers.iter().any(|s| *s == id.name.as_str()));
    if settler(self, callee) {
      let value = match call.arguments.first() {
        Some(arg) => self.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      let settle = self.expr(callee)?;
      let input = Expr::Object(vec![Entry::Field("settle".to_owned(), settle), Entry::Field("value".to_owned(), value)]);
      return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Settle.name().to_owned(), input }));
    }
    if id.name.as_str() == "setTimeout" && self.parsed.ctxt(id) == crate::Ctxt::Unresolved && self.reaches(Reach::Later) {
      if let [callback, delay] = call.arguments.as_slice() {
        if settler(self, crate::syntax::arg_expr(callback)) {
          let input = Expr::Object(vec![Entry::Field("settle".to_owned(), self.expr(crate::syntax::arg_expr(callback))?), Entry::Field("ms".to_owned(), self.expr(crate::syntax::arg_expr(delay))?)]);
          return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input }));
        }
        if let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(crate::syntax::arg_expr(callback)) {
          let ms = self.expr(crate::syntax::arg_expr(delay))?;
          let (event, carry) = self.continuation(call.span, None, |this| match crate::syntax::arrow_body_of(arrow) {
            crate::syntax::ArrowBody::Block(b) => this.async_stmts(&b.body),
            crate::syntax::ArrowBody::Expr(e) => {
          let b = this.build();
          this.async_stmts(&[b.expr_stmt(b.copy(e))])
        }
          })?;
          let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("ms".to_owned(), ms), Entry::Field("repeats".to_owned(), Expr::Lit(Lit::Bool(false))), Entry::Field("carry".to_owned(), carry)]);
          return Ok(Some(Stmt::Act { action: snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input }));
        }
      }
    }
    Ok(None)
  }
}
