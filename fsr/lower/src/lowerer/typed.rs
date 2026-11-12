use crate::*;

/// The methods of a typed array that answer a typed array of its element type.
const TYPED_RESULTS: &[&str] = &["map", "filter", "slice", "subarray", "toReversed", "toSorted", "with"];

impl Lowerer<'_> {
  /// `array.<method>(...)` of a typed array: the method an array has, called on its numbers, the answer a typed array of the receiver's element type where JavaScript's is one. `subarray` copies, as a typed array the build holds owns its items. `toSorted` with no comparator orders the numbers, not their text.
  pub(crate) fn typed_call(&mut self, method: &str, member: &js::MemberExpression, call: &js::CallExpression) -> Lowered<Expr> {
    let receiver = self.expr(member.object())?;
    let held = self.temp();
    let b = self.build();
    let ast = b.ast();
    let span = member.span();
    let name = if method == "subarray" { "slice" } else { method };
    let items = format!("{held}items");
    let callee = js::Expression::new_static_member_expression(span, b.ident(&items, span), js::IdentifierName::new(span, b.str(name), &ast), false, &ast);
    let args = b.vec(call.arguments.iter().map(|a| b.copy(a)));
    if method == "toSorted" && call.arguments.is_empty() {
      let (x, y) = (self.temp(), self.temp());
      let numeric = Expr::Lambda { params: vec![x.clone(), y.clone()], body: Box::new(Expr::Arith(ArithOp::Sub, Box::new(Expr::Var(x)), Box::new(Expr::Var(y)))) };
      let sorted = Expr::Sort(Box::new(Expr::Var(held.clone())), Box::new(numeric));
      return Ok(Expr::Block(vec![Stmt::Let { name: held.clone(), expr: receiver }, Stmt::Return(Expr::Builtin { name: Builtin::TypedLike, args: vec![Expr::Var(held), sorted] })]));
    }
    let called = js::Expression::new_call_expression(call.span, callee, None, args, false, &ast);
    self.scope.push((items, Expr::Var(held.clone())));
    let lowered = self.expr(b.alloc(called));
    self.scope.pop();
    let answer = if TYPED_RESULTS.contains(&method) { Expr::Builtin { name: Builtin::TypedLike, args: vec![Expr::Var(held.clone()), lowered?] } } else { lowered? };
    Ok(Expr::Block(vec![Stmt::Let { name: held, expr: receiver }, Stmt::Return(answer)]))
  }
}
