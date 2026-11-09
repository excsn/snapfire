use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// Ink's `useAnimation({ interval, isActive })`: frame state of its own that
  /// starts at 0 and an `interval:<ms>` handler adding one to it while
  /// `isActive` holds, 100 ms and always active unless the options say so.
  /// `frame` reads the state and `time` the milliseconds it stands for.
  pub(super) fn animation_hook(&mut self, call: &'p js::CallExpression<'p>, pattern: &js::BindingPattern, span: Span) -> Lowered<Stmt> {
    let (mut interval, mut active) = (100u64, Expr::Lit(Lit::Bool(true)));
    if let Some(options) = call.arguments.first() {
      let js::Expression::ObjectExpression(object) = crate::syntax::arg_expr(options) else { return Err(self.lowerer.residue(span, "`useAnimation` options that are not an object written out")) };
      for prop in &object.properties {
        let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(span, "a spread in `useAnimation`'s options")) };
        let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.lowerer.residue(span, "a `useAnimation` option that is not `key: value`")) };
        let (js::PropertyKey::StaticIdentifier(key), false) = (&kv.key, kv.computed) else { return Err(self.lowerer.residue(span, "a `useAnimation` option named by an expression")) };
        match key.name.as_str() {
          "interval" => {
            let js::Expression::NumericLiteral(ms) = &kv.value else { return Err(self.lowerer.residue(span, "`useAnimation`'s `interval` is a number written out")) };
            interval = ms.value.max(1.0) as u64;
          }
          "isActive" => active = self.lowerer.expr(&kv.value)?,
          other => return Err(self.lowerer.residue(span, format!("`{other}` in `useAnimation`'s options"))),
        }
      }
    }
    let state = format!("$frame{}", self.animations);
    self.animations += 1;
    let js::BindingPattern::ObjectPattern(object) = pattern else { return Err(self.lowerer.residue(span, "`useAnimation` bound to one name; destructure `frame` and `time`")) };
    for prop in crate::syntax::pat_props(object) {
      let (key, local) = match prop {
        crate::syntax::PatProp::Assign(assign) => (assign.key.name.to_string(), assign.key.name.to_string()),
        crate::syntax::PatProp::KeyValue(kv) => {
          let (js::PropertyKey::StaticIdentifier(key), false, js::BindingPattern::BindingIdentifier(local)) = (&kv.key, kv.computed, &kv.value) else { return Err(self.lowerer.residue(span, "a pattern from `useAnimation`")) };
          (key.name.to_string(), local.name.to_string())
        }
        crate::syntax::PatProp::Rest(_) => return Err(self.lowerer.residue(span, "a rest pattern from `useAnimation`")),
      };
      let bound = match key.as_str() {
        "frame" => Expr::Var(state.clone()),
        "time" => Expr::Arith(ArithOp::Mul, Box::new(Expr::Var(state.clone())), Box::new(Expr::Lit(Lit::Int(interval.into())))),
        other => return Err(self.lowerer.residue(span, format!("`{other}` from `useAnimation`"))),
      };
      self.lowerer.scope.push((local, bound));
    }
    self.state.push(state.clone());
    self.state_bindings.push(state.clone());
    let next = Expr::Ternary(Box::new(active), Box::new(Expr::Arith(ArithOp::Add, Box::new(Expr::Var(state.clone())), Box::new(Expr::Lit(Lit::Int(1))))), Box::new(Expr::Var(state.clone())));
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Interval(interval).name(), body: vec![Stmt::Return(Expr::Object(vec![Entry::Field(state.clone(), next)]))] });
    Ok(Stmt::Let { name: state, expr: Expr::Lit(Lit::Int(0)) })
  }

  /// The timers a mount effect starts, `useEffect(() => { setInterval(f, ms) }, [])`,
  /// each lowered as a handler named `interval:<ms>` or `timeout:<ms>`, which a
  /// terminal runtime schedules. An effect with dependencies that change, a
  /// timer whose delay is not written out and a callback that does not lower
  /// are dropped, as every effect is on the server; a terminal page refuses
  /// the first when it could change what the page shows, since it has no
  /// browser to run the effect in.
  pub(super) fn effect_timers(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let was_after = std::mem::replace(&mut self.lowerer.after_body, true);
    let result = self.effect_timers_in(call);
    self.lowerer.after_body = was_after;
    result
  }

  pub(super) fn effect_timers_in(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let [effect, deps] = &call.arguments[..] else { return Ok(()) };
    let js::Expression::ArrayExpression(deps) = crate::syntax::arg_expr(deps) else { return Ok(()) };
    if !deps.elements.iter().all(|d| self.never_changes(d)) {
      if self.lowerer.reaches(Reach::Terminal) {
        return self.rerun_effect(crate::syntax::arg_expr(effect), deps);
      }
      return Ok(());
    }
    let mut inner: &'p js::Expression<'p> = crate::syntax::arg_expr(effect);
    while let js::Expression::ParenthesizedExpression(p) = inner {
      inner = &p.expression;
    }
    let js::Expression::ArrowFunctionExpression(arrow) = inner else { return Ok(()) };
    if self.lowerer.reaches(Reach::Terminal) {
      return self.terminal_effect(arrow);
    }
    let mut started: Vec<&'p js::Expression<'p>> = Vec::new();
    match arrow_body(arrow) {
      FunctionBody::Expr(e) => started.push(e),
      FunctionBody::Block(stmts) => {
        for stmt in stmts {
          match stmt {
            js::Statement::ExpressionStatement(e) => started.push(&e.expression),
            js::Statement::VariableDeclaration(var) => started.extend(var.declarations.iter().filter_map(|decl| decl.init.as_ref())),
            _ => {}
          }
        }
      }
    }
    let meanings = self.meanings();
    for expr in started {
      let js::Expression::CallExpression(timer) = expr else { continue };
      let callee = &timer.callee;
      let js::Expression::Identifier(id) = callee else { continue };
      let repeats = match id.name.as_str() {
        "setInterval" => true,
        "setTimeout" => false,
        _ => continue,
      };
      if meanings.ctxt(id) != crate::Ctxt::Unresolved {
        continue;
      }
      let [callback, delay] = &timer.arguments[..] else { continue };
      let js::Expression::NumericLiteral(delay) = crate::syntax::arg_expr(delay) else { continue };
      let held = self.lowered_handlers.len();
      match self.handler_body(crate::syntax::arg_expr(callback)) {
        Ok(body) => self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::timer(repeats, delay.value.max(0.0) as u64).name(), body }),
        Err(_) => self.lowered_handlers.truncate(held),
      }
    }
    Ok(())
  }

  /// `system.spawn(command, args, options)`.
  pub(super) fn spawn_call(&self, e: &'p js::Expression<'p>) -> Option<&'p js::CallExpression<'p>> {
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
    let callee = &call.callee;
    self.meanings().names_spawn(callee).then_some(call)
  }

  /// A mount handler that asks the terminal, with a `$spawn` act, to start `call`'s process as `id`.
  pub(super) fn spawn_start(&mut self, call: &'p js::CallExpression<'p>, id: &str, stdin: bool) -> Lowered<()> {
    let mut fields = vec![Entry::Field("id".to_owned(), Expr::lit_str(id)), Entry::Field("stdin".to_owned(), Expr::Lit(Lit::Bool(stdin)))];
    for (i, key) in ["command", "args", "options"].iter().enumerate() {
      let value = match call.arguments.get(i) {
        Some(arg) if crate::syntax::is_spread(arg) => return Err(self.lowerer.residue(crate::syntax::arg_expr(arg).span(), "a spread into `spawn`")),
        Some(arg) => self.lowerer.expr(crate::syntax::arg_expr(arg))?,
        None => Expr::Lit(Lit::Null),
      };
      fields.push(Entry::Field((*key).to_owned(), value));
    }
    let body = vec![Stmt::Act { action: snapfire_fsr_ir::host::Act::Spawn.name().to_owned(), input: Expr::Object(fields) }, Stmt::Return(Expr::Object(Vec::new()))];
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
    Ok(())
  }

  /// A listener on a process the effect spawned, `sub.on("exit", f)`, `sub.stdout.on("data", f)` or `sub.stderr.on("data", f)`, as a handler named `spawn:<id>:<event>`; `setEncoding` is taken as read, since a terminal hands output over as text. False for anything else.
  pub(super) fn spawn_listener(&mut self, e: &'p js::Expression<'p>) -> Lowered<bool> {
    if let Some((process, method, call)) = stdin_of(e) {
      let Some((_, id)) = self.spawns.iter().find(|(name, _)| *name == process).cloned() else { return Ok(false) };
      let mut input = vec![Entry::Field("id".to_owned(), Expr::lit_str(id))];
      match (method, call.arguments.as_slice()) {
        ("write", [text]) if !crate::syntax::is_spread(text) => input.push(Entry::Field("text".to_owned(), self.lowerer.expr(crate::syntax::arg_expr(text))?)),
        ("end", []) => input.push(Entry::Field("end".to_owned(), Expr::Lit(Lit::Bool(true)))),
        _ => return Err(self.lowerer.residue(call.span, "a spawned process's `stdin` other than `write(text)` and `end()`")),
      }
      let body = vec![Stmt::Act { action: snapfire_fsr_ir::host::Act::Stdin.name().to_owned(), input: Expr::Object(input) }, Stmt::Return(Expr::Object(Vec::new()))];
      self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
      return Ok(true);
    }
    let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return Ok(false) };
    let callee = &call.callee;
    let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return Ok(false) };
    let (process, stream) = match crate::meaning::bare(method.object()) {
      js::Expression::Identifier(id) => (id.name.to_string(), None),
      inner_expr if inner_expr.is_member_expression() => { let inner = inner_expr.as_member_expression().expect("matched as a member"); match (crate::meaning::bare(inner.object()), prop_name_of(inner).as_deref()) {
        (js::Expression::Identifier(id), Some("stdout")) => (id.name.to_string(), Some("stdout")),
        (js::Expression::Identifier(id), Some("stderr")) => (id.name.to_string(), Some("stderr")),
        _ => return Ok(false),
      } },
      _ => return Ok(false),
    };
    let Some((_, id)) = self.spawns.iter().find(|(name, _)| *name == process).cloned() else { return Ok(false) };
    let event = match call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) {
      Some(js::Expression::StringLiteral(event)) => Some(crate::lossy(event.value)),
      _ => None,
    };
    let report = match (stream, prop_name_of(method).as_deref(), event.as_deref()) {
      (Some(_), Some("setEncoding"), _) => return Ok(true),
      (Some("stdout"), Some("on"), Some("data")) => snapfire_fsr_ir::host::Report::Stdout,
      (Some(_), Some("on"), Some("data")) => snapfire_fsr_ir::host::Report::Stderr,
      (None, Some("on"), Some("error")) => snapfire_fsr_ir::host::Report::Error,
      (None, Some("on"), Some("exit")) => snapfire_fsr_ir::host::Report::Exit,
      (None, Some("on"), Some("close")) => snapfire_fsr_ir::host::Report::Close,
      _ => return Err(self.lowerer.residue(call.span, "a use of a spawned process other than `on(\"error\" | \"exit\" | \"close\", f)`, `stdout.on(\"data\", f)`, `stderr.on(\"data\", f)` and `setEncoding`")),
    };
    let Some(listener) = call.arguments.get(1) else { return Err(self.lowerer.residue(call.span, "a process listener without a function")) };
    let (params, body) = match crate::meaning::bare(crate::syntax::arg_expr(listener)) {
      js::Expression::ArrowFunctionExpression(f) => (crate::params_of(&f.params), arrow_body(f)),
      js::Expression::FunctionExpression(f) => match &f.body {
        Some(body) => (crate::params_of(&f.params), FunctionBody::Block(&body.statements)),
        None => return Err(self.lowerer.residue(crate::syntax::arg_expr(listener).span(), "a process listener without a body")),
      },
      other => return Err(self.lowerer.residue(other.span(), "a process listener that is not a function written in place")),
    };
    if matches!(report, snapfire_fsr_ir::host::Report::Exit | snapfire_fsr_ir::host::Report::Close) {
      let event = || Expr::Var("$event".to_owned());
      self.event_binds = Some(vec![event().field("code".to_owned()), event().field("signal".to_owned())]);
    }
    let lowered = self.handler_fn(&params, body, call.span)?;
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Process { id, report }.name(), body: lowered });
    Ok(true)
  }

  /// The act a local refers to in a terminal page, bound to an [`snapfire_fsr_ir::host::ACT_REF`].
  pub(super) fn act_bound(&self, name: &str) -> Option<String> {
    if !self.lowerer.reaches(Reach::Terminal) {
      return None;
    }
    let (_, bound) = self.lowerer.scope.iter().rev().find(|(n, _)| n == name)?;
    snapfire_fsr_ir::host::act_ref(bound)
  }

  /// Whether an effect's dependency is the same on every render: a state setter or a writer; in a terminal also a `useReducer` dispatch, a held ref or a `useCallback` whose own dependencies never change.
  pub(super) fn never_changes(&self, elem: &js::ArrayExpressionElement) -> bool {
    let item = match elem {
      js::ArrayExpressionElement::Elision(_) => return false,
      js::ArrayExpressionElement::SpreadElement(s) => &s.argument,
      other => other.to_expression(),
    };
    let js::Expression::Identifier(id) = item else { return false };
    let name = id.name.as_str();
    self.setters.iter().any(|(setter, _)| setter == name)
      || self.writers.iter().any(|(writer, _)| writer == name)
      || (self.lowerer.reaches(Reach::Terminal) && (self.dispatchers.iter().any(|(d, _, _)| d == name) || self.held_refs.iter().any(|r| r == name) || self.stable_callbacks.iter().any(|c| c == name) || self.act_bound(name).is_some()))
  }

  /// Whether `body` reads a browser global, the global itself rather than a local of that name: work for a browser, which a terminal page has none of.
  pub(super) fn reads_the_browser(&self, arrow: &js::ArrowFunctionExpression) -> bool {
    struct Browser<'m>(bool, &'m crate::meaning::Meanings);
    impl<'a> oxc_ast_visit::Visit<'a> for Browser<'_> {
      fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
        self.0 |= self.1.ctxt(id) == crate::Ctxt::Unresolved && BROWSER_GLOBALS.contains(&id.name.as_str());
      }
    }
    let meanings = self.meanings();
    let mut browser = Browser(false, &meanings);
    match crate::syntax::arrow_body_of(arrow) {
      crate::syntax::ArrowBody::Block(b) => b.body.walk(&mut browser),
      crate::syntax::ArrowBody::Expr(e) => e.walk(&mut browser),
    }
    browser.0
  }

  /// A terminal's effect whose dependencies change: its body a handler named [`snapfire_fsr_ir::host::Event::Effect`] and a [`crate::terminal::EFFECT`] the body emits with the dependencies' values, which the terminal compares from one render to the next. An effect that sets no state and calls no action changes nothing the page shows and is left out, as is one that does not lower and reads a browser global: that is the browser's work. An arrow it returns is its cleanup, [`snapfire_fsr_ir::host::Act::Cleanup`].
  pub(super) fn rerun_effect(&mut self, effect: &'p js::Expression<'p>, deps: &'p js::ArrayExpression<'p>) -> Lowered<()> {
    let js::Expression::ArrowFunctionExpression(arrow) = crate::meaning::bare(effect) else { return Err(self.lowerer.residue(effect.span(), "an effect that is not an arrow written in place")) };
    let event = snapfire_fsr_ir::host::Event::Effect(self.lowered_handlers.len()).name();
    let cleanup_for = self.cleanup_for.replace(event.clone());
    let lowered = self.handler_fn(&[], arrow_body(arrow), arrow.span);
    self.cleanup_for = cleanup_for;
    let lowered = match lowered {
      Ok(lowered) => lowered,
      Err(residue) if residue.message == NO_EFFECT => return Ok(()),
      Err(_) if self.reads_the_browser(arrow) => return Ok(()),
      Err(residue) => return Err(residue),
    };
    self.lowered_handlers.push(Handler { event: event.clone(), body: lowered });
    let mut values = Vec::with_capacity(deps.elements.len());
    for dep in array_items(deps) {
      values.push(Entry::Item(self.lowerer.expr(dep)?));
    }
    let emitted = Expr::Object(vec![Entry::Field("event".to_owned(), Expr::lit_str(event)), Entry::Field("deps".to_owned(), Expr::Array(values))]);
    let held = self.lowerer.temp();
    self.pending_lets.push(Stmt::Let { name: held, expr: Expr::Builtin { name: Builtin::Emit, args: vec![Expr::lit_str(crate::terminal::EFFECT), emitted] } });
    Ok(())
  }

  /// A terminal's mount effect, run as Ink runs it: each `let` it declares is
  /// state of the component's own, each function it declares a handler named
  /// `call:<n>`, a call to one at the top of the effect a `mount` handler, and
  /// a `setInterval` or `setTimeout` it starts a handler named `interval:<ms>`
  /// or `timeout:<ms>`. Any other statement at its top, an `if` around
  /// setter calls for one, is a `mount` handler of its own. Inside a handler,
  /// a timer started on one of the effect's functions is a `$timer` act the
  /// terminal schedules. The cleanup it returns is left out, since a terminal
  /// stops every timer on exit.
  pub(super) fn terminal_effect(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<()> {
    let depth = self.lowerer.scope.len();
    let (locals, fns, spawns) = (self.effect_locals.len(), self.effect_fns.len(), self.spawns.len());
    let result = self.terminal_effect_in(arrow);
    self.lowerer.scope.truncate(depth);
    self.effect_locals.truncate(locals);
    self.effect_fns.truncate(fns);
    self.spawns.truncate(spawns);
    result
  }

  pub(super) fn terminal_effect_in(&mut self, arrow: &'p js::ArrowFunctionExpression<'p>) -> Lowered<()> {
    let stmts: Vec<&'p js::Statement<'p>> = match arrow_body(arrow) {
      FunctionBody::Block(stmts) => stmts.iter().collect(),
      FunctionBody::Expr(_) => Vec::new(),
    };
    if let FunctionBody::Expr(e) = arrow_body(arrow) {
      return self.effect_start(e, arrow.span);
    }
    let fn_of = |init: &'p js::Expression<'p>| -> Option<(Vec<crate::Param<'p>>, FunctionBody<'p>)> {
      match crate::meaning::bare(init) {
        js::Expression::ArrowFunctionExpression(f) => Some((crate::params_of(&f.params), arrow_body(f))),
        js::Expression::FunctionExpression(f) => f.body.as_ref().map(|body| (crate::params_of(&f.params), FunctionBody::Block(&body.statements))),
        _ => None,
      }
    };
    for stmt in &stmts {
      match stmt {
        js::Statement::FunctionDeclaration(f) => {
          let (Some(body), Some(id)) = (&f.body, &f.id) else { continue };
          self.effect_fn(id.name.as_str(), crate::params_of(&f.params), FunctionBody::Block(&body.statements));
        }
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let js::BindingPattern::BindingIdentifier(id) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring in an effect")) };
            let name = id.name.to_string();
            match decl.init.as_ref() {
              Some(init) if fn_of(init).is_some() => {
                let (params, body) = fn_of(init).expect("checked");
                self.effect_fn(&name, params, body);
              }
              Some(init) if timer_call(init).is_some() => {}
              Some(init) if self.spawn_call(init).is_some() => self.spawns.push((name, crate::lo(decl.span).to_string())),
              init => {
                let value = match init {
                  Some(init) => self.lowerer.expr(init)?,
                  None => Expr::Lit(Lit::Undefined),
                };
                let hidden = format!("$fx{}:{name}", crate::lo(decl.span));
                self.state.push(hidden.clone());
                self.state_bindings.push(hidden.clone());
                self.pending_lets.push(Stmt::Let { name: hidden.clone(), expr: value });
                self.lowerer.scope.push((name.clone(), Expr::Var(hidden.clone())));
                self.effect_locals.push((name, hidden));
              }
            }
          }
        }
        _ => {}
      }
    }
    let declared: Vec<(String, String)> = self.effect_fns.clone();
    for (name, event) in &declared {
      let f = self.handler_fns.get(name).cloned().expect("an effect function is a handler function");
      let lowered = self.in_hook(f.hook, |this| this.handler_fn(&f.params, f.body, arrow.span))?;
      self.lowered_handlers.push(Handler { event: event.clone(), body: lowered });
    }
    for stmt in &stmts {
      match stmt {
        js::Statement::ExpressionStatement(e) if self.spawn_listener(&e.expression)? => {}
        js::Statement::ExpressionStatement(e) => self.effect_start(&e.expression, e.span)?,
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let Some(init) = decl.init.as_ref() else { continue };
            if timer_call(init).is_some() {
              self.effect_start(init, init.span())?;
            }
            if let Some(call) = self.spawn_call(init) {
              let writes = matches!(&decl.id, js::BindingPattern::BindingIdentifier(id) if stmts.iter().any(|s| matches!(s, js::Statement::ExpressionStatement(e) if stdin_of(&e.expression).is_some_and(|(name, _, _)| name == id.name.as_str()))));
              self.spawn_start(call, &crate::lo(decl.span).to_string(), writes)?;
            }
          }
        }
        js::Statement::FunctionDeclaration(_) | js::Statement::ReturnStatement(_) | js::Statement::EmptyStatement(_) => {}
        other => {
          let body = self.handler_fn(&[], FunctionBody::Block(std::slice::from_ref(*stmt)), other.span())?;
          self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
        }
      }
    }
    Ok(())
  }

  /// An effect's function as a handler the terminal can call, `call:<n>`, and a function the effect's other handlers inline.
  pub(super) fn effect_fn(&mut self, name: &str, params: Vec<crate::Param<'p>>, body: FunctionBody<'p>) {
    let event = snapfire_fsr_ir::host::Event::Call(self.lowered_handlers.len() + self.effect_fns.len()).name();
    self.handler_fns.insert(name.to_owned(), (params, body).into());
    self.effect_fns.push((name.to_owned(), event));
  }

  /// A statement at the top of a terminal effect: a timer it starts, from a `mount` handler as a timer a handler starts does, so its callback holds the values of the render that ran the effect; a call to one of its functions, which runs on mount; or a call made for its effect, which the terminal cannot run.
  pub(super) fn effect_start(&mut self, e: &'p js::Expression<'p>, span: Span) -> Lowered<()> {
    if timer_call(e).is_some() {
      let body = self.handler_fn(&[], FunctionBody::Expr(e), span)?;
      self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
      return Ok(());
    }
    if let js::Expression::CallExpression(call) = crate::meaning::bare(e) {
      {
      let callee = &call.callee;
        if let js::Expression::Identifier(id) = crate::meaning::bare(callee) {
          if self.effect_fns.iter().any(|(name, _)| *name == id.name.as_str()) {
            let body = self.handler_body(callee)?;
            self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
            return Ok(());
          }
        }
      }
    }
    match e {
      js::Expression::CallExpression(_) => {
        let body = self.handler_fn(&[], FunctionBody::Expr(e), span)?;
        self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Mount.name(), body });
        Ok(())
      }
      _ => Err(self.lowerer.residue(span, "an expression a terminal effect cannot run")),
    }
  }

  /// Ink's `useInput((input, key) => …)`: the component's `input` handler, run
  /// with each keypress as `$event`, `input` its text and `key` its flags. An
  /// empty handler, which keeps an Ink app reading keys, is no handler.
  pub(super) fn input_hook(&mut self, call: &'p js::CallExpression<'p>) -> Lowered<()> {
    let Some(arg) = call.arguments.first() else { return Err(self.lowerer.residue(call.span, "`useInput` with no handler")) };
    if let js::Expression::ArrowFunctionExpression(arrow) = crate::syntax::arg_expr(arg) {
      if matches!(arrow_body(arrow), FunctionBody::Block(stmts) if stmts.is_empty()) {
        return Ok(());
      }
    }
    let active = match call.arguments.get(1).map(|options| crate::syntax::arg_expr(options)) {
      None => None,
      Some(js::Expression::ObjectExpression(object)) => {
        let mut active = None;
        for prop in &object.properties {
          let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(call.span, "a spread in `useInput`'s options")) };
          let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(prop) else { return Err(self.lowerer.residue(call.span, "a `useInput` option that is not `key: value`")) };
          match prop_name(&kv.key, kv.computed).as_deref() {
            Some("isActive") => active = Some(self.lowerer.expr(&kv.value)?),
            other => return Err(self.lowerer.residue(call.span, format!("`{}` in `useInput`'s options", other.unwrap_or("a computed key")))),
          }
        }
        active
      }
      Some(_) => return Err(self.lowerer.residue(call.span, "`useInput` options that are not an object written out")),
    };
    let event = Expr::Var("$event".to_owned());
    self.event_binds = Some(vec![Expr::Field(Box::new(event.clone()), "input".to_owned()), Expr::Field(Box::new(event), "key".to_owned())]);
    let body = self.handler_body(crate::syntax::arg_expr(arg));
    self.event_binds = None;
    let body = match active {
      Some(cond) => vec![Stmt::If { cond, then: body?, r#else: Vec::new() }],
      None => body?,
    };
    self.lowered_handlers.push(Handler { event: snapfire_fsr_ir::host::Event::Input.name(), body });
    Ok(())
  }
}

/// `sub.stdin.write(text)` or `sub.stdin.end()`: the process's name, the method and the call.
pub(super) fn stdin_of<'e>(e: &'e js::Expression<'e>) -> Option<(String, &'static str, &'e js::CallExpression<'e>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
  let callee = &call.callee;
  let Some(method) = (crate::meaning::bare(callee)).as_member_expression() else { return None };
  let Some(stream) = (crate::meaning::bare(method.object())).as_member_expression() else { return None };
  let (js::Expression::Identifier(process), Some("stdin")) = (crate::meaning::bare(stream.object()), prop_name_of(stream).as_deref()) else { return None };
  let method = match prop_name_of(method).as_deref() {
    Some("write") => "write",
    Some("end") => "end",
    _ => return None,
  };
  Some((process.name.to_string(), method, call))
}
