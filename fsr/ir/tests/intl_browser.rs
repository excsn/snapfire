//! The Rust half of each `Intl` form the lowerer reads, against what Chrome answers for the same call. `fixtures/intl_chrome.json` was recorded with `browserc eval` in HeadlessChrome 153: `k` names the form, `loc` the locale, `a` the arguments and `o` the options.

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Ambient, Extensions};

/// Where ICU4X's data is known to differ from Chrome's, by form, locale and options. Reported, not failed. ICU4X pads the day of an `en-IN` medium date; it glues an `es-ES` full date to its time with "a las" where Chrome writes a comma; icu_experimental 0.6's percent data writes the Latin `%` and LRM beside Arabic digits where Chrome writes `٪` and ALM.
const KNOWN: &[(&str, &str, &str)] = &[
  ("intl.date", "en-IN", r#"{"dateStyle":"medium","timeStyle":"short"}"#),
  ("intl.date", "es-ES", r#"{"dateStyle":"full","timeStyle":"medium"}"#),
  ("intl.number", "ar-EG", r#"{"style":"percent"}"#),
  ("intl.number", "ar-EG", r#"{"style":"percent","maximumFractionDigits":1}"#),
];

fn to_value(json: &serde_json::Value) -> Value {
  match json {
    serde_json::Value::Null => Value::Null,
    serde_json::Value::Bool(b) => Value::Bool(*b),
    serde_json::Value::Number(n) => Value::F64(n.as_f64().unwrap()),
    serde_json::Value::String(s) => Value::str(s.clone()),
    serde_json::Value::Array(items) => Value::Seq(items.iter().map(to_value).collect()),
    serde_json::Value::Object(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), to_value(v))).collect()),
  }
}

fn rendered(value: &Value) -> String {
  match value {
    Value::Str(s) => s.to_string(),
    Value::Int(n) => n.to_string(),
    Value::F64(f) => format!("{}", *f as i64),
    other => format!("{other:?}"),
  }
}

/// The member and arguments the lowerer writes for one case.
fn call(case: &serde_json::Value) -> (String, Vec<Value>) {
  let mut options = case["o"].as_object().cloned().unwrap_or_default();
  options.insert("locale".to_owned(), case["loc"].clone());
  if case["k"] == "date" {
    if let Some(style) = options.remove("dateStyle") {
      options.insert("style".to_owned(), style);
    }
  }
  let mut args: Vec<Value> = case["a"].as_array().unwrap().iter().map(to_value).collect();
  args.push(to_value(&serde_json::Value::Object(options)));
  (format!("intl.{}", case["k"].as_str().unwrap()), args)
}

#[test]
fn every_intl_form_the_lowerer_reads_formats_as_chrome_does() {
  let fixture: serde_json::Value = serde_json::from_str(include_str!("fixtures/intl_chrome.json")).unwrap();
  let registry = Extensions::standard();
  let ambient = Ambient::default().now(0).catalogs(None);
  let mut wrong = Vec::new();
  let mut known = Vec::new();
  for case in fixture["cases"].as_array().unwrap() {
    let (name, args) = call(case);
    let got = match registry.call(&name, &ambient, &args) {
      Ok(out) => rendered(&out),
      Err(e) => format!("fails: {}", e.message),
    };
    let want = match &case["out"] {
      serde_json::Value::String(s) => s.clone(),
      other => other.to_string(),
    };
    if got != want {
      let line = format!("{name} {} {} {}: {got:?}, Chrome {want:?}", case["loc"], case["a"], case["o"]);
      let loc = case["loc"].as_str().unwrap();
      let options = case["o"].to_string();
      match KNOWN.iter().any(|(k, l, o)| *k == name && *l == loc && *o == options) {
        true => known.push(line),
        false => wrong.push(line),
      }
    }
  }
  for line in &known {
    eprintln!("known CLDR difference: {line}");
  }
  assert!(wrong.is_empty(), "{} of {} differ:\n{}", wrong.len(), fixture["cases"].as_array().unwrap().len(), wrong.join("\n"));
}
