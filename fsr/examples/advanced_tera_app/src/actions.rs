use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{ActionError, FailureKind, ActionRegistry};

use crate::state::Fleet;

pub fn register(actions: &mut ActionRegistry, fleet: Fleet) {
  actions.insert_fn("add_server", move |_ctx, input| {
    let fleet = fleet.clone();
    async move {
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
      let count = fleet
        .add(name.clone(), load)
        .map_err(|_| ActionError::new(FailureKind::Conflict, format!("server `{name}` already exists")))?;
      let mut out = ValueMap::new();
      out.insert("count".to_owned(), Value::int(count as i64));
      Ok(Value::Map(out))
    }
  });
}
