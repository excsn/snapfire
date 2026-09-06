//! The two halves of every `render` member of the standard library on the
//! same inputs: the Rust half here, the browser half through node over the
//! client's built `dist/std.js`. Skipped, with a line saying so, when node
//! or the build is missing.

use std::path::PathBuf;
use std::process::Command;

use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Ambient, Extensions};

const LOCALES: &[&str] = &["en_US", "fr_FR", "de_DE", "es_ES", "pt_BR", "ja_JP", "en_IN", "ar_EG", ""];

/// Where the two CLDR snapshots are known to differ: ICU4X pads the day of
/// an `en-IN` medium date and the browser's ICU does not. Reported, not failed.
const KNOWN: &[(&str, &str)] = &[("en_IN", "intl.date")];

fn cases() -> Vec<(&'static str, Vec<serde_json::Value>)> {
  use serde_json::json;
  let day = 20_701.0 * 86_400_000.0 + 60_067_250.0;
  vec![
    ("intl.number", vec![json!(1234567.891)]),
    ("intl.number", vec![json!(-0.5)]),
    ("intl.number", vec![json!(1.23456)]),
    ("intl.number", vec![json!(5)]),
    ("intl.number", vec![json!(2.005)]),
    ("intl.number", vec![json!(0.1234567)]),
    ("intl.number", vec![json!(1e21)]),
    ("intl.number", vec![json!(2), json!({ "minimumFractionDigits": 2, "maximumFractionDigits": 2 })]),
    ("intl.number", vec![json!(2.345), json!({ "maximumFractionDigits": 2 })]),
    ("intl.number", vec![json!(1234.5678), json!({ "minimumFractionDigits": 1, "maximumFractionDigits": 1 })]),
    ("intl.currency", vec![json!(1234.5), json!("USD")]),
    ("intl.currency", vec![json!(1234.567), json!("EUR")]),
    ("intl.currency", vec![json!(1234.5), json!("JPY")]),
    ("intl.currency", vec![json!(0.005), json!("GBP")]),
    ("intl.currency", vec![json!(-42), json!("CHF")]),
    ("intl.date", vec![json!(day), json!("short")]),
    ("intl.date", vec![json!(day), json!("medium")]),
    ("intl.date", vec![json!(day), json!("long")]),
    ("intl.date", vec![json!(day), json!("full")]),
    ("intl.date", vec![json!("2026-02-28T23:30:00-02:00"), json!("medium")]),
    ("intl.plural", vec![json!(0)]),
    ("intl.plural", vec![json!(1)]),
    ("intl.plural", vec![json!(2)]),
    ("intl.plural", vec![json!(5)]),
    ("intl.plural", vec![json!(21)]),
    ("intl.plural", vec![json!(1.5)]),
    ("text.slug", vec![json!("  Crème Brûlée & Café! ")]),
    ("text.slug", vec![json!("Ünïcödé--stuff__2026 ß Straße")]),
    ("text.truncate", vec![json!("héllo wörld 🌍!"), json!(13)]),
    ("text.truncate", vec![json!("short"), json!(10)]),
    ("time.format", vec![json!(day), json!("YYYY-MM-DD HH:mm:ss.SSS")]),
    ("time.add", vec![json!(day), json!(36), json!("h")]),
    ("time.diff", vec![json!(day), json!(0), json!("d")]),
    ("time.parse", vec![json!("2026-09-05T16:41:07.250Z")]),
    ("time.parse", vec![json!("2026-09-05T18:41+02:00")]),
    ("time.parse", vec![json!("2026-09-05")]),
    ("time.parse", vec![json!("Sep 5 2026")]),
    ("crypto.hash", vec![json!("hello")]),
    ("crypto.hash", vec![json!("")]),
    ("crypto.hash", vec![json!("a".repeat(200))]),
    ("crypto.verify", vec![json!("hello"), json!("2CF24DBA5FB0A30E26E83B2AC5B9E29E1B161E5C1FA7425E73043362938B9824")]),
    ("crypto.verify", vec![json!("hello"), json!("nope")]),
  ]
}

fn to_value(json: &serde_json::Value) -> Value {
  match json {
    serde_json::Value::Null => Value::Null,
    serde_json::Value::Bool(b) => Value::Bool(*b),
    serde_json::Value::Number(n) => Value::F64(n.as_f64().unwrap()),
    serde_json::Value::String(s) => Value::Str(s.clone()),
    serde_json::Value::Array(items) => Value::Seq(items.iter().map(to_value).collect()),
    serde_json::Value::Object(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), to_value(v))).collect()),
  }
}

fn rendered(value: &Value) -> String {
  match value {
    Value::Str(s) => s.clone(),
    Value::Bool(b) => b.to_string(),
    Value::Null => "null".to_owned(),
    Value::F64(f) if f.fract() == 0.0 && f.abs() < 1e21 => format!("{}", *f as i64),
    Value::F64(f) => f.to_string(),
    other => format!("{other:?}"),
  }
}

#[test]
fn the_browser_half_matches_the_rust_half() {
  let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../client/dist/std.js");
  if !dist.is_file() {
    eprintln!("skipped: {} is not built", dist.display());
    return;
  }
  let Ok(which) = Command::new("node").arg("--version").output() else {
    eprintln!("skipped: node is not on PATH");
    return;
  };
  if !which.status.success() {
    eprintln!("skipped: node is not on PATH");
    return;
  }

  let registry = Extensions::standard();
  let mut inputs = Vec::new();
  let mut expected = Vec::new();
  for locale in LOCALES {
    for (name, args) in cases() {
      let values: Vec<Value> = args.iter().map(to_value).collect();
      let out = registry.call(name, &Ambient { locale: (*locale).to_owned(), now: 0, catalogs: None }, &values).unwrap_or_else(|e| panic!("{locale} {name}: {}", e.message));
      expected.push(rendered(&out));
      inputs.push(serde_json::json!({ "locale": locale, "name": name, "args": args }));
    }
  }

  let script = format!(
    r#"
import {{ setLocale }} from "{locale}";
import * as std from "{std}";
const cases = {cases};
const out = [];
for (const c of cases) {{
  setLocale(c.locale);
  const [module, member] = c.name.split(".");
  let value;
  try {{ value = std[module][member](...c.args); }} catch (e) {{ value = "throws: " + e.message; }}
  out.push(value === null ? "null" : typeof value === "string" ? value : JSON.stringify(value));
}}
process.stdout.write(JSON.stringify(out));
"#,
    locale = dist.with_file_name("locale.js").display(),
    std = dist.display(),
    cases = serde_json::Value::Array(inputs.clone())
  );
  let dir = std::env::temp_dir().join(format!("fsr-conformance-{}", std::process::id()));
  std::fs::create_dir_all(&dir).unwrap();
  let path = dir.join("run.mjs");
  std::fs::write(&path, script).unwrap();
  let output = Command::new("node").arg(&path).output().unwrap();
  let _ = std::fs::remove_dir_all(&dir);
  assert!(output.status.success(), "node failed: {}", String::from_utf8_lossy(&output.stderr));
  let actual: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
  assert_eq!(actual.len(), expected.len());

  let mut diffs = Vec::new();
  for ((input, rust), js) in inputs.iter().zip(&expected).zip(&actual) {
    if rust != js {
      let known = KNOWN.iter().any(|(locale, name)| input["locale"] == *locale && input["name"] == *name);
      let line = format!("{input}\n  rust: {rust:?}\n  js:   {js:?}");
      if known {
        eprintln!("known divergence: {line}");
      } else {
        diffs.push(line);
      }
    }
  }
  assert!(diffs.is_empty(), "{} of {} cases differ:\n{}", diffs.len(), expected.len(), diffs.join("\n"));
}
