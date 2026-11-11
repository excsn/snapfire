//! `Intl.DateTimeFormat(...)`, `Intl.NumberFormat(...)` and `Intl.Collator(...)`
//! called without `new` read as the `new` expressions they are, so the lowerer
//! meets one shape of formatter.

use oxc_allocator::Allocator;
use oxc_ast::ast as js;
use oxc_ast_visit::VisitMut;

use crate::scope::Scope;
use crate::syntax::Build;
use crate::Ctxt;

/// The `Intl` constructors ECMA-402 lets a call make an instance of without `new`. The others throw a `TypeError` when called.
const CALLABLE: &[&str] = &["DateTimeFormat", "NumberFormat", "Collator"];

/// Rewrites every call to a [`CALLABLE`] constructor of the global `Intl` into a `new` expression, in place.
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
    if call.optional || !self.is_callable_formatter(&call.callee) {
      return;
    }
    let ast = self.b.ast();
    let callee = self.b.copy(&call.callee);
    let arguments = self.b.copy(&call.arguments);
    *expr = js::Expression::new_new_expression(call.span, callee, None, arguments, &ast);
  }
}

impl Desugar<'_, '_> {
  fn is_callable_formatter(&self, callee: &js::Expression) -> bool {
    let js::Expression::StaticMemberExpression(member) = callee else { return false };
    let js::Expression::Identifier(ns) = &member.object else { return false };
    ns.name.as_str() == "Intl" && self.scope.of(ns) == Ctxt::Unresolved && CALLABLE.contains(&member.property.name.as_str())
  }
}
