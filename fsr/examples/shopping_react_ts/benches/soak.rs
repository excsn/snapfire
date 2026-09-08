//! Sustained load rather than a timed batch: N requesters hit the same page
//! flat out for a fixed window and the harness counts what came out. This is
//! the shape a server sees, and it is not criterion's shape, so this target
//! runs its own loop and prints renders per second.
//!
//! `cargo bench --features mimalloc --bench soak`, after `--bench render` has
//! prepared the app. `SOAK_SECONDS` overrides the ten second window.

use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use futures::future::LocalBoxFuture;
use snapfire_fsr_cli::spec::{prepare, test_bundle};
use snapfire_fsr_cli::vendor::{ESM_HOST, VendorManifest};
use snapfire_fsr_cli::xwpm::Layout;
use snapfire_fsr_cli::{Options, build};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_payload::value_to_json;

#[cfg(feature = "mimalloc")]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;

const REQUESTERS: &[usize] = &[1, 2, 4, 8];
const MODULE: &str = "routes/page.tsx#default";
const PAGE: &str = "catalog_12";

struct NoHooks {
  interpreter: Interpreter,
}

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
  fn locale(&self, _id: u32) -> Result<String, String> {
    Ok("en".to_owned())
  }
  fn calls(&self, _id: u32) -> Result<String, String> {
    Ok("[]".to_owned())
  }
  fn render(&self, _module: &str, _props: &str) -> Result<Option<String>, String> {
    Ok(None)
  }
  fn ext(&self, name: &str, args: &str, locale: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("{name}: {e}"))?;
    let args = match snapfire_fsr_payload::json_to_value(&json).map_err(|e| format!("{name}: {e}"))? {
      Value::Seq(items) => items.into_items(),
      _ => return Err(format!("{name}: arguments must be an array")),
    };
    let ambient = snapfire_fsr_ir::Ambient { locale: locale.to_owned(), now: 0, catalogs: self.interpreter.catalogs().cloned() };
    let value = self.interpreter.extensions().call(name, &ambient, &args).map_err(|f| f.message)?;
    Ok(value_to_json(&value).to_string())
  }
  fn fetch(&self, _method: String, _url: String, _body: Option<String>, _headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse> {
    Box::pin(async { FetchResponse { status: 404, body: "{}".to_owned(), headers: Vec::new() } })
  }
}

fn map(entries: Vec<(&str, Value)>) -> Value {
  Value::Map(entries.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn product(id: i128) -> Value {
  map(vec![
    ("id", Value::Int(id)),
    ("name", Value::str(format!("Filament {id}"))),
    ("brand", Value::str("Prusa")),
    ("category", Value::str(if id % 3 == 0 { "tools" } else { "printing" })),
    ("price_cents", Value::Int(2400 + id * 100)),
    ("list_price_cents", Value::Int(2900 + id * 100)),
    ("image", map(vec![("color", Value::str("#e8d5b5")), ("emoji", Value::str("🧵"))])),
    ("rating", Value::F64(4.5)),
    ("reviews", Value::Int(12 * id)),
    ("stock", Value::Int(id % 4 * 3)),
    ("description", Value::str("A spool of filament for the printer on your desk, wound tight and dried before shipping.")),
    ("tags", Value::seq(vec![Value::str("pla"), Value::str("1.75mm")])),
    ("attributes", Value::seq(vec![map(vec![("name", Value::str("Ingredients")), ("value", Value::str("PLA"))]), map(vec![("name", Value::str("Weight")), ("value", Value::str("1 kg"))])])),
  ])
}

fn props() -> ValueMap {
  let products: Vec<Value> = (1..=12i128).map(product).collect();
  let Value::Map(props) = map(vec![
    ("products", Value::seq(products)),
    ("q", Value::str("")),
    ("category", Value::str("printing")),
    ("cartCount", Value::Int(2)),
  ]) else {
    unreachable!()
  };
  props
}

fn machine_state() -> String {
  let power = std::process::Command::new("pmset").args(["-g"]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).lines().find(|l| l.contains("powermode")).map(|l| l.trim().to_owned()).unwrap_or_default()).unwrap_or_default();
  let load = std::process::Command::new("uptime").output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default();
  format!("{power}; {load}")
}

/// Runs `body` on `requesters` threads until the window closes, counting what
/// each produced. Threads are built and warmed before the clock starts.
fn soak<T>(requesters: usize, window: Duration, build_worker: impl Fn() -> T + Send + Sync + 'static, render: impl Fn(&mut T) + Send + Sync + 'static) -> (u64, Duration) {
  let build_worker = Arc::new(build_worker);
  let render = Arc::new(render);
  let ready = Arc::new(Barrier::new(requesters + 1));
  let go = Arc::new(Barrier::new(requesters + 1));
  let stop = Arc::new(AtomicBool::new(false));
  let count = Arc::new(AtomicU64::new(0));
  let mut handles = Vec::new();
  for _ in 0..requesters {
    let (build_worker, render, ready, go, stop, count) = (Arc::clone(&build_worker), Arc::clone(&render), Arc::clone(&ready), Arc::clone(&go), Arc::clone(&stop), Arc::clone(&count));
    handles.push(std::thread::spawn(move || {
      let mut worker = build_worker();
      render(&mut worker);
      ready.wait();
      go.wait();
      let mut done = 0u64;
      while !stop.load(Ordering::Relaxed) {
        for _ in 0..16 {
          render(&mut worker);
        }
        done += 16;
      }
      count.fetch_add(done, Ordering::Relaxed);
    }));
  }
  ready.wait();
  let started = Instant::now();
  go.wait();
  std::thread::sleep(window);
  stop.store(true, Ordering::Relaxed);
  for handle in handles {
    handle.join().expect("requester joins");
  }
  (count.load(Ordering::Relaxed), started.elapsed())
}

fn main() {
  let seconds: u64 = std::env::var("SOAK_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
  let window = Duration::from_secs(seconds);
  let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("app");
  let workspace_snapfirec = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../target/debug/snapfirec");
  if std::env::var_os("SNAPFIREC").is_none() && workspace_snapfirec.is_file() {
    unsafe { std::env::set_var("SNAPFIREC", &workspace_snapfirec) };
  }
  let built = build(&app, &Options::default()).expect("fsr build app");
  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(snapfire_fsr_ir::render::prepare(&c.body)))).collect());
  let component = components.get(MODULE).cloned().expect("the page lowered");
  let prepared = prepare(&app).expect("fsr test's preparation");
  let layout = Layout::of(&app).expect("layout");
  let react_dom = VendorManifest::read(&app, &layout).expect("vendor manifest").packages.get("react-dom").map(|p| p.version.clone()).expect("react-dom is vendored");
  let server = test_bundle(&app, "react-dom/server", &react_dom, &format!("{ESM_HOST}/react-dom@{react_dom}/server?target=es2022&bundle&external=react")).expect("react-dom/server");
  let mut resolution = prepared.resolution.clone();
  resolution.overrides.remove("react");
  resolution.overrides.remove("react/jsx-runtime");
  resolution.overrides.remove("react-dom/client");
  resolution.overrides.insert("react-dom/server".to_owned(), server);
  let module = prepared.test_dir.join(format!("bench-{PAGE}.js"));
  let page = Arc::new(props());
  let json = value_to_json(&Value::Map((*page).clone())).to_string();

  eprintln!("machine before: {}", machine_state());
  eprintln!("window {seconds}s per row, catalog with twelve products\n");
  println!("engine     requesters   renders      renders/sec   system/render   per requester");

  for &requesters in REQUESTERS {
    let (interpreter, component, components, page) = (Interpreter::default(), Arc::clone(&component), Arc::clone(&components), Arc::clone(&page));
    let (count, elapsed) = soak(
      requesters,
      window,
      move || (interpreter.clone(), Arc::clone(&component), Arc::clone(&components), Arc::clone(&page)),
      |(interpreter, component, components, page)| {
        let out = interpreter.render(component, page, components).expect("renders");
        std::hint::black_box(out.html.len());
      },
    );
    let per_second = count as f64 / elapsed.as_secs_f64();
    println!("fsr        {requesters:<12} {count:<12} {per_second:<13.0} {:<15} {:.2} µs", format!("{:.2} µs", 1_000_000.0 / per_second), 1_000_000.0 / per_second * requesters as f64);
  }

  for &requesters in REQUESTERS {
    let (resolution, dom, module, json) = (resolution.clone(), prepared.dom.clone(), module.clone(), json.clone());
    let (count, elapsed) = soak(
      requesters,
      window,
      move || {
        let rt = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
        let engine = Engine::new(resolution.clone(), &dom, Rc::new(NoHooks { interpreter: Interpreter::default() }), JsCalls::new()).expect("engine");
        let local = tokio::task::LocalSet::new();
        rt.block_on(local.run_until(engine.import(&module))).expect("bench module loads");
        engine.eval_string(&format!("globalThis.__json = {}; globalThis.__props = __decode(__json); ''", serde_json::to_string(&json).expect("a JSON string"))).expect("props set");
        engine
      },
      |engine| {
        engine.eval_string("__render(__props)").expect("react renders");
      },
    );
    let per_second = count as f64 / elapsed.as_secs_f64();
    println!("quickjs    {requesters:<12} {count:<12} {per_second:<13.0} {:<15} {:.2} µs", format!("{:.2} µs", 1_000_000.0 / per_second), 1_000_000.0 / per_second * requesters as f64);
  }

  eprintln!("\nmachine after: {}", machine_state());
}
