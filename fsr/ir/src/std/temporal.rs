//! `temporal`: JavaScript's `Temporal` through snapfire_time. A Temporal value is the typed variant `$temporal` holding its type and canonical string; every call reads that string and writes a new one.

use snapfire_fsr_core::value::typed;
use snapfire_fsr_core::Value;
use snapfire_time::temporal::{self, Answer, Arg, Kind};

use crate::ext::{text, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("temporal.new", Reach::Render, |_, args| {
    let kind = kind_named("temporal.new", args)?;
    answered(temporal::construct(kind, &rest(args, 1)?))
  });
  extensions.register("temporal.static", Reach::Render, |_, args| {
    let kind = kind_named("temporal.static", args)?;
    answered(temporal::call_static(kind, text("temporal.static", args, 1)?, &rest(args, 2)?))
  });
  extensions.register("temporal.get", Reach::Render, |_, args| {
    let (kind, value) = held("temporal.get", args)?;
    answered(temporal::get(kind, value, text("temporal.get", args, 1)?))
  });
  extensions.register("temporal.call", Reach::Render, |_, args| {
    let (kind, value) = held("temporal.call", args)?;
    answered(temporal::call(kind, value, text("temporal.call", args, 1)?, &rest(args, 2)?))
  });
  extensions.register("temporal.now", Reach::Body, |ambient, args| answered(temporal::now(text("temporal.now", args, 0)?, ambient.now as f64, &rest(args, 1)?)));
}

fn kind_named(what: &str, args: &[Value]) -> Result<Kind, Fail> {
  let name = text(what, args, 0)?;
  Kind::of(name).ok_or_else(|| Fail::internal(format!("TypeError: Temporal.{name} is not a constructor")))
}

/// The Temporal value a method or a field read is called on.
fn held<'a>(what: &str, args: &'a [Value]) -> Result<(Kind, &'a str), Fail> {
  let value = args.first().unwrap_or(&Value::Undefined);
  let (kind, text) = typed::temporal(value).ok_or_else(|| Fail::internal(format!("TypeError: {what}: {} is not a Temporal value", snapfire_fsr_core::ext::kind_name(value))))?;
  Ok((Kind::of(kind).ok_or_else(|| Fail::internal(format!("TypeError: Temporal.{kind} is not a type")))?, text))
}

fn rest(args: &[Value], from: usize) -> Result<Vec<Arg>, Fail> {
  args.iter().skip(from).map(arg).collect()
}

/// `value` as snapfire_time reads an argument.
pub fn arg(value: &Value) -> Result<Arg, Fail> {
  if let Some((kind, text)) = typed::temporal(value) {
    let kind = Kind::of(kind).ok_or_else(|| Fail::internal(format!("TypeError: Temporal.{kind} is not a type")))?;
    return Ok(Arg::Temporal(kind, text.to_owned()));
  }
  Ok(match value {
    Value::Undefined => Arg::Undefined,
    Value::Null => Arg::Null,
    Value::Bool(b) => Arg::Bool(*b),
    Value::F64(n) => Arg::Num(*n),
    Value::F32(n) => Arg::Num(f64::from(*n)),
    Value::Int(n) => Arg::BigInt(*n),
    Value::UInt(n) => Arg::BigInt(i128::try_from(*n).map_err(|_| Fail::internal("RangeError: a bigint past 128 bits"))?),
    Value::Str(s) => Arg::Str(s.to_string()),
    Value::Map(map) => Arg::Obj(map.iter().map(|(k, v)| Ok((k.to_string(), arg(v)?))).collect::<Result<_, Fail>>()?),
    other => return Err(Fail::internal(format!("TypeError: Temporal does not take {}", snapfire_fsr_core::ext::kind_name(other)))),
  })
}

fn answered(result: Result<Answer, snapfire_time::Error>) -> Result<Value, Fail> {
  match result {
    Ok(Answer::Undefined) => Ok(Value::Undefined),
    Ok(Answer::Bool(b)) => Ok(Value::Bool(b)),
    Ok(Answer::Num(n)) => Ok(Value::F64(n)),
    Ok(Answer::BigInt(n)) => Ok(Value::Int(n)),
    Ok(Answer::Str(s)) => Ok(Value::str(s)),
    Ok(Answer::Temporal(kind, text)) => Ok(typed::temporal_value(kind.name(), &text)),
    Err(snapfire_time::Error::Type(message)) => Err(Fail::internal(format!("TypeError: {message}"))),
    Err(e @ snapfire_time::Error::NoSource | e @ snapfire_time::Error::NotBuilt(_)) => Err(Fail::internal(e.to_string())),
    Err(other) => Err(Fail::internal(format!("RangeError: {other}"))),
  }
}
