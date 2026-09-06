//! The Rust half of the pairs `app/ext/fleet.ts` declares.

use snapfire_fsr_core::Value;
use snapfire_fsr_host::HostBuilder;
use snapfire_fsr_ir::{Ambient, Fail, Reach};

pub fn queue_label(depth: f64) -> String {
  if depth == 0.0 {
    "idle".to_owned()
  } else if depth == 1.0 {
    "1 queued".to_owned()
  } else {
    format!("{depth} queued")
  }
}

fn queue_label_ext(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let depth = match args.first() {
    Some(Value::Int(n)) => *n as f64,
    Some(Value::UInt(n)) => *n as f64,
    Some(Value::F64(f)) => *f,
    Some(Value::F32(f)) => *f as f64,
    other => return Err(Fail::new(snapfire_fsr_runtime::FailureKind::Internal, format!("fleet.queueLabel takes a number, got {other:?}"))),
  };
  Ok(Value::Str(queue_label(depth)))
}

/// Registers every pair on `builder`.
pub fn register(builder: HostBuilder) -> HostBuilder {
  builder.extension("fleet.queueLabel", Reach::Render, queue_label_ext)
}
