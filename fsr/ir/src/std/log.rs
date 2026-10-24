//! `log`: a structured line through `tracing` under the target `fsr::app`, one member per level, `(message, fields?)`, answering null. `fatal` writes at the error level, since `tracing` has none above it.

use crate::ext::{text, Extensions, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::Value;

pub const TARGET: &str = "fsr::app";

pub const LEVELS: [&str; 6] = ["trace", "debug", "info", "warn", "error", "fatal"];

pub fn register(extensions: &mut Extensions) {
  for level in LEVELS {
    extensions.register(&format!("log.{level}"), Reach::Render, move |_, args| write(level, args));
  }
}

fn write(level: &str, args: &[Value]) -> Result<Value, Fail> {
  let message = text(&format!("log.{level}"), args, 0)?.to_owned();
  let fields = args.get(1).filter(|value| !matches!(value, Value::Null | Value::Undefined)).map(crate::interp::console_text);
  macro_rules! line {
    ($level:ident) => {
      match &fields {
        Some(fields) => tracing::$level!(target: TARGET, fields = %fields, "{message}"),
        None => tracing::$level!(target: TARGET, "{message}"),
      }
    };
  }
  match level {
    "trace" => line!(trace),
    "debug" => line!(debug),
    "info" => line!(info),
    "warn" => line!(warn),
    _ => line!(error),
  }
  Ok(Value::Null)
}
