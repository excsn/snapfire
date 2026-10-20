//! `intl`: number, currency, date, plural and list formatting and string collation through ICU4X, under
//! the ambient locale. The browser half is `Intl` with the same options, so
//! the outputs agree wherever the two CLDR snapshots do.

use fixed_decimal::{Decimal, FloatPrecision, SignedRoundingMode, UnsignedRoundingMode};
use icu_calendar::Date;
use icu_collator::options::CollatorOptions;
use icu_collator::Collator;
use icu_datetime::fieldsets::{D, DE, E, M, MD, MDE, Y, YM, YMD, YMDE};
use icu_datetime::DateTimeFormatter;
use icu_decimal::DecimalFormatter;
use icu_experimental::dimension::currency::formatter::CurrencyFormatter;
use icu_experimental::dimension::currency::CurrencyType;
use icu_list::options::{ListFormatterOptions, ListLength};
use icu_list::ListFormatter;
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
  extensions.register("intl.list", Reach::Render, list);
  extensions.register("intl.compare", Reach::Render, compare);
}

fn locale_of(ambient: &Ambient) -> Locale {
  ambient.bcp47().parse().unwrap_or_else(|_| locale!("en"))
}

/// The locale an options object names under `locale`, else the ambient one. A locale written in the code formats the same everywhere, which is what the author asked for by writing it.
fn locale_for(what: &str, ambient: &Ambient, options: Option<&Value>) -> Result<Locale, Fail> {
  match options {
    Some(Value::Map(map)) => match map.get("locale") {
      Some(Value::Str(tag)) => tag.as_str().parse().map_err(|_| Fail::internal(format!("{what}: `{tag}` is not a locale"))),
      Some(Value::Null) | None => Ok(locale_of(ambient)),
      Some(other) => Err(Fail::internal(format!("{what}: `locale` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    },
    _ => Ok(locale_of(ambient)),
  }
}

/// A value as a decimal or the string JavaScript prints for a number no
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
    Err(text) => return Ok(Value::str(text.to_owned())),
  };
  let min = digits(what, args, 1, "minimumFractionDigits")?.unwrap_or(0);
  let max = digits(what, args, 1, "maximumFractionDigits")?.unwrap_or(3.max(min)).max(min);
  round(&mut d, min, max);
  let formatter = DecimalFormatter::try_new(locale_for(what, ambient, args.get(1))?.into(), Default::default()).map_err(|e| data(what, e))?;
  Ok(Value::str(formatter.format(&d).to_string()))
}

/// `intl.currency(n, code, { display?, locale?, minimumFractionDigits?, maximumFractionDigits? })`: the amount with the ISO code, the currency's own fraction digits, which is `currencyDisplay: "code"` in the browser; `display` `symbol` or `narrowSymbol` writes the symbol CLDR gives the locale instead. With fraction digits given the amount is rounded half away from zero as `intl.number` rounds.
fn currency(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.currency";
  let mut d = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Ok(Value::str(text.to_owned())),
  };
  let code = text(what, args, 1)?;
  let code: CurrencyType = code.parse().map_err(|_| Fail::internal(format!("{what}: `{code}` is not a currency code")))?;
  let min = digits(what, args, 2, "minimumFractionDigits")?;
  let max = digits(what, args, 2, "maximumFractionDigits")?;
  let locale = locale_for(what, ambient, args.get(2))?;
  let prefs = locale.clone().into();
  let formatter = match option(what, args, 2, "display")? {
    None => CurrencyFormatter::try_new_code(prefs, code, Default::default()),
    Some(Value::Str(display)) => match display.as_str() {
      "code" => CurrencyFormatter::try_new_code(prefs, code, Default::default()),
      "symbol" => CurrencyFormatter::try_new_symbol(prefs, code, Default::default()),
      "narrowSymbol" => CurrencyFormatter::try_new_symbol_narrow(prefs, code, Default::default()),
      other => return Err(Fail::internal(format!("{what}: `{other}` is not a display; code, symbol or narrowSymbol"))),
    },
    Some(other) => return Err(Fail::internal(format!("{what}: `display` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  }
  .map_err(|e| data(what, e))?;
  if min.is_none() && max.is_none() {
    return Ok(Value::str(formatter.format_fixed_decimal(&d).to_string()));
  }
  // The currency formatter pads to the currency's own digits, so the digits
  // the options ask for go through the decimal formatter and take the
  // currency's symbol and placing from a zero formatted the currency's way.
  let min = min.unwrap_or(0);
  round(&mut d, min, max.unwrap_or(3.max(min)).max(min));
  let negative = d.sign() == fixed_decimal::Sign::Negative;
  d.set_sign(fixed_decimal::Sign::None);
  let decimals = DecimalFormatter::try_new(locale.into(), Default::default()).map_err(|e| data(what, e))?;
  let zero: Decimal = "0".parse().map_err(|_| Fail::internal(format!("{what}: zero is a decimal")))?;
  let sentinel = formatter.format_fixed_decimal(&zero).to_string();
  let (prefix, suffix) = (0..=4i16)
    .rev()
    .find_map(|places| {
      let mut padded = zero.clone();
      padded.pad_end(-places);
      let text = decimals.format(&padded).to_string();
      sentinel.find(&text).map(|at| (sentinel[..at].to_owned(), sentinel[at + text.len()..].to_owned()))
    })
    .ok_or_else(|| Fail::internal(format!("{what}: `{sentinel}` holds no number the locale writes")))?;
  Ok(Value::str(format!("{}{prefix}{}{suffix}", if negative { "-" } else { "" }, decimals.format(&d))))
}

/// `intl.date(when, style?)`: a calendar date in UTC at `dateStyle`
/// `short`, `medium` (the default), `long` or `full`, or at the browser's
/// `weekday`, `year`, `month` and `day` options given as an object, which may
/// also name a `locale`. `when` is milliseconds since the epoch or an ISO
/// 8601 string.
fn date(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.date";
  let ms = match args.first() {
    Some(Value::Str(s)) => super::time::parse_iso(s).ok_or_else(|| Fail::internal(format!("{what}: `{s}` is not an ISO 8601 date")))?,
    _ => number(what, args, 0)?,
  };
  let (y, m, d) = super::time::civil_from_ms(ms);
  let date = Date::try_new_iso(y as i32, m as u8, d as u8).map_err(|e| data(what, e))?;
  let prefs = locale_for(what, ambient, args.get(1))?.into();
  let part = |name: &str| -> Result<Option<String>, Fail> {
    let Some(Value::Map(options)) = args.get(1) else { return Ok(None) };
    match options.get(name) {
      None | Some(Value::Null) => Ok(None),
      Some(Value::Str(s)) => Ok(Some(s.to_string())),
      Some(other) => Err(Fail::internal(format!("{what}: `{name}` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    }
  };
  let (weekday, year, month, day) = (part("weekday")?, part("year")?, part("month")?, part("day")?);
  let parts = weekday.is_some() || year.is_some() || month.is_some() || day.is_some();
  let style = match args.get(1) {
    Some(Value::Map(_)) if parts => None,
    Some(Value::Map(map)) => Some(match map.get("style") {
      Some(Value::Str(s)) => s.to_string(),
      _ => "medium".to_owned(),
    }),
    _ => Some(text_opt(what, args, 1)?.unwrap_or("medium").to_owned()),
  };
  macro_rules! format_with {
    ($set:ident, $length:expr) => {
      match $length {
        Length::Long => DateTimeFormatter::try_new(prefs, $set::long()).map_err(|e| data(what, e))?.format(&date).to_string(),
        Length::Medium => DateTimeFormatter::try_new(prefs, $set::medium()).map_err(|e| data(what, e))?.format(&date).to_string(),
        Length::Short => DateTimeFormatter::try_new(prefs, $set::short()).map_err(|e| data(what, e))?.format(&date).to_string(),
      }
    };
  }
  let out = match style.as_deref() {
    Some("short") => format_with!(YMD, Length::Short),
    Some("medium") => format_with!(YMD, Length::Medium),
    Some("long") => format_with!(YMD, Length::Long),
    Some("full") => format_with!(YMDE, Length::Long),
    Some(other) => return Err(Fail::internal(format!("{what}: `{other}` is not a style; short, medium, long or full"))),
    None => {
      // The browser's `weekday`, `month`, `year` and `day` options as ICU4X
      // field sets. A text month or weekday sets the length: long, short
      // (ICU4X medium, "Jun") or narrow (ICU4X short). A numeric month has no
      // field set that agrees with the browser's digits, so it is refused.
      let length = |text: &Option<String>| -> Result<Option<Length>, Fail> {
        match text.as_deref() {
          None => Ok(None),
          Some("long") => Ok(Some(Length::Long)),
          Some("short") => Ok(Some(Length::Medium)),
          Some("narrow") => Ok(Some(Length::Short)),
          Some(other) => Err(Fail::internal(format!("{what}: `{other}` is not a text width; long, short or narrow"))),
        }
      };
      if let Some(value) = year.as_deref().filter(|v| *v != "numeric") {
        return Err(Fail::internal(format!("{what}: `year: \"{value}\"` is not read; numeric is")));
      }
      if let Some(value) = day.as_deref().filter(|v| *v != "numeric") {
        return Err(Fail::internal(format!("{what}: `day: \"{value}\"` is not read; numeric is")));
      }
      let length = match (length(&month)?, length(&weekday)?) {
        (Some(l), _) | (None, Some(l)) => l,
        (None, None) if month.is_some() => return Err(Fail::internal(format!("{what}: a numeric month has no field set that agrees with the browser; use long, short or narrow"))),
        (None, None) => Length::Long,
      };
      match (weekday.is_some(), year.is_some(), month.is_some(), day.is_some()) {
        (true, false, false, false) => format_with!(E, length),
        (false, false, false, true) => format_with!(D, length),
        (true, false, false, true) => format_with!(DE, length),
        (false, false, true, false) => format_with!(M, length),
        (false, false, true, true) => format_with!(MD, length),
        (true, false, true, true) => format_with!(MDE, length),
        (false, true, false, false) => format_with!(Y, length),
        (false, true, true, false) => format_with!(YM, length),
        (false, true, true, true) => format_with!(YMD, length),
        (true, true, true, true) => format_with!(YMDE, length),
        _ => return Err(Fail::internal(format!("{what}: no field set holds weekday {}, year {}, month {} and day {}", weekday.is_some(), year.is_some(), month.is_some(), day.is_some()))),
      }
    }
  };
  Ok(Value::str(out))
}

enum Length {
  Long,
  Medium,
  Short,
}

/// `intl.plural(n)`: the cardinal category, `zero`, `one`, `two`, `few`,
/// `many` or `other`.
fn plural(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  Ok(Value::str(category(ambient, args.first().unwrap_or(&Value::Null))?))
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

/// `intl.list(items, { type, style, locale })`: the strings joined as `Intl.ListFormat` joins them, `type` `conjunction` (the default), `disjunction` or `unit` and `style` `long` (the default), `short` or `narrow`.
fn list(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.list";
  let items = match args.first() {
    Some(Value::Seq(items)) => items.iter().map(|item| match item {
      Value::Str(s) => Ok(s.to_string()),
      other => Err(Fail::internal(format!("{what}: each item wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    }).collect::<Result<Vec<_>, _>>()?,
    Some(other) => return Err(Fail::internal(format!("{what}: wants an array of strings, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    None => Vec::new(),
  };
  let options = args.get(1);
  let read = |key: &str| match options {
    Some(Value::Map(map)) => match map.get(key) {
      Some(Value::Str(s)) => Ok(Some(s.to_string())),
      Some(Value::Null) | None => Ok(None),
      Some(other) => Err(Fail::internal(format!("{what}: `{key}` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    },
    _ => Ok(None),
  };
  let length = match read("style")?.as_deref() {
    None | Some("long") => ListLength::Wide,
    Some("short") => ListLength::Short,
    Some("narrow") => ListLength::Narrow,
    Some(other) => return Err(Fail::internal(format!("{what}: `style` is long, short or narrow, got `{other}`"))),
  };
  let locale = locale_for(what, ambient, options)?;
  let made = ListFormatterOptions::default().with_length(length);
  let formatter = match read("type")?.as_deref() {
    None | Some("conjunction") => ListFormatter::try_new_and(locale.into(), made),
    Some("disjunction") => ListFormatter::try_new_or(locale.into(), made),
    Some("unit") => ListFormatter::try_new_unit(locale.into(), made),
    Some(other) => return Err(Fail::internal(format!("{what}: `type` is conjunction, disjunction or unit, got `{other}`"))),
  }
  .map_err(|e| data(what, e))?;
  Ok(Value::str(formatter.format(items.iter().map(String::as_str)).to_string()))
}

/// `intl.compare(a, b, { locale })`: -1, 0 or 1 as `a.localeCompare(b, locale)` answers, by the locale's collation with the default options.
fn compare(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.compare";
  let a = text(what, args, 0)?;
  let b = text(what, args, 1)?;
  let locale = locale_for(what, ambient, args.get(2))?;
  let collator = Collator::try_new(locale.into(), CollatorOptions::default()).map_err(|e| data(what, e))?;
  Ok(Value::int(match collator.compare(&a, &b) {
    std::cmp::Ordering::Less => -1i64,
    std::cmp::Ordering::Equal => 0,
    std::cmp::Ordering::Greater => 1,
  }))
}
