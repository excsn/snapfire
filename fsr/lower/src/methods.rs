//! A method in an object literal read as the function-valued field it is,
//! so the lowerer meets one shape of function in an object.

use oxc_ast::ast as js;
use oxc_ast_visit::VisitMut;

/// Rewrites every `key() {}` and `async key() {}` in an object literal into `key: function () {}`, in place. A getter or a setter stays what it is.
pub(crate) fn desugar(module: &mut js::Program<'_>) {
  Methods.visit_program(module);
}

struct Methods;

impl<'a> VisitMut<'a> for Methods {
  fn visit_object_property(&mut self, prop: &mut js::ObjectProperty<'a>) {
    if prop.method && prop.kind == js::PropertyKind::Init {
      prop.method = false;
    }
    oxc_ast_visit::walk_mut::walk_object_property(self, prop);
  }
}
