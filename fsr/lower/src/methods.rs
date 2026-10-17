//! A method in an object literal read as the function-valued field it is,
//! so the lowerer meets one shape of function in an object.

use swc_core::ecma::ast as js;
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

/// Rewrites every `key() {}` and `async key() {}` in an object literal into `key: function () {}`, in place. A getter or a setter stays what it is.
pub(crate) fn desugar(module: &mut js::Module) {
  module.visit_mut_with(&mut Methods);
}

struct Methods;

impl VisitMut for Methods {
  fn visit_mut_prop(&mut self, prop: &mut js::Prop) {
    if let js::Prop::Method(method) = prop {
      let key = method.key.clone();
      let function = method.function.clone();
      *prop = js::Prop::KeyValue(js::KeyValueProp { key, value: Box::new(js::Expr::Fn(js::FnExpr { ident: None, function })) });
    }
    prop.visit_mut_children_with(self);
  }
}
