use super::*;

impl<'a, 'p> ComponentLowerer<'a, 'p> {
  pub(super) fn component(&mut self, found: &Found<'p>) -> Lowered<(Component, Vec<(String, (usize, usize))>)> {
    let depth = self.lowerer.scope.len();
    let (params, body): (Vec<crate::Param<'p>>, FunctionBody<'p>) = match found {
      Found::Declared(params, stmts, block) => {
        self.hook = Some(Hook::Block { after: self.lowerer.parsed.range(*block).start + 1 });
        (crate::params_of(params), FunctionBody::Block(stmts))
      }
      Found::Arrow(arrow) => {
        self.hook = Some(match crate::syntax::arrow_body_of(arrow) {
          crate::syntax::ArrowBody::Block(b) => Hook::Block { after: self.lowerer.parsed.range(b.span).start + 1 },
          crate::syntax::ArrowBody::Expr(e) => Hook::Expression(self.lowerer.parsed.range(e.span())),
        });
        (crate::params_of(&arrow.params), arrow_body(arrow))
      }
    };
    let floor = self.lowerer.scope.len();
    self.bind_props(&params)?;
    if self.forwards_ref {
      if let Some(js::BindingPattern::BindingIdentifier(id)) = params.get(1).map(|p| p.pattern) {
        self.lowerer.scope.push((id.name.to_string(), Expr::Field(Box::new(Expr::Var("$props".to_owned())), "ref".to_owned())));
      }
    }
    let mut lets = Vec::new();
    if let Some(class) = self.class {
      lets.extend(self.bind_this(class)?);
    }
    let render = match body {
      FunctionBody::Expr(e) => self.child_expr(e)?,
      FunctionBody::Block(stmts) => {
        let mut render = None;
        let mut early: Vec<(Expr, Tmpl)> = Vec::new();
        // A value bound after an early `return` is computed only where none of them fired, since the body binds every value before the tree picks a branch.
        let guarded = |early: &[(Expr, Tmpl)], expr: Expr| match early.iter().map(|(cond, _)| cond.clone()).reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))) {
          None => expr,
          Some(fired) => Expr::Ternary(Box::new(fired), Box::new(Expr::Lit(Lit::Null)), Box::new(expr)),
        };
        // A function declaration is hoisted: a handler above it calls it by name.
        self.lowerer.local_fns.clear();
        for stmt in stmts.iter() {
          if let (js::Statement::FunctionDeclaration(f), Some(id)) = (stmt, function_name(stmt)) {
            if let Some(body) = &f.body {
              self.handler_fns.insert(id.to_owned(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
              self.lowerer.local_fns.push((id.to_owned(), &**f));
            }
          }
        }
        self.declare_below(stmts, "");
        for (at, stmt) in stmts.iter().enumerate() {
          match stmt {
            js::Statement::ReturnStatement(ret) => {
              render = Some(match ret.argument.as_ref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              });
              break;
            }
            js::Statement::IfStatement(branch) if holds_return(stmt) && early_return(branch).is_none() => {
              let rest: Vec<&'p js::Statement<'p>> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) | js::Statement::TryStatement(_) if holds_return(stmt) => {
              let rest: Vec<&'p js::Statement<'p>> = stmts[at..].iter().collect();
              render = Some(self.tree_of(&rest, floor)?);
              break;
            }
            js::Statement::IfStatement(branch) if early_return(branch).is_some() => {
              let ret = early_return(branch).expect("checked");
              let cond = self.lowerer.expr(&branch.test)?;
              let then = match ret.argument.as_ref() {
                Some(arg) => self.child_expr(arg)?,
                None => Tmpl::Fragment(Vec::new()),
              };
              early.push((cond, then));
            }
            js::Statement::VariableDeclaration(var) if !early.is_empty() => {
              self.premake_stmt(stmt)?;
              for decl in &var.declarations {
                if let Some(init) = decl.init.as_ref() {
                  if let Some((hook, _)) = hook_call(&self.meanings(), init).filter(|(hook, _)| *hook != "use") {
                    return Err(self.lowerer.residue(decl.span, format!("`{hook}` after an early `return`, which React refuses since the hooks would change between renders")));
                  }
                }
                let stmt = self.let_stmt(decl)?;
                self.note_alias_of(decl, floor)?;
                for pending in std::mem::take(&mut self.pending_lets) {
                  if let Stmt::Let { name, expr } = pending {
                    lets.push(Stmt::Let { name, expr: guarded(&early, expr) });
                  }
                }
                if let Some(Stmt::Let { name, expr }) = stmt {
                  lets.push(Stmt::Let { name, expr: guarded(&early, expr) });
                }
              }
            }
            js::Statement::ExpressionStatement(e) if library_hook(&self.meanings(), &e.expression).is_some_and(|(name, _)| name == "useInput") => {
              let (_, call) = library_hook(&self.meanings(), &e.expression).expect("checked");
              self.input_hook(call)?;
            }
            js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| self.custom_hook(name)) => {
              let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
              self.inline_hook(hook, call, None)?;
              lets.append(&mut self.pending_lets);
            }
            js::Statement::ExpressionStatement(e) if !hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Statement::IfStatement(_) | js::Statement::SwitchStatement(_) | js::Statement::BlockStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::ForStatement(_) | js::Statement::TryStatement(_) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            js::Statement::FunctionDeclaration(f) => {
              let name = f.id.as_ref().map(|id| id.name.to_string()).unwrap_or_default();
              self.browser_function(name.clone());
              if let Some(body) = &f.body {
                self.handler_fns.insert(name, (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
              }
            }
            js::Statement::ExpressionStatement(e) if hook_call(&self.meanings(), &e.expression).is_some_and(|(name, _)| EFFECT_HOOKS.contains(&name)) => {
              let (hook, call) = hook_call(&self.meanings(), &e.expression).expect("checked");
              if hook == "useEffect" || hook == "useLayoutEffect" {
                self.effect_timers(call)?;
                lets.append(&mut self.pending_lets);
              }
            }
            js::Statement::VariableDeclaration(var) => {
              self.premake_stmt(stmt)?;
              for decl in &var.declarations {
                let stmt = self.let_stmt(decl)?;
                self.note_alias_of(decl, floor)?;
                lets.append(&mut self.pending_lets);
                if let Some(stmt) = stmt {
                  lets.push(stmt);
                }
              }
            }
            js::Statement::WhileStatement(_) if self.lowerer.reaches(Reach::Terminal) => {
              self.premake_stmt(stmt)?;
              self.rebound(stmt, floor, &early, &mut lets, &guarded)?
            }
            other => return Err(self.lowerer.residue(other.span(), "a statement a component cannot hold before its `return`")),
          }
        }
        let mut asked: Vec<String> = Vec::new();
        for (_, state) in &self.setters {
          let key = format!("{}{state}", snapfire_fsr_ir::render::RERUN_PREFIX);
          if !asked.contains(&key) && self.lowerer.scope.iter().any(|(name, _)| *name == key) {
            lets.push(Stmt::Let { name: key.clone(), expr: current(&self.lowerer, &key) });
            asked.push(key);
          }
        }
        let render = render.ok_or_else(|| self.lowerer.residue(Span::default(), "a component must return its tree"))?;
        early.into_iter().rev().fold(render, |r#else, (cond, then)| Tmpl::If { cond, then: Box::new(then), r#else: Some(Box::new(r#else)) })
      }
    };
    self.lowerer.scope.truncate(depth);
    // A module-level `let` a function writes is state the store holds per request, seeded by the module's own initialisation; a handler's write to it reaches the store as any store write does.
    let mut bound = Vec::new();
    for (name, key, seed) in std::mem::take(&mut self.module_lets) {
      if self.lowerer.reaches(Reach::Later) {
        bound.push(Stmt::Let { name, expr: Expr::ModuleCell { key: Box::new(Expr::lit_str(key)), init: Box::new(seed) } });
        continue;
      }
      self.state.push(name.clone());
      self.state_bindings.push(name.clone());
      self.store_bindings.push((name.clone(), key.clone()));
      bound.push(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(seed)) });
    }
    bound.extend(lets);
    let mut lets = bound;
    lets.extend(std::mem::take(&mut self.store_lets));
    for (bound, span) in std::mem::take(&mut self.lowerer.read_below) {
      if !lets.iter().any(|stmt| matches!(stmt, Stmt::Let { name, .. } if *name == bound)) {
        return Err(self.lowerer.residue(span, format!("`{bound}`, declared below what reads it, is not a value the body binds")));
      }
    }
    self.lowerer.declared_below.clear();
    self.lowered_handlers.append(&mut self.lowerer.continuations);
    Ok((Component { body: lets, render, state: std::mem::take(&mut self.state_bindings), stores: std::mem::take(&mut self.store_bindings), handlers: std::mem::take(&mut self.lowered_handlers), owner: Owner::React, shadow: None, scope: None }, std::mem::take(&mut self.refs)))
  }

  /// Records `decl` as an alias when it binds a name to an element of a local array (`note_alias`).
  pub(super) fn note_alias_of(&mut self, decl: &js::VariableDeclarator, floor: usize) -> Lowered<()> {
    let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) else { return Ok(()) };
    let frozen = self.state.clone();
    note_alias(&mut self.lowerer, id.name.as_str(), init, &Writable { floor, frozen: &frozen, setters: &[] })
  }

  /// The statements from a branching `return` on, as the tree each path returns: a branch is a `Tmpl::If`, a declaration a `Tmpl::Let` over what follows it, a write to a local a `Tmpl::Let` of its new value. A path that ends without a `return` renders nothing.
  pub(super) fn tree_of(&mut self, stmts: &[&'p js::Statement<'p>], floor: usize) -> Lowered<Tmpl> {
    let Some((first, rest)) = stmts.split_first() else { return Ok(Tmpl::Fragment(Vec::new())) };
    let joined = |head: &'p js::Statement<'p>, rest: &[&'p js::Statement<'p>]| -> Vec<&'p js::Statement<'p>> {
      let mut out: Vec<&'p js::Statement<'p>> = match head {
        js::Statement::BlockStatement(b) => b.body.iter().collect(),
        other => vec![other],
      };
      out.extend_from_slice(rest);
      out
    };
    match first {
      js::Statement::ReturnStatement(ret) => match ret.argument.as_ref() {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      },
      js::Statement::ThrowStatement(thrown) => Ok(Tmpl::Expr(throw_expr(&mut self.lowerer, thrown)?)),
      js::Statement::EmptyStatement(_) => self.tree_of(rest, floor),
      js::Statement::BlockStatement(_) if holds_return(first) => {
        let depth = self.lowerer.scope.len();
        let tree = self.tree_of(&joined(first, rest), floor);
        self.lowerer.scope.truncate(depth);
        tree
      }
      js::Statement::IfStatement(branch) if holds_return(first) => {
        let cond = self.lowerer.expr(&branch.test)?;
        let depth = self.lowerer.scope.len();
        let then = self.tree_of(&joined(&branch.consequent, rest), floor);
        self.lowerer.scope.truncate(depth);
        let r#else = match branch.alternate.as_ref() {
          Some(alt) => self.tree_of(&joined(alt, rest), floor),
          None => self.tree_of(rest, floor),
        };
        self.lowerer.scope.truncate(depth);
        Ok(Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(r#else?)) })
      }
      js::Statement::SwitchStatement(sw) if holds_return(first) => {
        let disc = self.lowerer.expr(&sw.discriminant)?;
        let (arms, default) = switch_arms(&self.lowerer, sw)?;
        let depth = self.lowerer.scope.len();
        let mut fallback: Vec<&'p js::Statement<'p>> = default.map(|d| d.iter().collect()).unwrap_or_default();
        fallback.extend_from_slice(rest);
        let tree = self.tree_of(&fallback, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = tree?;
        for (tests, body) in arms.into_iter().rev() {
          let mut cond: Option<Expr> = None;
          for test in tests {
            let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(self.lowerer.expr(test)?));
            cond = Some(match cond {
              None => eq,
              Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
            });
          }
          let mut path: Vec<&'p js::Statement<'p>> = body.iter().collect();
          path.extend_from_slice(rest);
          let then = self.tree_of(&path, floor);
          self.lowerer.scope.truncate(depth);
          let cond = cond.unwrap_or(Expr::Lit(Lit::Bool(false)));
          tree = Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(tree)) };
        }
        Ok(tree)
      }
      js::Statement::VariableDeclaration(var) => {
        self.premake_stmt(first)?;
        let depth = self.lowerer.scope.len();
        let mut lets = Vec::new();
        for decl in &var.declarations {
          if let Some((hook, _)) = decl.init.as_ref().and_then(|init| hook_call(&self.meanings(), init)).filter(|(hook, _)| !matches!(*hook, "use" | "useContext")) {
            return Err(self.lowerer.residue(decl.span, format!("`{hook}` after a `return`, which React refuses since the hooks would change between renders")));
          }
          let expr = match decl.init.as_ref() {
            Some(init) => self.lowerer.expr(init)?,
            None => Expr::Lit(Lit::Undefined),
          };
          let name = match &decl.id {
            js::BindingPattern::BindingIdentifier(id) => {
              let name = id.name.to_string();
              self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
              if let Some(init) = decl.init.as_ref() {
                self.lowerer.note_kind(&name, init);
              }
              self.note_alias_of(decl, floor)?;
              name
            }
            pattern => {
              let name = self.lowerer.temp();
              bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
              name
            }
          };
          lets.push((name, expr));
        }
        let then = self.tree_of(rest, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = then?;
        for (name, expr) in lets.into_iter().rev() {
          tree = Tmpl::Let { name, expr, then: Box::new(tree) };
        }
        Ok(tree)
      }
      js::Statement::FunctionDeclaration(f) => Err(self.lowerer.residue(f.span, "a function declared after a branching `return`; declare it before")),
      js::Statement::TryStatement(t) if holds_return(first) => self.try_tree(t, rest, floor),
      straight => {
        self.premake_stmt(straight)?;
        let depth = self.lowerer.scope.len();
        let frozen = self.state.clone();
        let writable = Writable { floor, frozen: &frozen, setters: &[] };
        let mut assigned = Vec::new();
        let (held, run) = held_frame(&mut self.lowerer, |lowerer| rebind_stmt(lowerer, straight, &writable, &mut assigned));
        run?;
        let mut lets: Vec<(String, Expr)> = held.into_iter().filter_map(|stmt| match stmt {
          Stmt::Let { name, expr } => Some((name, expr)),
          _ => None,
        }).collect();
        for name in assigned {
          let value = current(&self.lowerer, &name);
          let fresh = self.lowerer.temp();
          keep_kind(&mut self.lowerer, &name, &value, Expr::Var(fresh.clone()));
          self.lowerer.scope.push((name, Expr::Var(fresh.clone())));
          lets.push((fresh, value));
        }
        let then = self.tree_of(rest, floor);
        self.lowerer.scope.truncate(depth);
        let mut tree = then?;
        for (name, expr) in lets.into_iter().rev() {
          tree = Tmpl::Let { name, expr, then: Box::new(tree) };
        }
        Ok(tree)
      }
    }
  }

  /// A `try` that returns markup, then what follows it. The `try` runs as statements. It ends with which of its `return`s it reached and the locals in scope there. The tree picks that `return`'s markup over those locals. When it runs to its end it holds the locals it wrote and the tree goes on with what follows. A failure while the markup itself evaluates is not the `try`'s to catch.
  pub(super) fn try_tree(&mut self, t: &'p js::TryStatement<'p>, rest: &[&'p js::Statement<'p>], floor: usize) -> Lowered<Tmpl> {
    self.premake_stmt_try(t)?;
    let frozen: Vec<String> = self.state.clone();
    let writable = Writable { floor, frozen: &frozen, setters: &[] };
    let base = self.lowerer.scope.len();
    let outer = (self.lowerer.markup_base.replace(base), std::mem::take(&mut self.lowerer.markup_returns));
    let ran = try_run(&mut self.lowerer, t, &writable);
    let returns = std::mem::replace(&mut self.lowerer.markup_returns, outer.1);
    self.lowerer.markup_base = outer.0;
    let (ran, written) = ran?;
    let held = self.lowerer.temp();
    let read = |key: &str| Expr::Field(Box::new(Expr::Var(held.clone())), key.to_owned());
    let depth = self.lowerer.scope.len();
    for name in &written {
      self.lowerer.scope.push((name.clone(), read(name)));
    }
    let after = self.tree_of(rest, floor);
    self.lowerer.scope.truncate(depth);
    let mut tree = after?;
    let found = returns_in(t);
    for (path, (at, names)) in returns.into_iter().enumerate().rev() {
      let Some(ret) = found.iter().find(|ret| crate::lo(ret.span) == at) else { return Err(self.lowerer.residue(t.span, "a `return` inside a `try` the build could not place")) };
      for (k, name) in names.iter().enumerate() {
        self.lowerer.scope.push((name.clone(), read(&format!("${k}"))));
      }
      let then = match ret.argument.as_ref() {
        Some(arg) => self.child_expr(arg),
        None => Ok(Tmpl::Fragment(Vec::new())),
      };
      self.lowerer.scope.truncate(depth);
      let cond = Expr::Compare(CompareOp::Eq, Box::new(read("$path")), Box::new(Expr::Lit(Lit::Int(path as i128))));
      tree = Tmpl::If { cond, then: Box::new(then?), r#else: Some(Box::new(tree)) };
    }
    Ok(Tmpl::Let { name: held, expr: ran, then: Box::new(tree) })
  }

  pub(super) fn premake_stmt_try(&mut self, t: &'p js::TryStatement<'p>) -> Lowered<()> {
    self.premake_block(&t.block.body)?;
    if let Some(catch) = &t.handler {
      let depth = self.lowerer.scope.len();
      let made = catch_param(&mut self.lowerer, catch.param.as_ref()).and_then(|_| self.premake_block(&catch.body.body));
      self.lowerer.scope.truncate(depth);
      made?;
    }
    match &t.finalizer {
      Some(finalizer) => self.premake_block(&finalizer.body),
      None => Ok(()),
    }
  }

  /// A statement that writes locals, run over the scope: each name it wrote is bound to its final value by a `let` of its own.
  pub(super) fn rebound(&mut self, stmt: &js::Statement, floor: usize, early: &[(Expr, Tmpl)], lets: &mut Vec<Stmt>, guarded: &dyn Fn(&[(Expr, Tmpl)], Expr) -> Expr) -> Lowered<()> {
    let frozen: Vec<String> = self.state.iter().filter(|name| !self.held_refs.contains(name)).cloned().collect();
    let setters = self.setters.clone();
    let writable = Writable { floor, frozen: &frozen, setters: &setters };
    let mut assigned = Vec::new();
    let (held, run) = held_frame(&mut self.lowerer, |lowerer| rebind_stmt(lowerer, stmt, &writable, &mut assigned));
    run?;
    for held in held {
      if let Stmt::Let { name, expr } = held {
        lets.push(Stmt::Let { name, expr: guarded(early, expr) });
      }
    }
    for name in assigned {
      let value = current(&self.lowerer, &name);
      let fresh = self.lowerer.temp();
      keep_kind(&mut self.lowerer, &name, &value, Expr::Var(fresh.clone()));
      lets.push(Stmt::Let { name: fresh.clone(), expr: guarded(early, value) });
      if self.held_refs.contains(&name) {
        lets.push(Stmt::Let { name: format!("{}{name}", snapfire_fsr_ir::render::HOLD_PREFIX), expr: Expr::Var(fresh.clone()) });
      }
      self.lowerer.scope.push((name, Expr::Var(fresh)));
    }
    Ok(())
  }

  /// `components={{ h2: Heading }}` on a placed MDX file: each element and the
  /// component this file's name for it resolves to.
  pub(super) fn component_overrides(&mut self, attr: &'p js::JSXAttribute<'p>) -> Lowered<Vec<Override>> {
    let object = match attr_expression(attr) {
      Some(e) => match e {
        js::Expression::ObjectExpression(object) => object,
        other => return Err(self.lowerer.residue(other.span(), "`components` that is not an object written out, as `{{ h2: Heading }}`")),
      },
      _ => return Err(self.lowerer.residue(attr.span, "`components` that is not an object written out, as `{{ h2: Heading }}`")),
    };
    let mut out = Vec::new();
    for prop in &object.properties {
      let (key, local) = match prop {
        js::ObjectPropertyKind::ObjectProperty(p) => match crate::syntax::prop_of(p) {
          crate::syntax::Prop::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
            (Some(key), js::Expression::Identifier(local)) => (key, local.name.to_string()),
            _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
          },
          crate::syntax::Prop::Shorthand(id) => (id.name.to_string(), id.name.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a `components` entry that is not a component's name")),
        },
        js::ObjectPropertyKind::SpreadProperty(spread) => return Err(self.lowerer.residue(spread.argument.span(), "a spread in `components`")),
      };
      match self.meanings().bindings.get(&local) {
        Some(crate::meaning::Meaning::Own { file, name }) => out.push(Override { key, specifier: format!("@app/{file}"), export: name.clone() }),
        Some(meant @ crate::meaning::Meaning::Library(_)) if meant.placement().is_some() => {
          let Some((source, imported)) = find_import(self.lowerer.parsed, &local) else { return Err(self.lowerer.residue(prop.span(), format!("`{local}` is not imported"))) };
          out.push(Override { key, specifier: source, export: imported });
        }
        _ => return Err(self.lowerer.residue(prop.span(), format!("`{local}` is not a component this file declares or imports"))),
      }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(out)
  }

  /// The id a placement of `target` names: the file itself, or with
  /// `overrides` a variant of it the set lowers with those elements replaced.
  pub(super) fn variant_of(&mut self, target: &str, overrides: &[Override]) -> String {
    if overrides.is_empty() {
      return target.to_owned();
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for o in overrides {
      for byte in format!("{}={}:{};", o.key, o.specifier, o.export).bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
      }
    }
    let variant = format!("{target}~{:08x}", hash as u32);
    if !self.variants.iter().any(|(v, _, _)| *v == variant) {
      self.variants.push((variant.clone(), target.to_owned(), overrides.to_vec()));
    }
    variant
  }

  /// `const { default: Post, meta } = await import(`../content/${slug}.mdx`)`:
  /// `Post` places whichever file the key names and `meta` is that file's
  /// export, each chosen by comparing the key with the text that names a file.
  pub(super) fn template_import(&mut self, decl: &'p js::VariableDeclarator<'p>, call: &'p js::ImportExpression<'p>) -> Lowered<()> {
    let Some(choices) = self.imports.get(&crate::lo(call.span)).cloned() else {
      return Err(self.lowerer.residue(call.span, "an `import()` of something other than a template with one substitution, such as `../content/${slug}.mdx`"));
    };
    let js::Expression::TemplateLiteral(tpl) = &call.source else { unreachable!("a template import") };
    let key = self.lowerer.expr(&tpl.expressions[0])?;
    let js::BindingPattern::ObjectPattern(pattern) = &decl.id else {
      return Err(self.lowerer.residue(decl.span, "an imported module bound whole; destructure it, as `const { default: Post, meta } = await import(...)`"));
    };
    if choices.is_empty() {
      return Err(self.lowerer.residue(call.span, "an `import()` no file under the app matches"));
    }
    let picks = |value: &str| Expr::Compare(CompareOp::Eq, Box::new(Expr::Str(Box::new(key.clone()))), Box::new(Expr::lit_str(value)));
    for prop in crate::syntax::pat_props(pattern) {
      let (export, local) = match prop {
        crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
          (Some(export), js::BindingPattern::BindingIdentifier(local)) => (export, local.name.to_string()),
          _ => return Err(self.lowerer.residue(prop.span(), "a nested pattern in an imported module's destructuring")),
        },
        crate::syntax::PatProp::Assign(a) if a.value.is_none() => (a.key.name.to_string(), a.key.name.to_string()),
        other => return Err(self.lowerer.residue(other.span(), "a default or a rest in an imported module's destructuring")),
      };
      if export == "default" {
        if !local.starts_with(|c: char| c.is_ascii_uppercase()) {
          return Err(self.lowerer.residue(prop.span(), format!("`{local}` names a component; capitalise it so JSX places it")));
        }
        self.chosen.insert(local, (key.clone(), choices.clone()));
        continue;
      }
      let mut value = Expr::Lit(Lit::Null);
      for (text, target) in choices.iter().rev() {
        let Some((_, export_value)) = self.lowerer.globals.iter().find(|(k, _)| *k == format!("{target}#{export}")).cloned() else {
          return Err(self.lowerer.residue(prop.span(), format!("`{export}`, which `{target}` does not export as a value the build reads")));
        };
        value = Expr::Ternary(Box::new(picks(text)), Box::new(export_value), Box::new(value));
      }
      let fresh = self.lowerer.temp();
      self.pending_lets.push(Stmt::Let { name: fresh.clone(), expr: value });
      self.lowerer.scope.push((local, Expr::Var(fresh)));
    }
    Ok(())
  }

  /// `this` in a class's `render()`: `this.properties` the props, `this.state` the initial state as component state, each method and arrow field a function the browser runs, `createRef()` a ref holding null and each other field its value.
  pub(super) fn bind_this(&mut self, class: &'p js::Class<'p>) -> Lowered<Option<Stmt>> {
    let mut entries = vec![Entry::Field("props".to_owned(), Expr::Var("$props".to_owned()))];
    let mut state: Option<Expr> = None;
    let browser = || Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER);
    for member in &class.body.body {
      match member {
        js::ClassElement::PropertyDefinition(p) if !p.r#static => {
          let Some(key) = prop_name(&p.key, p.computed) else { return Err(self.lowerer.residue(p.span, "a computed class field")) };
          let value = match p.value.as_ref().map(crate::meaning::bare) {
            None => Expr::Lit(Lit::Null),
            Some(js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => browser(),
            Some(js::Expression::CallExpression(call)) if matches!(&call.callee, c if react_export(self.lowerer.parsed, c).as_deref() == Some("createRef")) => Expr::Object(vec![Entry::Field("current".to_owned(), Expr::Lit(Lit::Null))]),
            Some(value) => self.lowerer.expr(value)?,
          };
          if key == "state" {
            state = Some(value);
          } else {
            entries.push(Entry::Field(key, value));
          }
        }
        js::ClassElement::MethodDefinition(m) if !m.r#static && m.kind != js::MethodDefinitionKind::Constructor => {
          if let Some(key) = prop_name(&m.key, m.computed).filter(|k| k != "render") {
            entries.push(Entry::Field(key, browser()));
          }
        }
        js::ClassElement::MethodDefinition(c) if c.kind == js::MethodDefinitionKind::Constructor => {
          let depth = self.lowerer.scope.len();
          if let Some(id) = crate::first_param(&c.value.params).and_then(|p| p.ident()) {
            self.lowerer.scope.push((id.name.to_string(), Expr::Var("$props".to_owned())));
          }
          for stmt in c.value.body.iter().flat_map(|b| &b.statements) {
            let js::Statement::ExpressionStatement(e) = stmt else { return Err(self.lowerer.residue(stmt.span(), "a constructor statement other than `super(props)` or a write to `this`")) };
            match &e.expression {
              js::Expression::CallExpression(call) if matches!(call.callee, js::Expression::Super(_)) => {}
              js::Expression::AssignmentExpression(a) if a.operator == js::AssignmentOperator::Assign => {
                let field = match &a.left {
                  target if target.as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::ThisExpression(_))) => target.as_member_expression().and_then(prop_name_of),
                  _ => None,
                };
                let Some(field) = field else { return Err(self.lowerer.residue(a.span, "a constructor write to something other than `this`")) };
                if field == "state" {
                  state = Some(self.lowerer.expr(&a.right)?);
                } else if !entries.iter().any(|e| matches!(e, Entry::Field(n, _) if *n == field)) {
                  let value = match crate::meaning::bare(&a.right) {
                    js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) | js::Expression::CallExpression(_) => browser(),
                    other => self.lowerer.expr(other)?,
                  };
                  entries.push(Entry::Field(field, value));
                }
              }
              other => return Err(self.lowerer.residue(other.span(), "a constructor statement other than `super(props)` or a write to `this`")),
            }
          }
          self.lowerer.scope.truncate(depth);
        }
        _ => {}
      }
    }
    let stmt = state.map(|expr| {
      let name = "this$state".to_owned();
      self.state.push(name.clone());
      self.state_bindings.push(name.clone());
      entries.push(Entry::Field("state".to_owned(), Expr::Var(name.clone())));
      Stmt::Let { name, expr }
    });
    self.lowerer.scope.push(("this".to_owned(), Expr::Object(entries)));
    Ok(stmt)
  }

  pub(super) fn bind_props(&mut self, params: &[crate::Param]) -> Lowered<()> {
    if self.positional {
      for (i, param) in params.iter().enumerate() {
        bind_param(&mut self.lowerer, param, Expr::Field(Box::new(Expr::Var("$props".to_owned())), i.to_string()))?;
      }
      return Ok(());
    }
    let Some(first) = params.first() else { return Ok(()) };
    for (key, kind) in prop_kinds_of(self.lowerer.parsed, first) {
      match self.lowerer.prop_kinds.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = kind,
        None => self.lowerer.prop_kinds.push((key, kind)),
      }
    }
    if first.default.is_some() || first.rest {
      return Err(self.lowerer.residue(first.span, "the props parameter must be a name or a destructuring"));
    }
    match first.pattern {
      js::BindingPattern::BindingIdentifier(id) => {
        self.props_name = Some(id.name.to_string());
        self.lowerer.scope.push((id.name.to_string(), Expr::Var("$props".to_owned())));
        Ok(())
      }
      js::BindingPattern::ObjectPattern(obj) => {
        self.children_name = crate::syntax::pat_props(obj).into_iter().find_map(|prop| match prop {
          crate::syntax::PatProp::Assign(a) if a.key.name.as_str() == "children" => Some("children".to_owned()),
          crate::syntax::PatProp::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("children") => match &kv.value {
            js::BindingPattern::BindingIdentifier(local) => Some(local.name.to_string()),
            _ => None,
          },
          _ => None,
        });
        self.props_rest = match (&self.children_name, &obj.rest) {
          (None, Some(rest)) => match &rest.argument {
            js::BindingPattern::BindingIdentifier(local) => Some(local.name.to_string()),
            _ => None,
          },
          _ => None,
        };
        if self.layout_root {
          for prop in crate::syntax::pat_props(obj) {
            let (local, key) = match prop {
              crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
              crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
                (Some(key), js::BindingPattern::BindingIdentifier(local)) => (local.name.to_string(), key),
                _ => continue,
              },
              crate::syntax::PatProp::Rest(_) => continue,
            };
            if self.slot_names.contains(&key) {
              self.slot_props.push((local, key));
            }
          }
        }
        bind_object(&mut self.lowerer, obj, Expr::Var("$props".to_owned()))?;
        for prop in crate::syntax::pat_props(obj) {
          let (local, key) = match prop {
            crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
            crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
              (Some(key), js::BindingPattern::BindingIdentifier(local)) => (local.name.to_string(), key),
              _ => continue,
            },
            crate::syntax::PatProp::Rest(_) => continue,
          };
          let Some(kind) = self.lowerer.prop_kinds.iter().find(|(k, _)| *k == key).map(|(_, kind)| *kind) else { continue };
          let Some((_, bound)) = self.lowerer.scope.iter().rev().find(|(n, _)| *n == local) else { continue };
          let bound = bound.clone();
          self.lowerer.kinds.push((local, bound, kind));
        }
        Ok(())
      }
      other => Err(self.lowerer.residue(other.span(), "the props parameter must be a name or a destructuring")),
    }
  }

  /// `const x = e` as a `let`; `const [x, setX] = useState(e)` as `let x = e`
  /// with `setX` a handler; `const { a, b } = e` as one `let` plus field reads.
  /// A function value, `useCallback` included, is a handler.
  pub(super) fn let_stmt(&mut self, decl: &'p js::VariableDeclarator<'p>) -> Lowered<Option<Stmt>> {
    let Some(init) = decl.init.as_ref() else {
      let js::BindingPattern::BindingIdentifier(name) = &decl.id else { return Err(self.lowerer.residue(decl.span, "a destructuring without a value")) };
      let local = name.name.to_string();
      self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
      return Ok(Some(Stmt::Let { name: local, expr: Expr::Lit(Lit::Undefined) }));
    };
    if let Some(call) = awaited_import(init) {
      self.template_import(decl, call)?;
      return Ok(None);
    }
    if let Some(("useAnimation", call)) = library_hook(&self.meanings(), init) {
      return self.animation_hook(call, &decl.id, decl.span).map(Some);
    }
    match &decl.id {
      js::BindingPattern::BindingIdentifier(name) => {
        let local = name.name.to_string();
        if self.lowerer.bind_formatter(&local, init) {
          return Ok(None);
        }
        let expr = match hook_call(&self.meanings(), init) {
          Some((hook, call)) if FSR_HOOKS.contains(&hook) && self.foreign_callee(call) => {
            return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
          }
          Some(("useCallback", call)) => {
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
              self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
            }
            if matches!(call.arguments.get(1).map(|a| crate::syntax::arg_expr(a)), Some(js::Expression::ArrayExpression(deps)) if deps.elements.iter().all(|d| self.never_changes(d))) {
              self.stable_callbacks.push(local.clone());
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some(("useMemo", call)) => {
            let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) else {
              return Err(self.lowerer.residue(decl.span, "`useMemo` of something other than an arrow"));
            };
            match crate::syntax::arrow_body_of(arrow) {
              crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e)?,
              crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body)?,
            }
          }
          Some(("useRef", call)) => {
            let current = match call.arguments.first() {
              Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
              None => Expr::Lit(Lit::Null),
            };
            self.state.push(local.clone());
            let mut fields = vec![Entry::Field("current".to_owned(), current)];
            if self.lowerer.reaches(Reach::Terminal) {
              self.state_bindings.push(local.clone());
              self.held_refs.push(local.clone());
              let range = self.lowerer.parsed.range(call.span);
              if let Some(candidates) = &mut self.lowerer.hoisting {
                fields.push(Entry::Field(crate::REF_ID.to_owned(), candidates.id(range)));
              }
            }
            Expr::Object(fields)
          }
          Some(("useStore", _)) => {
            return Err(self.lowerer.residue(decl.span, "`useStore` bound to one name; it is a pair, as `const [x, setX] = useStore(key, initial)`"))
          }
          Some(("useLocale", _)) => Expr::Locale,
          Some(("useEffectEvent", call)) => {
            if let Some(js::Expression::ArrowFunctionExpression(arrow)) = call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
              self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
            }
            self.browser_function(local);
            return Ok(None);
          }
          Some(("useLinkStatus", call)) => {
            let expr = self.lowerer.server_hook_value("useLinkStatus", call)?;
            self.state.push(local.clone());
            self.state_bindings.push(local.clone());
            self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
            return Ok(Some(Stmt::Let { name: local, expr }));
          }
          Some((hook @ ("useDeferredValue" | "useFormStatus" | "useSyncExternalStore"), call)) => self.lowerer.server_hook_value(hook, call)?,
          Some(("useId", call)) if !matches!(self.meanings().bindings.get("useId"), Some(crate::meaning::Meaning::Own { .. })) => {
            let range = self.lowerer.parsed.range(call.span);
            let viewer = self.lowerer.reaches(Reach::Terminal);
            match &mut self.lowerer.hoisting {
              Some(candidates) => candidates.id(range),
              // A terminal page has no browser copy whose ids must match, so a hook's own id is the component's to number.
              None if viewer => {
                self.hook_ids += 1;
                Expr::HoistId(HOOK_IDS + self.hook_ids)
              }
              None => return Err(self.lowerer.residue_with(decl.span, "`useId` inside a custom hook", "the id is keyed by the component that calls the hook; call `useId` there and pass the id in")),
            }
          }
          Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(&decl.id)),
          Some((hook, _)) if !matches!(hook, "useState" | "use" | "useContext") => return Err(self.lowerer.residue(decl.span, format!("`{hook}`"))),
          _ if matches!(init, js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => {
            match init {
              js::Expression::ArrowFunctionExpression(arrow) => {
                self.handler_fns.insert(local.clone(), (crate::params_of(&arrow.params), arrow_body(arrow)).into());
                if self.lowerer.reaches(Reach::Terminal) {
                  let unbound = self.lowerer.unbound.take();
                  match self.lowerer.lambda(arrow) {
                    Ok(f) => {
                      self.lowerer.scope.push((local.clone(), f));
                      return Ok(None);
                    }
                    Err(residue) if self.unbound_is_followable() => return Err(residue),
                    Err(_) => self.lowerer.unbound = unbound,
                  }
                }
              }
              js::Expression::FunctionExpression(f) => {
                if let Some(body) = &f.body {
                  self.handler_fns.insert(local.clone(), (crate::params_of(&f.params), FunctionBody::Block(&body.statements)).into());
                }
              }
              _ => {}
            }
            self.browser_function(local);
            return Ok(None);
          }
          _ if functions_only(init) => match self.lowerer.expr(init) {
            Ok(expr) => expr,
            Err(residue) if self.unbound_is_followable() => return Err(residue),
            Err(_) => {
              self.lowerer.unbound = None;
              let js::Expression::ObjectExpression(obj) = init else { unreachable!("checked") };
              let mut entries = Vec::new();
              for prop in &obj.properties {
                let js::ObjectPropertyKind::ObjectProperty(p) = prop else { unreachable!("checked") };
                let crate::syntax::Prop::KeyValue(kv) = crate::syntax::prop_of(p) else { unreachable!("checked") };
                let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.lowerer.residue(kv.key.span(), "a numeric property name"))?;
                let value = match self.handler_body(&kv.value) {
                  Ok(body) => {
                    self.lowered_handlers.push(Handler { event: format!("{local}.{key}"), body });
                    Expr::Handler((self.lowered_handlers.len() - 1) as u32)
                  }
                  Err(_) => {
                    self.lowerer.unbound = None;
                    Expr::Handler(snapfire_fsr_ir::render::BROWSER_HANDLER)
                  }
                };
                entries.push(Entry::Field(key, value));
              }
              self.lowerer.scope.push((local.clone(), Expr::Object(entries.clone())));
              return Ok(Some(Stmt::Let { name: local, expr: Expr::Object(entries) }));
            }
          },
          _ => {
            self.lowerer.browser_value = None;
            match self.lowerer.expr(init) {
              Ok(expr) => expr,
              Err(residue) => match self.lowerer.browser_value.take() {
                Some(reason) => {
                  let (line, column) = self.lowerer.parsed.loc(init.span().start);
                  self.browser_texts.push(BrowserText { wrap: Some(self.lowerer.parsed.range(init.span())), line, column, reason });
                  self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
                  self.lowerer.kinds.push((local.clone(), Expr::Var(local.clone()), crate::Kind::Browser));
                  return Ok(Some(Stmt::Let { name: local, expr: Expr::Lit(Lit::Null) }));
                }
                None => return Err(residue),
              },
            }
          }
        };
        self.lowerer.scope.push((local.clone(), Expr::Var(local.clone())));
        self.lowerer.note_kind(&local, init);
        Ok(Some(Stmt::Let { name: local, expr }))
      }
      js::BindingPattern::ArrayPattern(arr) => {
        let called = hook_call(&self.meanings(), init).map(|(name, _)| name).unwrap_or_default();
        let js::Expression::CallExpression(call) = init else { return self.pattern_stmt(&decl.id, init) };
        if called == "useStore" {
          return self.store_stmt(decl, arr, call);
        }
        if called == "useReducer" {
          return self.reducer_stmt(arr, call);
        }
        if !called.is_empty() && called != "useState" && self.custom_hook(called) {
          return self.inline_hook(called, call, Some(&decl.id));
        }
        if matches!(called, "useTransition" | "useActionState" | "useOptimistic") {
          return self.pending_hook_stmt(called, arr, call);
        }
        if called != "useState" {
          if !called.is_empty() {
            return Err(self.lowerer.residue(decl.span, format!("`{called}`")));
          }
          return self.pattern_stmt(&decl.id, init);
        }
        self.lowerer.browser_value = None;
        let initial = match call.arguments.first() {
          Some(a) => match crate::syntax::arg_expr(a) {
            js::Expression::ArrowFunctionExpression(arrow) => match crate::syntax::arrow_body_of(arrow) {
              crate::syntax::ArrowBody::Expr(e) => self.lowerer.expr(e),
              crate::syntax::ArrowBody::Block(b) => block_to_expr(&mut self.lowerer, &b.body),
            },
            member_expr if member_expr.as_member_expression().is_some_and(|member| is_date_now(member)) => { let member = member_expr.as_member_expression().expect("matched as a member"); self.lowerer.date_now(member.span()) },
            e => self.lowerer.expr(e),
          },
          None => Ok(Expr::Lit(Lit::Null)),
        };
        // State the browser alone can start, `useState(() => new Date())`: null on the server and a browser value wherever it is read.
        let (expr, browser) = match initial {
          Ok(expr) => (expr, false),
          Err(residue) => match self.lowerer.browser_value.take() {
            Some(_) => (Expr::Lit(Lit::Null), true),
            None => return Err(residue),
          },
        };
        let mut names = arr.elements.iter().map(|e| match e {
          Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
          _ => None,
        });
        let state = names.next().flatten();
        let setter = names.next().flatten();
        let Some(name) = state else { return Ok(None) };
        if let Some(setter) = setter {
          self.setters.push((setter.clone(), name.clone()));
          self.browser_function(setter);
        }
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
        self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        if browser {
          self.lowerer.kinds.push((name.clone(), Expr::Var(name.clone()), crate::Kind::Browser));
        } else if let Some(initial) = call.arguments.first() {
          self.lowerer.note_kind(&name, crate::syntax::arg_expr(initial));
        }
        Ok(Some(Stmt::Let { name, expr }))
      }
      pattern => self.pattern_stmt(pattern, init),
    }
  }

  /// `useTransition()` as `[false, startTransition]`, `useActionState(action, initial)` as `[initial, formAction, false]` and `useOptimistic(value, update)` as `[value, addOptimistic]`, which is what React's server renderer returns; each function is the browser's.
  pub(super) fn pending_hook_stmt(&mut self, hook: &str, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let arg = |this: &mut Self, i: usize| -> Lowered<Expr> {
      match call.arguments.get(i) {
        Some(a) => this.lowerer.expr(crate::syntax::arg_expr(a)),
        None => Ok(Expr::Lit(Lit::Null)),
      }
    };
    let pending = Some(Expr::Lit(Lit::Bool(false)));
    let parts: Vec<Option<Expr>> = match hook {
      "useTransition" => vec![pending, None],
      "useActionState" => vec![Some(arg(self, 1)?), None, pending],
      _ => vec![Some(arg(self, 0)?), None],
    };
    let mut stmt = None;
    for (at, (elem, part)) in arr.elements.iter().zip(parts).enumerate() {
      let Some(js::BindingPattern::BindingIdentifier(id)) = elem else { continue };
      let name = id.name.to_string();
      let state = hook == "useActionState" && at == 0;
      match part {
        None => {
          if hook == "useTransition" && self.lowerer.reaches(Reach::Terminal) {
            self.transitions.push(name.clone());
          }
          self.browser_function(name)
        }
        Some(lit @ Expr::Lit(_)) if !state => self.lowerer.scope.push((name, lit)),
        Some(expr) => {
          if state {
            self.state.push(name.clone());
            self.state_bindings.push(name.clone());
          }
          self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
          stmt = Some(Stmt::Let { name, expr });
        }
      }
    }
    Ok(stmt)
  }

  /// `const [state, dispatch] = useReducer(reducer, initial, init?)`: state the way `useState` holds it, starting at `initial` or `init(initial)`, with `dispatch` a handler the browser runs.
  pub(super) fn reducer_stmt(&mut self, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let initial = match call.arguments.get(1) {
      Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
      None => return Err(self.lowerer.residue(call.span, "`useReducer` without an initial state")),
    };
    let expr = match call.arguments.get(2).map(|a| crate::syntax::arg_expr(a)) {
      None => initial,
      Some(js::Expression::ArrowFunctionExpression(arrow)) => Expr::Apply { f: Box::new(self.lowerer.lambda(arrow)?), args: vec![initial] },
      Some(init) => Expr::Apply { f: Box::new(self.lowerer.expr(init)?), args: vec![initial] },
    };
    let mut names = arr.elements.iter().map(|e| match e {
      Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
      _ => None,
    });
    let state = names.next().flatten();
    let dispatch = names.next().flatten();
    if let Some(dispatch) = &dispatch {
      self.browser_function(dispatch.clone());
    }
    let Some(name) = state else { return Ok(None) };
    if let (Some(dispatch), true) = (dispatch, self.lowerer.reaches(Reach::Terminal)) {
      let reducer = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
        Some(js::Expression::ArrowFunctionExpression(arrow)) => self.lowerer.lambda(arrow)?,
        Some(reducer) => self.lowerer.expr(reducer)?,
        None => return Err(self.lowerer.residue(call.span, "`useReducer` without a reducer")),
      };
      self.dispatchers.push((dispatch, name.clone(), reducer));
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr }))
  }

  /// A destructuring in a component body: the value under a temporary each name reads a part of.
  pub(super) fn pattern_stmt(&mut self, pattern: &'p js::BindingPattern<'p>, init: &'p js::Expression<'p>) -> Lowered<Option<Stmt>> {
    if let (Some(_), js::BindingPattern::ObjectPattern(obj)) = (self.class, pattern) {
      if is_this_props(init) {
        self.children_name = crate::syntax::pat_props(obj).into_iter().find_map(|prop| match prop {
          crate::syntax::PatProp::Assign(a) if a.key.name.as_str() == "children" => Some("children".to_owned()),
          crate::syntax::PatProp::KeyValue(kv) if prop_name(&kv.key, kv.computed).as_deref() == Some("children") => match &kv.value {
            js::BindingPattern::BindingIdentifier(id) => Some(id.name.to_string()),
            _ => None,
          },
          _ => None,
        }).or(self.children_name.take());
      }
    }
    let expr = match hook_call(&self.meanings(), init) {
      Some((hook, call)) if FSR_HOOKS.contains(&hook) && self.foreign_callee(call) => {
        return Err(self.lowerer.residue(call.span, format!("`{hook}`, a hook the build cannot follow")));
      }
      Some((hook, call)) if self.custom_hook(hook) => return self.inline_hook(hook, call, Some(pattern)),
      Some(("useLinkStatus", call)) => {
        let expr = self.lowerer.server_hook_value("useLinkStatus", call)?;
        let name = self.lowerer.temp();
        self.state.push(name.clone());
        self.state_bindings.push(name.clone());
        bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
        return Ok(Some(Stmt::Let { name, expr }));
      }
      Some(("useFormStatus", call)) => {
        // Each field is a constant on the server, bound as one so a read of `data` is known to be null.
        let Expr::Object(fields) = self.lowerer.server_hook_value("useFormStatus", call)? else { unreachable!("useFormStatus is an object") };
        let js::BindingPattern::ObjectPattern(obj) = pattern else { return Err(self.lowerer.residue(pattern.span(), "`useFormStatus` destructured as other than an object")) };
        for prop in crate::syntax::pat_props(obj) {
          let (key, local) = match prop {
            crate::syntax::PatProp::Assign(a) => (a.key.name.to_string(), a.key.name.to_string()),
            crate::syntax::PatProp::KeyValue(kv) => match (prop_name(&kv.key, kv.computed), &kv.value) {
              (Some(key), js::BindingPattern::BindingIdentifier(id)) => (key, id.name.to_string()),
              _ => return Err(self.lowerer.residue(prop.span(), "a `useFormStatus` field bound to other than a name")),
            },
            crate::syntax::PatProp::Rest(r) => return Err(self.lowerer.residue(r.span, "a rest of `useFormStatus`")),
          };
          let value = fields.iter().find_map(|f| match f {
            Entry::Field(n, v) if *n == key => Some(v.clone()),
            _ => None,
          }).unwrap_or(Expr::Lit(Lit::Null));
          self.lowerer.scope.push((local, value));
        }
        return Ok(None);
      }
      Some(("useContext" | "use", _)) => self.lowerer.expr(init)?,
      Some((hook @ ("useDeferredValue" | "useSyncExternalStore"), call)) => self.lowerer.server_hook_value(hook, call)?,
      Some((hook, _)) => return Err(self.lowerer.residue(pattern.span(), format!("`{hook}`"))),
      None => self.lowerer.expr(init)?,
    };
    let name = self.lowerer.temp();
    bind_pattern(&mut self.lowerer, pattern, Expr::Var(name.clone()))?;
    Ok(Some(Stmt::Let { name, expr }))
  }

  /// `const [x, setX] = useStore(key, initial)` as `let x = <store key> ?? initial`,
  /// with `setX` a handler. The key must lower to a string: a literal or a
  /// `key()` from the client's store, wherever it is declared.
  pub(super) fn store_stmt(&mut self, decl: &js::VariableDeclarator, arr: &js::ArrayPattern, call: &js::CallExpression) -> Lowered<Option<Stmt>> {
    let Some(first) = call.arguments.first() else {
      return Err(self.lowerer.residue(decl.span, "`useStore` without a key"));
    };
    let key = match self.lowerer.expr(crate::syntax::arg_expr(first))? {
      Expr::Lit(Lit::Str(key)) => key,
      _ => return Err(self.lowerer.residue(crate::syntax::arg_expr(first).span(), "a `useStore` key that is not a string the build can read")),
    };
    let initial = match call.arguments.get(1) {
      Some(a) => self.lowerer.expr(crate::syntax::arg_expr(a))?,
      None => Expr::Lit(Lit::Null),
    };
    let mut names = arr.elements.iter().map(|e| match e {
      Some(js::BindingPattern::BindingIdentifier(id)) => Some(id.name.to_string()),
      _ => None,
    });
    let held = names.next().flatten();
    let setter = names.next().flatten();
    let Some(name) = held else { return Ok(None) };
    if let Some(setter) = setter {
      self.setters.push((setter.clone(), name.clone()));
      self.browser_function(setter);
    }
    self.state.push(name.clone());
    self.state_bindings.push(name.clone());
    self.store_bindings.push((name.clone(), key.clone()));
    self.lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
    Ok(Some(Stmt::Let { name, expr: Expr::Coalesce(Box::new(Expr::Store(key)), Box::new(initial)) }))
  }
}
