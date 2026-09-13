use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::HostBuilder;
use snapfire_fsr_runtime::{ActionError, FailureKind};

use crate::session::{self, LOT_MAX, LOT_MIN};
use crate::state;

fn str_field(input: &Value, key: &str) -> String {
  match input {
    Value::Map(map) => match map.get(key) {
      Some(Value::Str(s)) => s.to_string(),
      _ => String::new(),
    },
    _ => String::new(),
  }
}

fn int_field(input: &Value, key: &str) -> i64 {
  match input {
    Value::Map(map) => match map.get(key) {
      Some(Value::Int(n)) => *n as i64,
      Some(Value::F64(f)) => *f as i64,
      _ => 0,
    },
    _ => 0,
  }
}

/// The desk's three actions, each writing the session both islands and the
/// lot stepper are rendered from, so the revalidation that follows patches
/// every one of them in place.
pub fn register(builder: HostBuilder) -> HostBuilder {
  builder
    .action("desk.watch", |ctx, input| async move {
      let symbol = str_field(&input, "symbol");
      if state::holding(&symbol).is_none() {
        return Err(ActionError::new(FailureKind::NotFound, format!("the desk does not hold `{symbol}`")));
      }
      ctx.session.insert("watched", Value::str(symbol.clone()));
      let mut answer = ValueMap::default();
      answer.insert("symbol".to_owned(), Value::str(symbol));
      Ok(Value::Map(answer))
    })
    .action("desk.lot", |ctx, input| async move {
      let next = (session::lot(&ctx) + int_field(&input, "by")).clamp(LOT_MIN, LOT_MAX);
      ctx.session.insert("lot", Value::Int(next as i128));
      let mut answer = ValueMap::default();
      answer.insert("lot".to_owned(), Value::Int(next as i128));
      Ok(Value::Map(answer))
    })
    .action("desk.buy", |ctx, input| async move {
      let symbol = str_field(&input, "symbol");
      if state::holding(&symbol).is_none() {
        return Err(ActionError::new(FailureKind::NotFound, format!("the desk does not hold `{symbol}`")));
      }
      let lot = session::lot(&ctx);
      let mut bought = match ctx.session.get("bought") {
        Some(Value::Map(map)) => map,
        _ => ValueMap::default(),
      };
      let held = session::bought(&ctx, &symbol);
      bought.insert(symbol.clone(), Value::Int((held + lot) as i128));
      ctx.session.insert("bought", Value::Map(bought));
      let mut answer = ValueMap::default();
      answer.insert("symbol".to_owned(), Value::str(symbol));
      answer.insert("shares".to_owned(), Value::Int((held + lot) as i128));
      Ok(Value::Map(answer))
    })
}
