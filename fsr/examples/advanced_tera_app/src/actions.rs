use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{ActionError, ActionRegistry, FailureKind};

use crate::services::FLEET;

pub fn register(actions: &mut ActionRegistry) {
  actions.insert_fn("add_server", move |ctx, input| async move {
    let Value::Map(fields) = input else {
      return Err(ActionError::new(FailureKind::Invalid, "input must be a map"));
    };
    let name = match fields.get("name") {
      Some(Value::Str(name)) if !name.is_empty() => name.clone(),
      _ => return Err(ActionError::new(FailureKind::Invalid, "`name` must be a non-empty string")),
    };
    let load = match fields.get("load") {
      Some(Value::F64(load)) => *load,
      Some(Value::Int(load)) => *load as f64,
      Some(Value::Str(raw)) => raw
        .parse()
        .map_err(|_| ActionError::new(FailureKind::Invalid, "`load` must be a number"))?,
      _ => return Err(ActionError::new(FailureKind::Invalid, "`load` must be a number")),
    };

    let mut args = ValueMap::new();
    args.insert("name".to_owned(), Value::Str(name));
    args.insert("load".to_owned(), Value::F64(load));
    ctx
      .services
      .call(FLEET, "add", args)
      .await
      .map_err(|e| ActionError::new(e.kind, e.message))
  });
}
