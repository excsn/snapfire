//! The constructors JavaScript lets a call make an instance of without `new`,
//! read as the `new` expressions they are, so the lowerer meets one shape of
//! each: `Intl.DateTimeFormat(...)`, `Intl.NumberFormat(...)`,
//! `Intl.Collator(...)`, `Object(...)`, `Array(...)` and `RegExp(...)`.
//! `Date(...)` is read as `new Date().toString()`, the string it answers
//! whatever it is given.

use oxc_allocator::Allocator;
use oxc_ast::ast as js;
use oxc_ast_visit::VisitMut;

use crate::scope::Scope;
use crate::syntax::Build;
use crate::Ctxt;

/// The `Intl` constructors ECMA-402 lets a call make an instance of without `new`. The others throw a `TypeError` when called.
const CALLABLE: &[&str] = &["DateTimeFormat", "NumberFormat", "Collator"];

/// The global constructors whose call is the `new` expression.
const GLOBAL: &[&str] = &["Object", "Array", "RegExp"];

/// Rewrites every call to a [`CALLABLE`] constructor of the global `Intl` or a [`GLOBAL`] one into a `new` expression, in place.
pub(crate) fn desugar<'a>(arena: &'a Allocator, scope: &Scope, module: &mut js::Program<'a>) {
  Desugar { scope, b: Build::new(arena) }.visit_program(module);
}

struct Desugar<'s, 'a> {
  scope: &'s Scope,
  b: Build<'a>,
}

impl<'a> VisitMut<'a> for Desugar<'_, 'a> {
  fn visit_expression(&mut self, expr: &mut js::Expression<'a>) {
    oxc_ast_visit::walk_mut::walk_expression(self, expr);
    let js::Expression::CallExpression(call) = expr else { return };
    if call.optional {
      return;
    }
    if matches!(&call.callee, js::Expression::Identifier(id) if id.name.as_str() == "Date" && self.scope.of(id) == Ctxt::Unresolved) {
      let ast = self.b.ast();
      let span = call.span;
      let made = js::Expression::new_new_expression(span, self.b.copy(&call.callee), None, self.b.vec([]), &ast);
      let member = js::Expression::new_static_member_expression(span, made, js::IdentifierName::new(span, self.b.str("toString"), &ast), false, &ast);
      *expr = js::Expression::new_call_expression(span, member, None, self.b.vec([]), false, &ast);
      return;
    }
    if !self.is_constructor_call(&call.callee) {
      return;
    }
    let ast = self.b.ast();
    let callee = self.b.copy(&call.callee);
    let arguments = self.b.copy(&call.arguments);
    *expr = js::Expression::new_new_expression(call.span, callee, None, arguments, &ast);
  }
}

impl Desugar<'_, '_> {
  fn is_constructor_call(&self, callee: &js::Expression) -> bool {
    if let js::Expression::Identifier(id) = callee {
      return GLOBAL.contains(&id.name.as_str()) && self.scope.of(id) == Ctxt::Unresolved;
    }
    let js::Expression::StaticMemberExpression(member) = callee else { return false };
    let js::Expression::Identifier(ns) = &member.object else { return false };
    ns.name.as_str() == "Intl" && self.scope.of(ns) == Ctxt::Unresolved && CALLABLE.contains(&member.property.name.as_str())
  }
}
