//! `intl`: number, currency, date and plural formatting through ICU4X, under
//! the ambient locale. The browser half is `Intl` with the same options, so
//! the outputs agree wherever the two CLDR snapshots do.

use fixed_decimal::{Decimal, FloatPrecision, SignedRoundingMode, UnsignedRoundingMode};
use icu_calendar::Date;
use icu_datetime::fieldsets::{YMD, YMDE};
use icu_datetime::DateTimeFormatter;
use icu_decimal::DecimalFormatter;
use icu_experimental::dimension::currency::formatter::CurrencyFormatter;
use icu_experimental::dimension::currency::CurrencyType;
use icu_locale_core::{locale, Locale};
use icu_plurals::{PluralOperands, PluralRules};
use snapfire_fsr_core::Value;

use crate::ext::{number, option, text, text_opt, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("intl.number", Reach::Render, number_fn);
  extensions.register("intl.currency", Reach::Render, currency);
  extensions.register("intl.date", Reach::Render, date);
  extensions.register("intl.plural", Reach::Render, plural);
}

fn locale_of(ambient: &Ambient) -> Locale {
  ambient.bcp47().parse().unwrap_or_else(|_| locale!("en"))
}

/// A value as a decimal, or the string JavaScript prints for a number no
/// decimal holds.
fn decimal(what: &str, args: &[Value], i: usize) -> Result<Result<Decimal, &'static str>, Fail> {
  match args.get(i) {
    Some(Value::Int(n)) => Ok(Ok(n.to_string().parse().map_err(|_| Fail::internal(format!("{what}: {n} is not a decimal")))?)),
    Some(Value::UInt(n)) => Ok(Ok(n.to_string().parse().map_err(|_| Fail::internal(format!("{what}: {n} is not a decimal")))?)),
    _ => {
      let n = number(what, args, i)?;
      if n.is_nan() {
        return Ok(Err("NaN"));
      }
      if n.is_infinite() {
        return Ok(Err(if n > 0.0 { "∞" } else { "-∞" }));
      }
      Ok(Ok(Decimal::try_from_f64(n, FloatPrecision::RoundTrip).map_err(|e| Fail::internal(format!("{what}: {e:?}")))?))
    }
  }
}

fn digits(what: &str, args: &[Value], i: usize, field: &str) -> Result<Option<i16>, Fail> {
  match option(what, args, i, field)? {
    None => Ok(None),
    Some(v) => {
      let n = number(what, std::slice::from_ref(v), 0)?;
      Ok(Some(n.clamp(0.0, 20.0) as i16))
    }
  }
}

fn round(d: &mut Decimal, min: i16, max: i16) {
  d.round_with_mode(-max, SignedRoundingMode::Unsigned(UnsignedRoundingMode::HalfExpand));
  d.trim_end();
  d.pad_end(-min);
}

fn data(what: &str, e: impl std::fmt::Debug) -> Fail {
  Fail::internal(format!("{what}: {e:?}"))
}

/// `intl.number(n, { minimumFractionDigits?, maximumFractionDigits? })`:
/// grouped and rounded half away from zero to at most three fraction digits
/// unless the options say otherwise, trailing zeros dropped past the minimum.
fn number_fn(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.number";
  let mut d = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Ok(Value::Str(text.to_owned())),
  };
  let min = digits(what, args, 1, "minimumFractionDigits")?.unwrap_or(0);
  let max = digits(what, args, 1, "maximumFractionDigits")?.unwrap_or(3.max(min)).max(min);
  round(&mut d, min, max);
  let formatter = DecimalFormatter::try_new(locale_of(ambient).into(), Default::default()).map_err(|e| data(what, e))?;
  Ok(Value::Str(formatter.format(&d).to_string()))
}

/// `intl.currency(n, code)`: the amount with the ISO code, the currency's
/// own fraction digits, which is `currencyDisplay: "code"` in the browser.
fn currency(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.currency";
  let d = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Ok(Value::Str(text.to_owned())),
  };
  let code = text(what, args, 1)?;
  let code: CurrencyType = code.parse().map_err(|_| Fail::internal(format!("{what}: `{code}` is not a currency code")))?;
  let formatter = CurrencyFormatter::try_new_code(locale_of(ambient).into(), code, Default::default()).map_err(|e| data(what, e))?;
  Ok(Value::Str(formatter.format_fixed_decimal(&d).to_string()))
}

/// `intl.date(when, style?)`: a calendar date in UTC at `dateStyle`
/// `short`, `medium` (the default), `long` or `full`. `when` is milliseconds
/// since the epoch or an ISO 8601 string.
fn date(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.date";
  let ms = match args.first() {
    Some(Value::Str(s)) => super::time::parse_iso(s).ok_or_else(|| Fail::internal(format!("{what}: `{s}` is not an ISO 8601 date")))?,
    _ => number(what, args, 0)?,
  };
  let style = match args.get(1) {
    Some(Value::Map(map)) => match map.get("style") {
      Some(Value::Str(s)) => s.clone(),
      _ => "medium".to_owned(),
    },
    _ => text_opt(what, args, 1)?.unwrap_or("medium").to_owned(),
  };
  let (y, m, d) = super::time::civil_from_ms(ms);
  let date = Date::try_new_iso(y as i32, m as u8, d as u8).map_err(|e| data(what, e))?;
  let prefs = locale_of(ambient).into();
  let out = match style.as_str() {
    "short" => DateTimeFormatter::try_new(prefs, YMD::short()).map_err(|e| data(what, e))?.format(&date).to_string(),
    "medium" => DateTimeFormatter::try_new(prefs, YMD::medium()).map_err(|e| data(what, e))?.format(&date).to_string(),
    "long" => DateTimeFormatter::try_new(prefs, YMD::long()).map_err(|e| data(what, e))?.format(&date).to_string(),
    "full" => DateTimeFormatter::try_new(prefs, YMDE::long()).map_err(|e| data(what, e))?.format(&date).to_string(),
    other => return Err(Fail::internal(format!("{what}: `{other}` is not a style; short, medium, long or full"))),
  };
  Ok(Value::Str(out))
}

/// `intl.plural(n)`: the cardinal category, `zero`, `one`, `two`, `few`,
/// `many` or `other`.
fn plural(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  Ok(Value::Str(category(ambient, args.first().unwrap_or(&Value::Null))?))
}

/// The cardinal plural category of `n` under the ambient locale.
pub fn category(ambient: &Ambient, n: &Value) -> Result<String, Fail> {
  let what = "intl.plural";
  let rules = PluralRules::try_new_cardinal(locale_of(ambient).into()).map_err(|e| data(what, e))?;
  let operands = match decimal(what, std::slice::from_ref(n), 0)? {
    Ok(d) => PluralOperands::from(&d),
    Err(_) => return Ok("other".to_owned()),
  };
  Ok(format!("{:?}", rules.category_for(operands)).to_ascii_lowercase())
}
