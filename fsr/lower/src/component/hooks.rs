use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  /// What the file's module-level names mean.
  pub(super) fn meanings(&self) -> Rc<crate::meaning::Meanings> {
    self.lowerer.meanings.clone().unwrap_or_default()
  }

  /// Whether a `use` call names one of the application's own hooks rather than React's or FSR client's.
  /// Whether `call`'s callee is an import of a package the build does not follow.
  pub(super) fn foreign_callee(&self, call: &js::CallExpression) -> bool {
    let callee = &call.callee;
    matches!(self.meanings().of_expr(callee), Some(crate::meaning::Meaning::Foreign))
  }

  pub(super) fn custom_hook(&self, name: &str) -> bool {
    use crate::meaning::{Construct, Meaning, LIBRARY};
    match self.meanings().bindings.get(name) {
      Some(Meaning::Own { .. } | Meaning::Foreign) => true,
      Some(_) => false,
      None => !LIBRARY.iter().any(|(_, _, construct)| matches!(construct, Construct::Hook(hook) if *hook == name)),
    }
  }

  /// A custom hook's call inlined: its body binds under a prefix of its own so two calls hold two sets of state. What it returns binds at the call site. A returned setter or function is a handler there, run by the browser.
  pub(super) fn inline_hook(&mut self, hook: &str, call: &'p js::CallExpression<'p>, pattern: Option<&'p js::BindingPattern<'p>>) -> Lowered<Option<Stmt>> {
    let Some((_, parsed, export)) = self.hooks.iter().find(|(n, _, _)| n == hook).cloned() else {
      let origin = self.meanings().of_expr(&call.callee).and_then(|meant| meant.origin().map(|(f, n)| (f.to_owned(), n.to_owned())));
      self.wanted_hook = Some((hook.to_owned(), origin));
      return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
    };
    let Some(found) = find_function(parsed, &export) else { return Err(self.lowerer.residue(call.span, format!("`{hook}` is not a function the build can read"))) };
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, _) => (crate::params_of(params), FunctionBody::Block(stmts)),
      Found::Arrow(arrow) => (crate::params_of(&arrow.params), arrow_body(arrow)),
    };
    let mut args = Vec::with_capacity(call.arguments.len());
    for arg in &call.arguments {
      match self.lowerer.expr(crate::syntax::arg_expr(arg)) {
        Ok(value) => args.push(value),
        // A callback that writes state or calls out is one only the browser runs, as an effect or a listener does.
        Err(_) if matches!(crate::meaning::bare(crate::syntax::arg_expr(arg)), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) && !self.unbound_is_followable() => {
          self.lowerer.unbound = None;
          args.push(Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER));
        }
        Err(residue) => return Err(residue),
      }
    }
    self.hook_calls += 1;
    let prefix = format!("{hook}${}$", self.hook_calls);
    let outer = self.lowerer.parsed;
    let hoisting = self.lowerer.hoisting.take();
    let depth = self.lowerer.scope.len();
    let declared = self.lowerer.declared_below.len();
    self.lowerer.parsed = parsed;
    let outer_meanings = std::mem::replace(&mut self.lowerer.meanings, parsed.meanings.get().cloned());
    let fns: std::collections::HashSet<String> = self.handler_fns.keys().cloned().collect();
    let mut renames = Vec::new();
    let returned = self.hook_body(&prefix, &params, body, args, &mut renames);
    let scope = Rc::new(HookScope { parsed, meanings: self.lowerer.meanings.clone(), scope: self.lowerer.scope[depth..].to_vec(), declared_below: self.lowerer.declared_below[declared..].to_vec(), renames });
    for (name, f) in self.handler_fns.iter_mut() {
      if f.hook.is_none() && !fns.contains(name) {
        f.hook = Some(scope.clone());
      }
    }
    self.lowerer.parsed = outer;
    self.lowerer.meanings = outer_meanings;
    self.lowerer.hoisting = hoisting;
    self.lowerer.scope.truncate(depth);
    self.lowerer.declared_below.truncate(declared);
    let returned = match returned {
      Ok(returned) => returned,
      Err(residue) => {
        if self.lowerer.unbound.is_some() && self.unbound_in.is_none() {
          self.unbound_in = Some(parsed.file.clone());
        }
        return Err(residue);
      }
    };
    if let Some(pattern) = pattern {
      self.bind_returned(pattern, returned, &prefix)?;
    }
    Ok(None)
  }

  pub(super) fn hook_body(&mut self, prefix: &str, params: &[crate::Param], body: FunctionBody<'p>, args: Vec<Expr>, renames: &mut Vec<(String, String)>) -> Lowered<Returned> {
    for (i, param) in params.iter().enumerate() {
      let mut arg = args.get(i).cloned().unwrap_or(Expr::Lit(Lit::Null));
      if !plain(&arg) {
        let held = self.lowerer.temp();
        self.pending_lets.push(Stmt::Let { name: held.clone(), expr: arg });
        arg = Expr::Var(held);
      }
      bind_param(&mut self.lowerer, param, arg)?;
    }
    let stmts = match body {
      FunctionBody::Expr(e) => return self.returned(e, &[]),
      FunctionBody::Block(stmts) => stmts,
    };
    let floor = self.lowerer.scope.len();
    let mut early: Vec<(Expr, Expr)> = Vec::new();
    let guard = |early: &[(Expr, Expr)], expr: Expr| match early.iter().map(|(cond, _)| cond.clone()).reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))) {
      None => expr,
      Some(fired) => Expr::Ternary(Box::new(fired), Box::new(Expr::Lit(Lit::Null)), Box::new(expr)),
    };
    // A function declaration is hoisted: a handler above it calls it by name.
    for stmt in stmts {
      if let js::Statement::FunctionDeclaration(f) = stmt {
        let Some(id) = &f.id else { continue };
        let bound = format!("{prefix}{}", id.name);
        if let Some(body) = &f.body {
          self.handler_fns.insert(bound.clone(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
          self.lowerer.local_fns.push((id.name.to_string(), &**f));
        }
        self.lowerer.browser_functions.push(id.name.to_string());
        self.browser_function(bound.clone());
        renames.push((id.name.to_string(), bound));
      }
    }
    renames.extend(self.declare_below(stmts, prefix).into_iter().filter(|(from, to)| from != to));
    for stmt in stmts {
      match stmt {
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            let from = self.pending_lets.len();
            self.hook_let(decl, prefix, renames)?;
            for pending in &mut self.pending_lets[from..] {
              if let Stmt::Let { expr, .. } = pending {
                *expr = guard(&early, std::mem::replace(expr, Expr::Lit(Lit::Null)));
              }
            }
          }
        }
        js::Statement::FunctionDeclaration(_) => {}
        js::Statement::ExpressionStatement(e) if library_hook(&self.meanings(), &e.expression).is_some_and(|(name, _)| name == "useInput") => {
          let (_, call) = library_hook(&self.meanings(), &e.expression).expect("checked");
          let depth = self.hook_renames.len();
          self.hook_renames.extend(renames.iter().cloned());
          let result = self.input_hook(call);
          self.hook_renames.truncate(depth);
          result?;
        }
        js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
          let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
          if hook == "useEffect" || hook == "useLayoutEffect" {
            let depth = self.hook_renames.len();
            self.hook_renames.extend(renames.iter().cloned());
            let result = self.effect_timers(call);
            self.hook_renames.truncate(depth);
            result?;
          }
        }
        js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| self.custom_hook(name)) => {
          let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
          self.inline_hook(hook, call, None)?;
        }
        js::Statement::ReturnStatement(ret) => {
          let returned = match ret.argument.as_ref() {
            Some(arg) => self.returned(arg, renames)?,
            None => Returned::Value(Expr::Lit(Lit::Null)),
          };
          return self.after_early(early, returned, ret.span);
        }
        js::Statement::IfStatement(branch) if early_exit(branch).is_some() => {
          let cond = guard(&early, self.lowerer.expr(&branch.test)?);
          let then = match early_exit(branch).expect("checked") {
            js::Statement::ThrowStatement(thrown) => throw_expr(&mut self.lowerer, thrown)?,
            js::Statement::ReturnStatement(ret) => match ret.argument.as_ref() {
              None => Expr::Lit(Lit::Null),
              Some(arg) => match self.returned(arg, renames)? {
                Returned::Value(value) => value,
                _ => return Err(self.lowerer.residue(ret.span, "an early `return` of handlers from a custom hook")),
              },
            },
            _ => unreachable!("an early exit is a `return` or a `throw`"),
          };
          early.push((cond, then));
        }
        js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) if holds_return(stmt) => {
          return Err(self.lowerer.residue(stmt.span(), "a `return` inside a branch of a custom hook"));
        }
        js::Statement::ExpressionStatement(_) | js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) => {
          let mut held = std::mem::take(&mut self.pending_lets);
          let result = self.rebound(stmt, floor, &[], &mut held, &|_, expr| guard(&early, expr));
          self.pending_lets = held;
          result?;
        }
        other => return Err(self.lowerer.residue(other.span(), "a statement a custom hook cannot hold; it holds hooks, `const`s, `let`s, functions and a `return`")),
      }
    }
    self.after_early(early, Returned::Value(Expr::Lit(Lit::Null)), Span::default())
  }

  /// Declares each `const` and `let` of `stmts` for what runs after the body, as JavaScript binds them from the start of it: a value under the name it binds, `prefix` and its own in a custom hook, and a function as a handler function. Of a hook's result, `useState` and `useStore` declare their value and their setter, `useRef` its ref, `useMemo` its value and `useCallback` its function; any other hook's is declared where it is, since what it binds is known once the hook lowers. Returns each name with the one it binds.
  pub(super) fn declare_below(&mut self, stmts: &'p [js::Statement], prefix: &str) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    let value = |this: &mut Self, bound: &mut Vec<(String, String)>, name: &str| {
      let to = format!("{prefix}{name}");
      this.lowerer.declared_below.push((name.to_owned(), to.clone()));
      bound.push((name.to_owned(), to));
    };
    for stmt in stmts {
      let js::Statement::VariableDeclaration(var) = stmt else { continue };
      for decl in &var.declarations {
        let init = decl.init.as_ref().map(crate::meaning::bare);
        if let Some((hook, call)) = init.and_then(|init| hook_call(&self.meanings(), init)) {
          match (hook, &decl.id) {
            ("useState" | "useStore", js::BindingPattern::ArrayPattern(arr)) => {
              let mut names = arr.elements.iter().map(|e| match e {
                Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
                _ => None,
              });
              let (Some(Some(state)), setter) = (names.next(), names.next().flatten()) else { continue };
              value(self, &mut bound, &state);
              if let Some(setter) = setter {
                bound.push((setter.clone(), format!("{prefix}{setter}")));
                if !self.setters.iter().any(|(s, _)| *s == format!("{prefix}{setter}")) {
                  self.setters.push((format!("{prefix}{setter}"), format!("{prefix}{state}")));
                }
              }
            }
            ("useRef", js::BindingPattern::BindingIdentifier(id)) => {
              value(self, &mut bound, &id.name);
              if self.lowerer.reaches(Reach::Terminal) && !self.held_refs.contains(&format!("{prefix}{}", id.name)) {
                self.held_refs.push(format!("{prefix}{}", id.name));
              }
            }
            ("useMemo", js::BindingPattern::BindingIdentifier(id)) => value(self, &mut bound, &id.name),
            ("useCallback", js::BindingPattern::BindingIdentifier(id)) => {
              if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::meaning::bare(crate::syntax::arg_expr(a))) {
                self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&arrow.params), arrow_body(arrow)).into());
                bound.push((id.name.to_string(), format!("{prefix}{}", id.name)));
              }
            }
            _ => {}
          }
          continue;
        }
        match (&decl.id, init) {
          (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::ArrowFunctionExpression(arrow))) => {
            self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&arrow.params), arrow_body(arrow)).into());
          }
          (js::BindingPattern::BindingIdentifier(id), Some(js::Expression::FunctionExpression(f))) => {
            if let Some(body) = &f.body {
              self.handler_fns.entry(format!("{prefix}{}", id.name)).or_insert((crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
            }
          }
          (pattern, _) => {
            let mut names = Vec::new();
            crate::pattern_names(pattern, &mut names);
            for name in names {
              value(self, &mut bound, &name.name);
            }
          }
        }
      }
    }
    bound
  }

  /// A hook's return taken when none of its early exits fired, each `(cond, value)` checked in order before it.
  pub(super) fn after_early(&mut self, early: Vec<(Expr, Expr)>, returned: Returned, span: Span) -> Lowered<Returned> {
    if early.is_empty() {
      return Ok(returned);
    }
    let Returned::Value(last) = returned else {
      return Err(self.lowerer.residue(span, "a custom hook that returns handlers after an early `return`"));
    };
    Ok(Returned::Value(early.into_iter().rev().fold(last, |r#else, (cond, then)| Expr::Ternary(Box::new(cond), Box::new(then), Box::new(r#else)))))
  }

  /// One declaration inside a hook: lowered as the component's are, then every name it bound renamed under the call's prefix.
  pub(super) fn hook_let(&mut self, decl: &'p js::VariableDeclarator<'p>, prefix: &str, renames: &mut Vec<(String, String)>) -> Lowered<()> {
    let (state, bindings, stores, setters, handlers, scope) = (self.state.len(), self.state_bindings.len(), self.store_bindings.len(), self.setters.len(), self.handlers.len(), self.lowerer.scope.len());
    let fns: Vec<String> = self.handler_fns.keys().cloned().collect();
    let stmt = self.let_stmt(decl)?;
    let mut map: Vec<(String, String)> = Vec::new();
    let rename = |map: &mut Vec<(String, String)>, name: &str| -> String {
      if let Some((_, to)) = map.iter().find(|(from, _)| from == name) {
        return to.clone();
      }
      let to = format!("{prefix}{name}");
      map.push((name.to_owned(), to.clone()));
      to
    };
    for name in self.state[state..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for name in self.state_bindings[bindings..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for (name, _) in self.store_bindings[stores..].iter_mut() {
      *name = rename(&mut map, name);
    }
    for (setter, held) in self.setters[setters..].iter_mut() {
      *setter = rename(&mut map, setter);
      *held = rename(&mut map, held);
    }
    for name in self.handlers[handlers..].iter_mut() {
      *name = rename(&mut map, name);
    }
    let added: Vec<String> = self.handler_fns.keys().filter(|k| !fns.contains(k)).cloned().collect();
    for key in added {
      if let Some(f) = self.handler_fns.remove(&key) {
        let to = rename(&mut map, &key);
        self.handler_fns.insert(to, f);
      }
    }
    if let Some(Stmt::Let { name, expr }) = stmt {
      let to = if name.starts_with("$d") { name } else { rename(&mut map, &name) };
      self.pending_lets.push(Stmt::Let { name: to, expr });
    }
    for (_, bound) in self.lowerer.scope[scope..].iter_mut() {
      if let Expr::Var(var) = bound {
        if let Some((_, to)) = map.iter().find(|(from, _)| from == var) {
          *var = to.clone();
        }
      }
    }
    renames.extend(map);
    Ok(())
  }

  /// A hook's return as values and handlers, part by part where it is an object or array literal.
  pub(super) fn returned(&mut self, e: &'p js::Expression<'p>, renames: &[(String, String)]) -> Lowered<Returned> {
    let handler = |this: &Self, e: &js::Expression| -> Option<String> {
      let js::Expression::Identifier(id) = e else { return None };
      let bound = renames.iter().rev().find(|(from, _)| from == id.name.as_str()).map(|(_, to)| to.clone())?;
      this.handlers.contains(&bound).then_some(bound)
    };
    let part = |this: &mut Self, e: &'p js::Expression<'p>| -> Lowered<Part> {
      if let js::Expression::ArrowFunctionExpression(arrow) = e {
        this.hook_calls += 1;
        let bound = format!("$hook{}", this.hook_calls);
        this.handler_fns.insert(bound.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
        this.browser_function(bound.clone());
        return Ok(Part::Handler(bound));
      }
      Ok(match handler(this, e) {
        Some(bound) => Part::Handler(bound),
        None => Part::Value(this.lowerer.expr(e)?),
      })
    };
    match e {
      js::Expression::ParenthesizedExpression(p) => self.returned(&p.expression, renames),
      js::Expression::TSAsExpression(a) => self.returned(&a.expression, renames),
      js::Expression::TSSatisfiesExpression(a) => self.returned(&a.expression, renames),
      js::Expression::ObjectExpression(obj) => {
        let mut parts = Vec::with_capacity(obj.properties.len());
        for prop in &obj.properties {
          let js::ObjectPropertyKind::ObjectProperty(prop) = prop else { return Err(self.lowerer.residue(obj.span, "a spread in what a custom hook returns")) };
          match crate::syntax::prop_of(prop) {
            crate::syntax::Prop::Shorthand(id) => {
              let ident = self.lowerer.build().read(id);
              let p = match handler(self, &ident) {
                Some(bound) => Part::Handler(bound),
                None => Part::Value(self.lowerer.expr(&ident)?),
              };
              parts.push((id.name.to_string(), p));
            }
            crate::syntax::Prop::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed key in what a custom hook returns"))?;
              parts.push((key, part(self, &kv.value)?));
            }
            other => return Err(self.lowerer.residue(other.span(), "a method in what a custom hook returns")),
          }
        }
        Ok(Returned::Object(parts))
      }
      js::Expression::ArrayExpression(arr) => {
        let mut parts = Vec::with_capacity(arr.elements.len());
        for elem in &arr.elements {
          match elem {
            js::ArrayExpressionElement::Elision(_) => parts.push(None),
            js::ArrayExpressionElement::SpreadElement(s) => return Err(self.lowerer.residue(s.argument.span(), "a spread in what a custom hook returns")),
            other => parts.push(Some(part(self, other.to_expression())?)),
          }
        }
        Ok(Returned::Array(parts))
      }
      other => Ok(match handler(self, other) {
        Some(bound) => Returned::Handler(bound),
        None => Returned::Value(self.lowerer.expr(other)?),
      }),
    }
  }

  /// `local` at the call site is the handler `bound` inside the hook: the same function, the same state when it is a setter.
  pub(super) fn alias_handler(&mut self, local: &str, bound: &str) {
    self.browser_function(local.to_owned());
    if let Some(f) = self.handler_fns.get(bound).cloned() {
      self.handler_fns.insert(local.to_owned(), f);
    }
    if let Some((_, state)) = self.setters.iter().find(|(s, _)| s == bound).cloned() {
      self.setters.push((local.to_owned(), state));
    }
  }

  pub(super) fn bind_value(&mut self, pattern: &js::BindingPattern, value: Expr) -> Lowered<()> {
    if snapfire_fsr_ir::host::act_ref(&value).is_some() || crate::terminal::primitive_ref(&value).is_some() || (self.lowerer.reaches(Reach::Terminal) && matches!(value, Expr::Lambda { .. })) {
      return bind_pattern(&mut self.lowerer, pattern, value);
    }
    let name = self.lowerer.temp();
    self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
    bind_pattern(&mut self.lowerer, pattern, Expr::Var(name))
  }

  pub(super) fn bind_returned(&mut self, pattern: &'p js::BindingPattern<'p>, returned: Returned, prefix: &str) -> Lowered<()> {
    let part_into = |this: &mut Self, pattern: &js::BindingPattern, part: Option<Part>| -> Lowered<()> {
      match part {
        Some(Part::Handler(bound)) => match pattern {
          js::BindingPattern::BindingIdentifier(id) => {
            this.alias_handler(id.name.as_str(), &bound);
            Ok(())
          }
          other => Err(this.lowerer.residue(other.span(), "a function a custom hook returns bound to a pattern")),
        },
        Some(Part::Value(value)) => this.bind_value(pattern, value),
        None => this.bind_value(pattern, Expr::Lit(Lit::Null)),
      }
    };
    match (pattern, returned) {
      (js::BindingPattern::BindingIdentifier(id), Returned::Handler(bound)) => {
        self.alias_handler(id.name.as_str(), &bound);
        Ok(())
      }
      (js::BindingPattern::BindingIdentifier(id), Returned::Value(value)) => {
        let name = format!("{prefix}{}", id.name);
        self.pending_lets.push(Stmt::Let { name: name.clone(), expr: value });
        self.lowerer.scope.push((id.name.to_string(), Expr::Var(name)));
        Ok(())
      }
      (js::BindingPattern::ObjectPattern(obj), Returned::Object(mut parts)) => {
        for prop in crate::syntax::pat_props(obj) {
          match prop {
            crate::syntax::PatProp::Assign(a) => {
              let key = a.key.name.to_string();
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              let b = self.lowerer.build();
              let target = js::BindingPattern::BindingIdentifier(b.boxed(b.copy(a.key)));
              match (part, &a.value) {
                (Some(Part::Value(value)), Some(default)) => {
                  let default = self.lowerer.expr(default)?;
                  self.bind_value(&target, Expr::Coalesce(Box::new(value), Box::new(default)))?;
                }
                (part, _) => part_into(self, &target, part)?,
              }
            }
            crate::syntax::PatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
              let part = parts.iter().position(|(k, _)| *k == key).map(|at| parts.remove(at).1);
              part_into(self, &kv.value, part)?;
            }
            crate::syntax::PatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns")),
          }
        }
        Ok(())
      }
      (js::BindingPattern::ArrayPattern(arr), Returned::Array(mut parts)) => {
        if let Some(r) = &arr.rest {
          return Err(self.lowerer.residue(r.span, "a rest of what a custom hook returns"));
        }
        for (i, elem) in arr.elements.iter().enumerate() {
          let part = if i < parts.len() { parts[i].take() } else { None };
          if let Some(pattern) = elem {
            part_into(self, pattern, part)?;
          }
        }
        Ok(())
      }
      (pattern, Returned::Object(parts)) => {
        let mut entries = Vec::new();
        for (key, part) in parts {
          if let Part::Value(value) = part {
            entries.push(Entry::Field(key, value));
          }
        }
        self.bind_value(pattern, Expr::Object(entries))
      }
      (pattern, Returned::Array(parts)) => {
        let items = parts.into_iter().map(|p| Entry::Item(match p {
          Some(Part::Value(value)) => value,
          _ => Expr::Lit(Lit::Null),
        })).collect();
        self.bind_value(pattern, Expr::Array(items))
      }
      (pattern, Returned::Value(value)) => self.bind_value(pattern, value),
      (pattern, Returned::Handler(_)) => Err(self.lowerer.residue(pattern.span(), "a function a custom hook returns bound to a pattern")),
    }
  }
}
