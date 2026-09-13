//! What the desk keeps per visitor: the symbol being watched, the lot size
//! and the shares bought on top of the holdings.

use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::RequestCtx;

use crate::state;

pub const LOT_MIN: i64 = 10;
pub const LOT_MAX: i64 = 100;

pub fn watched(ctx: &RequestCtx) -> String {
  match ctx.session.get("watched") {
    Some(Value::Str(symbol)) => symbol.to_string(),
    _ => state::HOLDINGS[0].symbol.to_owned(),
  }
}

/// The shares one buy takes, clamped: the value is written by the lot
/// stepper's action and read as input like any other.
pub fn lot(ctx: &RequestCtx) -> i64 {
  match ctx.session.get("lot") {
    Some(Value::Int(n)) => (n as i64).clamp(LOT_MIN, LOT_MAX),
    _ => LOT_MIN,
  }
}

/// The shares this session has bought on top of what the desk holds.
pub fn bought(ctx: &RequestCtx, symbol: &str) -> i64 {
  match ctx.session.get("bought") {
    Some(Value::Map(map)) => match map.get(symbol) {
      Some(Value::Int(n)) => *n as i64,
      _ => 0,
    },
    _ => 0,
  }
}
