//! The IR renderer against QuickJS `renderToString` on the storefront's own
//! pages, with the same props. Needs the network once, for react-dom/server;
//! everything else is what `fsr test app` already prepares. See
//! `fsr/docs/benches/render.md` for what each group measures and how to
//! record a run.

use std::path::Path;
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use futures::future::LocalBoxFuture;
use snapfire_fsr_cli::spec::{Prepared, prepare, test_bundle};
use snapfire_fsr_cli::vendor::{ESM_HOST, VendorManifest};
use snapfire_fsr_cli::xwpm::Layout;
use snapfire_fsr_cli::{Options, build};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_payload::value_to_json;

struct NoHooks;

impl Hooks for NoHooks {
  fn ctx(&self, _spec: &str) -> Result<u32, String> {
    Ok(0)
  }
  fn use_ctx(&self, _id: u32) -> Result<(), String> {
    Ok(())
  }
  fn session(&self, _id: u32) -> Result<String, String> {
    Ok("{}".to_owned())
  }
  fn calls(&self, _id: u32) -> Result<String, String> {
    Ok("[]".to_owned())
  }
  fn render(&self, _module: &str, _props: &str) -> Result<Option<String>, String> {
    Ok(None)
  }
  fn fetch(&self, _method: String, _url: String, _body: Option<String>, _headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse> {
    Box::pin(async { FetchResponse { status: 404, body: "{}".to_owned(), headers: Vec::new() } })
  }
}

struct Page {
  name: &'static str,
  module: &'static str,
  file: &'static str,
  props: ValueMap,
}

fn map(entries: Vec<(&str, Value)>) -> Value {
  Value::Map(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn product(id: i128, name: &str, category: &str, stock: i128) -> Value {
  map(vec![
    ("id", Value::Int(id)),
    ("name", Value::str(name)),
    ("brand", Value::str("Prusa")),
    ("category", Value::str(category)),
    ("price_cents", Value::Int(2400 + id * 100)),
    ("list_price_cents", Value::Int(2900 + id * 100)),
    ("image", map(vec![("color", Value::str("#e8d5b5")), ("emoji", Value::str("🧵"))])),
    ("rating", Value::F64(4.5)),
    ("reviews", Value::Int(12 * id)),
    ("stock", Value::Int(stock)),
    ("description", Value::str("A spool of filament for the printer on your desk, wound tight and dried before shipping.")),
    ("tags", Value::Seq(vec![Value::str("pla"), Value::str("1.75mm")])),
    ("attributes", Value::Seq(vec![map(vec![("name", Value::str("Ingredients")), ("value", Value::str("PLA"))]), map(vec![("name", Value::str("Weight")), ("value", Value::str("1 kg"))])])),
  ])
}

fn with_quantity(product: Value, quantity: i128) -> Value {
  let Value::Map(mut map) = product else { unreachable!() };
  map.insert("quantity".to_owned(), Value::Int(quantity));
  Value::Map(map)
}

fn pages() -> Vec<Page> {
  let catalog: Vec<Value> = (1..=12).map(|i| product(i, &format!("Filament {i}"), if i % 3 == 0 { "tools" } else { "printing" }, i % 4 * 3)).collect();
  let Value::Map(catalog_props) = map(vec![("products", Value::Seq(catalog)), ("q", Value::str("")), ("category", Value::str("printing")), ("cartCount", Value::Int(2))]) else { unreachable!() };
  let Value::Map(product_props) = map(vec![
    ("product", product(1, "PLA filament", "printing", 8)),
    ("stock", map(vec![("product_id", Value::Int(1)), ("on_hand", Value::Int(8)), ("reserved", Value::Int(0)), ("warehouse", Value::str("Prague")), ("bins", Value::Seq(vec![Value::str("A1"), Value::str("B2")]))])),
    ("inCart", Value::Int(0)),
    ("cartCount", Value::Int(2)),
  ]) else {
    unreachable!()
  };
  let lines: Vec<Value> = (1..=3).map(|i| with_quantity(product(i, &format!("Filament {i}"), "printing", 5), i)).collect();
  let Value::Map(cart_props) = map(vec![("lines", Value::Seq(lines)), ("cartCount", Value::Int(6))]) else { unreachable!() };
  vec![
    Page { name: "catalog_12", module: "routes/index/page.tsx#default", file: "routes/index/page.js", props: catalog_props },
    Page { name: "product", module: "routes/product/[id]/page.tsx#default", file: "routes/product/[id]/page.js", props: product_props },
    Page { name: "cart_3", module: "routes/cart/page.tsx#default", file: "routes/cart/page.js", props: cart_props },
  ]
}

fn machine_state() -> String {
  let power = Command::new("pmset").args(["-g"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.contains("powermode")).map(|l| l.trim().to_owned()).unwrap_or_default()).unwrap_or_default();
  let load = Command::new("uptime").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
  format!("{power}; {load}")
}

/// A module that renders one page with `renderToString` from props set on the global once.
fn bench_module(prepared: &Prepared, page: &Page) -> std::path::PathBuf {
  let path = prepared.test_dir.join(format!("bench-{}.js", page.name));
  let source = format!(
    "import {{ createElement }} from \"react\";\nimport {{ renderToString }} from \"react-dom/server\";\nimport {{ decodeValue }} from \"@snapfire/fsr-client\";\nimport Page from \"./dist/{}\";\nglobalThis.__decode = (json) => decodeValue(JSON.parse(json));\nglobalThis.__render = (props) => renderToString(createElement(Page, props));\n",
    page.file
  );
  std::fs::write(&path, source).expect("bench module");
  path
}

fn engine_for(resolution: &Resolution, dom: &Path, module: &Path, rt: &tokio::runtime::Runtime) -> Engine {
  let engine = Engine::new(resolution.clone(), dom, Rc::new(NoHooks), JsCalls::new()).expect("engine");
  let local = tokio::task::LocalSet::new();
  rt.block_on(local.run_until(engine.import(module))).expect("bench module loads");
  engine
}

fn bench(c: &mut Criterion) {
  eprintln!("machine before: {}", machine_state());
  let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("app");
  let workspace_snapfirec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/snapfirec");
  if std::env::var_os("SNAPFIREC").is_none() && workspace_snapfirec.is_file() {
    unsafe { std::env::set_var("SNAPFIREC", &workspace_snapfirec) };
  }
  let built = build(&app, &Options::default()).expect("fsr build app");
  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(c.body.clone()))).collect());
  let prepared = prepare(&app).expect("fsr test's preparation");
  let layout = Layout::of(&app).expect("layout");
  let react_dom = VendorManifest::read(&app, &layout).expect("vendor manifest").packages.get("react-dom").map(|p| p.version.clone()).expect("react-dom is vendored");
  let server = test_bundle(&app, "react-dom/server", &react_dom, &format!("{ESM_HOST}/react-dom@{react_dom}/server?target=es2022&bundle&external=react")).expect("react-dom/server");
  let mut resolution = prepared.resolution.clone();
  resolution.overrides.remove("react");
  resolution.overrides.remove("react/jsx-runtime");
  resolution.overrides.remove("react-dom/client");
  resolution.overrides.insert("react-dom/server".to_owned(), server);

  let interpreter = Interpreter::default();
  let rt = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
  let components_json = serde_json::to_string(&built.manifest.components).expect("components serialise");

  c.bench_function("ir/load_components", |b| b.iter(|| serde_json::from_str::<Vec<snapfire_fsr_plan::ComponentEntry>>(black_box(&components_json)).expect("parses")));

  for page in pages() {
    let component = components.get(page.module).cloned().expect("the page lowered");
    let ir_html = interpreter.render(&component, &page.props, &components).expect("ir renders").html;
    let module = bench_module(&prepared, &page);
    let engine = engine_for(&resolution, &prepared.dom, &module, &rt);
    let json = value_to_json(&Value::Map(page.props.clone())).to_string();
    engine.eval_string(&format!("globalThis.__json = {}; globalThis.__props = __decode(__json); ''", serde_json::to_string(&json).expect("a JSON string"))).expect("props set");
    let js_html = engine.eval_string("__render(__props)").expect("react renders");
    eprintln!("{}: ir {} bytes, react {} bytes, {}", page.name, ir_html.len(), js_html.len(), if ir_html == js_html { "identical" } else { "DIFFERENT" });
    std::fs::write(prepared.test_dir.join(format!("render-{}.ir.html", page.name)), &ir_html).expect("dump");
    std::fs::write(prepared.test_dir.join(format!("render-{}.react.html", page.name)), &js_html).expect("dump");

    c.bench_with_input(BenchmarkId::new("ir/render", page.name), &page, |b, page| b.iter(|| interpreter.render(black_box(&component), black_box(&page.props), &components).expect("ir renders")));
    c.bench_with_input(BenchmarkId::new("quickjs/render", page.name), &page, |b, _| b.iter(|| engine.eval_string("__render(__props)").expect("react renders")));
    c.bench_with_input(BenchmarkId::new("quickjs/render_with_decode", page.name), &page, |b, _| b.iter(|| engine.eval_string("__render(__decode(__json))").expect("react renders")));
    c.bench_with_input(BenchmarkId::new("quickjs/cold_context", page.name), &page, |b, _| b.iter(|| engine_for(&resolution, &prepared.dom, &module, &rt)));
  }
}

fn machine_after(_c: &mut Criterion) {
  eprintln!("machine after: {}", machine_state());
}

criterion_group!(benches, bench, machine_after);
criterion_main!(benches);
