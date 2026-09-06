use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{standard_reach, Ambient, Expr, Extensions, Interpreter, Reach, STANDARD};

fn call(locale: &str, name: &str, args: Vec<Value>) -> Result<Value, String> {
  let (module, member) = name.split_once('.').unwrap();
  Extensions::standard().call(name, &Ambient { locale: locale.to_owned(), now: 1_757_090_467_250, catalogs: None }, &args).map(|v| {
    let _ = (module, member);
    v
  }).map_err(|e| e.message)
}

fn s(text: &str) -> Value {
  Value::Str(text.to_owned())
}

fn n(f: f64) -> Value {
  Value::F64(f)
}

#[test]
fn the_standard_table_and_the_registry_agree() {
  let registry = Extensions::standard();
  for (module, name, reach) in STANDARD {
    let key = format!("{module}.{name}");
    let held = registry.get(&key).unwrap_or_else(|| panic!("{key} is registered"));
    assert_eq!(held.reach, *reach, "{key}");
    assert_eq!(standard_reach(module, name), Some(*reach));
  }
  assert_eq!(registry.names().len(), STANDARD.len());
  assert_eq!(standard_reach("intl", "money"), None);
  assert_eq!(Reach::Body.as_str(), "body");
}

#[test]
fn numbers_group_and_round_under_the_locale() {
  assert_eq!(call("en_US", "intl.number", vec![n(1234567.891)]), Ok(s("1,234,567.891")));
  assert_eq!(call("fr_FR", "intl.number", vec![n(1234567.891)]), Ok(s("1\u{202f}234\u{202f}567,891")));
  assert_eq!(call("de_DE", "intl.number", vec![n(1234567.891)]), Ok(s("1.234.567,891")));
  assert_eq!(call("en_US", "intl.number", vec![n(1.23456)]), Ok(s("1.235")));
  assert_eq!(call("en_US", "intl.number", vec![n(5.0)]), Ok(s("5")));
  assert_eq!(call("en_US", "intl.number", vec![n(-0.5)]), Ok(s("-0.5")));
  assert_eq!(call("en_US", "intl.number", vec![Value::Int(12345678901234567890)]), Ok(s("12,345,678,901,234,567,890")));
  assert_eq!(call("", "intl.number", vec![n(1000.0)]), Ok(s("1,000")), "no locale is `en`");
  let options = |min: f64, max: f64| {
    let mut map = snapfire_fsr_core::ValueMap::new();
    map.insert("minimumFractionDigits".to_owned(), n(min));
    map.insert("maximumFractionDigits".to_owned(), n(max));
    Value::Map(map)
  };
  assert_eq!(call("en_US", "intl.number", vec![n(2.0), options(2.0, 2.0)]), Ok(s("2.00")));
  assert_eq!(call("en_US", "intl.number", vec![n(2.345), options(0.0, 2.0)]), Ok(s("2.35")));
  assert_eq!(call("en_US", "intl.number", vec![n(f64::NAN)]), Ok(s("NaN")));
  assert_eq!(call("en_US", "intl.number", vec![n(f64::INFINITY)]), Ok(s("∞")));
  assert!(call("en_US", "intl.number", vec![s("x")]).unwrap_err().contains("a number"));
}

#[test]
fn currency_spells_the_code_with_its_own_digits() {
  assert_eq!(call("en_US", "intl.currency", vec![n(1234.5), s("USD")]), Ok(s("USD\u{a0}1,234.50")));
  assert_eq!(call("de_DE", "intl.currency", vec![n(1234.5), s("EUR")]), Ok(s("1.234,50\u{a0}EUR")));
  assert_eq!(call("ja_JP", "intl.currency", vec![n(1234.5), s("JPY")]), Ok(s("JPY\u{a0}1,235")));
  assert!(call("en_US", "intl.currency", vec![n(1.0), s("dollars")]).unwrap_err().contains("not a currency code"));
}

#[test]
fn dates_take_a_style_in_utc() {
  let day = 20_701.0 * 86_400_000.0 + 82_800_000.0;
  assert_eq!(call("en_US", "intl.date", vec![n(day)]), Ok(s("Sep 5, 2026")));
  assert_eq!(call("en_US", "intl.date", vec![n(day), s("short")]), Ok(s("9/5/26")));
  assert_eq!(call("fr_FR", "intl.date", vec![n(day), s("long")]), Ok(s("5 septembre 2026")));
  assert_eq!(call("de_DE", "intl.date", vec![n(day), s("full")]), Ok(s("Samstag, 5. September 2026")));
  assert_eq!(call("en_US", "intl.date", vec![s("2026-09-05T23:00:00Z"), s("medium")]), Ok(s("Sep 5, 2026")));
  assert!(call("en_US", "intl.date", vec![n(day), s("tiny")]).unwrap_err().contains("not a style"));
  assert!(call("en_US", "intl.date", vec![s("Sep 5")]).unwrap_err().contains("ISO 8601"));
}

#[test]
fn plural_categories_follow_the_locale() {
  assert_eq!(call("en_US", "intl.plural", vec![n(1.0)]), Ok(s("one")));
  assert_eq!(call("en_US", "intl.plural", vec![n(2.0)]), Ok(s("other")));
  assert_eq!(call("fr_FR", "intl.plural", vec![n(0.0)]), Ok(s("one")));
  assert_eq!(call("ar_EG", "intl.plural", vec![n(2.0)]), Ok(s("two")));
  assert_eq!(call("ar_EG", "intl.plural", vec![Value::Int(21)]), Ok(s("many")));
}

#[test]
fn text_slugs_and_truncates_by_code_point() {
  assert_eq!(call("", "text.slug", vec![s("  Crème Brûlée & Café! ")]), Ok(s("creme-brulee-cafe")));
  assert_eq!(call("", "text.slug", vec![s("Ünïcödé--stuff__2026")]), Ok(s("unicode-stuff-2026")));
  assert_eq!(call("", "text.truncate", vec![s("héllo wörld"), n(5.0)]), Ok(s("héllo…")));
  assert_eq!(call("", "text.truncate", vec![s("hi"), n(5.0)]), Ok(s("hi")));
  assert_eq!(call("", "text.truncate", vec![s("hello"), n(3.0), s("...")]), Ok(s("hel...")));
}

#[test]
fn time_is_utc_milliseconds() {
  let ms = 20_701.0 * 86_400_000.0 + 60_067_250.0;
  assert_eq!(call("", "time.format", vec![n(ms), s("YYYY-MM-DD HH:mm:ss")]), Ok(s("2026-09-05 16:41:07")));
  assert_eq!(call("", "time.add", vec![n(ms), n(2.0), s("d")]), Ok(n(ms + 172_800_000.0)));
  assert_eq!(call("", "time.diff", vec![n(ms + 90_000.0), n(ms), s("m")]), Ok(n(1.5)));
  assert_eq!(call("", "time.parse", vec![s("2026-09-05T16:41:07.250Z")]), Ok(n(ms)));
  assert_eq!(call("", "time.parse", vec![s("yesterday")]), Ok(Value::Null));
  assert_eq!(call("", "time.now", vec![]), Ok(Value::Int(1_757_090_467_250)));
  assert!(call("", "time.add", vec![n(ms), n(1.0), s("weeks")]).unwrap_err().contains("not a unit"));
}

#[test]
fn crypto_hashes_verifies_and_draws() {
  let hash = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
  assert_eq!(call("", "crypto.hash", vec![s("hello")]), Ok(s(hash)));
  assert_eq!(call("", "crypto.verify", vec![s("hello"), s(&hash.to_uppercase())]), Ok(Value::Bool(true)));
  assert_eq!(call("", "crypto.verify", vec![s("hello!"), s(hash)]), Ok(Value::Bool(false)));
  let Ok(Value::Str(random)) = call("", "crypto.random", vec![n(8.0)]) else { panic!() };
  assert_eq!(random.len(), 16);
  assert_ne!(call("", "crypto.random", vec![n(8.0)]), Ok(s(&random)));
}

#[test]
fn ids_are_uuid_v7() {
  let Ok(Value::Str(id)) = call("", "id.new", vec![]) else { panic!() };
  assert_eq!(id.len(), 36);
  assert_eq!(&id[14..15], "7");
  assert_ne!(call("", "id.new", vec![]), Ok(s(&id)));
}

#[tokio::test]
async fn an_ext_expression_runs_through_the_interpreter_under_its_locale() {
  let interpreter = Interpreter::default();
  let expr = Expr::ext("intl", "number", vec![Expr::Lit(snapfire_fsr_ir::Lit::Float(1234.5))]);
  assert_eq!(interpreter.evaluate(&expr, Vec::new()).await.unwrap(), s("1,234.5"), "a detached evaluation has no locale, which is `en`");

  let missing = Expr::ext("fmt", "pretty", vec![]);
  let err = interpreter.evaluate(&missing, Vec::new()).await.unwrap_err();
  assert!(err.message.contains("extension `fmt.pretty` is not registered"), "{}", err.message);

  let mut extensions = Extensions::standard();
  extensions.register("fmt.pretty", Reach::Render, |ambient, args| Ok(Value::Str(format!("{}:{}", ambient.bcp47(), args.len()))));
  let interpreter = Interpreter::default().with_extensions(Arc::new(extensions));
  assert_eq!(interpreter.evaluate(&missing, Vec::new()).await.unwrap(), s("en:0"));
  assert!(interpreter.extensions().contains("fmt.pretty"));
}

#[test]
fn t_reads_the_catalog_under_the_locale_with_plurals_and_placeholders() {
  use std::collections::BTreeMap;
  let table = |pairs: &[(&str, &str)]| pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect::<BTreeMap<_, _>>();
  let mut tables = BTreeMap::new();
  tables.insert("en_US".to_owned(), table(&[("hi", "Hello {name}"), ("items.one", "{count} item"), ("items.other", "{count} items"), ("only.en", "English only")]));
  tables.insert("fr_FR".to_owned(), table(&[("hi", "Bonjour {name}"), ("items.one", "{count} article"), ("items.other", "{count} articles")]));
  let catalogs = Arc::new(snapfire_fsr_ir::Catalogs::from_tables("en_US", tables));
  assert_eq!(catalogs.lookup("fr_FR", "only.en"), Some("English only"), "a locale reads the default's keys under its own");
  assert_eq!(catalogs.lookup("de_DE", "hi"), Some("Hello {name}"), "an unknown locale reads the default");
  assert!(catalogs.json("fr_FR").unwrap().contains("\"only.en\":\"English only\""), "the table shipped is the merged one");
  assert_eq!(catalogs.rows(), vec![("en_US".to_owned(), 4), ("fr_FR".to_owned(), 3)]);

  let registry = Extensions::standard();
  let call = |locale: &str, args: Vec<Value>| registry.call("i18n.t", &Ambient { locale: locale.to_owned(), now: 0, catalogs: Some(catalogs.clone()) }, &args).map_err(|e| e.message);
  let named = |pairs: Vec<(&str, Value)>| {
    let mut map = snapfire_fsr_core::ValueMap::new();
    for (k, v) in pairs {
      map.insert(k.to_owned(), v);
    }
    Value::Map(map)
  };
  assert_eq!(call("fr_FR", vec![s("hi"), named(vec![("name", s("Norm"))])]), Ok(s("Bonjour Norm")));
  assert_eq!(call("en_US", vec![s("hi")]), Ok(s("Hello {name}")), "an unfilled placeholder stays");
  assert_eq!(call("en_US", vec![s("items"), named(vec![("count", n(1.0))])]), Ok(s("1 item")));
  assert_eq!(call("fr_FR", vec![s("items"), named(vec![("count", n(0.0))])]), Ok(s("0 article")), "French counts zero as one");
  assert_eq!(call("en_US", vec![s("items"), named(vec![("count", Value::Int(12))])]), Ok(s("12 items")));
  assert_eq!(call("en_US", vec![s("missing.key")]), Ok(s("missing.key")));
  assert_eq!(registry.call("i18n.t", &Ambient::default(), &[s("hi")]).unwrap(), s("hi"), "no catalogs at all answers the key");
}
