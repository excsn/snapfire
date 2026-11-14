//! A program that installs locale data formats from it. This target holds one test, since what a program installed is the whole program's.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::intl_data::{Cut, IntlData, COMPILED_ICU};
use snapfire_fsr_ir::locales::Unlisted;
use snapfire_fsr_ir::{Ambient, Extensions};

fn packed(locales: &[&str], default: &str) -> &'static [u8] {
  let tables = snapfire_i18n::Data::embedded().trimmed(locales);
  let cut = Cut::new(locales.iter().map(|tag| (*tag).to_owned()).collect(), default.to_owned(), Unlisted::Fallback);
  Box::leak(IntlData::packed(COMPILED_ICU, &tables, &cut).into_boxed_slice())
}

fn call(locale: &str, name: &str, args: Vec<Value>) -> Result<Value, String> {
  Extensions::standard().call(name, &Ambient::default().locale(locale.to_owned()).now(1_757_090_467_250).catalogs(None), &args).map_err(|e| e.message)
}

fn options(pairs: &[(&str, &str)]) -> Value {
  let mut map = snapfire_fsr_core::ValueMap::default();
  for (key, value) in pairs {
    map.insert((*key).to_owned(), Value::str((*value).to_owned()));
  }
  Value::Map(map)
}

#[test]
fn a_program_formats_from_the_data_it_installed() {
  let bytes = packed(&["en-GB", "fr"], "fr");
  IntlData::from_static(bytes).unwrap().install();
  assert_eq!(IntlData::current().unwrap().cut().map(|cut| cut.default.as_str()), Some("fr"));

  let french = Value::str("1\u{202f}234\u{202f}567,891".to_owned());
  assert_eq!(call("fr", "intl.number", vec![Value::F64(1234567.891)]), Ok(french.clone()));
  assert_eq!(call("fr", "intl.number", vec![Value::F64(1234567.891), options(&[("locale", "de")])]), Ok(french.clone()), "an unlisted locale formats in the default");
  assert_eq!(call("fr", "intl.number", vec![Value::F64(1234567.891), options(&[("locale", "en-GB")])]), Ok(Value::str("1,234,567.891".to_owned())));
  assert_eq!(call("en-GB", "intl.currency", vec![Value::F64(1234.5), Value::str("GBP".to_owned()), options(&[("display", "symbol")])]), Ok(Value::str("£1,234.50".to_owned())));
  assert_eq!(call("fr", "intl.date", vec![Value::F64(1_788_566_400_000.0), options(&[("style", "long"), ("locale", "ja")])]), Ok(Value::str("5 septembre 2026".to_owned())));

  let resolved = call("fr", "intl.resolvedOptions", vec![Value::str("NumberFormat".to_owned()), options(&[("locale", "de-CH")])]).unwrap();
  let Value::Map(resolved) = resolved else { panic!("resolvedOptions gave {resolved:?}") };
  assert_eq!(resolved.get("locale"), Some(&Value::str("fr".to_owned())));

  IntlData::from_static(packed(&["en-GB"], "en-GB")).unwrap().install();
  assert_eq!(call("fr", "intl.number", vec![Value::F64(1234567.891), options(&[("locale", "fr")])]), Ok(Value::str("1,234,567.891".to_owned())), "what was installed last is what formats");
}
