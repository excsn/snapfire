//! React's `renderToString` in FSR's QuickJS engine, the oracle a page's IR markup is held against.

use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures::future::LocalBoxFuture;
use snapfire_fsr_cli::spec::{prepare, test_bundle, TEST_DIR};
use snapfire_fsr_cli::vendor::{VendorManifest, ESM_HOST};
use snapfire_fsr_cli::xwpm::Layout;
use snapfire_fsr_core::Value;
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_ir::Interpreter;
use snapfire_fsr_payload::value_to_json;

use crate::copy;

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

/// React's `renderToString` over the case's page in QuickJS, loaded once.
pub fn react_engine(app: &Path, built: &snapfire_fsr_cli::Built, cache: &Path, rt: &tokio::runtime::Runtime) -> Result<Engine, String> {
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
  let app = std::fs::canonicalize(app).map_err(|e| format!("app: {e}"))?;
  let root = app.display().to_string();
  let tsconfig = format!("{{ \"compilerOptions\": {{ \"target\": \"es2022\", \"outDir\": \"dist\", \"rootDir\": {root:?}, \"allowJs\": true, \"jsx\": \"react-jsx\", \"jsxImportSource\": \"react\", \"baseUrl\": {root:?}, \"paths\": {{ \"@app/*\": [\"./*\"], \"@routes/*\": [\"./routes/*\"], \"@src/*\": [\"./src/*\"] }} }}, \"include\": [{routes:?}, {src:?}] }}\n", routes = format!("{root}/routes/**/*"), src = format!("{root}/src/**/*"));
  std::fs::write(reference.join("tsconfig.json"), tsconfig).map_err(|e| format!("ref: {e}"))?;
  let snapfirec = std::env::var_os("SNAPFIREC").unwrap_or_else(|| "snapfirec".into());
  let compiled = std::process::Command::new(snapfirec).arg("--import-map").arg(app.join("importmap.json")).current_dir(&reference).output().map_err(|e| format!("snapfirec: {e}"))?;
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

