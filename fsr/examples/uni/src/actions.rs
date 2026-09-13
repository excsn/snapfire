use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_host::HostBuilder;
use snapfire_fsr_runtime::{ActionError, FailureKind};

use crate::state;

/// Ten more shares of one symbol, kept in the session. Both islands are
/// rendered from it, so the payload that follows patches each in place.
pub fn register(builder: HostBuilder) -> HostBuilder {
  builder.action("desk.buy", |ctx, input| async move {
    let symbol = match &input {
      Value::Map(map) => match map.get("symbol") {
        Some(Value::Str(symbol)) => symbol.to_string(),
        _ => String::new(),
      },
      _ => String::new(),
    };
    if state::holding(&symbol).is_none() {
      return Err(ActionError::new(FailureKind::NotFound, format!("the desk does not hold `{symbol}`")));
    }
    let mut bought = match ctx.session.get("bought") {
      Some(Value::Map(map)) => map,
      _ => ValueMap::default(),
    };
    let held = match bought.get(&symbol) {
      Some(Value::Int(n)) => *n,
      _ => 0,
    };
    bought.insert(symbol.clone(), Value::Int(held + 10));
    ctx.session.insert("bought", Value::Map(bought));
    let mut answer = ValueMap::default();
    answer.insert("symbol".to_owned(), Value::str(symbol));
    answer.insert("shares".to_owned(), Value::Int(held + 10));
    Ok(Value::Map(answer))
  })
}
