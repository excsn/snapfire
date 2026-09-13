//! What the desk keeps per visitor: the symbol being watched, the lot size
//! and the position bought on top of the holdings.

use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::RequestCtx;

use crate::state::{self, Position};

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

/// What this session bought on top of what the desk holds, plus what it paid.
pub fn position(ctx: &RequestCtx, symbol: &str) -> Position {
  let Some(Value::Map(book)) = ctx.session.get("bought") else { return Position::default() };
  let Some(Value::Map(held)) = book.get(symbol) else { return Position::default() };
  Position {
    shares: match held.get("shares") {
      Some(Value::Int(n)) => *n as i64,
      _ => 0,
    },
    spent: match held.get("spent") {
      Some(Value::F64(spent)) => *spent,
      _ => 0.0,
    },
  }
}
