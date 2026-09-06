//! `id`: fresh identifiers. `id.new()` is a UUID version 7, time ordered.

use snapfire_fsr_core::Value;

use crate::ext::{Extensions, Reach};

pub fn register(extensions: &mut Extensions) {
  extensions.register("id.new", Reach::Body, |_, _| Ok(Value::Str(uuid::Uuid::now_v7().to_string())));
}
