//! The locale data a build cuts formats the listed locales as every locale's data does and holds no other. This target holds one test, since what a program installed is the whole program's.

use snapfire_fsr_cli::intl;
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::intl_data::{Cut, IntlData};
use snapfire_fsr_ir::locales::Unlisted;
use snapfire_fsr_ir::{Ambient, Expr, Extensions, Interpreter};

fn call(locale: &str, name: &str, args: Vec<Value>) -> Result<Value, String> {
  Extensions::standard().call(name, &Ambient::default().locale(locale.to_owned()).now(1_757_090_467_250).catalogs(None), &args).map_err(|e| e.message)
}

fn s(text: &str) -> Value {
  Value::str(text.to_owned())
}

fn options(pairs: &[(&str, &str)]) -> Value {
  let mut map = snapfire_fsr_core::ValueMap::default();
  for (key, value) in pairs {
    map.insert((*key).to_owned(), s(value));
  }
  Value::Map(map)
}

/// What a set of calls writes under the data the program formats from.
fn written() -> Vec<Result<Value, String>> {
  let day = 1_788_566_400_000.0;
  let mut out = Vec::new();
  for locale in ["fr", "en-GB", "fr-CA", "th"] {
    out.push(call(locale, "intl.number", vec![Value::F64(1234567.891)]));
    out.push(call(locale, "intl.currency", vec![Value::F64(1234.5), s("EUR"), options(&[("display", "symbol")])]));
    out.push(call(locale, "intl.date", vec![Value::F64(day), s("full")]));
    out.push(call(locale, "intl.date", vec![Value::F64(day), options(&[("timeStyle", "short"), ("style", "long")])]));
    out.push(call(locale, "intl.plural", vec![Value::F64(1.0)]));
    out.push(call(locale, "intl.plural", vec![Value::F64(2.0), options(&[("type", "ordinal")])]));
    out.push(call(locale, "intl.list", vec![Value::Seq(vec![s("a"), s("b"), s("c")].into())]));
    out.push(call(locale, "intl.compare", vec![s("côte"), s("coté")]));
    out.push(call(locale, "intl.relative", vec![Value::F64(-1.0), s("day"), options(&[("numeric", "auto")])]));
    out.push(call(locale, "intl.displayNames", vec![s("US"), options(&[("type", "region")])]));
    out.push(call(locale, "intl.locale", vec![s(locale), s("maximize")]));
    out.push(call(locale, "intl.locale", vec![s(locale), s("getWeekInfo")]));
  }
  out
}

fn segments(granularity: &str, text: &str) -> Result<Value, String> {
  let body = Expr::Builtin { name: snapfire_fsr_ir::ast::Builtin::Segments, args: vec![Expr::lit_str(text), Expr::lit_str(granularity)] };
  let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
  runtime.block_on(Interpreter::default().evaluate(&body, Vec::new())).map_err(|e| e.to_string())
}

fn cut(locales: &[&str], default: &str) -> Cut {
  Cut::new(locales.iter().map(|tag| (*tag).to_owned()).collect(), default.to_owned(), Unlisted::Fallback).source(snapfire_fsr_ir::intl_data::compiled_source())
}

fn installed(bytes: Vec<u8>) {
  IntlData::from_static(Box::leak(bytes.into_boxed_slice())).unwrap().install();
}

#[test]
fn a_cut_formats_its_locales_as_every_locales_data_does() {
  let whole = written();
  assert!(whole.iter().all(Result::is_ok), "{whole:?}");
  let thai = "ภาษาไทยเป็นภาษาราชการ";
  let words = segments("word", thai).unwrap();
  let graphemes = segments("grapheme", thai).unwrap();

  let listed = cut(&["fr", "en-GB", "fr-CA", "th"], "fr");
  let bytes = intl::data(&listed, None).unwrap();
  let every = IntlData::packed_from_compiled(&listed).len();
  assert!(bytes.len() * 4 < every, "the cut is {} bytes of {every}", bytes.len());
  assert_eq!(IntlData::cut_of(&bytes), Ok(listed.clone()));
  assert_eq!(intl::data(&listed, Some(bytes.clone())).unwrap(), bytes, "a cut for the same is kept");
  assert_ne!(intl::data(&cut(&["fr"], "fr"), Some(bytes.clone())).unwrap(), bytes, "a cut for other locales is made again");

  installed(bytes);
  assert_eq!(written(), whole);
  assert_eq!(segments("grapheme", thai), Ok(graphemes));
  let refusal = segments("word", thai).unwrap_err();
  assert!(refusal.contains("Intl.Segmenter"), "{refusal}");

  installed(intl::data(&listed.clone().words(true), None).unwrap());
  assert_eq!(written(), whole);
  assert_eq!(segments("word", thai), Ok(words));

  installed(intl::data(&cut(&["en-GB"], "en-GB"), None).unwrap());
  assert_eq!(call("en-GB", "intl.number", vec![Value::F64(1234567.891), options(&[("locale", "fr")])]), Ok(s("1,234,567.891")), "a locale outside the cut formats in the default");
}

#[test]
fn a_list_the_host_would_refuse_is_refused_at_the_build() {
  let refusal = intl::data(&cut(&["en"], "fr"), None).unwrap_err().to_string();
  assert!(refusal.contains("[locales]") && refusal.contains("`fr`"), "{refusal}");
}

#[test]
fn a_locale_the_code_names_outside_the_cut_is_found_in_the_plan() {
  let plan = r#"(ext intl number "1" (obj (locale "de-DE")))
(ext intl date "1" (obj (locale "fr-CA")))
(ext intl number "1" (obj (locale "en-GB")))"#;
  assert_eq!(intl::unlisted(&cut(&["fr", "en-GB"], "fr"), plan), ["de-DE"]);
  assert!(intl::unlisted(&cut(&["fr", "en-GB", "de"], "fr"), plan).is_empty());
}
