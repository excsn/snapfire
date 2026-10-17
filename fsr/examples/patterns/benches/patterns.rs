//! Each case that builds, rendered by the IR and by React's `renderToString` in QuickJS: whether the two agree once what legitimately differs is taken out, and what each render costs. Needs the network once, for react-dom/server. `PATTERNS_LABEL` names the run; its table goes to `results/<label>.tsv`.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use criterion::{black_box, BenchmarkId, Criterion};
use futures::future::LocalBoxFuture;
use patterns::{app_of, cases, flatten, normalised, outcome, root, without_fsr_markers, PAGE};
use snapfire_fsr_cli::spec::{prepare, test_bundle, TEST_DIR};
use snapfire_fsr_cli::vendor::{VendorManifest, ESM_HOST};
use snapfire_fsr_cli::xwpm::Layout;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_payload::value_to_json;

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
      Value::Seq(items) => items,
      _ => return Err(format!("{name}: arguments must be an array")),
    };
    let ambient = snapfire_fsr_ir::Ambient::default().locale(locale.to_owned()).now(0).catalogs(self.interpreter.catalogs().cloned());
    let value = self.interpreter.extensions().call(name, &ambient, &args).map_err(|f| f.message)?;
    Ok(value_to_json(&value).to_string())
  }
  fn fetch(&self, _method: String, _url: String, _body: Option<String>, _headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse> {
    Box::pin(async { FetchResponse::new(404, "{}") })
  }
}

fn copy(from: &Path, to: &Path) {
  std::fs::create_dir_all(to).unwrap();
  for entry in std::fs::read_dir(from).unwrap().flatten() {
    let target = to.join(entry.file_name());
    if entry.file_type().unwrap().is_dir() {
      copy(&entry.path(), &target);
    } else {
      std::fs::copy(entry.path(), target).unwrap();
    }
  }
}

struct Ready {
  name: String,
  component: Arc<snapfire_fsr_ir::ast::Component>,
  components: Arc<Components>,
  engine: Option<Engine>,
}

fn bench(c: &mut Criterion) {
  let label = std::env::var("PATTERNS_LABEL").unwrap_or_else(|_| "current".to_owned());
  let scratch = std::env::temp_dir().join(format!("fsr-patterns-bench-{label}"));
  let _ = std::fs::remove_dir_all(&scratch);
  let cache = scratch.join("react-dom-server");
  let rt = tokio::runtime::Builder::new_current_thread().build().expect("runtime");
  let interpreter = Interpreter::default();
  let mut rows = vec!["case\toutcome\treact\tir_bytes\treact_bytes".to_owned()];
  let mut ready = Vec::new();
  for case in cases() {
    let app = app_of(&case, &scratch);
    let (line, built) = outcome(&app);
    let kind = line.split(':').next().unwrap_or_default().to_owned();
    let Some(built) = built else {
      rows.push(format!("{}\t{kind}\t-\t-\t-", case.name));
      continue;
    };
    let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(snapfire_fsr_ir::render::prepare(&c.body)))).collect());
    let Some(component) = components.get(PAGE).cloned() else {
      rows.push(format!("{}\t{kind}\tno page\t-\t-", case.name));
      continue;
    };
    let ir = interpreter.render(&component, &ValueMap::default(), &components).map(|r| flatten(&r));
    let react = react_engine(&app, &built, &cache, &rt).and_then(|engine| {
      let html = engine.eval_string("__render({})").map_err(|e| format!("react: {e}"))?;
      Ok((engine, html))
    });
    let (verdict, ir_bytes, react_bytes, engine) = match (&ir, react) {
      (Ok(ir), Ok((engine, html))) => {
        std::fs::write(scratch.join(format!("{}.ir.html", case.name)), ir).ok();
        std::fs::write(scratch.join(format!("{}.react.html", case.name)), &html).ok();
        let verdict = match (normalised(ir) == normalised(&html), normalised(&without_fsr_markers(ir)) == normalised(&html)) {
          (true, _) => "same",
          (false, true) => "equivalent",
          (false, false) => "differs",
        };
        (verdict.to_owned(), ir.len().to_string(), html.len().to_string(), Some(engine))
      }
      (Err(e), _) => (format!("ir fails: {}", e.message.lines().next().unwrap_or_default()), "-".to_owned(), "-".to_owned(), None),
      (Ok(_), Err(e)) => (format!("react fails: {}", e.lines().next().unwrap_or_default()), "-".to_owned(), "-".to_owned(), None),
    };
    rows.push(format!("{}\t{kind}\t{verdict}\t{ir_bytes}\t{react_bytes}", case.name));
    if ir.is_ok() {
      ready.push(Ready { name: case.name.clone(), component, components, engine });
    }
  }
  let table = rows.join("\n") + "\n";
  std::fs::create_dir_all(root().join("results")).expect("results/");
  std::fs::write(root().join("results").join(format!("{label}.tsv")), &table).expect("results/");
  eprintln!("{table}");
  for case in &ready {
    c.bench_with_input(BenchmarkId::new(format!("{label}/ir"), &case.name), case, |b, case| b.iter(|| interpreter.render(black_box(&case.component), black_box(&ValueMap::default()), &case.components).expect("ir renders")));
    if let Some(engine) = &case.engine {
      c.bench_with_input(BenchmarkId::new(format!("{label}/react"), &case.name), case, |b, _| b.iter(|| engine.eval_string("__render({})").expect("react renders")));
    }
  }
}

/// React's `renderToString` over the case's page in QuickJS, loaded once.
fn react_engine(app: &Path, built: &snapfire_fsr_cli::Built, cache: &Path, rt: &tokio::runtime::Runtime) -> Result<Engine, String> {
  let mut plugins = snapfire_fsr_cli::plugins::Plugins::new();
  snapfire_fsr_cli::write_overlay(app, built).map_err(|e| format!("overlay: {e}"))?;
  snapfire_fsr_cli::write_generated(app, built).map_err(|e| format!("generated: {e}"))?;
  let compositions: Vec<String> = built.manifest.components.iter().filter(|c| c.body.owner == snapfire_fsr_ir::Owner::Fsr && c.module.starts_with("routes/")).map(|c| c.module.clone()).collect();
  let prepared = prepare(app, &built.browser_files, &compositions, &[], &built.composition_files, &built.added, &mut plugins).map_err(|e| format!("prepare: {e}"))?;
  let layout = Layout::of(app).map_err(|e| format!("layout: {e}"))?;
  let react_dom = VendorManifest::read(app, &layout).map_err(|e| format!("vendor: {e}"))?.packages.get("react-dom").map(|p| p.version.clone()).ok_or("react-dom is not vendored")?;
  let vendor = app.join(TEST_DIR).join("vendor");
  if cache.is_dir() {
    copy(cache, &vendor);
  }
  let server: PathBuf = test_bundle(app, "react-dom/server", &react_dom, &format!("{ESM_HOST}/react-dom@{react_dom}/server?target=es2022&bundle&external=react")).map_err(|e| format!("react-dom/server: {e}"))?;
  if !cache.is_dir() {
    copy(&vendor, cache);
  }
  let mut resolution: Resolution = prepared.resolution.clone();
  resolution.overrides.remove("react");
  resolution.overrides.remove("react/jsx-runtime");
  resolution.overrides.remove("react-dom/client");
  resolution.overrides.insert("react-dom/server".to_owned(), server);
  let reference = prepared.test_dir.join("ref");
  std::fs::create_dir_all(&reference).map_err(|e| format!("ref: {e}"))?;
  std::fs::write(reference.join("tsconfig.json"), "{ \"compilerOptions\": { \"target\": \"es2022\", \"outDir\": \"dist\", \"rootDir\": \"../..\", \"jsx\": \"react-jsx\", \"jsxImportSource\": \"react\" }, \"files\": [\"../../routes/page.tsx\"] }\n").map_err(|e| format!("ref: {e}"))?;
  let snapfirec = std::env::var_os("SNAPFIREC").unwrap_or_else(|| "snapfirec".into());
  let compiled = std::process::Command::new(snapfirec).arg("--import-map").arg("../../importmap.json").current_dir(&reference).output().map_err(|e| format!("snapfirec: {e}"))?;
  if !compiled.status.success() {
    return Err(format!("snapfirec: {}", String::from_utf8_lossy(&compiled.stderr).lines().next().unwrap_or_default()));
  }
  let module = prepared.test_dir.join("bench-page.js");
  std::fs::write(prepared.test_dir.join("bench-channel.js"), CHANNEL).map_err(|e| format!("module: {e}"))?;
  std::fs::write(&module, "import \"./bench-channel.js\";\nimport { createElement } from \"react\";\nimport { renderToString } from \"react-dom/server\";\nimport Page from \"./ref/dist/routes/page.js\";\nglobalThis.__render = (props) => renderToString(createElement(Page, props));\n").map_err(|e| format!("module: {e}"))?;
  let engine = Engine::new(resolution, &prepared.dom, Rc::new(NoHooks { interpreter: Interpreter::default() }), JsCalls::new()).map_err(|e| format!("engine: {e}"))?;
  let local = tokio::task::LocalSet::new();
  rt.block_on(local.run_until(engine.import(&module))).map_err(|e| format!("import: {e}"))?;
  Ok(engine)
}

/// React 19's server build reaches for `MessageChannel` as it loads, which QuickJS lacks: a pair of ports that deliver on a microtask, which `renderToString` never waits on.
const CHANNEL: &str = "globalThis.MessageChannel ??= class { constructor() { const a = { onmessage: null }; const b = { onmessage: null }; a.postMessage = (data) => queueMicrotask(() => b.onmessage?.({ data })); b.postMessage = (data) => queueMicrotask(() => a.onmessage?.({ data })); this.port1 = a; this.port2 = b; } };\n";

fn main() {
  let mut c = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(1)).warm_up_time(Duration::from_millis(300)).configure_from_args();
  bench(&mut c);
  c.final_summary();
}
