use crate::*;

impl<'a> Lowerer<'a> {
  pub(crate) fn new(parsed: &'a Parsed<'a>, defaults: &'a SessionDefaults) -> Self {
    Self { shared: Vec::new(), extra_lets: Vec::new(), parsed, defaults, roots: Vec::new(), scope: Vec::new(), premade: std::collections::HashMap::new(), globals: Vec::new(), global_kinds: Vec::new(), unbound: None, middleware: false, theming: false, meta: false, extends: false, redirects: false, hoisting: None, natives: Vec::new(), in_handler: false, render_path: false, module_level: false, reach_violation: false, temps: 0, kinds: Vec::new(), methods: Vec::new(), prop_kinds: Vec::new(), consulted: std::cell::RefCell::new(Vec::new()), formatters: Vec::new(), global_formatters: Vec::new(), browser_value: None, reaches: Vec::new(), declared_below: Vec::new(), after_body: false, read_below: Vec::new(), meanings: parsed.meanings.get().cloned(), reader: None, browser_functions: Vec::new(), local_fns: Vec::new(), unfollowed: Vec::new(), browser_effects: Vec::new(), provided: Vec::new(), cells: Vec::new(), held: Vec::new(), held_reach: Vec::new(), rebind_reach: Vec::new(), settlers: Vec::new(), continuations: Vec::new(), spawned: Vec::new(), async_floor: None, wrapped_returns: false, loop_tail: None, breakable: Vec::new(), markup_base: None, markup_returns: Vec::new(), classes: None, class_parent: None, this_kinds: Vec::new(), dispatching: Vec::new(), type_kinds: Vec::new() }
  }

  pub(crate) fn resolved(mut self, resolved: &Resolved) -> Self {
    self.globals = resolved.globals.clone();
    self.global_kinds = resolved.global_kinds.clone();
    self.global_formatters.clear();
    self.formatters_from(&resolved.global_formatters);
    self.natives = resolved.natives.clone();
    self.classes = resolved.classes.clone();
    self.type_kinds = resolved.type_kinds.clone();
    self.reaches = resolved.reaches.clone();
    self
  }

  /// True while lowering a component's render path: the browser runs it too.
  pub(crate) fn on_render_path(&self) -> bool {
    (self.render_path || self.hoisting.is_some()) && !self.in_handler
  }

  /// Whether this page may use `reach`. Every page may use `Render` and `Body`.
  pub(crate) fn reaches(&self, reach: Reach) -> bool {
    reach.needs().is_none() || self.reaches.contains(&reach)
  }

  /// Refuses a `body` extension on a web page's render path and any reach the page may not use.
  /// A call to a standard member imported from a side, as the member's extension call. A member the side lacks and a host member the browser would run are refused with where the member is.
  pub(crate) fn std_call(&mut self, call: &js::CallExpression, callee: &js::Expression) -> Lowered<Option<Expr>> {
    use meaning::{Meaning, Side, CLIENT_ROOT, HOST_ROOT};
    let Some(meant) = self.meanings.as_ref().and_then(|meanings| meanings.of_expr(callee)) else { return Ok(None) };
    if let Meaning::StdAbsent { side, module, name } = &meant {
      self.reach_violation = true;
      let (why, hint) = meaning::absent(*side, module, name);
      return Err(match hint {
        Some(hint) => self.residue_with(call.span, why, hint),
        None => self.residue(call.span, why),
      });
    }
    if let Meaning::Library(meaning::Construct::StdModule(_, module)) = &meant {
      return Err(self.residue(callee.span(), format!("`{module}` from the standard library is a module; call one of its members")));
    }
    let Some((side, module, name)) = meant.std_member() else { return Ok(None) };
    let key = format!("{module}.{name}");
    let Some(reach) = standard_reach(module, name) else { return Ok(None) };
    if side == Side::Host && self.on_render_path() && !self.reaches(Reach::Terminal) {
      self.reach_violation = true;
      let why = match Side::Client.holds(module, reach) {
        true => format!("`{key}` from `{HOST_ROOT}` in code the browser also runs; import it from `{CLIENT_ROOT}`"),
        false => format!("`{key}` from `{HOST_ROOT}` in code the browser also runs: it runs only in a host. Read it in the route's loader `page.loader.ts` and pass it to the page as a prop"),
      };
      return Err(self.residue(call.span, why));
    }
    self.check_reach(call.span, &key, reach)?;
    let mut args = Vec::with_capacity(call.arguments.len());
    for a in &call.arguments {
      if crate::syntax::is_spread(a) {
        return Err(self.residue(crate::syntax::arg_expr(a).span(), format!("a spread into `{key}`")));
      }
      args.push(self.expr(crate::syntax::arg_expr(a))?);
    }
    let expr = Expr::Ext { module: module.to_owned(), name: name.to_owned(), args };
    Ok(Some(if reach == Reach::Render { self.candidate(call.span, expr) } else { expr }))
  }

  pub(crate) fn check_reach(&mut self, span: Span, key: &str, reach: Reach) -> Lowered<()> {
    if reach == Reach::Body && self.on_render_path() && !self.reaches(Reach::Terminal) {
      self.reach_violation = true;
      return Err(self.residue(span, format!("`{key}` on a render path")));
    }
    if let (false, Some(needs)) = (self.reaches(reach), reach.needs()) {
      return Err(self.residue(span, format!("`{key}`, which {needs}")));
    }
    Ok(())
  }

  /// The first `body` extension a lowered expression calls, by name.
  pub(crate) fn body_extension_in(&self, expr: &Expr) -> Option<String> {
    let mut found = None;
    expr.visit(&mut |e| {
      if found.is_some() {
        return;
      }
      if let Expr::Ext { module, name, .. } = e {
        let key = format!("{module}.{name}");
        let reach = standard_reach(module, name).or_else(|| self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r));
        if reach == Some(Reach::Body) {
          found = Some(key);
        }
      }
    });
    found
  }

  /// Whether `expr`'s only server-only reads are of what the viewer's browser knows: the clock as a number, this machine's zone offset, its locale and `Temporal.Now`. A call of a helper making only those on a render path is a value the browser fills in.
  pub(crate) fn reads_only_the_date_clock(&self, expr: &Expr) -> bool {
    let is_clock = |e: &Expr| matches!(e, Expr::Ext { module, name, args } if module == "time" && name == "now" && args.is_empty());
    let viewers = |module: &str, name: &str| matches!((module, name), ("time", "localOffset") | ("intl", "localTime" | "systemLocale") | ("temporal", "now"));
    let (mut clocks, mut wrapped, mut viewer, mut other) = (0usize, 0usize, 0usize, false);
    expr.visit(&mut |e| match e {
      Expr::Num(inner) if is_clock(inner) => wrapped += 1,
      e if is_clock(e) => clocks += 1,
      Expr::Ext { module, name, .. } if viewers(module, name) => viewer += 1,
      Expr::Ext { module, name, .. } => {
        let key = format!("{module}.{name}");
        other |= standard_reach(module, name).or_else(|| self.natives.iter().find(|(n, _)| *n == key).map(|(_, r)| *r)) == Some(Reach::Body);
      }
      _ => {}
    });
    !other && clocks == wrapped && clocks + viewer > 0
  }

  /// `expr` as a hoist candidate when a component is being lowered, else itself.
  pub(crate) fn candidate(&mut self, span: Span, expr: Expr) -> Expr {
    match &mut self.hoisting {
      Some(candidates) => {
        let range = self.parsed.range(span);
        candidates.add(range, expr)
      }
      None => expr,
    }
  }

  /// `session.key`, with the schema's default folded in when there is one.
  pub(crate) fn session_read(&self, key: String) -> Expr {
    match self.defaults.iter().find(|(k, _)| *k == key) {
      Some((_, default)) => Expr::Coalesce(Box::new(Expr::Session(key)), Box::new(default.clone())),
      None => Expr::Session(key),
    }
  }

  pub(crate) fn literal(&mut self, expr: &js::Expression) -> Lowered<Expr> {
    self.expr(expr)
  }

  pub(crate) fn residue(&self, span: Span, message: impl Into<String>) -> Residue {
    self.parsed.residue(span, message)
  }

  /// A residue that names the rewrite doing the same thing in the IR.
  pub(crate) fn residue_with(&self, span: Span, message: impl Into<String>, hint: impl Into<String>) -> Residue {
    Residue { hint: Some(hint.into()), ..self.parsed.residue(span, message) }
  }

  /// The body's first parameter: `ctx` or a destructuring of it.
  pub(crate) fn bind_ctx(&mut self, first: Option<Param>) -> Lowered<()> {
    let Some(first) = first else { return Ok(()) };
    if first.rest || first.default.is_some() {
      return Err(self.residue(first.span, "the context parameter must be `ctx` or a destructuring"));
    }
    match first.pattern {
      js::BindingPattern::BindingIdentifier(id) => {
        self.roots.push((id.name.to_string(), Root::Ctx));
        Ok(())
      }
      js::BindingPattern::ObjectPattern(obj) => {
        for prop in crate::syntax::pat_props(obj) {
          match prop {
            crate::syntax::PatProp::Assign(a) => {
              let name = a.key.name.to_string();
              let root = self.root_named(&name).ok_or_else(|| self.residue(a.span, format!("`{name}` is not a field of the context")))?;
              self.roots.push((name, root));
            }
            crate::syntax::PatProp::KeyValue(kv) => {
              let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed context field"))?;
              let js::BindingPattern::BindingIdentifier(local) = &kv.value else {
                return Err(self.residue(kv.value.span(), "a nested destructuring of the context"));
              };
              let root = self.root_named(&key).ok_or_else(|| self.residue(kv.key.span(), format!("`{key}` is not a field of the context")))?;
              self.roots.push((local.name.to_string(), root));
            }
            crate::syntax::PatProp::Rest(r) => return Err(self.residue(r.span, "a rest of the context")),
          }
        }
        Ok(())
      }
      other => Err(self.residue(other.span(), "the context parameter must be `ctx` or a destructuring")),
    }
  }

  /// Whether `init` is the context parameter itself, so a declaration of it binds as the parameter's own destructuring does.
  pub(crate) fn is_ctx(&self, init: Option<&js::Expression>) -> bool {
    matches!(init, Some(js::Expression::Identifier(id)) if matches!(self.root_of(id), Some(Root::Ctx)))
  }

  /// `params`, `query`, `session` or `config`, by name or as a field of the context: a root read only by key.
  pub(crate) fn keyed_root(&self, init: &js::Expression) -> Option<Root> {
    let keyed = |root: Root| matches!(root, Root::Params | Root::Query | Root::Session | Root::Config).then_some(root);
    match init {
      js::Expression::ParenthesizedExpression(p) => self.keyed_root(&p.expression),
      js::Expression::Identifier(id) if !self.scope.iter().any(|(n, _)| n == id.name.as_str()) => self.root_of(id).and_then(keyed),
      m_expr if m_expr.is_member_expression() => {
let m = m_expr.as_member_expression().expect("matched as a member");
        let (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(field)) = (m.object(), crate::syntax::prop(m)) else { return None };
        if self.scope.iter().any(|(n, _)| n == obj.name.as_str()) || !matches!(self.root_of(obj), Some(Root::Ctx)) {
          return None;
        }
        root_named(field.name.as_str()).and_then(keyed)
      }
      _ => None,
    }
  }

  /// `const { id, slug: s = "a" } = params`: each name bound to its key's read.
  pub(crate) fn bind_keyed(&mut self, fields: &js::ObjectPattern, root: Root) -> Lowered<()> {
    let read = |lowerer: &Self, key: String| match root {
      Root::Params => Expr::Param(key),
      Root::Query => Expr::Query(key),
      Root::Session => lowerer.session_read(key),
      _ => Expr::Config(key),
    };
    for field in crate::syntax::pat_props(fields) {
      match field {
        crate::syntax::PatProp::Assign(a) => {
          let name = a.key.name.to_string();
          let bound = match a.value {
            Some(default) => Expr::Coalesce(Box::new(read(self, name.clone())), Box::new(self.expr(default)?)),
            None => read(self, name.clone()),
          };
          self.scope.push((name, bound));
        }
        crate::syntax::PatProp::KeyValue(kv) => {
          let key = prop_name(&kv.key, kv.computed).ok_or_else(|| self.residue(kv.key.span(), "a computed key in a pattern"))?;
          let value = read(self, key);
          crate::component::bind_pattern(self, &kv.value, value)?;
        }
        crate::syntax::PatProp::Rest(r) => return Err(self.residue(r.span, "a rest of a value read only by key")),
      }
    }
    Ok(())
  }

  pub(crate) fn root_named(&self, name: &str) -> Option<Root> {
    if self.middleware && name == "request" {
      return Some(Root::Input);
    }
    if self.middleware && name == "next" {
      return Some(Root::Next);
    }
    if self.theming && name == "theme" {
      return Some(Root::Input);
    }
    if self.meta && name == "data" {
      return Some(Root::Input);
    }
    root_named(name)
  }

  pub(crate) fn block(&mut self, stmts: &[js::Statement]) -> Lowered<Body> {
    let depth = self.scope.len();
    let _ = self.shared.len();
    let shared = crate::component::closure_shared(&self.parsed.scope, stmts);
    self.shared.extend(shared);
    let mut out = Vec::with_capacity(stmts.len());
    if let Err(residue) = self.hoist_functions(stmts, &mut out) {
      self.scope.truncate(depth);
      return Err(residue);
    }
    for stmt in stmts {
      self.lift_updates(stmt, &mut out)?;
      match stmt {
        js::Statement::FunctionDeclaration(_) => {}
        js::Statement::ForStatement(counted) => out.extend(self.counted_for(counted)?),
        js::Statement::WhileStatement(looped) if self.reaches(Reach::Terminal) => out.push(self.capped_while(looped)?),
        js::Statement::VariableDeclaration(var) => {
          for decl in &var.declarations {
            if self.is_ctx(decl.init.as_ref()) {
              self.bind_ctx(Some(Param::of_pattern(&decl.id)))?;
              continue;
            }
            if let (js::BindingPattern::ObjectPattern(fields), Some(root)) = (&decl.id, decl.init.as_ref().and_then(|init| self.keyed_root(init))) {
              self.bind_keyed(fields, root)?;
              continue;
            }
            out.push(self.declare(decl)?);
            out.append(&mut self.extra_lets);
            if var.kind == js::VariableDeclarationKind::Const {
              self.note_methods(decl);
            }
          }
        }
        js::Statement::SwitchStatement(sw) => out.extend(self.switch(sw)?),
        stmt => out.push(self.stmt(stmt)?),
      }
    }
    self.scope.truncate(depth);
    Ok(out)
  }

  /// The functions `stmts` declare, each bound before the first statement as JavaScript hoists them: a `Fix` when it calls itself.
  pub(crate) fn hoist_functions(&mut self, stmts: &[js::Statement], out: &mut Vec<Stmt>) -> Lowered<()> {
    let declared: Vec<&js::Function> = stmts.iter().filter_map(|stmt| match stmt {
      js::Statement::FunctionDeclaration(f) => Some(&**f),
      _ => None,
    }).collect();
    let name_of = |f: &js::Function| f.id.as_ref().map_or(String::new(), |id| id.name.to_string());
    for f in &declared {
      let name = name_of(f);
      self.scope.push((name.clone(), Expr::Var(name)));
    }
    for (at, f) in declared.iter().enumerate() {
      if f.generator {
        return Err(self.residue(f.span, "a generator function"));
      }
      let name = name_of(f);
      let pats = crate::params_of(&f.params);
      let body = f.body.as_ref().map(|body| body.statements.as_slice()).unwrap_or(&[]);
      let outer = std::mem::take(&mut self.breakable);
      let lambda = crate::component::function_to_lambda(self, &pats, crate::component::FunctionBody::Block(body), f.r#async);
      self.breakable = outer;
      let lambda = lambda?;
      let mut free = Vec::new();
      lambda.free_vars(&mut free);
      if let Some(later) = declared[at + 1..].iter().find(|later| free.iter().any(|name| *name == name_of(later))) {
        return Err(self.residue_with(f.span, format!("`{name}` calls `{}`, a function declared after it in the same body", name_of(later)), "declare the function it calls first"));
      }
      let expr = if free.contains(&name) { Expr::Fix { name: name.clone(), f: Box::new(lambda) } } else { lambda };
      out.push(Stmt::Let { name, expr });
    }
    Ok(())
  }

  /// `for (let i = a; i < b; i += s)` as its start under a temporary and a `for...of` over the count, the counter read as the start plus the item times the step. A bound that may change between passes, one that calls something, is tested every pass as JavaScript tests it.
  pub(crate) fn counted_for(&mut self, f: &js::ForStatement) -> Lowered<Vec<Stmt>> {
    let counted = crate::component::Counted::of(self, f)?;
    let (from, to, inclusive) = counted.bounds();
    let start = self.temp();
    let item = self.temp();
    let bound = to;
    let from = self.expr(from)?;
    let to = self.expr(bound)?;
    if !crate::component::plain(&to) {
      return self.tested_for(f, &counted, from, bound, inclusive);
    }
    let over = Expr::Builtin { name: Builtin::Range, args: vec![counted.count(self, Expr::Var(start.clone()), to)?] };
    let counter = counted.counter(self, Expr::Var(start.clone()), Expr::Var(item.clone()))?;
    let depth = self.scope.len();
    self.scope.push((counted.name.clone(), counter));
    self.breakable.push(Breakable::Loop);
    let body = self.branch(&f.body);
    self.breakable.pop();
    self.scope.truncate(depth);
    Ok(vec![Stmt::Let { name: start, expr: from }, Stmt::ForOf { name: item, over, body: body? }])
  }

  /// A counted loop whose bound is read again every pass: the counter a local, the test and the step after every pass as written.
  pub(crate) fn tested_for(&mut self, f: &js::ForStatement, counted: &crate::component::Counted, from: Expr, bound: &js::Expression, inclusive: bool) -> Lowered<Vec<Stmt>> {
    let counter = counted.name.as_str();
    let step = counted.step_expr(self)?;
    let depth = self.scope.len();
    self.scope.push((counter.to_owned(), Expr::Var(counter.to_owned())));
    let to = self.expr(bound);
    let body = to.and_then(|to| {
      let op = if inclusive { CompareOp::Le } else { CompareOp::Lt };
      let cond = Expr::Compare(op, Box::new(Expr::Var(counter.to_owned())), Box::new(to));
      self.breakable.push(Breakable::Loop);
      let body = self.branch(&f.body);
      self.breakable.pop();
      let step = Stmt::Set { name: counter.to_owned(), expr: Expr::Arith(ArithOp::Add, Box::new(Expr::Var(counter.to_owned())), Box::new(step)) };
      Ok(Stmt::While { cond, body: body?, step: vec![step] })
    });
    self.scope.truncate(depth);
    Ok(vec![Stmt::Let { name: counter.to_owned(), expr: from }, body?])
  }

  /// A terminal's `while (test) body`, the test read before every pass.
  pub(crate) fn capped_while(&mut self, looped: &js::WhileStatement) -> Lowered<Stmt> {
    let test = self.expr(&looped.test)?;
    self.breakable.push(Breakable::Loop);
    let body = self.branch(&looped.body);
    self.breakable.pop();
    Ok(Stmt::While { cond: test, body: body?, step: Vec::new() })
  }

  /// `switch` as its discriminant under a temporary and an `if` chain over the arms, each compared with `===`.
  pub(crate) fn switch(&mut self, sw: &js::SwitchStatement) -> Lowered<Vec<Stmt>> {
    self.breakable.push(Breakable::Switch);
    let lowered = self.switch_arms(sw);
    self.breakable.pop();
    lowered
  }

  pub(crate) fn switch_arms(&mut self, sw: &js::SwitchStatement) -> Lowered<Vec<Stmt>> {
    let disc = self.expr(&sw.discriminant)?;
    let held = self.temp();
    let (arms, default) = crate::component::switch_arms(self, sw)?;
    let mut otherwise = match default {
      Some(body) => self.block(body)?,
      None => Vec::new(),
    };
    for (tests, body) in arms.into_iter().rev() {
      let mut cond: Option<Expr> = None;
      for test in tests {
        let eq = Expr::Compare(CompareOp::Eq, Box::new(Expr::Var(held.clone())), Box::new(self.expr(test)?));
        cond = Some(match cond {
          None => eq,
          Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
        });
      }
      let then = self.block(body)?;
      otherwise = vec![Stmt::If { cond: cond.unwrap_or(Expr::Lit(Lit::Bool(false))), then, r#else: otherwise }];
    }
    let mut out = vec![Stmt::Let { name: held, expr: disc }];
    out.extend(otherwise);
    Ok(out)
  }

  /// A fresh name for a value a destructuring reads its fields from.
  pub(crate) fn temp(&mut self) -> String {
    self.temps += 1;
    format!("$d{}", self.temps)
  }

  /// The kind `e` holds: a `new Map(...)`, `new Set(...)`, `new Date(...)` or bigint written in place, a name noted as bound to one or a prop whose kind is known.
  pub(crate) fn kind_of(&self, e: &js::Expression) -> Option<Kind> {
    match e {
      js::Expression::ParenthesizedExpression(p) => self.kind_of(&p.expression),
      js::Expression::AwaitExpression(a) => self.kind_of(&a.argument),
      js::Expression::CallExpression(call) if self.calls_next(call) => Some(Kind::Response),
      js::Expression::TSAsExpression(a) => self.kind_of(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.kind_of(&a.expression),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        match self.scope.iter().rev().find(|(n, _)| n == name) {
          Some((_, bound)) => {
            let kind = self.kinds.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, kind)| *kind);
            if let (Some(_), Some(key)) = (kind, prop_key(bound)) {
              self.consulted.borrow_mut().push(key);
            }
            kind
          }
          None => self.global_kinds.iter().find(|(n, _)| n == name).map(|(_, kind)| *kind),
        }
      }
      js::Expression::CallExpression(call) => match &call.callee {
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Browser)) => Some(Kind::Browser),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Set) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if matches!(p.name.as_str(), "union" | "intersection" | "difference" | "symmetricDifference"))) => Some(Kind::Set),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Set) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "add")) => Some(Kind::Set),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Map) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "set")) => Some(Kind::Map),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Date) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "toTemporalInstant")) => Some(Kind::Temporal),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Locale) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if matches!(p.name.as_str(), "maximize" | "minimize"))) => Some(Kind::Locale),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Stack) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "move")) => Some(Kind::Stack),
        callee if callee.as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if id.name.as_str() == "Map" && !self.scope.iter().any(|(n, _)| n == "Map")) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "groupBy")) => Some(Kind::Map),
        callee if callee.as_member_expression().is_some_and(|m| self.kind_of(m.object()) == Some(Kind::Temporal) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if crate::TEMPORAL_MAKERS.contains(&p.name.as_str()))) => Some(Kind::Temporal),
        callee if callee.as_member_expression().is_some_and(|m| self.makes_iterator(m)) => Some(Kind::Iterator),
        js::Expression::StaticMemberExpression(m) if matches!(&m.object, js::Expression::Identifier(r) if r.name.as_str() == "Reflect" && !self.scope.iter().any(|(n, _)| n == "Reflect")) && m.property.name.as_str() == "construct" => {
          let constructor = call.arguments.first().map(crate::syntax::arg_expr)?;
          let b = self.build();
          let made = js::Expression::new_new_expression(call.span, b.copy(constructor), None, b.vec([]), &b.ast());
          made_kind(&made)
        }
        js::Expression::Identifier(f) if !self.scope.iter().any(|(n, _)| n == f.name.as_str()) => {
          let returned = returned_by(f.name.as_str());
          self.global_kinds.iter().find(|(n, _)| *n == returned).map(|(_, kind)| *kind).or_else(|| made_kind(e))
        }
        _ => made_kind(e),
      },
      member if member.is_member_expression() => self.member_kind(member.as_member_expression().expect("matched as a member")),
      other => made_kind(other),
    }
  }

  /// Whether calling `m` answers an iterator: `values()`, `keys()` or `entries()` of an array written in place or of a value whose kind holds items, `Iterator.from(...)` and an iterator helper of an iterator.
  fn makes_iterator(&self, m: &js::MemberExpression) -> bool {
    let crate::syntax::MemberProp::Ident(p) = crate::syntax::prop(m) else { return false };
    let object = m.object();
    match p.name.as_str() {
      "values" | "keys" | "entries" => matches!(strip_parens(object), js::Expression::ArrayExpression(_)) || matches!(self.kind_of(object), Some(Kind::Map | Kind::Set | Kind::Typed | Kind::Iterator)),
      "from" | "concat" | "zip" => matches!(object, js::Expression::Identifier(id) if id.name.as_str() == "Iterator" && !self.scope.iter().any(|(n, _)| n == "Iterator")),
      "map" | "filter" | "flatMap" | "take" | "drop" => self.kind_of(object) == Some(Kind::Iterator),
      _ => false,
    }
  }

  /// The kind a member read holds: a browser object's member, a URL's `searchParams`, a class field or a prop whose kind is known.
  pub(crate) fn member_kind(&self, m: &js::MemberExpression) -> Option<Kind> {
    if let (js::Expression::Identifier(obj), crate::syntax::MemberProp::Ident(p)) = (m.object(), crate::syntax::prop(m)) {
      if obj.name.as_str() == "Symbol" && !self.scope.iter().any(|(n, _)| n == "Symbol") && crate::WELL_KNOWN_SYMBOLS.contains(&p.name.as_str()) {
        return Some(Kind::Symbol);
      }
    }
    if self.kind_of(m.object()) == Some(Kind::Browser) {
      return Some(Kind::Browser);
    }
    if self.kind_of(m.object()) == Some(Kind::Url) && matches!(crate::syntax::prop(m), crate::syntax::MemberProp::Ident(p) if p.name.as_str() == "searchParams") {
      return Some(Kind::Query);
    }
    if let (js::Expression::Identifier(this), crate::syntax::MemberProp::Ident(field)) = (m.object(), crate::syntax::prop(m)) {
      if this.name.as_str() == crate::classes::THIS {
        return self.this_kinds.iter().find(|(n, _)| n == field.name.as_str()).map(|(_, kind)| *kind);
      }
    }
    let js::Expression::Identifier(obj) = m.object() else { return None };
    let crate::syntax::MemberProp::Ident(prop) = crate::syntax::prop(m) else { return None };
    let (_, bound) = self.scope.iter().rev().find(|(n, _)| n == obj.name.as_str())?;
    if *bound != Expr::Var("$props".to_owned()) {
      return None;
    }
    let kind = self.prop_kinds.iter().find(|(k, _)| k == prop.name.as_str()).map(|(_, kind)| *kind)?;
    self.consulted.borrow_mut().push(prop.name.to_string());
    Some(kind)
  }

  /// `x instanceof Map`, `Set`, `Date` or `Array`: a constant where the build knows what `x` holds, `Array.isArray` for `Array`.
  pub(crate) fn instance_of(&mut self, bin: &js::BinaryExpression) -> Lowered<Expr> {
    self.instance_test(&bin.left, &bin.right, bin.span)
  }

  /// `C.prototype.isPrototypeOf(x)`: `x instanceof C`, with `Object` any object.
  pub(crate) fn prototype_of(&mut self, class: &js::Expression, value: &js::Expression, span: Span) -> Lowered<Expr> {
    if matches!(class, js::Expression::Identifier(id) if id.name.as_str() == "Object" && !self.scope.iter().any(|(n, _)| n == "Object")) {
      let held = self.temp();
      let kind = Expr::Builtin { name: Builtin::TypeOf, args: vec![Expr::Var(held.clone())] };
      let object = Expr::Logic(LogicOp::And, Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind.clone()), Box::new(Expr::lit_str("object")))), Box::new(Expr::Not(Box::new(Expr::Builtin { name: Builtin::IsNullish, args: vec![Expr::Var(held.clone())] }))));
      let answer = Expr::Logic(LogicOp::Or, Box::new(object), Box::new(Expr::Compare(CompareOp::Eq, Box::new(kind), Box::new(Expr::lit_str("function")))));
      return Ok(Expr::Block(vec![Stmt::Let { name: held, expr: self.expr(value)? }, Stmt::Return(answer)]));
    }
    self.instance_test(value, class, span)
  }

  fn instance_test(&mut self, left: &js::Expression, right: &js::Expression, span: Span) -> Lowered<Expr> {
    let bin_span = span;
    if let Some(test) = self.class_instance_of(left, right)? {
      return Ok(test);
    }
    let js::Expression::Identifier(class) = right else { return Err(self.residue(bin_span, "`instanceof` of a computed class")) };
    let class = class.name.as_str();
    if self.scope.iter().any(|(n, _)| n == class) || self.globals.iter().any(|(n, _)| n == class) {
      return Err(self.residue(bin_span, format!("`instanceof {class}`, a class this file declares")));
    }
    if class == "Array" {
      return Ok(Expr::Builtin { name: Builtin::IsArray, args: vec![self.expr(left)?] });
    }
    if ERROR_CONSTRUCTORS.contains(&class) {
      return Ok(Expr::Builtin { name: Builtin::IsError, args: vec![self.expr(left)?, Expr::lit_str(class)] });
    }
    if !matches!(class, "Map" | "Set" | "Date" | "WeakMap" | "WeakSet" | "WeakRef" | "FinalizationRegistry") {
      return Err(self.residue(bin_span, format!("`instanceof {class}`")));
    }
    match self.kind_of(left) {
      Some(Kind::Browser) => Err(self.browser_only(left.span(), "`instanceof` of a value only the browser knows")),
      Some(kind) => Ok(Expr::Lit(Lit::Bool(kind.name() == class))),
      None => Err(self.residue_with(left.span(), format!("`instanceof {class}` on a value whose type the build does not know"), "make the value in place with `new` or type the prop `Map<K, V>`, `Set<T>` or `Date` where the component declares its props")),
    }
  }

  /// The `Intl` formatter `e` holds: a `new Intl.NumberFormat(...)` written in place, or a name bound to one, local or module-level.
  pub(crate) fn formatter_of(&self, e: &js::Expression) -> Option<&'a js::NewExpression<'a>> {
    match strip_parens(e) {
      js::Expression::TSAsExpression(a) => self.formatter_of(&a.expression),
      js::Expression::TSNonNullExpression(a) => self.formatter_of(&a.expression),
      js::Expression::NewExpression(n) if platform_object(n).is_some() => Some(self.keep(&**n)),
      js::Expression::Identifier(id) => {
        let name = id.name.as_str();
        match self.scope.iter().rev().find(|(n, _)| n == name) {
          Some((_, bound)) => self.formatters.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, made)| *made),
          None => self.global_formatters.iter().find(|(n, _)| n == name).map(|(_, made)| *made),
        }
      }
      _ => None,
    }
  }

  /// Builds nodes in the arena of the module being lowered.
  pub(crate) fn build(&self) -> syntax::Build<'a> {
    syntax::Build::new(self.parsed.arena())
  }

  /// A copy of `node` in the module's arena, which lives as long as the lowerer's view of the module.
  pub(crate) fn formatters_from(&mut self, formatters: &[(String, String)]) {
    for (name, text) in formatters {
      let Ok(js::Expression::NewExpression(made)) = self.parsed.parse_expr_at(text, 1, 1) else { continue };
      if platform_object(&made).is_some() {
        let made: &'a js::NewExpression<'a> = self.parsed.arena().alloc(made);
        self.global_formatters.push((name.clone(), made));
      }
    }
  }

  pub(crate) fn keep<T: oxc_allocator::CloneIn<'a>>(&self, node: &T) -> &'a T::Cloned {
    let b = self.build();
    b.alloc(b.copy(node))
  }

  /// Binds `name` to an `Intl` formatter `init` makes, so a later `.format(x)` on it lowers as the inline form; the name reads as null anywhere else.
  pub(crate) fn bind_formatter(&mut self, name: &str, init: &js::Expression) -> bool {
    let js::Expression::NewExpression(made) = strip_parens(init) else { return false };
    if platform_object(made).is_none() {
      return false;
    }
    self.scope.push((name.to_owned(), Expr::Lit(Lit::Null)));
    let made = self.keep(&**made);
    self.formatters.push((name.to_owned(), Expr::Lit(Lit::Null), made));
    true
  }

  /// Notes the fields of the object literal `decl` binds that are written as functions.
  pub(crate) fn note_methods(&mut self, decl: &js::VariableDeclarator) {
    let (js::BindingPattern::BindingIdentifier(name), Some(js::Expression::ObjectExpression(object))) = (&decl.id, decl.init.as_ref().map(crate::meaning::bare)) else { return };
    let fields: Vec<String> = object.properties.iter().filter_map(|prop| match prop {
      js::ObjectPropertyKind::ObjectProperty(prop) => match crate::syntax::prop_of(prop) {
        crate::syntax::Prop::KeyValue(kv) if matches!(crate::meaning::bare(&kv.value), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_)) => prop_name(&kv.key, kv.computed),
        crate::syntax::Prop::Method(method) => prop_name(&method.key, method.computed),
        _ => None,
      },
      js::ObjectPropertyKind::SpreadProperty(_) => None,
    }).collect();
    let name = name.name.to_string();
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| *n == name).map(|(_, e)| e.clone()) else { return };
    if !fields.is_empty() {
      self.methods.push((name, bound, fields));
    }
  }

  /// Whether `obj` is a name noted by [`Self::note_methods`] as holding an object whose field `field` is a function.
  pub(crate) fn holds_method(&self, obj: &js::Expression, field: &str) -> bool {
    let js::Expression::Identifier(id) = crate::meaning::bare(obj) else { return false };
    let name = id.name.as_str();
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e) else { return false };
    self.methods.iter().rev().any(|(n, at, fields)| n == name && at == bound && fields.iter().any(|f| f == field))
  }

  /// Notes `name`, just bound to `init`, as a collection when `init` is one.
  pub(crate) fn note_kind(&mut self, name: &str, init: &js::Expression) {
    let Some(kind) = self.kind_of(init) else { return };
    let Some(bound) = self.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()) else { return };
    self.kinds.push((name.to_owned(), bound, kind));
  }

  /// One declarator as a `let`: a name binds the value, `undefined` when it has none; a pattern binds it under a temporary and each name it declares reads a part of that.
  pub(crate) fn declare(&mut self, decl: &js::VariableDeclarator) -> Lowered<Stmt> {
    let init = decl.init.as_ref();
    if let (js::BindingPattern::BindingIdentifier(name), Some(init)) = (&decl.id, init) {
      if self.bind_formatter(name.name.as_str(), init) {
        return Ok(Stmt::Let { name: name.name.to_string(), expr: Expr::Lit(Lit::Null) });
      }
    }
    let expr = match (init, &decl.id) {
      (Some(init), _) => self.expr(init)?,
      (None, js::BindingPattern::BindingIdentifier(_)) => Expr::Lit(Lit::Undefined),
      (None, _) => return Err(self.residue(decl.span, "a pattern declared without a value")),
    };
    let name = match &decl.id {
      js::BindingPattern::BindingIdentifier(name) if self.shared.iter().any(|(n, c)| *n == name.name.as_str() && *c == self.parsed.binding_ctxt(name)) => {
        let handle = self.temp();
        let local = name.name.to_string();
        self.cells.push((local.clone(), Expr::Var(handle.clone())));
        self.scope.push((local.clone(), Expr::ReadCell(Box::new(Expr::Var(handle.clone())))));
        if let Some(init) = init {
          self.note_kind(&local, init);
        }
        return Ok(Stmt::Let { name: handle, expr: Expr::Cell(Box::new(expr)) });
      }
      js::BindingPattern::BindingIdentifier(name) => {
        let name = name.name.to_string();
        self.scope.push((name.clone(), Expr::Var(name.clone())));
        if let Some(init) = init {
          self.note_kind(&name, init);
        }
        name
      }
      pattern => {
        let name = self.temp();
        let from = self.scope.len();
        crate::component::bind_pattern(self, pattern, Expr::Var(name.clone()))?;
        {
          for at in from..self.scope.len() {
            let (bound, value) = self.scope[at].clone();
            if bound.starts_with('$') || crate::component::plain(&value) {
              continue;
            }
            let held = self.temp();
            self.extra_lets.push(Stmt::Let { name: held.clone(), expr: value });
            self.scope[at].1 = Expr::Var(held);
          }
        }
        name
      }
    };
    Ok(Stmt::Let { name, expr })
  }
}
