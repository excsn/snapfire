use snapfire_fsr::AppBuilder;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{LoadError, RequestCtx};

use crate::server::cart;
use crate::server::clients::shopping;

async fn call(ctx: &RequestCtx, method: &str, args: ValueMap, source: &str) -> Result<Value, LoadError> {
  ctx.services.call(shopping::NAME, method, args).await.map_err(|e| LoadError {
    source_id: source.to_owned(),
    message: e.message,
  })
}

/// Every name the plan file declares, answered in Rust. A TypeScript loader
/// would claim the same names.
pub fn bind(builder: AppBuilder) -> AppBuilder {
  builder
    .source("catalog_loader", |ctx: RequestCtx| async move {
    let mut args = ValueMap::new();
    if let Some(tag) = ctx.params.get("tag") {
      args.insert("tag".to_owned(), Value::Str(tag.clone()));
    }
    let products = call(&ctx, "listProducts", args, "catalog_loader").await?;

      let mut data = ValueMap::new();
      data.insert("products".to_owned(), products);
      Ok(data)
    })
    .source("product_loader", |ctx: RequestCtx| async move {
    let id = ctx
      .params
      .get("id")
      .and_then(|id| id.parse::<i64>().ok())
      .ok_or_else(|| LoadError {
        source_id: "product_loader".to_owned(),
        message: "the product id is not a number".to_owned(),
      })?;

    let mut args = ValueMap::new();
    args.insert("id".to_owned(), Value::int(id));
    let product = call(&ctx, "getProduct", args, "product_loader").await?;

      let mut data = ValueMap::new();
      data.insert("product".to_owned(), product);
      Ok(data)
    })
    .source("cart_loader", |ctx: RequestCtx| async move {
      let held = cart::lines(&ctx.session);
      let mut args = ValueMap::new();
      let catalog = call(&ctx, "listProducts", args.split_off(0), "cart_loader").await?;

      // The cart holds ids and quantities; the names and prices come from the
      // catalog, so the component never has to ask for them.
      let mut lines = Vec::new();
      if let Value::Seq(products) = &catalog {
        for product in products {
          let Value::Map(fields) = product else { continue };
          let Some(Value::Int(id)) = fields.get("id") else { continue };
          let Some(Value::Int(quantity)) = held.get(&id.to_string()) else { continue };
          let mut line = fields.clone();
          line.insert("quantity".to_owned(), Value::Int(*quantity));
          lines.push(Value::Map(line));
        }
      }

      let mut data = ValueMap::new();
      data.insert("lines".to_owned(), Value::Seq(lines));
      Ok(data)
    })
}
