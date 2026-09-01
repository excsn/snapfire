use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::SessionCell;

pub const KEY: &str = "cart";

/// The cart is application data, so it lives in the session cell rather than
/// in token custody. Lines are keyed by product id and hold a quantity.
pub fn lines(session: &SessionCell) -> ValueMap {
  match session.get(KEY) {
    Some(Value::Map(map)) => map,
    _ => ValueMap::new(),
  }
}

pub fn add(session: &SessionCell, product_id: i128, quantity: i128) {
  let mut cart = lines(session);
  let key = product_id.to_string();
  let held = match cart.get(&key) {
    Some(Value::Int(n)) => *n,
    _ => 0,
  };
  let wanted = (held + quantity).max(0);
  if wanted == 0 {
    cart.shift_remove(&key);
  } else {
    cart.insert(key, Value::Int(wanted));
  }
  session.insert(KEY, Value::Map(cart));
}

pub fn clear(session: &SessionCell) {
  session.insert(KEY, Value::Map(ValueMap::new()));
}

/// The order lines the shopping service wants, built from what the cart holds.
pub fn order_lines(session: &SessionCell) -> Vec<Value> {
  lines(session)
    .into_iter()
    .filter_map(|(id, quantity)| {
      let product_id = id.parse::<i128>().ok()?;
      let quantity = match quantity {
        Value::Int(n) => n,
        _ => return None,
      };
      let mut line = ValueMap::new();
      line.insert("product_id".to_owned(), Value::Int(product_id));
      line.insert("quantity".to_owned(), Value::Int(quantity));
      Some(Value::Map(line))
    })
    .collect()
}
