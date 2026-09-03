//! `fsr test`, the DOM half: every `*.spec.tsx` under the app, compiled by
//! snapfirec beside the app's own modules into `.fsr-test/dist` and run in
//! QuickJS over linkedom with React's development build, so a hydration
//! mismatch says what mismatched. A page the build lowered is hydrated over
//! the server's own markup. An action the page calls is answered by the
//! interpreter under the mock ctx the spec built, its service methods being
//! the spec's own functions behind the contract.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;

use futures_util::future::{BoxFuture, LocalBoxFuture};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{Body, Interpreter};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{Identity, RequestCtx, ServiceError, SessionCell};
use snapfire_fsr_service::{Call, Contract, Services, Transport};

use crate::test::Summary;
use crate::vendor::{self, ESM_HOST, VendorManifest};
use crate::xwpm::Layout;
use crate::{BuildError, Built, dev};

pub const TEST_DIR: &str = ".fsr-test";
const LINKEDOM: &str = "0.18.12";
/// The React modules a spec loads as development builds, so React's own messages arrive in words.
const DEV_BUILDS: &[&str] = &["react", "react/jsx-runtime", "react-dom/client"];
const TESTING_SPECIFIER: &str = "@snapfire/fsr-client/testing";
const TESTING_URL: &str = "/static/js/fsr/testing.js";

/// The app compiled for the engine: where the modules landed, how a specifier reaches a file and the DOM bundle.
pub struct Prepared {
  pub app: PathBuf,
  pub test_dir: PathBuf,
  pub dist: PathBuf,
  pub resolution: Resolution,
  pub dom: PathBuf,
  pub boot: PathBuf,
}

/// Vendors the test-only builds, writes the test config and compiles the app's modules and spec files into `.fsr-test/dist`.
pub fn prepare(app: &Path) -> Result<Prepared, BuildError> {
  let app = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
  let layout = Layout::of(&app)?;
  let test_dir = app.join(TEST_DIR);
  std::fs::create_dir_all(&test_dir).map_err(|e| BuildError::Io(test_dir.clone(), e))?;
  let overrides = test_vendor(&app, &layout, &test_dir)?;
  let dom = overrides.get("linkedom").cloned().expect("linkedom is vendored");
  write_config(&app, &layout, &test_dir)?;
  compile(&app, &test_dir)?;

  let mut import_map: HashMap<String, String> = imports_of(&vendor::read_import_map(&app, &layout)?).into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned()))).collect();
  import_map.insert(TESTING_SPECIFIER.to_owned(), TESTING_URL.to_owned());
  let dist = test_dir.join("dist");
  let mut roots = vec![(layout.base.clone(), app.join(&layout.vendor)), ("/static/js/app".to_owned(), dist.clone())];
  roots.extend(static_roots(&app)?);
  let resolution = Resolution { import_map, roots, overrides: overrides.into_iter().filter(|(k, _)| k != "linkedom").collect() };

  let boot = test_dir.join("boot.js");
  std::fs::write(&boot, "import { registerIslands } from \"./dist/generated/islands.js\";\nregisterIslands();\n").map_err(|e| BuildError::Io(boot.clone(), e))?;
  Ok(Prepared { app, test_dir, dist, resolution, dom, boot })
}

/// Fetches one more esm.sh build into `.fsr-test/vendor`, for a bench or a tool that needs a module the specs do not; cached by specifier and version.
pub fn test_bundle(app: &Path, specifier: &str, version: &str, url: &str) -> Result<PathBuf, BuildError> {
  let dir = app.join(TEST_DIR).join("vendor");
  let manifest_path = dir.join("manifest.json");
  let mut manifest: TestVendor = std::fs::read_to_string(&manifest_path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
  if let Some((have, file)) = manifest.entries.get(specifier) {
    let path = dir.join(file);
    if have == version && path.is_file() {
      return Ok(path);
    }
  }
  let client = vendor::client()?;
  let file = fetch_bundle(&client, url, &dir, specifier)?;
  manifest.entries.insert(specifier.to_owned(), (version.to_owned(), file.clone()));
  let text = serde_json::to_string_pretty(&manifest).expect("serialisable");
  std::fs::write(&manifest_path, text).map_err(|e| BuildError::Io(manifest_path.clone(), e))?;
  Ok(dir.join(file))
}

/// Runs every spec file under `app` whose name matches `filter`, adding to `summary`.
pub fn run(app: &Path, built: &Built, contract: &Arc<Contract>, filter: Option<&str>, runtime: &tokio::runtime::Runtime, summary: &mut Summary) -> Result<(), BuildError> {
  let app = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
  let mut files = Vec::new();
  discover(&app, &app, &mut files)?;
  if files.is_empty() {
    return Ok(());
  }
  let Prepared { test_dir, resolution, dom, boot, .. } = prepare(&app)?;

  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(c.body.clone()))).collect());
  let actions: HashMap<String, Arc<Body>> = built.manifest.actions.iter().filter_map(|a| a.body.clone().map(|b| (a.id.clone(), Arc::new(b)))).collect();

  for path in files {
    let rel = path.strip_prefix(&app).unwrap_or(&path).to_string_lossy().replace('\\', "/");
    let compiled = test_dir.join("dist").join(format!("{}.js", rel.trim_end_matches(".tsx").trim_end_matches(".ts")));
    if !compiled.is_file() {
      return Err(BuildError::Dev(format!("{rel}: snapfirec wrote no {}", compiled.display())));
    }
    let calls = JsCalls::new();
    let hooks = Rc::new(SpecHooks::new(contract.clone(), actions.clone(), components.clone(), calls.clone()));
    let local = tokio::task::LocalSet::new();
    let outcome: Result<Vec<(String, Result<(), String>)>, BuildError> = runtime.block_on(local.run_until(async {
      let engine = Engine::new(resolution.clone(), &dom, hooks.clone(), calls).map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      engine.import(&boot).await.map_err(|e| BuildError::Dev(format!("{rel}: registering islands: {e}")))?;
      engine.import(&compiled).await.map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      let names = engine.test_names().map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      let mut results = Vec::new();
      for (i, name) in names.iter().enumerate() {
        if filter.is_some_and(|f| !name.contains(f) && !rel.contains(f)) {
          continue;
        }
        engine.take_console();
        hooks.reset();
        let result = match engine.run_test(i).await {
          Ok(result) => result,
          Err(e) => Err(e.to_string()),
        };
        let console = engine.take_console();
        let errored = console.iter().any(|(level, _)| level == "error");
        let log = console.iter().map(|(level, text)| format!("console.{level}: {text}")).collect::<Vec<_>>().join("\n");
        let result = match (result, errored) {
          (Ok(()), false) => Ok(()),
          (Ok(()), true) => Err(format!("console.error during the test:\n{}", indent(&log))),
          (Err(failure), _) if log.is_empty() => Err(indent(&failure)),
          (Err(failure), _) => Err(format!("{}\nconsole during the test:\n{}", indent(&failure), indent(&log))),
        };
        engine.eval_string("document.body.innerHTML = \"\"; \"\"").map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
        results.push((name.clone(), result));
      }
      Ok(results)
    }));
    for (name, result) in outcome? {
      match result {
        Ok(()) => {
          summary.passed += 1;
          summary.lines.push(format!("test {rel}: {name} ... ok"));
        }
        Err(failure) => {
          summary.failed += 1;
          summary.lines.push(format!("test {rel}: {name} ... FAILED\n{failure}"));
        }
      }
    }
  }
  Ok(())
}

fn indent(text: &str) -> String {
  text.lines().map(|l| format!("  {l}")).collect::<Vec<_>>().join("\n")
}

fn discover(app: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), BuildError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.flatten().map(|e| e.path()).collect();
  entries.sort();
  for path in entries {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
      if path.parent() == Some(app) && (["generated", "dist", "types", "vendor", "node_modules"].contains(&name.as_str()) || name.starts_with(".fsr-")) {
        continue;
      }
      discover(app, &path, out)?;
    } else if name.ends_with(".spec.tsx") || name.ends_with(".spec.ts") {
      out.push(path);
    }
  }
  Ok(())
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TestVendor {
  /// Specifier to `(version, file under the test vendor dir)`.
  #[serde(default)]
  entries: BTreeMap<String, (String, String)>,
}

/// linkedom and the development builds of the React modules the app vendors, fetched once from esm.sh into `.fsr-test/vendor`; by specifier.
fn test_vendor(app: &Path, layout: &Layout, test_dir: &Path) -> Result<HashMap<String, PathBuf>, BuildError> {
  let dir = test_dir.join("vendor");
  let manifest_path = dir.join("manifest.json");
  let mut manifest: TestVendor = match std::fs::read_to_string(&manifest_path) {
    Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
    Err(_) => TestVendor::default(),
  };
  let vendored = VendorManifest::read(app, layout)?;
  let mut wanted: Vec<(String, String, String)> = vec![("linkedom".to_owned(), LINKEDOM.to_owned(), format!("{ESM_HOST}/linkedom@{LINKEDOM}/worker?target=es2022&bundle"))];
  for specifier in DEV_BUILDS {
    let package = vendor::package_of(specifier);
    let Some(entry) = vendored.packages.get(&package) else { continue };
    if !entry.entries.contains_key(*specifier) {
      continue;
    }
    let mut url = format!("{ESM_HOST}/{package}@{}", entry.version);
    if let Some(sub) = specifier.strip_prefix(&format!("{package}/")) {
      url.push('/');
      url.push_str(sub);
    }
    url.push_str("?target=es2022&bundle&dev");
    if !entry.externals.is_empty() {
      url.push_str("&external=");
      url.push_str(&entry.externals.join(","));
    }
    wanted.push((specifier.to_string(), entry.version.clone(), url));
  }
  let mut out = HashMap::new();
  let mut client = None;
  for (specifier, version, url) in wanted {
    if let Some((have, file)) = manifest.entries.get(&specifier) {
      let path = dir.join(file);
      if *have == version && path.is_file() {
        out.insert(specifier, path);
        continue;
      }
    }
    let client = match &client {
      Some(c) => c,
      None => client.insert(vendor::client()?),
    };
    let file = fetch_bundle(client, &url, &dir, &specifier)?;
    manifest.entries.insert(specifier.clone(), (version, file.clone()));
    out.insert(specifier, dir.join(file));
  }
  std::fs::create_dir_all(&dir).map_err(|e| BuildError::Io(dir.clone(), e))?;
  let text = serde_json::to_string_pretty(&manifest).expect("serialisable");
  std::fs::write(&manifest_path, text).map_err(|e| BuildError::Io(manifest_path.clone(), e))?;
  Ok(out)
}

/// Follows an esm.sh stub to its bundle, writes it under `dir/<specifier>/` with same-package imports rewritten to siblings and returns the file relative to `dir`.
fn fetch_bundle(client: &reqwest::blocking::Client, url: &str, dir: &Path, specifier: &str) -> Result<String, BuildError> {
  let stub = vendor::get(client, url)?.ok_or_else(|| BuildError::Http(url.to_owned(), "HTTP 404".to_owned()))?;
  let stub = String::from_utf8(stub).map_err(|e| BuildError::Http(url.to_owned(), e.to_string()))?;
  let path = vendor::stub_paths(&stub).into_iter().find(|p| p.ends_with(".mjs") || p.ends_with(".js")).ok_or_else(|| BuildError::Http(url.to_owned(), format!("esm.sh answered with no module path:\n{stub}")))?;
  let folder = dir.join(specifier.replace('/', "__"));
  std::fs::create_dir_all(&folder).map_err(|e| BuildError::Io(folder.clone(), e))?;
  let mut queue = vec![path];
  let mut first = None;
  while let Some(path) = queue.pop() {
    let name = vendor::file_name(&path);
    let file = folder.join(&name);
    if first.is_none() {
      first = Some(format!("{}/{name}", specifier.replace('/', "__")));
    }
    if file.is_file() {
      continue;
    }
    let module_url = format!("{ESM_HOST}{path}");
    let bytes = vendor::get(client, &module_url)?.ok_or_else(|| BuildError::Http(module_url.clone(), "HTTP 404".to_owned()))?;
    let mut text = String::from_utf8(bytes).map_err(|e| BuildError::Http(module_url.clone(), e.to_string()))?;
    for import in vendor::absolute_imports(&text) {
      let sibling = vendor::file_name(&import);
      text = text.replace(&format!("\"{import}\""), &format!("\"./{sibling}\"")).replace(&format!("'{import}'"), &format!("'./{sibling}'"));
      queue.push(import);
    }
    std::fs::write(&file, text).map_err(|e| BuildError::Io(file.clone(), e))?;
  }
  Ok(first.expect("one path"))
}

/// `.fsr-test/tsconfig.json` and `.fsr-test/importmap.json`: the browser build plus the spec files and the testing module.
fn write_config(app: &Path, layout: &Layout, test_dir: &Path) -> Result<(), BuildError> {
  let mut tsconfig = String::from("{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"outDir\": \"dist\",\n    \"rootDir\": \"..\",\n    \"sourceMap\": true,\n    \"jsx\": \"react-jsx\",\n    \"paths\": {\n");
  let aliases: Vec<(String, String)> = snapfire_fsr_lower::ALIASES.iter().map(|(alias, dir)| (format!("{alias}*"), format!("../{dir}*"))).collect();
  for (i, (from, to)) in aliases.iter().enumerate() {
    tsconfig.push_str(&format!("      \"{from}\": [\"{to}\"]{}\n", if i + 1 == aliases.len() { "" } else { "," }));
  }
  tsconfig.push_str("    }\n  },\n  \"include\": [\"../src/**/*\", \"../routes/**/*.tsx\", \"../generated/islands.ts\", \"../generated/client.ts\", \"../tests/**/*.spec.tsx\", \"../tests/**/*.spec.ts\"]\n}\n");
  let path = test_dir.join("tsconfig.json");
  std::fs::write(&path, tsconfig).map_err(|e| BuildError::Io(path, e))?;
  let mut map = vendor::read_import_map(app, layout)?;
  let mut imports = imports_of(&map);
  imports.insert(TESTING_SPECIFIER.to_owned(), serde_json::Value::String(TESTING_URL.to_owned()));
  map.insert("imports".to_owned(), serde_json::Value::Object(imports));
  let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).expect("serialisable");
  let path = test_dir.join("importmap.json");
  std::fs::write(&path, text).map_err(|e| BuildError::Io(path, e))?;
  Ok(())
}

fn imports_of(map: &serde_json::Map<String, serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
  map.get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default()
}

fn compile(app: &Path, test_dir: &Path) -> Result<(), BuildError> {
  let snapfirec = dev::find_snapfirec(None);
  let config = format!("{TEST_DIR}/tsconfig.json");
  let import_map = format!("{TEST_DIR}/importmap.json");
  let output = Command::new(&snapfirec)
    .current_dir(app)
    .args(["--config", &config, "--source-map", "--public-path", "/static/js/app", "--import-map", &import_map])
    .output()
    .map_err(|e| BuildError::Dev(format!("{}: {e}; put snapfirec beside fsr or on PATH", snapfirec.display())))?;
  if !output.status.success() {
    return Err(BuildError::Dev(format!("snapfirec failed compiling the spec files into {}:\n{}{}", test_dir.display(), String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr))));
  }
  Ok(())
}

#[derive(Deserialize)]
struct AppToml {
  #[serde(default, rename = "static")]
  statics: Vec<StaticEntry>,
}

#[derive(Deserialize)]
struct StaticEntry {
  route: String,
  dir: String,
}

/// The `[[static]]` routes of `config/app.toml` beside the app, so the client library resolves the way the host serves it.
fn static_roots(app: &Path) -> Result<Vec<(String, PathBuf)>, BuildError> {
  let Some(config_dir) = app.parent().map(|p| p.join("config")) else { return Ok(Vec::new()) };
  let path = config_dir.join("app.toml");
  let Ok(text) = std::fs::read_to_string(&path) else { return Ok(Vec::new()) };
  let parsed: AppToml = toml::from_str(&text).map_err(|e| BuildError::Dev(format!("{}: {e}", path.display())))?;
  Ok(parsed.statics.into_iter().map(|s| (s.route, config_dir.join(s.dir))).collect())
}

/// A mocked service layer whose methods are the spec's own functions, reached through the engine between jobs.
struct JsTransport {
  ctx_id: u32,
  calls: JsCalls,
  records: Mutex<Vec<Value>>,
}

impl Transport for JsTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let mut record = ValueMap::new();
    record.insert("service".to_owned(), Value::Str(call.service.clone()));
    record.insert("method".to_owned(), Value::Str(call.method.clone()));
    record.insert("args".to_owned(), Value::Map(call.args.clone()));
    self.records.lock().push(Value::Map(record));
    let key = format!("{}:{}.{}", self.ctx_id, call.service, call.method);
    let args = value_to_json(&Value::Map(call.args)).to_string();
    let calls = self.calls.clone();
    let (service, method) = (call.service, call.method);
    Box::pin(async move {
      let answer = calls.call(key, args).await.map_err(|m| ServiceError::new(snapfire_fsr_runtime::FailureKind::Unavailable, &service, &method, m))?;
      let json: serde_json::Value = serde_json::from_str(&answer).map_err(|e| ServiceError::new(snapfire_fsr_runtime::FailureKind::Internal, &service, &method, format!("the mock's answer is not JSON: {e}")))?;
      json_to_value(&json).map_err(|e| ServiceError::new(snapfire_fsr_runtime::FailureKind::Internal, &service, &method, format!("the mock's answer does not decode: {e}")))
    })
  }
}

struct MockCtx {
  ctx: RequestCtx,
  input: Option<Value>,
  transport: Arc<JsTransport>,
}

#[derive(Deserialize)]
struct CtxSpec {
  #[serde(default)]
  session: serde_json::Value,
  #[serde(default)]
  params: BTreeMap<String, String>,
  #[serde(default)]
  query: BTreeMap<String, String>,
  #[serde(default)]
  input: serde_json::Value,
  #[serde(default)]
  identity: Option<IdentitySpec>,
}

#[derive(Deserialize)]
struct IdentitySpec {
  subject: String,
  #[serde(default)]
  claims: serde_json::Value,
}

struct SpecHooks {
  contract: Arc<Contract>,
  actions: HashMap<String, Arc<Body>>,
  components: Arc<Components>,
  calls: JsCalls,
  interpreter: Interpreter,
  ctxs: RefCell<Vec<Rc<MockCtx>>>,
  current: Cell<u32>,
}

impl SpecHooks {
  fn new(contract: Arc<Contract>, actions: HashMap<String, Arc<Body>>, components: Arc<Components>, calls: JsCalls) -> Self {
    let hooks = Self { contract, actions, components, calls, interpreter: Interpreter::default(), ctxs: RefCell::new(Vec::new()), current: Cell::new(0) };
    hooks.reset();
    hooks
  }

  /// Forgets every ctx and installs the empty one as id 0.
  fn reset(&self) {
    self.ctxs.borrow_mut().clear();
    self.current.set(0);
    let empty = self.build(CtxSpec { session: serde_json::Value::Null, params: BTreeMap::new(), query: BTreeMap::new(), input: serde_json::Value::Null, identity: None }).expect("the empty ctx builds");
    self.ctxs.borrow_mut().push(Rc::new(empty));
  }

  fn build(&self, spec: CtxSpec) -> Result<MockCtx, String> {
    let session = match &spec.session {
      serde_json::Value::Null => ValueMap::new(),
      json => match json_to_value(json).map_err(|e| format!("session: {e}"))? {
        Value::Map(map) => map,
        _ => return Err("session must be an object".to_owned()),
      },
    };
    let identity = match spec.identity {
      Some(id) => {
        let claims = match &id.claims {
          serde_json::Value::Null => ValueMap::new(),
          json => match json_to_value(json).map_err(|e| format!("identity.claims: {e}"))? {
            Value::Map(map) => map,
            _ => return Err("identity.claims must be an object".to_owned()),
          },
        };
        Some(Identity { subject: id.subject, claims })
      }
      None => None,
    };
    let input = match &spec.input {
      serde_json::Value::Null => None,
      json => Some(json_to_value(json).map_err(|e| format!("input: {e}"))?),
    };
    let id = self.ctxs.borrow().len() as u32;
    let transport = Arc::new(JsTransport { ctx_id: id, calls: self.calls.clone(), records: Mutex::new(Vec::new()) });
    let services = Services::builder().contract((*self.contract).clone()).default_transport(transport.clone()).build();
    let handle = services.bind(identity.clone(), Arc::new(snapfire_fsr_service::NoCredentials));
    let params: Params = spec.params.into_iter().collect();
    let query: Params = spec.query.into_iter().collect();
    let ctx = RequestCtx { params, query, session: SessionCell::new(session, identity), csrf: None, services: handle };
    Ok(MockCtx { ctx, input, transport })
  }

  fn get(&self, id: u32) -> Result<Rc<MockCtx>, String> {
    self.ctxs.borrow().get(id as usize).cloned().ok_or_else(|| format!("no ctx {id}"))
  }
}

fn json_response(status: u16, json: serde_json::Value) -> FetchResponse {
  FetchResponse { status, body: json.to_string() }
}

impl Hooks for SpecHooks {
  fn ctx(&self, spec: &str) -> Result<u32, String> {
    let spec: CtxSpec = serde_json::from_str(spec).map_err(|e| format!("ctx: {e}"))?;
    let built = self.build(spec)?;
    let mut ctxs = self.ctxs.borrow_mut();
    ctxs.push(Rc::new(built));
    Ok(ctxs.len() as u32 - 1)
  }

  fn use_ctx(&self, id: u32) -> Result<(), String> {
    self.get(id)?;
    self.current.set(id);
    Ok(())
  }

  fn session(&self, id: u32) -> Result<String, String> {
    let (session, _) = self.get(id)?.ctx.session.snapshot();
    Ok(value_to_json(&Value::Map(session)).to_string())
  }

  fn calls(&self, id: u32) -> Result<String, String> {
    let calls = self.get(id)?.transport.records.lock().clone();
    Ok(value_to_json(&Value::Seq(calls)).to_string())
  }

  fn render(&self, module: &str, props: &str) -> Result<Option<String>, String> {
    let Some(component) = self.components.get(module).cloned() else { return Ok(None) };
    let json: serde_json::Value = serde_json::from_str(props).map_err(|e| format!("props: {e}"))?;
    let props = match json_to_value(&json).map_err(|e| format!("props: {e}"))? {
      Value::Map(map) => map,
      Value::Null => ValueMap::new(),
      _ => return Err("props must be an object".to_owned()),
    };
    let html = futures_executor::block_on(self.interpreter.render(&component, &props, &self.components)).map_err(|f| format!("rendering {module}: {}", f.message))?;
    Ok(Some(html))
  }

  fn fetch(&self, method: String, url: String, body: Option<String>) -> LocalBoxFuture<'static, FetchResponse> {
    let path = url.split(['?', '#']).next().unwrap_or("").to_owned();
    let action = path.strip_prefix("/_sf/action/").map(|id| percent_decode(id));
    let Some(id) = action.filter(|_| method == "POST") else {
      return Box::pin(async move { json_response(404, serde_json::json!({ "kind": "not_found", "message": format!("fsr test answers POST /_sf/action/<id> only, not {method} {path}") })) });
    };
    let Some(body_ir) = self.actions.get(&id).cloned() else {
      let message = format!("`{id}` is not a lowered action; a test can only call what the build lowered");
      return Box::pin(async move { json_response(501, serde_json::json!({ "kind": "internal", "message": message })) });
    };
    let mock = match self.get(self.current.get()) {
      Ok(mock) => mock,
      Err(m) => return Box::pin(async move { json_response(500, serde_json::json!({ "kind": "internal", "message": m })) }),
    };
    let input = match body.as_deref().map(serde_json::from_str::<serde_json::Value>) {
      Some(Ok(json)) => match json_to_value(&json) {
        Ok(value) => value,
        Err(e) => return Box::pin(async move { json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })) }),
      },
      Some(Err(e)) => return Box::pin(async move { json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })) }),
      None => mock.input.clone().unwrap_or(Value::Null),
    };
    let interpreter = self.interpreter.clone();
    Box::pin(async move {
      match interpreter.run(&body_ir, &mock.ctx, Some(input)).await {
        Ok(outcome) => json_response(200, value_to_json(&outcome.value)),
        Err(fail) => json_response(fail.kind.http_status(), serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message })),
      }
    })
  }
}

fn percent_decode(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' && i + 2 < bytes.len() {
      if let Ok(byte) = u8::from_str_radix(&text[i + 1..i + 3], 16) {
        out.push(byte);
        i += 3;
        continue;
      }
    }
    out.push(bytes[i]);
    i += 1;
  }
  String::from_utf8_lossy(&out).into_owned()
}
