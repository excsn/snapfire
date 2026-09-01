use snapfire_fsr::AppBuilder;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{ActionError, FailureKind, RequestCtx};

use crate::server::cart;
use crate::server::clients::shopping;

fn field(input: &Value, name: &str) -> Option<i128> {
  match input {
    Value::Map(map) => match map.get(name) {
      Some(Value::Int(n)) => Some(*n),
      _ => None,
    },
    _ => None,
  }
}

/// Actions answered in Rust. The plan file declares these ids, so an
/// application that later answers them in TypeScript claims the same names.
pub fn bind(builder: AppBuilder) -> AppBuilder {
  builder
    .action("add_to_cart", |ctx: RequestCtx, input: Value| async move {
      let product_id = field(&input, "product_id")
        .ok_or_else(|| ActionError::new(FailureKind::Invalid, "`product_id` must be an integer"))?;
      let quantity = field(&input, "quantity").unwrap_or(1);

      cart::add(&ctx.session, product_id, quantity);

      let mut out = ValueMap::new();
      out.insert("lines".to_owned(), Value::Map(cart::lines(&ctx.session)));
      Ok(Value::Map(out))
    })
    .action("checkout", |ctx: RequestCtx, _input: Value| async move {
      let lines = cart::order_lines(&ctx.session);
      if lines.is_empty() {
        return Err(ActionError::new(FailureKind::Invalid, "the cart is empty"));
      }

      let mut args = ValueMap::new();
      args.insert("lines".to_owned(), Value::Seq(lines));
      let order = ctx
        .services
        .call(shopping::NAME, "placeOrder", args)
        .await
        .map_err(|e| ActionError::new(e.kind, e.message))?;

      cart::clear(&ctx.session);
      Ok(order)
    })
}
