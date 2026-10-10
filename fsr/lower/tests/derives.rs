//! A module-level `derive` the build reads by what it means under every
//! spelling. The ones it leaves to the browser carry where and why.

use std::path::PathBuf;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_lower::component::{ComponentSet, LoweredDerive};

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const KEYS: &str = "import { key } from \"@snapfire/fsr-client/store\";\nexport const price = key<number>(\"order/price\");\nexport const qty = key<number>(\"order/qty\");\nexport const total = key<number>(\"order/total\");\n";

fn derives(files: &[(&str, &str)]) -> Result<Vec<LoweredDerive>, String> {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let dir: PathBuf = std::env::temp_dir().join(format!("fsr-derives-{}-{n}", std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  for (name, source) in [("src/keys.ts", KEYS)].iter().chain(files) {
    let path = dir.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, source).unwrap();
  }
  let lowered = ComponentSet::new(&dir).lower_derives("src/main.ts").map_err(|e| e.to_string());
  let _ = std::fs::remove_dir_all(&dir);
  lowered
}

fn computed(row: &LoweredDerive) -> Option<Value> {
  let store: ValueMap = [("order/price", 12), ("order/qty", 5)].into_iter().map(|(k, v)| (k.to_owned(), Value::Int(v))).collect();
  Interpreter::default().compute(&snapfire_fsr_runtime::RequestCtx::anonymous(Default::default()), row.body.as_ref()?, &store)
}

fn main_ts(header: &str, call: &str) -> String {
  format!("{header}\nimport {{ price, qty, total }} from \"./keys\";\n{call}(total, [price, qty], (read) => (read(price) ?? 0) * (read(qty) ?? 0));\n")
}

#[test]
fn every_spelling_of_derive_lowers_the_same_row() {
  for module in ["@snapfire/fsr-client/store", "@snapfire/fsr-client"] {
    let spellings = [
      (format!("import {{ derive }} from \"{module}\";"), "derive".to_owned(), None),
      (format!("import {{ derive as computeKey }} from \"{module}\";"), "computeKey".to_owned(), None),
      (format!("import * as store from \"{module}\";"), "store.derive".to_owned(), None),
      ("import { derive } from \"./relay\";".to_owned(), "derive".to_owned(), Some(format!("export {{ derive }} from \"{module}\";\n"))),
      (format!("import {{ derive }} from \"{module}\";\nconst computeKey = derive;"), "computeKey".to_owned(), None),
    ];
    for (header, call, relay) in spellings {
      let source = main_ts(&header, &call);
      let mut files = vec![("src/main.ts", source.as_str())];
      if let Some(relay) = &relay {
        files.push(("src/relay.ts", relay));
      }
      let rows = derives(&files).unwrap_or_else(|e| panic!("{call} from {module}: {e}"));
      assert_eq!(rows.len(), 1, "{call} from {module}: {rows:?}");
      assert_eq!((rows[0].key.as_str(), rows[0].sources.as_slice()), ("order/total", &["order/price".to_owned(), "order/qty".to_owned()][..]), "{call} from {module}");
      assert_eq!(computed(&rows[0]), Some(Value::Int(60)), "{call} from {module}: {:?}", rows[0]);
      assert_eq!((&rows[0].at, &rows[0].why), (&None, &None));
    }
  }
}

#[test]
fn a_block_compute_a_function_compute_and_literal_keys_lower() {
  let block = "import { derive } from \"@snapfire/fsr-client/store\";\nimport { price, qty, total } from \"./keys\";\nderive(total, [price, qty], (read) => {\n  const p = read(price) ?? 0;\n  if (p > 100) return 0;\n  return p * (read(qty) ?? 0);\n});\n";
  assert_eq!(computed(&derives(&[("src/main.ts", block)]).unwrap()[0]), Some(Value::Int(60)));
  let function = "import { derive } from \"@snapfire/fsr-client/store\";\nderive(\"order/total\", [\"order/price\", \"order/qty\"], function (get) {\n  return (get(\"order/price\") ?? 0) * (get(\"order/qty\") ?? 0);\n});\n";
  let rows = derives(&[("src/main.ts", function)]).unwrap();
  assert_eq!(rows[0].key, "order/total");
  assert_eq!(computed(&rows[0]), Some(Value::Int(60)));
}

#[test]
fn a_derive_inside_a_function_is_the_browser_s_with_where_and_why() {
  let source = "import { derive } from \"@snapfire/fsr-client/store\";\nimport { price, qty, total } from \"./keys\";\nexport function start() {\n  derive(total, [price, qty], (read) => (read(price) ?? 0) * (read(qty) ?? 0));\n}\nstart();\n";
  let rows = derives(&[("src/main.ts", source)]).unwrap();
  assert_eq!(rows.len(), 1);
  assert_eq!((rows[0].key.as_str(), rows[0].body.is_none()), ("order/total", true));
  assert_eq!(rows[0].at.as_deref(), Some("src/main.ts:4:3"));
  assert!(rows[0].why.as_deref().is_some_and(|why| why.contains("inside a function")), "{:?}", rows[0].why);
}

#[test]
fn a_compute_that_does_not_lower_is_the_browser_s_with_its_residue() {
  let source = "import { derive } from \"@snapfire/fsr-client/store\";\nimport { price, total } from \"./keys\";\nderive(total, [price], (read) => (read(price) ?? 0) + (Date.now() % 7));\n";
  let rows = derives(&[("src/main.ts", source)]).unwrap();
  assert_eq!((rows[0].key.as_str(), rows[0].body.is_none()), ("order/total", true));
  assert_eq!(rows[0].at.as_deref(), Some("src/main.ts:3:1"));
  assert!(rows[0].why.as_deref().is_some_and(|why| why.contains("the current time")), "{:?}", rows[0].why);
}

#[test]
fn a_derived_key_the_build_cannot_read_fails_the_build() {
  let source = "import { derive } from \"@snapfire/fsr-client/store\";\nimport { price } from \"./keys\";\nconst name = location.hash;\nderive(name, [price], (read) => read(price));\n";
  let refused = derives(&[("src/main.ts", source)]).unwrap_err();
  assert!(refused.contains("src/main.ts"), "{refused}");
}

#[test]
fn a_local_named_derive_is_not_the_store_s() {
  let source = "import { price, qty, total } from \"./keys\";\nfunction derive(..._args: unknown[]) {}\nderive(total, [price, qty], (read: (k: unknown) => number) => read(price));\n";
  assert_eq!(derives(&[("src/main.ts", source)]).unwrap(), Vec::new());
}
