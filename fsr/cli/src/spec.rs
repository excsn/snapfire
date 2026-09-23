//! `fsr test`, the DOM half: every `*.spec.tsx` under the app, compiled by
//! snapfirec beside the app's own modules into `.fsr-test/dist` and run in
//! QuickJS over linkedom with React's development build, so a hydration
//! mismatch says what mismatched. A page the build lowered is hydrated over
//! the server's own markup. An action the page calls is answered by the
//! interpreter under the mock ctx the spec built, its service methods being
//! the spec's own functions behind the contract.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use futures_util::future::{BoxFuture, LocalBoxFuture};
use futures_util::StreamExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use snapfire_fsr_core::{Params, Value, ValueMap};
use snapfire_fsr_engine::{Engine, FetchResponse, Hooks, JsCalls, Resolution};
use snapfire_fsr_host::config::Config;
use snapfire_fsr_host::{Host, HostError, Preflight, PreflightAction, RenderMode};
use snapfire_fsr_ir::render::Components;
use snapfire_fsr_ir::{Body, Interpreter};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_plan::Manifest;
use snapfire_fsr_runtime::{parse_query, HandlerMatcher, Identity, RequestCtx, ServiceError, SessionCell};
use snapfire_fsr_service::Type;
use snapfire_fsr_service::{Call, Contract, Services, Transport};

use crate::test::{Outcome, Summary};
use crate::vendor::{self, ESM_HOST, VendorManifest};
use crate::xwpm::Layout;
use crate::{BuildError, Built, dev, serve};

pub const TEST_DIR: &str = ".fsr-test";
const LINKEDOM: &str = "0.18.12";
/// The React modules a spec loads as development builds, so React's own messages arrive in words.
const DEV_BUILDS: &[&str] = &["react", "react/jsx-runtime", "react-dom/client"];
const TESTING_SPECIFIER: &str = "@snapfire/fsr-client/testing";
const TESTING_URL: &str = "/static/js/fsr/testing.js";
const STD_SPECIFIER: &str = "@snapfire/fsr-client/std";
const STD_URL: &str = "/static/js/fsr/std.js";

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
pub fn prepare(app: &Path, browser_routes: &[String]) -> Result<Prepared, BuildError> {
  let app = app.canonicalize().map_err(|e| BuildError::Io(app.to_path_buf(), e))?;
  let layout = Layout::of(&app)?;
  let site = crate::site_beside(&app);
  let bundle = crate::bundle_base(site.as_ref());
  let shell = match site.as_ref().and_then(|site| site.shell.as_ref()) {
    Some(path) => Some(crate::ShellContract::read(path)?),
    None => None,
  };
  let test_dir = app.join(TEST_DIR);
  std::fs::create_dir_all(&test_dir).map_err(|e| BuildError::Io(test_dir.clone(), e))?;
  let overrides = test_vendor(&app, &layout, &test_dir, shell.as_ref())?;
  let dom = overrides.get("linkedom").cloned().expect("linkedom is vendored");
  write_config(&app, &layout, &test_dir, browser_routes)?;
  compile(&app, &test_dir, &bundle)?;

  let mut import_map: HashMap<String, String> = imports_of(&vendor::read_import_map(&app, &layout)?).into_iter().filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_owned()))).collect();
  import_map.insert(TESTING_SPECIFIER.to_owned(), TESTING_URL.to_owned());
  let dist = test_dir.join("dist");
  let mut roots = vec![(layout.base.clone(), app.join(&layout.vendor)), (bundle.clone(), dist.clone())];
  roots.extend(static_roots(&app)?);
  // The host answers this prefix out of the binary rather than off disk, so
  // nothing in the application's configuration points at the client and the
  // spec has to be given the same modules the host would have served. Last,
  // so an application serving its own client still wins the prefix.
  let client_dir = test_dir.join("client");
  snapfire_fsr_host::client::write_to(&client_dir, false).map_err(|e| BuildError::Io(client_dir.clone(), e))?;
  roots.push((snapfire_fsr_host::client::ROUTE.to_owned(), client_dir));
  let resolution = Resolution { import_map, roots, overrides: overrides.into_iter().filter(|(k, _)| k != "linkedom").collect() };

  let boot = test_dir.join("boot.js");
  let mut boot_source = String::new();
  for file in crate::sorted_files(&app.join(snapfire_fsr_lower::EXT_DIR), ".ts")? {
    let name = file.file_name().unwrap_or_default().to_string_lossy().trim_end_matches(".ts").to_owned();
    boot_source.push_str(&format!("import \"./dist/{}/{name}.js\";\n", snapfire_fsr_lower::EXT_DIR));
  }
  boot_source.push_str("import { discard } from \"@snapfire/fsr-client\";\nglobalThis.__sf.discard = discard;\nimport { registerIslands } from \"./dist/generated/islands.js\";\nregisterIslands();\n");
  if let Some(entry) = entry_module(&app) {
    boot_source.push_str(&format!("import \"./dist/src/{entry}.js\";\n"));
  }
  std::fs::write(&boot, boot_source).map_err(|e| BuildError::Io(boot.clone(), e))?;
  Ok(Prepared { app, test_dir, dist, resolution, dom, boot })
}

/// The application's entry module, `src/main.ts` or `src/main.tsx`, which a
/// spec's document imports so whatever it wires, a `derive`, a global or a
/// listener, is in place the way it is in a browser. Absent, only the island
/// registry runs.
fn entry_module(app: &Path) -> Option<String> {
  ["main.ts", "main.tsx"].iter().find(|name| app.join("src").join(name).is_file()).map(|name| name.trim_end_matches(".tsx").trim_end_matches(".ts").to_owned())
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
  crate::write_overlay(&app, built)?;
  crate::write_generated(&app, built)?;
  let Prepared { test_dir, resolution, dom, boot, .. } = prepare(&app, &built.browser_routes)?;

  let frameworks = snapfire_fsr_ir::Frameworks { react: built.manifest.frameworks.get("react").and_then(|version| snapfire_fsr_ir::ReactMajor::of(version)), vue: built.manifest.frameworks.get("vue").and_then(|version| snapfire_fsr_ir::VueMajor::of(version)) };
  let components: Arc<Components> = Arc::new(built.manifest.components.iter().map(|c| (c.module.clone(), Arc::new(snapfire_fsr_ir::render::prepare(&c.body)))).collect());
  let natives = native_names(&built.manifest);
  let actions: Actions = built.manifest.actions.iter().filter_map(|a| a.body.clone().map(|b| (a.id.clone(), (a.input.clone(), Arc::new(b))))).collect();
  let mut handlers: HashMap<String, (Option<String>, Arc<Body>)> = HashMap::new();
  let mut handler_matcher = HandlerMatcher::new();
  for row in built.manifest.lowered_handlers() {
    if let Some(body) = &row.body {
      handlers.insert(row.id.clone(), (row.input.clone(), Arc::new(body.clone())));
      handler_matcher.insert(&row.method, &row.pattern, row.id.clone()).map_err(|e| BuildError::Dev(format!("{}: {e}", row.pattern)))?;
    }
  }
  let handlers = Arc::new((handlers, handler_matcher, built.manifest.middleware.clone().map(Arc::new)));
  let calls = JsCalls::new();
  let current = Arc::new(AtomicU32::new(0));
  let records = Records::default();
  let transport = Arc::new(JsTransport { ctx: None, current: current.clone(), calls: calls.clone(), records: records.clone() });
  let native_modules = native_modules(&built.manifest);
  let host = page_host(&app, built, contract, transport, &natives, &native_modules, calls.clone(), current.clone())?;

  for path in files {
    let rel = path.strip_prefix(&app).unwrap_or(&path).to_string_lossy().replace('\\', "/");
    let compiled = test_dir.join("dist").join(format!("{}.js", rel.trim_end_matches(".tsx").trim_end_matches(".ts")));
    if !compiled.is_file() {
      return Err(BuildError::Dev(format!("{rel}: snapfirec wrote no {}", compiled.display())));
    }
    let hooks = Rc::new(SpecHooks::new(contract.clone(), actions.clone(), handlers.clone(), components.clone(), calls.clone(), current.clone(), records.clone(), host.clone(), &natives, frameworks));
    let local = tokio::task::LocalSet::new();
    let outcome: Result<Vec<(String, Outcome)>, BuildError> = runtime.block_on(local.run_until(async {
      let engine = Engine::new(resolution.clone(), &dom, hooks.clone(), calls.clone()).map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      engine.import(&boot).await.map_err(|e| BuildError::Dev(format!("{rel}: registering islands: {e}")))?;
      engine.import(&compiled).await.map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      let names = engine.test_names().map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      let modes = engine.test_modes().map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
      let mut results = Vec::new();
      let mut ran = false;
      for (i, name) in names.iter().enumerate() {
        if filter.is_some_and(|f| !name.contains(f) && !rel.contains(f)) {
          continue;
        }
        match modes.get(i).map(String::as_str) {
          Some("skip") => {
            results.push((name.clone(), Outcome::Skipped));
            continue;
          }
          Some("todo") => {
            results.push((name.clone(), Outcome::Todo));
            continue;
          }
          _ => ran = true,
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
          (Ok(()), false) => Outcome::Passed,
          (Ok(()), true) => Outcome::Failed(format!("console.error during the test:\n{}", indent(&log))),
          (Err(failure), _) if log.is_empty() => Outcome::Failed(indent(&failure)),
          (Err(failure), _) => Outcome::Failed(format!("{}\nconsole during the test:\n{}", indent(&failure), indent(&log))),
        };
        engine.eval_string("__sf.discard(document.body); document.body.innerHTML = \"\"; \"\"").map_err(|e| BuildError::Dev(format!("{rel}: {e}")))?;
        results.push((name.clone(), result));
      }
      if ran {
        engine.take_console();
        let finished = match engine.finish().await {
          Ok(result) => result,
          Err(e) => Err(e.to_string()),
        };
        if let Err(failure) = finished {
          results.push(("afterAll".to_owned(), Outcome::Failed(indent(&failure))));
        }
      }
      Ok(results)
    }));
    for (name, result) in outcome? {
      summary.record(&rel, &name, result);
    }
  }
  Ok(())
}

/// The stock host over the app, for the routes a spec loads, when the configuration the host reads is beside the app; the services are the spec's mocks.
fn page_host(
  app: &Path,
  built: &Built,
  contract: &Arc<Contract>,
  transport: Arc<JsTransport>,
  natives: &[String],
  native_modules: &[String],
  calls: JsCalls,
  current: Arc<AtomicU32>,
) -> Result<Option<Arc<Host>>, BuildError> {
  let root = serve::project_root(app);
  let config = match Config::load(&root) {
    Ok(config) => config,
    Err(HostError::NoConfig(_)) => return Ok(None),
    Err(e) => return Err(BuildError::Dev(format!("{}: {e}", root.display()))),
  };
  if config.app.canonicalize().ok().as_deref() != Some(app) {
    return Ok(None);
  }
  let mut config = config;
  config.session.store = "memory".to_owned();
  config.session.client = None;
  if let Some(cache) = config.cache.as_mut() {
    cache.data = None;
  }
  let artifact = snapfire_fsr_host::Artifact {
    config,
    plan: built.manifest.to_json(),
    contract: Some((**contract).clone()),
  };
  let host = Host::from_artifact(artifact)
    .and_then(|builder| {
      let mut builder = builder.services_over(transport);
      for name in natives {
        builder = builder.extension(name.clone(), snapfire_fsr_ir::Reach::Render, browser_half(name.clone()));
      }
      // A `ctx.native` module is Rust this runner cannot link, so the spec's
      // own function answers it, under whichever context is current.
      for module in native_modules {
        builder = builder.native(
          module.clone(),
          Arc::new(JsNative { ctx: None, current: current.clone(), module: module.clone(), calls: calls.clone() }),
        );
      }
      builder.build()
    })
    .map_err(|e| BuildError::Dev(format!("the host that renders a loaded route: {e}")))?;
  Ok(Some(Arc::new(host)))
}

/// Every native pair the plan calls: an extension call whose name is not the
/// standard library's.
fn native_names(manifest: &Manifest) -> Vec<String> {
  let mut names: Vec<String> = Vec::new();
  let mut note = |e: &snapfire_fsr_ir::Expr| {
    if let snapfire_fsr_ir::Expr::Ext { module, name, .. } = e {
      if snapfire_fsr_ir::standard_reach(module, name).is_none() {
        let key = format!("{module}.{name}");
        if !names.contains(&key) {
          names.push(key);
        }
      }
    }
  };
  let bodies = manifest
    .sources
    .iter()
    .flat_map(|s| [s.body.as_ref(), s.meta.as_ref(), s.store.as_ref()])
    .chain(manifest.actions.iter().map(|a| a.body.as_ref()))
    .chain(manifest.handlers.iter().map(|h| h.body.as_ref()))
    .chain([manifest.middleware.as_ref()])
    .flatten();
  for body in bodies {
    snapfire_fsr_ir::body_visit(body, &mut note);
  }
  for component in &manifest.components {
    component.body.visit(&mut note);
    for handler in &component.body.handlers {
      snapfire_fsr_ir::body_visit(&handler.body, &mut note);
    }
  }
  names
}

/// Every `ctx.native` module the plan calls, so the runner can answer each
/// with the spec's own function.
fn native_modules(manifest: &Manifest) -> Vec<String> {
  let mut names: Vec<String> = Vec::new();
  let mut note = |e: &snapfire_fsr_ir::Expr| {
    if let snapfire_fsr_ir::Expr::NativeCall { module, .. } = e {
      if !names.contains(module) {
        names.push(module.clone());
      }
    }
  };
  let bodies = manifest
    .sources
    .iter()
    .flat_map(|s| [s.body.as_ref(), s.meta.as_ref(), s.store.as_ref()])
    .chain(manifest.actions.iter().map(|a| a.body.as_ref()))
    .chain(manifest.handlers.iter().map(|h| h.body.as_ref()))
    .chain([manifest.middleware.as_ref()])
    .flatten();
  for body in bodies {
    snapfire_fsr_ir::body_visit(body, &mut note);
  }
  names
}

/// The Rust half `fsr test` gives a native pair: its browser half, reached
/// through the engine, since the application's Rust does not run here.
fn browser_half(name: String) -> impl Fn(&snapfire_fsr_ir::Ambient, &[Value]) -> Result<Value, snapfire_fsr_ir::Fail> + Send + Sync + 'static {
  move |_, args| {
    let json = value_to_json(&Value::seq(args.to_vec())).to_string();
    let answer = snapfire_fsr_engine::native(&name, &json).map_err(|m| snapfire_fsr_ir::Fail::new(snapfire_fsr_runtime::FailureKind::Internal, m))?;
    let json: serde_json::Value = serde_json::from_str(&answer).map_err(|e| snapfire_fsr_ir::Fail::new(snapfire_fsr_runtime::FailureKind::Internal, format!("{name}: {e}")))?;
    json_to_value(&json).map_err(|e| snapfire_fsr_ir::Fail::new(snapfire_fsr_runtime::FailureKind::Internal, format!("{name}: {e}")))
  }
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
  /// Specifier to `(the URL it was fetched from, file under the test vendor dir)`, so a changed URL fetches again.
  #[serde(default)]
  entries: BTreeMap<String, (String, String)>,
}

/// The development builds of linkedom and of the React modules the app vendors, fetched once from esm.sh into `.fsr-test/vendor`; by specifier.
fn test_vendor(app: &Path, layout: &Layout, test_dir: &Path, shell: Option<&crate::ShellContract>) -> Result<HashMap<String, PathBuf>, BuildError> {
  let dir = test_dir.join("vendor");
  let manifest_path = dir.join("manifest.json");
  let mut manifest: TestVendor = match std::fs::read_to_string(&manifest_path) {
    Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
    Err(_) => TestVendor::default(),
  };
  let vendored = VendorManifest::read(app, layout)?;
  let served = imports_of(&vendor::read_import_map(app, layout)?);
  let mut wanted: Vec<(String, String)> = vec![("linkedom".to_owned(), format!("{ESM_HOST}/linkedom@{LINKEDOM}/worker?target=es2022&bundle&dev"))];
  for specifier in DEV_BUILDS {
    let package = vendor::package_of(specifier);
    let from_vendor = vendored
      .packages
      .get(&package)
      .filter(|entry| entry.entries.contains_key(*specifier))
      .map(|entry| (entry.version.clone(), entry.externals.clone()));
    // A package the map serves at its own root is external here, so the three
    // development builds share one React rather than carrying a copy each.
    let from_shell = match shell {
      Some(contract) if from_vendor.is_none() && served.contains_key(*specifier) => contract.frameworks.get(&package).map(|version| {
        let mut externals: Vec<String> = DEV_BUILDS
          .iter()
          .map(|dev| vendor::package_of(dev))
          .filter(|shared| served.contains_key(shared.as_str()) && !(shared == &package && *specifier == package))
          .collect();
        externals.sort();
        externals.dedup();
        (version.clone(), externals)
      }),
      _ => None,
    };
    let Some((version, externals)) = from_vendor.or(from_shell) else { continue };
    let mut url = format!("{ESM_HOST}/{package}@{version}");
    if let Some(sub) = specifier.strip_prefix(&format!("{package}/")) {
      url.push('/');
      url.push_str(sub);
    }
    url.push_str("?target=es2022&bundle&dev");
    if !externals.is_empty() {
      url.push_str("&external=");
      url.push_str(&externals.join(","));
    }
    wanted.push((specifier.to_string(), url));
  }
  let mut out = HashMap::new();
  let mut client = None;
  for (specifier, url) in wanted {
    if let Some((have, file)) = manifest.entries.get(&specifier) {
      let path = dir.join(file);
      if *have == url && path.is_file() {
        out.insert(specifier, path);
        continue;
      }
    }
    let client = match &client {
      Some(c) => c,
      None => client.insert(vendor::client()?),
    };
    let file = fetch_bundle(client, &url, &dir, &specifier)?;
    manifest.entries.insert(specifier.clone(), (url, file.clone()));
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
fn write_config(app: &Path, layout: &Layout, test_dir: &Path, browser_routes: &[String]) -> Result<(), BuildError> {
  let mut tsconfig = String::from("{\n  \"compilerOptions\": {\n    \"target\": \"es2022\",\n    \"outDir\": \"dist\",\n    \"rootDir\": \"..\",\n    \"sourceMap\": true,\n    \"jsx\": \"react-jsx\",\n    \"paths\": {\n");
  let aliases: Vec<(String, String)> = snapfire_fsr_lower::ALIASES.iter().map(|(alias, dir)| (format!("{alias}*"), format!("../{dir}*"))).collect();
  for (i, (from, to)) in aliases.iter().enumerate() {
    tsconfig.push_str(&format!("      \"{from}\": [\"{to}\"]{}\n", if i + 1 == aliases.len() { "" } else { "," }));
  }
  let mut include: Vec<String> = vec!["../src/**/*".to_owned(), "../ext/**/*".to_owned()];
  include.extend(browser_routes.iter().map(|file| format!("../{file}")));
  include.extend(["../generated/islands.ts", "../generated/client.ts", "../tests/**/*.spec.tsx", "../tests/**/*.spec.ts"].map(str::to_owned));
  let include: Vec<String> = include.into_iter().map(|i| format!("\"{i}\"")).collect();
  tsconfig.push_str(&format!("    }}\n  }},\n  \"include\": [{}]\n}}\n", include.join(", ")));
  let path = test_dir.join("tsconfig.json");
  std::fs::write(&path, tsconfig).map_err(|e| BuildError::Io(path, e))?;
  let mut map = vendor::read_import_map(app, layout)?;
  let mut imports = imports_of(&map);
  imports.insert(TESTING_SPECIFIER.to_owned(), serde_json::Value::String(TESTING_URL.to_owned()));
  imports.entry(STD_SPECIFIER.to_owned()).or_insert_with(|| serde_json::Value::String(STD_URL.to_owned()));
  map.insert("imports".to_owned(), serde_json::Value::Object(imports));
  let text = serde_json::to_string_pretty(&serde_json::Value::Object(map)).expect("serialisable");
  let path = test_dir.join("importmap.json");
  std::fs::write(&path, text).map_err(|e| BuildError::Io(path, e))?;
  Ok(())
}

fn imports_of(map: &serde_json::Map<String, serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
  map.get("imports").and_then(|v| v.as_object()).cloned().unwrap_or_default()
}

fn compile(app: &Path, test_dir: &Path, bundle: &str) -> Result<(), BuildError> {
  let snapfirec = dev::find_snapfirec(None);
  let config = format!("{TEST_DIR}/tsconfig.json");
  let import_map = format!("{TEST_DIR}/importmap.json");
  let mut command = Command::new(&snapfirec);
  command.current_dir(app).args(["--config", &config, "--source-map", "--public-path", bundle, "--import-map", &import_map]);
  if app.join(dev::BUNDLE_OVERLAY).is_dir() {
    command.args(["--overlay", dev::BUNDLE_OVERLAY]);
  }
  let output = command
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

/// Every ctx's service calls in order, by ctx id.
type Records = Arc<Mutex<HashMap<u32, Vec<Value>>>>;

/// A mocked service layer whose methods are the spec's own functions, reached through the engine between jobs. One is bound per ctx; the host's is bound to whichever ctx is current.
struct JsTransport {
  ctx: Option<u32>,
  current: Arc<AtomicU32>,
  calls: JsCalls,
  records: Records,
}

/// A native module the spec answers, reached through the engine the same way
/// a mocked service method is. The key is marked `native:` so a module and a
/// service of the same name never collide.
struct JsNative {
  /// The ctx whose mocks answer or the current one when the host holds this
  /// rather than a single spec context.
  ctx: Option<u32>,
  current: Arc<AtomicU32>,
  module: String,
  calls: JsCalls,
}

impl snapfire_fsr_runtime::Native for JsNative {
  fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let id = self.ctx.unwrap_or_else(|| self.current.load(Ordering::Relaxed));
    let key = format!("{id}:native:{}.{}", self.module, method);
    let args = value_to_json(&Value::Map(args)).to_string();
    let calls = self.calls.clone();
    let (module, method) = (self.module.clone(), method.to_owned());
    Box::pin(async move {
      let answer = calls.call(key, args).await.map_err(|m| ServiceError::new(snapfire_fsr_runtime::FailureKind::Unavailable, &module, &method, m))?;
      let json: serde_json::Value = serde_json::from_str(&answer)
        .map_err(|e| ServiceError::new(snapfire_fsr_runtime::FailureKind::Internal, &module, &method, format!("the mock's answer is not JSON: {e}")))?;
      json_to_value(&json).map_err(|e| ServiceError::new(snapfire_fsr_runtime::FailureKind::Internal, &module, &method, format!("the mock's answer does not decode: {e}")))
    })
  }
}

impl Transport for JsTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let id = self.ctx.unwrap_or_else(|| self.current.load(Ordering::Relaxed));
    let mut call = call;
    call.service = crate::unprefixed(&call.service).to_owned();
    let mut record = ValueMap::default();
    record.insert("service".to_owned(), Value::str(call.service.clone()));
    record.insert("method".to_owned(), Value::str(call.method.clone()));
    record.insert("args".to_owned(), Value::Map(call.args.clone()));
    self.records.lock().entry(id).or_default().push(Value::Map(record));
    let key = format!("{id}:{}.{}", call.service, call.method);
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
  /// What a browser's cookie would carry across the calls of one identity journey.
  flow: snapfire_fsr_host::AuthFlow,
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
  #[serde(default)]
  locale: Option<String>,
  #[serde(default)]
  path: Option<String>,
  #[serde(default)]
  host: Option<String>,
  /// `<module>.<method>` for every native the spec answers, since a spec
  /// cannot link the crate the real ones live in.
  #[serde(default)]
  natives: Vec<String>,
}

#[derive(Deserialize)]
struct IdentitySpec {
  subject: String,
  #[serde(default)]
  claims: serde_json::Value,
}

/// Each lowered action by id, with the input type it declares.
type Actions = HashMap<String, (Option<String>, Arc<Body>)>;

type Handlers = Arc<(HashMap<String, (Option<String>, Arc<Body>)>, HandlerMatcher, Option<Arc<Body>>)>;

struct SpecHooks {
  contract: Arc<Contract>,
  actions: Actions,
  handlers: Handlers,
  components: Arc<Components>,
  calls: JsCalls,
  interpreter: Interpreter,
  ctxs: RefCell<Vec<Rc<MockCtx>>>,
  current: Arc<AtomicU32>,
  records: Records,
  host: Option<Arc<Host>>,
}

impl SpecHooks {
  fn new(contract: Arc<Contract>, actions: Actions, handlers: Handlers, components: Arc<Components>, calls: JsCalls, current: Arc<AtomicU32>, records: Records, host: Option<Arc<Host>>, natives: &[String], frameworks: snapfire_fsr_ir::Frameworks) -> Self {
    let mut extensions = snapfire_fsr_ir::Extensions::standard();
    for name in natives {
      extensions.register(name.clone(), snapfire_fsr_ir::Reach::Render, browser_half(name.clone()));
    }
    let interpreter = Interpreter::default().with_extensions(Arc::new(extensions)).with_catalogs(host.as_ref().and_then(|h| h.catalogs())).with_frameworks(frameworks);
    let hooks = Self { contract, actions, handlers, components, calls, interpreter, ctxs: RefCell::new(Vec::new()), current, records, host };
    hooks.reset();
    hooks
  }

  /// Forgets every ctx and its calls and installs the empty one as id 0.
  fn reset(&self) {
    self.ctxs.borrow_mut().clear();
    self.records.lock().clear();
    self.current.store(0, Ordering::Relaxed);
    let empty = self.build(CtxSpec { session: serde_json::Value::Null, params: BTreeMap::new(), query: BTreeMap::new(), input: serde_json::Value::Null, identity: None, locale: None, path: None, host: None, natives: Vec::new() }).expect("the empty ctx builds");
    self.ctxs.borrow_mut().push(Rc::new(empty));
  }

  fn build(&self, spec: CtxSpec) -> Result<MockCtx, String> {
    let session = match &spec.session {
      serde_json::Value::Null => ValueMap::default(),
      json => match json_to_value(json).map_err(|e| format!("session: {e}"))? {
        Value::Map(map) => map,
        _ => return Err("session must be an object".to_owned()),
      },
    };
    let identity = match spec.identity {
      Some(id) => {
        let claims = match &id.claims {
          serde_json::Value::Null => ValueMap::default(),
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
    let transport = Arc::new(JsTransport { ctx: Some(id), current: self.current.clone(), calls: self.calls.clone(), records: self.records.clone() });
    let services = Services::builder().contract((*self.contract).clone()).default_transport(transport).build();
    let handle = services.bind(identity.clone(), Arc::new(snapfire_fsr_service::NoCredentials));
    let params: Params = spec.params.into_iter().collect();
    let query: Params = spec.query.into_iter().collect();
    let locale = self.locale_of(spec.locale.as_deref());
    let mut natives = snapfire_fsr_runtime::Natives::new();
    for named in &spec.natives {
      let module = named.split('.').next().unwrap_or_default().to_owned();
      if !module.is_empty() {
        natives.register(module.clone(), Arc::new(JsNative { ctx: Some(id), current: self.current.clone(), module, calls: self.calls.clone() }));
      }
    }
    let natives = snapfire_fsr_runtime::NativeHandle::new(Arc::new(natives));
    let config = self.host.as_ref().map(|h| h.public()).unwrap_or_default();
    let mut ctx = RequestCtx::anonymous(params);
    ctx.query = query;
    ctx.path = spec.path.unwrap_or_default();
    ctx.session = SessionCell::new(session, identity);
    ctx.locale = locale;
    ctx.host = spec.host;
    ctx.config = config;
    ctx.services = handle;
    ctx.natives = natives;
    Ok(MockCtx { ctx, input, flow: snapfire_fsr_host::AuthFlow::new() })
  }

  /// The locale a ctx runs under: the one it names, else the host's default,
  /// else `en`, which is what a document without a `[locales]` section says.
  fn locale_of(&self, named: Option<&str>) -> snapfire_fsr_runtime::Locale {
    let default = self.host.as_ref().map(|h| h.locales().default.clone()).unwrap_or_else(|| "en".to_owned());
    match named {
      Some(tag) => snapfire_fsr_runtime::Locale::new(tag, tag == default),
      None => snapfire_fsr_runtime::Locale::new(default, true),
    }
  }

  fn get(&self, id: u32) -> Result<Rc<MockCtx>, String> {
    self.ctxs.borrow().get(id as usize).cloned().ok_or_else(|| format!("no ctx {id}"))
  }
}

fn json_response(status: u16, json: serde_json::Value) -> FetchResponse {
  FetchResponse { headers: Vec::new(), status, body: json.to_string() }
}

/// The `303` a form post is answered with, as the host answers it.
fn see_other(location: &str) -> FetchResponse {
  FetchResponse { headers: vec![("location".to_owned(), location.to_owned())], status: 303, body: String::new() }
}

fn header_of<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
  headers.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|(_, value)| value.as_str())
}

fn is_form(headers: &[(String, String)]) -> bool {
  header_of(headers, "content-type").is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"))
}

/// Where a form post lands again: the `Referer`'s path on this origin, else
/// `/`, carrying the fragment the action's own query asked for.
fn back_to(headers: &[(String, String)], query: &str) -> String {
  let back = header_of(headers, "referer").and_then(snapfire_fsr_host::referer_path).unwrap_or_else(|| "/".to_owned());
  match snapfire_fsr_host::fragment_of(query) {
    Some(slot) => snapfire_fsr_host::with_fragment(&back, slot.as_deref()),
    None => back,
  }
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
    self.current.store(id, Ordering::Relaxed);
    Ok(())
  }

  fn session(&self, id: u32) -> Result<String, String> {
    let (session, _) = self.get(id)?.ctx.session.snapshot();
    Ok(value_to_json(&Value::Map(session)).to_string())
  }

  fn locale(&self, id: u32) -> Result<String, String> {
    Ok(self.get(id)?.ctx.locale.tag.clone())
  }

  fn calls(&self, id: u32) -> Result<String, String> {
    self.get(id)?;
    let calls = self.records.lock().get(&id).cloned().unwrap_or_default();
    Ok(value_to_json(&Value::seq(calls)).to_string())
  }

  fn ext(&self, name: &str, args: &str, locale: &str) -> Result<String, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("{name}: {e}"))?;
    let args = match json_to_value(&json).map_err(|e| format!("{name}: {e}"))? {
      Value::Seq(items) => items,
      _ => return Err(format!("{name}: arguments must be an array")),
    };
    let ambient = snapfire_fsr_ir::Ambient { locale: locale.to_owned(), now: 0, catalogs: self.interpreter.catalogs().cloned() };
    let value = self.interpreter.extensions().call(name, &ambient, &args).map_err(|f| f.message)?;
    Ok(value_to_json(&value).to_string())
  }

  fn render(&self, module: &str, props: &str) -> Result<Option<String>, String> {
    let Some(component) = self.components.get(module).cloned() else { return Ok(None) };
    let json: serde_json::Value = serde_json::from_str(props).map_err(|e| format!("props: {e}"))?;
    let mut props = match json_to_value(&json).map_err(|e| format!("props: {e}"))? {
      Value::Map(map) => map,
      Value::Null => ValueMap::default(),
      _ => return Err("props must be an object".to_owned()),
    };
    if let Ok(current) = self.get(self.current.load(Ordering::Relaxed)) {
      props.entry("locale".to_owned()).or_insert_with(|| Value::str(current.ctx.locale.tag.clone()));
    }
    let rendered = self.interpreter.render_module(module, &component, &props, &self.components).map_err(|f| format!("rendering {module}: {}", f.message))?;
    let hoisted = value_to_json(&Value::Map(rendered.hoisted.clone()));
    let html = if rendered.islands.is_empty() { rendered.html } else { snapfire_fsr_payload::html_serialize(&snapfire_fsr_core::Node::Seq(snapfire_fsr_ir::rendered_nodes(&rendered))) };
    Ok(Some(serde_json::json!({ "html": html, "hoisted": hoisted }).to_string()))
  }

  fn fetch(&self, method: String, url: String, body: Option<String>, headers: Vec<(String, String)>) -> LocalBoxFuture<'static, FetchResponse> {
    let target = url.strip_prefix("http://localhost").unwrap_or(&url).to_owned();
    let (path, query) = target.split_once('?').map(|(p, q)| (p.to_owned(), q.to_owned())).unwrap_or((target.clone(), String::new()));
    let mock = match self.get(self.current.load(Ordering::Relaxed)) {
      Ok(mock) => mock,
      Err(m) => return Box::pin(async move { json_response(500, serde_json::json!({ "kind": "internal", "message": m })) }),
    };
    let middleware = self.handlers.2.clone();
    let interpreter = self.interpreter.clone();
    let hooks = self.clone_for_fetch();
    Box::pin(async move {
      let preflight = match middleware {
        Some(body_ir) => {
          let mut request = ValueMap::default();
          request.insert("method".to_owned(), Value::str(method.clone()));
          request.insert("path".to_owned(), Value::str(path.clone()));
          let mut ctx = mock.ctx.clone();
          ctx.params = Params::new();
          ctx.query = parse_query(&query);
          match interpreter.run(&body_ir, &ctx, Some(Value::Map(request))).await {
            Ok(outcome) => match Preflight::from_value(&outcome.value) {
              Ok(preflight) => preflight,
              Err(message) => return json_response(500, serde_json::json!({ "kind": "internal", "message": message })),
            },
            Err(fail) => return json_response(fail.kind.http_status(), serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message })),
          }
        }
        None => Preflight::pass(),
      };
      let (path, query, target) = match &preflight.action {
        PreflightAction::Continue => (path, query, target),
        PreflightAction::Rewrite(to) => {
          let (to_path, to_query) = to.split_once('?').unwrap_or((to.as_str(), ""));
          let query = if to_query.is_empty() { query } else { to_query.to_owned() };
          let target = if query.is_empty() { to_path.to_owned() } else { format!("{to_path}?{query}") };
          (to_path.to_owned(), query, target)
        }
        PreflightAction::Redirect { to, status } => {
          let mut response = FetchResponse::new(*status, "").header("location", to.clone());
          response.headers.extend(preflight.headers.iter().cloned());
          return response;
        }
        PreflightAction::Respond { status, body } => {
          let mut response = match body {
            Value::Null => FetchResponse::new(*status, ""),
            Value::Str(text) => FetchResponse::new(*status, text.clone()),
            other => FetchResponse::new(*status, value_to_json(other).to_string()),
          };
          response.headers.extend(preflight.headers.iter().cloned());
          return response;
        }
      };
      let mut response = hooks.dispatch(method, path, query, target, body, headers).await;
      response.headers.extend(preflight.headers.iter().cloned());
      response
    })
  }
}

impl SpecHooks {
  /// The parts of the hooks a fetch needs once the middleware has run.
  fn clone_for_fetch(&self) -> FetchHooks {
    FetchHooks { actions: self.actions.clone(), handlers: self.handlers.clone(), contract: self.contract.clone(), interpreter: self.interpreter.clone(), current_ctx: self.get(self.current.load(Ordering::Relaxed)).ok(), host: self.host.clone() }
  }
}

struct FetchHooks {
  actions: Actions,
  handlers: Handlers,
  contract: Arc<Contract>,
  interpreter: Interpreter,
  current_ctx: Option<Rc<MockCtx>>,
  host: Option<Arc<Host>>,
}

impl FetchHooks {
  async fn dispatch(&self, method: String, path: String, query: String, target: String, body: Option<String>, headers: Vec<(String, String)>) -> FetchResponse {
    if method == "POST" {
      if let Some(module) = path.strip_prefix("/_sf/island/").map(|m| percent_decode(m)) {
        let Some(host) = &self.host else {
          return json_response(500, serde_json::json!({ "kind": "internal", "message": "an island step needs the configuration beside the app" }));
        };
        let locale = self.current_ctx.as_ref().map(|c| c.ctx.locale.tag.clone()).unwrap_or_default();
        let lowered = host.lowered();
        let step = match snapfire_fsr_host::island_step(lowered.as_deref(), &module, body.as_deref().unwrap_or("").as_bytes(), &locale) {
          Ok(step) => step,
          Err((status, json)) => return json_response(status.as_u16(), json),
        };
        for (id, input) in &step.acts {
          if let Err(response) = self.run_action(id, input.clone(), false).await {
            return response;
          }
        }
        return json_response(200, step.json());
      }
    }
    if path.starts_with("/auth/") {
      return self.auth(method, path, query, body, headers).await;
    }
    let action = path.strip_prefix("/_sf/action/").map(|id| percent_decode(id));
    let Some(id) = action.filter(|_| method == "POST") else {
      if let Some(found) = self.handlers.1.match_request(&method, &path) {
        return self.handler(found.id, found.params, query, body, is_form(&headers)).await;
      }
      return self.page(method, path, query, target, headers).await;
    };
    let Some(mock) = self.current_ctx.clone() else {
      return json_response(500, serde_json::json!({ "kind": "internal", "message": "no current ctx" }));
    };
    let form = is_form(&headers);
    let input = if form {
      let mut fields = ValueMap::default();
      for (key, value) in parse_query(body.as_deref().unwrap_or("")) {
        if key != "_csrf" {
          fields.insert(key, Value::str(value));
        }
      }
      Value::Map(fields)
    } else {
      match body.as_deref().map(serde_json::from_str::<serde_json::Value>) {
        Some(Ok(json)) => match json_to_value(&json) {
          Ok(value) => value,
          Err(e) => return json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })),
        },
        Some(Err(e)) => return json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })),
        None => mock.input.clone().unwrap_or(Value::Null),
      }
    };
    match self.run_action(&id, input, form).await {
      Ok(_) if form => see_other(&back_to(&headers, &query)),
      Ok(value) => json_response(200, value_to_json(&value)),
      Err(response) => response,
    }
  }

  /// A lowered action run under the current ctx: the input read against the
  /// action's declared type, as text when `form`, then the body. A failure
  /// is the response to answer with.
  async fn run_action(&self, id: &str, mut input: Value, form: bool) -> Result<Value, FetchResponse> {
    let Some((input_type, body_ir)) = self.actions.get(id).cloned() else {
      let message = format!("`{id}` is not a lowered action; a test can only call what the build lowered");
      return Err(json_response(501, serde_json::json!({ "kind": "internal", "message": message })));
    };
    let Some(mock) = self.current_ctx.clone() else {
      return Err(json_response(500, serde_json::json!({ "kind": "internal", "message": "no current ctx" })));
    };
    if let Some(name) = input_type {
      let ty = Type::Named(name);
      match form {
        true => self.contract.conform_text(&ty, &mut input),
        false => self.contract.conform(&ty, &mut input),
      }
      if let Err(e) = self.contract.check_value(&ty, &input, "input") {
        return Err(json_response(400, serde_json::json!({ "kind": "invalid", "message": e.to_string() })));
      }
    }
    match self.interpreter.run(&body_ir, &mock.ctx, Some(input)).await {
      Ok(outcome) => Ok(outcome.value),
      Err(fail) => Err(json_response(fail.kind.http_status(), serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message }))),
    }
  }

  /// The identity routes through the host, against the spec's own session, so
  /// a spec signs in the way a browser does and every later render sees the
  /// identity the flow settled. A redirect comes back as it does at the edge,
  /// which the spec follows itself.
  async fn auth(&self, method: String, path: String, query: String, body: Option<String>, headers: Vec<(String, String)>) -> FetchResponse {
    let Some(host) = self.host.clone() else {
      return json_response(500, serde_json::json!({ "kind": "internal", "message": "the identity routes need the configuration beside the app" }));
    };
    let Some(mock) = self.current_ctx.clone() else {
      return FetchResponse::new(500, "no current ctx");
    };
    let answered = host.auth_call(&mock.flow, &method, &path, &query, body.unwrap_or_default().as_bytes(), &headers, mock.ctx.session.clone()).await;
    let Some((status, headers, text)) = answered else {
      let message = format!("{method} {path}: this application mounts no identity provider, or that is not one of its three routes");
      return json_response(404, serde_json::json!({ "kind": "not_found", "message": message }));
    };
    let mut response = FetchResponse::new(status, text);
    for (name, value) in headers {
      response = response.header(name, value);
    }
    response
  }

  /// A lowered handler run under the current ctx with the matched params, the
  /// URL's query and the request body as its input. `form` says the body was
  /// urlencoded, whose values are all text.
  async fn handler(&self, id: String, params: Params, query: String, body: Option<String>, form: bool) -> FetchResponse {
    let Some((input_type, body_ir)) = self.handlers.0.get(&id).cloned() else {
      return json_response(501, serde_json::json!({ "kind": "internal", "message": format!("`{id}` is not a lowered handler") }));
    };
    let Some(mock) = self.current_ctx.clone() else {
      return json_response(500, serde_json::json!({ "kind": "internal", "message": "no current ctx" }));
    };
    let mut input = if form {
      let mut fields = ValueMap::default();
      for (key, value) in parse_query(body.as_deref().unwrap_or("")) {
        if key != "_csrf" {
          fields.insert(key, Value::str(value));
        }
      }
      Value::Map(fields)
    } else {
      match body.as_deref().filter(|b| !b.is_empty()).map(serde_json::from_str::<serde_json::Value>) {
        Some(Ok(json)) => match json_to_value(&json) {
          Ok(value) => value,
          Err(e) => return json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid request body: {e}") })),
        },
        Some(Err(e)) => return json_response(400, serde_json::json!({ "kind": "invalid", "message": format!("invalid request body: {e}") })),
        None => Value::Null,
      }
    };
    if let Some(name) = input_type {
      let ty = Type::Named(name);
      match form {
        true => self.contract.conform_text(&ty, &mut input),
        false => self.contract.conform(&ty, &mut input),
      }
      if let Err(e) = self.contract.check_value(&ty, &input, "input") {
        return json_response(400, serde_json::json!({ "kind": "invalid", "message": e.to_string() }));
      }
    }
    let mut ctx = mock.ctx.clone();
    ctx.params = params;
    ctx.query = parse_query(&query);
    match self.interpreter.run(&body_ir, &ctx, Some(input)).await {
      Ok(outcome) => json_response(200, value_to_json(&outcome.value)),
      Err(fail) => json_response(fail.kind.http_status(), serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message })),
    }
  }

  /// A route rendered by the host under the current ctx: the document; the wire payload when the query carries `__payload`; one segment as markup when it carries `__fragment`. A payload is intercepted the way the host would when the navigator says where it comes from.
  async fn page(&self, method: String, path: String, query: String, target: String, headers: Vec<(String, String)>) -> FetchResponse {
    let Some(host) = self.host.clone() else {
      let message = format!("fsr test answers POST /_sf/action/<id>, and GET of a route when config/app.toml is beside the app; not {method} {path}");
      return json_response(404, serde_json::json!({ "kind": "not_found", "message": message }));
    };
    if method != "GET" {
      return FetchResponse::new(405, format!("{method} {path}: a route is fetched with GET"));
    }
    let Some(mock) = self.current_ctx.clone() else {
      return FetchResponse::new(500, "no current ctx");
    };
    let session = mock.ctx.session.clone();
    let fragment = query.split('&').find_map(|pair| match pair.split_once('=') {
      Some(("__fragment", slot)) => Some(Some(percent_decode(slot))),
      None if pair == "__fragment" => Some(None),
      _ => None,
    });
    let mode = if query.split('&').any(|p| p == "__payload") {
      RenderMode::Payload
    } else if let Some(slot) = fragment {
      RenderMode::Fragment(slot)
    } else {
      RenderMode::Html
    };
    let header = |name: &str| headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str());
    let (from, into) = (header("x-sf-from"), header("x-sf-into"));
    let rendered = if mode == RenderMode::Payload && (from.is_some() || into.is_some()) {
      host.render_navigation_with_status(&target, from, into, session.clone()).await
    } else {
      host.render_with_status(&target, mode.clone(), session.clone()).await
    };
    match rendered {
      Ok((status, body)) => FetchResponse::new(status.as_u16(), body),
      Err(HostError::NoSlot(name)) => FetchResponse::new(404, format!("no slot named `{name}` on this route")),
      Err(HostError::NotFound(path)) => match host.render_not_found(&target, mode, session).await {
        Ok(Some(chunks)) => FetchResponse::new(404, chunks.collect::<Vec<String>>().await.concat()),
        Ok(None) => FetchResponse::new(404, format!("no route: {path}")),
        Err(e) => FetchResponse::new(500, e.to_string()),
      },
      Err(e) => FetchResponse::new(500, e.to_string()),
    }
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
