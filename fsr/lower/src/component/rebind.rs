use super::*;

impl Writable<'_> {
  pub(super) fn local(&self, lowerer: &Lowerer<'_>, name: &str) -> bool {
    !self.frozen.iter().any(|f| f == name) && lowerer.scope.iter().rposition(|(n, _)| n == name).is_some_and(|at| at >= self.floor)
  }
}

/// The state a call to one of the component's own setters sets.
pub(super) fn setter_called(call: &js::CallExpression, writable: &Writable<'_>) -> Option<String> {
  let callee = &call.callee;
  let js::Expression::Identifier(id) = callee else { return None };
  writable.setters.iter().find(|(setter, _)| *setter == id.name.as_str()).map(|(_, state)| state.clone())
}

/// A setter called while the component renders: `RERUN_PREFIX` and the state hold `[value]`, the next pass's state. An updater applies to what an earlier call in the pass asked for, else to the state this pass read, as React applies queued updates in order.
pub(super) fn rebind_setter(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, state: &str, assigned: &mut Vec<String>) -> Lowered<()> {
  let [arg] = call.arguments.as_slice() else { return Err(lowerer.residue(call.span, "a setter called while rendering with other than one argument")) };
  let key = format!("{}{state}", snapfire_fsr_ir::render::RERUN_PREFIX);
  let asked = current(lowerer, &key);
  let value = match crate::syntax::arg_expr(arg) {
    js::Expression::ArrowFunctionExpression(_) | js::Expression::FunctionExpression(_) => {
      let base = Expr::Ternary(Box::new(asked.clone()), Box::new(Expr::Index(Box::new(asked), Box::new(Expr::Lit(Lit::Int(0))))), Box::new(current(lowerer, state)));
      let f = match crate::syntax::arg_expr(arg) {
        js::Expression::ArrowFunctionExpression(arrow) => lowerer.lambda(arrow)?,
        other => lowerer.expr(other)?,
      };
      Expr::Apply { f: Box::new(f), args: vec![base] }
    }
    other => lowerer.expr(other)?,
  };
  lowerer.scope.push((key.clone(), Expr::Array(vec![Entry::Item(value)])));
  if !assigned.contains(&key) {
    assigned.push(key);
  }
  Ok(())
}

pub(super) fn current(lowerer: &Lowerer<'_>, name: &str) -> Expr {
  lowerer.scope.iter().rev().find(|(n, _)| n == name).map(|(_, e)| e.clone()).unwrap_or(Expr::Lit(Lit::Null))
}

pub(super) fn alias_key(name: &str) -> String {
  format!("$alias:{name}")
}

/// `test` with each `x++`, `x--`, `++x` or `--x` of a name `counted` accepts replaced by the value JavaScript reads there, and each such write in `updates`, in order.
pub(super) fn counted_test<'a>(b: crate::syntax::Build<'a>, test: &js::Expression, counted: &dyn Fn(&str) -> bool, updates: &mut Vec<(String, js::BinaryOperator, Span)>) -> js::Expression<'a> {
  match test {
    js::Expression::UpdateExpression(update) => match &update.argument {
      js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if counted(id.name.as_str()) => {
        let op = if update.operator == js::UpdateOperator::Increment { js::BinaryOperator::Addition } else { js::BinaryOperator::Subtraction };
        updates.push((id.name.to_string(), op, update.span));
        match update.prefix {
          false => b.read(id),
          true => b.binary(b.read(id), op, b.number(1.0, update.span), update.span),
        }
      }
      _ => b.copy(test),
    },
    js::Expression::BinaryExpression(bin) => b.binary(counted_test(b, &bin.left, counted, updates), bin.operator, counted_test(b, &bin.right, counted, updates), bin.span),
    js::Expression::LogicalExpression(l) => js::Expression::new_logical_expression(l.span, counted_test(b, &l.left, counted, updates), l.operator, counted_test(b, &l.right, counted, updates), &b.ast()),
    js::Expression::UnaryExpression(unary) => js::Expression::new_unary_expression(unary.span, unary.operator, counted_test(b, &unary.argument, counted, updates), &b.ast()),
    js::Expression::ParenthesizedExpression(paren) => b.paren(counted_test(b, &paren.expression, counted, updates), paren.span),
    other => b.copy(other),
  }
}

/// `setInterval(callback, ms)` or `setTimeout(callback, ms)` with its delay written out: the timer's kind, its callback and its delay.
pub(super) fn timer_call<'e>(e: &'e js::Expression<'e>) -> Option<(bool, &'e js::Expression<'e>, &'e js::Expression<'e>)> {
  let js::Expression::CallExpression(call) = crate::meaning::bare(e) else { return None };
  let callee = &call.callee;
  let js::Expression::Identifier(id) = crate::meaning::bare(callee) else { return None };
  let repeats = match id.name.as_str() {
    "setInterval" => true,
    "setTimeout" => false,
    _ => return None,
  };
  let [callback, delay] = call.arguments.as_slice() else { return None };
  Some((repeats, crate::syntax::arg_expr(callback), crate::syntax::arg_expr(delay)))
}

/// Records `const name = array[index]` or `array.at(index)` on a writable local array as an alias:
/// the array's name, the index as it was then and where the array was bound,
/// so a write through `name` reaches the element JavaScript shares.
pub(super) fn note_alias(lowerer: &mut Lowerer<'_>, name: &str, init: &js::Expression, writable: &Writable<'_>) -> Lowered<()> {
  if crate::heap_pass() {
    return Ok(());
  }
  let (array, index, from_end) = match init {
    member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match (crate::syntax::prop(member), member.object()) {
      (crate::syntax::MemberProp::Computed(prop), js::Expression::Identifier(array)) => (array, prop, false),
      _ => return note_field_alias(lowerer, name, init, writable),
    } },
    js::Expression::ChainExpression(_) => return note_field_alias(lowerer, name, init, writable),
    js::Expression::CallExpression(call) => match (&call.callee, call.arguments.as_slice()) {
      (callee, [at]) if !crate::syntax::is_spread(at) => match callee {
        member_expr if member_expr.is_member_expression() => { let member = member_expr.as_member_expression().expect("matched as a member"); match (crate::syntax::prop(member), member.object()) {
          (crate::syntax::MemberProp::Ident(method), js::Expression::Identifier(array)) if method.name.as_str() == "at" => (array, crate::syntax::arg_expr(at), true),
          _ => return Ok(()),
        } },
        _ => return Ok(()),
      },
      _ => return Ok(()),
    },
    _ => return Ok(()),
  };
  let array = array.name.to_string();
  if !writable.local(lowerer, &array) {
    return Ok(());
  }
  let Some(at) = lowerer.scope.iter().rposition(|(n, _)| *n == array) else { return Ok(()) };
  let index = lowerer.expr(index)?;
  let index = match from_end {
    false => index,
    true => {
      let counted = Expr::Arith(ArithOp::Add, Box::new(Expr::Length(Box::new(current(lowerer, &array)))), Box::new(index.clone()));
      Expr::Ternary(Box::new(Expr::Compare(CompareOp::Lt, Box::new(index.clone()), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(counted), Box::new(index))
    }
  };
  lowerer.scope.push((alias_key(name), Expr::Array(vec![Entry::Item(Expr::lit_str(array)), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(at as i128)))])));
  Ok(())
}

/// A local bound to a field of another local, `const verbs = config.settings.verbs`, through `.`, `?.` or a computed key: recorded so a write through it rewrites the root at that path too. The root must be a local the body may write.
pub(super) fn note_field_alias(lowerer: &mut Lowerer<'_>, name: &str, init: &js::Expression, writable: &Writable<'_>) -> Lowered<()> {
  let mut keys = Vec::new();
  let mut at = init;
  let root = loop {
    match crate::meaning::bare(at) {
      js::Expression::Identifier(id) => break id.name.to_string(),
      js::Expression::ChainExpression(chain) => match chain.expression.as_member_expression() {
        Some(member) => {
          keys.push(crate::syntax::prop(member));
          at = member.object();
        }
        None => return Ok(()),
      },
      member_expr if member_expr.is_member_expression() => {
        let member = member_expr.as_member_expression().expect("matched as a member");
        keys.push(crate::syntax::prop(member));
        at = member.object();
      }
      _ => return Ok(()),
    }
  };
  if keys.is_empty() || !writable.local(lowerer, &root) {
    return Ok(());
  }
  let Some(bound) = lowerer.scope.iter().rposition(|(n, _)| *n == root) else { return Ok(()) };
  keys.reverse();
  let mut lowered = Vec::with_capacity(keys.len());
  for key in keys {
    lowered.push(Entry::Item(match key {
      crate::syntax::MemberProp::Ident(field) => Expr::lit_str(field.name.as_str()),
      crate::syntax::MemberProp::Computed(key) => lowerer.expr(key)?,
      crate::syntax::MemberProp::Private(_) => return Ok(()),
    }));
  }
  lowerer.scope.push((alias_key(name), field_alias(&root, lowered, bound)));
  Ok(())
}

pub(super) fn field_alias(root: &str, keys: Vec<Entry>, bound: usize) -> Expr {
  Expr::Array(vec![Entry::Item(Expr::lit_str(FIELD_ALIAS)), Entry::Item(Expr::lit_str(root)), Entry::Item(Expr::Array(keys)), Entry::Item(Expr::Lit(Lit::Int(bound as i128)))])
}

/// `current` with `value` written at `keys`, a name a field and anything else an index, as [`Lowerer::written_at`] writes along a member chain.
pub(super) fn write_keys(current: Expr, keys: &[Expr], value: Expr) -> Expr {
  let Some((key, rest)) = keys.split_first() else { return value };
  match key {
    Expr::Lit(Lit::Str(field)) => {
      let inner = write_keys(current.clone().field(field.to_string()), rest, value);
      Expr::Object(vec![Entry::Spread(current), Entry::Field(field.to_string(), inner)])
    }
    key => {
      let inner = write_keys(Expr::Index(Box::new(current.clone()), Box::new(key.clone())), rest, value);
      Expr::Builtin { name: Builtin::SetAt, args: vec![current, key.clone(), inner] }
    }
  }
}

/// What a write to `name` reaches besides `name`. An alias whose array was
/// rebound after it stands for an element the build no longer knows.
pub(super) fn alias_of(lowerer: &Lowerer<'_>, name: &str) -> Alias {
  let key = alias_key(name);
  let Some(at) = lowerer.scope.iter().rposition(|(n, _)| *n == key) else { return Alias::None };
  if lowerer.scope.iter().rposition(|(n, _)| n == name).is_none_or(|bound| bound > at) {
    return Alias::None;
  }
  let Expr::Array(entries) = &lowerer.scope[at].1 else { return Alias::None };
  if let [Entry::Item(Expr::Lit(Lit::Str(tag))), Entry::Item(Expr::Lit(Lit::Str(root))), Entry::Item(Expr::Array(keys)), Entry::Item(Expr::Lit(Lit::Int(bound)))] = entries.as_slice() {
    if tag.as_str() == FIELD_ALIAS {
      let keys = keys.iter().filter_map(|key| match key { Entry::Item(key) => Some(key.clone()), _ => None }).collect();
      return match lowerer.scope.iter().rposition(|(n, _)| n == root) {
        Some(now) if now as i128 == *bound => Alias::Field { root: root.to_string(), keys },
        _ => Alias::StaleField(root.to_string()),
      };
    }
  }
  let [Entry::Item(Expr::Lit(Lit::Str(array))), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(bound)))] = entries.as_slice() else { return Alias::None };
  match lowerer.scope.iter().rposition(|(n, _)| n == array) {
    Some(now) if now as i128 == *bound => Alias::Of { array: array.clone(), index: index.clone() },
    _ => Alias::Stale(array.clone()),
  }
}

/// Rebinds the local `name` to `value` after a write to one of its fields,
/// and the array element or the field of another local it aliases with it.
/// The kind `name` had while bound to `from`, noted for it bound to `to`.
pub(super) fn keep_kind(lowerer: &mut Lowerer<'_>, name: &str, from: &Expr, to: Expr) {
  if let Some(kind) = lowerer.kinds.iter().rev().find(|(n, held, _)| n == name && held == from).map(|(_, _, kind)| *kind) {
    lowerer.kinds.push((name.to_owned(), to, kind));
  }
}

/// `name` rebound to `value`, which keeps the kind `name` had.
/// The kind of the value a name holds now. A write that replaces the value leaves it with none.
pub(super) fn kind_held(lowerer: &Lowerer<'_>, name: &str) -> Option<crate::Kind> {
  let bound = lowerer.scope.iter().rev().find(|(n, _)| n == name).map(|(_, bound)| bound)?;
  lowerer.kinds.iter().rev().find(|(n, held, _)| n == name && held == bound).map(|(_, _, kind)| *kind)
}

pub(super) fn rebind_kind(lowerer: &mut Lowerer<'_>, name: &str, value: Expr, kind: crate::Kind) {
  lowerer.scope.push((name.to_owned(), value.clone()));
  lowerer.kinds.push((name.to_owned(), value, kind));
}

/// `m.set(k, v)`, `s.add(v)`, `.delete(k)` or `.clear()` with the receiver a local `Map` or `Set`: the local, what it holds after and its kind.
pub(super) fn map_or_set_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr, crate::Kind)>> {
  let callee = &call.callee;
  let Some(m) = (callee).as_member_expression() else { return Ok(None) };
  let js::Expression::Identifier(id) = m.object() else { return Ok(None) };
  let Some(method) = prop_name_of(m).filter(|method| matches!(method.as_ref(), "set" | "add" | "delete" | "clear")) else { return Ok(None) };
  let Some(kind @ (crate::Kind::Map | crate::Kind::Set | crate::Kind::WeakMap | crate::Kind::WeakSet)) = lowerer.kind_of(m.object()) else { return Ok(None) };
  let name = id.name.to_string();
  if !writable.local(lowerer, &name) {
    return Ok(None);
  }
  let held = current(lowerer, &name);
  let value = lowerer.collection_written(call, kind, &method, held)?;
  Ok(Some((name, value, kind)))
}

/// `v.setUint16(offset, value, little)` and the other setters with `v` a local `DataView`: the local and the bytes it holds after.
pub(super) fn view_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr)>> {
  let Some(m) = call.callee.as_member_expression() else { return Ok(None) };
  let js::Expression::Identifier(id) = m.object() else { return Ok(None) };
  let Some(method) = prop_name_of(m).filter(|method| method.starts_with("set")) else { return Ok(None) };
  if lowerer.kind_of(m.object()) != Some(crate::Kind::View) || !writable.local(lowerer, id.name.as_str()) {
    return Ok(None);
  }
  let name = id.name.to_string();
  let mut args = vec![current(lowerer, &name), Expr::lit_str(&method[3..])];
  args.extend(lowerer.plain_args(call)?);
  Ok(Some((name, Expr::Builtin { name: Builtin::ViewSet, args })))
}

/// `d.setUTCHours(h)` and the other setters with `d` a local `Date`: the local and the instant it holds after. A setter that reads this machine's time zone is one only off a web render path.
pub(super) fn date_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr)>> {
  let Some(m) = call.callee.as_member_expression() else { return Ok(None) };
  let js::Expression::Identifier(id) = m.object() else { return Ok(None) };
  let Some(method) = prop_name_of(m) else { return Ok(None) };
  let local_time = lowerer.reaches(Reach::Terminal) || !lowerer.on_render_path();
  if !(crate::DATE_SETTERS.contains(&method.as_ref()) || local_time && crate::LOCAL_DATE_SETTERS.contains(&method.as_ref())) || lowerer.kind_of(m.object()) != Some(crate::Kind::Date) {
    return Ok(None);
  }
  let name = id.name.to_string();
  if !writable.local(lowerer, &name) {
    return Ok(None);
  }
  let held = current(lowerer, &name);
  let value = lowerer.date_set(&method, held, call)?;
  Ok(Some((name, value)))
}

/// `q.set(k, v)`, `q.append(k, v)` or `q.delete(k)` with `q` a local `URLSearchParams` (or the same on `url.searchParams` with `url` a local `URL`): the local, what it holds after the write and its kind.
pub(super) fn search_params_write(lowerer: &mut Lowerer<'_>, call: &js::CallExpression, writable: &Writable<'_>) -> Lowered<Option<(String, Expr, crate::Kind)>> {
  let callee = &call.callee;
  let Some(m) = (callee).as_member_expression() else { return Ok(None) };
  let method = match prop_name_of(m).as_deref() {
    Some(method @ ("set" | "append" | "delete")) => method.to_owned(),
    _ => return Ok(None),
  };
  let (name, kind) = match m.object() {
    js::Expression::Identifier(id) if lowerer.kind_of(m.object()) == Some(crate::Kind::Query) => (id.name.to_string(), crate::Kind::Query),
    params_expr if params_expr.is_member_expression() => { let params = params_expr.as_member_expression().expect("matched as a member"); match (params.object(), prop_name_of(params).as_deref()) {
      (js::Expression::Identifier(id), Some("searchParams")) if lowerer.kind_of(params.object()) == Some(crate::Kind::Url) => (id.name.to_string(), crate::Kind::Url),
      _ => return Ok(None),
    } },
    _ => return Ok(None),
  };
  if !writable.local(lowerer, &name) {
    return Ok(None);
  }
  let held = current(lowerer, &name);
  let search = match kind {
    crate::Kind::Url => Expr::Builtin { name: Builtin::UrlPart, args: vec![held.clone(), Expr::lit_str("search")] },
    _ => held.clone(),
  };
  let written = lowerer.collection_written(call, crate::Kind::Query, &method, search)?;
  Ok(Some(match kind {
    crate::Kind::Url => (name, Expr::Builtin { name: Builtin::UrlSet, args: vec![held, Expr::lit_str("search"), written] }, crate::Kind::Url),
    _ => (name, written, crate::Kind::Query),
  }))
}

pub(super) fn write_field(lowerer: &mut Lowerer<'_>, span: Span, name: &str, value: Expr, assigned: &mut Vec<String>) -> Lowered<()> {
  let alias = alias_of(lowerer, name);
  if let Alias::Stale(array) = &alias {
    return Err(lowerer.residue(span, format!("a write through `{name}`, an element of `{array}`, after `{array}` was written")));
  }
  if let Alias::StaleField(root) = &alias {
    return Err(lowerer.residue(span, format!("a write through `{name}`, a field of `{root}`, after `{root}` was written")));
  }
  if let Alias::Field { root, keys } = &alias {
    lowerer.scope.push((name.to_owned(), value.clone()));
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
    let rewritten = write_keys(current(lowerer, root), keys, value);
    write_field(lowerer, span, root, rewritten, assigned)?;
    let bound = lowerer.scope.iter().rposition(|(n, _)| n == root).expect("the root was just bound");
    lowerer.scope.push((alias_key(name), field_alias(root, keys.iter().cloned().map(Entry::Item).collect(), bound)));
    return Ok(());
  }
  let mut note = |name: &str| {
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
  };
  lowerer.scope.push((name.to_owned(), value.clone()));
  note(name);
  if let Alias::Of { array, index } = alias {
    let held = current(lowerer, &array);
    let after = Expr::Arith(ArithOp::Add, Box::new(index.clone()), Box::new(Expr::Lit(Lit::Int(1))));
    let rewritten = Expr::Array(vec![
      Entry::Spread(Expr::Builtin { name: Builtin::Slice, args: vec![held.clone(), Expr::Lit(Lit::Int(0)), index.clone()] }),
      Entry::Item(value),
      Entry::Spread(Expr::Builtin { name: Builtin::Slice, args: vec![held, after] }),
    ]);
    lowerer.scope.push((array.clone(), rewritten));
    note(&array);
    let at = lowerer.scope.len() - 1;
    lowerer.scope.push((alias_key(name), Expr::Array(vec![Entry::Item(Expr::lit_str(array)), Entry::Item(index), Entry::Item(Expr::Lit(Lit::Int(at as i128)))])));
  }
  Ok(())
}

/// Statements with no `return` that bind and assign, run at build time over the scope: a write rebinds its name to the new value, an `if` rebinds each name a branch wrote to the branch's value under the condition. `assigned` collects the names written, first write first.
pub(crate) fn rebind_stmt(lowerer: &mut Lowerer<'_>, stmt: &js::Statement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if let js::Statement::ExpressionStatement(e) = stmt {
    if lowerer.object_write(&e.expression) {
      if crate::heap_pass() {
        let effect = lowerer.heap_write(&e.expression)?;
        if lowerer.held.is_empty() {
          let held = lowerer.temp();
          lowerer.scope.push((held.clone(), effect));
          if !assigned.iter().any(|a| *a == held) {
            assigned.push(held);
          }
        } else {
          hold_in_reach(lowerer, effect);
        }
        return Ok(());
      }
      crate::want_heap();
    }
  }
  if !lowerer.held.is_empty() {
    if let js::Statement::ExpressionStatement(e) = stmt {
      let to_cell = match &e.expression {
        js::Expression::AssignmentExpression(assign) => lowerer.cell_of_target(&assign.left).is_some(),
        js::Expression::UpdateExpression(update) => matches!(&update.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) if lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())),
        js::Expression::CallExpression(call) => matches!(&call.callee, callee if (callee).as_member_expression().is_some_and(|m| matches!(m.object(), js::Expression::Identifier(id) if lowerer.cells.iter().any(|(name, _)| *name == id.name.as_str())))),
        _ => false,
      };
      if to_cell {
        let effect = lowerer.expr(&e.expression)?;
        hold_in_reach(lowerer, effect);
        return Ok(());
      }
    }
  }
  let from = lowerer.scope.len();
  rebind_stmt_inner(lowerer, stmt, writable, assigned)?;
  if !lowerer.held.is_empty() {
    settle(lowerer, from);
  }
  Ok(())
}

pub(super) fn rebind_stmt_inner(lowerer: &mut Lowerer<'_>, stmt: &js::Statement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let note = |assigned: &mut Vec<String>, name: &str| {
    if !assigned.iter().any(|a| a == name) {
      assigned.push(name.to_owned());
    }
  };
  match stmt {
    js::Statement::VariableDeclaration(var) => {
      for decl in &var.declarations {
        let mut expr = match decl.init.as_ref() {
          Some(init) => lowerer.expr(init)?,
          None => Expr::Lit(Lit::Undefined),
        };
        if !lowerer.held.is_empty() && lowerer.body_extension_in(&expr).is_some() {
          let name = lowerer.temp();
          lowerer.held.last_mut().expect("a frame").push(Stmt::Let { name: name.clone(), expr });
          expr = Expr::Var(name);
        } else if !lowerer.held.is_empty() && !plain(&expr) {
          expr = hold_in_reach(lowerer, expr);
        }
        bind_pattern(lowerer, &decl.id, expr)?;
        if let (js::BindingPattern::BindingIdentifier(id), Some(init)) = (&decl.id, decl.init.as_ref()) {
          lowerer.note_kind(id.name.as_str(), init);
          note_alias(lowerer, id.name.as_str(), init, writable)?;
        }
      }
      Ok(())
    }
    js::Statement::ExpressionStatement(e) => match &e.expression {
      js::Expression::AssignmentExpression(assign) => {
        let name = match &assign.left {
          js::AssignmentTarget::AssignmentTargetIdentifier(id) => id.name.to_string(),
          target => match target.as_member_expression().and_then(crate::write_root) {
            Some((id, _)) => id.name.to_string(),
            None => return Err(lowerer.residue(assign.span, "a write to something other than a local")),
          },
        };
        let b = lowerer.build();
        let read = crate::syntax::target_expr(b, &assign.left).expect("a name or a member");
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue_with(assign.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
        }
        let value = match crate::syntax::compound(b, assign.operator, b.copy(&read), b.copy(&assign.right), assign.span) {
          None => lowerer.expr(&assign.right)?,
          Some(computed) => lowerer.expr(&computed)?,
        };
        match read.as_member_expression() {
          Some(member) if matches!(member.object(), js::Expression::Identifier(_)) && lowerer.kind_of(member.object()) == Some(crate::Kind::Url) => {
            let crate::syntax::MemberProp::Ident(part) = crate::syntax::prop(member) else { return Err(lowerer.residue(member.span(), "a computed part of a `URL`")) };
            let part = part.name.to_string();
            if part == "origin" || !crate::URL_PARTS.contains(&part.as_str()) && part != "href" {
              return Err(lowerer.residue(member.span(), format!("a write to `.{part}` of a `URL`")));
            }
            let value = Expr::Builtin { name: Builtin::UrlSet, args: vec![current(lowerer, &name), Expr::lit_str(&part), value] };
            rebind_kind(lowerer, &name, value, crate::Kind::Url);
            note(assigned, &name);
            Ok(())
          }
          Some(member) => {
            let (_, path) = crate::write_root(member).expect("the root found above");
            let value = lowerer.written_at(current(lowerer, &name), &path, value)?;
            write_field(lowerer, assign.span, &name, value, assigned)
          }
          None => {
            lowerer.scope.push((name.clone(), value));
            note(assigned, &name);
            Ok(())
          }
        }
      }
      js::Expression::UpdateExpression(update) => {
        let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument else { return Err(lowerer.residue(update.span, "an update of something other than a local")) };
        let name = id.name.to_string();
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue(update.span, format!("an update of `{name}`, which this body did not declare")));
        }
        let op = match update.operator {
          js::UpdateOperator::Increment => js::BinaryOperator::Addition,
          js::UpdateOperator::Decrement => js::BinaryOperator::Subtraction,
        };
        let b = lowerer.build();
        let value = lowerer.expr(&b.binary(b.updated(&update.argument), op, b.number(1.0, update.span), update.span))?;
        lowerer.scope.push((name.clone(), value));
        note(assigned, &name);
        Ok(())
      }
      js::Expression::UnaryExpression(unary) if unary.operator == js::UnaryOperator::Delete => {
        let Some(member) = (crate::meaning::bare(&unary.argument)).as_member_expression() else { return Err(lowerer.residue(unary.span, "`delete` of something other than a field of a local")) };
        let Some((id, path)) = crate::write_root(member) else { return Err(lowerer.residue(unary.span, "`delete` of something other than a field of a local")) };
        let name = id.name.to_string();
        if !writable.local(lowerer, &name) {
          return Err(lowerer.residue_with(unary.span, format!("a write to `{name}`, which this body did not declare"), "declare a local with `let` and write that; state changes through its setter"));
        }
        let key = match crate::syntax::prop(member) {
          crate::syntax::MemberProp::Ident(key) => Expr::lit_str(key.name.as_str()),
          crate::syntax::MemberProp::Computed(computed) => lowerer.expr(computed)?,
          crate::syntax::MemberProp::Private(_) => return Err(lowerer.residue(member.span(), "`delete` of a private field")),
        };
        let parent = lowerer.expr(member.object())?;
        let omitted = Expr::Builtin { name: Builtin::Omit, args: vec![parent, key] };
        let value = lowerer.written_at(current(lowerer, &name), &path[..path.len() - 1], omitted)?;
        write_field(lowerer, unary.span, &name, value, assigned)
      }
      js::Expression::CallExpression(call) => {
        if let Some(logged) = console_call(lowerer, call)? {
          let name = format!("$log{}", crate::lo(call.span));
          lowerer.scope.push((name.clone(), logged));
          note(assigned, &name);
          return Ok(());
        }
        if let Some(looped) = for_each_loop(lowerer.build(), call, lowerer.meanings.as_deref()) {
          return rebind_stmt(lowerer, &looped, writable, assigned);
        }
        if let Some(state) = setter_called(call, writable) {
          return rebind_setter(lowerer, call, &state, assigned);
        }
        {
      let callee = &call.callee;
          if let Some(m) = (callee).as_member_expression() {
            if let (js::Expression::Identifier(id), Some(method @ ("reverse" | "sort" | "pop" | "shift"))) = (m.object(), prop_name_of(m).as_deref()) {
              let name = id.name.to_string();
              if writable.local(lowerer, &name) {
                let held = current(lowerer, &name);
                let value = match method {
                  "reverse" => Expr::Builtin { name: Builtin::Reverse, args: vec![held] },
                  "sort" => {
                    let by = match call.arguments.first().map(|a| crate::syntax::arg_expr(a)) {
                      None => Expr::Lit(Lit::Null),
                      Some(js::Expression::ArrowFunctionExpression(arrow)) => lowerer.lambda(arrow)?,
                      Some(other) => lowerer.expr(other)?,
                    };
                    Expr::Sort(Box::new(held), Box::new(by))
                  }
                  "pop" => Expr::Builtin { name: Builtin::Slice, args: vec![held, Expr::Lit(Lit::Int(0)), Expr::Lit(Lit::Int(-1))] },
                  _ => Expr::Builtin { name: Builtin::Slice, args: vec![held, Expr::Lit(Lit::Int(1))] },
                };
                lowerer.scope.push((name.clone(), value));
                note(assigned, &name);
                return Ok(());
              }
            }
          }
        }
        if let Some((name, value, kind)) = map_or_set_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, kind);
          note(assigned, &name);
          return Ok(());
        }
        if let Some((name, value)) = view_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, crate::Kind::View);
          note(assigned, &name);
          return Ok(());
        }
        if let Some((name, value)) = date_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, crate::Kind::Date);
          note(assigned, &name);
          return Ok(());
        }
        if let Some((name, value, kind)) = search_params_write(lowerer, call, writable)? {
          rebind_kind(lowerer, &name, value, kind);
          note(assigned, &name);
          return Ok(());
        }
        let target = match call.callee.as_member_expression() {
          Some(m) => match (m.object(), prop_name_of(m).as_deref()) {
            (js::Expression::Identifier(id), Some(method @ ("push" | "unshift"))) if writable.local(lowerer, id.name.as_str()) => Some((id.name.to_string(), None, method == "push")),
            (object, Some(method @ ("push" | "unshift"))) => match object.as_member_expression().map(|field| (field.object(), prop_name_of(field))) {
              Some((js::Expression::Identifier(id), Some(name))) if writable.local(lowerer, id.name.as_str()) => Some((id.name.to_string(), Some((name, m.object())), method == "push")),
              _ => None,
            },
            _ => None,
          },
          None => None,
        };
        if lowerer.reaches(Reach::Terminal) && target.is_none() {
          // A terminal page runs a render's side effects as JavaScript does, in order: a call that emits, acts or writes a cell is held where it stands.
          let effect = lowerer.expr(&e.expression)?;
          if effectful(&effect) {
            let held = lowerer.temp();
            lowerer.scope.push((held.clone(), effect));
            note(assigned, &held);
            return Ok(());
          }
        }
        if target.is_none() && crate::heap_pass() {
          let effect = lowerer.expr(&e.expression)?;
          if lowerer.held.is_empty() {
            let held = lowerer.temp();
            lowerer.scope.push((held.clone(), effect));
            note(assigned, &held);
          } else {
            hold_in_reach(lowerer, effect);
          }
          return Ok(());
        }
        let Some((name, field, at_end)) = target else { return Err(lowerer.residue(call.span, "a call for its effect, which a straight-line body cannot hold")) };
        let mut items = Vec::with_capacity(call.arguments.len() + 1);
        for arg in &call.arguments {
          let value = lowerer.expr(crate::syntax::arg_expr(arg))?;
          items.push(if crate::syntax::is_spread(arg) { Entry::Spread(value) } else { Entry::Item(value) });
        }
        let held = Entry::Spread(match &field {
          Some((_, read)) => lowerer.expr(read)?,
          None => current(lowerer, &name),
        });
        if at_end {
          items.insert(0, held);
        } else {
          items.push(held);
        }
        match field {
          Some((field, _)) => {
            let value = Expr::Object(vec![Entry::Spread(current(lowerer, &name)), Entry::Field(field, Expr::Array(items))]);
            write_field(lowerer, call.span, &name, value, assigned)
          }
          None => {
            lowerer.scope.push((name.clone(), Expr::Array(items)));
            note(assigned, &name);
            Ok(())
          }
        }
      }
      _ => {
        let effect = lowerer.expr(&e.expression)?;
        if lowerer.held.is_empty() {
          let held = lowerer.temp();
          lowerer.scope.push((held.clone(), effect));
          note(assigned, &held);
        } else {
          hold_in_reach(lowerer, effect);
        }
        Ok(())
      }
    },
    js::Statement::IfStatement(branch) => {
      let mut cond = lowerer.expr(&branch.test)?;
      if !lowerer.held.is_empty() && !plain(&cond) {
        cond = hold_in_reach(lowerer, cond);
      }
      let arms = [Some(&branch.consequent), branch.alternate.as_ref()];
      rebind_arms(lowerer, vec![(Some(cond), arms[0].map(std::slice::from_ref).unwrap_or(&[]))], arms[1].map(std::slice::from_ref).unwrap_or(&[]), writable, assigned)
    }
    js::Statement::SwitchStatement(sw) => {
      let mut disc = lowerer.expr(&sw.discriminant)?;
      if !lowerer.held.is_empty() && !plain(&disc) {
        disc = hold_in_reach(lowerer, disc);
      }
      let (arms, default) = switch_arms(lowerer, sw)?;
      let mut lowered = Vec::with_capacity(arms.len());
      for (tests, body) in arms {
        let mut cond: Option<Expr> = None;
        for test in tests {
          let eq = Expr::Compare(CompareOp::Eq, Box::new(disc.clone()), Box::new(lowerer.expr(test)?));
          cond = Some(match cond {
            None => eq,
            Some(c) => Expr::Logic(LogicOp::Or, Box::new(c), Box::new(eq)),
          });
        }
        lowered.push((cond, body));
      }
      rebind_arms(lowerer, lowered, default.unwrap_or(&[]), writable, assigned)
    }
    js::Statement::BlockStatement(block) => {
      let depth = lowerer.scope.len();
      let mut inner = Vec::new();
      for stmt in &block.body {
        rebind_stmt(lowerer, stmt, writable, &mut inner)?;
      }
      let values: Vec<(String, Expr)> = inner.iter().map(|n| (n.clone(), current(lowerer, n))).collect();
      lowerer.scope.truncate(depth);
      for (name, value) in values {
        lowerer.scope.push((name.clone(), value));
        note(assigned, &name);
      }
      Ok(())
    }
    js::Statement::ForOfStatement(for_of) => rebind_for_of(lowerer, for_of, writable, assigned),
    js::Statement::ForStatement(counted) => rebind_counted(lowerer, counted, writable, assigned),
    js::Statement::WhileStatement(looped) if lowerer.reaches(Reach::Terminal) => rebind_while(lowerer, looped, writable, assigned),
    js::Statement::EmptyStatement(_) => Ok(()),
    js::Statement::TryStatement(t) if !holds_return(stmt) && !lowerer.held_reach.is_empty() => {
      let (ran, written) = try_run(lowerer, t, writable)?;
      let base = *lowerer.held_reach.last().expect("checked");
      let reach = lowerer.rebind_reach[base..].iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::And, Box::new(a), Box::new(b)));
      let ran = match reach {
        Some(reach) => Expr::Ternary(Box::new(reach), Box::new(ran), Box::new(Expr::Lit(Lit::Null))),
        None => ran,
      };
      let held = lowerer.temp();
      lowerer.held.last_mut().expect("checked").push(Stmt::Let { name: held.clone(), expr: ran });
      for name in written {
        let before = current(lowerer, &name);
        let after = Expr::Field(Box::new(Expr::Var(held.clone())), name.clone());
        keep_kind(lowerer, &name, &before, after.clone());
        lowerer.scope.push((name.clone(), after));
        note(assigned, &name);
      }
      Ok(())
    }
    other => Err(lowerer.residue(other.span(), "a statement a straight-line body cannot hold; it holds `const`s, `let`s, writes to them, `if`, `switch`, `for…of`, `for (let i = a; i < b; i += s)` and `try`")),
  }
}

impl Writes {
  pub(super) fn wrote(&mut self, name: &str) {
    if !self.written.iter().any(|w| w == name) {
      self.written.push(name.to_owned());
    }
  }
}

impl<'a> oxc_ast_visit::Visit<'a> for Writes {
  fn visit_assignment_expression(&mut self, assign: &js::AssignmentExpression<'a>) {
    match &assign.left {
      js::AssignmentTarget::AssignmentTargetIdentifier(id) => self.wrote(id.name.as_str()),
      target if target.is_member_expression() => {
        if let Some(js::Expression::Identifier(id)) = target.as_member_expression().map(|m| m.object()) {
          self.wrote(id.name.as_str());
        }
      }
      _ => {}
    }
    assign.right.walk(self);
  }
  fn visit_update_expression(&mut self, update: &js::UpdateExpression<'a>) {
    if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &update.argument {
      self.wrote(id.name.as_str());
    }
    oxc_ast_visit::walk::walk_update_expression(self, update);
  }
  fn visit_call_expression(&mut self, call: &js::CallExpression<'a>) {
    if let Some(m) = call.callee.as_member_expression() {
      match (m.object(), prop_name_of(m).as_deref()) {
        (js::Expression::Identifier(id), Some("push" | "unshift" | "set" | "append" | "delete" | "add" | "clear")) => self.wrote(id.name.as_str()),
        (object, Some("set" | "append" | "delete")) => {
          if let Some(params) = object.as_member_expression() {
            if let (js::Expression::Identifier(id), Some("searchParams")) = (params.object(), prop_name_of(params).as_deref()) {
              self.wrote(id.name.as_str());
            }
          }
        }
        _ => {}
      }
    }
    oxc_ast_visit::walk::walk_call_expression(self, call);
  }
  fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
    self.declared.push(id.name.to_string());
  }
  fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
}

impl LoopNames {
  pub(crate) fn of(for_of: &js::ForOfStatement) -> Self {
    Self::over(for_of.span, &for_of.left, &for_of.body)
  }

  pub(crate) fn over<'a>(span: Span, head: &impl crate::syntax::Walk<'a>, body: &js::Statement<'a>) -> Self {
    let mut writes = Writes::default();
    head.walk(&mut writes);
    body.walk(&mut writes);
    let Writes { written, declared } = writes;
    let at = crate::lo(span);
    Self { item: format!("$item{at}"), acc: format!("$acc{at}"), written: written.into_iter().filter(|w| !declared.contains(w)).collect() }
  }

  /// What a written name reads inside the loop: the accumulator itself for one name, its field for several.
  pub(crate) fn read(&self, name: &str) -> Expr {
    match self.written.len() {
      1 => Expr::Var(self.acc.clone()),
      _ => Expr::Field(Box::new(Expr::Var(self.acc.clone())), name.to_owned()),
    }
  }

  /// The pattern the loop binds each item to: its head's one `const` or `let`.
  pub(crate) fn head<'e>(for_of: &'e js::ForOfStatement<'e>) -> Option<&'e js::BindingPattern<'e>> {
    match &for_of.left {
      js::ForStatementLeft::VariableDeclaration(var) => match var.declarations.as_slice() {
        [decl] if decl.init.is_none() => Some(&decl.id),
        _ => None,
      },
      _ => None,
    }
  }
}

/// A `for…of` as a fold: each outer local its body writes is the `reduce` over the items of the body's writes, starting from its value before the loop.
pub(super) fn rebind_for_of(lowerer: &mut Lowerer<'_>, for_of: &js::ForOfStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if for_of.r#await {
    return Err(lowerer.residue(for_of.span, "`for await`, which a render cannot wait on"));
  }
  let Some(head) = LoopNames::head(for_of) else { return Err(lowerer.residue(for_of.span, "a `for…of` whose head is not one `const` or `let`")) };
  let names = LoopNames::of(for_of);
  let over = lowerer.expr(&for_of.right)?;
  let item = Expr::Var(names.item.clone());
  fold(lowerer, for_of.span, &names, over, &for_of.body, None, |lowerer| bind_pattern(lowerer, head, item), writable, assigned)
}

impl<'e> Counted<'e> {
  pub(crate) fn of(lowerer: &Lowerer<'_>, f: &'e js::ForStatement<'e>) -> Lowered<Self> {
    let shape = |why: &str| lowerer.residue_with(f.span, format!("a `for` loop {why}"), "a loop the build reads is `for (const x of items)` or `for (let i = a; i < b; i += s)`");
    let Some(js::ForStatementInit::VariableDeclaration(var)) = f.init.as_ref() else { return Err(shape("that does not declare its counter")) };
    let [decl] = var.declarations.as_slice() else { return Err(shape("that declares more than one counter")) };
    let (js::BindingPattern::BindingIdentifier(id), Some(from)) = (&decl.id, decl.init.as_ref()) else { return Err(shape("whose counter has no starting value")) };
    let name = id.name.to_string();
    let is_counter = |e: &js::Expression| matches!(e, js::Expression::Identifier(i) if i.name.as_str() == name);
    let (to, inclusive) = match f.test.as_ref() {
      Some(js::Expression::BinaryExpression(b)) if is_counter(&b.left) && b.operator == js::BinaryOperator::LessThan => (&b.right, false),
      Some(js::Expression::BinaryExpression(b)) if is_counter(&b.left) && b.operator == js::BinaryOperator::LessEqualThan => (&b.right, true),
      _ => return Err(shape("whose test is not `i < b` or `i <= b`")),
    };
    let step = match f.update.as_ref() {
      Some(js::Expression::UpdateExpression(u)) if u.operator == js::UpdateOperator::Increment && matches!(&u.argument, js::SimpleAssignmentTarget::AssignmentTargetIdentifier(i) if i.name.as_str() == name) => None,
      Some(js::Expression::AssignmentExpression(a)) if a.operator == js::AssignmentOperator::Addition && matches!(&a.left, js::AssignmentTarget::AssignmentTargetIdentifier(i) if i.name.as_str() == name) => match &a.right {
        js::Expression::NumericLiteral(n) if n.value == 1.0 => None,
        step => Some(step),
      },
      _ => return Err(shape("that does not step up with `i++` or `i += s`")),
    };
    let mut writes = Writes::default();
    f.body.walk(&mut writes);
    if writes.written.contains(&name) {
      return Err(shape(&format!("whose body writes `{name}`")));
    }
    struct Reads(Vec<String>);
    impl<'a> oxc_ast_visit::Visit<'a> for Reads {
      fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
        self.0.push(id.name.to_string());
      }
    }
    let mut reads = Reads(Vec::new());
    to.walk(&mut reads);
    if let Some(read) = reads.0.iter().find(|r| writes.written.contains(r)) {
      return Err(shape(&format!("whose bound reads `{read}`, which its body writes")));
    }
    if let Some(step) = &step {
      let mut reads = Reads(Vec::new());
      step.walk(&mut reads);
      if let Some(read) = reads.0.iter().find(|r| writes.written.contains(r) || **r == name) {
        return Err(shape(&format!("whose step reads `{read}`, which the loop writes")));
      }
    }
    Ok(Self { name, from, to, inclusive, step })
  }

  /// What one pass adds to the counter.
  pub(crate) fn step_expr(&self, lowerer: &mut Lowerer<'_>) -> Lowered<Expr> {
    match self.step {
      Some(step) => lowerer.expr(step),
      None => Ok(Expr::Lit(Lit::Float(1.0))),
    }
  }

  /// How many passes the loop makes from `from` to `to`. A step that is not a positive number counts without end, which `Range` refuses, since JavaScript would loop for ever.
  pub(crate) fn count(&self, lowerer: &mut Lowerer<'_>, from: Expr, to: Expr) -> Lowered<Expr> {
    use snapfire_fsr_ir::ast::ArithOp;
    let diff = Expr::Arith(ArithOp::Sub, Box::new(to), Box::new(from));
    let Some(step) = &self.step else {
      let count = if self.inclusive { Expr::Arith(ArithOp::Add, Box::new(diff), Box::new(Expr::Lit(Lit::Float(1.0)))) } else { diff };
      return Ok(Expr::Builtin { name: Builtin::Ceil, args: vec![count] });
    };
    let positive = matches!(step, js::Expression::NumericLiteral(n) if n.value > 0.0);
    let step = lowerer.expr(step)?;
    let passes = Expr::Arith(ArithOp::Div, Box::new(diff), Box::new(step.clone()));
    let count = match self.inclusive {
      true => Expr::Arith(ArithOp::Add, Box::new(Expr::Builtin { name: Builtin::Floor, args: vec![passes] }), Box::new(Expr::Lit(Lit::Float(1.0)))),
      false => Expr::Builtin { name: Builtin::Ceil, args: vec![passes] },
    };
    if positive {
      return Ok(count);
    }
    let without_end = Expr::Arith(ArithOp::Div, Box::new(Expr::Lit(Lit::Float(1.0))), Box::new(Expr::Lit(Lit::Float(0.0))));
    Ok(Expr::Ternary(Box::new(Expr::Compare(snapfire_fsr_ir::ast::CompareOp::Gt, Box::new(step), Box::new(Expr::Lit(Lit::Float(0.0))))), Box::new(count), Box::new(without_end)))
  }

  /// The counter on the pass `item` counts, from the loop's start `start`.
  pub(crate) fn counter(&self, lowerer: &mut Lowerer<'_>, start: Expr, item: Expr) -> Lowered<Expr> {
    use snapfire_fsr_ir::ast::ArithOp;
    let offset = match &self.step {
      None => item,
      Some(step) => Expr::Arith(ArithOp::Mul, Box::new(item), Box::new(lowerer.expr(step)?)),
    };
    Ok(Expr::Arith(ArithOp::Add, Box::new(start), Box::new(offset)))
  }

  /// The counter's start, its bound and whether the bound is inclusive.
  pub(crate) fn bounds(&self) -> (&js::Expression<'_>, &js::Expression<'_>, bool) {
    (&self.from, &self.to, self.inclusive)
  }

  /// The items the loop runs over (`0..count`) with what the counter is for the item `k`.
  pub(crate) fn lowered(&self, lowerer: &mut Lowerer<'_>, item: &str) -> Lowered<(Expr, Expr)> {
    let from = lowerer.expr(&self.from)?;
    let to = lowerer.expr(&self.to)?;
    let over = Expr::Builtin { name: Builtin::Range, args: vec![self.count(lowerer, from.clone(), to)?] };
    let counter = self.counter(lowerer, from, Expr::Var(item.to_owned()))?;
    Ok((over, counter))
  }
}

pub(super) fn rebind_counted(lowerer: &mut Lowerer<'_>, f: &js::ForStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let counted = Counted::of(lowerer, f)?;
  let names = LoopNames::over(f.span, &f.init, &f.body);
  let (over, counter) = counted.lowered(lowerer, &names.item)?;
  let name = counted.name.clone();
  fold(lowerer, f.span, &names, over, &f.body, None, |lowerer| {
    lowerer.scope.push((name, counter));
    Ok(())
  }, writable, assigned)
}

/// A terminal's `while (test) body` as a fold over [`WHILE_CAP`] items whose
/// step runs only while `test` holds, so an item past the first false test
/// reads the test alone. A loop with an empty body only spends time and lowers to nothing.
pub(super) fn rebind_while(lowerer: &mut Lowerer<'_>, looped: &js::WhileStatement, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  if matches!(&looped.body, js::Statement::BlockStatement(block) if block.body.is_empty()) || matches!(&looped.body, js::Statement::EmptyStatement(_)) {
    return Ok(());
  }
  let b = lowerer.build();
  let guarded = js::Statement::new_if_statement(looped.span, b.copy(&looped.test), b.copy(&looped.body), None, &b.ast());
  let names = LoopNames::over(looped.span, &looped.test, &guarded);
  let over = Expr::Builtin { name: Builtin::Range, args: vec![Expr::Lit(Lit::Float(WHILE_CAP))] };
  fold(lowerer, looped.span, &names, over, &looped.body, Some(&looped.test), |_| Ok(()), writable, assigned)
}

/// A loop as a fold: each outer local the body writes is the `reduce` over `over` of the body's writes, `bind` binding the item, starting from its value before the loop. A step under `gate` leaves the locals as they were once the gate is false.
#[allow(clippy::too_many_arguments)]
pub(super) fn fold(lowerer: &mut Lowerer<'_>, span: Span, names: &LoopNames, over: Expr, body: &js::Statement, gate: Option<&js::Expression>, bind: impl FnOnce(&mut Lowerer<'_>) -> Lowered<()>, writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  for name in &names.written {
    if !crate::heap_pass() && !writable.local(lowerer, name) {
      return Err(lowerer.residue_with(span, format!("a write to `{name}` inside a loop, which this body did not declare"), "declare a local with `let` before the loop and write that"));
    }
  }
  let body: &[js::Statement] = match body {
    js::Statement::BlockStatement(block) => &block.body,
    other => std::slice::from_ref(other),
  };
  let body = iteration(lowerer, body)?;
  let cell_written = names.written.iter().any(|name| lowerer.cells.iter().any(|(cell, _)| cell == name));
  let kept;
  let names = if cell_written {
    kept = LoopNames { item: names.item.clone(), acc: names.acc.clone(), written: names.written.iter().filter(|name| !lowerer.cells.iter().any(|(cell, _)| cell == *name)).cloned().collect() };
    &kept
  } else {
    names
  };
  if (cell_written || crate::heap_pass()) && names.written.is_empty() && !lowerer.held.is_empty() {
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    lowerer.held.push(Vec::new());
    lowerer.held_reach.push(lowerer.rebind_reach.len());
    let mut test = None;
    let run = bind(lowerer).and_then(|_| {
      test = gate.map(|g| lowerer.expr(g)).transpose()?;
      body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner))
    });
    let held = lowerer.held.pop().expect("the fold's frame");
    lowerer.held_reach.pop();
    lowerer.scope.truncate(depth);
    run?;
    let stepped = sequenced(held, Expr::Var(names.acc.clone()));
    let stepped = match test {
      Some(test) => Expr::Ternary(Box::new(test), Box::new(stepped), Box::new(Expr::Var(names.acc.clone()))),
      None => stepped,
    };
    let step = Expr::Lambda { params: vec![names.acc.clone(), names.item.clone()], body: Box::new(stepped) };
    hold_in_reach(lowerer, Expr::Reduce(Box::new(over), Box::new(Expr::Lit(Lit::Null)), Box::new(step)));
    return Ok(());
  }
  if names.written.is_empty() {
    // Nothing outlives the loop, though a statement the build cannot run is still refused.
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    lowerer.held.push(Vec::new());
    lowerer.held_reach.push(lowerer.rebind_reach.len());
    let run = bind(lowerer).and_then(|_| body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner)));
    lowerer.held.pop();
    lowerer.held_reach.pop();
    lowerer.scope.truncate(depth);
    return run;
  }
  let before: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  let kinds: Vec<Option<crate::Kind>> = names.written.iter().map(|n| kind_held(lowerer, n)).collect();
  let depth = lowerer.scope.len();
  for (name, kind) in names.written.iter().zip(&kinds) {
    match kind {
      Some(kind) => rebind_kind(lowerer, name, names.read(name), *kind),
      None => lowerer.scope.push((name.clone(), names.read(name))),
    }
  }
  let mut inner = Vec::new();
  lowerer.held.push(Vec::new());
  lowerer.held_reach.push(lowerer.rebind_reach.len());
  let mut test = None;
  let run = bind(lowerer).and_then(|_| {
    test = gate.map(|g| lowerer.expr(g)).transpose()?;
    body.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner))
  });
  let after: Vec<Expr> = names.written.iter().map(|n| current(lowerer, n)).collect();
  let kinds: Vec<Option<crate::Kind>> = names.written.iter().zip(kinds).map(|(n, kind)| kind.filter(|_| kind_held(lowerer, n) == kind)).collect();
  let held = lowerer.held.pop().expect("the fold's frame");
  lowerer.held_reach.pop();
  lowerer.scope.truncate(depth);
  run?;
  let single = names.written.len() == 1;
  let gather = |values: Vec<Expr>| match single {
    true => values.into_iter().next().expect("one name"),
    false => Expr::Object(names.written.iter().cloned().zip(values).map(|(n, v)| Entry::Field(n, v)).collect()),
  };
  let stepped = sequenced(held, gather(after));
  let stepped = match test {
    Some(test) => Expr::Ternary(Box::new(test), Box::new(stepped), Box::new(Expr::Var(names.acc.clone()))),
    None => stepped,
  };
  let step = Expr::Lambda { params: vec![names.acc.clone(), names.item.clone()], body: Box::new(stepped) };
  let folded = Expr::Reduce(Box::new(over), Box::new(gather(before)), Box::new(step));
  let folded = if !lowerer.held.is_empty() { hold_in_reach(lowerer, folded) } else { folded };
  for (name, kind) in names.written.iter().zip(kinds) {
    let value = match single {
      true => folded.clone(),
      false => Expr::Field(Box::new(folded.clone()), name.clone()),
    };
    match kind {
      Some(kind) => rebind_kind(lowerer, name, value, kind),
      None => lowerer.scope.push((name.clone(), value)),
    }
    if !assigned.contains(name) {
      assigned.push(name.clone());
    }
  }
  Ok(())
}

/// One pass of a loop body with each `if (...) continue;` turned into a branch around the rest; a `break`, a `return` or a `continue` anywhere else is residue.
pub(super) fn iteration<'a>(lowerer: &Lowerer<'a>, stmts: &[js::Statement]) -> Lowered<Vec<js::Statement<'a>>> {
  let b = lowerer.build();
  let mut out = Vec::new();
  for (at, stmt) in stmts.iter().enumerate() {
    match stmt {
      js::Statement::ContinueStatement(c) if c.label.is_none() => return Ok(out),
      js::Statement::IfStatement(branch) if branch.alternate.is_none() && is_continue(&branch.consequent) => {
        let rest = iteration(lowerer, &stmts[at + 1..])?;
        let test = js::Expression::new_unary_expression(branch.test.span(), js::UnaryOperator::LogicalNot, b.copy(&branch.test), &b.ast());
        out.push(js::Statement::new_if_statement(branch.span, test, b.block(rest, branch.span), None, &b.ast()));
        return Ok(out);
      }
      other => {
        if let Some((span, what)) = leaves_loop(other) {
          return Err(lowerer.residue_with(span, format!("{what} inside a loop"), "a loop the build reads runs over every item; filter the items first or put `if (...) continue;` at the top of the loop"));
        }
        out.push(b.copy(other));
      }
    }
  }
  Ok(out)
}

pub(super) fn is_continue(stmt: &js::Statement) -> bool {
  match stmt {
    js::Statement::ContinueStatement(c) => c.label.is_none(),
    js::Statement::BlockStatement(block) => matches!(block.body.as_slice(), [js::Statement::ContinueStatement(c)] if c.label.is_none()),
    _ => false,
  }
}

/// The first `break`, `continue` or `return` in `stmt` that leaves the enclosing loop's iteration, outside a nested function, loop or (for `break`) `switch`.
pub(super) fn leaves_loop(stmt: &js::Statement) -> Option<(Span, &'static str)> {
  struct Leaves {
    found: Option<(Span, &'static str)>,
    switches: usize,
  }
  impl<'a> oxc_ast_visit::Visit<'a> for Leaves {
    fn visit_break_statement(&mut self, b: &js::BreakStatement<'a>) {
      if self.switches == 0 || b.label.is_some() {
        self.found.get_or_insert((b.span, "`break`"));
      }
    }
    fn visit_continue_statement(&mut self, c: &js::ContinueStatement<'a>) {
      self.found.get_or_insert((c.span, "`continue`"));
    }
    fn visit_return_statement(&mut self, r: &js::ReturnStatement<'a>) {
      self.found.get_or_insert((r.span, "`return`"));
    }
    fn visit_switch_statement(&mut self, sw: &js::SwitchStatement<'a>) {
      self.switches += 1;
      oxc_ast_visit::walk::walk_switch_statement(self, sw);
      self.switches -= 1;
    }
    fn visit_for_of_statement(&mut self, _: &js::ForOfStatement<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
  }
  let mut leaves = Leaves { found: None, switches: 0 };
  stmt.walk(&mut leaves);
  leaves.found
}

/// Arms tried in order, each a condition and its statements, then the statements when none holds: each name any arm wrote is rebound to the value of the arm that ran.
pub(super) fn rebind_arms(lowerer: &mut Lowerer<'_>, arms: Vec<(Option<Expr>, &[js::Statement])>, otherwise: &[js::Statement], writable: &Writable<'_>, assigned: &mut Vec<String>) -> Lowered<()> {
  let run = |lowerer: &mut Lowerer<'_>, stmts: &[js::Statement]| -> Lowered<Vec<(String, Expr)>> {
    let depth = lowerer.scope.len();
    let mut inner = Vec::new();
    let result = stmts.iter().try_for_each(|s| rebind_stmt(lowerer, s, writable, &mut inner));
    let values = inner.iter().map(|n| (n.clone(), current(lowerer, n))).collect();
    lowerer.scope.truncate(depth);
    result.map(|_| values)
  };
  let mut written: Vec<Vec<(String, Expr)>> = Vec::with_capacity(arms.len());
  let mut conds = Vec::with_capacity(arms.len());
  let reached = |lowerer: &mut Lowerer<'_>, cond: Expr, stmts: &[js::Statement]| {
    lowerer.rebind_reach.push(cond);
    let values = run(lowerer, stmts);
    lowerer.rebind_reach.pop();
    values
  };
  for (cond, stmts) in &arms {
    let cond = cond.clone().unwrap_or(Expr::Lit(Lit::Bool(false)));
    written.push(reached(lowerer, cond.clone(), stmts)?);
    conds.push(cond);
  }
  let none = conds.iter().cloned().reduce(|a, b| Expr::Logic(LogicOp::Or, Box::new(a), Box::new(b))).map(|any| Expr::Not(Box::new(any))).unwrap_or(Expr::Lit(Lit::Bool(true)));
  let fallback = reached(lowerer, none, otherwise)?;
  let mut names: Vec<String> = Vec::new();
  for values in written.iter().chain(std::iter::once(&fallback)) {
    for (name, _) in values {
      if !names.contains(name) {
        names.push(name.clone());
      }
    }
  }
  for name in names {
    let before = current(lowerer, &name);
    let pick = |values: &Vec<(String, Expr)>| values.iter().find(|(n, _)| *n == name).map(|(_, e)| e.clone()).unwrap_or_else(|| before.clone());
    let mut value = pick(&fallback);
    for (cond, values) in conds.iter().zip(&written).rev() {
      value = Expr::Ternary(Box::new(cond.clone()), Box::new(pick(values)), Box::new(value));
    }
    lowerer.scope.push((name.clone(), value));
    if !assigned.contains(&name) {
      assigned.push(name);
    }
  }
  Ok(())
}

pub(super) fn prop_name_of(member: &js::MemberExpression) -> Option<String> {
  match crate::syntax::prop(member) {
    crate::syntax::MemberProp::Ident(i) => Some(i.name.to_string()),
    crate::syntax::MemberProp::Computed(c) => match c {
      js::Expression::StringLiteral(s) => Some(crate::lossy(s.value)),
      _ => None,
    },
    _ => None,
  }
}
