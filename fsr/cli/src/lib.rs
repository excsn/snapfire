//! Route discovery, the contract and the build that emits a plan file and the
//! generated TypeScript from them. The binary in `main.rs` is a thin front over
//! `build` and `write`.

pub mod dev;
pub mod serve;
pub mod spec;
pub mod infer;
pub mod test;
pub mod types;
pub mod vendor;
pub mod xwpm;

use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::{lower_actions_with, lower_handlers_with, lower_loader_with, lower_meta_with, lower_middleware_with, lower_store_with, read_schema, read_session_defaults, LowerError, SessionDefaults};
use snapfire_fsr_plan::{ActionEntry, Child, ComponentEntry, HandlerEntry, Manifest, Node, RouteEntry, RowOwner, SourceEntry};
use snapfire_fsr_service::typescript::Flavour;
use snapfire_fsr_service::{typescript, Contract, ContractError, ImportError};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("no `routes/` directory under {0}")]
  NoRoutes(PathBuf),
  #[error("{path}: `{name}` is not a route segment; use a name, `[param]` or `[...rest]`")]
  Segment { path: PathBuf, name: String },
  #[error(transparent)]
  Lower(#[from] LowerError),
  #[error("{document}: {error}")]
  Import { document: String, error: ImportError },
  #[error("type `{name}` is declared in both {first} and {second}")]
  DuplicateType { name: String, first: String, second: String },
  #[error("the contract does not hold together: {0}")]
  Contract(#[from] ContractError),
  #[error("action `{action}` names input type `{name}`, which no schema under schemas/ declares")]
  UnknownInput { action: String, name: String },
  #[error("{0}: holds both `page.tsx` and `route.ts`; a directory is a page or a handler")]
  PageAndRoute(PathBuf),
  #[error("{0}: `slots/` belongs beside a `layout.tsx`, and this directory has none")]
  SlotsWithoutLayout(PathBuf),
  #[error("{0}: a slot needs a `page.tsx`")]
  SlotWithoutPage(PathBuf),
  #[error("{0}: a slot holds one `page.tsx` and no routes beneath it")]
  SlotRoute(PathBuf),
  #[error("{path}: `{file}` names slot `{slot}`, which no layout above it declares")]
  SlotUndeclared { path: PathBuf, file: String, slot: String },
  #[error("handler `{handler}` names input type `{name}`, which no schema under schemas/ declares")]
  UnknownHandlerInput { handler: String, name: String },
  #[error("`{0}` is not a package spec; write `name@version` or `name@version/subpath`")]
  Spec(String),
  #[error("{0}: {1}")]
  Http(String, String),
  #[error("{0}: {1}")]
  Manifest(PathBuf, String),
  #[error("`{package}` imports `{wants}`, a package outside its bundle; vendor that package and name it with --external")]
  Dependency { package: String, wants: String },
  #[error("{0}")]
  Xwpm(String),
  #[error("{0}")]
  Dev(String),
  #[error("{0}")]
  Serve(String),
}

/// What `build` found and emitted, in the order the report prints it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
  pub routes: Vec<(String, String)>,
  pub sources: Vec<(String, String)>,
  pub actions: Vec<(String, String)>,
  /// `METHOD pattern` and the `route.ts` that exports it.
  pub handlers: Vec<(String, String)>,
  /// `middleware.ts` when the app has one.
  pub middleware: Option<String>,
  /// The directory pattern a layout wraps and its module.
  pub layouts: Vec<(String, String)>,
  /// A parallel slot's source id and its page module.
  pub slots: Vec<(String, String)>,
  /// `<pattern> into <slot>` and the `page.<slot>.tsx` a soft navigation renders there.
  pub intercepts: Vec<(String, String)>,
  /// Module; `lowered` or `client`; for `client`, the line that decided it.
  pub components: Vec<(String, String, String)>,
  pub services: Vec<(String, String)>,
  pub schemas: Vec<(String, String)>,
  pub types: Vec<(String, String)>,
}

impl fmt::Display for Report {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    section(f, "routes", &self.routes, "")?;
    section(f, "layouts", &self.layouts, "")?;
    section(f, "slots", &self.slots, "")?;
    section(f, "intercepts", &self.intercepts, "")?;
    section(f, "sources", &self.sources, "lowered")?;
    section(f, "actions", &self.actions, "lowered")?;
    section(f, "handlers", &self.handlers, "lowered")?;
    if let Some(module) = &self.middleware {
      writeln!(f, "{:<9} {:<22} {:<11} {module}", "middleware", "middleware", "lowered")?;
    }
    for (i, (module, owner, detail)) in self.components.iter().enumerate() {
      let label = if i == 0 { "rendered" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {owner:<11} {detail}")?;
    }
    for (i, (service, document)) in self.services.iter().enumerate() {
      let label = if i == 0 { "services" } else { "" };
      let kind = if document.ends_with(".proto") { "grpc" } else { "http" };
      writeln!(f, "{label:<9} {service:<22} {kind:<11} {document}")?;
    }
    section(f, "schemas", &self.schemas, "")?;
    section(f, "types", &self.types, "")
  }
}

fn section(f: &mut fmt::Formatter<'_>, label: &str, rows: &[(String, String)], owner: &str) -> fmt::Result {
  for (i, (a, b)) in rows.iter().enumerate() {
    let label = if i == 0 { label } else { "" };
    if owner.is_empty() {
      writeln!(f, "{label:<9} {a:<22} {b}")?;
    } else {
      writeln!(f, "{label:<9} {a:<22} {owner:<11} {b}")?;
    }
  }
  Ok(())
}

/// Where the build writes one contract file per client document plus `schemas.json`; the host merges the directory.
pub const CONTRACTS_DIR: &str = "generated/contracts";
/// Where the build writes the plan file.
pub const PLAN_FILE: &str = "generated/plan.json";

pub struct Options {
  /// The module the document renders through; every route's root node.
  pub shell: String,
  /// The slot of the shell a page lands in.
  pub slot: String,
  /// The module and export the generated island registry mounts pages with.
  pub mounter_module: String,
  pub mounter: String,
  /// The export of the mounter module that re-renders a mounted island with new props.
  pub patcher: String,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      shell: "shell#document".to_owned(),
      slot: "content".to_owned(),
      mounter_module: "@snapfire/fsr-client/react".to_owned(),
      mounter: "reactMounter".to_owned(),
      patcher: "reactPatcher".to_owned(),
    }
  }
}

/// Everything a build produces. `files` are paths relative to the app
/// directory with their contents; `write` puts them on disk.
pub struct Built {
  pub manifest: Manifest,
  pub contract: Contract,
  pub report: Report,
  pub files: Vec<(String, String)>,
  /// The session defaults every body was lowered with, so a test lowers its target the same way.
  pub defaults: SessionDefaults,
}

#[derive(Clone)]
struct Route {
  pattern: String,
  dir: PathBuf,
  id: String,
}

/// Walks `<app>/routes`, `<app>/clients` and `<app>/schemas`, lowers every
/// `page.loader.ts` and `actions.ts`, builds the contract and returns the plan file
/// and the generated TypeScript. Nothing is written; `write` does that.
pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError> {
  let routes_dir = app.join("routes");
  if !routes_dir.is_dir() {
    return Err(BuildError::NoRoutes(app.to_path_buf()));
  }

  let mut report = Report::default();
  let mut contract = Contract::new();
  let mut contracts: Vec<(String, Contract)> = Vec::new();
  let mut session_import: Option<String> = None;
  let mut defaults = SessionDefaults::new();

  for document in sorted_files(&app.join("clients"), ".openapi.json")? {
    let file = document.file_name().unwrap_or_default().to_string_lossy().to_string();
    let name = file.trim_end_matches(".openapi.json").to_owned();
    let text = std::fs::read_to_string(&document).map_err(|e| BuildError::Io(document.clone(), e))?;
    let imported = snapfire_fsr_service::import(&text, &name)
      .map_err(|error| BuildError::Import { document: format!("clients/{file}"), error })?;
    for service in imported.contract.services.keys() {
      report.services.push((service.clone(), format!("clients/{file}")));
    }
    contract.merge(imported.contract.clone(), &format!("clients/{file}"))?;
    contracts.push((format!("{CONTRACTS_DIR}/{name}.json"), imported.contract));
  }
  for document in sorted_files(&app.join("clients"), ".proto")? {
    let file = document.file_name().unwrap_or_default().to_string_lossy().to_string();
    let name = file.trim_end_matches(".proto").to_owned();
    let imported = snapfire_fsr_service::import_proto(&document, &name)
      .map_err(|error| BuildError::Import { document: format!("clients/{file}"), error })?;
    for service in imported.contract.services.keys() {
      report.services.push((service.clone(), format!("clients/{file}")));
    }
    contract.merge(imported.contract.clone(), &format!("clients/{file}"))?;
    contracts.push((format!("{CONTRACTS_DIR}/{name}.json"), imported.contract));
  }

  let mut schemas = Contract::new();
  for schema in sorted_files(&app.join("schemas"), ".ts")? {
    let file = schema.file_name().unwrap_or_default().to_string_lossy().to_string();
    let rel = format!("schemas/{file}");
    let text = std::fs::read_to_string(&schema).map_err(|e| BuildError::Io(schema.clone(), e))?;
    for ty in read_schema(&rel, &text)? {
      if let Some((_, first)) = report.schemas.iter().find(|(n, _)| *n == ty.name) {
        return Err(BuildError::DuplicateType { name: ty.name, first: first.clone(), second: rel });
      }
      if ty.name == "Session" {
        session_import = Some(format!("../schemas/{}", file.trim_end_matches(".ts")));
        defaults = read_session_defaults(&rel, &text)?;
      }
      report.schemas.push((ty.name.clone(), rel.clone()));
      schemas.types.insert(ty.name, ty.def);
    }
  }
  contract.merge(schemas.clone(), "schemas/")?;
  contracts.push((format!("{CONTRACTS_DIR}/schemas.json"), schemas));
  contract.validate()?;

  let mut routes = Vec::new();
  let mut handler_routes = Vec::new();
  discover(&routes_dir, &routes_dir, &mut routes, &mut handler_routes)?;
  routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));
  handler_routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));

  let error_module = ["error.tsx", "error.ts"]
    .iter()
    .find(|f| routes_dir.join(f).is_file())
    .map(|f| format!("routes/{f}#default"));

  let not_found_module = ["not-found.tsx", "not-found.ts"]
    .iter()
    .find(|f| routes_dir.join(f).is_file())
    .map(|f| format!("routes/{f}#default"));

  let mut entries = Vec::new();
  let mut sources = Vec::new();
  let mut actions = Vec::new();
  let mut islands: Vec<String> = Vec::new();
  for module in [&error_module, &not_found_module].into_iter().flatten() {
    islands.push(module.clone());
  }

  let mut layouts: Vec<LayoutInfo> = Vec::new();
  let mut layout_ids: Vec<String> = Vec::new();
  for dir in routes.iter().chain(handler_routes.iter()).flat_map(|r| r.dir.ancestors().map(Path::to_path_buf).collect::<Vec<_>>()) {
    if !dir.starts_with(&routes_dir) || !dir.join("layout.tsx").is_file() || layouts.iter().any(|l| l.dir == dir) {
      continue;
    }
    let rel = dir.strip_prefix(app).unwrap_or(&dir).to_string_lossy().replace('\\', "/");
    let (prefix, dir_id) = pattern_of(&routes_dir, &dir)?;
    let id = if dir == routes_dir { "layout".to_owned() } else { format!("{dir_id}.layout") };
    let module = format!("{rel}/layout.tsx#default");
    let loader = dir.join("layout.loader.ts");
    let source = if loader.is_file() {
      let loader_module = format!("{rel}/layout.loader.ts");
      let text = std::fs::read_to_string(&loader).map_err(|e| BuildError::Io(loader.clone(), e))?;
      let body = lower_loader_with(&loader_module, &text, &defaults)?;
      let meta = lower_meta_with(&loader_module, &text, &defaults)?;
      let store = lower_store_with(&loader_module, &text, &defaults)?;
      sources.push(SourceEntry::lowered(id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store));
      report.sources.push((id.clone(), loader_module));
      Some(id.clone())
    } else {
      None
    };
    report.layouts.push((prefix, module.clone()));
    islands.push(module.clone());
    layout_ids.push(id.clone());
    let mut slots = Vec::new();
    for slot_dir in sorted_dirs(&dir.join("slots"))? {
      let name = slot_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
      if !slot_dir.join("page.tsx").is_file() {
        return Err(BuildError::SlotWithoutPage(slot_dir));
      }
      if sorted_dirs(&slot_dir)?.iter().any(|d| d.join("page.tsx").is_file() || d.join("route.ts").is_file()) {
        return Err(BuildError::SlotRoute(slot_dir));
      }
      let slot_rel = format!("{rel}/slots/{name}");
      let slot_id = format!("{id}.{name}");
      let page = format!("{slot_rel}/page.tsx#default");
      let loader = slot_dir.join("page.loader.ts");
      let source = if loader.is_file() {
        let loader_module = format!("{slot_rel}/page.loader.ts");
        let text = std::fs::read_to_string(&loader).map_err(|e| BuildError::Io(loader.clone(), e))?;
        let body = lower_loader_with(&loader_module, &text, &defaults)?;
        let meta = lower_meta_with(&loader_module, &text, &defaults)?;
        let store = lower_store_with(&loader_module, &text, &defaults)?;
        sources.push(SourceEntry::lowered(slot_id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store));
        report.sources.push((slot_id.clone(), loader_module));
        Some(slot_id.clone())
      } else {
        None
      };
      let loading = ["loading.tsx", "loading.ts"].iter().find(|f| slot_dir.join(f).is_file()).map(|f| format!("{slot_rel}/{f}#default"));
      let error = ["error.tsx", "error.ts"].iter().find(|f| slot_dir.join(f).is_file()).map(|f| format!("{slot_rel}/{f}#default"));
      for module in [Some(&page), loading.as_ref(), error.as_ref()].into_iter().flatten() {
        islands.push(module.clone());
      }
      report.slots.push((slot_id.clone(), page.clone()));
      layout_ids.push(slot_id);
      slots.push(SlotInfo { name, page, source, loading, error });
    }
    layouts.push(LayoutInfo { dir, module, source, slots, placed: Vec::new() });
  }
  layouts.sort_by(|a, b| a.dir.cmp(&b.dir));

  let mut set = ComponentSet::new(app);
  set.layouts = layouts.iter().map(|l| l.module.clone()).collect();
  set.slots = layouts.iter().map(|l| (l.module.clone(), l.slots.iter().map(|s| s.name.clone()).collect())).collect();
  for layout in &mut layouts {
    lower_into(&mut set, &layout.module, &mut report)?;
    if let Some((_, component)) = set.components.iter().find(|(m, _)| *m == layout.module) {
      layout.placed = slots_placed(&component.render);
    }
  }
  let mut intercepts = Vec::new();

  for route in &routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let page = format!("{rel}/page.tsx#default");
    report.routes.push((route.pattern.clone(), rel.clone()));

    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| route.dir.starts_with(&l.dir)).collect();
    let loader = route.dir.join("page.loader.ts");
    let source = if loader.is_file() {
      let module = format!("{rel}/page.loader.ts");
      let text = std::fs::read_to_string(&loader).map_err(|e| BuildError::Io(loader.clone(), e))?;
      let body = lower_loader_with(&module, &text, &defaults)?;
      let meta = lower_meta_with(&module, &text, &defaults)?;
      let store = lower_store_with(&module, &text, &defaults)?;
      sources.push(SourceEntry::lowered(route.id.clone(), module.clone(), body).with_meta(meta).with_store(store));
      report.sources.push((route.id.clone(), module));
      Some(route.id.clone())
    } else {
      None
    };

    let actions_file = route.dir.join("actions.ts");
    if actions_file.is_file() {
      let module = format!("{rel}/actions.ts");
      let text = std::fs::read_to_string(&actions_file).map_err(|e| BuildError::Io(actions_file.clone(), e))?;
      for lowered in lower_actions_with(&module, &text, &defaults)? {
        let id = format!("{}.{}", route.id, lowered.export);
        if let Some(name) = &lowered.input {
          if !contract.types.contains_key(name) {
            return Err(BuildError::UnknownInput { action: id, name: name.clone() });
          }
        }
        let mut entry = ActionEntry::lowered(id.clone(), module.clone(), lowered.body);
        entry.export = Some(lowered.export);
        entry.input = lowered.input;
        actions.push(entry);
        report.actions.push((id, module.clone()));
      }
    }

    let local_error = ["error.tsx", "error.ts"]
      .iter()
      .find(|f| route.dir.join(f).is_file())
      .map(|f| format!("{rel}/{f}#default"));
    let loading = ["loading.tsx", "loading.ts"]
      .iter()
      .find(|f| route.dir.join(f).is_file())
      .map(|f| format!("{rel}/{f}#default"));

    for module in [Some(&page), local_error.as_ref(), loading.as_ref()].into_iter().flatten() {
      if !islands.contains(module) {
        islands.push(module.clone());
      }
    }

    let content = Node {
      id: 0,
      module: page.clone(),
      source: source.clone(),
      deferred: loading.is_some(),
      fallback: loading.clone(),
      error: local_error.clone().or_else(|| error_module.clone()),
      cache_key: Some(page),
      children: Vec::new(),
      keep: Vec::new(),
    };
    let content = wrap_in_layouts(content, &wrapping, error_module.as_deref());
    entries.push(RouteEntry { pattern: route.pattern.clone(), plan: shell_over(options, content) });

    for (file, slot) in variant_files(&route.dir)? {
      let module = format!("{rel}/{file}#default");
      let Some(declaring) = wrapping.iter().rposition(|l| l.declares(&slot)) else {
        return Err(BuildError::SlotUndeclared { path: route.dir.clone(), file, slot });
      };
      islands.push(module.clone());
      let slot_loading = [format!("loading.{slot}.tsx"), format!("loading.{slot}.ts")].iter().find(|f| route.dir.join(f).is_file()).map(|f| format!("{rel}/{f}#default"));
      if let Some(loading) = &slot_loading {
        islands.push(loading.clone());
      }
      let variant = Node {
        id: 0,
        module: module.clone(),
        source: source.clone(),
        deferred: slot_loading.is_some(),
        fallback: slot_loading,
        error: local_error.clone().or_else(|| error_module.clone()),
        cache_key: Some(module.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      };
      let plan = intercept_plan(variant, &slot, &wrapping[..=declaring], error_module.as_deref());
      report.intercepts.push((format!("{} into {slot}", route.pattern), module));
      intercepts.push(RouteEntry { pattern: route.pattern.clone(), plan: shell_over(options, plan) });
    }
  }

  let mut handlers = Vec::new();
  for route in &handler_routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let file = route.dir.join("route.ts");
    let module = format!("{rel}/route.ts");
    let text = std::fs::read_to_string(&file).map_err(|e| BuildError::Io(file.clone(), e))?;
    for lowered in lower_handlers_with(&module, &text, &defaults)? {
      let id = format!("{}.{}", route.id, lowered.method);
      if let Some(name) = &lowered.input {
        if !contract.types.contains_key(name) {
          return Err(BuildError::UnknownHandlerInput { handler: id, name: name.clone() });
        }
      }
      let mut entry = HandlerEntry::lowered(id, lowered.method.clone(), route.pattern.clone(), module.clone(), lowered.body);
      entry.input = lowered.input;
      report.handlers.push((format!("{} {}", lowered.method, route.pattern), module.clone()));
      handlers.push(entry);
    }
  }

  let middleware_file = app.join("middleware.ts");
  let middleware = if middleware_file.is_file() {
    let text = std::fs::read_to_string(&middleware_file).map_err(|e| BuildError::Io(middleware_file.clone(), e))?;
    report.middleware = Some("middleware.ts".to_owned());
    Some(lower_middleware_with("middleware.ts", &text, &defaults)?)
  } else {
    None
  };

  let mut not_found = not_found_module.map(|module| {
    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| l.dir == routes_dir).collect();
    let content = Node { id: 0, module: module.clone(), source: None, deferred: false, fallback: None, error: error_module.clone(), cache_key: Some(module), children: Vec::new(), keep: Vec::new() };
    shell_over(options, wrap_in_layouts(content, &wrapping, error_module.as_deref()))
  });
  for plan in entries.iter_mut().map(|e| &mut e.plan).chain(intercepts.iter_mut().map(|e| &mut e.plan)).chain(not_found.iter_mut()) {
    renumber(plan, &mut 0);
  }

  for module in &islands {
    lower_into(&mut set, module, &mut report)?;
  }
  let mut components = Vec::new();
  let mut islands = islands;
  for (module, component) in set.components {
    report.components.push((module.clone(), "lowered".to_owned(), String::new()));
    for placed in island_modules(&component.render) {
      if !islands.contains(&placed) {
        islands.push(placed);
      }
    }
    components.push(ComponentEntry { module, body: component });
  }
  report.components.sort();

  let session_type = session_import.as_ref().map(|_| "Session");
  let client = client_module(&contract, session_type, &routes, &layout_ids, &sources, &actions);
  let manifest = Manifest::new(entries).with_sources(sources).with_actions(actions).with_components(components).with_not_found(not_found).with_handlers(handlers).with_middleware(middleware).with_intercepts(intercepts);
  debug_assert!(manifest.sources.iter().all(|s| s.owner == RowOwner::Lowered));

  let mut files = vec![(PLAN_FILE.to_owned(), manifest.to_json() + "\n")];
  files.extend(contracts.into_iter().map(|(rel, c)| (rel, c.to_json() + "\n")));
  files.extend([
    ("generated/services.d.ts".to_owned(), typescript::declarations(&contract)),
    ("generated/fsr.ts".to_owned(), ctx_module(&routes.iter().chain(handler_routes.iter()).cloned().collect::<Vec<_>>(), session_import.as_deref())),
    ("generated/islands.ts".to_owned(), islands_module(&islands, options)),
    ("generated/client.ts".to_owned(), client),
    ("generated/testing.ts".to_owned(), testing_module()),
    ("tsconfig.json".to_owned(), types::tsconfig(app)?),
    ("tsconfig.build.json".to_owned(), types::tsconfig_build()),
  ]);
  let mut report = report;
  report.types = types::status(app)?;
  Ok(Built { manifest, contract, report, files, defaults })
}

/// Writes every generated file under `<app>` and returns their paths. The
/// contracts directory is emptied first, so a client that was removed leaves
/// no file behind for the host to merge.
pub fn write(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError> {
  let contracts = app.join(CONTRACTS_DIR);
  if contracts.is_dir() {
    for entry in std::fs::read_dir(&contracts).map_err(|e| BuildError::Io(contracts.clone(), e))?.flatten() {
      let path = entry.path();
      if path.extension().is_some_and(|x| x == "json") {
        std::fs::remove_file(&path).map_err(|e| BuildError::Io(path, e))?;
      }
    }
  }
  let mut written = Vec::new();
  for (rel, content) in &built.files {
    let path = app.join(rel);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
    written.push(path);
  }
  Ok(written)
}

/// `generated/fsr.ts`: the per-route params, `Ctx`, `ActionCtx` and the typed
/// `action` and `fail` a body imports.
fn ctx_module(routes: &[Route], session_import: Option<&str>) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import { action as declare, fail } from \"@snapfire/fsr-authoring\";\nimport type { Identity } from \"@snapfire/fsr-authoring\";\nimport type { Services } from \"./services\";\n");
  match session_import {
    Some(path) => {
      let _ = writeln!(out, "import type {{ Session }} from \"{path}\";");
    }
    None => out.push_str("type Session = Record<string, unknown>;\n"),
  }
  out.push_str("\nexport { fail };\nexport type { Services, Session };\n\nexport interface Routes {\n");
  for route in routes {
    let params: Vec<String> = route
      .pattern
      .split('/')
      .filter_map(|segment| segment.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
      .map(|name| format!("{}: string", name.trim_start_matches('*')))
      .collect();
    let _ = writeln!(out, "  \"{}\": {{{}}};", route.pattern, if params.is_empty() { String::new() } else { format!(" {} ", params.join("; ")) });
  }
  out.push_str(
    "}\n\nexport interface Ctx<P extends keyof Routes = keyof Routes> {\n  params: Routes[P];\n  query: Record<string, string>;\n  session: Session;\n  identity: Identity | null;\n  services: Services;\n  now: bigint;\n}\n\nexport interface ActionCtx<Input = void, P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  input: Input;\n}\n\nexport interface RequestLine {\n  method: string;\n  path: string;\n}\n\nexport interface MiddlewareCtx extends Ctx {\n  request: RequestLine;\n}\n\nexport interface Meta {\n  title?: string;\n  description?: string;\n}\n\nexport interface MetaCtx<Data> {\n  data: Data;\n}\n\nexport type DataOf<Load> = Load extends (...args: never[]) => Promise<infer Data> ? Data : never;\n\nexport interface MiddlewareResult {\n  redirect?: string;\n  rewrite?: string;\n  status?: number;\n  body?: unknown;\n  headers?: Record<string, string>;\n}\n\nexport function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): (ctx: ActionCtx<Input>) => Promise<Out> {\n  return declare<Input, Out>(body as never) as never;\n}\n",
  );
  out
}

/// `generated/testing.ts`: what a `*.test.ts` imports from `@snapfire/fsr/testing`.
/// The bodies throw because `fsr test` lowers the file rather than running it;
/// the types are the point.
fn testing_module() -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import type { ActionCtx, RequestLine, Routes, Services, Session } from \"./fsr\";\n\n");
  out.push_str("type Mocked<S> = { [K in keyof S]?: { [M in keyof S[K]]?: S[K][M] extends (args: infer A) => Promise<infer R> ? ((args: A) => R) | R : never } };\n\n");
  out.push_str("export interface Mock<Input = void> {\n  session?: Partial<Session>;\n  services?: Mocked<Services>;\n  input?: Input;\n  request?: RequestLine;\n  params?: Record<string, string>;\n  query?: Record<string, string>;\n  identity?: { subject: string; claims?: Record<string, unknown> };\n}\n\n");
  out.push_str("export interface Trace {\n  calls: { service: string; method: string; args: Record<string, unknown> }[];\n  session: { written: string[] };\n}\n\n");
  out.push_str("export type TestCtx<Input = void, P extends keyof Routes = keyof Routes> = ActionCtx<Input, P> & { trace: Trace; request: RequestLine };\n\n");
  out.push_str("const lowered = (): never => {\n  throw new Error(\"a test file is lowered by `fsr test`, never run as JavaScript\");\n};\n\n");
  out.push_str("export function ctx<Input = void, P extends keyof Routes = keyof Routes>(mock: Mock<Input>): TestCtx<Input, P> {\n  void mock;\n  return lowered();\n}\n\n");
  out.push_str("export function test(name: string, body: () => Promise<void>): void {\n  void name;\n  void body;\n  lowered();\n}\n\n");
  out.push_str("export const assert = {\n  ok(value: unknown, message?: string): void {\n    void value;\n    void message;\n    lowered();\n  },\n  equal(actual: unknown, expected: unknown, message?: string): void {\n    void actual;\n    void expected;\n    void message;\n    lowered();\n  },\n  rejects(run: Promise<unknown> | (() => Promise<unknown>), kind?: string): Promise<void> {\n    void run;\n    void kind;\n    return lowered();\n  },\n};\n");
  out
}

/// `generated/client.ts`: the contract's types as the browser sees them, the
/// props of every page inferred from its loader's return and one typed callable
/// per action, nested by route id.
fn client_module(contract: &Contract, session: Option<&str>, routes: &[Route], layouts: &[String], sources: &[SourceEntry], actions: &[ActionEntry]) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import { action as call } from \"@snapfire/fsr-client\";\n\n");
  out.push_str(&typescript::type_declarations(contract, Flavour::Client));

  for route in routes {
    let props = match sources.iter().find(|s| s.id == route.id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None }.returns(body).print(Flavour::Client),
      None => "{}".to_owned(),
    };
    let _ = writeln!(out, "export type {} = {props};", props_name(&route.id));
  }
  for id in layouts {
    let props = match sources.iter().find(|s| s.id == *id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None }.returns(body).print(Flavour::Client),
      None => "{}".to_owned(),
    };
    let _ = writeln!(out, "export type {} = {props};", props_name(id));
  }
  if !routes.is_empty() || !layouts.is_empty() {
    out.push('\n');
  }

  let mut tree: Vec<(Vec<String>, String)> = Vec::new();
  for action in actions {
    let Some(body) = &action.body else { continue };
    let returns = infer::Inferer { contract, session, input: action.input.as_deref() }.returns(body).print(Flavour::Client);
    let arg = match &action.input {
      Some(input) => format!("input: {input}"),
      None => String::new(),
    };
    let path: Vec<String> = action.id.split('.').map(str::to_owned).collect();
    tree.push((path, format!("call(\"{}\") as unknown as ({arg}) => Promise<{returns}>", action.id)));
  }
  out.push_str("export const actions = {\n");
  write_action_tree(&mut out, &tree, &[], 1);
  out.push_str("};\n");
  out
}

fn write_action_tree(out: &mut String, entries: &[(Vec<String>, String)], prefix: &[String], depth: usize) {
  let indent = "  ".repeat(depth);
  let mut groups: Vec<String> = Vec::new();
  for (path, _) in entries {
    if path.len() > prefix.len() + 1 && path[..prefix.len()] == *prefix && !groups.contains(&path[prefix.len()]) {
      groups.push(path[prefix.len()].clone());
    }
  }
  for group in groups {
    let _ = writeln!(out, "{indent}{group}: {{");
    let mut next = prefix.to_vec();
    next.push(group);
    write_action_tree(out, entries, &next, depth + 1);
    let _ = writeln!(out, "{indent}}},");
  }
  for (path, value) in entries {
    if path.len() == prefix.len() + 1 && path[..prefix.len()] == *prefix {
      let _ = writeln!(out, "{indent}{}: {value},", path[prefix.len()]);
    }
  }
}

/// `index` is `IndexProps`, `product` is `ProductProps`, `admin.users` is
/// `AdminUsersProps`.
fn props_name(id: &str) -> String {
  let mut name = String::new();
  for part in id.split(['.', '-', '_']) {
    let mut chars = part.chars();
    if let Some(first) = chars.next() {
      name.extend(first.to_uppercase());
      name.push_str(chars.as_str());
    }
  }
  name + "Props"
}

/// Every module a component places as an island, in tree order.
fn island_modules(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Island { module, children, .. } => {
        out.push(module.clone());
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Component { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
      Tmpl::Text(_) | Tmpl::Expr(_) | Tmpl::Slot(_) => {}
    }
  }
  walk(tmpl, &mut out);
  out
}

/// `generated/islands.ts`: one `registerIsland` per module discovery named
/// and per component a page or layout places as an island, so the browser
/// mounts exactly what the plan file refers to.
fn islands_module(islands: &[String], options: &Options) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  let _ = writeln!(out, "import {{ registerIsland }} from \"@snapfire/fsr-client\";");
  let _ = writeln!(out, "import {{ {}, {} }} from \"{}\";\n", options.mounter, options.patcher, options.mounter_module);
  out.push_str("export function registerIslands(): void {\n");
  for module in islands {
    let Some((path, export)) = module.split_once('#') else { continue };
    let js = path.rsplit_once('.').map(|(stem, _)| format!("{stem}.js")).unwrap_or_else(|| format!("{path}.js"));
    let _ = writeln!(
      out,
      "  registerIsland(\"{module}\", {{ loader: () => import(\"../{js}\").then((m) => m.{export}), mount: {}, patch: {} }});",
      options.mounter,
      options.patcher
    );
  }
  out.push_str("}\n");
  out
}

fn sorted_files(dir: &Path, suffix: &str) -> Result<Vec<PathBuf>, BuildError> {
  if !dir.is_dir() {
    return Ok(Vec::new());
  }
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_file() && p.file_name().is_some_and(|n| n.to_string_lossy().ends_with(suffix)))
    .collect();
  files.sort();
  Ok(files)
}

struct LayoutInfo {
  dir: PathBuf,
  module: String,
  source: Option<String>,
  /// The parallel slots under its `slots/` directory.
  slots: Vec<SlotInfo>,
  /// The named slots its template places, `content` aside.
  placed: Vec<String>,
}

impl LayoutInfo {
  fn declares(&self, slot: &str) -> bool {
    self.slots.iter().any(|s| s.name == slot) || self.placed.iter().any(|p| p == slot)
  }

  /// Every slot of this layout, `content` first, that `filled` leaves out.
  fn kept(&self, filled: &[String]) -> Vec<String> {
    let mut kept = vec!["content".to_owned()];
    for slot in self.slots.iter().map(|s| &s.name).chain(&self.placed) {
      if !kept.contains(slot) {
        kept.push(slot.clone());
      }
    }
    kept.retain(|slot| !filled.contains(slot));
    kept
  }

  fn node(&self, children: Vec<Child>, keep: Vec<String>, error: Option<&str>) -> Node {
    Node {
      id: 0,
      module: self.module.clone(),
      source: self.source.clone(),
      deferred: false,
      fallback: None,
      error: error.map(str::to_owned),
      cache_key: Some(self.module.clone()),
      children,
      keep,
    }
  }
}

struct SlotInfo {
  name: String,
  page: String,
  source: Option<String>,
  loading: Option<String>,
  error: Option<String>,
}

impl SlotInfo {
  fn child(&self, error: Option<&str>) -> Child {
    Child {
      slot: self.name.clone(),
      node: Node {
        id: 0,
        module: self.page.clone(),
        source: self.source.clone(),
        deferred: self.loading.is_some(),
        fallback: self.loading.clone(),
        error: self.error.clone().or_else(|| error.map(str::to_owned)),
        cache_key: Some(self.page.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      },
    }
  }
}

/// Nests `content` under each layout, outermost first, each layout's
/// parallel slots beside it. Ids are assigned afterwards by `renumber`.
fn wrap_in_layouts(content: Node, wrapping: &[&LayoutInfo], error: Option<&str>) -> Node {
  let mut node = content;
  for layout in wrapping.iter().rev() {
    let mut children = vec![Child { slot: "content".to_owned(), node }];
    children.extend(layout.slots.iter().map(|s| s.child(error)));
    node = layout.node(children, Vec::new(), error);
  }
  node
}

/// The tree a soft navigation renders for a `page.<slot>.tsx`: the layouts
/// down to the one declaring `slot`, which takes `variant` there and keeps
/// its page; every other slot along the way is kept too, so only the one
/// region changes in the browser.
fn intercept_plan(variant: Node, slot: &str, wrapping: &[&LayoutInfo], error: Option<&str>) -> Node {
  let (declaring, above) = wrapping.split_last().expect("an intercept sits under the layout declaring its slot");
  let mut node = declaring.node(vec![Child { slot: slot.to_owned(), node: variant }], declaring.kept(&[slot.to_owned()]), error);
  for layout in above.iter().rev() {
    node = layout.node(vec![Child { slot: "content".to_owned(), node }], layout.kept(&["content".to_owned()]), error);
  }
  node
}

fn shell_over(options: &Options, content: Node) -> Node {
  Node {
    id: 0,
    module: options.shell.clone(),
    source: None,
    deferred: false,
    fallback: None,
    error: None,
    cache_key: None,
    children: vec![Child { slot: options.slot.clone(), node: content }],
    keep: Vec::new(),
  }
}

/// Ids in tree order from `next`, so a plan's ids are unique whatever its shape.
fn renumber(node: &mut Node, next: &mut u32) {
  node.id = *next;
  *next += 1;
  for child in &mut node.children {
    renumber(&mut child.node, next);
  }
}

/// `page.<slot>.tsx` files in a route directory: the file and the slot it names.
fn variant_files(dir: &Path) -> Result<Vec<(String, String)>, BuildError> {
  let mut out = Vec::new();
  for file in sorted_files(dir, ".tsx")? {
    let name = file.file_name().unwrap_or_default().to_string_lossy().to_string();
    let Some(middle) = name.strip_prefix("page.").and_then(|n| n.strip_suffix(".tsx")) else { continue };
    if middle.is_empty() || middle == "loader" || middle.contains('.') {
      continue;
    }
    out.push((name.clone(), middle.to_owned()));
  }
  Ok(out)
}

/// Lowers `module` into the set, recording residue as a client-only component.
fn lower_into(set: &mut ComponentSet, module: &str, report: &mut Report) -> Result<(), BuildError> {
  match set.lower(module) {
    Ok(()) => Ok(()),
    Err(LowerError::Residue(residue)) => {
      report.components.push((module.to_owned(), "client".to_owned(), format!("{}:{}: {}", residue.file, residue.line, residue.message)));
      Ok(())
    }
    Err(LowerError::Parse { file, message }) => {
      report.components.push((module.to_owned(), "client".to_owned(), format!("{file}: {message}")));
      Ok(())
    }
    Err(e) => Err(e.into()),
  }
}

/// The named slots a template places, `content` aside, in tree order.
fn slots_placed(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Slot(name) if name != "content" && !out.contains(name) => out.push(name.clone()),
      Tmpl::Slot(_) | Tmpl::Text(_) | Tmpl::Expr(_) => {}
      Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
      Tmpl::If { then, r#else, .. } => {
        walk(then, out);
        if let Some(e) = r#else {
          walk(e, out);
        }
      }
      Tmpl::For { body, .. } => walk(body, out),
      Tmpl::Let { then, .. } => walk(then, out),
    }
  }
  walk(tmpl, &mut out);
  out
}

fn sorted_dirs(dir: &Path) -> Result<Vec<PathBuf>, BuildError> {
  if !dir.is_dir() {
    return Ok(Vec::new());
  }
  let mut dirs: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_dir())
    .collect();
  dirs.sort();
  Ok(dirs)
}

fn discover(root: &Path, dir: &Path, out: &mut Vec<Route>, handlers: &mut Vec<Route>) -> Result<(), BuildError> {
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_dir())
    .collect();
  children.sort();

  let page = dir.join("page.tsx").is_file() || dir.join("page.ts").is_file();
  let handler = dir.join("route.ts").is_file();
  if page && handler {
    return Err(BuildError::PageAndRoute(dir.to_path_buf()));
  }
  if page || handler {
    let (pattern, id) = pattern_of(root, dir)?;
    let route = Route { pattern, dir: dir.to_path_buf(), id };
    if page {
      out.push(route);
    } else {
      handlers.push(route);
    }
  }
  for child in children {
    if child.file_name().is_some_and(|n| n == "slots") {
      if !dir.join("layout.tsx").is_file() {
        return Err(BuildError::SlotsWithoutLayout(child));
      }
      continue;
    }
    discover(root, &child, out, handlers)?;
  }
  Ok(())
}

/// `routes/index` is `/`; `routes/product/[id]` is `/product/{id}`;
/// `routes/docs/[...rest]` is `/docs/{*rest}`. The id is the static segments
/// joined with `.`, `index` for the root.
fn pattern_of(root: &Path, dir: &Path) -> Result<(String, String), BuildError> {
  let rel = dir.strip_prefix(root).unwrap_or(dir);
  let mut segments = Vec::new();
  let mut id_parts = Vec::new();
  for component in rel.components() {
    let name = component.as_os_str().to_string_lossy().to_string();
    if name == "index" && segments.is_empty() && id_parts.is_empty() {
      continue;
    }
    if let Some(inner) = name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
      if let Some(rest) = inner.strip_prefix("...") {
        segments.push(format!("{{*{rest}}}"));
      } else {
        segments.push(format!("{{{inner}}}"));
      }
      continue;
    }
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
      return Err(BuildError::Segment { path: dir.to_path_buf(), name });
    }
    segments.push(name.clone());
    id_parts.push(name);
  }
  let pattern = if segments.is_empty() { "/".to_owned() } else { format!("/{}", segments.join("/")) };
  let id = if id_parts.is_empty() { "index".to_owned() } else { id_parts.join(".") };
  Ok((pattern, id))
}
