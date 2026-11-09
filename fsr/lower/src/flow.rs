use super::*;

/// The local a member write changes and the members from it outward: `x.a[i].b` is `x` with `.a`, `[i]` and `.b`.
pub(crate) fn write_root<'e>(member: &'e js::MemberExpression<'e>) -> Option<(&'e js::IdentifierReference<'e>, Vec<&'e js::MemberExpression<'e>>)> {
  let mut path = vec![member];
  let mut at = member;
  loop {
    let object = crate::meaning::bare(at.object());
    match object {
      js::Expression::Identifier(id) => {
        path.reverse();
        return Some((id, path));
      }
      other => match other.as_member_expression() {
        Some(inner) => {
          path.push(inner);
          at = inner;
        }
        None => return None,
      },
    }
  }
}

/// `first`'s statements, when it is a block, followed by `rest` in one block, so a `let` among them stays bound for what follows; a nested block would end its `let`s with it.
pub(crate) fn spliced(first: Expr, rest: Vec<Stmt>) -> Expr {
  let mut out = Vec::with_capacity(rest.len() + 1);
  splice(first, &mut out);
  out.extend(rest);
  Expr::Block(out)
}

pub(crate) fn splice(expr: Expr, out: &mut Vec<Stmt>) {
  match expr {
    Expr::Block(stmts) => out.extend(stmts),
    other => out.push(Stmt::Expr(other)),
  }
}

/// Where `span` starts as a source-map position: one past the byte offset, the first byte of the module being position 1.
pub(crate) fn lo(span: Span) -> u32 {
  span.start + 1
}

/// Where `span` ends as a source-map position, as [`lo`] counts.
pub(crate) fn hi(span: Span) -> u32 {
  span.end + 1
}

/// The statements a loop or branch body holds: a block's own, or the one statement.
pub(crate) fn body_stmts<'a>(b: syntax::Build<'a>, body: &js::Statement<'_>) -> Vec<js::Statement<'a>> {
  match body {
    js::Statement::BlockStatement(block) => block.body.iter().map(|s| b.copy(s)).collect(),
    other => vec![b.copy(other)],
  }
}

/// `for...of` over an array, a counted `for` or a `while` that awaits in its body, as an [`AwaitingLoop`]: `for (const x of xs)` becomes `const $items = xs; let $i = 0;` with the test `$i < $items.length`, a pass starting `const x = $items[$i]` and ending `$i++`.
pub(crate) fn awaiting_loop<'a>(parsed: &'a Parsed<'a>, stmt: &js::Statement<'_>) -> Option<AwaitingLoop<'a>> {
  if !awaits(stmt) {
    return None;
  }
  let b = syntax::Build::new(parsed.arena());
  let ast = b.ast();
  let span = stmt.span();
  let ident = |name: &str| {
    let made = b.ident(name, span);
    if let js::Expression::Identifier(id) = &made {
      parsed.scope.mark_top(id);
    }
    made
  };
  let binding = |name: &str| {
    let made = b.binding(name, span);
    if let js::BindingPattern::BindingIdentifier(id) = &made {
      parsed.scope.mark_top_binding(id);
    }
    made
  };
  let decl = |kind: js::VariableDeclarationKind, name: js::BindingPattern<'a>, init: js::Expression<'a>| {
    js::Statement::new_variable_declaration(span, kind, b.vec([js::VariableDeclarator::new(span, name, None, Some(init), false, &ast)]), false, &ast)
  };
  match stmt {
    js::Statement::ForOfStatement(for_of) => {
      let js::ForStatementLeft::VariableDeclaration(left) = &for_of.left else { return None };
      let first = left.declarations.first()?;
      let pattern = b.copy(&first.id);
      let (items, index) = (format!("$items{}", lo(span)), format!("$i{}", lo(span)));
      let init = vec![
        decl(js::VariableDeclarationKind::Const, binding(&items), b.copy(&for_of.right)),
        decl(js::VariableDeclarationKind::Let, binding(&index), js::Expression::new_numeric_literal(span, 0.0, None, oxc_syntax::number::NumberBase::Decimal, &ast)),
      ];
      let length = js::Expression::new_static_member_expression(span, ident(&items), js::IdentifierName::new(span, b.str("length"), &ast), false, &ast);
      let test = js::Expression::new_binary_expression(span, ident(&index), js::BinaryOperator::LessThan, length, &ast);
      let item = js::Expression::new_computed_member_expression(span, ident(&items), ident(&index), false, &ast);
      let mut body = vec![decl(js::VariableDeclarationKind::Const, pattern, item)];
      let inner = body_stmts(b, &for_of.body);
      let exits_deep = exits_deeper(&inner);
      body.extend(inner);
      let target = js::SimpleAssignmentTarget::new_assignment_target_identifier(span, b.str(&index), &ast);
      if let js::SimpleAssignmentTarget::AssignmentTargetIdentifier(id) = &target {
        parsed.scope.mark_top(id);
      }
      let step = vec![b.expr_stmt(js::Expression::new_update_expression(span, js::UpdateOperator::Increment, false, target, &ast))];
      Some(AwaitingLoop { init, test, pass: continued(b, body, &step), over: Some(b.copy(&for_of.right)), span, exits_deep })
    }
    js::Statement::ForStatement(counted) => {
      let init = match &counted.init {
        Some(js::ForStatementInit::VariableDeclaration(d)) => vec![js::Statement::VariableDeclaration(b.copy(d))],
        Some(other) => vec![b.expr_stmt(b.copy(other.to_expression()))],
        None => Vec::new(),
      };
      let test = counted.test.as_ref().map(|t| b.copy(t)).unwrap_or_else(|| b.boolean(true, span));
      let step: Vec<js::Statement<'a>> = counted.update.iter().map(|update| b.expr_stmt(b.copy(update))).collect();
      let body = body_stmts(b, &counted.body);
      Some(AwaitingLoop { init, test, exits_deep: exits_deeper(&body), pass: continued(b, body, &step), over: None, span })
    }
    js::Statement::WhileStatement(looped) => {
      let body = body_stmts(b, &looped.body);
      Some(AwaitingLoop { init: Vec::new(), test: b.copy(&looped.test), exits_deep: exits_deeper(&body), pass: continued(b, body, &[]), over: None, span })
    }
    _ => None,
  }
}

/// `body` with `step` at the end of every way through it, a `continue` or `return` at the body's top or ending an `if` there taken as that way's end: `if (c) continue; rest` becomes `if (c) { step } else { rest; step }` and `if (c) return v; rest` becomes `if (c) { return v } else { rest; step }`.
pub(crate) fn continued<'a>(b: syntax::Build<'a>, body: Vec<js::Statement<'a>>, step: &[js::Statement<'a>]) -> Vec<js::Statement<'a>> {
  let mut out = Vec::with_capacity(body.len() + step.len());
  let mut stmts = body.into_iter();
  while let Some(stmt) = stmts.next() {
    match stmt {
      js::Statement::ContinueStatement(_) => {
        out.extend(step.iter().map(|s| b.copy(s)));
        return out;
      }
      ret @ js::Statement::ReturnStatement(_) => {
        out.push(ret);
        return out;
      }
      js::Statement::IfStatement(branch) if branch.alternate.is_none() && (ends_in(&branch.consequent, Exit::Continue) || ends_in(&branch.consequent, Exit::Return)) => {
        let branch = branch.unbox();
        let span = branch.span;
        let returns = ends_in(&branch.consequent, Exit::Return);
        let mut taken: Vec<js::Statement<'a>> = match branch.consequent {
          js::Statement::BlockStatement(block) => block.unbox().body.into_iter().collect(),
          other => vec![other],
        };
        if !returns {
          taken.pop();
          taken.extend(step.iter().map(|s| b.copy(s)));
        }
        let rest = continued(b, stmts.collect(), step);
        out.push(js::Statement::new_if_statement(span, branch.test, b.block(taken, span), Some(b.block(rest, span)), &b.ast()));
        return out;
      }
      other => out.push(other),
    }
  }
  out.extend(step.iter().map(|s| b.copy(s)));
  out
}

/// Whether `stmt` is the exit or a block ending in it.
pub(crate) fn ends_in(stmt: &js::Statement, exit: Exit) -> bool {
  let is = |s: &js::Statement| match exit {
    Exit::Continue => matches!(s, js::Statement::ContinueStatement(_)),
    Exit::Return => matches!(s, js::Statement::ReturnStatement(_)),
  };
  match stmt {
    js::Statement::BlockStatement(block) => block.body.last().is_some_and(is),
    other => is(other),
  }
}

/// Whether an `if` ends either branch in a `return`, which splits a pass of a loop that awaits as an `await` does.
pub(crate) fn if_ends_in_return(stmt: &js::Statement) -> bool {
  matches!(stmt, js::Statement::IfStatement(branch) if ends_in(&branch.consequent, Exit::Return) || branch.alternate.as_ref().is_some_and(|alt| ends_in(alt, Exit::Return)))
}

/// Whether `body` leaves the loop from somewhere a pass cannot end: a `break` anywhere or a `continue` or `return` other than at the body's top or ending an `if` there.
pub(crate) fn exits_deeper(body: &[js::Statement]) -> bool {
  body.iter().any(|stmt| match stmt {
    js::Statement::ContinueStatement(_) | js::Statement::ReturnStatement(_) => false,
    js::Statement::IfStatement(branch) if branch.alternate.is_none() && (ends_in(&branch.consequent, Exit::Continue) || ends_in(&branch.consequent, Exit::Return)) => match &branch.consequent {
      js::Statement::BlockStatement(block) => block.body[..block.body.len() - 1].iter().any(loop_exits),
      _ => false,
    },
    other => loop_exits(other),
  })
}

/// Whether `stmt` leaves the loop around it with `break`, `continue` or `return`, outside any function it declares.
pub(crate) fn loop_exits(stmt: &js::Statement) -> bool {
  struct Exits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Exits {
    fn visit_break_statement(&mut self, _: &js::BreakStatement<'a>) {
      self.0 = true;
    }
    fn visit_continue_statement(&mut self, _: &js::ContinueStatement<'a>) {
      self.0 = true;
    }
    fn visit_return_statement(&mut self, _: &js::ReturnStatement<'a>) {
      self.0 = true;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Exits(false);
  oxc_ast_visit::Visit::visit_statement(&mut found, stmt);
  found.0
}

pub(crate) fn awaits(stmt: &js::Statement) -> bool {
  struct Awaits(bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Awaits {
    fn visit_await_expression(&mut self, _: &js::AwaitExpression<'a>) {
      self.0 = true;
    }
    fn visit_arrow_function_expression(&mut self, _: &js::ArrowFunctionExpression<'a>) {}
    fn visit_function(&mut self, _: &js::Function<'a>, _: oxc_syntax::scope::ScopeFlags) {}
  }
  let mut found = Awaits(false);
  oxc_ast_visit::Visit::visit_statement(&mut found, stmt);
  found.0
}

/// Whether `e` builds a new array, which `sort` and `reverse` may rewrite without another holder seeing it.
pub(crate) fn fresh_array(e: &js::Expression) -> bool {
  match e {
    js::Expression::ArrayExpression(_) => true,
    js::Expression::ParenthesizedExpression(p) => fresh_array(&p.expression),
    js::Expression::CallExpression(call) => match &call.callee {
      js::Expression::StaticMemberExpression(m) => matches!(m.property.name.as_str(), "slice" | "map" | "filter" | "concat" | "toSorted" | "toReversed" | "flat" | "flatMap" | "split" | "keys" | "values" | "entries" | "from"),
      _ => false,
    },
    _ => false,
  }
}

pub(crate) fn root_named(name: &str) -> Option<Root> {
  Some(match name {
    "params" => Root::Params,
    "query" => Root::Query,
    "session" => Root::Session,
    "services" => Root::Services,
    "native" => Root::Native,
    "identity" => Root::Identity,
    "locale" => Root::Locale,
    "locales" => Root::Locales,
    "path" => Root::Path,
    "host" => Root::Host,
    "origin" => Root::Origin,
    "address" => Root::Address,
    "config" => Root::Config,
    "input" => Root::Input,
    "now" => Root::Now,
    _ => return None,
  })
}
