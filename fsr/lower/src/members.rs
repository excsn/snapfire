use super::*;

/// The object at the root of a chain of members: `a` of `a.b[c].d`.
pub(crate) fn member_root<'e>(member: &'e js::MemberExpression<'e>) -> &'e js::Expression<'e> {
  let mut root = member.object();
  while let Some(inner) = root.as_member_expression() {
    root = inner.object();
  }
  root
}

/// Whether `stmts` write a field of one of `params`, an object the caller holds too, so the parameters stay unwritable; rebinding a parameter itself is a local write.
pub(crate) fn writes_a_member_of(stmts: &[js::Statement], params: &[String]) -> bool {
  if heap_pass() {
    return false;
  }
  struct Writes<'p>(&'p [String], bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Writes<'_> {
    fn visit_simple_assignment_target(&mut self, target: &js::SimpleAssignmentTarget<'a>) {
      if let Some(member) = target.as_member_expression() {
        if matches!(member_root(member), js::Expression::Identifier(id) if self.0.iter().any(|p| p == id.name.as_str())) {
          self.1 = true;
        }
      }
      oxc_ast_visit::walk::walk_simple_assignment_target(self, target);
    }
  }
  let mut writes = Writes(params, false);
  stmts.iter().for_each(|stmt| oxc_ast_visit::Visit::visit_statement(&mut writes, stmt));
  writes.1
}

/// A `reduce` callback that writes its accumulator and answers it, `(acc[k] = v, acc)` or `{ acc.k = v; return acc; }`, as one that answers `({ ...acc, [k]: v })`. Only for an accumulator that starts as an object literal, which nothing else holds, so a fresh object answers what the writes would; only the first write may read the accumulator, since a later one would see the earlier write.
pub(crate) fn fresh_accumulator<'a>(b: syntax::Build<'a>, arrow: &js::ArrowFunctionExpression<'_>) -> Option<js::ArrowFunctionExpression<'a>> {
  let acc = first_param(&arrow.params)?.ident()?.name.as_str();
  let mut writes: Vec<&js::AssignmentExpression> = Vec::new();
  let answers_acc = |e: &js::Expression| matches!(strip_parens(e), js::Expression::Identifier(id) if id.name.as_str() == acc);
  match arrow_body(arrow) {
    FunctionBody::Expr(e) => {
      let js::Expression::SequenceExpression(seq) = strip_parens(e) else { return None };
      let (last, rest) = seq.expressions.split_last()?;
      if !answers_acc(last) {
        return None;
      }
      for e in rest {
        let js::Expression::AssignmentExpression(assign) = strip_parens(e) else { return None };
        writes.push(assign);
      }
    }
    FunctionBody::Block(stmts) => {
      let (last, rest) = stmts.split_last()?;
      let js::Statement::ReturnStatement(ret) = last else { return None };
      if !ret.argument.as_ref().is_some_and(|arg| answers_acc(arg)) {
        return None;
      }
      for stmt in rest {
        let js::Statement::ExpressionStatement(stmt) = stmt else { return None };
        let js::Expression::AssignmentExpression(assign) = strip_parens(&stmt.expression) else { return None };
        writes.push(assign);
      }
    }
  }
  struct Reads<'n>(&'n str, bool);
  impl<'a> oxc_ast_visit::Visit<'a> for Reads<'_> {
    fn visit_identifier_reference(&mut self, id: &js::IdentifierReference<'a>) {
      if id.name.as_str() == self.0 {
        self.1 = true;
      }
    }
    fn visit_binding_identifier(&mut self, id: &js::BindingIdentifier<'a>) {
      if id.name.as_str() == self.0 {
        self.1 = true;
      }
    }
  }
  let ast = b.ast();
  let mut props = vec![js::ObjectPropertyKind::new_spread_property(arrow.span, b.ident(acc, arrow.span), &ast)];
  for (i, assign) in writes.iter().enumerate() {
    if assign.operator != js::AssignmentOperator::Assign {
      return None;
    }
    let Some(member) = assign.left.as_member_expression() else { return None };
    if !matches!(member.object(), js::Expression::Identifier(id) if id.name.as_str() == acc) {
      return None;
    }
    let (key, computed) = match syntax::prop(member) {
      syntax::MemberProp::Ident(name) => (js::PropertyKey::StaticIdentifier(b.boxed(b.copy(name))), false),
      syntax::MemberProp::Computed(computed) => (js::PropertyKey::from(b.copy(computed)), true),
      syntax::MemberProp::Private(_) => return None,
    };
    if i > 0 {
      let mut reads = Reads(acc, false);
      oxc_ast_visit::Visit::visit_expression(&mut reads, &assign.right);
      if let syntax::MemberProp::Computed(computed) = syntax::prop(member) {
        oxc_ast_visit::Visit::visit_expression(&mut reads, computed);
      }
      if reads.1 {
        return None;
      }
    }
    props.push(js::ObjectPropertyKind::new_object_property(assign.span, js::PropertyKind::Init, key, b.copy(&assign.right), false, false, computed, &ast));
  }
  let object = js::Expression::new_object_expression(arrow.span, b.vec(props), &ast);
  let mut made = b.copy(arrow);
  made.body = js::ArrowFunctionBody::from(b.paren(object, arrow.span));
  Some(made)
}

/// The name a property key spells: an identifier, a string or a number written out, `None` for one computed.
pub(crate) fn prop_name(key: &js::PropertyKey, computed: bool) -> Option<String> {
  if computed {
    return None;
  }
  match key {
    js::PropertyKey::StaticIdentifier(id) => Some(id.name.to_string()),
    js::PropertyKey::StringLiteral(s) => Some(lossy(s.value)),
    js::PropertyKey::NumericLiteral(n) => Some(snapfire_fsr_ir::interp::number_key(n.value)),
    _ => None,
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
pub(crate) fn describe_stmt(stmt: &js::Statement) -> (&'static str, &'static str) {
  match stmt {
    js::Statement::WhileStatement(_) | js::Statement::DoWhileStatement(_) => ("`while`, a loop whose length the build cannot know", "a body loops over data it already has: `map`, `filter`, `reduce`, `find` and `for...of`"),
    js::Statement::ForStatement(_) | js::Statement::ForInStatement(_) => ("a `for` loop other than `for...of`", "`for...of` over a list, or `map` and `filter`, which the interpreter runs directly"),
    js::Statement::SwitchStatement(_) => ("`switch`", "a chain of `if`, or a ternary when every arm is an expression"),
    js::Statement::FunctionDeclaration(_) => ("a function declared inside a body", "declare it at module level and call it; the build follows a module-level helper"),
    js::Statement::ClassDeclaration(_) => ("a class outside module level", "declare the class at module level or use an object literal"),
    js::Statement::BreakStatement(_) | js::Statement::ContinueStatement(_) => ("`break` or `continue`", "`find` stops at the first match and `filter` selects, so neither needs to leave a loop early"),
    js::Statement::LabeledStatement(_) => ("a label", "a `for...of` in a body runs over every item; `find` and `filter` express the same intent"),
    js::Statement::WithStatement(_) => ("`with`", "read what the body needs from `ctx` by name"),
    js::Statement::DebuggerStatement(_) => ("`debugger`", "the body runs in Rust, where there is nothing to attach to; `fsr test` replays it instead"),
    js::Statement::EmptyStatement(_) => ("an empty statement", "remove the stray `;`"),
    js::Statement::BlockStatement(_) | js::Statement::IfStatement(_) | js::Statement::ForOfStatement(_) | js::Statement::ReturnStatement(_) | js::Statement::ExpressionStatement(_) | js::Statement::ThrowStatement(_) | js::Statement::TryStatement(_) => unreachable!("`stmt` lowers these itself"),
    _ => ("a declaration the build does not read", "a body declares values with `const` or `let`"),
  }
}

/// What the construct is and the rewrite that does the same thing in the IR.
pub(crate) fn describe_expr(expr: &js::Expression) -> (&'static str, &'static str) {
  match expr {
    js::Expression::ThisExpression(_) => ("`this`, and a body has no receiver", "read what the body needs from `ctx`"),
    js::Expression::ClassExpression(_) => ("a class outside module level", "declare the class at module level or use an object literal"),
    js::Expression::SequenceExpression(_) => ("a comma expression", "one expression per statement"),
    js::Expression::UpdateExpression(_) => ("`++` or `--` on something this body did not declare", "declare a local with `let` and write that"),
    js::Expression::YieldExpression(_) => ("`yield`", "a body runs once and returns; there is no generator in the serving path"),
    js::Expression::TaggedTemplateExpression(_) => ("a tagged template", "a plain template literal, or a module-level helper the build can follow"),
    js::Expression::JSXElement(_) | js::Expression::JSXFragment(_) => ("JSX in a body", "a body returns data; the page it feeds is where the markup goes"),
    js::Expression::ImportMeta(_) | js::Expression::NewTarget(_) => ("`import.meta`", "the build resolves every import, so nothing reads them while serving"),
    _ => ("an expression outside the IR", "section 2 of snapfire_fsr_lower's API reference lists what a body may say"),
  }
}
