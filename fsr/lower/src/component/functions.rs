use super::*;

/// A module-level function as a lambda: parameters bind as in `Lowerer::lambda`;
/// `const`s inline; `if (c) return a;` chains become ternaries.
/// A module function as a lambda. An `async` one in a terminal page runs its statements as an async function and returns the promise they settle.
pub(crate) fn function_to_lambda(lowerer: &mut Lowerer<'_>, params: &[crate::Param], body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let depth = lowerer.scope.len();
  let names = bind_params(lowerer, params)?;
  let roots = lowerer.bind_services_params(params, depth);
  let lowered = function_to_lambda_bound(lowerer, params, names, depth, body, is_async);
  lowerer.roots.truncate(roots);
  lowered
}

/// [`function_to_lambda`] once its parameters `names` are bound above `depth`, `Ctx["services"]` ones as the services root.
pub(super) fn function_to_lambda_bound(lowerer: &mut Lowerer<'_>, params: &[crate::Param], names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let written = match body {
    FunctionBody::Block(stmts) => written_params(params, stmts),
    FunctionBody::Expr(_) => Vec::new(),
  };
  if let (FunctionBody::Block(stmts), false) = (body, written.is_empty()) {
    if is_async {
      lowerer.scope.truncate(depth);
      return Err(lowerer.residue(stmts[0].span(), format!("an async function that writes a field of its parameter `{}`; write the field before the function is called or return the new value", written[0].1)));
    }
    let packed = match packing_returns(lowerer, stmts, &written) {
      Ok(packed) => packed,
      Err(residue) => {
        lowerer.scope.truncate(depth);
        return Err(residue);
      }
    };
    let lambda = lambda_of(lowerer, names, depth, FunctionBody::Block(&packed), false)?;
    let Expr::Lambda { params, body } = lambda else { unreachable!("a function lowers to a lambda") };
    return Ok(Expr::Lambda { params, body: Box::new(write_back(*body, &written)) });
  }
  lambda_of(lowerer, names, depth, body, is_async)
}

/// [`function_to_lambda`] once its parameters `names` are bound above `depth`.
pub(super) fn lambda_of(lowerer: &mut Lowerer<'_>, names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let outer = std::mem::take(&mut lowerer.breakable);
  let lambda = lambda_in(lowerer, names, depth, body, is_async);
  lowerer.breakable = outer;
  lambda
}

pub(super) fn lambda_in(lowerer: &mut Lowerer<'_>, names: Vec<String>, depth: usize, body: FunctionBody<'_>, is_async: bool) -> Lowered<Expr> {
  let defaults = if !(is_async && lowerer.reaches(Reach::Later)) {
    held_frame(lowerer, |lowerer| settle(lowerer, depth)).0
  } else {
    Vec::new()
  };
  if is_async && lowerer.reaches(Reach::Later) {
    let done = lowerer.temp();
    let b = lowerer.build();
    let ran = match body {
      FunctionBody::Block(stmts) => lowerer.async_to(stmts, Some(Expr::Var(done.clone()))),
      FunctionBody::Expr(e) => lowerer.async_to(&[b.return_stmt(Some(b.copy(e)), e.span())], Some(Expr::Var(done.clone()))),
    };
    lowerer.scope.truncate(depth);
    let body = Expr::Block(vec![Stmt::Let { name: done.clone(), expr: lowerer.new_deferred() }, Stmt::Expr(ran?), Stmt::Return(Expr::Var(done))]);
    return Ok(Expr::Lambda { params: names, body: Box::new(body) });
  }
  let result = match body {
    FunctionBody::Expr(e) => lowerer.expr(e),
    FunctionBody::Block(stmts) => lowerer.block(stmts).map(Expr::Block),
  };
  lowerer.scope.truncate(depth);
  Ok(Expr::Lambda { params: names, body: Box::new(sequenced(defaults, result?)) })
}

/// The locals declared outside a function that an arrow or a function inside `stmts` writes, by name and syntax context: a bare name, a path into one or an update.
/// The locals `stmts` write from inside a function or a `try`, whose writes outlive the path that makes them.
pub(super) fn function_writes(scope: &crate::scope::Scope, stmts: &[&js::Statement]) -> Vec<(String, crate::Ctxt)> {
  shared_writes(scope, stmts, false)
}

pub(crate) fn closure_shared(scope: &crate::scope::Scope, stmts: &[js::Statement]) -> Vec<(String, crate::Ctxt)> {
  let refs: Vec<&js::Statement> = stmts.iter().collect();
  shared_writes(scope, &refs, true)
}

pub(super) fn shared_writes(scope: &crate::scope::Scope, stmts: &[&js::Statement], closures_only: bool) -> Vec<(String, crate::Ctxt)> {
  struct Writes<'s> {
    scope: &'s crate::scope::Scope,
    depth: usize,
    found: Vec<(String, crate::Ctxt)>,
    closures: usize,
    read: Vec<(String, crate::Ctxt)>,
    all: Vec<(String, crate::Ctxt)>,
  }
  impl Writes<'_> {
    fn note(&mut self, id: &js::IdentifierReference) {
      let ctxt = self.scope.of(id);
      self.all.push((id.name.to_string(), ctxt));
      if self.depth > 0 && !self.found.iter().any(|(name, c)| *name == id.name.as_str() && *c == ctxt) {
        self.found.push((id.name.to_string(), ctxt));
      }
    }
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Writes<'_> {
    fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
      match &assign.left {
        js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.note(id),
        target if target.is_member_expression() && !crate::heap_pass() => {
          if let Some((id, _)) = target.as_member_expression().and_then(crate::write_root) {
            self.note(id);
          }
        }
        _ => {}
      }
      oxc_ast_visit::walk::walk_assignment_expression(self, assign);
    }
    fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
      if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
        self.note(id);
      }
      oxc_ast_visit::walk::walk_update_expression(self, update);
    }
    fn visit_arrow_function_expression(&mut self, arrow: &js::ArrowFunctionExpression<'a>) {
      self.depth += 1;
      self.closures += 1;
      oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
      self.closures -= 1;
      self.depth -= 1;
    }
    fn visit_function(&mut self, function: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
      self.depth += 1;
      self.closures += 1;
      oxc_ast_visit::walk::walk_function(self, function, flags);
      self.closures -= 1;
      self.depth -= 1;
    }
    fn visit_try_statement(&mut self, t: &js::TryStatement<'a>) {
      self.depth += 1;
      oxc_ast_visit::walk::walk_try_statement(self, t);
      self.depth -= 1;
    }
    fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
      if self.closures > 0 {
        self.read.push((id.name.to_string(), self.scope.of(id)));
      }
    }
  }
  let mut writes = Writes { scope, depth: 0, found: Vec::new(), closures: 0, read: Vec::new(), all: Vec::new() };
  stmts.iter().for_each(|stmt| stmt.walk(&mut writes));
  struct Bound<'s>(&'s crate::scope::Scope, Vec<(String, crate::Ctxt)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Bound<'_> {
    fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
      self.1.push((id.name.to_string(), self.0.of_binding(id)));
    }
  }
  let mut bound = Bound(scope, Vec::new());
  stmts.iter().for_each(|stmt| stmt.walk(&mut bound));
  let shared: Vec<_> = writes.read.iter().filter(|name| writes.all.contains(name) && bound.1.contains(name)).cloned().collect();
  if closures_only {
    return shared;
  }
  writes.found.extend(shared);
  writes.found
}

/// The bare names `stmts` call as a statement of their own, `f()`, `void f()` or `await f()`.
pub(super) fn called_names(stmts: &[js::Statement]) -> Vec<String> {
  stmts
    .iter()
    .filter_map(|stmt| {
      let js::Statement::ExpressionStatement(e) = stmt else { return None };
      let mut called = crate::meaning::bare(&e.expression);
      loop {
        match called {
          js::Expression::UnaryExpression(u) if u.operator == js::UnaryOperator::Void => called = crate::meaning::bare(&u.argument),
          js::Expression::AwaitExpression(a) => called = crate::meaning::bare(&a.argument),
          _ => break,
        }
      }
      let js::Expression::CallExpression(call) = called else { return None };
      let callee = &call.callee;
      let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return None };
      Some(id.name.to_string())
    })
    .collect()
}

/// The bare names `stmts` pass as the function of a `.on(event, f)` or `.off(event, f)` call, nested arrows included.
pub(super) fn listened_names(stmts: &[js::Statement]) -> Vec<String> {
  struct Listened(Vec<String>);
  impl<'a> oxc_ast_visit::Visit<'a> for Listened {
    fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
      if let Some(f) = call.arguments.get(1) {
        if let (Some(member), js::Expression::Identifier(name)) = (crate::meaning::bare(&call.callee).as_member_expression(), crate::meaning::bare(crate::syntax::arg_expr(f))) {
          if matches!(crate::syntax::prop(member), crate::syntax::MemberProp::Ident(method) if method.name == "on" || method.name == "off") {
            self.0.push(name.name.to_string());
          }
        }
      }
      oxc_ast_visit::walk::walk_call_expression(self, call);
    }
  }
  let mut listened = Listened(Vec::new());
  stmts.iter().for_each(|stmt| stmt.walk(&mut listened));
  listened.0
}

/// The bare names `stmts` write through `=`, a compound assignment or `++`, in source order; nested functions and arrows are walked when `through_functions` says so.
pub(crate) fn written_names(stmts: &[js::Statement], through_functions: bool) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions, through_calls: false };
  for stmt in stmts {
    stmt.walk(&mut writes);
  }
  writes.names
}

/// The bare names a module writes anywhere, its functions included.
/// The names `module` calls `set`, `delete`, `add` or `clear` on, which change a collection in place.
pub(super) fn module_mutations(module: &js::Program) -> Vec<String> {
  let mut found = Mutations(Vec::new());
  module.walk(&mut found);
  found.0
}

/// The parameters, by position, whose fields `stmts` write: what a call writes back to the arguments it passed, as JavaScript's caller sees the objects it passed changed.
pub(crate) fn written_params(params: &[crate::Param], stmts: &[js::Statement]) -> Vec<(usize, String)> {
  if crate::heap_pass() {
    return Vec::new();
  }
  params
    .iter()
    .enumerate()
    .filter_map(|(i, param)| match param.pattern {
      js::BindingPattern::BindingIdentifier(id) if param.default.is_none() && !param.rest && crate::writes_a_member_of(stmts, std::slice::from_ref(&id.name.to_string())) => Some((i, id.name.to_string())),
      _ => None,
    })
    .collect()
}

/// `stmts` with each `return e` outside a function of their own made `return [e, p, ...]` and a last `return [undefined, p, ...]`, `p` each written parameter, so the value answers with each parameter's last value where it returns. A written parameter the body also assigns whole is refused: the caller would keep the object it passed and the field writes after the assignment would reach nothing it holds.
pub(crate) fn packing_returns<'a>(lowerer: &Lowerer<'a>, stmts: &[js::Statement], written: &[(usize, String)]) -> Lowered<Vec<js::Statement<'a>>> {
  use crate::syntax::{Walk, WalkMut};
  struct Replaced<'w>(&'w [(usize, String)], Option<(Span, String)>);
  impl<'a> oxc_ast_visit::Visit<'a> for Replaced<'_> {
    fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
      if let js::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left {
        if self.1.is_none() && self.0.iter().any(|(_, p)| *p == id.name.as_str()) {
          self.1 = Some((assign.span, id.name.to_string()));
        }
      }
      oxc_ast_visit::walk::walk_assignment_expression(self, assign);
    }
  }
  let mut replaced = Replaced(written, None);
  stmts.iter().for_each(|stmt| stmt.walk(&mut replaced));
  if let Some((span, name)) = replaced.1 {
    return Err(lowerer.residue_with(span, format!("a write to `{name}`, a parameter whose fields the function also writes"), "write the fields of the object passed, or return a new object and have the caller use it"));
  }
  let b = lowerer.build();
  let pack = |value: Option<js::Expression<'a>>, span: Span| -> js::Expression<'a> {
    let mut elems = vec![js::ArrayExpressionElement::from(value.unwrap_or_else(|| b.ident("undefined", span)))];
    elems.extend(written.iter().map(|(_, p)| js::ArrayExpressionElement::from(b.ident(p, span))));
    js::Expression::new_array_expression(span, b.vec(elems), &b.ast())
  };
  struct Returns<'p, 'a>(&'p dyn Fn(Option<js::Expression<'a>>, Span) -> js::Expression<'a>);
  impl<'a> oxc_ast_visit::VisitMut<'a> for Returns<'_, 'a> {
    fn visit_return_statement(&mut self, ret: &mut js::ReturnStatement<'a>) {
      ret.argument = Some((self.0)(ret.argument.take(), ret.span));
    }
    fn visit_function(&mut self, _: &mut js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _: &mut js::ArrowFunctionExpression<'a>) {}
    fn visit_class(&mut self, _: &mut js::Class<'a>) {}
  }
  let mut out: Vec<js::Statement<'a>> = stmts.iter().map(|stmt| b.copy(stmt)).collect();
  out.iter_mut().for_each(|stmt| stmt.walk_mut(&mut Returns(&pack)));
  if !matches!(out.last(), Some(js::Statement::ReturnStatement(_))) {
    let span = stmts.last().map(|s| s.span()).unwrap_or_default();
    out.push(js::Statement::new_return_statement(span, Some(pack(None, span)), &b.ast()));
  }
  Ok(out)
}

/// A body lowered from [`packing_returns`]' statements, `[value, last, ...]`, as what [`Builtin::WriteBack`] answers.
pub(crate) fn write_back(packed: Expr, written: &[(usize, String)]) -> Expr {
  let indexes = written.iter().map(|(i, _)| Entry::Item(Expr::Lit(Lit::Int(*i as i128)))).collect();
  Expr::Builtin { name: Builtin::WriteBack, args: vec![packed, Expr::Array(indexes)] }
}

/// Whether `expr` applies a function written in place that writes back to its arguments.
pub(super) fn writes_back(expr: &Expr) -> bool {
  matches!(expr, Expr::Apply { f, .. } if matches!(&**f, Expr::Lambda { body, .. } if matches!(&**body, Expr::Builtin { name: Builtin::WriteBack, .. })))
}

impl<'a> oxc_ast_visit::Visit<'a> for Mutations {
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if let Some(member) = call.callee.as_member_expression() {
      if let (js::Expression::Identifier(id), Some("set" | "delete" | "add" | "clear" | "run")) = (member.object(), prop_name_of(member).as_deref()) {
        if !self.0.iter().any(|n| *n == id.name.as_str()) {
          self.0.push(id.name.to_string());
        }
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
}

pub(crate) fn module_writes(module: &js::Program) -> Vec<String> {
  let mut writes = BareWrites { names: Vec::new(), through_functions: true, through_calls: true };
  module.walk(&mut writes);
  writes.names
}

impl BareWrites {
  pub(super) fn note(&mut self, name: &str) {
    if !self.names.iter().any(|n| n == name) {
      self.names.push(name.to_owned());
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for BareWrites {
  fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
    match &assign.left {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.note(&id.name),
      target if target.is_member_expression() && self.through_calls => {
        if let Some((id, _)) = target.as_member_expression().and_then(crate::write_root) {
          self.note(&id.name);
        }
      }
      _ => {}
    }
    oxc_ast_visit::walk::walk_assignment_expression(self, assign);
  }
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if self.through_calls {
      for arg in &call.arguments {
        if let Some(js::Expression::Identifier(id)) = arg.as_expression() {
          self.note(&id.name);
        }
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
  fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
    if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
      let name = id.name.to_string();
      if !self.names.contains(&name) {
        self.names.push(name);
      }
    }
    oxc_ast_visit::walk::walk_update_expression(self, update);
  }
  fn visit_arrow_function_expression(&mut self, arrow: &js::ArrowFunctionExpression<'a>) {
    if self.through_functions {
      oxc_ast_visit::walk::walk_arrow_function_expression(self, arrow);
    }
  }
  fn visit_function(&mut self, function: &js::Function<'a>, flags: oxc_syntax::scope::ScopeFlags) {
    if self.through_functions {
      oxc_ast_visit::walk::walk_function(self, function, flags);
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for Free<'_> {
  fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
    self.reads.push((id.name.to_string(), self.scope.of(id)));
  }
  fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
    self.declared.push((id.name.to_string(), self.scope.of_binding(id)));
  }
}

/// The first name `node` reads that is neither its own, a module-level binding nor a global: a name of the component it sits in. A function reading one cannot be imported from its module.
pub(crate) fn closes_over<'a>(parsed: &Parsed, node: &impl crate::syntax::Walk<'a>) -> Option<String> {
  let mut free = Free { scope: &parsed.scope, reads: Vec::new(), declared: Vec::new() };
  node.walk(&mut free);
  free.reads.into_iter().find(|(name, ctxt)| !matches!(ctxt, crate::Ctxt::Top | crate::Ctxt::Unresolved) && !free.declared.contains(&(name.clone(), *ctxt))).map(|(name, _)| name)
}

/// Whether `e` is an object literal with at least one field, every field a name holding an arrow or a `function`.
pub(super) fn functions_only(e: &js::Expression) -> bool {
  let js::Expression::ObjectExpression(obj) = e else { return false };
  !obj.properties.is_empty()
    && obj.properties.iter().all(|prop| match prop {
      js::ObjectPropertyKind::ObjectProperty(p) => matches!(crate::syntax::prop_of(p), crate::syntax::Prop::KeyValue(kv) if matches!(crate::meaning::bare(&kv.value), js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_))),
      js::ObjectPropertyKind::SpreadProperty(_) => false,
    })
}

/// Each `x++`, `x--`, `++x` or `--x` on a bare name that `stmt` reaches unconditionally, in source order: its span, the name, whether it is a prefix and whether it adds. One under a condition, a short-circuit or inside a function is left out, since lifting it would run it too soon.
pub(crate) fn updates_in(stmt: &js::Statement) -> Vec<(Span, String, bool, bool)> {
  struct Updates {
    found: Vec<(Span, String, bool, bool)>,
    guarded: usize,
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Updates {
    fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
      if self.guarded == 0 {
        if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
          self.found.push((update.span, id.name.to_string(), update.prefix, update.operator == js::UpdateOperator::Increment));
        }
      }
      oxc_ast_visit::walk::walk_update_expression(self, update);
    }
    fn visit_conditional_expression(&mut self, cond: &js::ConditionalExpression<'a>) {
      cond.test.walk(self);
      self.guarded += 1;
      cond.consequent.walk(self);
      cond.alternate.walk(self);
      self.guarded -= 1;
    }
    fn visit_logical_expression(&mut self, logical: &js::LogicalExpression<'a>) {
      logical.left.walk(self);
      self.guarded += 1;
      logical.right.walk(self);
      self.guarded -= 1;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_if_statement(&mut self, _: &js::IfStatement<'a>) {}
    fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
    fn visit_for_statement(&mut self, _: &js::ForStatement<'a>) {}
    fn visit_while_statement(&mut self, _: &js::WhileStatement<'a>) {}
    fn visit_switch_statement(&mut self, _: &js::SwitchStatement<'a>) {}
  }
  let mut updates = Updates { found: Vec::new(), guarded: 0 };
  stmt.walk(&mut updates);
  updates.found
}

pub(crate) fn bind_params(lowerer: &mut Lowerer<'_>, params: &[crate::Param]) -> Lowered<Vec<String>> {
  let mut names = Vec::new();
  for (i, param) in params.iter().enumerate() {
    match (param.pattern, param.default, param.rest) {
      (js::BindingPattern::BindingIdentifier(id), None, false) => {
        let name = id.name.to_string();
        lowerer.scope.push((name.clone(), Expr::Var(name.clone())));
        if let Some(kind) = param.type_annotation.and_then(|t| crate::Kind::of_type_in(&t.type_annotation, &lowerer.type_kinds)) {
          lowerer.kinds.push((name.clone(), Expr::Var(name.clone()), kind));
        }
        names.push(name);
      }
      (js::BindingPattern::BindingIdentifier(id), Some(default), false) => {
        let name = id.name.to_string();
        let default = lowerer.expr(default)?;
        lowerer.scope.push((name.clone(), Expr::Coalesce(Box::new(Expr::Var(name.clone())), Box::new(default))));
        names.push(name);
      }
      (pattern, _, true) => {
        let js::BindingPattern::BindingIdentifier(id) = pattern else { return Err(lowerer.residue(param.span, "a rest parameter that is not a name")) };
        if i + 1 != params.len() {
          return Err(lowerer.residue(param.span, "a rest parameter that is not last"));
        }
        let name = id.name.to_string();
        let gathered = format!("...{name}");
        lowerer.scope.push((name, Expr::Var(gathered.clone())));
        names.push(gathered);
      }
      (pattern, default, false) => {
        let positional = format!("${i}");
        let mut target = Expr::Var(positional.clone());
        if let Some(default) = default {
          target = Expr::Coalesce(Box::new(target), Box::new(lowerer.expr(default)?));
        }
        bind_pattern(lowerer, pattern, target)?;
        names.push(positional);
      }
    }
  }
  Ok(names)
}

/// Binds each name `pat` declares to the part of `target` it reads: a field, an item, the rest of an object or of an array. A default applies when the part is null or absent.
pub(crate) fn bind_pattern(lowerer: &mut Lowerer<'_>, pat: &js::BindingPattern, target: Expr) -> Lowered<()> {
  match pat {
    js::BindingPattern::BindingIdentifier(id) => {
      lowerer.scope.push((id.name.to_string(), target));
      Ok(())
    }
    js::BindingPattern::AssignmentPattern(assign) => {
      let default = lowerer.expr(&assign.right)?;
      bind_pattern(lowerer, &assign.left, Expr::Coalesce(Box::new(target), Box::new(default)))
    }
    js::BindingPattern::ObjectPattern(obj) => bind_object(lowerer, obj, target),
    js::BindingPattern::ArrayPattern(arr) => {
      for (j, elem) in crate::syntax::pat_elems(arr).into_iter().enumerate() {
        match elem {
          None => {}
          Some(crate::syntax::Elem::Rest(rest)) => {
            let tail = Expr::Builtin { name: Builtin::Slice, args: vec![target.clone(), Expr::Lit(Lit::Float(j as f64))] };
            bind_pattern(lowerer, &rest.argument, tail)?;
          }
          Some(crate::syntax::Elem::Pat(pat)) => bind_pattern(lowerer, pat, target.clone().index(Expr::Lit(Lit::Float(j as f64))))?,
        }
      }
      Ok(())
    }
  }
}

/// The kinds the props parameter's type names, `{ map: Map<string, number> }` written in place or through a `type` or `interface` this file declares, by prop name.
pub(super) fn prop_kinds_of(parsed: &Parsed, param: &crate::Param) -> Vec<(String, crate::Kind)> {
  let ann = match param.pattern {
    js::BindingPattern::BindingIdentifier(_) | js::BindingPattern::ObjectPattern(_) if param.default.is_none() && !param.rest => param.type_annotation,
    _ => None,
  };
  ann.map(|ann| member_kinds(parsed, &ann.type_annotation, 0)).unwrap_or_default()
}

pub(super) fn member_kinds(parsed: &Parsed, ty: &js::TSType, depth: usize) -> Vec<(String, crate::Kind)> {
  if depth > 8 {
    return Vec::new();
  }
  let of_members = |members: &[js::TSSignature]| -> Vec<(String, crate::Kind)> {
    members
      .iter()
      .filter_map(|member| {
        let js::TSSignature::TSPropertySignature(prop) = member else { return None };
        let (js::PropertyKey::StaticIdentifier(key), false) = (&prop.key, prop.computed) else { return None };
        crate::Kind::of_type(&prop.type_annotation.as_ref()?.type_annotation).map(|kind| (key.name.to_string(), kind))
      })
      .collect()
  };
  match ty {
    js::TSType::TSParenthesizedType(p) => member_kinds(parsed, &p.type_annotation, depth + 1),
    js::TSType::TSTypeLiteral(lit) => of_members(&lit.members),
    js::TSType::TSIntersectionType(i) => i.types.iter().flat_map(|t| member_kinds(parsed, t, depth + 1)).collect(),
    js::TSType::TSTypeReference(r) => {
      let js::TSTypeName::IdentifierReference(id) = &r.type_name else { return Vec::new() };
      let name = id.name.as_str();
      for item in &parsed.module().body {
        let crate::syntax::Top::Decl(decl) = crate::syntax::top(item) else { continue };
        match decl {
          js::Declaration::TSTypeAliasDeclaration(alias) if alias.id.name.as_str() == name => return member_kinds(parsed, &alias.type_annotation, depth + 1),
          js::Declaration::TSInterfaceDeclaration(interface) if interface.id.name.as_str() == name => return of_members(&interface.body.body),
          _ => {}
        }
      }
      Vec::new()
    }
    _ => Vec::new(),
  }
}

/// The kinds every placement recorded under `name` agrees on: a prop one placement passes as a `Map` and another as something else has no kind.
pub(super) fn agreed_kinds(placed: &[(String, Vec<(String, crate::Kind)>)], name: &str) -> Vec<(String, crate::Kind)> {
  let placements: Vec<&Vec<(String, crate::Kind)>> = placed.iter().filter(|(n, _)| n == name).map(|(_, kinds)| kinds).collect();
  let Some(first) = placements.first() else { return Vec::new() };
  first
    .iter()
    .filter(|(prop, kind)| placements.iter().all(|kinds| kinds.iter().any(|(p, k)| p == prop && k == kind)))
    .cloned()
    .collect()
}

/// Writes each placement's typed props under `TYPES_PROP`, an object of prop name to variant tag, so the island mounts with the real types.
pub(super) fn write_types(tmpl: &mut Tmpl, kinds_of: &dyn Fn(&str) -> Vec<(String, crate::Kind)>) {
  match tmpl {
    Tmpl::Component { module, props, children, .. } => {
      let fields: Vec<Entry> = kinds_of(module).into_iter().filter_map(|(prop, kind)| kind.tag().map(|tag| Entry::Field(prop, Expr::lit_str(tag)))).collect();
      if !fields.is_empty() && !props.iter().any(|e| matches!(e, Entry::Field(n, _) if n == TYPES_PROP)) {
        props.push(Entry::Field(TYPES_PROP.to_owned(), Expr::Object(fields)));
      }
      children.iter_mut().for_each(|c| write_types(c, kinds_of));
    }
    Tmpl::Element { children, .. } | Tmpl::Fragment(children) | Tmpl::Place { children, .. } | Tmpl::Island { children, .. } | Tmpl::Baked { children, .. } => children.iter_mut().for_each(|c| write_types(c, kinds_of)),
    Tmpl::If { then, r#else, .. } => {
      write_types(then, kinds_of);
      if let Some(e) = r#else {
        write_types(e, kinds_of);
      }
    }
    Tmpl::For { body, .. } => write_types(body, kinds_of),
    Tmpl::Let { then, .. } => write_types(then, kinds_of),
    Tmpl::Text(_) | Tmpl::Marker(_) | Tmpl::Outlet { .. } | Tmpl::Html(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
  }
}

pub(crate) fn bind_object(lowerer: &mut Lowerer<'_>, obj: &js::ObjectPattern, target: Expr) -> Lowered<()> {
  let mut taken: Vec<String> = Vec::new();
  for prop in crate::syntax::pat_props(obj) {
    match prop {
      crate::syntax::PatProp::Assign(a) => {
        let name = a.key.name.to_string();
        taken.push(name.clone());
        let read = target.clone().field(name.clone());
        let bound = match &a.value {
          Some(default) => Expr::Coalesce(Box::new(read), Box::new(lowerer.expr(default)?)),
          None => read,
        };
        lowerer.scope.push((name, bound));
      }
      crate::syntax::PatProp::KeyValue(kv) => {
        let key = prop_name(&kv.key, kv.computed).ok_or_else(|| lowerer.residue(kv.key.span(), "a computed field in a pattern"))?;
        taken.push(key.clone());
        bind_pattern(lowerer, &kv.value, target.clone().field(key))?;
      }
      crate::syntax::PatProp::Rest(r) => {
        let mut args = vec![target.clone()];
        args.extend(taken.iter().map(|key| Expr::lit_str(key.clone())));
        bind_pattern(lowerer, &r.argument, Expr::Builtin { name: Builtin::Omit, args })?;
      }
    }
  }
  Ok(())
}
