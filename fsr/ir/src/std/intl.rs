//! `intl`: number, currency, date, plural and list formatting and string collation through ICU4X, with
//! currency, percent and relative-time wording from snapfire_i18n, under the ambient locale. The browser half is `Intl` with the same options, so
//! the outputs agree wherever the two CLDR snapshots do.

use fixed_decimal::{Decimal, FloatPrecision, SignDisplay, SignedRoundingMode, UnsignedRoundingMode};
use icu_calendar::Date;
use icu_collator::options::{AlternateHandling, CaseLevel, CollatorOptions, Strength};
use icu_collator::preferences::{CollationCaseFirst, CollationNumericOrdering};
use icu_collator::{Collator, CollatorPreferences};
use icu_datetime::fieldsets::{D, DE, E, M, MD, MDE, T, Y, YM, YMD, YMDE, YMDET, YMDT};
use icu_datetime::input::{DateTime, Time};
use icu_datetime::options::{TimePrecision, YearStyle};
use icu_datetime::DateTimeFormatter;
use icu_decimal::options::{DecimalFormatterOptions, GroupingStrategy};
use icu_decimal::DecimalFormatter;
use icu_list::options::{ListFormatterOptions, ListLength};
use icu_list::ListFormatter;
use icu_locale_core::{locale, Locale};
use icu_plurals::{PluralCategory, PluralOperands, PluralRules};
use snapfire_fsr_core::Value;
use snapfire_i18n::{Display, Plural, Sign, Unit, Width};

use crate::ext::{number, option, text, text_opt, Ambient, Extensions, Reach};
use crate::interp::Fail;
use crate::intl_data::IntlData;

fn intl() -> Result<&'static IntlData, Fail> {
  IntlData::current()
}

/// A list's width as the part of a [`made`] key that tells widths apart.
fn list_width(length: ListLength) -> u64 {
  match length {
    ListLength::Short => 1,
    ListLength::Narrow => 2,
    _ => 0,
  }
}

/// How many formatters a thread keeps before it starts over.
const MADE_LIMIT: usize = 64;

/// The formatter `make` builds for `locale` and `options`, built once for each thread and kept. `options` tells apart every set of options a formatter of this type is built with.
fn made<T: 'static>(locale: &Locale, options: u64, make: impl FnOnce(&'static IntlData) -> Result<T, Fail>) -> Result<std::rc::Rc<T>, Fail> {
  type Key = (std::any::TypeId, u64, Locale, u64);
  thread_local! {
    static MADE: std::cell::RefCell<std::collections::HashMap<Key, std::rc::Rc<dyn std::any::Any>>> = std::cell::RefCell::default();
  }
  let intl = intl()?;
  let key: Key = (std::any::TypeId::of::<T>(), intl.id(), locale.clone(), options);
  if let Some(held) = MADE.with(|made| made.borrow().get(&key).cloned()).and_then(|held| held.downcast::<T>().ok()) {
    return Ok(held);
  }
  let formatter = std::rc::Rc::new(make(intl)?);
  MADE.with(|made| {
    let mut made = made.borrow_mut();
    if made.len() >= MADE_LIMIT {
      made.clear();
    }
    made.insert(key, formatter.clone());
  });
  Ok(formatter)
}

pub fn register(extensions: &mut Extensions) {
  extensions.register("intl.number", Reach::Render, number_fn);
  extensions.register("intl.currency", Reach::Render, currency);
  extensions.register("intl.date", Reach::Render, date);
  extensions.register("intl.dateParts", Reach::Render, date_parts);
  extensions.register("intl.numberParts", Reach::Render, number_parts);
  extensions.register("intl.locale", Reach::Render, locale_op);
  extensions.register("intl.duration", Reach::Render, duration);
  extensions.register("intl.displayNames", Reach::Render, display_names);
  extensions.register("intl.resolvedOptions", Reach::Render, resolved_options);
  extensions.register("intl.supportedLocales", Reach::Render, |_, args| {
    let given: Vec<String> = match args.first() {
      Some(Value::Str(tag)) => vec![tag.to_string()],
      Some(Value::Seq(tags)) => tags.iter().map(|t| match t {
        Value::Str(tag) => Ok(tag.to_string()),
        other => Err(Fail::internal(format!("TypeError: supportedLocalesOf: a locale is a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
      }).collect::<Result<_, _>>()?,
      Some(Value::Null | Value::Undefined) | None => Vec::new(),
      Some(other) => return Err(Fail::internal(format!("TypeError: supportedLocalesOf: the locales are a string or an array, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    };
    let mut out: Vec<Value> = Vec::new();
    for tag in given {
      let canonical: Locale = tag.parse().map_err(|_| Fail::internal(format!("RangeError: Incorrect locale information provided: {tag}")))?;
      let canonical = canonical.to_string();
      if intl()?.tables().supported(&canonical) && !out.iter().any(|v| matches!(v, Value::Str(s) if s.as_str() == canonical)) {
        out.push(Value::str(canonical));
      }
    }
    Ok(Value::seq(out))
  });
  extensions.register("intl.supported", Reach::Render, |_, args| {
    let key = text("intl.supported", args, 0)?;
    let values = super::supported::supported(key).ok_or_else(|| Fail::internal(format!("RangeError: Invalid key : {key}")))?;
    Ok(Value::seq(values.iter().map(|v| Value::str(*v)).collect::<Vec<_>>()))
  });
  extensions.register("intl.plural", Reach::Render, plural);
  extensions.register("intl.list", Reach::Render, list);
  extensions.register("intl.listParts", Reach::Render, list_parts);
  extensions.register("intl.compare", Reach::Render, compare);
  extensions.register("intl.relative", Reach::Render, relative);
  extensions.register("intl.relativeParts", Reach::Render, relative_parts);
  extensions.register("intl.localeTag", Reach::Render, |ambient, _| Ok(Value::str(ambient.bcp47())));
  extensions.register("intl.localTime", Reach::Body, |_, args| Ok(Value::str(local_time(number("intl.localTime", args, 0)?, named_locale("intl.localTime", args, 1)?)?)));
  extensions.register("intl.upper", Reach::Render, |_, args| case("intl.upper", args, snapfire_i18n::upper_case));
  extensions.register("intl.lower", Reach::Render, |_, args| case("intl.lower", args, snapfire_i18n::lower_case));
  extensions.register("intl.systemLocale", Reach::Body, |_, _| Ok(Value::str(system_locale())));
  extensions.register("intl.numeric", Reach::Render, |ambient, args| {
    let what = "intl.numeric";
    let tag = named_locale(what, args, 1)?.map_or_else(|| ambient.bcp47().to_owned(), str::to_owned);
    let mode = match args.get(2) {
      Some(Value::Str(mode)) => mode.to_string(),
      _ => "date".to_owned(),
    };
    Ok(Value::str(numeric_written(what, number(what, args, 0)?, &tag, &mode)?))
  });
  extensions.register("intl.numericParts", Reach::Render, |ambient, args| {
    let what = "intl.numericParts";
    let tag = named_locale(what, args, 1)?.map_or_else(|| ambient.bcp47().to_owned(), str::to_owned);
    let tag = intl()?.locale(what, &tag)?.to_string();
    let local = number(what, args, 0)?;
    if local.is_nan() {
      return Err(Fail::internal("RangeError: Invalid time value"));
    }
    let (y, m, d) = super::time::civil_from_ms(local);
    let parts = intl()?.tables().numeric_date_parts(&tag, y, m, d).ok_or_else(|| Fail::internal(format!("{what}: the parts of a date in the calendar `{tag}` names")))?;
    Ok(Value::seq(parts.into_iter().map(|(kind, value)| {
      let mut map = snapfire_fsr_core::ValueMap::default();
      map.insert("type".to_owned(), Value::str(kind));
      map.insert("value".to_owned(), Value::str(value));
      Value::Map(map)
    }).collect::<Vec<_>>()))
  });
  extensions.register("intl.localDate", Reach::Body, |_, args| Ok(Value::str(local_date(number("intl.localDate", args, 0)?, named_locale("intl.localDate", args, 1)?, matches!(args.get(2), Some(Value::Bool(true))))?)));
  extensions.register("intl.timeString", Reach::Body, |ambient, args| Ok(Value::str(time_string(number("intl.timeString", args, 0)?, ambient.now as f64)?)));
  extensions.register("intl.dateTimeString", Reach::Body, |ambient, args| Ok(Value::str(date_time_string(number("intl.dateTimeString", args, 0)?, ambient.now as f64)?)));
}

/// `intl.upper(text, locales, fallback?)` and `intl.lower(text, locales, fallback?)`: `toLocaleUpperCase` and `toLocaleLowerCase`. `locales` is a tag or an array whose first tag applies. `fallback` applies when it names none: `undefined`, `null` or an empty array. A tag that is not well formed is refused, as JavaScript throws a `RangeError`.
fn case(what: &str, args: &[Value], map: fn(&str, &str) -> String) -> Result<Value, Fail> {
  let subject = text(what, args, 0)?;
  let tag = match named_locale(what, args, 1)? {
    Some(tag) => tag,
    None => text(what, args, 2)?,
  };
  tag.parse::<Locale>().map_err(|_| Fail::internal(format!("{what}: `{tag}` is not a locale")))?;
  Ok(Value::str(map(subject, tag)))
}

/// The tag a `locales` argument at `i` names, as JavaScript reads one: a string, or the first of an array. `None` for `undefined`, `null`, an empty array or no argument.
fn named_locale<'a>(what: &str, args: &'a [Value], i: usize) -> Result<Option<&'a str>, Fail> {
  Ok(match args.get(i) {
    Some(Value::Str(tag)) => Some(tag.as_str()),
    Some(Value::Seq(tags)) => match tags.iter().next() {
      Some(Value::Str(tag)) => Some(tag.as_str()),
      Some(other) => return Err(Fail::internal(format!("{what}: a locale is a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
      None => None,
    },
    Some(Value::Null | Value::Undefined) | None => None,
    Some(other) => return Err(Fail::internal(format!("{what}: the locales are a string or an array, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  })
}

fn locale_of(ambient: &Ambient) -> Locale {
  let tag = ambient.bcp47();
  intl().ok().and_then(|intl| intl.locale("the request's locale", &tag).ok()).or_else(|| tag.parse().ok()).unwrap_or_else(|| locale!("en"))
}

/// The locale an options object names under `locale`, else the ambient one. A list of locales names its first; an empty list is the ambient one. A locale written in the code formats the same everywhere, which is what the author asked for by writing it.
fn locale_for(what: &str, ambient: &Ambient, options: Option<&Value>) -> Result<Locale, Fail> {
  match options {
    Some(Value::Map(map)) => match map.get("locale") {
      Some(Value::Str(tag)) => intl()?.locale(what, tag.as_str()),
      Some(Value::Seq(tags)) => match tags.first() {
        Some(Value::Str(tag)) => intl()?.locale(what, tag.as_str()),
        None => Ok(locale_of(ambient)),
        Some(other) => Err(Fail::internal(format!("TypeError: {what}: a locale wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
      },
      Some(Value::Null | Value::Undefined) | None => Ok(locale_of(ambient)),
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

/// `intl.number(n, { style?, minimumFractionDigits?, maximumFractionDigits?, minimumIntegerDigits?, useGrouping?, signDisplay?, locale? })`: what `Intl.NumberFormat` writes with the same options. Rounded half away from zero to at most three fraction digits, none for a percent, unless the options say otherwise, trailing zeros dropped past the minimum. `style` is `decimal` (the default) or `percent`; `useGrouping` is `true`, `false`, `"auto"`, `"always"` or `"min2"`; `signDisplay` is `auto`, `always`, `exceptZero`, `negative` or `never`.
fn number_fn(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.number";
  let mut d = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Ok(Value::str(text.to_owned())),
  };
  let percent = match string_option(what, args, 1, "style")?.as_deref() {
    None | Some("decimal") => false,
    Some("percent") => true,
    Some(other) => return Err(Fail::internal(format!("{what}: `style` is decimal or percent, got `{other}`; a currency is `intl.currency`"))),
  };
  if percent {
    d.multiply_pow10(2);
    d.absolute.trim_start();
  }
  let min = digits(what, args, 1, "minimumFractionDigits")?.unwrap_or(0);
  let max = digits(what, args, 1, "maximumFractionDigits")?.unwrap_or(if percent { 0 } else { 3 }.max(min)).max(min);
  round(&mut d, min, max);
  if let Some(integers) = digits(what, args, 1, "minimumIntegerDigits")? {
    d.absolute.pad_start(integers);
  }
  d.apply_sign_display(match string_option(what, args, 1, "signDisplay")?.as_deref() {
    None | Some("auto") => SignDisplay::Auto,
    Some("always") => SignDisplay::Always,
    Some("exceptZero") => SignDisplay::ExceptZero,
    Some("negative") => SignDisplay::Negative,
    Some("never") => SignDisplay::Never,
    Some(other) => return Err(Fail::internal(format!("{what}: `signDisplay` is auto, always, exceptZero, negative or never, got `{other}`"))),
  });
  let mut options = DecimalFormatterOptions::default();
  options.grouping_strategy = Some(match option(what, args, 1, "useGrouping")? {
    None | Some(Value::Bool(true)) => GroupingStrategy::Auto,
    Some(Value::Bool(false)) => GroupingStrategy::Never,
    Some(Value::Str(s)) if s.as_str() == "auto" => GroupingStrategy::Auto,
    Some(Value::Str(s)) if s.as_str() == "always" => GroupingStrategy::Always,
    Some(Value::Str(s)) if s.as_str() == "min2" => GroupingStrategy::Min2,
    Some(other) => return Err(Fail::internal(format!("{what}: `useGrouping` is true, false, auto, always or min2, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  });
  let locale = locale_for(what, ambient, args.get(1))?;
  let grouping = match options.grouping_strategy {
    Some(GroupingStrategy::Never) => 2,
    Some(GroupingStrategy::Always) => 3,
    Some(GroupingStrategy::Min2) => 4,
    _ => 1,
  };
  let formatter = made(&locale, grouping, |intl| DecimalFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), options).map_err(|e| data(what, e)))?;
  if !percent {
    return Ok(Value::str(formatter.format(&d).to_string()));
  }
  let (digits, sign) = unsigned(&formatter, &d);
  Ok(Value::str(intl()?.tables().percent(&locale.to_string()).format(&digits, sign)))
}

/// The absolute value of `d` in the formatter's digits with the sign it was written with.
fn unsigned(formatter: &DecimalFormatter, d: &Decimal) -> (String, Sign) {
  let sign = match d.sign() {
    fixed_decimal::Sign::Negative => Sign::Minus,
    fixed_decimal::Sign::Positive => Sign::Plus,
    fixed_decimal::Sign::None => Sign::None,
  };
  let mut absolute = d.clone();
  absolute.set_sign(fixed_decimal::Sign::None);
  (formatter.format(&absolute).to_string(), sign)
}

/// `intl.currency(n, code, { display?, locale?, minimumFractionDigits?, maximumFractionDigits? })`: the amount with the ISO code, the currency's own fraction digits, which is `currencyDisplay: "code"` in the browser; `display` `symbol` or `narrowSymbol` writes the symbol CLDR gives the locale instead. Fraction digits default to the currency's own for both bounds; one bound given moves the other only as far as it must, as the browser does. The amount is rounded half away from zero as `intl.number` rounds.
fn currency(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.currency";
  let mut d = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Ok(Value::str(text.to_owned())),
  };
  let code = text(what, args, 1)?;
  let display = match option(what, args, 2, "display")? {
    None => Display::Code,
    Some(Value::Str(display)) => match display.as_str() {
      "code" => Display::Code,
      "symbol" => Display::Symbol,
      "narrowSymbol" => Display::NarrowSymbol,
      other => return Err(Fail::internal(format!("{what}: `{other}` is not a display; code, symbol or narrowSymbol"))),
    },
    Some(other) => return Err(Fail::internal(format!("{what}: `display` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  };
  let locale = locale_for(what, ambient, args.get(2))?;
  let currency = intl()?.tables().currency(&locale.to_string(), &code, display).map_err(|e| Fail::internal(format!("{what}: {e}")))?;
  let own = i16::from(currency.digits());
  let (min, max) = match (digits(what, args, 2, "minimumFractionDigits")?, digits(what, args, 2, "maximumFractionDigits")?) {
    (None, None) => (own, own),
    (Some(min), None) => (min, own.max(min)),
    (None, Some(max)) => (own.min(max), max),
    (Some(min), Some(max)) => (min, max.max(min)),
  };
  round(&mut d, min, max);
  let formatter = made(&locale, 0, |intl| DecimalFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), Default::default()).map_err(|e| data(what, e)))?;
  let (digits, sign) = unsigned(&formatter, &d);
  Ok(Value::str(currency.format(&digits, sign)))
}

/// `intl.date(when, style?)`: a calendar date in UTC at `dateStyle`
/// `short`, `medium` (the default), `long` or `full`, or at the browser's
/// `weekday`, `year`, `month` and `day` options given as an object, which may
/// also name a `locale`. `when` is milliseconds since the epoch or an ISO
/// 8601 string.
pub(super) fn date(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.date";
  let ms = match args.first() {
    Some(Value::Str(s)) => super::time::parse_iso(s).ok_or_else(|| Fail::internal(format!("{what}: `{s}` is not an ISO 8601 date")))?,
    _ => number(what, args, 0)?,
  };
  let (y, m, d) = super::time::civil_from_ms(ms);
  let date = Date::try_new_iso(y as i32, m as u8, d as u8).map_err(|e| data(what, e))?;
  let locale = locale_for(what, ambient, args.get(1))?;
  let prefs = (&locale).into();
  let part = |name: &str| -> Result<Option<String>, Fail> {
    let Some(Value::Map(options)) = args.get(1) else { return Ok(None) };
    match options.get(name) {
      None | Some(Value::Null | Value::Undefined) => Ok(None),
      Some(Value::Str(s)) => Ok(Some(s.to_string())),
      Some(other) => Err(Fail::internal(format!("{what}: `{name}` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
    }
  };
  let (weekday, year, month, day) = (part("weekday")?, part("year")?, part("month")?, part("day")?);
  let parts = weekday.is_some() || year.is_some() || month.is_some() || day.is_some();
  if let Some(time_style) = part("timeStyle")? {
    let (precision, timed) = match time_style.as_str() {
      "short" => (TimePrecision::Minute, 16),
      "medium" => (TimePrecision::Second, 32),
      other => return Err(Fail::internal(format!("{what}: `timeStyle: \"{other}\"` is not read; short and medium are, since long and full name the time zone"))),
    };
    if parts {
      return Err(Fail::internal(format!("{what}: `timeStyle` beside a weekday, year, month or day part")));
    }
    let of_day = ms.rem_euclid(86_400_000.0) as u64;
    let time = Time::try_new((of_day / 3_600_000) as u8, (of_day / 60_000 % 60) as u8, (of_day / 1000 % 60) as u8, 0).map_err(|e| data(what, e))?;
    let at = DateTime { date, time };
    let date_style = part("style")?;
    let out = match date_style.as_deref() {
      None => made(&locale, timed + 2, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, T::medium().with_time_precision(precision)).map_err(|e| data(what, e)))?.format(&at).parts_out(),
      Some("short") => made(&locale, timed + 1, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, YMDT::short().with_time_precision(precision)).map_err(|e| data(what, e)))?.format(&at).parts_out(),
      Some("medium") => made(&locale, timed + 2, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, YMDT::medium().with_time_precision(precision)).map_err(|e| data(what, e)))?.format(&at).parts_out(),
      Some("long") => made(&locale, timed + 3, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, YMDT::long().with_time_precision(precision)).map_err(|e| data(what, e)))?.format(&at).parts_out(),
      Some("full") => made(&locale, timed + 3, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, YMDET::long().with_time_precision(precision)).map_err(|e| data(what, e)))?.format(&at).parts_out(),
      Some(other) => return Err(Fail::internal(format!("{what}: `{other}` is not a style; short, medium, long or full"))),
    };
    // Chrome writes an ordinary space where CLDR 42 and later put U+202F before the day period.
    return Ok(Value::str(out.replace('\u{202f}', " ")));
  }
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
        Length::Long => made(&locale, 3, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, $set::long()).map_err(|e| data(what, e)))?.format(&date).parts_out(),
        Length::Medium => made(&locale, 2, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, $set::medium()).map_err(|e| data(what, e)))?.format(&date).parts_out(),
        Length::Short => made(&locale, 1, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), prefs, $set::short()).map_err(|e| data(what, e)))?.format(&date).parts_out(),
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

/// `toLocaleTimeString(locale?)` of an instant in this machine's local time zone: the time CLDR's patterns write on the clock the locale's region keeps. `Invalid Date` for an invalid instant. Without a locale, the system's.
pub fn local_time(ms: f64, locale: Option<&str>) -> Result<String, Fail> {
  let offset = super::time::local_offset(ms)?;
  numeric_written("toLocaleTimeString", ms - offset * 60_000.0, &locale.map_or_else(system_locale, str::to_owned), "time")
}

/// The locale ICU takes as the default, as node runs it: the one `LC_ALL`, then `LC_MESSAGES`, then `LANG` names, with `en_US.UTF-8` read as `en-US`. `C`, `POSIX` and none at all are `en-US`.
fn system_locale() -> String {
  let named = ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()));
  let Some(named) = named else { return "en-US".to_owned() };
  let tag = named.split(['.', '@']).next().unwrap_or_default().replace('_', "-");
  match tag.as_str() {
    "" | "C" | "POSIX" => "en-US".to_owned(),
    _ => tag,
  }
}

/// `toLocaleDateString(locale?)` of an instant in this machine's local time zone with no options, `toLocaleString(locale?)` with `with_time`: the numeric date ICU writes by default, then the time to the second. A calendar `snapfire_i18n` does not write, named by a `-u-ca-` keyword, is written by ICU4X. `Invalid Date` for an invalid instant. Without a locale, the system's.
pub fn local_date(ms: f64, locale: Option<&str>, with_time: bool) -> Result<String, Fail> {
  let what = if with_time { "toLocaleString" } else { "toLocaleDateString" };
  let offset = super::time::local_offset(ms)?;
  numeric_written(what, ms - offset * 60_000.0, &locale.map_or_else(system_locale, str::to_owned), if with_time { "both" } else { "date" })
}

/// The UTC fields of `local` as JavaScript writes a date with no options other than a time zone: the numeric date for `mode` `"date"`, the time for `"time"` and both joined for `"both"`. `Invalid Date` for NaN.
pub(super) fn numeric_written(what: &str, local: f64, tag: &str, mode: &str) -> Result<String, Fail> {
  if local.is_nan() {
    return Ok("Invalid Date".to_owned());
  }
  tag.parse::<Locale>().map_err(|_| Fail::internal(format!("RangeError: {what}: `{tag}` is not a locale")))?;
  let locale = intl()?.locale(what, tag)?;
  let tag = &locale.to_string();
  let (y, m, d) = super::time::civil_from_ms(local);
  let (h, min, s, _) = super::time::clock_from_ms(local);
  let tables = intl()?.tables();
  let clock = || tables.clock(tag, h as u32, min as u32, s as u32);
  if mode == "time" {
    return Ok(clock());
  }
  let out = match intl()?.tables().numeric_date(tag, y, m, d) {
    Some(written) if mode == "both" => intl()?.tables().date_with_time(tag, &written, &clock()),
    Some(written) => written,
    None => {
      let date = Date::try_new_iso(y as i32, m as u8, d as u8).map_err(|e| data(what, e))?;
      let time = Time::try_new(h as u8, min as u8, s as u8, 0).map_err(|e| data(what, e))?;
      match mode {
        "both" => made(&locale, 64, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), YMDT::short().with_year_style(YearStyle::Full).with_time_precision(TimePrecision::Second)).map_err(|e| data(what, e)))?.format(&DateTime { date, time }).to_string(),
        _ => made(&locale, 64, |intl| DateTimeFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), YMD::short().with_year_style(YearStyle::Full)).map_err(|e| data(what, e)))?.format(&date).to_string(),
      }
    }
  };
  Ok(out.replace('\u{202f}', " "))
}

/// The zone rules of the time zone source in use.
pub(super) struct Zones(pub(super) snapfire_time::Source);

impl snapfire_i18n::ZoneRules for Zones {
  fn observed(&self, zone: &str, ms: f64) -> Option<(i32, i32)> {
    snapfire_time::observed_in(self.0, zone, ms).ok().map(|o| (o.offset, o.saving))
  }

  fn saving_in_year(&self, zone: &str, ms: f64) -> Option<i32> {
    snapfire_time::saving_in_year(self.0, zone, ms).ok().flatten()
  }
}

/// An instant in this machine's zone: the local milliseconds, the offset in seconds and the zone's name in the system locale, its metazone read at `now`. `None` for an invalid instant.
fn local_parts(ms: f64, now: f64) -> Result<Option<(f64, i32, String)>, Fail> {
  if !(ms.abs() <= 8.64e15) {
    return Ok(None);
  }
  let source = snapfire_time::source().map_err(|e| Fail::internal(e.to_string()))?;
  let zone = snapfire_time::system_zone();
  let rules = Zones(source);
  let offset = match snapfire_time::observed_in(source, zone, ms) {
    Ok(observed) => observed.offset,
    Err(snapfire_time::Error::UnknownZone(_)) => 0,
    Err(e) => return Err(Fail::internal(e.to_string())),
  };
  let daylight = intl()?.tables().in_daylight(zone, snapfire_time::equivalent_time(ms), &rules);
  let system = system_locale();
  let system = intl()?.locale("Date", &system).map_or(system, |locale| locale.to_string());
  let name = intl()?.tables().zone_name(&system, zone, daylight, now, &rules);
  Ok(Some((ms.floor() + f64::from(offset) * 1000.0, offset, name)))
}

/// `HH:MM:SS GMT-0400 (Eastern Daylight Time)`, the offset cut to whole minutes as V8 cuts it.
fn time_text(local: f64, offset: i32, name: &str) -> String {
  let (h, m, s, _) = super::time::clock_from_ms(local);
  let minutes = offset / 60;
  let sign = if minutes < 0 { '-' } else { '+' };
  format!("{h:02}:{m:02}:{s:02} GMT{sign}{:02}{:02} ({name})", minutes.abs() / 60, minutes.abs() % 60)
}

/// `date.toTimeString()` in this machine's zone and locale; `Invalid Date` for an invalid instant.
pub fn time_string(ms: f64, now: f64) -> Result<String, Fail> {
  Ok(match local_parts(ms, now)? {
    Some((local, offset, name)) => time_text(local, offset, &name),
    None => "Invalid Date".to_owned(),
  })
}

/// `date.toString()` in this machine's zone and locale, `Tue Sep 10 2024 19:37:20 GMT-0400 (Eastern Daylight Time)`; `Invalid Date` for an invalid instant.
pub fn date_time_string(ms: f64, now: f64) -> Result<String, Fail> {
  const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  let Some((local, offset, name)) = local_parts(ms, now)? else { return Ok("Invalid Date".to_owned()) };
  let (year, month, day) = super::time::civil_from_ms(local);
  let weekday = ((local / 86_400_000.0).floor() as i64 + 4).rem_euclid(7) as usize;
  let year = if year < 0 { format!("-{:04}", -year) } else { format!("{year:04}") };
  Ok(format!("{} {} {day:02} {year} {}", DAYS[weekday], MONTHS[month as usize - 1], time_text(local, offset, &name)))
}

enum Length {
  Long,
  Medium,
  Short,
}

/// `intl.plural(n, { type?, locale? })`: the category `Intl.PluralRules` selects, `zero`, `one`, `two`, `few`, `many` or `other`, cardinal unless `type` is `ordinal`.
fn plural(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.plural";
  let locale = locale_for(what, ambient, args.get(1))?;
  let rules = match string_option(what, args, 1, "type")?.as_deref() {
    None | Some("cardinal") => made(&locale, 0, |intl| PluralRules::try_new_cardinal_with_buffer_provider(intl.icu(), (&locale).into()).map_err(|e| data(what, e))),
    Some("ordinal") => made(&locale, 1, |intl| PluralRules::try_new_ordinal_with_buffer_provider(intl.icu(), (&locale).into()).map_err(|e| data(what, e))),
    Some(other) => return Err(Fail::internal(format!("{what}: `type` is cardinal or ordinal, got `{other}`"))),
  }?;
  Ok(Value::str(select(&rules, args.first().unwrap_or(&Value::Null))?))
}

/// The cardinal plural category of `n` under the ambient locale.
pub fn category(ambient: &Ambient, n: &Value) -> Result<String, Fail> {
  let locale = locale_of(ambient);
  let rules = made(&locale, 0, |intl| PluralRules::try_new_cardinal_with_buffer_provider(intl.icu(), (&locale).into()).map_err(|e| data("intl.plural", e)))?;
  select(&rules, n)
}

fn select(rules: &PluralRules, n: &Value) -> Result<String, Fail> {
  let operands = match decimal("intl.plural", std::slice::from_ref(n), 0)? {
    Ok(d) => PluralOperands::from(&d),
    Err(_) => return Ok("other".to_owned()),
  };
  Ok(format!("{:?}", rules.category_for(operands)).to_ascii_lowercase())
}

/// A string field of an optional options object.
fn string_option(what: &str, args: &[Value], i: usize, field: &str) -> Result<Option<String>, Fail> {
  match option(what, args, i, field)? {
    None => Ok(None),
    Some(Value::Str(s)) => Ok(Some(s.to_string())),
    Some(other) => Err(Fail::internal(format!("{what}: `{field}` wants a string, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  }
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
      Some(Value::Null | Value::Undefined) | None => Ok(None),
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
  let wide = ListFormatterOptions::default().with_length(length);
  let width = list_width(length);
  let formatter = match read("type")?.as_deref() {
    None | Some("conjunction") => made(&locale, width, |intl| ListFormatter::try_new_and_with_buffer_provider(intl.icu(), (&locale).into(), wide).map_err(|e| data(what, e))),
    Some("disjunction") => made(&locale, 4 + width, |intl| ListFormatter::try_new_or_with_buffer_provider(intl.icu(), (&locale).into(), wide).map_err(|e| data(what, e))),
    Some("unit") => made(&locale, 8 + width, |intl| ListFormatter::try_new_unit_with_buffer_provider(intl.icu(), (&locale).into(), wide).map_err(|e| data(what, e))),
    Some(other) => return Err(Fail::internal(format!("{what}: `type` is conjunction, disjunction or unit, got `{other}`"))),
  }?;
  Ok(Value::str(formatter.format(items.iter().map(String::as_str)).parts_out()))
}

/// `intl.listParts(items, options)`: `formatToParts` of an `Intl.ListFormat`, each item an `element` and the text between them a `literal`.
fn list_parts(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  PARTS.with(|parts| *parts.borrow_mut() = Some(Vec::new()));
  let written = list(ambient, args);
  let collected = PARTS.with(|parts| parts.borrow_mut().take()).unwrap_or_default();
  written?;
  Ok(parts_value(collected))
}

/// Collected parts as the array of `{ type, value }` objects `formatToParts` answers.
fn parts_value(parts: Vec<(String, String)>) -> Value {
  Value::seq(parts.into_iter().map(|(kind, value)| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("type".to_owned(), Value::str(kind));
    map.insert("value".to_owned(), Value::str(value));
    Value::Map(map)
  }).collect::<Vec<_>>())
}

/// `intl.compare(a, b, { locale?, sensitivity?, numeric?, ignorePunctuation?, caseFirst? })`: -1, 0 or 1 as `new Intl.Collator(locale, options).compare(a, b)` answers. `sensitivity` is `base`, `accent`, `case` or `variant` (the default); `numeric` orders runs of digits by their value; `ignorePunctuation` skips punctuation and spaces; `caseFirst` is `upper`, `lower` or `false`.
fn compare(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.compare";
  let a = text(what, args, 0)?;
  let b = text(what, args, 1)?;
  let collator = collator(what, ambient, args, 2)?;
  let collator = collator.as_borrowed();
  Ok(Value::F64(match collator.compare(&a, &b) {
    std::cmp::Ordering::Less => -1.0,
    std::cmp::Ordering::Equal => 0.0,
    std::cmp::Ordering::Greater => 1.0,
  }))
}

/// The collator `new Intl.Collator(locale, options)` makes, from the options at `args[at]`. A locale's own defaults stand where an option is not given: Thai ignores punctuation and Danish and Maltese sort capitals first.
fn collator(what: &str, ambient: &Ambient, args: &[Value], at: usize) -> Result<std::rc::Rc<Collator>, Fail> {
  let locale = locale_for(what, ambient, args.get(at))?;
  let mut prefs: CollatorPreferences = (&locale).into();
  let mut key = 0u64;
  match option(what, args, at, "numeric")? {
    None => {}
    Some(Value::Bool(numeric)) => {
      key |= if *numeric { 1 } else { 2 };
      prefs.numeric_ordering = Some(if *numeric { CollationNumericOrdering::True } else { CollationNumericOrdering::False });
    }
    Some(other) => return Err(Fail::internal(format!("{what}: `numeric` wants a boolean, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  }
  match string_option(what, args, at, "caseFirst")?.as_deref() {
    None => {}
    Some("upper") => {
      key |= 4;
      prefs.case_first = Some(CollationCaseFirst::Upper);
    }
    Some("lower") => {
      key |= 8;
      prefs.case_first = Some(CollationCaseFirst::Lower);
    }
    Some("false") => {
      key |= 12;
      prefs.case_first = Some(CollationCaseFirst::False);
    }
    Some(other) => return Err(Fail::internal(format!("RangeError: {what}: `caseFirst` is upper, lower or false, got `{other}`"))),
  }
  let mut options = CollatorOptions::default();
  match string_option(what, args, at, "sensitivity")?.as_deref() {
    None | Some("variant") => {}
    Some("base") => {
      key |= 16;
      options.strength = Some(Strength::Primary);
    }
    Some("accent") => {
      key |= 32;
      options.strength = Some(Strength::Secondary);
    }
    Some("case") => {
      key |= 48;
      options.strength = Some(Strength::Primary);
      options.case_level = Some(CaseLevel::On);
    }
    Some(other) => return Err(Fail::internal(format!("{what}: `sensitivity` is base, accent, case or variant, got `{other}`"))),
  }
  match option(what, args, at, "ignorePunctuation")? {
    None => {}
    Some(Value::Bool(ignore)) => {
      key |= if *ignore { 64 } else { 128 };
      options.alternate_handling = Some(if *ignore { AlternateHandling::Shifted } else { AlternateHandling::NonIgnorable });
    }
    Some(other) => return Err(Fail::internal(format!("{what}: `ignorePunctuation` wants a boolean, got {}", snapfire_fsr_core::ext::kind_name(other)))),
  }
  made(&locale, key, |intl| Collator::try_new_with_buffer_provider(intl.icu(), prefs, options).map_err(|e| data(what, e)))
}

/// `intl.relative(value, unit, { locale?, numeric?, style? })`: the phrase `new Intl.RelativeTimeFormat(locale, options).format(value, unit)` writes. `unit` is `second`, `minute`, `hour`, `day`, `week`, `month`, `quarter` or `year`, singular or plural; `numeric` is `always` (the default) or `auto`, which writes "yesterday" where the locale has a word; `style` is `long` (the default), `short` or `narrow`.
fn relative(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  Ok(Value::str(relative_written(ambient, args)?.0))
}

/// `intl.relativeParts(value, unit, options)`: `formatToParts` of an `Intl.RelativeTimeFormat`, the number's parts carrying the singular `unit` and the text around it a `literal`.
fn relative_parts(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let unit = text("intl.relativeParts", args, 1)?.trim_end_matches('s').to_owned();
  let (phrase, number) = relative_written(ambient, args)?;
  let Some((digits, parts)) = number else { return Ok(parts_value(vec![("literal".to_owned(), phrase)])) };
  let at = phrase.find(&digits).ok_or_else(|| Fail::internal("intl.relativeParts: the number is not in the text it was written into"))?;
  let mut out = Vec::new();
  let literal = |piece: &str| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("type".to_owned(), Value::str("literal"));
    map.insert("value".to_owned(), Value::str(piece));
    Value::Map(map)
  };
  if at > 0 {
    out.push(literal(&phrase[..at]));
  }
  for (kind, value) in parts {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("type".to_owned(), Value::str(kind));
    map.insert("value".to_owned(), Value::str(value));
    map.insert("unit".to_owned(), Value::str(unit.as_str()));
    out.push(Value::Map(map));
  }
  if at + digits.len() < phrase.len() {
    out.push(literal(&phrase[at + digits.len()..]));
  }
  Ok(Value::seq(out))
}

/// The phrase [`relative`] writes and, when it holds a number, the number's text and parts.
fn relative_written(ambient: &Ambient, args: &[Value]) -> Result<(String, Option<(String, Vec<(String, String)>)>), Fail> {
  let what = "intl.relative";
  let mut value = match decimal(what, args, 0)? {
    Ok(d) => d,
    Err(text) => return Err(Fail::internal(format!("{what}: {text} is not a finite number"))),
  };
  round(&mut value, 0, 3);
  let name = text(what, args, 1)?;
  let auto = match string_option(what, args, 2, "numeric")?.as_deref() {
    None | Some("always") => false,
    Some("auto") => true,
    Some(other) => return Err(Fail::internal(format!("{what}: `numeric` is always or auto, got `{other}`"))),
  };
  let width = match string_option(what, args, 2, "style")?.as_deref() {
    None | Some("long") => Width::Long,
    Some("short") => Width::Short,
    Some("narrow") => Width::Narrow,
    Some(other) => return Err(Fail::internal(format!("{what}: `style` is long, short or narrow, got `{other}`"))),
  };
  let Some(unit) = Unit::from_name(&name) else {
    return Err(Fail::internal(format!("{what}: `{name}` is not a unit; second, minute, hour, day, week, month, quarter or year")));
  };
  let locale = locale_for(what, ambient, args.get(2))?;
  let tag = locale.to_string();
  let relative = intl()?.tables().relative(&tag, unit, width);
  let past = value.sign() == fixed_decimal::Sign::Negative;
  value.set_sign(fixed_decimal::Sign::None);
  let whole = value.absolute.magnitude_range() == (0..=0);
  if auto && whole {
    let digit = value.absolute.digit_at(0) as i8;
    if let Some(word) = relative.literal(if past { -digit } else { digit }) {
      return Ok((word.to_owned(), None));
    }
  }
  let rules = made(&locale, 0, |intl| PluralRules::try_new_cardinal_with_buffer_provider(intl.icu(), (&locale).into()).map_err(|e| data(what, e)))?;
  let plural = match rules.category_for(PluralOperands::from(&value)) {
    PluralCategory::Zero => Plural::Zero,
    PluralCategory::One => Plural::One,
    PluralCategory::Two => Plural::Two,
    PluralCategory::Few => Plural::Few,
    PluralCategory::Many => Plural::Many,
    PluralCategory::Other => Plural::Other,
  };
  let exact = match (whole, value.absolute.digit_at(0)) {
    (true, digit @ (0 | 1)) => Some(digit),
    _ => None,
  };
  let formatter = made(&locale, 0, |intl| DecimalFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), Default::default()).map_err(|e| data(what, e)))?;
  let formatted = formatter.format(&value);
  let mut sink = Collector { parts: Vec::new(), current: None };
  let _ = writeable::Writeable::write_to_parts(&formatted, &mut sink);
  let digits = formatted.to_string();
  Ok((relative.format(&digits, past, plural, exact), Some((digits, sink.parts))))
}

std::thread_local! {
  /// The parts the formatting under way collects for `formatToParts`, when it asked for them.
  static PARTS: std::cell::RefCell<Option<Vec<(String, String)>>> = const { std::cell::RefCell::new(None) };
}

/// A formatted value's text, collecting its parts into [`PARTS`] when `formatToParts` asked for them.
trait PartsOut {
  fn parts_out(&self) -> String;
}

impl<T: writeable::Writeable> PartsOut for T {
  fn parts_out(&self) -> String {
    PARTS.with(|parts| {
      if let Some(collected) = parts.borrow_mut().as_mut() {
        let mut sink = Collector { parts: Vec::new(), current: None };
        let _ = self.write_to_parts(&mut sink);
        collected.extend(sink.parts);
      }
    });
    self.write_to_string().into_owned()
  }
}

/// Collects written text by the part each piece sits in, the outermost part naming it: a year's digits are `year`, not `integer`. A group separator inside an integer is a `group`, as JavaScript splits it.
struct Collector {
  parts: Vec<(String, String)>,
  current: Option<&'static str>,
}

impl std::fmt::Write for Collector {
  fn write_str(&mut self, text: &str) -> std::fmt::Result {
    if text.is_empty() {
      return Ok(());
    }
    let kind = self.current.unwrap_or("literal");
    match self.parts.last_mut() {
      Some((last, value)) if last == kind => value.push_str(text),
      _ => self.parts.push((kind.to_owned(), text.to_owned())),
    }
    Ok(())
  }
}

impl writeable::PartsWrite for Collector {
  type SubPartsWrite = Self;

  fn with_part(&mut self, part: writeable::Part, mut f: impl FnMut(&mut Self) -> std::fmt::Result) -> std::fmt::Result {
    let outer = self.current;
    if outer.is_none() || (outer == Some("integer") && part == icu_decimal::parts::GROUP) {
      self.current = Some(part_name(part));
    }
    let written = f(self);
    self.current = outer;
    written
  }
}

/// The `type` JavaScript's `formatToParts` gives the ICU4X part.
fn part_name(part: writeable::Part) -> &'static str {
  use icu_datetime::parts as d;
  use icu_decimal::parts as n;
  match part {
    p if p == d::ERA => "era",
    p if p == d::YEAR || p == d::EXTENDED_YEAR => "year",
    p if p == d::RELATED_YEAR => "relatedYear",
    p if p == d::YEAR_NAME => "yearName",
    p if p == d::MONTH => "month",
    p if p == d::DAY => "day",
    p if p == d::WEEKDAY => "weekday",
    p if p == d::DAY_PERIOD => "dayPeriod",
    p if p == d::HOUR => "hour",
    p if p == d::MINUTE => "minute",
    p if p == d::SECOND => "second",
    p if p == d::TIME_ZONE_NAME => "timeZoneName",
    p if p == n::INTEGER => "integer",
    p if p == n::FRACTION => "fraction",
    p if p == n::GROUP => "group",
    p if p == n::DECIMAL => "decimal",
    p if p == n::MINUS_SIGN => "minusSign",
    p if p == n::PLUS_SIGN => "plusSign",
    p if p == icu_list::parts::ELEMENT => "element",
    _ => "literal",
  }
}

/// `intl.dateParts(when, options)`: `formatToParts` of an `Intl.DateTimeFormat`, the parts `intl.date` writes as `{ type, value }`.
fn date_parts(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  PARTS.with(|parts| *parts.borrow_mut() = Some(Vec::new()));
  let written = date(ambient, args);
  let collected = PARTS.with(|parts| parts.borrow_mut().take()).unwrap_or_default();
  written?;
  Ok(Value::seq(collected.into_iter().map(|(kind, value)| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("type".to_owned(), Value::str(kind));
    map.insert("value".to_owned(), Value::str(value.replace('\u{202f}', " ")));
    Value::Map(map)
  }).collect::<Vec<_>>()))
}

/// `intl.numberParts(text, style, options)`: `formatToParts` of an `Intl.NumberFormat`, the `text` it wrote read back into parts. The locale's group and decimal separators are the ones ICU4X writes for 1234.5. Digits before the decimal separator are `integer` and after it `fraction`, a separator between them is `group`, the signs are `minusSign` and `plusSign`, a percent style's sign is `percentSign`, a currency style's other text is `currency` and a space is `literal`.
fn number_parts(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.numberParts";
  let written = text(what, args, 0)?;
  let style = text(what, args, 1)?;
  let locale = locale_for(what, ambient, args.get(2))?;
  let probe = made(&locale, 0, |intl| DecimalFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), DecimalFormatterOptions::default()).map_err(|e| data(what, e)))?;
  let mut sample = Decimal::from(12345);
  sample.multiply_pow10(-1);
  PARTS.with(|parts| *parts.borrow_mut() = Some(Vec::new()));
  let _ = probe.format(&sample).parts_out();
  let symbols = PARTS.with(|parts| parts.borrow_mut().take()).unwrap_or_default();
  let symbol = |kind: &str| symbols.iter().find(|(k, _)| k == kind).map(|(_, v)| v.clone()).unwrap_or_default();
  let group = symbol("group");
  let decimal = symbol("decimal");
  let mut parts: Vec<(&'static str, String)> = Vec::new();
  let mut seen_decimal = false;
  let mut push = |kind: &'static str, text: &str| match parts.last_mut() {
    Some((last, value)) if *last == kind && !matches!(kind, "group" | "decimal") => value.push_str(text),
    _ => parts.push((kind, text.to_owned())),
  };
  let mut rest = written;
  while let Some(c) = rest.chars().next() {
    let width = c.len_utf8();
    let after = &rest[width..];
    let next_digit = after.chars().next().is_some_and(|n| n.is_numeric());
    if c.is_numeric() {
      push(if seen_decimal { "fraction" } else { "integer" }, &rest[..width]);
      rest = after;
    } else if !decimal.is_empty() && rest.starts_with(decimal.as_str()) && next_digit {
      seen_decimal = true;
      push("decimal", &decimal);
      rest = &rest[decimal.len()..];
    } else if !group.is_empty() && rest.starts_with(group.as_str()) && rest[group.len()..].chars().next().is_some_and(|n| n.is_numeric()) && !seen_decimal {
      push("group", &group);
      rest = &rest[group.len()..];
    } else if matches!(c, '-' | '\u{2212}') {
      push("minusSign", &rest[..width]);
      rest = after;
    } else if c == '+' {
      push("plusSign", &rest[..width]);
      rest = after;
    } else if c.is_whitespace() || matches!(c, '\u{a0}' | '\u{202f}' | '\u{200e}' | '\u{200f}' | '\u{61c}') {
      push("literal", &rest[..width]);
      rest = after;
    } else {
      push(match style { "percent" => "percentSign", "currency" => "currency", _ => "literal" }, &rest[..width]);
      rest = after;
    }
  }
  Ok(Value::seq(parts.into_iter().map(|(kind, value)| {
    let mut map = snapfire_fsr_core::ValueMap::default();
    map.insert("type".to_owned(), Value::str(kind));
    map.insert("value".to_owned(), Value::str(value));
    Value::Map(map)
  }).collect::<Vec<_>>()))
}

/// `intl.locale(tag, op)`: `"canonical"` the tag `Intl.getCanonicalLocales` gives, `"maximize"` and `"minimize"` the tags `Intl.Locale`'s methods give and `"parse"` an `Intl.Locale`'s fields with its tag under `$tag`. `"getCalendars"`, `"getHourCycles"`, `"getNumberingSystems"`, `"getTextInfo"`, `"getTimeZones"` and `"getWeekInfo"` answer what those methods of an `Intl.Locale` answer. A tag that does not parse is a `RangeError`, as JavaScript throws.
fn locale_op(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.locale";
  let tag = text(what, args, 0)?;
  let op = text(what, args, 1)?;
  let mut locale: Locale = tag.parse().map_err(|_| Fail::internal(format!("RangeError: Incorrect locale information provided")))?;
  let intl = intl()?;
  icu_locale::LocaleCanonicalizer::try_new_extended_with_buffer_provider(intl.icu()).map_err(|e| data(what, e))?.canonicalize(&mut locale);
  let expander = icu_locale::LocaleExpander::try_new_extended_with_buffer_provider(intl.icu()).map_err(|e| data(what, e))?;
  let keyword = |key: &str| key.parse().ok().and_then(|k| locale.extensions.unicode.keywords.get(&k)).map(|v| v.to_string()).filter(|v| !v.is_empty());
  let list = |items: Vec<String>| Value::Seq(items.into_iter().map(Value::str).collect::<Vec<_>>().into());
  let maximized = || {
    let mut id = locale.id.clone();
    expander.maximize(&mut id);
    id
  };
  match op {
    "canonical" => Ok(Value::str(locale.to_string())),
    "getCalendars" => Ok(list(match keyword("ca") {
      Some(calendar) => vec![calendar],
      None => intl.tables().preferred_calendars(maximized().region.as_ref().map_or("001", |r| r.as_str())).iter().map(|c| (*c).to_owned()).collect(),
    })),
    "getHourCycles" => Ok(list(vec![keyword("hc").unwrap_or_else(|| intl.tables().hour_cycle(&locale.to_string()).to_owned())])),
    "getNumberingSystems" => Ok(list(vec![keyword("nu").unwrap_or_else(|| intl.tables().numbering_system(&locale.to_string()).to_owned())])),
    "getTextInfo" => {
      let direction = match icu_locale::LocaleDirectionality::try_new_extended_with_buffer_provider(intl.icu()).map_err(|e| data(what, e))?.get(&locale.id) {
        Some(icu_locale::Direction::RightToLeft) => "rtl",
        _ => "ltr",
      };
      let mut map = snapfire_fsr_core::ValueMap::default();
      map.insert("direction".to_owned(), Value::str(direction));
      Ok(Value::Map(map))
    }
    "getTimeZones" => Ok(match locale.id.region {
      Some(region) => list(intl.tables().region_zones(region.as_str()).iter().map(|z| (*z).to_owned()).collect()),
      None => Value::Undefined,
    }),
    "getWeekInfo" => {
      let mut regional = locale.clone();
      regional.id = maximized();
      let week = icu_calendar::week::WeekInformation::try_new_with_buffer_provider(intl.icu(), (&regional).into()).map_err(|e| data(what, e))?;
      let mut weekend: Vec<f64> = week.weekend().map(|day| f64::from(day as u8)).collect();
      weekend.sort_by(f64::total_cmp);
      let mut map = snapfire_fsr_core::ValueMap::default();
      map.insert("firstDay".to_owned(), Value::F64(f64::from(week.first_weekday as u8)));
      map.insert("weekend".to_owned(), Value::Seq(weekend.into_iter().map(Value::F64).collect::<Vec<_>>().into()));
      Ok(Value::Map(map))
    }
    "maximize" | "minimize" => {
      if op == "maximize" { expander.maximize(&mut locale.id) } else { expander.minimize(&mut locale.id) };
      Ok(Value::str(locale.to_string()))
    }
    _ => {
      let mut map = snapfire_fsr_core::ValueMap::default();
      map.insert("$tag".to_owned(), Value::str(locale.to_string()));
      map.insert("baseName".to_owned(), Value::str(locale.id.to_string()));
      map.insert("language".to_owned(), Value::str(locale.id.language.as_str()));
      if let Some(script) = locale.id.script {
        map.insert("script".to_owned(), Value::str(script.as_str()));
      }
      if let Some(region) = locale.id.region {
        map.insert("region".to_owned(), Value::str(region.as_str()));
      }
      for (key, field) in [("ca", "calendar"), ("co", "collation"), ("hc", "hourCycle"), ("kf", "caseFirst"), ("nu", "numberingSystem")] {
        if let Some(value) = key.parse().ok().and_then(|k| locale.extensions.unicode.keywords.get(&k)) {
          map.insert(field.to_owned(), Value::str(value.to_string()));
        }
      }
      let numeric = "kn".parse().ok().and_then(|k| locale.extensions.unicode.keywords.get(&k)).is_some_and(|v| v.to_string().is_empty() || v.to_string() == "true");
      map.insert("numeric".to_owned(), Value::Bool(numeric));
      Ok(Value::Map(map.with_kind(snapfire_fsr_core::Collection::Opaque)))
    }
  }
}

/// `intl.duration(duration, { locale?, style? })`: `new Intl.DurationFormat(locale, { style }).format(duration)`. Each unit the duration holds is written in its CLDR pattern for the style and its plural form. The units are joined as a unit list. `digital` writes years to days in their short patterns, then hours, minutes and seconds as numbers with the locale's time separator and anything below a second as their fraction.
pub(super) fn duration(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.duration";
  let Some(Value::Map(given)) = args.first() else { return Err(Fail::internal(format!("TypeError: {what}: a duration is an object"))) };
  let amount = |key: &str| -> f64 {
    match given.get(key) {
      Some(Value::Int(n)) => *n as f64,
      Some(Value::UInt(n)) => *n as f64,
      Some(Value::F64(n)) => *n,
      Some(Value::F32(n)) => f64::from(*n),
      _ => 0.0,
    }
  };
  let style = match args.get(1) {
    Some(Value::Map(o)) => match o.get("style") {
      Some(Value::Str(s)) => s.to_string(),
      _ => "short".to_owned(),
    },
    _ => "short".to_owned(),
  };
  let locale = locale_for(what, ambient, args.get(1))?;
  let tag = locale.to_string();
  let numbers = made(&locale, 0, |intl| DecimalFormatter::try_new_with_buffer_provider(intl.icu(), (&locale).into(), DecimalFormatterOptions::default()).map_err(|e| data(what, e)))?;
  let plurals = made(&locale, 0, |intl| PluralRules::try_new_cardinal_with_buffer_provider(intl.icu(), (&locale).into()).map_err(|e| data(what, e)))?;
  let unit_width = if style == "digital" { "short" } else { style.as_str() };
  let written = |unit: &str, n: f64| -> Result<String, Fail> {
    let decimal = Decimal::try_from_f64(n, FloatPrecision::RoundTrip).map_err(|e| Fail::internal(format!("{what}: {e:?}")))?;
    let plural = match plurals.category_for(&decimal) {
      PluralCategory::Zero => "zero",
      PluralCategory::One => "one",
      PluralCategory::Two => "two",
      PluralCategory::Few => "few",
      PluralCategory::Many => "many",
      PluralCategory::Other => "other",
    };
    let pattern = intl()?.tables().duration_unit(&tag, unit_width, unit, plural).ok_or_else(|| Fail::internal(format!("{what}: no `{unit}` pattern for {tag}")))?;
    Ok(pattern.replace("{0}", &numbers.format(&decimal).to_string()))
  };
  let mut items = Vec::new();
  let date_units = [("years", "year"), ("months", "month"), ("weeks", "week"), ("days", "day")];
  let time_units = [("hours", "hour"), ("minutes", "minute"), ("seconds", "second"), ("milliseconds", "millisecond"), ("microseconds", "microsecond"), ("nanoseconds", "nanosecond")];
  for (key, unit) in date_units {
    if amount(key) != 0.0 {
      items.push(written(unit, amount(key))?);
    }
  }
  if style == "digital" {
    let below = amount("milliseconds") * 1e6 + amount("microseconds") * 1e3 + amount("nanoseconds");
    let separator = intl()?.tables().time_separator(&tag);
    let mut clock = format!("{}{separator}{:02}{separator}{:02}", amount("hours"), amount("minutes"), amount("seconds"));
    if below != 0.0 {
      let fraction = format!("{:09}", below as u64);
      clock.push('.');
      clock.push_str(fraction.trim_end_matches('0'));
    }
    items.push(clock);
  } else {
    for (key, unit) in time_units {
      if amount(key) != 0.0 {
        items.push(written(unit, amount(key))?);
      }
    }
  }
  let length = match style.as_str() {
    "long" => ListLength::Wide,
    "narrow" => ListLength::Narrow,
    _ => ListLength::Short,
  };
  let list = made(&locale, 8 + list_width(length), |intl| ListFormatter::try_new_unit_with_buffer_provider(intl.icu(), (&locale).into(), ListFormatterOptions::default().with_length(length)).map_err(|e| data(what, e)))?;
  Ok(Value::str(list.format(items.iter().map(String::as_str)).to_string()))
}

/// `intl.resolvedOptions(formatter, options)`: what `resolvedOptions()` reports for an `Intl.DateTimeFormat` or an `Intl.NumberFormat` made with `options`, its locale under `locale` and a date formatter's zone under `timeZone`, with every default the options leave out and in the order node reports them.
fn resolved_options(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let tables = intl()?.tables();
  let what = "intl.resolvedOptions";
  let formatter = text(what, args, 0)?;
  let options = args.get(1);
  let tag = locale_for(what, ambient, options)?.to_string();
  let given = |key: &str| match options {
    Some(Value::Map(map)) => map.get(key).filter(|v| !matches!(v, Value::Undefined)).cloned(),
    _ => None,
  };
  let word = |key: &str| match given(key) {
    Some(Value::Str(s)) => Some(s.to_string()),
    _ => None,
  };
  let mut out = snapfire_fsr_core::ValueMap::default();
  out.insert("locale".to_owned(), Value::str(tag.as_str()));
  if formatter == "DateTimeFormat" {
    let calendar = word("calendar").unwrap_or_else(|| match tables.calendar(&tag) {
      Some(snapfire_i18n::Calendar::Persian) => "persian".to_owned(),
      Some(snapfire_i18n::Calendar::Buddhist) => "buddhist".to_owned(),
      Some(snapfire_i18n::Calendar::Gregorian) => "gregory".to_owned(),
      None => tag.split("-u-").nth(1).and_then(|keys| keys.split('-').skip_while(|k| *k != "ca").nth(1)).unwrap_or("gregory").to_owned(),
    });
    out.insert("calendar".to_owned(), Value::str(calendar));
    out.insert("numberingSystem".to_owned(), Value::str(word("numberingSystem").unwrap_or_else(|| tables.numbering_system(&tag).to_owned())));
    if let Some(zone) = given("timeZone") {
      out.insert("timeZone".to_owned(), zone);
    }
    const FIELDS: [&str; 10] = ["weekday", "era", "year", "month", "day", "dayPeriod", "hour", "minute", "second", "fractionalSecondDigits"];
    let styled = given("dateStyle").is_some() || given("timeStyle").is_some();
    let hour = given("hour").is_some();
    let cycle = match (word("hourCycle"), given("hour12")) {
      (_, Some(Value::Bool(true))) => if tables.hour_cycle(&tag) == "h11" { "h11" } else { "h12" }.to_owned(),
      (_, Some(Value::Bool(false))) => if tables.hour_cycle(&tag) == "h24" { "h24" } else { "h23" }.to_owned(),
      (Some(cycle), _) => cycle,
      _ => tables.hour_cycle(&tag).to_owned(),
    };
    if hour || given("timeStyle").is_some() {
      out.insert("hourCycle".to_owned(), Value::str(cycle.as_str()));
      out.insert("hour12".to_owned(), Value::Bool(matches!(cycle.as_str(), "h11" | "h12")));
    }
    if !styled && !FIELDS.iter().any(|f| given(f).is_some()) {
      for field in ["year", "month", "day"] {
        out.insert(field.to_owned(), Value::str("numeric"));
      }
    }
    let widths = tables.time_widths(&tag, &cycle, given("minute").is_some(), given("second").is_some());
    for (field, at) in FIELDS.iter().zip(0..) {
      let Some(value) = given(field) else { continue };
      let value = match (hour, at) {
        (true, 6..=8) if widths[at - 6] => Value::str("2-digit"),
        (true, 6..=8) if matches!(&value, Value::Str(s) if s.as_str() == "2-digit") => value,
        (true, 6..=8) => Value::str("numeric"),
        _ => value,
      };
      out.insert((*field).to_owned(), value);
    }
    for field in ["timeZoneName", "dateStyle", "timeStyle"] {
      if let Some(value) = given(field) {
        out.insert(field.to_owned(), value);
      }
    }
    return Ok(Value::Map(out));
  }
  let text_or = |key: &str, default: &str| Value::str(word(key).unwrap_or_else(|| default.to_owned()));
  match formatter {
    "Collator" => {
      let resolved = collator(what, ambient, args, 1)?.as_borrowed().resolved_options();
      let keyword = |key: &str| tag.split("-u-").nth(1).and_then(|keys| keys.split('-').skip_while(|k| *k != key).nth(1)).map(str::to_owned);
      out.insert("usage".to_owned(), text_or("usage", "sort"));
      out.insert("sensitivity".to_owned(), text_or("sensitivity", "variant"));
      out.insert("ignorePunctuation".to_owned(), Value::Bool(resolved.alternate_handling == AlternateHandling::Shifted));
      out.insert("collation".to_owned(), Value::str(keyword("co").unwrap_or_else(|| "default".to_owned())));
      out.insert("numeric".to_owned(), Value::Bool(resolved.numeric == CollationNumericOrdering::True));
      let case_first = match resolved.case_first {
        CollationCaseFirst::Upper => "upper",
        CollationCaseFirst::Lower => "lower",
        _ => "false",
      };
      out.insert("caseFirst".to_owned(), Value::str(case_first));
      return Ok(Value::Map(out));
    }
    "ListFormat" => {
      out.insert("type".to_owned(), text_or("type", "conjunction"));
      out.insert("style".to_owned(), text_or("style", "long"));
      return Ok(Value::Map(out));
    }
    "Segmenter" => {
      out.insert("granularity".to_owned(), text_or("granularity", "grapheme"));
      return Ok(Value::Map(out));
    }
    "DisplayNames" => {
      let kind = word("type").ok_or_else(|| Fail::internal("TypeError: Required option 'type' is missing"))?;
      out.insert("style".to_owned(), text_or("style", "long"));
      out.insert("type".to_owned(), Value::str(kind.as_str()));
      out.insert("fallback".to_owned(), text_or("fallback", "code"));
      if kind == "language" {
        out.insert("languageDisplay".to_owned(), text_or("languageDisplay", "dialect"));
      }
      return Ok(Value::Map(out));
    }
    "RelativeTimeFormat" => {
      out.insert("style".to_owned(), text_or("style", "long"));
      out.insert("numeric".to_owned(), text_or("numeric", "always"));
      out.insert("numberingSystem".to_owned(), Value::str(word("numberingSystem").unwrap_or_else(|| tables.numbering_system(&tag).to_owned())));
      return Ok(Value::Map(out));
    }
    "DurationFormat" => {
      const UNITS: [&str; 10] = ["years", "months", "weeks", "days", "hours", "minutes", "seconds", "milliseconds", "microseconds", "nanoseconds"];
      out.insert("numberingSystem".to_owned(), Value::str(word("numberingSystem").unwrap_or_else(|| tables.numbering_system(&tag).to_owned())));
      let style = word("style").unwrap_or_else(|| "short".to_owned());
      out.insert("style".to_owned(), Value::str(style.as_str()));
      for (unit, at) in UNITS.iter().zip(0..) {
        let (width, display) = match (style.as_str(), at) {
          ("digital", 0..=3) => ("short", "auto"),
          ("digital", 4) => ("numeric", "always"),
          ("digital", 5 | 6) => ("2-digit", "always"),
          ("digital", _) => ("numeric", "auto"),
          (other, _) => (other, "auto"),
        };
        out.insert((*unit).to_owned(), Value::str(width));
        out.insert(format!("{unit}Display"), Value::str(display));
      }
      if let Some(digits) = given("fractionalDigits") {
        out.insert("fractionalDigits".to_owned(), digits);
      }
      return Ok(Value::Map(out));
    }
    "PluralRules" => {
      let ordinal = word("type").as_deref() == Some("ordinal");
      let mut numbers = args.to_vec();
      numbers[0] = Value::str("NumberFormat");
      let Value::Map(number) = resolved_options(ambient, &numbers)? else { return Err(Fail::internal(format!("{what}: the number options are not an object"))) };
      out.insert("type".to_owned(), Value::str(if ordinal { "ordinal" } else { "cardinal" }));
      let compact = word("notation").as_deref() == Some("compact") && given("minimumFractionDigits").is_none() && given("maximumFractionDigits").is_none();
      for key in ["notation", "compactDisplay", "minimumIntegerDigits", "minimumFractionDigits", "maximumFractionDigits", "minimumSignificantDigits", "maximumSignificantDigits"] {
        if compact && key.ends_with("FractionDigits") {
          continue;
        }
        if let Some(value) = number.get(key) {
          out.insert(key.to_owned(), value.clone());
        }
      }
      let locale: Locale = tag.parse().map_err(|_| Fail::internal(format!("{what}: `{tag}` is not a locale")))?;
      let rules = made(&locale, u64::from(ordinal), |intl| if ordinal { PluralRules::try_new_ordinal_with_buffer_provider(intl.icu(), (&locale).into()) } else { PluralRules::try_new_cardinal_with_buffer_provider(intl.icu(), (&locale).into()) }.map_err(|e| data(what, e)))?;
      let order = [PluralCategory::Zero, PluralCategory::One, PluralCategory::Two, PluralCategory::Few, PluralCategory::Many, PluralCategory::Other];
      let held: Vec<PluralCategory> = rules.categories().collect();
      let categories = order.iter().filter(|c| held.contains(c)).map(|c| Value::str(format!("{c:?}").to_ascii_lowercase())).collect::<Vec<_>>();
      out.insert("pluralCategories".to_owned(), Value::seq(categories));
      for key in ["roundingIncrement", "roundingMode", "roundingPriority", "trailingZeroDisplay"] {
        if let Some(value) = number.get(key) {
          out.insert(key.to_owned(), value.clone());
        }
      }
      return Ok(Value::Map(out));
    }
    _ => {}
  }
  let int = |key: &str| match given(key) {
    Some(Value::Int(n)) => Some(n as f64),
    Some(Value::F64(n)) => Some(n),
    Some(Value::F32(n)) => Some(f64::from(n)),
    _ => None,
  };
  out.insert("numberingSystem".to_owned(), Value::str(word("numberingSystem").unwrap_or_else(|| tables.numbering_system(&tag).to_owned())));
  let style = word("style").unwrap_or_else(|| "decimal".to_owned());
  out.insert("style".to_owned(), Value::str(style.as_str()));
  let compact = word("notation").as_deref() == Some("compact");
  let (mut low, mut high) = (0.0, if style == "percent" { 0.0 } else { 3.0 });
  match style.as_str() {
    "currency" => {
      let code = word("currency").ok_or_else(|| Fail::internal("TypeError: Currency code is required with currency style."))?;
      let digits = tables.currency(&tag, &code, snapfire_i18n::Display::Symbol).map_or(2, |c| c.digits());
      (low, high) = (f64::from(digits), f64::from(digits));
      out.insert("currency".to_owned(), Value::str(code.to_uppercase()));
      out.insert("currencyDisplay".to_owned(), Value::str(word("currencyDisplay").unwrap_or_else(|| "symbol".to_owned())));
      out.insert("currencySign".to_owned(), Value::str(word("currencySign").unwrap_or_else(|| "standard".to_owned())));
    }
    "unit" => {
      out.insert("unit".to_owned(), given("unit").unwrap_or(Value::Undefined));
      out.insert("unitDisplay".to_owned(), Value::str(word("unitDisplay").unwrap_or_else(|| "short".to_owned())));
    }
    _ => {}
  }
  out.insert("minimumIntegerDigits".to_owned(), Value::F64(int("minimumIntegerDigits").unwrap_or(1.0)));
  let significant = int("minimumSignificantDigits").is_some() || int("maximumSignificantDigits").is_some();
  if compact && !significant && int("minimumFractionDigits").is_none() && int("maximumFractionDigits").is_none() {
    out.insert("minimumFractionDigits".to_owned(), Value::F64(0.0));
    out.insert("maximumFractionDigits".to_owned(), Value::F64(0.0));
    out.insert("minimumSignificantDigits".to_owned(), Value::F64(1.0));
    out.insert("maximumSignificantDigits".to_owned(), Value::F64(2.0));
  } else if significant {
    out.insert("minimumSignificantDigits".to_owned(), Value::F64(int("minimumSignificantDigits").unwrap_or(1.0)));
    out.insert("maximumSignificantDigits".to_owned(), Value::F64(int("maximumSignificantDigits").unwrap_or(21.0)));
  } else {
    match (int("minimumFractionDigits"), int("maximumFractionDigits")) {
      (Some(min), Some(max)) => (low, high) = (min, max),
      (Some(min), None) => (low, high) = (min, high.max(min)),
      (None, Some(max)) => (low, high) = (low.min(max), max),
      (None, None) => {}
    }
    out.insert("minimumFractionDigits".to_owned(), Value::F64(low));
    out.insert("maximumFractionDigits".to_owned(), Value::F64(high));
  }
  let grouping = match given("useGrouping") {
    Some(Value::Bool(false)) => Value::Bool(false),
    Some(Value::Bool(true)) => Value::str("always"),
    Some(Value::Str(s)) => Value::str(s.as_str()),
    _ => Value::str(if compact { "min2" } else { "auto" }),
  };
  out.insert("useGrouping".to_owned(), grouping);
  out.insert("notation".to_owned(), Value::str(word("notation").unwrap_or_else(|| "standard".to_owned())));
  if compact {
    out.insert("compactDisplay".to_owned(), Value::str(word("compactDisplay").unwrap_or_else(|| "short".to_owned())));
  }
  out.insert("signDisplay".to_owned(), Value::str(word("signDisplay").unwrap_or_else(|| "auto".to_owned())));
  out.insert("roundingIncrement".to_owned(), Value::F64(int("roundingIncrement").unwrap_or(1.0)));
  out.insert("roundingMode".to_owned(), Value::str(word("roundingMode").unwrap_or_else(|| "halfExpand".to_owned())));
  out.insert("roundingPriority".to_owned(), Value::str(word("roundingPriority").unwrap_or_else(|| if compact && !significant { "morePrecision" } else { "auto" }.to_owned())));
  out.insert("trailingZeroDisplay".to_owned(), Value::str(word("trailingZeroDisplay").unwrap_or_else(|| "auto".to_owned())));
  Ok(Value::Map(out))
}

/// `intl.displayNames(code, { locale?, type, style?, languageDisplay?, fallback? })`: `new Intl.DisplayNames(locale, options).of(code)`. A code CLDR does not name is itself under `fallback: "code"` (the default) and `undefined` under `"none"`.
fn display_names(ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "intl.displayNames";
  let code = text(what, args, 0)?;
  let read = |key: &str| -> Option<String> {
    match args.get(1) {
      Some(Value::Map(o)) => match o.get(key) {
        Some(Value::Str(s)) => Some(s.to_string()),
        _ => None,
      },
      _ => None,
    }
  };
  let kind_name = read("type").ok_or_else(|| Fail::internal("TypeError: Required option 'type' is missing"))?;
  let kind = snapfire_i18n::NameKind::from_type(&kind_name).ok_or_else(|| Fail::internal(format!("RangeError: Value {kind_name} out of range for Intl.DisplayNames options property type")))?;
  if kind == snapfire_i18n::NameKind::Language && code.split(['-', '_']).any(|part| part.len() == 1) {
    return Err(Fail::internal("RangeError: invalid_argument"));
  }
  let locale = locale_for(what, ambient, args.get(1))?;
  let style = read("style").unwrap_or_else(|| "long".to_owned());
  let standard = read("languageDisplay").as_deref() == Some("standard");
  match intl()?.tables().display_name(&locale.to_string(), kind, code, &style, standard) {
    Some(name) => Ok(Value::str(name)),
    None if read("fallback").as_deref() == Some("none") => Ok(Value::Undefined),
    None => Ok(Value::str(code)),
  }
}
