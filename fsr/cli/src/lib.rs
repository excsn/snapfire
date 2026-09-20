//! Route discovery, the contract and the build that emits a plan file and the
//! generated TypeScript from them. `build` analyses, `write` puts the generated
//! files on disk and `emit` does both and then the browser bundle, which is the
//! whole artifact a host reads. The binary in `main.rs` is a thin front over them.

pub mod dev;
pub mod direction;
pub mod doctor;
pub mod new;
pub mod serve;
pub mod sites;
pub mod spec;
pub mod infer;
pub mod native;
pub mod test;
pub mod bundle;
pub mod install;
pub mod typecheck;
pub mod types;
pub mod vendor;
pub mod xwpm;

pub use dev::{emit, DevOptions, Emitted};

use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::{read_schema, read_session_defaults, LowerError, SessionDefaults, EXT_DIR};
use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::HydratedBy;
use snapfire_fsr_plan::{ActionEntry, Child, ComponentEntry, HandlerEntry, Manifest, Node, RouteEntry, RowOwner, SourceEntry};
use snapfire_fsr_service::typescript::Flavour;
use snapfire_fsr_service::{typescript, Contract, ContractError, ImportError};

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
  /// What `fsr doctor` found, when a bundle asked for it first. Carries the
  /// report so a caller prints the findings rather than a summary of them.
  #[error("{0}")]
  Doctor(crate::doctor::Report),
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
  #[error("two {kind} rows claim `{id}`: {first} and {second}")]
  ClaimedId { kind: String, id: String, first: String, second: String },
  #[error("`{module}` cannot be an island in server mode: {reason}")]
  ServerIsland { module: String, reason: String },
  #[error("`{module}` is a .{ext} component, but this fsr has no client adapter that mounts one")]
  NoAdapter { module: String, ext: String },
  #[error("`{module}` is not a component this build can mount: no framework claims its extension")]
  UnknownComponent { module: String },
  #[error("`{module}` mounts through `{adapter}`, but the import map does not name {missing}{remedy}")]
  IslandImports { module: String, adapter: String, missing: String, remedy: String },
  #[error("`{name}` is not a direction; the directions are {known}")]
  Direction { name: String, known: String },
  #[error("{map} maps `{specifier}` to `{found}`, but the host serves it at `{want}`; the adapter the build registers is the one the host serves")]
  AdapterUrl { map: String, specifier: String, found: String, want: String },
  #[error("{manifest} records {package}@{recorded}, but `{direction}` pins {package}@{wanted}; moving a vendored framework is `fsr add`, not `fsr use`")]
  DirectionPinned { direction: String, package: String, recorded: String, wanted: String, manifest: String },
  #[error("{0} already exists; an example is never written over a file")]
  ExampleExists(PathBuf),
  #[error("the import map serves `{specifier}`, but {manifest} does not say which {package} it is; `fsr add {app} {package}@{version}` records it")]
  FrameworkUnrecorded { package: String, specifier: String, manifest: String, app: String, version: String },
  #[error("react@{version} is vendored, but this fsr renders for React {supported} only")]
  ReactMajor { version: String, supported: String },
  #[error("vue@{version} is vendored, but this fsr renders for Vue {supported} only")]
  VueMajor { version: String, supported: String },
  /// A framework plugin that started and then failed to answer.
  #[error("{0}")]
  Plugin(String),
  #[error("the shell serves `{specifier}`, but {contract} does not say which {package} it is; a shell built by this fsr records it under `frameworks`. A hand-written contract needs `\"frameworks\": {{\"{package}\": \"{version}\"}}`")]
  FrameworkShellUnrecorded { package: String, specifier: String, contract: String, version: String },
  #[error("{package}@{site} is recorded in {manifest}, but {contract} serves {package}@{shell}; the browser loads the shell's copy")]
  FrameworkShellMismatch { package: String, site: String, shell: String, manifest: String, contract: String },
  #[error("{map} maps `{specifier}` to `{found}`, but {contract} serves it at `{want}`; the shell's import map overrides this one at mount, so the browser never loads `{found}`")]
  ShellUrl { map: String, specifier: String, found: String, want: String, contract: String },
  #[error("`{specifier}` is served by {contract}, which vendors {package}@{shell}; a site cannot pin {package}@{wanted}, since the shell's import map overrides its own at mount")]
  ShellPinned { specifier: String, package: String, wanted: String, shell: String, contract: String },
  #[error("elements/{file}: an element template is named after its tag, which is lowercase, starts with a letter and holds a hyphen")]
  ElementName { file: String },
  #[error("`{module}` is an element template, which is markup the server writes: {reason}")]
  ElementTemplate { module: String, reason: String },
  #[error("the contract does not hold together: {0}")]
  Contract(#[from] ContractError),
  #[error("action `{action}` names input type `{name}`, which no schema under schemas/ declares")]
  UnknownInput { action: String, name: String },
  #[error("{0}: holds both `page.tsx` and `route.ts`; a directory is a page or a handler")]
  PageAndRoute(PathBuf),
  #[error("{0}: holds `actions.ts` but no `page.tsx`; an action is named for the page beside it")]
  ActionsWithoutPage(PathBuf),
  #[error("{0}: `slots/` belongs beside a `layout.tsx`, and this directory has none")]
  SlotsWithoutLayout(PathBuf),
  #[error("{0}: a slot needs a `page.tsx`")]
  SlotWithoutPage(PathBuf),
  #[error("{0}: a slot holds one `page.tsx` and no routes beneath it")]
  SlotRoute(PathBuf),
  #[error("{dir}: holds both `{first}` and `{second}`; a directory has one page file and one layout file")]
  PageAndTemplate { dir: PathBuf, first: String, second: String },
  #[error("{0}: a template route needs an fsr built with the `tera` feature")]
  TemplateFeature(PathBuf),
  #[error("{file}:{line}: `island(` whose `module` is not a string literal; the build bundles only a module it can name")]
  TemplateIsland { file: PathBuf, line: usize },
  #[error("{0} exports `paths`, which only a page loader may: a layout or a slot has no parameter set of its own")]
  PathsOffPage(String),
  #[error("{module} exports `paths` but its route `{pattern}` has no parameter to enumerate")]
  PathsWithoutParameter { module: String, pattern: String },
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
  #[error("{map} maps `{specifier}` to `{found}`, but this application serves its vendor tree from `{base}`; write `{want}` or run `fsr add` on the package, which rewrites every vendored entry")]
  VendorUrl { map: String, specifier: String, found: String, base: String, want: String },
  #[error("{0}")]
  Xwpm(String),
  #[error("{0}")]
  Dev(String),
  #[error("{0}")]
  Types(String),
  #[error("bundle: {0}")]
  Bundle(String),
  #[error("{0}")]
  Tool(String),
  #[error("typecheck: {0}")]
  Typecheck(String),
  #[error("{0}")]
  Serve(String),
  #[error("{0}")]
  Sites(String),
}

/// A residue that took pages out of server rendering: where it sits, what it
/// says, the rewrite that does the same thing in the IR, plus every page it
/// de-lowered with the chain of placements reaching it. One cause per
/// location, however many pages import their way to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cause {
  /// `file:line:column`; the file alone when the module did not parse.
  pub at: String,
  pub message: String,
  pub hint: Option<String>,
  /// The de-lowered module and the placements from it down to `at`, empty
  /// when the residue is in the module itself.
  pub pages: Vec<(String, String)>,
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
  /// The shell contract a site was built against: its path, how many store
  /// keys and imports it names and which of the site's import map entries
  /// differ from the shell's, which the shell's win at mount.
  pub shell: Option<(String, usize, usize, Vec<String>)>,
  /// The directory pattern a layout wraps and its module.
  pub layouts: Vec<(String, String)>,
  /// A parallel slot's source id and its page module.
  pub slots: Vec<(String, String)>,
  /// `<pattern> into <slot>` and the `page.<slot>.tsx` a soft navigation renders there.
  pub intercepts: Vec<(String, String)>,
  /// Module; `lowered` or `client`; for `client`, the location of the cause,
  /// which `causes` states once however many modules point at it.
  pub components: Vec<(String, String, String)>,
  /// Why each `client` module is one, each with the pages it took down.
  pub causes: Vec<Cause>,
  /// Why each `foreign` module the plugin described stays foreign, each with
  /// the modules that mount in the browser for it.
  pub foreign: Vec<Cause>,
  /// A framework plugin the build could not start and what that leaves foreign.
  pub plugins: Vec<String>,
  /// Module, how many of its render-path calls and how many of its static subtrees the server computes for the browser.
  pub hoisted: Vec<(String, usize, usize)>,
  /// Components placed as islands in server mode and how many handlers each answers.
  pub islands: Vec<(String, usize)>,
  /// Each export under `ext/` as `file#name` and whether it is `lowered`, `native render` or `native body`.
  pub extensions: Vec<(String, String)>,
  /// Per module, a render-path call the browser still makes after hoisting, as `file:line:column`, or a handler it runs as written, as `file:line:column: reason`.
  pub browser: Vec<(String, String)>,
  pub services: Vec<(String, String)>,
  pub schemas: Vec<(String, String)>,
  pub types: Vec<(String, String)>,
  /// The plan file as written and as it would be without whitespace, in bytes.
  /// The build writes it pretty so it reads and diffs; the second number is
  /// what a deployment would ship.
  pub plan: Option<usize>,
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
    if let Some((path, keys, imports, differ)) = &self.shell {
      writeln!(f, "{:<9} {path}: {keys} store keys, {imports} imports", "shell")?;
      if !differ.is_empty() {
        writeln!(f, "{:<9} {} mapped differently here; the shell's mapping serves at mount", "", differ.join(", "))?;
      }
    }
    for (i, (module, owner, detail)) in self.components.iter().enumerate() {
      let label = if i == 0 { "rendered" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {owner:<11} {detail}")?;
    }
    for (i, cause) in self.causes.iter().enumerate() {
      let label = if i == 0 { "client" } else { "" };
      writeln!(f, "{label:<9} {:<34} {}", cause.at, cause.message)?;
      if let Some(hint) = &cause.hint {
        writeln!(f, "{:<9} {hint}", "")?;
      }
      let pages = cause.pages.len();
      writeln!(f, "{:<9} {pages} page{} render{} in the browser for it", "", if pages == 1 { "" } else { "s" }, if pages == 1 { "s" } else { "" })?;
      for (module, chain) in &cause.pages {
        writeln!(f, "{:<11} {module:<32} {chain}", "")?;
      }
    }
    for (i, cause) in self.foreign.iter().enumerate() {
      let label = if i == 0 { "foreign" } else { "" };
      writeln!(f, "{label:<9} {:<34} {}", cause.at, cause.message)?;
      if let Some(hint) = &cause.hint {
        writeln!(f, "{:<9} {hint}", "")?;
      }
      let modules = cause.pages.len();
      writeln!(f, "{:<9} {modules} component{} mount{} in the browser for it, written empty by the server", "", if modules == 1 { "" } else { "s" }, if modules == 1 { "s" } else { "" })?;
      for (module, _) in &cause.pages {
        writeln!(f, "{:<11} {module}", "")?;
      }
    }
    for (i, line) in self.plugins.iter().enumerate() {
      let label = if i == 0 { "plugins" } else { "" };
      writeln!(f, "{label:<9} {line}")?;
    }
    for (i, (module, handlers)) in self.islands.iter().enumerate() {
      let label = if i == 0 { "islands" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {:<11} {handlers} handler{}", "server", if *handlers == 1 { "" } else { "s" })?;
    }
    section(f, "extensions", &self.extensions, "")?;
    for (i, (module, site)) in self.browser.iter().enumerate() {
      let label = if i == 0 { "browser" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {site}")?;
    }
    for (i, (module, values, chunks)) in self.hoisted.iter().enumerate() {
      let label = if i == 0 { "hoisted" } else { "" };
      let mut parts = Vec::new();
      if *values > 0 {
        parts.push(format!("{values} value{}", if *values == 1 { "" } else { "s" }));
      }
      if *chunks > 0 {
        parts.push(format!("{chunks} subtree{}", if *chunks == 1 { "" } else { "s" }));
      }
      writeln!(f, "{label:<9} {module:<34} {}", parts.join(", "))?;
    }
    for (i, (service, document)) in self.services.iter().enumerate() {
      let label = if i == 0 { "services" } else { "" };
      let kind = if document.ends_with(".proto") {
        "grpc"
      } else if document.ends_with(".rs") {
        "rust"
      } else {
        "http"
      };
      writeln!(f, "{label:<9} {service:<22} {kind:<11} {document}")?;
    }
    section(f, "schemas", &self.schemas, "")?;
    section(f, "types", &self.types, "")?;
    if let Some(written) = self.plan {
      writeln!(f, "{:<9} {:<22} {:.1} KB written", "plan", PLAN_FILE, written as f64 / 1024.0)?;
    }
    Ok(())
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
/// The file under `CONTRACTS_DIR` holding every `#[service]` block's contract.
pub const RUST_CONTRACT: &str = "rust.json";
/// Where the build writes the plan file.
pub const PLAN_FILE: &str = "generated/plan.sexp";

pub struct Options {
  /// The application as a site: every id the build emits prefixed
  /// `<name>:` and every pattern under `at`. Read from `[site]` beside the app.
  pub site: Option<SiteOptions>,
  /// The module the document renders through; every route's root node.
  pub shell: String,
  /// The slot of the shell a page lands in.
  pub slot: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteOptions {
  pub name: String,
  pub at: String,
  /// The shell's `generated/shell.json` the site is built against, resolved.
  pub shell: Option<PathBuf>,
}

impl SiteOptions {
  pub fn prefix(&self) -> String {
    format!("{}:", self.name)
  }

  /// `at` joined with a path, the way the host joins a site's own routes.
  pub fn under(&self, path: &str) -> String {
    format!("{}{path}", self.at.trim_end_matches('/'))
  }
}

/// Where an application's browser bundle is served from.
pub const BUNDLE_BASE: &str = "/static/js/app";

/// Where an application's vendor tree is served from.
pub const VENDOR_BASE: &str = "/static/js/vendor";

/// Where `site`'s browser bundle is served from: under its own prefix for a
/// site, since a mount keeps only the static roots that sit under it.
pub fn bundle_base(site: Option<&SiteOptions>) -> String {
  site.map(|site| site.under(BUNDLE_BASE)).unwrap_or_else(|| BUNDLE_BASE.to_owned())
}

/// Where `site`'s vendor tree is served from, under the same rule.
pub fn vendor_base(site: Option<&SiteOptions>) -> String {
  site.map(|site| site.under(VENDOR_BASE)).unwrap_or_else(|| VENDOR_BASE.to_owned())
}

impl Options {
  /// The prefix on every emitted id: `<name>:` for a site, nothing otherwise.
  pub fn prefix(&self) -> String {
    self.site.as_ref().map(SiteOptions::prefix).unwrap_or_default()
  }

  /// `Options` with the `[site]` section of the configuration beside `app`,
  /// when there is one; the default otherwise.
  pub fn beside(app: &Path) -> Self {
    let mut options = Self::default();
    options.site = site_beside(app);
    options
  }
}

/// A service name without its site prefix: the name a test mocks it by,
/// since a site's bodies call `<name>:<service>` and its tests are written
/// against `<service>`.
pub fn unprefixed(service: &str) -> &str {
  service.rsplit_once(':').map(|(_, rest)| rest).unwrap_or(service)
}

/// The `[site]` section of the configuration beside `app`, when one names
/// this app directory.
pub fn site_beside(app: &Path) -> Option<SiteOptions> {
  let root = serve::project_root(app);
  let config = snapfire_fsr_host::config::Config::load(&root).ok()?;
  let given = app.canonicalize().ok()?;
  if config.app.canonicalize().ok()? != given {
    return None;
  }
  let root = config.root.clone();
  config.site.map(|s| SiteOptions { name: s.name, at: s.at, shell: s.shell.as_ref().map(|rel| root.join(rel)) })
}

/// The shell contract a shell's build emits as `generated/shell.json` and a
/// site's build reads: the store keys the shell's loaders seed with their
/// types as the browser sees them, the import map it serves, the exact
/// version of each framework it vendors and the fsr version that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ShellContract {
  pub version: u32,
  #[serde(default)]
  pub store: std::collections::BTreeMap<String, String>,
  #[serde(default)]
  pub imports: std::collections::BTreeMap<String, String>,
  /// The exact version of every framework package the shell vendors, by
  /// package: what a client adapter imports, so a site renders under the same
  /// React and its specs fetch matching development builds.
  #[serde(default)]
  pub frameworks: std::collections::BTreeMap<String, String>,
  #[serde(default)]
  pub fsr: String,
}

pub const SHELL_CONTRACT_VERSION: u32 = 1;

impl ShellContract {
  pub fn read(path: &Path) -> Result<Self, BuildError> {
    let text = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.to_path_buf(), e))?;
    let contract: Self = serde_json::from_str(&text).map_err(|e| BuildError::Dev(format!("{}: {e}", path.display())))?;
    if contract.version > SHELL_CONTRACT_VERSION {
      return Err(BuildError::Dev(format!("{}: shell contract version {}, this fsr reads up to {SHELL_CONTRACT_VERSION}", path.display(), contract.version)));
    }
    Ok(contract)
  }

  /// `generated/shell.d.ts` for a site: the shell's store keys as one
  /// interface and the specifiers its import map serves as one union.
  pub fn declarations(&self) -> String {
    let mut out = String::from("// Generated by fsr build from the shell contract. Do not edit.\n\n");
    out.push_str("/** The store keys the shell's loaders seed, by key, as the browser reads them. */\nexport interface ShellStore {\n");
    for (key, ty) in &self.store {
      let _ = writeln!(out, "  \"{key}\": {ty};");
    }
    out.push_str("}\n\n/** The bare specifiers the shell's import map serves; a site imports these from the shell, never its own copy. */\nexport type ShellImport =");
    if self.imports.is_empty() {
      out.push_str(" never;\n");
    } else {
      for (i, specifier) in self.imports.keys().enumerate() {
        let _ = write!(out, "{} \"{specifier}\"", if i == 0 { "" } else { " |" });
      }
      out.push_str(";\n");
    }
    out
  }
}

impl Default for Options {
  fn default() -> Self {
    Self {
      site: None,
      shell: "shell#document".to_owned(),
      slot: "content".to_owned(),
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
  /// The route modules the browser mounts, as files, which is all of `routes/` a bundle compiles.
  pub browser_routes: Vec<String>,
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

  // The application's own Rust sits beside `app/`, so the crate's `src/` is
  // the sibling. Read rather than compiled, which is what lets `build.rs` run
  // this before the crate exists.
  let natives = match app.parent() {
    Some(project) => native::read(&project.join("src"))?,
    None => native::Natives::default(),
  };
  let mut rust = Contract::new();
  for service in &natives.services {
    report.services.push((service.name.clone(), service.file.clone()));
    contract.adopt(service.contract.clone(), &service.file)?;
    rust.adopt(service.contract.clone(), &service.file)?;
  }
  if !rust.services.is_empty() {
    contracts.push((format!("{CONTRACTS_DIR}/{RUST_CONTRACT}"), rust));
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

  let elements = element_templates(app)?;
  let mut set = ComponentSet::new(app).with_defaults(defaults.clone()).provide(snapfire_fsr_lower::HEAD_MODULE, HEAD_HELPERS).with_elements(elements.iter().cloned().collect());
  describe_foreign(app, &mut set, &mut report)?;
  for file in sorted_files(&app.join(EXT_DIR), ".ts")? {
    let rel = format!("{EXT_DIR}/{}", file.file_name().unwrap_or_default().to_string_lossy());
    report.extensions.extend(set.lower_extensions(&rel)?);
  }
  for (_, module) in &elements {
    set.lower(module)?;
    let refused = |reason: String| BuildError::ElementTemplate { module: module.clone(), reason };
    let Some((_, component)) = set.components.iter_mut().find(|(m, _)| m == module) else { continue };
    if component.hydrated_by.is_some() {
      return Err(refused("it holds state or a handler, which belong in the element's class".to_owned()));
    }
    component.shadow = snapfire_fsr_ir::ShadowRoot::take(&mut component.render).map_err(|e| refused(e.to_string()))?;
  }

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
  let mut templates: Vec<PathBuf> = Vec::new();
  for module in [&error_module, &not_found_module].into_iter().flatten() {
    islands.push(module.clone());
  }

  let mut layouts: Vec<LayoutInfo> = Vec::new();
  let mut layout_ids: Vec<String> = Vec::new();
  for dir in routes.iter().chain(handler_routes.iter()).flat_map(|r| r.dir.ancestors().map(Path::to_path_buf).collect::<Vec<_>>()) {
    if !dir.starts_with(&routes_dir) || layouts.iter().any(|l| l.dir == dir) {
      continue;
    }
    let Some(layout_file) = layout_file(&dir)? else { continue };
    let rel = dir.strip_prefix(app).unwrap_or(&dir).to_string_lossy().replace('\\', "/");
    let (prefix, dir_id) = pattern_of(&routes_dir, &dir)?;
    let id = if dir == routes_dir { "layout".to_owned() } else { format!("{dir_id}.layout") };
    let module = format!("{rel}/{layout_file}#default");
    let loader = dir.join("layout.loader.ts");
    let source = if loader.is_file() {
      let loader_module = format!("{rel}/layout.loader.ts");
      let body = set.lower_loader(&loader_module)?;
      let meta = set.lower_meta(&loader_module)?;
      let store = set.lower_store(&loader_module)?;
      if set.lower_paths(&loader_module)?.is_some() {
        return Err(BuildError::PathsOffPage(loader_module));
      }
      sources.push(SourceEntry::lowered(id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store));
      report.sources.push((id.clone(), loader_module));
      Some(id.clone())
    } else {
      None
    };
    report.layouts.push((prefix, module.clone()));
    if is_template(&module) {
      report.components.push((module.clone(), "template".to_owned(), String::new()));
      templates.push(dir.join(&layout_file));
    } else {
      islands.push(module.clone());
    }
    layout_ids.push(id.clone());
    let mut slots = Vec::new();
    for slot_dir in sorted_dirs(&dir.join("slots"))? {
      let name = slot_dir.file_name().unwrap_or_default().to_string_lossy().to_string();
      let Some(slot_page) = page_file(&slot_dir)? else {
        return Err(BuildError::SlotWithoutPage(slot_dir));
      };
      for d in sorted_dirs(&slot_dir)? {
        if page_file(&d)?.is_some() || d.join("route.ts").is_file() {
          return Err(BuildError::SlotRoute(slot_dir));
        }
      }
      let slot_rel = format!("{rel}/slots/{name}");
      let slot_id = format!("{id}.{name}");
      let page = format!("{slot_rel}/{slot_page}#default");
      let loader = slot_dir.join("page.loader.ts");
      let source = if loader.is_file() {
        let loader_module = format!("{slot_rel}/page.loader.ts");
        let body = set.lower_loader(&loader_module)?;
        let meta = set.lower_meta(&loader_module)?;
        let store = set.lower_store(&loader_module)?;
        if set.lower_paths(&loader_module)?.is_some() {
          return Err(BuildError::PathsOffPage(loader_module));
        }
        sources.push(SourceEntry::lowered(slot_id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store));
        report.sources.push((slot_id.clone(), loader_module));
        Some(slot_id.clone())
      } else {
        None
      };
      let slot_actions = slot_dir.join("actions.ts");
      if slot_actions.is_file() {
        let module = format!("{slot_rel}/actions.ts");
        for lowered in set.lower_actions(&module)? {
          let action_id = format!("{slot_id}.{}", lowered.export);
          if let Some(name) = &lowered.input {
            if !contract.types.contains_key(name) {
              return Err(BuildError::UnknownInput { action: action_id, name: name.clone() });
            }
          }
          let mut entry = ActionEntry::lowered(action_id.clone(), module.clone(), lowered.body);
          entry.export = Some(lowered.export);
          entry.input = lowered.input;
          actions.push(entry);
          report.actions.push((action_id, module.clone()));
        }
      }

      let loading = ["loading.tsx", "loading.ts"].iter().find(|f| slot_dir.join(f).is_file()).map(|f| format!("{slot_rel}/{f}#default"));
      let error = ["error.tsx", "error.ts"].iter().find(|f| slot_dir.join(f).is_file()).map(|f| format!("{slot_rel}/{f}#default"));
      for module in [Some(&page), loading.as_ref(), error.as_ref()].into_iter().flatten() {
        if is_template(module) {
          report.components.push((module.clone(), "template".to_owned(), String::new()));
          templates.push(slot_dir.join(&slot_page));
        } else {
          islands.push(module.clone());
        }
      }
      report.slots.push((slot_id.clone(), page.clone()));
      layout_ids.push(slot_id);
      slots.push(SlotInfo { name, page, source, loading, error });
    }
    layouts.push(LayoutInfo { dir, module, source, slots, placed: Vec::new() });
  }
  layouts.sort_by(|a, b| a.dir.cmp(&b.dir));

  set.layouts = layouts.iter().filter(|l| !is_template(&l.module)).map(|l| l.module.clone()).collect();
  set.slots = layouts.iter().filter(|l| !is_template(&l.module)).map(|l| (l.module.clone(), l.slots.iter().map(|s| s.name.clone()).collect())).collect();
  for layout in &mut layouts {
    if is_template(&layout.module) {
      continue;
    }
    lower_into(&mut set, &layout.module, &mut report)?;
    if let Some((_, component)) = set.components.iter().find(|(m, _)| *m == layout.module) {
      layout.placed = slots_placed(&component.render);
    }
  }
  let mut intercepts = Vec::new();

  for route in &routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let page_file = page_file(&route.dir)?.expect("a discovered route has a page file");
    let page = format!("{rel}/{page_file}#default");
    report.routes.push((route.pattern.clone(), rel.clone()));

    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| route.dir.starts_with(&l.dir)).collect();
    let loader = route.dir.join("page.loader.ts");
    let source = if loader.is_file() {
      let module = format!("{rel}/page.loader.ts");
      let body = set.lower_loader(&module)?;
      let meta = set.lower_meta(&module)?;
      let store = set.lower_store(&module)?;
      let paths = set.lower_paths(&module)?;
      if paths.is_some() && !route.pattern.contains('{') {
        return Err(BuildError::PathsWithoutParameter { module, pattern: route.pattern.clone() });
      }
      sources.push(SourceEntry::lowered(route.id.clone(), module.clone(), body).with_meta(meta).with_store(store).with_paths(paths));
      report.sources.push((route.id.clone(), module));
      Some(route.id.clone())
    } else {
      None
    };

    let actions_file = route.dir.join("actions.ts");
    if actions_file.is_file() {
      let module = format!("{rel}/actions.ts");
      for lowered in set.lower_actions(&module)? {
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
      if is_template(module) {
        report.components.push((module.clone(), "template".to_owned(), String::new()));
        templates.push(route.dir.join(&page_file));
      } else if !islands.contains(module) {
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
    let module = format!("{rel}/route.ts");
    for lowered in set.lower_handlers(&module)? {
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
    report.middleware = Some("middleware.ts".to_owned());
    Some(set.lower_middleware("middleware.ts")?)
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

  for file in &templates {
    for placed in template_islands(file)? {
      if !islands.contains(&placed) {
        islands.push(placed);
      }
    }
  }
  for module in &islands {
    lower_into(&mut set, module, &mut report)?;
  }
  for (module, component) in &set.components {
    for placed in server_islands(&component.render) {
      let Some((_, inner)) = set.components.iter().find(|(m, _)| *m == placed) else { continue };
      if let Some(reason) = unlowered_handler(&inner.render) {
        return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but a handler did not lower: {reason}") });
      }
      if let Some((name, index)) = captured_by_handler(inner) {
        let reason = format!(
          "{module} places it there, but handler {index} reads `{name}`, which the markup around it bound: a handler runs with the props, the state and the event, so put the value on the element and read it from `e.target`"
        );
        return Err(BuildError::ServerIsland { module: placed.clone(), reason });
      }
      let mut nested = nested_components(&inner.render);
      let mut i = 0;
      while i < nested.len() {
        if let Some((_, component)) = set.components.iter().find(|(m, _)| *m == nested[i]) {
          for below in nested_components(&component.render) {
            if !nested.contains(&below) {
              nested.push(below);
            }
          }
        }
        i += 1;
      }
      let mut handlers = inner.handlers.len();
      for within in &nested {
        let Some((_, component)) = set.components.iter().find(|(m, _)| m == within) else { continue };
        if let Some(reason) = unlowered_handler(&component.render) {
          return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but a handler of `{within}` inside it did not lower: {reason}") });
        }
        if let Some((name, index)) = captured_by_handler(component) {
          let reason = format!(
            "{module} places it there, but handler {index} of `{within}` inside it reads `{name}`, which the markup around it bound: a handler runs with the props, the state and the event, so put the value on the element and read it from `e.target`"
          );
          return Err(BuildError::ServerIsland { module: placed.clone(), reason });
        }
        handlers += component.handlers.len();
      }
      if let Some(slot) = first_slot(&inner.render) {
        let what = if slot == "content" { "`children`".to_owned() } else { format!("the slot `{slot}`") };
        return Err(BuildError::ServerIsland { module: placed.clone(), reason: format!("{module} places it there, but it renders {what}, which a step would drop: a server island's markup is its own component's and nothing else fills a part of it") });
      }
      let row = (format!("{}{placed}", options.prefix()), handlers);
      if !report.islands.contains(&row) {
        report.islands.push(row);
      }
    }
  }
  report.islands.sort();
  for rewrite in &mut set.rewrites {
    rewrite.module = format!("{}{}", options.prefix(), rewrite.module);
    report.hoisted.push((rewrite.module.clone(), rewrite.sites.len(), rewrite.chunks.len()));
  }
  let rewritten = set.rewritten();
  report.hoisted.sort();
  report.browser = set.remaining.iter().map(|(module, site)| (format!("{}{module}", options.prefix()), site.clone())).collect();
  for (module, component) in &set.components {
    let file = module.split('#').next().unwrap_or(module);
    for why in unlowered_handlers(&component.render) {
      report.browser.push((format!("{}{module}", options.prefix()), format!("{file}:{why}")));
    }
  }
  report.browser.sort();
  let mut components = Vec::new();
  let mut islands = islands;
  // Modules an `<Island define>` names: the browser imports one at its timing and the element upgrades itself.
  let mut defines: Vec<String> = Vec::new();
  // A template nothing mounts has no browser twin: it is left out of the
  // island registry and out of the bundle, so a page of such templates loads
  // no framework at all.
  let mut static_modules: Vec<String> = Vec::new();
  // Layouts declared `tree(Layout)`: the React adapter mounts each as one root with its page.
  let mut trees: Vec<String> = Vec::new();
  for (module, residue) in std::mem::take(&mut set.foreign_residue) {
    let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
    report.components.push((module.clone(), "foreign".to_owned(), at.clone()));
    match report.foreign.iter_mut().find(|c| c.at == at && c.message == residue.message) {
      Some(cause) => cause.pages.push((module, String::new())),
      None => report.foreign.push(Cause { at, message: residue.message, hint: residue.hint, pages: vec![(module, String::new())] }),
    }
  }
  for (module, component) in std::mem::take(&mut set.components) {
    let detail = match component.hydrated_by {
      Some(HydratedBy::ReactTree) => HydratedBy::TREE.to_owned(),
      Some(HydratedBy::Vue) => HydratedBy::VUE.to_owned(),
      Some(HydratedBy::React) => String::new(),
      None => "static".to_owned(),
    };
    report.components.push((module.clone(), "lowered".to_owned(), detail));
    if component.hydrated_by.is_none() {
      static_modules.push(module.clone());
    }
    if component.hydrated_by == Some(HydratedBy::ReactTree) {
      trees.push(module.clone());
    }
    for (placed, define) in island_modules(&component.render) {
      if define && !defines.contains(&placed) {
        defines.push(placed.clone());
      }
      if !islands.contains(&placed) {
        islands.push(placed);
      }
    }
    components.push(ComponentEntry { module, body: component });
  }
  // A component a page places as an island is mounted because the page asked
  // for it, whatever its own markup would need.
  let placed: Vec<String> = components.iter().flat_map(|entry| island_modules(&entry.body.render)).map(|(module, _)| module).collect();
  static_modules.retain(|module| !placed.contains(module));
  for (module, _, detail) in &mut report.components {
    if placed.contains(module) && detail == "static" {
      detail.clear();
    }
  }
  report.components.sort();
  let layout = crate::xwpm::Layout::of_site(app, options.site.as_ref())?;
  let shim = types::foreign_shim(app, &set.foreign);
  let mut browser_route_files: Vec<String> = report
    .components
    .iter()
    .filter(|(module, owner, detail)| module.starts_with("routes/") && (owner == "client" || detail != "static"))
    .filter_map(|(module, _, _)| module.split_once('#').map(|(file, _)| file.to_owned()))
    .collect();
  browser_route_files.sort();
  browser_route_files.dedup();
  report.causes.sort_by(|a, b| (&a.at, &a.message).cmp(&(&b.at, &b.message)));
  for cause in &mut report.causes {
    cause.pages.sort();
  }

  let session_type = session_import.as_ref().map(|_| "Session");
  let prefix = options.prefix();
  let config = public_of(app);
  let client = client_module(&contract, session_type, &routes, &layout_ids, &sources, &actions, &prefix, &set.consts, &config);
  let shell = match options.site.as_ref().and_then(|site| site.shell.as_ref()) {
    Some(path) => Some((path, ShellContract::read(path)?)),
    None => None,
  };
  check_vendor_urls(app, &layout)?;
  check_shell_urls(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  let frameworks = vendored_frameworks(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  let clients: Vec<snapfire_fsr_plan::ClientEntry> = report
    .components
    .iter()
    .filter(|(_, owner, _)| owner == "client")
    .map(|(module, _, at)| {
      let cause = report.causes.iter().find(|c| &c.at == at && c.pages.iter().any(|(page, _)| page == module));
      snapfire_fsr_plan::ClientEntry {
        module: module.clone(),
        at: at.clone(),
        message: cause.map(|c| c.message.clone()).unwrap_or_default(),
        hint: cause.and_then(|c| c.hint.clone()),
      }
    })
    .collect();
  let manifest = Manifest::new(entries).with_sources(sources).with_actions(actions).with_components(components).with_clients(clients).with_not_found(not_found).with_handlers(handlers).with_middleware(middleware).with_intercepts(intercepts).with_consts(set.consts.clone()).with_frameworks(frameworks.clone());
  debug_assert!(manifest.sources.iter().all(|s| s.owner == RowOwner::Lowered));
  let declarations = typescript::declarations(&contract);
  let (manifest, contract, contracts) = match &options.site {
    Some(site) => (
      manifest.namespaced(&site.name, &site.at, &options.shell),
      contract.namespaced(&site.name),
      contracts.into_iter().map(|(rel, c)| (rel, c.namespaced(&site.name))).collect(),
    ),
    None => (manifest, contract, contracts),
  };

  let plan_text = manifest.to_sexpr();
  report.plan = Some(plan_text.len());
  let mut files = vec![(PLAN_FILE.to_owned(), plan_text)];
  files.extend(contracts.into_iter().map(|(rel, c)| (rel, c.to_json() + "\n")));
  files.extend(rewritten.into_iter().map(|(file, source)| (format!("{}/{file}", dev::BUNDLE_OVERLAY), source)));
  match &options.site {
    None => {
      let shell_contract = shell_contract(app, &contract, session_type, &manifest.sources, &set.consts, &config, &frameworks)?;
      files.push(("generated/shell.json".to_owned(), serde_json::to_string_pretty(&shell_contract).expect("a shell contract serializes") + "\n"));
    }
    Some(_) => {
      if let Some((path, shell_contract)) = &shell {
        let own: std::collections::BTreeMap<String, String> = std::fs::read_to_string(app.join("importmap.json"))
          .ok()
          .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
          .and_then(|json| json.get("imports").cloned())
          .and_then(|imports| serde_json::from_value(imports).ok())
          .unwrap_or_default();
        let differ: Vec<String> = own.iter().filter(|(k, v)| shell_contract.imports.get(*k).is_some_and(|theirs| theirs != *v)).map(|(k, _)| k.clone()).collect();
        report.shell = Some((path.display().to_string(), shell_contract.store.len(), shell_contract.imports.len(), differ));
        files.push(("generated/shell.d.ts".to_owned(), shell_contract.declarations()));
      }
    }
  }
  let registry = islands_module(&islands, &static_modules, &defines, &trees, options)?;
  check_island_imports(app, &layout, shell.as_ref().map(|(_, contract)| contract), &islands, &static_modules, &defines, &trees, &set)?;
  files.extend([
    ("generated/native.d.ts".to_owned(), native::declarations(&natives)),
    ("generated/services.d.ts".to_owned(), declarations),
    ("generated/elements.d.ts".to_owned(), types::element_declarations(app, &layout, &elements)?),
    ("generated/head.ts".to_owned(), HEAD_HELPERS.to_owned()),
    ("generated/fsr.ts".to_owned(), ctx_module(&routes.iter().chain(handler_routes.iter()).cloned().collect::<Vec<_>>(), session_import.as_deref(), &config)),
    ("generated/islands.ts".to_owned(), registry),
    ("generated/client.ts".to_owned(), client),
    ("generated/testing.ts".to_owned(), testing_module()),
    ("tsconfig.json".to_owned(), types::tsconfig(app, true, shim.is_some())?),
    ("tsconfig.build.json".to_owned(), types::tsconfig_build(app, &browser_route_files)),
  ]);
  if let Some(shim) = shim {
    files.push((format!("{}/{}", layout.types.trim_end_matches('/'), types::FOREIGN_SHIM), shim));
  }
  let mut report = report;
  report.types = types::status(app)?;
  claimed(&report, &routes, &handler_routes, &layout_ids)?;
  Ok(Built { manifest, contract, report, files, defaults, browser_routes: browser_route_files })
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
  clear_overlay(app)?;
  // Where the shim lived before it moved under `<types>/`; two copies would
  // redeclare the module.
  let moved = app.join("generated/foreign.d.ts");
  if moved.is_file() {
    std::fs::remove_file(&moved).map_err(|e| BuildError::Io(moved, e))?;
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
  written.extend(types::refresh_embedded(app)?);
  Ok(written)
}

/// Writes the `generated/` files of `built` and nothing else: what the browser
/// half of a test compiles against, so a run sees the build it was given
/// rather than whatever the last `fsr build` left on disk.
pub fn write_generated(app: &Path, built: &Built) -> Result<(), BuildError> {
  for (rel, content) in &built.files {
    if !rel.starts_with("generated/") {
      continue;
    }
    let path = app.join(rel);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
  }
  Ok(())
}

/// Removes the bundle overlay, so a file no longer rewritten does not shadow its source.
fn clear_overlay(app: &Path) -> Result<(), BuildError> {
  let overlay = app.join(dev::BUNDLE_OVERLAY);
  if overlay.is_dir() {
    std::fs::remove_dir_all(&overlay).map_err(|e| BuildError::Io(overlay, e))?;
  }
  Ok(())
}

/// Writes only the bundle overlay of `built`: the sources the build rewrote
/// for the browser, under `.fsr-bundle`. `write` includes them; this is for a
/// caller that compiles without writing the rest.
pub fn write_overlay(app: &Path, built: &Built) -> Result<(), BuildError> {
  clear_overlay(app)?;
  for (rel, content) in &built.files {
    if !rel.starts_with(dev::BUNDLE_OVERLAY) {
      continue;
    }
    let path = app.join(rel);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
  }
  Ok(())
}

/// The shell contract of this build: every store key a loader's `store`
/// export seeds, typed the way the browser reads it, plus the import map and
/// the frameworks this build vendors.
fn shell_contract(app: &Path, contract: &Contract, session: Option<&str>, sources: &[SourceEntry], consts: &Consts, config: &[(String, infer::Ts)], frameworks: &std::collections::BTreeMap<String, String>) -> Result<ShellContract, BuildError> {
  let mut store = std::collections::BTreeMap::new();
  for source in sources {
    let (Some(loader), Some(body)) = (&source.body, &source.store) else { continue };
    let data = infer::Inferer { contract, session, input: None, input_type: None, consts, config }.returns(loader);
    let inferred = infer::Inferer { contract, session, input: None, input_type: Some(data), consts, config }.returns(body);
    if let infer::Ts::Record(fields) = inferred {
      for (key, ty) in fields {
        store.insert(key, ty.print(Flavour::Client));
      }
    }
  }
  let imports = std::fs::read_to_string(app.join("importmap.json"))
    .ok()
    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    .and_then(|json| json.get("imports").cloned())
    .and_then(|imports| serde_json::from_value(imports).ok())
    .unwrap_or_default();
  Ok(ShellContract { version: SHELL_CONTRACT_VERSION, store, imports, frameworks: frameworks.clone(), fsr: env!("CARGO_PKG_VERSION").to_owned() })
}

/// The head helpers a `meta` body imports from `@snapfire/fsr`. Plain
/// functions, so the lowerer inlines each call the way it inlines any
/// module-level helper.
const HEAD_HELPERS: &str = r##"// Generated by fsr build. Do not edit.

/** One element a `meta` body puts in the document head. */
export interface HeadEl {
  tag: string;
  children?: string;
  [attribute: string]: string | undefined;
}

/** One `<meta name>` element. */
export function metaTag(name: string, content: string): HeadEl {
  return { tag: "meta", name, content };
}

/** One `<link>` element. */
export function linkTag(rel: string, href: string): HeadEl {
  return { tag: "link", rel, href };
}

/** An Open Graph property: `og("title", t)` is `<meta property="og:title">`. */
export function og(property: string, content: string): HeadEl {
  return { tag: "meta", property: `og:${property}`, content };
}

/** A Twitter card property: `twitter("card", c)` is `<meta name="twitter:card">`. */
export function twitter(name: string, content: string): HeadEl {
  return { tag: "meta", name: `twitter:${name}`, content };
}

/** The canonical URL of this page. */
export function canonical(href: string): HeadEl {
  return { tag: "link", rel: "canonical", href };
}

/** What crawlers may do: "noindex, follow" and its kin. */
export function robots(content: string): HeadEl {
  return { tag: "meta", name: "robots", content };
}

/** The browser chrome colour for this page. */
export function themeColor(content: string): HeadEl {
  return { tag: "meta", name: "theme-color", content };
}

/** One icon. Two icons differing only in `sizes` are separate elements, so a page may override one of them. */
export function icon(href: string, sizes: string): HeadEl {
  return { tag: "link", rel: "icon", href, sizes };
}

/** This page in another locale, for search engines. */
export function alternate(href: string, hreflang: string): HeadEl {
  return { tag: "link", rel: "alternate", href, hreflang };
}

/** Structured data, already serialised. `JSON.stringify` is not lowerable, so build the string in the body. */
export function jsonLd(json: string): HeadEl {
  return { tag: "script", type: "application/ld+json", children: json };
}
"##;

/// `[public]` from the configuration beside `app`, each value typed the way
/// a body reads it. A project with no configuration or one the CLI cannot
/// load, declares nothing and `ctx.config` is `{}`.
fn public_of(app: &Path) -> Vec<(String, infer::Ts)> {
  use snapfire_fsr_host::config::PublicValue;
  let root = serve::project_root(app);
  let Ok(config) = snapfire_fsr_host::config::Config::load(&root) else { return Vec::new() };
  config
    .public
    .iter()
    .map(|(key, value)| {
      let ts = match value {
        PublicValue::Str(_) => infer::Ts::Str,
        PublicValue::Int(_) => infer::Ts::Big,
        PublicValue::Float(_) => infer::Ts::Num,
        PublicValue::Bool(_) => infer::Ts::Bool,
      };
      (key.clone(), ts)
    })
    .collect()
}

/// `generated/fsr.ts`: the per-route params, `Ctx`, `ActionCtx` and the typed
/// `action` and `fail` a body imports.
fn ctx_module(routes: &[Route], session_import: Option<&str>, config: &[(String, infer::Ts)]) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import { action as declare, fail } from \"@snapfire/fsr-authoring\";\nimport type { Identity } from \"@snapfire/fsr-authoring\";\nimport type { Services } from \"./services\";\nimport type { Natives } from \"./native\";\nimport type { HeadEl } from \"./head\";\n");
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
  out.push_str("}\n\n/** `[public]` from the configuration: the deployment's own values, the same on every request. */\nexport interface Config {\n");
  for (key, ts) in config {
    let _ = writeln!(out, "  {key}: {};", ts.print(Flavour::Server));
  }
  out.push_str(
    "}\n\nexport interface Address {\n  path: string;\n  params: Record<string, string>;\n  query: Record<string, string>;\n}\n\nexport interface Ctx<P extends keyof Routes = keyof Routes> {\n  params: Routes[P];\n  query: Record<string, string>;\n  /** The path this request matched, query excluded, locale prefix included. Empty under an action, whose path is the action endpoint rather than the document's. */\n  path: string;\n  session: Session;\n  identity: Identity | null;\n  locale: string;\n  /** The host the request named, when `[server] hosts` lists it; null when it does not, null whenever that key is unset. */\n  host: string | null;\n  /** The navigation's own request when this render is an intercept, null otherwise. A layout the document keeps loads under the document's request, so `params`, `query` and `path` are the document's there and this is the navigation's. */\n  address: Address | null;\n  /** `[public]` from the configuration, one field per key, typed from the value written there. */\n  config: Config;\n  services: Services;\n  /** The application's own Rust, in this process. A method the build read as `fn` answers a value; an `async fn` answers a promise. */\n  native: Natives;\n  now: bigint;\n}\n\n/** What an action, a route handler or middleware can do to the session beyond reading and writing its keys. */\nexport interface SessionControl {\n  /** Moves the session's end to `seconds` from now once the body commits; the cookie and the store follow. Nothing extends on its own. */\n  extend(seconds: number): void;\n}\n\nexport interface ActionCtx<Input = void, P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  input: Input;\n  session: Session & SessionControl;\n}\n\n/** A `route.ts` handler's context: a loader's plus `session.extend`. */\nexport interface HandlerCtx<P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  session: Session & SessionControl;\n}\n\nexport interface RequestLine {\n  method: string;\n  path: string;\n  /** Whether the request is a navigation's payload rather than a document; absent under a body test. */\n  payload?: boolean;\n}\n\nexport interface MiddlewareCtx extends Ctx {\n  request: RequestLine;\n  session: Session & SessionControl;\n}\n\nexport interface Meta {\n  title?: string;\n  description?: string;\n  head?: HeadEl[];\n}\n\nexport interface MetaCtx<Data> {\n  data: Data;\n}\n\nexport type DataOf<Load> = Load extends (...args: never[]) => Promise<infer Data> ? Data : never;\n\nexport interface MiddlewareResult {\n  redirect?: string;\n  rewrite?: string;\n  status?: number;\n  body?: unknown;\n  headers?: Record<string, string>;\n}\n\nexport function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): (ctx: ActionCtx<Input>) => Promise<Out> {\n  return declare<Input, Out>(body as never) as never;\n}\n",
  );
  out.push_str("\nexport type { HeadEl } from \"./head\";\n");
  out
}

/// What a body test declares its tests, hooks, mock functions and expectations with.
const TESTING_DECLARATIONS: &str = r#"/** A test's or a hook's body. */
export type Body = () => Promise<void> | void;

type Named = (name: string, body?: Body, timeout?: number) => void;

type Rows = {
  (table: readonly unknown[]): (name: string, body: (...args: any[]) => Promise<void> | void) => void;
  (strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => Promise<void> | void) => void;
};

export type TestApi = Named & { only: Named & { each: Rows }; skip: Named & { each: Rows }; todo(name: string): void; each: Rows; concurrent: Named };

type Group = (name: string, body: () => void) => void;

type GroupRows = {
  (table: readonly unknown[]): (name: string, body: (...args: any[]) => void) => void;
  (strings: TemplateStringsArray, ...values: unknown[]): (name: string, body: (row: any) => void) => void;
};

export type DescribeApi = Group & { only: Group & { each: GroupRows }; skip: Group & { each: GroupRows }; each: GroupRows };

export const test: TestApi = lowered as unknown as TestApi;
export const it: TestApi = test;
export const xit: Named = test;
export const xtest: Named = test;
export const fit: Named = test;
export const describe: DescribeApi = lowered as unknown as DescribeApi;
export const xdescribe: Group = describe;
export const fdescribe: Group = describe;

export function beforeAll(body: Body): void {
  void body;
  lowered();
}

export function afterAll(body: Body): void {
  void body;
  lowered();
}

export function beforeEach(body: Body): void {
  void body;
  lowered();
}

export function afterEach(body: Body): void {
  void body;
  lowered();
}

/** A function that records its calls and answers what it is told, for a ctx's services to name. */
export interface MockFunction<A extends unknown[] = any[], R = any> {
  (...args: A): R;
  mockReturnValue(value: unknown): this;
  mockReturnValueOnce(value: unknown): this;
  mockResolvedValue(value: unknown): this;
  mockResolvedValueOnce(value: unknown): this;
  mockRejectedValue(error: unknown): this;
  mockRejectedValueOnce(error: unknown): this;
  mockImplementation(impl: (...args: A) => R): this;
  mockImplementationOnce(impl: (...args: A) => R): this;
  mockClear(): this;
  mockReset(): this;
}

export function fn<A extends unknown[] = any[], R = any>(impl?: (...args: A) => R): MockFunction<A, R> {
  void impl;
  return lowered();
}

export const vi = { fn, clearAllMocks: (): void => lowered(), resetAllMocks: (): void => lowered(), restoreAllMocks: (): void => lowered() };
export const jest = vi;

export interface Matchers<R> {
  toBe(expected: unknown): R;
  toEqual(expected: unknown): R;
  toStrictEqual(expected: unknown): R;
  toBeTruthy(): R;
  toBeFalsy(): R;
  toBeNull(): R;
  toBeUndefined(): R;
  toBeDefined(): R;
  toBeNaN(): R;
  toBeGreaterThan(n: number | bigint): R;
  toBeGreaterThanOrEqual(n: number | bigint): R;
  toBeLessThan(n: number | bigint): R;
  toBeLessThanOrEqual(n: number | bigint): R;
  toBeCloseTo(n: number, digits?: number): R;
  toContain(item: unknown): R;
  toContainEqual(item: unknown): R;
  toHaveLength(length: number): R;
  toHaveProperty(path: string | (string | number)[], value?: unknown): R;
  toMatch(pattern: string | RegExp): R;
  toMatchObject(expected: object): R;
  toBeTypeOf(type: "string" | "number" | "bigint" | "boolean" | "object"): R;
  toBeOneOf(options: readonly unknown[]): R;
  toThrow(expected?: string | RegExp): R;
  toThrowError(expected?: string | RegExp): R;
  toHaveBeenCalled(): R;
  toHaveBeenCalledOnce(): R;
  toHaveBeenCalledTimes(times: number): R;
  toHaveBeenCalledWith(...args: unknown[]): R;
  toHaveBeenCalledExactlyOnceWith(...args: unknown[]): R;
  toHaveBeenLastCalledWith(...args: unknown[]): R;
  toHaveBeenNthCalledWith(n: number, ...args: unknown[]): R;
  toHaveReturned(): R;
  toHaveReturnedTimes(times: number): R;
  toHaveReturnedWith(value: unknown): R;
  toHaveLastReturnedWith(value: unknown): R;
  toBeCalled(): R;
  toBeCalledTimes(times: number): R;
  toBeCalledWith(...args: unknown[]): R;
  lastCalledWith(...args: unknown[]): R;
  nthCalledWith(n: number, ...args: unknown[]): R;
  toReturn(): R;
  toReturnTimes(times: number): R;
  toReturnWith(value: unknown): R;
  lastReturnedWith(value: unknown): R;
}

export interface Assertion extends Matchers<void> {
  not: Matchers<void>;
  resolves: Matchers<Promise<void>> & { not: Matchers<Promise<void>> };
  rejects: Matchers<Promise<void>> & { not: Matchers<Promise<void>> };
}

export interface Expect {
  /** `message` leads the report when the expectation fails. */
  (value: unknown, message?: string): Assertion;
  any(type: unknown): any;
  anything(): any;
  objectContaining(value: object): any;
  arrayContaining(value: readonly unknown[]): any;
  stringContaining(value: string): any;
  stringMatching(value: string | RegExp): any;
  closeTo(value: number, digits?: number): any;
  not: {
    objectContaining(value: object): any;
    arrayContaining(value: readonly unknown[]): any;
    stringContaining(value: string): any;
    stringMatching(value: string | RegExp): any;
  };
}

export const expect: Expect = lowered as unknown as Expect;

"#;

/// `generated/testing.ts`: what a `*.test.ts` imports from `@snapfire/fsr/testing`.
/// The bodies throw because `fsr test` lowers the file rather than running it;
/// the types are the point.
fn testing_module() -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import type { ActionCtx, Config, RequestLine, Routes, Services, Session } from \"./fsr\";\n\n");
  out.push_str("/** A mock's answer with every integer field also taking a number, since the harness reads the contract and takes a whole number as the integer it names. */\ntype Loose<T> = T extends bigint ? bigint | number : T extends (infer U)[] ? Loose<U>[] : T extends object ? { [K in keyof T]: Loose<T[K]> } : T;\n\ntype Mocked<S> = { [K in keyof S]?: { [M in keyof S[K]]?: S[K][M] extends (args: infer A) => Promise<infer R> ? ((args: A) => Loose<R>) | Loose<R> : never } };\n\n");
  out.push_str("export interface Mock<Input = void> {\n  session?: Partial<Session>;\n  host?: string;\n  config?: Partial<Config>;\n  services?: Mocked<Services>;\n  input?: Input;\n  request?: RequestLine;\n  params?: Record<string, string>;\n  query?: Record<string, string>;\n  identity?: { subject: string; claims?: Record<string, unknown> };\n}\n\n");
  out.push_str("export interface Trace {\n  calls: { service: string; method: string; args: Record<string, unknown> }[];\n  session: { written: string[]; extended: number | null };\n}\n\n");
  out.push_str("export type TestCtx<Input = void, P extends keyof Routes = keyof Routes> = ActionCtx<Input, P> & { trace: Trace; request: RequestLine };\n\n");
  out.push_str("const lowered = (): never => {\n  throw new Error(\"a test file is lowered by `fsr test`, never run as JavaScript\");\n};\n\n");
  out.push_str("export function ctx<Input = void, P extends keyof Routes = keyof Routes>(mock: Mock<Input>): TestCtx<Input, P> {\n  void mock;\n  return lowered();\n}\n\n");
  out.push_str(TESTING_DECLARATIONS);
  out.push_str("export const assert = {\n  ok(value: unknown, message?: string): void {\n    void value;\n    void message;\n    lowered();\n  },\n  equal(actual: unknown, expected: unknown, message?: string): void {\n    void actual;\n    void expected;\n    void message;\n    lowered();\n  },\n  match(actual: unknown, pattern: string | RegExp, message?: string): void {\n    void actual;\n    void pattern;\n    void message;\n    lowered();\n  },\n  rejects(run: Promise<unknown> | (() => Promise<unknown>), kind?: string): Promise<void> {\n    void run;\n    void kind;\n    return lowered();\n  },\n  throws(run: Promise<unknown> | (() => Promise<unknown>), kind?: string): Promise<void> {\n    void run;\n    void kind;\n    return lowered();\n  },\n};\n");
  out
}

/// `generated/client.ts`: the contract's types as the browser sees them, the
/// props of every page inferred from its loader's return and one typed callable
/// per action, nested by route id.
fn client_module(contract: &Contract, session: Option<&str>, routes: &[Route], layouts: &[String], sources: &[SourceEntry], actions: &[ActionEntry], prefix: &str, consts: &Consts, config: &[(String, infer::Ts)]) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import { action as call } from \"@snapfire/fsr-client\";\n\n");
  out.push_str(&typescript::type_declarations(contract, Flavour::Client));

  for route in routes {
    let props = match sources.iter().find(|s| s.id == route.id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None, input_type: None, consts, config }.returns(body).print(Flavour::Client),
      None => "{}".to_owned(),
    };
    let _ = writeln!(out, "export type {} = {props};", props_name(&route.id));
  }
  for id in layouts {
    let props = match sources.iter().find(|s| s.id == *id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None, input_type: None, consts, config }.returns(body).print(Flavour::Client),
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
    let returns = infer::Inferer { contract, session, input: action.input.as_deref(), input_type: None, consts, config }.returns(body).print(Flavour::Client);
    let arg = match &action.input {
      Some(input) => format!("input: {input}"),
      None => String::new(),
    };
    let path: Vec<String> = action.id.split('.').map(str::to_owned).collect();
    tree.push((path, format!("call(\"{prefix}{}\") as unknown as ({arg}) => Promise<{returns}>", action.id)));
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

/// Refuses two rows claiming one id. Ids are derived from directory names, so
/// a collision reaches the host as `Bind(Claimed(..))` naming neither file and
/// the generated module as a type declared twice; this names both.
fn claimed(report: &Report, routes: &[Route], handler_routes: &[Route], layout_ids: &[String]) -> Result<(), BuildError> {
  for (kind, rows) in [("source", &report.sources), ("action", &report.actions), ("handler", &report.handlers)] {
    let mut seen: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for (id, module) in rows {
      if let Some(first) = seen.insert(id, module) {
        return Err(BuildError::ClaimedId { kind: kind.to_owned(), id: id.clone(), first: first.to_owned(), second: module.clone() });
      }
    }
  }
  for (kind, rows) in [("route", routes), ("handler route", handler_routes)] {
    let mut seen: std::collections::HashMap<&str, &Path> = std::collections::HashMap::new();
    for route in rows {
      if let Some(first) = seen.insert(&route.id, &route.dir) {
        return Err(BuildError::ClaimedId {
          kind: kind.to_owned(),
          id: route.id.clone(),
          first: first.display().to_string(),
          second: route.dir.display().to_string(),
        });
      }
    }
  }
  let mut named: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
  for id in routes.iter().map(|r| r.id.as_str()).chain(layout_ids.iter().map(String::as_str)) {
    if let Some(first) = named.insert(props_name(id), id) {
      return Err(BuildError::ClaimedId { kind: "props type".to_owned(), id: props_name(id), first: first.to_owned(), second: id.to_owned() });
    }
  }
  Ok(())
}

/// `$root` is `RootProps`, `product.$id` is `ProductIdProps`, `admin.users` is
/// `AdminUsersProps`. A parameter's `$` marker is not part of the name.
fn props_name(id: &str) -> String {
  let mut name = String::new();
  for part in id.split(['.', '-', '_']) {
    let mut chars = part.trim_start_matches('$').chars();
    if let Some(first) = chars.next() {
      name.extend(first.to_uppercase());
      name.push_str(chars.as_str());
    }
  }
  name + "Props"
}

/// Every module a component places as an island, in tree order.
fn island_modules(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<(String, bool)> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<(String, bool)>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Island { module, children, define, .. } => {
        out.push((module.clone(), *define));
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

/// A client module that mounts one framework's components: the three exports
/// the registry names and the bare specifiers the module imports, which the
/// page's import map has to supply.
pub(crate) struct Adapter {
  pub(crate) module: &'static str,
  mounter: &'static str,
  patcher: &'static str,
  unmounter: &'static str,
  /// The entry's `claims`, for a layout mounted as one tree with its page.
  claims: Option<&'static str>,
  needs: &'static [&'static str],
}

pub(crate) const REACT: Adapter = Adapter { module: "@snapfire/fsr-client/react", mounter: "reactMounter", patcher: "reactPatcher", unmounter: "reactUnmounter", claims: None, needs: &["react", "react-dom/client"] };

/// A layout declared `tree(Layout)`: the React adapter's tree mounter, which renders the page inside the layout's root.
pub(crate) const REACT_TREE: Adapter = Adapter { module: "@snapfire/fsr-client/react", mounter: "reactTreeMounter", patcher: "reactTreePatcher", unmounter: "reactUnmounter", claims: Some("reactTreeClaims"), needs: &["react", "react-dom/client"] };

pub(crate) const VUE: Adapter = Adapter { module: "@snapfire/fsr-client/vue", mounter: "vueMounter", patcher: "vuePatcher", unmounter: "vueUnmounter", claims: None, needs: &["vue"] };

const ADAPTERS: &[&Adapter] = &[&REACT, &VUE];

/// The packages a client adapter imports at runtime. A shell records the
/// version it vendors of each, so a site built against it renders under the
/// same ones and its specs fetch matching development builds.
fn framework_packages() -> Vec<String> {
  let mut packages: Vec<String> = ADAPTERS.iter().flat_map(|adapter| adapter.needs.iter()).map(|need| crate::vendor::package_of(need)).collect();
  packages.sort();
  packages.dedup();
  packages
}

/// The specifiers a client adapter imports from one framework package.
fn framework_needs(package: &str) -> Vec<&'static str> {
  ADAPTERS.iter().flat_map(|adapter| adapter.needs.iter().copied()).filter(|need| crate::vendor::package_of(need) == package).collect()
}

/// The version `fsr add` is suggested with for a package nothing records.
fn suggested_version(package: &str) -> &'static str {
  match package {
    "react" | "react-dom" => crate::direction::REACT,
    _ => "<version>",
  }
}

/// The adapter a module is registered with. A source the lowerer reads is
/// React's; a foreign one belongs to the framework whose plugin claims its
/// extension, when the client has an adapter for that framework.
fn adapter_for(module: &str, trees: &[String]) -> Result<&'static Adapter, BuildError> {
  let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
  if !snapfire_fsr_lower::component::is_foreign(file) {
    return Ok(if trees.iter().any(|m| m == module) { &REACT_TREE } else { &REACT });
  }
  let ext = Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
  match snapfire_compiler_wire::claimed(&ext) {
    Some("vue") => Ok(&VUE),
    Some(ext) => Err(BuildError::NoAdapter { module: module.to_owned(), ext: ext.to_owned() }),
    None => Err(BuildError::UnknownComponent { module: module.to_owned() }),
  }
}

/// `generated/islands.ts`: one `registerIsland` per module discovery named
/// and per component a page or layout places as an island, so the browser
/// mounts exactly what the plan file refers to. A module nothing mounts is
/// left out and a mounter nothing registers is never imported, which is what
/// keeps a framework off a page that has no component of it.
fn islands_module(islands: &[String], static_modules: &[String], defines: &[String], trees: &[String], options: &Options) -> Result<String, BuildError> {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  let registered: Vec<&String> = islands.iter().filter(|m| !static_modules.contains(m)).collect();
  let defining = registered.iter().any(|m| defines.contains(m));
  match defining {
    true => {
      let _ = writeln!(out, "import {{ defineMounter, registerIsland }} from \"@snapfire/fsr-client\";");
    }
    false => {
      let _ = writeln!(out, "import {{ registerIsland }} from \"@snapfire/fsr-client\";");
    }
  }
  let mut imported: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
  for module in registered.iter().filter(|m| !defines.contains(**m)) {
    let adapter = adapter_for(module, trees)?;
    let names = match imported.iter_mut().find(|(seen, _)| *seen == adapter.module) {
      Some((_, names)) => names,
      None => {
        imported.push((adapter.module, Vec::new()));
        &mut imported.last_mut().unwrap().1
      }
    };
    for name in [adapter.mounter, adapter.patcher, adapter.unmounter].into_iter().chain(adapter.claims) {
      if !names.contains(&name) {
        names.push(name);
      }
    }
  }
  for (module, names) in &imported {
    let _ = writeln!(out, "import {{ {} }} from \"{module}\";", names.join(", "));
  }
  out.push_str("\nexport function registerIslands(): void {\n");
  let prefix = options.prefix();
  for module in registered {
    let Some((path, export)) = module.split_once('#') else { continue };
    // A foreign source is imported as itself: the typechecker knows it through
    // `<types>/foreign.d.ts` and snapfirec rewrites the specifier to the
    // module its plugin emitted.
    let js = match snapfire_fsr_lower::component::is_foreign(path) {
      true => path.to_owned(),
      false => path.rsplit_once('.').map(|(stem, _)| format!("{stem}.js")).unwrap_or_else(|| format!("{path}.js")),
    };
    if defines.contains(module) {
      let _ = writeln!(out, "  registerIsland(\"{prefix}{module}\", {{ loader: () => import(\"../{js}\"), mount: defineMounter }});");
      continue;
    }
    let adapter = adapter_for(module, trees)?;
    let claims = adapter.claims.map(|claims| format!(", claims: {claims}")).unwrap_or_default();
    let _ = writeln!(out, "  registerIsland(\"{prefix}{module}\", {{ loader: () => import(\"../{js}\").then((m) => m.{export}), mount: {}, patch: {}, unmount: {}{claims} }});", adapter.mounter, adapter.patcher, adapter.unmounter);
  }
  out.push_str("}\n");
  Ok(out)
}

/// Refuses a registry the page's import map cannot supply: each adapter the
/// registry imports and the specifiers that adapter imports, looked up in the
/// app's map and, for a site, the shell's. An app with no readable map has
/// nothing to check against.
fn check_island_imports(app: &Path, layout: &crate::xwpm::Layout, shell: Option<&ShellContract>, islands: &[String], static_modules: &[String], defines: &[String], trees: &[String], set: &ComponentSet) -> Result<(), BuildError> {
  let Some(served) = served_specifiers(app, layout, shell) else {
    return Ok(());
  };
  let mut checked: Vec<&str> = Vec::new();
  for module in islands.iter().filter(|m| !static_modules.contains(m) && !defines.contains(m)) {
    let adapter = adapter_for(module, trees)?;
    let remedy = || crate::direction::for_adapter(adapter.module).map(|d| format!("; `fsr use <app dir> {}` writes it", d.name)).unwrap_or_default();
    // The dialect's placements have the React module as their runtime, so a
    // mounted module importing them needs the map to say so.
    if set.imports_value_from(module, TEMPLATE_SPECIFIER) && !resolves(&served, TEMPLATE_SPECIFIER) {
      return Err(BuildError::IslandImports { module: module.clone(), adapter: adapter.module.to_owned(), missing: format!("`{TEMPLATE_SPECIFIER}`"), remedy: remedy() });
    }
    if checked.contains(&adapter.module) {
      continue;
    }
    checked.push(adapter.module);
    let missing: Vec<String> = std::iter::once(adapter.module).chain(adapter.needs.iter().copied()).filter(|specifier| !resolves(&served, specifier)).map(|specifier| format!("`{specifier}`")).collect();
    if let Some((last, rest)) = missing.split_last() {
      let missing = match rest.is_empty() {
        true => last.clone(),
        false => format!("{} or {last}", rest.join(", ")),
      };
      return Err(BuildError::IslandImports { module: module.clone(), adapter: adapter.module.to_owned(), missing, remedy: remedy() });
    }
  }
  Ok(())
}

/// The dialect's template module, whose placements a mounted module loads
/// from the client's `template.js`.
const TEMPLATE_SPECIFIER: &str = "@snapfire/fsr-authoring/template";

/// Every specifier the app's import map serves, its scopes included, plus the
/// shell's for a site. `None` when the app has no readable map.
fn served_specifiers(app: &Path, layout: &crate::xwpm::Layout, shell: Option<&ShellContract>) -> Option<Vec<String>> {
  let map = std::fs::read_to_string(app.join(&layout.importmap)).ok().and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
  let scopes = map.get("scopes").and_then(|scopes| scopes.as_object()).into_iter().flat_map(|scopes| scopes.values());
  let mut served: Vec<String> = map.get("imports").into_iter().chain(scopes).filter_map(|table| table.as_object()).flat_map(|table| table.keys().cloned()).collect();
  served.extend(shell.into_iter().flat_map(|contract| contract.imports.keys().cloned()));
  Some(served)
}

/// Whether `specifier` is a key of `served` or sits under one ending in `/`.
fn resolves(served: &[String], specifier: &str) -> bool {
  served.iter().any(|key| key == specifier || (key.ends_with('/') && specifier.starts_with(key.as_str())))
}

/// Refuses an import map entry for a vendored package that does not sit under
/// the layout's base. A site serves its vendor tree under its own prefix, so a
/// map written before the `[site]` section names URLs the site never answers.
/// Only a specifier the map already carries is checked: the manifest can name a
/// package whose files the tree no longer holds.
fn check_vendor_urls(app: &Path, layout: &crate::xwpm::Layout) -> Result<(), BuildError> {
  let manifest = crate::vendor::VendorManifest::read(app, layout)?;
  if manifest.packages.is_empty() {
    return Ok(());
  }
  let map = crate::vendor::read_import_map(app, layout)?;
  let Some(imports) = map.get("imports").and_then(|imports| imports.as_object()) else {
    return Ok(());
  };
  let base = layout.base.trim_end_matches('/');
  for package in manifest.packages.values() {
    for (specifier, rel) in &package.entries {
      let Some(found) = imports.get(specifier).and_then(|url| url.as_str()) else {
        continue;
      };
      let want = format!("{base}/{rel}");
      if found != want {
        return Err(BuildError::VendorUrl {
          map: format!("`{}`", app.join(&layout.importmap).display()),
          specifier: specifier.clone(),
          found: found.to_owned(),
          base: base.to_owned(),
          want,
        });
      }
    }
  }
  Ok(())
}

/// The exact version of each vendored framework whose server markup the
/// renderer matches, one row per package a client adapter imports. A site
/// takes each version from the shell it is built against, since the shell's
/// import map overrides its own and the browser loads one copy; every other
/// application reads its own vendor manifest. An import map serving a
/// framework package with no version recorded anywhere is refused. So is a
/// React major the renderer has no rules for.
fn vendored_frameworks(app: &Path, layout: &crate::xwpm::Layout, shell: Option<(&Path, &ShellContract)>) -> Result<std::collections::BTreeMap<String, String>, BuildError> {
  let contract = shell.map(|(_, contract)| contract);
  let manifest = || format!("`{}`", app.join(&layout.vendor).join(crate::vendor::VENDOR_MANIFEST).display());
  let vendored = crate::vendor::VendorManifest::read(app, layout)?;
  let served = served_specifiers(app, layout, contract).unwrap_or_default();
  let mut frameworks = std::collections::BTreeMap::new();
  for package in framework_packages() {
    let own = vendored.packages.get(&package).map(|entry| entry.version.clone());
    let from_shell = contract.and_then(|contract| contract.frameworks.get(&package).cloned());
    if let (Some(own), Some(from_shell), Some((path, _))) = (&own, &from_shell, shell) {
      if own != from_shell {
        return Err(BuildError::FrameworkShellMismatch { package, site: own.clone(), shell: from_shell.clone(), manifest: manifest(), contract: format!("`{}`", path.display()) });
      }
    }
    match from_shell.or(own) {
      Some(version) if package == "react" && snapfire_fsr_ir::ReactMajor::of(&version).is_none() => {
        let majors: Vec<String> = snapfire_fsr_ir::ReactMajor::ALL.iter().map(|major| major.number().to_string()).collect();
        return Err(BuildError::ReactMajor { version, supported: majors_text(&majors) });
      }
      Some(version) if package == "vue" && snapfire_fsr_ir::VueMajor::of(&version).is_none() => {
        let majors: Vec<String> = snapfire_fsr_ir::VueMajor::ALL.iter().map(|major| major.number().to_string()).collect();
        return Err(BuildError::VueMajor { version, supported: majors_text(&majors) });
      }
      Some(version) => {
        frameworks.insert(package, version);
      }
      None => {
        let Some(specifier) = framework_needs(&package).into_iter().find(|need| resolves(&served, need)) else {
          continue;
        };
        let version = suggested_version(&package).to_owned();
        let served_by_shell = shell.filter(|(_, contract)| resolves(&contract.imports.keys().cloned().collect::<Vec<_>>(), specifier));
        return match served_by_shell {
          Some((path, _)) => Err(BuildError::FrameworkShellUnrecorded { package, specifier: specifier.to_owned(), contract: format!("`{}`", path.display()), version }),
          None => Err(BuildError::FrameworkUnrecorded { package, specifier: specifier.to_owned(), manifest: manifest(), app: app.display().to_string(), version }),
        };
      }
    }
  }
  Ok(frameworks)
}

/// Refuses a framework specifier the application maps somewhere other than the
/// URL its shell serves. The shell's import map overrides the site's on a
/// shared specifier, so the browser never fetches the site's own URL for one.
/// A package the site vendors itself is left to `check_vendor_urls`.
fn check_shell_urls(app: &Path, layout: &crate::xwpm::Layout, shell: Option<(&Path, &ShellContract)>) -> Result<(), BuildError> {
  let Some((path, contract)) = shell else {
    return Ok(());
  };
  let vendored = crate::vendor::VendorManifest::read(app, layout)?;
  let map = crate::vendor::read_import_map(app, layout)?;
  let Some(imports) = map.get("imports").and_then(|imports| imports.as_object()) else {
    return Ok(());
  };
  let packages = framework_packages();
  for (specifier, url) in imports {
    let package = crate::vendor::package_of(specifier);
    if !packages.contains(&package) || vendored.packages.contains_key(&package) {
      continue;
    }
    let (Some(found), Some(want)) = (url.as_str(), contract.imports.get(specifier)) else {
      continue;
    };
    if found != want {
      return Err(BuildError::ShellUrl {
        map: format!("`{}`", app.join(&layout.importmap).display()),
        specifier: specifier.clone(),
        found: found.to_owned(),
        want: want.clone(),
        contract: format!("`{}`", path.display()),
      });
    }
  }
  Ok(())
}

/// Where an application keeps its custom elements' shadow templates.
pub const ELEMENTS_DIR: &str = "elements";

/// `elements/<tag>.tsx`, each the default export of a custom element's shadow
/// template, as the tag and the template's module.
/// `18 and 19` for a list of majors or the one there is.
fn majors_text(majors: &[String]) -> String {
  match majors.split_last() {
    Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
    _ => majors.concat(),
  }
}

/// Asks each framework plugin to describe the components it claims under the
/// source directories, so the set lowers them rather than leaving them
/// foreign. A plugin that is not on PATH leaves its components foreign and
/// the report says so; the bundle names the same binary when it needs it.
fn describe_foreign(app: &Path, set: &mut ComponentSet, report: &mut Report) -> Result<(), BuildError> {
  use snapfire_compiler_wire::host::{HostError, Worker};
  use snapfire_compiler_wire::{Outcome, Unit};
  let mut by_ext: std::collections::BTreeMap<&'static str, Vec<PathBuf>> = std::collections::BTreeMap::new();
  for dir in types::source_dirs(app) {
    collect_plugin_files(&app.join(dir), &mut by_ext);
  }
  for (ext, mut files) in by_ext {
    files.sort();
    let mut worker = match Worker::start(ext) {
      Ok(worker) => worker,
      Err(HostError::NotFound { binary, hint }) => {
        report.plugins.push(format!("`{binary}` is not on PATH, so a `.{ext}` component mounts in the browser rather than hydrating the server's markup; `{hint}` puts it there"));
        continue;
      }
      Err(e) => return Err(BuildError::Plugin(e.to_string())),
    };
    let mut units = Vec::new();
    for path in &files {
      let filename = path.strip_prefix(app).unwrap_or(path).to_string_lossy().replace('\\', "/");
      let source = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.clone(), e))?;
      units.push(Unit { filename, path: path.to_string_lossy().into_owned(), source, options: Default::default(), files: Default::default() });
    }
    for round in 0..2 {
      let outcomes = worker.describe(units.clone()).map_err(|e| BuildError::Plugin(e.to_string()))?;
      let mut again = Vec::new();
      for (mut unit, outcome) in units.into_iter().zip(outcomes) {
        match outcome {
          Outcome::Described(described) => set.describe(unit.filename, described),
          Outcome::Failed { diagnostics } => {
            let why = diagnostics.iter().map(|d| match (d.line, d.column) {
              (Some(line), Some(column)) => format!("{line}:{column}: {}", d.message),
              _ => d.message.clone(),
            }).collect::<Vec<_>>().join("; ");
            set.undescribed(unit.filename, why);
          }
          Outcome::Needs { files } => {
            if round == 1 {
              set.undescribed(unit.filename, format!("{} asked for {} again after being given it", worker.name(), files.join(", ")));
              continue;
            }
            let base = Path::new(&unit.path).parent().map(Path::to_path_buf).unwrap_or_default();
            let mut missing = None;
            for specifier in files {
              match std::fs::read_to_string(base.join(&specifier)) {
                Ok(content) => {
                  unit.files.insert(specifier, content);
                }
                Err(e) => missing = Some(format!("`{specifier}`, which the component names, could not be read: {e}")),
              }
            }
            match missing {
              Some(why) => set.undescribed(unit.filename, why),
              None => again.push(unit),
            }
          }
          Outcome::Ok(_) => return Err(BuildError::Plugin(format!("{} compiled `{}` where it was asked to describe it", worker.name(), unit.filename))),
        }
      }
      units = again;
      if units.is_empty() {
        break;
      }
    }
  }
  Ok(())
}

fn collect_plugin_files(dir: &Path, into: &mut std::collections::BTreeMap<&'static str, Vec<PathBuf>>) {
  let Ok(entries) = std::fs::read_dir(dir) else { return };
  for entry in entries.flatten() {
    let path = entry.path();
    if path.is_dir() {
      collect_plugin_files(&path, into);
    } else if let Some(ext) = path.extension().and_then(|e| e.to_str()).and_then(|e| snapfire_compiler_wire::claimed(&e.to_ascii_lowercase())) {
      into.entry(ext).or_default().push(path);
    }
  }
}

fn element_templates(app: &Path) -> Result<Vec<(String, String)>, BuildError> {
  let mut out = Vec::new();
  for file in sorted_files(&app.join(ELEMENTS_DIR), ".tsx")? {
    let name = file.file_name().unwrap_or_default().to_string_lossy().into_owned();
    let tag = name.trim_end_matches(".tsx");
    let valid = tag.starts_with(|c: char| c.is_ascii_lowercase()) && tag.contains('-') && tag.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.' | '_'));
    if !valid {
      return Err(BuildError::ElementName { file: name });
    }
    out.push((tag.to_owned(), format!("{ELEMENTS_DIR}/{name}#default")));
  }
  Ok(out)
}

pub(crate) fn sorted_files(dir: &Path, suffix: &str) -> Result<Vec<PathBuf>, BuildError> {
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
      let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
      let chain = residue.chain();
      blame(report, module, at, residue.message, residue.hint, chain);
      Ok(())
    }
    Err(LowerError::Parse { file, message }) => {
      blame(report, module, file, message, None, String::new());
      Ok(())
    }
    Err(e) => Err(e.into()),
  }
}

/// Marks `module` client and files it under the cause at `at`, which several
/// modules reach when they import their way to the same unlowerable line.
fn blame(report: &mut Report, module: &str, at: String, message: String, hint: Option<String>, chain: String) {
  report.components.push((module.to_owned(), "client".to_owned(), at.clone()));
  let page = (module.to_owned(), chain);
  match report.causes.iter_mut().find(|c| c.at == at && c.message == message) {
    Some(cause) => cause.pages.push(page),
    None => report.causes.push(Cause { at, message, hint, pages: vec![page] }),
  }
}

/// The first handler of `component` reading a name that will not exist when
/// the host runs it, with the handler's index. A step binds `$props`, `$state`
/// and `$event` and re-runs the component's own `let`s; anything else a
/// handler reads was bound by the markup it sits in, a loop's variable for
/// one, which is gone by then.
fn captured_by_handler(component: &snapfire_fsr_ir::Component) -> Option<(String, usize)> {
  let mut bound: Vec<&str> = vec!["$props", "$state", "$event"];
  for stmt in &component.body {
    if let snapfire_fsr_ir::Stmt::Let { name, .. } = stmt {
      bound.push(name);
    }
  }
  component.handlers.iter().enumerate().find_map(|(index, handler)| {
    snapfire_fsr_ir::body_free_vars(&handler.body)
      .into_iter()
      .find(|name| !bound.contains(&name.as_str()))
      .map(|name| (name, index))
  })
}

/// The modules a template places as islands in server mode.
fn server_islands(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Island { module, mode, children, .. } => {
        if mode.as_deref() == Some(snapfire_fsr_ir::render::SERVER_MODE) && !out.contains(module) {
          out.push(module.clone());
        }
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
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The line and reason left on the first element whose handler did not lower.
fn unlowered_handler(tmpl: &snapfire_fsr_ir::Tmpl) -> Option<String> {
  unlowered_handlers(tmpl).into_iter().next()
}

/// The line and reason left on every element whose handler did not lower, in
/// document order.
fn unlowered_handlers(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::ast::{Entry, Expr, Lit};
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Element { attrs, children, .. } => {
        out.extend(attrs.iter().filter_map(|e| match e {
          Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR => Some(why.clone()),
          _ => None,
        }));
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
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
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The modules a template renders as components, islands aside.
fn nested_components(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Component { module, children, .. } => {
        if !out.contains(module) {
          out.push(module.clone());
        }
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
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
  let mut out = Vec::new();
  walk(tmpl, &mut out);
  out
}

/// The first slot a template renders, `content` included. A step renders the
/// island with no slot stack, so whatever filled it at first paint is gone
/// from the answer and the patch removes it from the document.
fn first_slot(tmpl: &snapfire_fsr_ir::Tmpl) -> Option<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl) -> Option<String> {
    match tmpl {
      Tmpl::Slot(name) => Some(name.clone()),
      Tmpl::Baked { children, .. } | Tmpl::Component { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().find_map(walk),
      Tmpl::If { then, r#else, .. } => walk(then).or_else(|| r#else.as_ref().and_then(|e| walk(e))),
      Tmpl::For { body, .. } => walk(body),
      Tmpl::Let { then, .. } => walk(then),
      Tmpl::Text(_) | Tmpl::Expr(_) => None,
    }
  }
  walk(tmpl)
}

/// The named slots a template places, `content` aside, in tree order.
fn slots_placed(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
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

  let page = page_file(dir)?.is_some();
  let handler = dir.join("route.ts").is_file();
  if page && handler {
    return Err(BuildError::PageAndRoute(dir.to_path_buf()));
  }
  if !page && dir.join("actions.ts").is_file() {
    return Err(BuildError::ActionsWithoutPage(dir.to_path_buf()));
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
      if layout_file(dir)?.is_none() {
        return Err(BuildError::SlotsWithoutLayout(child));
      }
      continue;
    }
    discover(root, &child, out, handlers)?;
  }
  Ok(())
}

/// The one page file in `dir`: `page.tsx`, `page.ts` or a `page.<ext>` for an
/// extension in `TEMPLATE_EXTENSIONS`. Two of them is a refusal naming both,
/// and a template is a refusal when this fsr was built without its feature,
/// so a template never becomes a silent 404.
fn page_file(dir: &Path) -> Result<Option<String>, BuildError> {
  route_file(dir, &["page.tsx", "page.ts"], "page")
}

/// The one layout file in `dir`, `layout.tsx` or a `layout.<ext>` for a
/// template extension, under the same rules as `page_file`.
fn layout_file(dir: &Path) -> Result<Option<String>, BuildError> {
  route_file(dir, &["layout.tsx"], "layout")
}

fn route_file(dir: &Path, sources: &[&str], stem: &str) -> Result<Option<String>, BuildError> {
  let templates: Vec<String> = snapfire_fsr_host::TEMPLATE_EXTENSIONS.iter().map(|ext| format!("{stem}.{ext}")).collect();
  let present: Vec<String> = sources.iter().map(|s| (*s).to_owned()).chain(templates.iter().cloned()).filter(|f| dir.join(f).is_file()).collect();
  match present.as_slice() {
    [] => Ok(None),
    [one] => {
      if templates.contains(one) && !cfg!(feature = "tera") {
        return Err(BuildError::TemplateFeature(dir.join(one)));
      }
      Ok(Some(one.clone()))
    }
    [first, second, ..] => Err(BuildError::PageAndTemplate { dir: dir.to_path_buf(), first: first.clone(), second: second.clone() }),
  }
}

/// The modules a template places as islands, `island(module="...")` with a
/// string literal, so they are bundled, registered and, for server mode,
/// lowered the way a TSX placement's are. An `island(` whose `module` is not
/// a literal is refused naming the line, since the build cannot bundle a
/// module it cannot name.
fn template_islands(file: &Path) -> Result<Vec<String>, BuildError> {
  let text = std::fs::read_to_string(file).map_err(|e| BuildError::Io(file.to_path_buf(), e))?;
  let call = regex::Regex::new(r#"\bisland\s*\("#).expect("a literal pattern");
  let literal = regex::Regex::new(r#"\bmodule\s*=\s*(?:"([^"]+)"|'([^']+)')"#).expect("a literal pattern");
  let mut out = Vec::new();
  for found in call.find_iter(&text) {
    let rest = &text[found.end()..];
    let args = &rest[..rest.find(')').unwrap_or(rest.len())];
    match literal.captures(args).and_then(|c| c.get(1).or_else(|| c.get(2))) {
      Some(m) => out.push(m.as_str().to_owned()),
      None => {
        let line = text[..found.start()].matches('\n').count() + 1;
        return Err(BuildError::TemplateIsland { file: file.to_path_buf(), line });
      }
    }
  }
  Ok(out)
}

/// Whether a module id names a template file rather than a component.
fn is_template(module: &str) -> bool {
  let path = module.split('#').next().unwrap_or(module);
  snapfire_fsr_host::TEMPLATE_EXTENSIONS.iter().any(|ext| path.ends_with(&format!(".{ext}")))
}

/// `routes/index` is `/`; `routes/product/[id]` is `/product/{id}`;
/// `routes/docs/[...rest]` is `/docs/{*rest}`. A directory holding a
/// `page.tsx` is a route at its own path, so `routes/` itself is `/` and
/// `routes/index/` is `/index`; no directory name is special. The id is every
/// segment joined with `.`, `$root` for the root, a parameter contributing
/// `$<name>`. The marker is what makes the id injective: a directory name is
/// alphanumerics, `_` and `-` only, so no static segment can produce a `$`
/// part and `routes/a/x` can never share an id with `routes/a/[x]`.
fn pattern_of(root: &Path, dir: &Path) -> Result<(String, String), BuildError> {
  let rel = dir.strip_prefix(root).unwrap_or(dir);
  let mut segments = Vec::new();
  let mut id_parts = Vec::new();
  for component in rel.components() {
    let name = component.as_os_str().to_string_lossy().to_string();
    if let Some(inner) = name.strip_prefix('[').and_then(|n| n.strip_suffix(']')) {
      if let Some(rest) = inner.strip_prefix("...") {
        segments.push(format!("{{*{rest}}}"));
        id_parts.push(format!("${rest}"));
      } else {
        segments.push(format!("{{{inner}}}"));
        id_parts.push(format!("${inner}"));
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
  let id = if id_parts.is_empty() { "$root".to_owned() } else { id_parts.join(".") };
  Ok((pattern, id))
}
