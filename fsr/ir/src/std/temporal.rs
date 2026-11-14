//! `temporal`: JavaScript's `Temporal` through snapfire_time. A Temporal value is the typed variant `$temporal` holding its type and canonical string; every call reads that string and writes a new one.

use snapfire_fsr_core::value::typed;
use snapfire_fsr_core::Value;
use snapfire_time::temporal::{self, Answer, Arg, Kind};

use crate::ext::{text, Ambient, Extensions, Reach};
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
  extensions.register("temporal.call", Reach::Render, |ambient, args| {
    let (kind, value) = held("temporal.call", args)?;
    let method = text("temporal.call", args, 1)?;
    if method == "toLocaleString" {
      return locale_string(ambient, kind, value, &args[2..]);
    }
    answered(temporal::call(kind, value, method, &rest(args, 2)?))
  });
  extensions.register("temporal.now", Reach::Body, |ambient, args| answered(temporal::now(text("temporal.now", args, 0)?, ambient.now as f64, &rest(args, 1)?)));
}

/// `value.toLocaleString(locales, options)` of a Temporal value, as V8 writes it: a plain date, time or date and time as `Date` writes the same fields in UTC, a zoned value in its zone followed by the zone's short name, an instant in the zone `options` names or this machine's, a duration as `Intl.DurationFormat` writes it in its short style. A year and month or a month and day is in the ISO calendar, which no locale writes dates in, so V8 throws.
fn locale_string(ambient: &Ambient, kind: Kind, value: &str, args: &[Value]) -> Result<Value, Fail> {
  let what = "toLocaleString";
  let tag = match args.first() {
    Some(Value::Str(tag)) => tag.to_string(),
    Some(Value::Seq(tags)) => match tags.iter().next() {
      Some(Value::Str(tag)) => tag.to_string(),
      _ => ambient.bcp47().to_owned(),
    },
    _ => ambient.bcp47().to_owned(),
  };
  let options = match args.get(1) {
    Some(Value::Map(map)) => map.clone(),
    _ => snapfire_fsr_core::ValueMap::default(),
  };
  let zone = match options.get("timeZone") {
    Some(Value::Str(zone)) => Some(zone.to_string()),
    _ => None,
  };
  let styled = options.keys().any(|k| k != "timeZone");
  let field = |kind: Kind, value: &str, name: &str| -> Result<f64, Fail> {
    match temporal::get(kind, value, name) {
      Ok(Answer::Num(n)) => Ok(n),
      other => Err(Fail::internal(format!("{what}: `{name}` of a Temporal.{} read {other:?}", kind.name()))),
    }
  };
  let local = |kind: Kind, value: &str, date: bool, time: bool| -> Result<f64, Fail> {
    let (y, m, d) = if date { (field(kind, value, "year")?, field(kind, value, "month")?, field(kind, value, "day")?) } else { (1970.0, 1.0, 1.0) };
    let (h, min, s) = if time { (field(kind, value, "hour")?, field(kind, value, "minute")?, field(kind, value, "second")?) } else { (0.0, 0.0, 0.0) };
    Ok(snapfire_fsr_core::time::days_from_civil(y as i64, m as u32, d as u32) as f64 * 86_400_000.0 + ((h * 60.0 + min) * 60.0 + s) * 1000.0)
  };
  let refuse = |kind: Kind| Fail::internal(format!("{what}: a Temporal.{} written with options other than `timeZone`, which the server does not write", kind.name()));
  let written = match kind {
    Kind::PlainYearMonth | Kind::PlainMonthDay => return Err(Fail::internal("RangeError: Mismatched calendars.")),
    Kind::Duration => {
      let mut amounts = snapfire_fsr_core::ValueMap::default();
      for unit in ["years", "months", "weeks", "days", "hours", "minutes", "seconds", "milliseconds", "microseconds", "nanoseconds"] {
        amounts.insert(unit.to_owned(), Value::F64(field(kind, value, unit)?));
      }
      let mut format = options.clone();
      format.insert("locale".to_owned(), Value::str(tag.as_str()));
      if !format.contains_key("style") {
        format.insert("style".to_owned(), Value::str("short"));
      }
      return super::intl::duration(ambient, &[Value::Map(amounts), Value::Map(format)]);
    }
    Kind::PlainDate if styled => {
      let mut format = options.clone();
      format.insert("locale".to_owned(), Value::str(tag.as_str()));
      if let Some(style) = format.shift_remove("dateStyle") {
        format.insert("style".to_owned(), style);
      }
      return super::intl::date(ambient, &[Value::F64(local(kind, value, true, false)?), Value::Map(format)]);
    }
    _ if styled => return Err(refuse(kind)),
    Kind::PlainDate => super::intl::numeric_written(what, local(kind, value, true, false)?, &tag, "date")?,
    Kind::PlainTime => super::intl::numeric_written(what, local(kind, value, false, true)?, &tag, "time")?,
    Kind::PlainDateTime => super::intl::numeric_written(what, local(kind, value, true, true)?, &tag, "both")?,
    Kind::Instant | Kind::ZonedDateTime => {
      let (zoned, named) = match kind {
        Kind::ZonedDateTime => (value.to_owned(), true),
        _ => {
          let zone = zone.clone().unwrap_or_else(|| snapfire_time::system_zone().to_owned());
          match temporal::call(kind, value, "toZonedDateTimeISO", &[Arg::Str(zone)]) {
            Ok(Answer::Temporal(_, text)) => (text, false),
            Ok(other) => return Err(Fail::internal(format!("{what}: an instant moved into a zone read {other:?}"))),
            Err(e) => return answered(Err(e)),
          }
        }
      };
      let text = super::intl::numeric_written(what, local(Kind::ZonedDateTime, &zoned, true, true)?, &tag, "both")?;
      if !named {
        return Ok(Value::str(text));
      }
      let id = match temporal::get(Kind::ZonedDateTime, &zoned, "timeZoneId") {
        Ok(Answer::Str(id)) => id,
        other => return Err(Fail::internal(format!("{what}: the zone of a Temporal.ZonedDateTime read {other:?}"))),
      };
      let at = field(Kind::ZonedDateTime, &zoned, "epochMilliseconds")?;
      let source = snapfire_time::source().map_err(|e| Fail::internal(e.to_string()))?;
      format!("{text} {}", crate::intl_data::IntlData::current()?.tables().short_zone_name(&tag, &id, at, &super::intl::Zones(source)))
    }
  };
  Ok(Value::str(written))
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
