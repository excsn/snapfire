//! Derived store keys on the server: a key a module-level `derive` computes is
//! in the store the server renders from, as it is in the browser's, so the
//! first paint shows what hydration will. GAPS 9.63.

use std::path::{Path, PathBuf};

use snapfire_fsr_cli::new::{create, NewOptions};
use snapfire_fsr_cli::{build, write, Options};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;

static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

const STORE: &str = "import { key } from \"@snapfire/fsr-client/store\";\nexport const price = key<number>(\"order/price\");\nexport const qty = key<number>(\"order/qty\");\nexport const total = key<number>(\"order/total\");\n";
const DERIVE: &str = "import { derive } from \"@snapfire/fsr-client/store\";\nimport { price, qty, total } from \"./store\";\nderive(total, [price, qty], (read) => (read(price) ?? 0) * (read(qty) ?? 0));\n";
const TOTAL: &str = "import { useStore } from \"@snapfire/fsr-client/react\";\nimport { total } from \"@src/store\";\nexport function Total() {\n  const [t] = useStore(total, 0);\n  return <p className=\"total\">Total: {t}</p>;\n}\n";
const PAGE: &str = "import { Island } from \"@snapfire/fsr-client/react\";\nimport { Total } from \"@src/Total\";\nexport default function Page() {\n  return <main><Island><Total /></Island></main>;\n}\n";
const LOADER: &str = "export async function load() {\n  return {};\n}\n\nexport const store = () => ({ \"order/price\": 12, \"order/qty\": 5 });\n";

/// A React application whose page places `Total` with `main` as its document entry.
fn project(tag: &str, main: &str, loader: &str) -> PathBuf {
  let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
  let root = std::env::temp_dir().join(format!("fsr-cli-derive-{}-{n}-{tag}", std::process::id()));
  let _ = std::fs::remove_dir_all(&root);
  create(&root, NewOptions { fetch: false, with: vec!["react".to_owned()], ..NewOptions::default() }).unwrap();
  let app = root.join("app");
  std::fs::write(app.join("importmap.json"), r#"{"imports":{"@snapfire/fsr-client":"/f","@snapfire/fsr-client/store":"/s","@snapfire/fsr-client/react":"/r","react":"/r","react-dom/client":"/d"}}"#).unwrap();
  std::fs::create_dir_all(app.join("vendor")).unwrap();
  std::fs::write(app.join("vendor/.fsr-vendor.json"), r#"{"packages":{"react":{"version":"19.1.0"},"react-dom":{"version":"19.1.0"}}}"#).unwrap();
  std::fs::create_dir_all(app.join("src")).unwrap();
  for (file, text) in [("src/store.ts", STORE), ("src/main.ts", main), ("src/Total.tsx", TOTAL), ("routes/page.tsx", PAGE), ("routes/page.loader.ts", loader)] {
    std::fs::write(app.join(file), text).unwrap();
  }
  root
}

/// `/` as the host built from `root` renders it, with the build's report.
fn render(root: &Path) -> (String, String) {
  let app = root.join("app");
  let built = build(&app, &Options::beside(&app)).unwrap_or_else(|e| panic!("builds: {e}"));
  write(&app, &built).unwrap();
  let host = Host::from_config(Config::load(root).unwrap()).unwrap().build().unwrap_or_else(|e| panic!("boots: {e}"));
  let html = tokio::runtime::Runtime::new().unwrap().block_on(host.render_to_string("/", RenderMode::Html, SessionCell::default())).unwrap_or_else(|e| panic!("renders: {e}"));
  (html, built.report.to_string())
}

fn shown(html: &str) -> String {
  let at = html.find("class=\"total\"").map(|i| &html[i..]).unwrap_or("");
  at.split("</p>").next().unwrap_or("").replace("<!-- -->", "")
}

#[test]
fn the_first_paint_holds_what_a_module_level_derive_computes() {
  let root = project("paint", DERIVE, LOADER);
  let (html, _) = render(&root);
  assert!(shown(&html).ends_with("Total: 60"), "the server computed 12 * 5: {}", shown(&html));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_key_both_seeded_and_derived_reads_as_derived_as_it_does_in_the_browser() {
  let seeded = LOADER.replace("\"order/qty\": 5 }", "\"order/qty\": 5, \"order/total\": 1 }");
  let root = project("precedence", DERIVE, &seeded);
  let (html, _) = render(&root);
  assert!(shown(&html).ends_with("Total: 60"), "{}", shown(&html));
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_derive_the_build_cannot_lower_holds_its_reader_and_the_report_says_why() {
  let main = DERIVE.replace("(read(price) ?? 0) * (read(qty) ?? 0)", "Date.now() % 7");
  let root = project("unlowered", &main, LOADER);
  let (html, report) = render(&root);
  assert!(!html.contains("Total: 0") && !html.contains("Total: <!-- -->0"), "no fallback painted: {}", shown(&html));
  assert!(report.contains("order/total") && report.contains("src/main.ts:3"), "{report}");
  let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_derive_written_inside_a_function_holds_its_reader_and_the_report_says_why() {
  let main = DERIVE.replace("derive(total, [price, qty], (read) => (read(price) ?? 0) * (read(qty) ?? 0));", "export function start() {\n  derive(total, [price, qty], (read) => (read(price) ?? 0) * (read(qty) ?? 0));\n}\nstart();");
  let root = project("nested", &main, LOADER);
  let (html, report) = render(&root);
  assert!(!html.contains("Total: 0") && !html.contains("Total: <!-- -->0"), "{}", shown(&html));
  assert!(report.contains("order/total") && report.contains("src/main.ts:4"), "{report}");
  let _ = std::fs::remove_dir_all(&root);
}

fn refused(main: &str) -> String {
  let root = project("refused", main, LOADER);
  let app = root.join("app");
  let refused = build(&app, &Options::beside(&app)).err().map(|e| e.to_string());
  let _ = std::fs::remove_dir_all(&root);
  refused.unwrap_or_else(|| panic!("the build refuses:\n{main}"))
}

#[test]
fn a_key_derived_twice_fails_the_build() {
  let main = format!("{DERIVE}derive(total, [price], (read) => read(price));\n");
  assert!(refused(&main).contains("`order/total` is derived in both"), "{}", refused(&main));
}

#[test]
fn derived_keys_that_feed_each_other_fail_the_build() {
  let main = format!("{DERIVE}derive(price, [total], (read) => read(total));\n");
  let why = refused(&main);
  assert!(why.contains("cycle") && why.contains("order/total") && why.contains("order/price"), "{why}");
}
