use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  pub(super) fn handler_body(&mut self, e: &'p js::Expression<'p>) -> Lowered<Vec<Stmt>> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.handler_body(&p.expression),
      js::Expression::ArrowFunctionExpression(arrow) => {
        let params = crate::params_of(&arrow.params);
        let body = arrow_body(arrow);
        self.handler_fn(&params, body, arrow.span)
      }
      js::Expression::Identifier(id) => {
        let name = id.name.to_string();
        let Some(f) = self.handler_fns.get(&name).cloned() else {
          return Err(self.lowerer.residue(id.span, format!("`{name}` is not a handler this component declares")));
        };
        self.in_hook(f.hook, |this| this.handler_fn(&f.params, f.body, id.span))
      }
      other => Err(self.lowerer.residue(other.span(), "a handler that is not an arrow or a name")),
    }
  }

  pub(super) fn handler_fn(&mut self, params: &[crate::Param], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let hoisting = self.lowerer.hoisting.take();
    let was_handler = std::mem::replace(&mut self.lowerer.in_handler, true);
    let was_after = std::mem::replace(&mut self.lowerer.after_body, true);
    let result = self.handler_fn_in(params, body, span);
    self.lowerer.hoisting = hoisting;
    self.lowerer.in_handler = was_handler;
    self.lowerer.after_body = was_after;
    result
  }

  pub(super) fn handler_fn_in(&mut self, params: &[crate::Param], body: FunctionBody<'p>, span: Span) -> Lowered<Vec<Stmt>> {
    let mut walk = HandlerWalk::default();
    self.handler_fn_into(params, body, &mut walk)?;
    self.patch_module_lets(&mut walk);
    if walk.patch.is_empty() && !holds_effect(&walk.out) {
      return Err(self.lowerer.residue(span, NO_EFFECT));
    }
    walk.out.push(Stmt::Return(Expr::Object(walk.patch)));
    Ok(walk.out)
  }

  /// A module-level `let` a function the handler calls writes is read back into the patch, so the step writes the store key that holds it.
  pub(super) fn patch_module_lets(&mut self, walk: &mut HandlerWalk) {
    let mut written = Vec::new();
    let mut collect = |e: &Expr| {
      for name in crate::outer_set_targets(e) {
        if !written.contains(&name) {
          written.push(name);
        }
      }
    };
    snapfire_fsr_ir::ast::body_visit(&walk.out, &mut collect);
    for entry in &walk.patch {
      if let Entry::Field(_, value) = entry {
        value.visit(&mut collect);
      }
    }
    for name in written {
      if self.module_lets.iter().any(|(n, _, _)| *n == name) && !walk.patch.iter().any(|e| matches!(e, Entry::Field(n, _) if *n == name)) {
        walk.patch.push(Entry::Field(name.clone(), Expr::Var(name)));
      }
    }
  }

  /// Runs `lower` inside the custom hook call that declared the function it lowers, so the function's body reads the hook's module, parameters, locals and renamed names. A function no hook declared lowers where it stands.
  pub(super) fn in_hook<R>(&mut self, hook: Option<Rc<HookScope<'p>>>, lower: impl FnOnce(&mut Self) -> R) -> R {
    let Some(hook) = hook else { return lower(self) };
    let parsed = std::mem::replace(&mut self.lowerer.parsed, hook.parsed);
    let meanings = std::mem::replace(&mut self.lowerer.meanings, hook.meanings.clone());
    let (scope, below, renames) = (self.lowerer.scope.len(), self.lowerer.declared_below.len(), self.hook_renames.len());
    self.lowerer.scope.extend(hook.scope.iter().cloned());
    self.lowerer.declared_below.extend(hook.declared_below.iter().cloned());
    self.hook_renames.extend(hook.renames.iter().cloned());
    let result = lower(self);
    self.lowerer.parsed = parsed;
    self.lowerer.meanings = meanings;
    self.lowerer.scope.truncate(scope);
    self.lowerer.declared_below.truncate(below);
    self.hook_renames.truncate(renames);
    result
  }

  /// A handler's statements into `walk`, under the reach it holds: an inlined handler runs under the branch that called it.
  pub(super) fn handler_fn_into(&mut self, params: &[crate::Param], body: FunctionBody<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let binds = self.event_binds.take().unwrap_or_else(|| vec![Expr::Var("$event".to_owned())]);
    for (at, param) in params.iter().enumerate() {
      let bind = binds.get(at).cloned().unwrap_or(Expr::Lit(Lit::Undefined));
      match (param.pattern, param.default, param.rest) {
        (js::BindingPattern::BindingIdentifier(id), None, false) => self.lowerer.scope.push((id.name.to_string(), bind)),
        (_, Some(_), false) => bind_param(&mut self.lowerer, param, bind)?,
        _ => return Err(self.lowerer.residue(param.span, "a handler's event parameter must be a name")),
      }
    }
    let (spawned, cells) = (self.lowerer.spawned.len(), self.lowerer.cells.len());
    let result = match body {
      FunctionBody::Expr(e) => self.handler_stmt(e, walk),
      FunctionBody::Block(stmts) if self.lowerer.reaches(Reach::Later) && stmts.iter().any(crate::awaits) => self.handler_async(stmts, walk),
      FunctionBody::Block(stmts) => self.handler_block(stmts, walk).map(|_| ()),
    };
    self.lowerer.scope.truncate(depth);
    self.lowerer.spawned.truncate(spawned);
    self.lowerer.cells.truncate(cells);
    result
  }

  /// Statements of a handler or of one of its branches. True when the block ended in a bare `return`, which makes what follows the branch reachable only when the branch was not taken.
  pub(super) fn handler_block(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk) -> Lowered<bool> {
    let mut declared: Vec<(String, Option<HandlerFn<'p>>)> = Vec::new();
    let result = self.handler_stmts(stmts, walk, &mut declared);
    for (name, outer) in declared.into_iter().rev() {
      match outer {
        Some(f) => self.handler_fns.insert(name, f),
        None => self.handler_fns.remove(&name),
      };
    }
    result
  }

  /// [`Self::handler_block`]'s statements; a `const` arrow the rest calls by name is a function the calls inline, recorded in `declared` with what it shadowed.
  pub(super) fn handler_stmts(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, declared: &mut Vec<(String, Option<HandlerFn<'p>>)>) -> Lowered<bool> {
    for (at, stmt) in stmts.iter().enumerate() {
      match stmt {
        js::Statement::ExpressionStatement(e) => self.handler_stmt(&e.expression, walk)?,
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let js::BindingPattern::BindingIdentifier(name) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring in a handler")) };
            let local = name.name.to_string();
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = decl.init.as_ref().map(crate::meaning::bare) {
              if var.kind == js::VariableDeclarationKind::Const && called_names(&stmts[at + 1..]).contains(&local) && !listened_names(&stmts[at + 1..]).contains(&local) {
                let outer = self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
                declared.push((local, outer));
                continue;
              }
            }
            let listener = match decl.init.as_ref().map(crate::meaning::bare) {
              Some(js::Expression::ArrowFunctionExpression(arrow)) if self.lowerer.reaches(Reach::Process) && listened_names(&stmts[at + 1..]).contains(&local) => Some(arrow),
              _ => None,
            };
            let expr = match (decl.init.as_ref(), listener) {
              (Some(_), Some(arrow)) => self.named_listener(&crate::params_of(&arrow.params), arrow_body(arrow), decl.span)?,
              (Some(init), None) => match self.arrow_timer(init) {
                Some((repeats, callback, delay)) => self.handler_timer(repeats, callback, delay, decl.span, walk)?,
                None => self.lowerer.expr(init)?,
              },
              (None, _) => Expr::Lit(Lit::Null),
            };
            if let Some(js::Expression::CallExpression(call)) = decl.init.as_ref().map(crate::meaning::bare) {
              if self.lowerer.reaches(Reach::Process) && self.lowerer.spawns(call) {
                self.lowerer.spawned.push(local.clone());
              }
            }
            // A local a listener or a continuation writes lasts past this step, so it is a cell under a key fresh to this run.
            if var.kind != js::VariableDeclarationKind::Const && written_names(&stmts[at + 1..], true).contains(&local) {
              let key = self.lowerer.temp();
              walk.out.push(Stmt::Let { name: key.clone(), expr: Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] } });
              let handle = Expr::ModuleCell { key: Box::new(Expr::Var(key)), init: Box::new(Expr::Lit(Lit::Null)) };
              walk.effect(Stmt::Expr(Expr::WriteCell(Box::new(handle.clone()), Box::new(expr))));
              self.lowerer.cells.push((local.clone(), handle.clone()));
              self.lowerer.scope.push((local, Expr::ReadCell(Box::new(handle))));
              continue;
            }
            let (name, expr) = match &walk.reach {
              None => (local.clone(), expr),
              Some(reach) => {
                walk.locals += 1;
                (format!("{local}${}", walk.locals), Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))))
              }
            };
            if let Some(arrow) = listener {
              self.named_listeners.push((name.clone(), !arrow.params.is_empty()));
            }
            self.lowerer.scope.push((local, Expr::Var(name.clone())));
            walk.out.push(Stmt::Let { name, expr });
          }
        }
        js::Statement::FunctionDeclaration(f) if f.body.is_some() => {
          let local = function_name(&stmts[at]).unwrap_or_default().to_owned();
          let params: Vec<crate::Param<'p>> = crate::params_of(&f.params);
          let body = FunctionBody::Block(&f.body.as_ref().expect("checked").statements);
          if !(self.lowerer.reaches(Reach::Process) && listened_names(&stmts[at + 1..]).contains(&local)) {
            let outer = self.handler_fns.insert(local.clone(), (params, body).into());
            declared.push((local, outer));
            continue;
          }
          let expr = self.named_listener(&params, body, f.span)?;
          let (name, expr) = match &walk.reach {
            None => (local.clone(), expr),
            Some(reach) => {
              walk.locals += 1;
              (format!("{local}${}", walk.locals), Expr::Ternary(Box::new(reach.clone()), Box::new(expr), Box::new(Expr::Lit(Lit::Null))))
            }
          };
          self.named_listeners.push((name.clone(), !params.is_empty()));
          self.lowerer.scope.push((local, Expr::Var(name.clone())));
          walk.out.push(Stmt::Let { name, expr });
        }
        js::Statement::ReturnStatement(r) if r.argument.as_ref().is_none_or(is_undefined) => return Ok(true),
        js::Statement::ReturnStatement(ret) if ret.argument.is_some() && self.cleanup_for.is_some() => {
          let (Some(cleanup), span) = (&ret.argument, &ret.span) else { unreachable!("checked") };
          let effect = self.cleanup_for.clone().expect("checked");
          let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(cleanup) else { return Err(self.lowerer.residue(*span, "an effect's cleanup that is not an arrow written in place")) };
          let (event, carry) = self.arrow_continuation(arrow, Vec::new(), *span)?;
          walk.act(snapfire_fsr_ir::host::Act::Cleanup.name().to_owned(), Expr::Object(vec![Entry::Field("effect".to_owned(), Expr::lit_str(effect)), Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]));
          return Ok(true);
        }
        js::Statement::IfStatement(branch) => self.handler_if(branch, walk)?,
        js::Statement::TryStatement(t) => {
          if self.handler_try(t, walk)? {
            return Ok(true);
          }
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")),
      }
    }
    Ok(false)
  }

  /// An async function a terminal handler runs, `void (async () => { … })()`: its statements run in the handler as far as the first `await`, which hands the awaited promise a continuation, a handler of the component named [`snapfire_fsr_ir::host::Event::Resume`] holding the rest. The names the rest reads that the component does not hold itself are carried to it, the handler's own `$event` among them.
  pub(super) fn handler_async(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk) -> Lowered<()> {
    self.async_within(stmts, walk, &Within::default())
  }

  /// `stmts` run as an async function's are, inside `within`. An `if` holding an `await` runs each branch under its condition, the statements after it a join each branch runs once it is done; a `try` holding one runs the statements after it as a join too, its `catch` where a promise inside rejects.
  pub(super) fn async_within(&mut self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, within: &Within<'p>) -> Lowered<()> {
    let Some(at) = stmts.iter().position(crate::awaits) else {
      self.handler_block(stmts, walk)?;
      run_join(within.then.as_ref(), walk);
      return Ok(());
    };
    self.handler_block(&stmts[..at], walk)?;
    let rest = &stmts[at + 1..];
    match &stmts[at] {
      js::Statement::IfStatement(branch) => {
        let inner = Within { catch: within.catch.clone(), then: self.join(rest, within)? };
        let cond = self.lowerer.expr(&branch.test)?;
        let cond = self.branch_cond(cond, walk);
        let outer = walk.reach.clone();
        let depth = self.lowerer.scope.len();
        walk.reach = Some(reach_under(&outer, cond.clone()));
        let then = self.async_within(arm(&branch.consequent), walk, &inner);
        self.lowerer.scope.truncate(depth);
        then?;
        walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond))));
        let otherwise = match &branch.alternate {
          Some(alt) => self.async_within(arm(alt), walk, &inner),
          None => {
            run_join(inner.then.as_ref(), walk);
            Ok(())
          }
        };
        self.lowerer.scope.truncate(depth);
        walk.reach = outer;
        otherwise
      }
      js::Statement::TryStatement(t) => {
        let mut after = self.join(rest, within)?;
        if let Some(finalizer) = &t.finalizer {
          if t.block.body.iter().chain(t.handler.iter().flat_map(|c| &c.body.body)).any(holds_return) {
            return Err(self.lowerer.residue(t.span, "a `return` inside a `try` whose `finally` follows an `await`"));
          }
          after = self.join(&finalizer.body, &Within { catch: within.catch.clone(), then: after })?;
        }
        let caught = match (&t.handler, &t.finalizer) {
          (Some(catch), _) => {
            let param = match catch.param.as_ref().map(|p| &p.pattern) {
              Some(js::BindingPattern::BindingIdentifier(name)) => Some(name.name.to_string()),
              None => None,
              Some(other) => return Err(self.lowerer.residue(other.span(), "a `catch` parameter that is not a name")),
            };
            Some(Catch { param, body: &catch.body.body[..], after: after.clone() })
          }
          (None, Some(_)) if within.catch.is_some() => return Err(self.lowerer.residue(t.span, "a `try` with only a `finally` inside another `try`, around an `await`")),
          (None, Some(finalizer)) => Some(Catch { param: None, body: &finalizer.body[..], after: None }),
          (None, None) => within.catch.clone(),
        };
        let inner = Within { catch: caught, then: after };
        self.async_within(&t.block.body, walk, &inner)
      }
      stmt => {
        let (awaited, bound) = match stmt {
          js::Statement::ExpressionStatement(e) => match crate::meaning::bare(&e.expression) {
            js::Expression::AwaitExpression(a) => (&a.argument, None),
            _ => return Err(self.lowerer.residue(e.span, "an `await` inside an expression of a handler; await as a statement or a `const`'s value")),
          },
          js::Statement::VariableDeclaration(var) => match (var.declarations.as_slice(), var.declarations.first().and_then(|d| d.init.as_ref()).map(crate::meaning::bare)) {
            ([decl], Some(js::Expression::AwaitExpression(a))) => match &decl.id {
              js::BindingPattern::BindingIdentifier(name) => (&a.argument, Some(name.name.to_string())),
              _ => return Err(self.lowerer.residue(decl.span, "an awaited value bound to a pattern")),
            },
            _ => return Err(self.lowerer.residue(var.span, "an `await` inside an expression of a handler; await as a statement or a `const`'s value")),
          },
          other => return Err(self.lowerer.residue(other.span(), "an `await` inside a loop of a handler")),
        };
        let deferred = self.lowerer.expr(awaited)?;
        let resumed = Expr::Var(RESUME.to_owned());
        let mut cw = HandlerWalk::default();
        let depth = self.lowerer.scope.len();
        let captured = self.capture_states();
        // The settled value is a variable the continuation declares, so a continuation after the next await carries it instead of reading that await's value.
        if let Some(name) = &bound {
          cw.out.push(Stmt::Let { name: name.clone(), expr: resumed.clone().field("value") });
          self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        }
        let lowered = (|| -> Lowered<()> {
          let ok = resumed.clone().field("ok");
          if let Some(catch) = &within.catch {
            cw.reach = Some(Expr::Not(Box::new(ok.clone())));
            if let Some(param) = &catch.param {
              self.lowerer.scope.push((param.clone(), resumed.clone().field("value")));
            }
            self.handler_block(catch.body, &mut cw)?;
            run_join(catch.after.as_ref(), &mut cw);
          }
          cw.reach = Some(ok);
          self.async_within(rest, &mut cw, within)
        })();
        self.lowerer.scope.truncate(depth);
        lowered?;
        let (event, carry) = self.continuation_handler(stmt.span(), cw, &captured);
        let then = |field: &str, value: Expr| Stmt::Act {
          action: snapfire_fsr_ir::host::Act::Then.name().to_owned(),
          input: Expr::Object(vec![Entry::Field(field.to_owned(), value), Entry::Field("event".to_owned(), Expr::lit_str(event.clone())), Entry::Field("carry".to_owned(), carry.clone())]),
        };
        // A call that fails as it is awaited is a rejected promise, so a `catch` around the `await` runs.
        match within.catch {
          None => walk.effect(then("deferred", deferred)),
          Some(_) => walk.effect(Stmt::Try { body: vec![then("deferred", deferred)], name: Some(REJECTED.to_owned()), handler: Some(vec![then("rejected", Expr::Var(REJECTED.to_owned()))]), finally: Vec::new() }),
        }
        Ok(())
      }
    }
  }

  /// The statements after an `if` or a `try` that awaits, run inside `within` as a continuation every way through it ends with; `within`'s own join when there are none.
  pub(super) fn join(&mut self, rest: &'p [js::Statement], within: &Within<'p>) -> Lowered<Option<Join>> {
    if rest.is_empty() {
      return Ok(within.then.clone());
    }
    let mut cw = HandlerWalk::default();
    let depth = self.lowerer.scope.len();
    let captured = self.capture_states();
    let lowered = self.async_within(rest, &mut cw, within);
    self.lowerer.scope.truncate(depth);
    lowered?;
    let (event, carry) = self.continuation_handler(rest[0].span(), cw, &captured);
    Ok(Some(Join { event, carry }))
  }

  /// A listener on a process the handler spawned as the act [`snapfire_fsr_ir::host::Act::Listen`], its callback a continuation of the component with `code` and `signal` bound from an exit's report; a promise's `resolve` or `reject` as the callback settles that promise instead; a function the effect declares runs as the handler it already is.
  pub(super) fn handler_listen(&mut self, call: &'p js::CallExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let (process, report, callback, removes) = self.lowerer.listener(call).expect("checked");
    let Some(report) = report else { return Ok(()) };
    let Some(callback) = callback else { return Err(self.lowerer.residue(call.span, "a process listener without a function")) };
    let mut fields = vec![Entry::Field("process".to_owned(), self.lowerer.expr(process)?), Entry::Field("report".to_owned(), Expr::lit_str(report.name()))];
    let exits = matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close);
    let effect_fn = match crate::meaning::bare(callback) {
      js::Expression::Identifier(id) => self.effect_fns.iter().find(|(f, _)| *f == id.name.as_str()).map(|(_, event)| event.clone()),
      _ => None,
    };
    let named = match (effect_fn, crate::meaning::bare(callback)) {
      (Some(event), _) => Some((Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), Expr::Object(Vec::new()))]), false)),
      (None, js::Expression::Identifier(_)) => match self.lowerer.expr(callback)? {
        Expr::Var(var) => self.named_listeners.iter().find(|(name, _)| *name == var).map(|(_, takes)| (Expr::Var(var.clone()), *takes)),
        _ => None,
      },
      _ => None,
    };
    if removes {
      let Some((named, _)) = named else { return Err(self.lowerer.residue(callback.span(), "an `off` whose function is not a `const` arrow of the handler that passes it to `on` or a function its effect declares")) };
      fields.push(Entry::Field("event".to_owned(), named.field("event")));
      walk.act(snapfire_fsr_ir::host::Act::Unlisten.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    if let Some((named, takes)) = named {
      if exits && takes {
        return Err(self.lowerer.residue(callback.span(), "an `exit` or `close` listener taking parameters that is not written in place"));
      }
      fields.push(Entry::Field("event".to_owned(), named.clone().field("event")));
      fields.push(Entry::Field("carry".to_owned(), named.field("carry")));
      walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    if matches!(crate::meaning::bare(callback), js::Expression::Identifier(id) if self.lowerer.settlers.iter().any(|s| *s == id.name.as_str())) {
      fields.push(Entry::Field("settle".to_owned(), self.lowerer.expr(callback)?));
      walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
      return Ok(());
    }
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(callback) else { return Err(self.lowerer.residue(callback.span(), "a process listener that is not an arrow written in place, an arrow a `const` names or a promise's `resolve` or `reject`")) };
    let value = Expr::Var(RESUME.to_owned()).field("value");
    let mut binds = Vec::new();
    for (i, param) in crate::params_of(&arrow.params).iter().enumerate() {
      let Some(name) = param.ident() else { return Err(self.lowerer.residue(param.span, "a process listener's parameter that is not a name")) };
      let bound = match (exits, i) {
        (true, 0) => value.clone().field("code"),
        (true, _) => value.clone().field("signal"),
        (false, _) => value.clone(),
      };
      binds.push((name.name.to_string(), bound));
    }
    let (event, carry) = self.arrow_continuation(arrow, binds, call.span)?;
    fields.push(Entry::Field("event".to_owned(), Expr::lit_str(event)));
    fields.push(Entry::Field("carry".to_owned(), carry));
    walk.act(snapfire_fsr_ir::host::Act::Listen.name().to_owned(), Expr::Object(fields));
    Ok(())
  }

  /// A `const` arrow or a function declaration a terminal handler passes to `on` or `off` as the value `{ event, carry }` of the continuation it is, its parameters bound to what the report carries.
  pub(super) fn named_listener(&mut self, params: &[crate::Param], body: FunctionBody<'p>, at: Span) -> Lowered<Expr> {
    let value = Expr::Var(RESUME.to_owned()).field("value");
    let mut binds = Vec::new();
    for param in params {
      let Some(name) = param.ident() else { return Err(self.lowerer.residue(param.span, "a process listener's parameter that is not a name")) };
      binds.push((name.name.to_string(), value.clone()));
    }
    let (event, carry) = self.body_continuation(body, binds, at)?;
    Ok(Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("carry".to_owned(), carry)]))
  }

  /// `arrow`'s body as a continuation of the component with `binds` in scope: its event and the carry the act that runs it passes.
  pub(super) fn arrow_continuation(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>, binds: Vec<(String, Expr)>, at: Span) -> Lowered<(String, Expr)> {
    self.body_continuation(arrow_body(arrow), binds, at)
  }

  /// Binds each name that reads a state value to [`CAPTURED`] and the value, so a continuation lowered under it reads the value the render that ran the act held, as a closure holds the render's `const`. An updater and a reducer still apply to the state as it is. Returns each captured name with what the act carries in for it.
  pub(super) fn capture_states(&mut self) -> Vec<(String, Expr)> {
    let states: Vec<String> = self.setters.iter().map(|(_, state)| state.clone()).chain(self.dispatchers.iter().map(|(_, state, _)| state.clone())).collect();
    let mut captured: Vec<(String, Expr)> = Vec::new();
    let mut shadows: Vec<(String, Expr)> = Vec::new();
    for (at, (name, bound)) in self.lowerer.scope.iter().enumerate() {
      let latest = !self.lowerer.scope[at + 1..].iter().any(|(later, _)| later == name);
      if let (true, Expr::Var(state)) = (latest, bound) {
        if states.contains(state) {
          let held = format!("{CAPTURED}{state}");
          if !captured.iter().any(|(c, _)| *c == held) {
            captured.push((held.clone(), Expr::Var(state.clone())));
          }
          shadows.push((name.clone(), Expr::Var(held)));
        }
      }
    }
    self.lowerer.scope.extend(shadows);
    captured
  }

  /// A function's `body` as a continuation of the component with `binds` in scope: its event and the carry the act that runs it passes.
  pub(super) fn body_continuation(&mut self, body: FunctionBody<'p>, binds: Vec<(String, Expr)>, at: Span) -> Lowered<(String, Expr)> {
    let depth = self.lowerer.scope.len();
    let captured = self.capture_states();
    self.lowerer.scope.extend(binds);
    let cleanup_for = self.cleanup_for.take();
    let mut cw = HandlerWalk::default();
    let lowered = match body {
      FunctionBody::Block(stmts) => self.async_within(stmts, &mut cw, &Within::default()),
      FunctionBody::Expr(e) => self.handler_stmt(e, &mut cw),
    };
    self.cleanup_for = cleanup_for;
    self.lowerer.scope.truncate(depth);
    lowered?;
    Ok(self.continuation_handler(at, cw, &captured))
  }

  /// `setInterval(f, ms)` or `setTimeout(f, ms)` in a terminal handler with `f` an arrow written in place.
  pub(super) fn arrow_timer(&self, e: &'p js::Expression<'p>) -> Option<(bool, &'p js::ArrowFunctionExpression<'p>, &'p js::Expression<'p>)> {
    let (repeats, callback, delay) = timer_call(e).filter(|_| self.lowerer.reaches(Reach::Later))?;
    match crate::meaning::bare(callback) {
      js::Expression::ArrowFunctionExpression(arrow) => Some((repeats, arrow, delay)),
      _ => None,
    }
  }

  /// A timer a handler starts on an arrow, [`Self::arrow_timer`]: the act [`snapfire_fsr_ir::host::Act::Timer`] with a fresh id and the arrow as a continuation. Its value is `{ "$timer": id }`, [`snapfire_fsr_ir::host::TIMER`].
  pub(super) fn handler_timer(&mut self, repeats: bool, callback: &'p js::ArrowFunctionExpression<'p>, delay: &'p js::Expression<'p>, at: Span, walk: &mut HandlerWalk) -> Lowered<Expr> {
    let continuation = self.arrow_continuation(callback, Vec::new(), at)?;
    self.start_timer(repeats, continuation, delay, walk)
  }

  /// The act [`snapfire_fsr_ir::host::Act::Timer`] with a fresh id, running `continuation`, its event and carry, after `delay`. Its value is `{ "$timer": id }`.
  pub(super) fn start_timer(&mut self, repeats: bool, (event, carry): (String, Expr), delay: &'p js::Expression<'p>, walk: &mut HandlerWalk) -> Lowered<Expr> {
    let id = self.lowerer.temp();
    walk.out.push(Stmt::Let { name: id.clone(), expr: Expr::Ext { module: "crypto".to_owned(), name: "random".to_owned(), args: vec![Expr::Lit(Lit::Int(8))] } });
    let ms = self.lowerer.expr(delay)?;
    let input = vec![("id", Expr::Var(id.clone())), ("event", Expr::lit_str(event)), ("carry", carry), ("ms", ms), ("repeats", Expr::Lit(Lit::Bool(repeats)))];
    walk.act(snapfire_fsr_ir::host::Act::Timer.name().to_owned(), Expr::Object(input.into_iter().map(|(key, value)| Entry::Field(key.to_owned(), value)).collect()));
    Ok(Expr::Object(vec![Entry::Field(snapfire_fsr_ir::host::TIMER.to_owned(), Expr::Var(id))]))
  }

  /// `walk` as a handler of the component named [`snapfire_fsr_ir::host::Event::Resume`] for `at`: what it was run with bound as [`RESUME`], every name it reads that the component does not hold carried in, each of `captured` as the expression beside it. Returns the event and the carry the act that runs it passes.
  pub(super) fn continuation_handler(&mut self, at: Span, walk: HandlerWalk, captured: &[(String, Expr)]) -> (String, Expr) {
    let mut body = walk.out;
    body.push(Stmt::Return(Expr::Object(walk.patch)));
    let held: Vec<String> = self.state.iter().chain(self.state_bindings.iter()).cloned().chain(["$state".to_owned(), "$props".to_owned(), RESUME.to_owned()]).chain(self.lowerer.cells.iter().map(|(name, _)| name.clone())).collect();
    let carried: Vec<String> = snapfire_fsr_ir::body_free_vars(&body).into_iter().filter(|name| !held.contains(name)).collect();
    let resumed = Expr::Var(RESUME.to_owned());
    let mut handler = vec![Stmt::Let { name: RESUME.to_owned(), expr: Expr::Var("$event".to_owned()) }];
    for name in &carried {
      handler.push(Stmt::Let { name: name.clone(), expr: resumed.clone().field("carry").field(name.clone()) });
    }
    handler.extend(body);
    let event = snapfire_fsr_ir::host::Event::Resume(format!("{}@{}", self.lowerer.parsed.file, crate::lo(at))).name();
    if !self.lowered_handlers.iter().any(|held| held.event == event) {
      self.lowered_handlers.push(Handler { event: event.clone(), body: handler });
    }
    let carry = |name: &String| captured.iter().find(|(held, _)| held == name).map_or_else(|| Expr::Var(name.clone()), |(_, value)| value.clone());
    (event, Expr::Object(carried.iter().map(|name| Entry::Field(name.clone(), carry(name))).collect()))
  }

  /// `if`, `else` and `else if`: each branch lowers under its condition joined to the reach around it and the names it declares go out of scope with it. A branch that returns leaves the rest of the handler reachable only when the other one ran.
  /// `c ? a() : b();` as a statement: each arm runs under its side of the condition, as `if (c) a(); else b();` does.
  pub(super) fn handler_cond(&mut self, cond: &'p js::ConditionalExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let test = self.lowerer.expr(&cond.test)?;
    let test = self.branch_cond(test, walk);
    let outer = walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.reach = Some(reach_under(&outer, test.clone()));
    self.handler_stmt(&cond.consequent, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(test))));
    self.handler_stmt(&cond.alternate, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    walk.reach = outer;
    Ok(())
  }

  pub(super) fn handler_if(&mut self, branch: &'p js::IfStatement<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    let mut updates = Vec::new();
    let b = self.lowerer.build();
    let test = counted_test(b, &branch.test, &|name| self.effect_locals.iter().any(|(local, _)| local == name), &mut updates);
    let cond = self.lowerer.expr(&test)?;
    for (name, op, span) in updates {
      let value = self.lowerer.expr(&b.binary(b.ident(&name, span), op, b.number(1.0, span), span))?;
      self.effect_write(&name, value, walk);
    }
    let cond = self.branch_cond(cond, walk);
    let outer = walk.reach.clone();
    let depth = self.lowerer.scope.len();
    walk.reach = Some(reach_under(&outer, cond.clone()));
    let then_returned = self.handler_branch(&branch.consequent, walk)?;
    self.lowerer.scope.truncate(depth);
    self.reread_effect_locals(walk);
    let mut else_returned = false;
    if let Some(alt) = &branch.alternate {
      walk.reach = Some(reach_under(&outer, Expr::Not(Box::new(cond.clone()))));
      else_returned = self.handler_branch(alt, walk)?;
      self.lowerer.scope.truncate(depth);
      self.reread_effect_locals(walk);
    }
    walk.reach = match (then_returned, else_returned) {
      (false, false) => outer,
      (true, false) => Some(reach_under(&outer, Expr::Not(Box::new(cond)))),
      (false, true) => Some(reach_under(&outer, cond)),
      (true, true) => Some(Expr::Lit(Lit::Bool(false))),
    };
    Ok(())
  }

  /// A branch's condition as the statements after the branch read it. One that reads a cell is bound once, where the branch is, since a write to the cell after the branch must not change which way it went.
  pub(super) fn branch_cond(&mut self, cond: Expr, walk: &mut HandlerWalk) -> Expr {
    if !reads_a_cell(&cond) {
      return cond;
    }
    let name = self.lowerer.temp();
    walk.out.push(Stmt::Let { name: name.clone(), expr: reach_under(&walk.reach, cond) });
    Expr::Var(name)
  }

  /// Writes `value` to the effect's `let` `name`: a patch of the state that holds it, which the rest of the handler reads.
  pub(super) fn effect_write(&mut self, name: &str, value: Expr, walk: &mut HandlerWalk) {
    let Some((_, hidden)) = self.effect_locals.iter().find(|(local, _)| local == name).cloned() else { return };
    walk.set(hidden, value);
    self.reread_effect_locals(walk);
  }

  /// Binds each effect `let` and each held ref a handler has written to the value its patch holds, so a read after the write, or after the branch that made it, sees it, as JavaScript's `ref.current` does.
  pub(super) fn reread_effect_locals(&mut self, walk: &HandlerWalk) {
    let refs: Vec<(String, String)> = self.held_refs.iter().map(|r| (r.clone(), r.clone())).collect();
    for (name, hidden) in self.effect_locals.clone().into_iter().chain(refs) {
      if let Some(value) = walk.patch.iter().find_map(|entry| match entry {
        Entry::Field(field, value) if *field == hidden => Some(value.clone()),
        _ => None,
      }) {
        self.lowerer.scope.push((name, value));
      }
    }
  }

  /// A `try` in a handler. Its block runs inside a `Try` whose `catch` notes that it failed and what was thrown; the `catch` block runs where it failed, inside a `Try` of its own; the `finally` block runs either way and then what failed and was not caught fails the handler. A state key set inside either block is written where the set runs, so a failure after it keeps it. True when the `finally` ended in a bare `return`.
  pub(super) fn handler_try(&mut self, t: &'p js::TryStatement<'p>, walk: &mut HandlerWalk) -> Lowered<bool> {
    let (failed, thrown) = (self.lowerer.temp(), self.lowerer.temp());
    let noted = |caught: &str| vec![Stmt::Set { name: failed.clone(), expr: Expr::Lit(Lit::Bool(true)) }, Stmt::Set { name: thrown.clone(), expr: Expr::Var(caught.to_owned()) }];
    let mut run = vec![Stmt::Let { name: failed.clone(), expr: Expr::Lit(Lit::Bool(false)) }, Stmt::Let { name: thrown.clone(), expr: Expr::Lit(Lit::Undefined) }];
    let mut in_place: Vec<(String, String, Expr)> = Vec::new();
    let guarded = |this: &mut Self, stmts: &'p [js::Statement], walk: &mut HandlerWalk, in_place: &mut Vec<(String, String, Expr)>| -> Lowered<Vec<Stmt>> {
      let mut inner = HandlerWalk { patch: walk.patch.clone(), locals: walk.locals, in_place: Some(std::mem::take(in_place)), ..HandlerWalk::default() };
      let depth = this.lowerer.scope.len();
      let returned = this.handler_block(stmts, &mut inner);
      this.lowerer.scope.truncate(depth);
      walk.locals = inner.locals;
      *in_place = inner.in_place.take().unwrap_or_default();
      if returned? {
        return Err(this.lowerer.residue(t.span, "a `return` inside a handler's `try` or `catch`"));
      }
      let caught = this.lowerer.temp();
      Ok(vec![Stmt::Try { body: inner.out, name: Some(caught.clone()), handler: Some(noted(&caught)), finally: Vec::new() }])
    };
    let tried = guarded(self, &t.block.body, walk, &mut in_place)?;
    run.extend(tried);
    if let Some(catch) = &t.handler {
      let depth = self.lowerer.scope.len();
      let bound = match catch.param.as_ref().map(|p| &p.pattern) {
        None => Ok(()),
        Some(js::BindingPattern::BindingIdentifier(id)) => {
          self.lowerer.scope.push((id.name.to_string(), Expr::Var(thrown.clone())));
          Ok(())
        }
        Some(pattern) => bind_pattern(&mut self.lowerer, pattern, Expr::Var(thrown.clone())),
      };
      let handled = bound.and_then(|_| guarded(self, &catch.body.body, walk, &mut in_place));
      self.lowerer.scope.truncate(depth);
      let mut handled = handled?;
      handled.insert(0, Stmt::Set { name: failed.clone(), expr: Expr::Lit(Lit::Bool(false)) });
      run.push(Stmt::If { cond: Expr::Var(failed.clone()), then: handled, r#else: Vec::new() });
    }
    for (_, var, start) in in_place.iter().rev() {
      run.insert(0, Stmt::Let { name: var.clone(), expr: start.clone() });
    }
    match &walk.reach {
      None => walk.out.extend(run),
      Some(reach) => {
        let (lets, rest): (Vec<Stmt>, Vec<Stmt>) = run.into_iter().partition(|stmt| matches!(stmt, Stmt::Let { .. }));
        walk.out.extend(lets);
        walk.out.push(Stmt::If { cond: reach.clone(), then: rest, r#else: Vec::new() });
      }
    }
    for (state, var, _) in in_place {
      walk.set(state, Expr::Var(var));
    }
    let returned = match &t.finalizer {
      Some(finalizer) => self.handler_block(&finalizer.body, walk)?,
      None => false,
    };
    let message = Expr::Coalesce(Box::new(Expr::Field(Box::new(Expr::Var(thrown.clone())), "message".to_owned())), Box::new(Expr::Var(thrown.clone())));
    let rethrow = Stmt::Expr(Expr::Builtin { name: Builtin::Throw, args: vec![message, Expr::Var(thrown.clone())] });
    walk.effect(Stmt::If { cond: Expr::Var(failed), then: vec![rethrow], r#else: Vec::new() });
    Ok(returned)
  }

  pub(super) fn handler_branch(&mut self, stmt: &'p js::Statement<'p>, walk: &mut HandlerWalk) -> Lowered<bool> {
    match stmt {
      js::Statement::BlockStatement(block) => self.handler_block(&block.body, walk),
      other => self.handler_block(std::slice::from_ref(other), walk),
    }
  }

  /// `setX(expr)` or `setX((prev) => expr)` adds to the patch; `e.preventDefault()`
  /// and `e.stopPropagation()` are the browser's; `void f()` or `f()` naming a
  /// declared handler inlines it. Under a branch, a key's value is the branch's
  /// where it ran and what the patch held before, or the state as it stands,
  /// where it did not; an action call runs inside an `if` of the same reach.
  pub(super) fn handler_stmt(&mut self, e: &'p js::Expression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.handler_stmt(&p.expression, walk),
      js::Expression::ConditionalExpression(cond) => self.handler_cond(cond, walk),
      js::Expression::UnaryExpression(u) if u.operator == js::UnaryOperator::Void => self.handler_stmt(&u.argument, walk),
      js::Expression::AwaitExpression(a) => self.handler_stmt(&a.argument, walk),
      js::Expression::CallExpression(call) if self.lowerer.async_iife(call).is_some() => {
        let arrow = self.lowerer.async_iife(call).expect("checked");
        match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => self.handler_async(&b.body, walk),
          crate::syntax::ArrowBody::Expr(e) => self.handler_stmt(e, walk),
        }
      }
      js::Expression::AssignmentExpression(assign) if self.lowerer.cell_of_target(&assign.left).is_some() => {
        let stmt = self.lowerer.effect(e)?;
        walk.effect(stmt);
        Ok(())
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())) => {
        let stmt = self.lowerer.effect(e)?;
        walk.effect(stmt);
        Ok(())
      }
      js::Expression::CallExpression(call) if self.lowerer.listens(call) => self.handler_listen(call, walk),
      js::Expression::CallExpression(call) if console_call(&mut self.lowerer, call)?.is_some() => {
        let logged = console_call(&mut self.lowerer, call)?.expect("checked");
        walk.effect(Stmt::Expr(logged));
        Ok(())
      }
      js::Expression::CallExpression(call) => {
        let callee = &call.callee;
        match callee {
          js::Expression::Identifier(id) => {
            let name = self.hook_renames.iter().rev().find(|(from, _)| *from == id.name.as_str()).map_or_else(|| id.name.to_string(), |(_, to)| to.clone());
            if let Some((_, action)) = self.writers.iter().find(|(w, _)| *w == name).cloned() {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::lit_str(String::new()),
              };
              walk.act(action.to_owned(), input);
              return Ok(());
            }
            if let Some((repeats, callback, delay)) = self.arrow_timer(e) {
              self.handler_timer(repeats, callback, delay, call.span, walk)?;
              return Ok(());
            }
            if self.lowerer.reaches(Reach::Later) && (name == "clearInterval" || name == "clearTimeout") {
              let timer = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Lit(Lit::Null),
              };
              walk.act(snapfire_fsr_ir::host::Act::ClearTimer.name().to_owned(), Expr::Object(vec![Entry::Field("timer".to_owned(), timer)]));
              return Ok(());
            }
            if let Some((repeats, callback, delay)) = timer_call(e) {
              let js::Expression::Identifier(target) = crate::meaning::bare(callback) else { return Err(self.lowerer.residue(call.span, "a timer a handler starts on something other than a function its effect or its component declares")) };
              let held = self.handler_fns.get(target.name.as_str()).cloned();
              let Some((_, event)) = self.effect_fns.iter().find(|(f, _)| *f == target.name.as_str()).cloned() else {
                let Some(HandlerFn { params, body, hook }) = held else { return Err(self.lowerer.residue(call.span, format!("a timer on `{}`, which is not a function its effect or its component declares", target.name))) };
                let binds = params.iter().filter_map(|param| match param.pattern {
                  js::BindingPattern::BindingIdentifier(id) if param.default.is_none() && !param.rest => Some((id.name.to_string(), Expr::Lit(Lit::Undefined))),
                  _ => None,
                });
                let continuation = self.in_hook(hook, |this| this.body_continuation(body, binds.collect(), call.span))?;
                self.start_timer(repeats, continuation, delay, walk)?;
                return Ok(());
              };
              let input = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("ms".to_owned(), self.lowerer.expr(delay)?), Entry::Field("repeats".to_owned(), Expr::Lit(Lit::Bool(repeats)))]);
              walk.act(snapfire_fsr_ir::host::Act::Timer.name().to_owned(), input);
              return Ok(());
            }
            if self.transitions.contains(&name) {
              let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) else { return Err(self.lowerer.residue(call.span, "`startTransition` of something other than an arrow")) };
              return match crate::syntax::arrow_body_of(arrow) {
                crate::syntax::ArrowBody::Block(block) => self.handler_block(&block.body, walk).map(|_| ()),
                crate::syntax::ArrowBody::Expr(e) => self.handler_stmt(e, walk),
              };
            }
            if let Some((_, state, reducer)) = self.dispatchers.iter().find(|(d, _, _)| *d == name).cloned() {
              let action = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Lit(Lit::Null),
              };
              walk.set(state.clone(), Expr::Apply { f: Box::new(reducer), args: vec![Expr::Var(state), action] });
              return Ok(());
            }
            if let Some((_, state)) = self.setters.iter().find(|(s, _)| *s == name).cloned() {
              let arg = call.arguments.first().ok_or_else(|| self.lowerer.residue(call.span, format!("`{name}` without a value")))?;
              let value = match crate::syntax::arg_expr(arg) {
                js::Expression::ArrowFunctionExpression(arrow) => {
                  let depth = self.lowerer.scope.len();
                  if let Some(prev) = crate::first_param(&arrow.params).and_then(|p| p.ident()) {
                    self.lowerer.scope.push((prev.name.to_string(), Expr::Var(state.clone())));
                  }
                  let value = match crate::syntax::arrow_body_of(arrow) {
                    crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e),
                    crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body),
                  };
                  self.lowerer.scope.truncate(depth);
                  value?
                }
                other => match self.lowerer.expr(other)? {
                  updater @ Expr::Lambda { .. } => Expr::Apply { f: Box::new(updater), args: vec![Expr::Var(state.clone())] },
                  value => value,
                },
              };
              walk.set(state, value);
              return Ok(());
            }
            if let Some(HandlerFn { params, body, hook }) = self.handler_fns.get(&name).cloned() {
              let before = (walk.out.len(), walk.patch.clone());
              let hoisting = self.lowerer.hoisting.take();
              let mut args = Vec::with_capacity(call.arguments.len());
              for arg in &call.arguments {
                args.push(self.lowerer.expr(crate::syntax::arg_expr(arg))?);
              }
              self.event_binds = Some(args);
              let result = self.in_hook(hook, |this| this.handler_fn_into(&params, body, walk));
              self.lowerer.hoisting = hoisting;
              result?;
              if walk.out.len() == before.0 && walk.patch == before.1 {
                return Err(self.lowerer.residue(call.span, "a handler that sets no state and calls no action"));
              }
              return Ok(());
            }
            if let Some(action) = self.act_bound(&name) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::lit_str(String::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            if let Some(action) = action_alias_of(self.lowerer.parsed, &name) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Object(Vec::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            if let Some(store_call) = self.meanings().of_expr(callee).and_then(|meant| meant.store_call()) {
              return self.store_call(store_call, call, walk);
            }
            let lowered = self.lowerer.expr(e)?;
            if effectful(&lowered) || calls_a_value(&lowered) || writes_back(&lowered) || (self.lowerer.reaches(Reach::Later) && !self.lowerer.cells.is_empty() && touches_cells(&lowered)) {
              walk.effect(Stmt::Expr(lowered));
              return Ok(());
            }
            Err(self.lowerer.residue(id.span, format!("a call to `{name}`, which is not a state setter, a write to the store, an action or a handler this component declares")))
          }
          m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
            let method = match crate::syntax::prop(m) {
              crate::syntax::MemberProp::Ident(i) => i.name.to_string(),
              _ => String::new(),
            };
            if method == "preventDefault" || method == "stopPropagation" {
              return Ok(());
            }
            if let Some(action) = generated_action_of(self.lowerer.parsed, callee) {
              let input = match call.arguments.first() {
                Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
                None => Expr::Object(Vec::new()),
              };
              walk.act(action, input);
              return Ok(());
            }
            let unbound = self.lowerer.unbound.clone();
            match self.lowerer.expr(e) {
              Ok(lowered) if effectful(&lowered) || writes_back(&lowered) => {
                walk.effect(Stmt::Expr(lowered));
                return Ok(());
              }
              _ => self.lowerer.unbound = unbound,
            }
            Err(self.lowerer.residue(call.span, format!("`.{method}()` in a handler; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")))
          }
          other => Err(self.lowerer.residue(other.span(), "a call a handler cannot make")),
        }
      }
      js::Expression::AssignmentExpression(assign) if timer_call(&assign.right).is_none() && matches!(&assign.left, js::AssignmentTarget::AssignmentTargetIdentifier(id) if self.effect_locals.iter().any(|(local, _)| *local == id.name.as_str())) => {
        let js::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else { unreachable!("checked") };
        let b = self.lowerer.build();
        let value = match crate::syntax::compound(b, assign.operator, b.read(id), b.copy(&assign.right), assign.span) {
          None => self.lowerer.expr(&assign.right)?,
          Some(computed) => self.lowerer.expr(&computed)?,
        };
        self.effect_write(id.name.as_str(), value, walk);
        Ok(())
      }
      js::Expression::UpdateExpression(update) if matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if self.effect_locals.iter().any(|(local, _)| *local == id.name.as_str())) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { unreachable!("checked") };
        let op = if update.operator == js::UpdateOperator::Increment { js::BinaryOperator::Addition } else { js::BinaryOperator::Subtraction };
        let b = self.lowerer.build();
        let value = self.lowerer.expr(&b.binary(b.read(id), op, b.number(1.0, update.span), update.span))?;
        self.effect_write(id.name.as_str(), value, walk);
        Ok(())
      }
      js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign && matches!(&assign.right, right if timer_call(right).is_some()) => self.handler_stmt(&assign.right, walk),
      js::Expression::AssignmentExpression(assign) if assign.operator == js::AssignmentOperator::Assign => {
        let held = match assign.left.as_member_expression() {
          Some(m) => match (m.object(), prop_name_of(m).as_deref()) {
            (js::Expression::Identifier(id), Some("current")) => self.held_refs.iter().find(|r| **r == id.name.as_str()).cloned(),
            _ => None,
          },
          None => None,
        };
        let Some(held) = held else { return Err(self.lowerer.residue(assign.span, "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")) };
        let value = self.lowerer.expr(&assign.right)?;
        walk.set(held, Expr::Object(vec![Entry::Field("current".to_owned(), value)]));
        self.reread_effect_locals(walk);
        Ok(())
      }
      other => Err(self.lowerer.residue(other.span(), "a statement a handler cannot hold; a handler is `const`s, branches, `try`, calls to state setters, writes to the store and calls to actions")),
    }
  }

  /// Whether the name a handler's residue left unbound is one the set can
  /// read, an import of the application's own, so the residue is worth
  /// returning for the resolving loop to follow rather than recording as the
  /// browser's handler.
  pub(super) fn unbound_is_followable(&self) -> bool {
    let Some(name) = &self.lowerer.unbound else { return false };
    !self.lowerer.unfollowed.contains(name) && matches!(self.meanings().bindings.get(name), Some(crate::meaning::Meaning::Own { .. }))
  }

  /// `set(key, value)`, `clear(key)`, `transaction(() => { … })` and
  /// `optimistic(key, guess, () => act(input))` in a handler. A write goes
  /// into the patch under the state bound to the key, the `useStore` binding
  /// when the component reads the key and a `let` of its own otherwise. A
  /// transaction is its closure's statements, since a patch is applied whole.
  /// `optimistic` is the write and the action; the host refuses the patch
  /// when the action fails, which is the restore the browser does by hand.
  pub(super) fn store_call(&mut self, name: &'static str, call: &'p js::CallExpression<'p>, walk: &mut HandlerWalk) -> Lowered<()> {
    match name {
      "set" | "clear" => {
        let key = self.store_key_arg(call)?;
        let value = match (name, call.arguments.get(1)) {
          ("set", Some(arg)) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
          ("set", None) => return Err(self.lowerer.residue(call.span, "`set` without a value")),
          _ => Expr::Lit(Lit::Null),
        };
        let state = self.store_state(&key);
        walk.set(state, value);
        Ok(())
      }
      "transaction" => {
        let Some(arg) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "`transaction` without a function")) };
        match crate::syntax::arg_expr(arg) {
          js::Expression::ArrowFunctionExpression(arrow) => {
            let params = crate::params_of(&arrow.params);
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "a `transaction` whose work is not an arrow")),
        }
      }
      _ => {
        let key = self.store_key_arg(call)?;
        let Some(guess) = call.arguments.get(1) else { return Err(self.lowerer.residue(call.span, "`optimistic` without a guess")) };
        let guess = self.lowerer.expr(&crate::syntax::arg_expr(&guess))?;
        let state = self.store_state(&key);
        walk.set(state, guess);
        let Some(remote) = call.arguments.get(2) else { return Err(self.lowerer.residue(call.span, "`optimistic` without the action it runs")) };
        match &*crate::syntax::arg_expr(&remote) {
          js::Expression::ArrowFunctionExpression(arrow) => {
            let params = crate::params_of(&arrow.params);
            self.handler_fn_into(&params, arrow_body(arrow), walk)
          }
          other => Err(self.lowerer.residue(other.span(), "an `optimistic` whose remote is not an arrow calling an action")),
        }
      }
    }
  }

  /// The key a store call names, a string the build can read.
  pub(super) fn store_key_arg(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<String> {
    let Some(first) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "a store call without a key")) };
    match self.lowerer.expr(crate::syntax::arg_expr(first))? {
      Expr::Lit(Lit::Str(key)) => Ok(key),
      _ => Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "a store key that is not a string the build can read")),
    }
  }

  /// The state name bound to `key`: the `useStore` binding reading it, else a
  /// binding made here so the handler's patch has a name the key rides under.
  pub(super) fn store_state(&mut self, key: &str) -> String {
    if let Some((name, _)) = self.store_bindings.iter().find(|(_, k)| k == key) {
      return name.clone();
    }
    let name = format!("$store{}", self.store_bindings.len());
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.store_bindings.push((name.clone(), key.to_owned()));
    self.store_lets.push(Stmt::Let { name: name.clone(), expr: Expr::Store(key.to_owned()) });
    name
  }
}
