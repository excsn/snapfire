//! Route discovery, the contract and the build that emits a plan file and the
//! generated TypeScript from them. The binary in `main.rs` is a thin front over
//! `build` and `write`.

pub mod infer;
pub mod types;
pub mod vendor;
pub mod xwpm;

use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use snapfire_fsr_lower::{lower_actions_with, lower_loader_with, read_schema, read_session_defaults, LowerError, SessionDefaults};
use snapfire_fsr_plan::{ActionEntry, Child, Manifest, Node, RouteEntry, RowOwner, SourceEntry};
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
}

/// What `build` found and emitted, in the order the report prints it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Report {
  pub routes: Vec<(String, String)>,
  pub sources: Vec<(String, String)>,
  pub actions: Vec<(String, String)>,
  pub services: Vec<(String, String)>,
  pub schemas: Vec<(String, String)>,
  pub types: Vec<(String, String)>,
}

impl fmt::Display for Report {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    section(f, "routes", &self.routes, "")?;
    section(f, "sources", &self.sources, "lowered")?;
    section(f, "actions", &self.actions, "lowered")?;
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
}

impl Default for Options {
  fn default() -> Self {
    Self {
      shell: "shell#document".to_owned(),
      slot: "content".to_owned(),
      mounter_module: "@snapfire/fsr-client/react".to_owned(),
      mounter: "reactMounter".to_owned(),
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
}

struct Route {
  pattern: String,
  dir: PathBuf,
  id: String,
}

/// Walks `<app>/routes`, `<app>/clients` and `<app>/schemas`, lowers every
/// `loader.ts` and `actions.ts`, builds the contract and returns the plan file
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
  discover(&routes_dir, &routes_dir, &mut routes)?;
  routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));

  let error_module = ["error.tsx", "error.ts"]
    .iter()
    .find(|f| routes_dir.join(f).is_file())
    .map(|f| format!("routes/{f}#default"));

  let mut entries = Vec::new();
  let mut sources = Vec::new();
  let mut actions = Vec::new();
  let mut islands: Vec<String> = Vec::new();
  if let Some(module) = &error_module {
    islands.push(module.clone());
  }

  for route in &routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let page = format!("{rel}/page.tsx#default");
    report.routes.push((route.pattern.clone(), rel.clone()));

    let loader = route.dir.join("loader.ts");
    let source = if loader.is_file() {
      let module = format!("{rel}/loader.ts");
      let text = std::fs::read_to_string(&loader).map_err(|e| BuildError::Io(loader.clone(), e))?;
      let body = lower_loader_with(&module, &text, &defaults)?;
      sources.push(SourceEntry::lowered(route.id.clone(), module.clone(), body));
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
      id: 1,
      module: page,
      source,
      deferred: loading.is_some(),
      fallback: loading,
      error: local_error.or_else(|| error_module.clone()),
      cache_key: None,
      children: Vec::new(),
    };
    entries.push(RouteEntry {
      pattern: route.pattern.clone(),
      plan: Node {
        id: 0,
        module: options.shell.clone(),
        source: None,
        deferred: false,
        fallback: None,
        error: None,
        cache_key: None,
        children: vec![Child { slot: options.slot.clone(), node: content }],
      },
    });
  }

  let session_type = session_import.as_ref().map(|_| "Session");
  let client = client_module(&contract, session_type, &routes, &sources, &actions);
  let manifest = Manifest::new(entries).with_sources(sources).with_actions(actions);
  debug_assert!(manifest.sources.iter().all(|s| s.owner == RowOwner::Lowered));

  let mut files = vec![(PLAN_FILE.to_owned(), manifest.to_json() + "\n")];
  files.extend(contracts.into_iter().map(|(rel, c)| (rel, c.to_json() + "\n")));
  files.extend([
    ("generated/services.d.ts".to_owned(), typescript::declarations(&contract)),
    ("generated/fsr.ts".to_owned(), ctx_module(&routes, session_import.as_deref())),
    ("generated/islands.ts".to_owned(), islands_module(&islands, options)),
    ("generated/client.ts".to_owned(), client),
    ("tsconfig.json".to_owned(), types::tsconfig(app)?),
    ("tsconfig.build.json".to_owned(), types::tsconfig_build()),
  ]);
  let mut report = report;
  report.types = types::status(app)?;
  Ok(Built { manifest, contract, report, files })
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
    "}\n\nexport interface Ctx<P extends keyof Routes = keyof Routes> {\n  params: Routes[P];\n  query: Record<string, string>;\n  session: Session;\n  identity: Identity | null;\n  services: Services;\n  now: bigint;\n}\n\nexport interface ActionCtx<Input = void, P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  input: Input;\n}\n\nexport function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>) {\n  return declare<Input, Out>(body as never);\n}\n",
  );
  out
}

/// `generated/client.ts`: the contract's types as the browser sees them, the
/// props of every page inferred from its loader's return and one typed callable
/// per action, nested by route id.
fn client_module(contract: &Contract, session: Option<&str>, routes: &[Route], sources: &[SourceEntry], actions: &[ActionEntry]) -> String {
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
  if !routes.is_empty() {
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

/// `generated/islands.ts`: one `registerIsland` per module discovery named,
/// so the browser mounts exactly what the plan file refers to.
fn islands_module(islands: &[String], options: &Options) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  let _ = writeln!(out, "import {{ registerIsland }} from \"@snapfire/fsr-client\";");
  let _ = writeln!(out, "import {{ {} }} from \"{}\";\n", options.mounter, options.mounter_module);
  out.push_str("export function registerIslands(): void {\n");
  for module in islands {
    let Some((path, _export)) = module.split_once('#') else { continue };
    let js = path.rsplit_once('.').map(|(stem, _)| format!("{stem}.js")).unwrap_or_else(|| format!("{path}.js"));
    let _ = writeln!(
      out,
      "  registerIsland(\"{module}\", {{ loader: () => import(\"../{js}\").then((m) => m.default), mount: {} }});",
      options.mounter
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

fn discover(root: &Path, dir: &Path, out: &mut Vec<Route>) -> Result<(), BuildError> {
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| BuildError::Io(dir.to_path_buf(), e))?
    .filter_map(|e| e.ok().map(|e| e.path()))
    .filter(|p| p.is_dir())
    .collect();
  children.sort();

  if dir.join("page.tsx").is_file() || dir.join("page.ts").is_file() {
    let (pattern, id) = pattern_of(root, dir)?;
    out.push(Route { pattern, dir: dir.to_path_buf(), id });
  }
  for child in children {
    discover(root, &child, out)?;
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
