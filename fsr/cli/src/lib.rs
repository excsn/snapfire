//! Route discovery, the contract and the build that emits a plan file and the
//! generated TypeScript from them. `build` analyses, `write` puts the generated
//! files on disk and `emit` does both and then the browser bundle, which is the
//! whole artifact a host reads. The binary in `main.rs` is a thin front over them.

pub mod assets;
pub mod dev;
pub mod direction;
pub mod doctor;
pub mod new;
pub mod plugins;
pub mod published;
pub mod serve;
pub mod sites;
pub mod spec;
pub mod infer;
pub mod native;
pub mod test;
pub mod bundle;
pub mod census;
pub mod install;
pub mod ir;
pub mod typecheck;
pub mod types;
pub mod vendor;
pub mod tailwind;
pub mod cards;
pub(crate) mod css;
pub mod mdx;
pub mod themes;
pub mod xwpm;

pub use dev::{emit, DevOptions, Emitted};

use std::fmt;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use snapfire_fsr_lower::component::ComponentSet;
use snapfire_fsr_lower::{builtin_types, read_schema, read_session_defaults, LowerError, SessionDefaults, EXT_DIR, UPLOAD};
use snapfire_fsr_ir::ast::Consts;
use snapfire_fsr_ir::Owner;
use std::collections::HashMap;
use snapfire_fsr_plan::{ActionEntry, Child, ComponentEntry, HandlerEntry, Manifest, Node, RouteEntry, RowOwner, SourceEntry};
use snapfire_fsr_service::typescript::Flavour;
use snapfire_fsr_service::{typescript, Contract, ContractError, ImportError};

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BuildError {
  #[error("`{module}` does not lower ({at}: {reason}) and the application has no JSX framework to run it in the browser; make it lower or write it as a Vue or Svelte component or a custom element")]
  NoJsxFramework { module: String, at: String, reason: String },
  #[error("`{file}` {reason} and `{root}` loads it in the browser, but the application has no JSX framework to compile it with; move what `{root}` uses from it into a file of its own")]
  Unrendered { file: String, root: String, reason: &'static str },
  /// What `fsr doctor` found, when a bundle asked for it first. Carries the
  /// report so a caller prints the findings rather than a summary of them.
  #[error("{0}")]
  Doctor(crate::doctor::Report),
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("the configuration does not load: {0}")]
  Configuration(String),
  #[error("no `routes/` directory under {0}")]
  NoRoutes(PathBuf),
  #[error("{path}: `{name}` is not a route segment; use a name, `[param]`, `[...rest]` or `(group)`")]
  Segment { path: PathBuf, name: String },
  #[error("{file} is answered at `{path}` by {by} as well; a web server in front answers the file and the host the other, so rename one")]
  PublicClaimed { file: String, path: String, by: String },
  #[error("two routes answer `{pattern}`: {first} and {second}")]
  ClaimedPattern { pattern: String, first: String, second: String },
  #[error(transparent)]
  Lower(#[from] LowerError),
  #[error("{document}: {error}")]
  Import { document: String, error: ImportError },
  #[error("`{key}` is derived in both {first} and {second}; the browser keeps whichever registers last")]
  DerivedTwice { key: String, first: String, second: String },
  #[error("derived keys feed each other in a cycle: {}", keys.join(" -> "))]
  DeriveCycle { keys: Vec<String> },
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
  #[error("{file} names a theme `default`, which is the root theme's name; rename the file")]
  ReservedTheme { file: String },
  #[error("{file} sets `{token}`, which is not a token of {defined}; a named theme overrides the root's tokens, so define it in the root's @theme")]
  ThemeToken { file: String, token: String, defined: String },
  #[error("{file} is a named theme, but the root theme is not in `theme.root`, so it would outrank every named one; import Tailwind's theme as `@import \"tailwindcss/theme\" layer(theme.root);` or declare the root's tokens in `@layer theme.root {{ :root {{ ... }} }}`")]
  ThemeLayer { file: String },
  #[error("{module} sets theme `{name}`, which is not a theme; the themes are {known}")]
  UnknownTheme { module: String, name: String, known: String },
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
  #[error("svelte {version} is vendored, but this fsr mounts Svelte {supported} only")]
  SvelteMajor { version: String, supported: String },
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
  #[error("{what} needs an fsr built with its `{feature}` feature, as `cargo install snapfire_fsr_cli --features {feature}`")]
  MdxFeature { what: String, feature: &'static str },
  #[error("{file}: {message}")]
  Mdx { file: String, message: String },
  #[error("{module} sets the route's meta and so does {page}; keep one")]
  MetaTwice { module: String, page: String },
  #[error("{file}:{line}: `island(` whose `module` is not a string literal; the build bundles only a module it can name")]
  TemplateIsland { file: PathBuf, line: usize },
  #[error("{0} exports `paths`, which only a page loader may: a layout or a slot has no parameter set of its own")]
  PathsOffPage(String),
  #[error("{0}: a head `script` in `meta` carries inline code; give it a `src` or put the code in a module `src/main.ts` imports")]
  InlineScript(String),
  #[error("{0}: {1}")]
  SocialImage(PathBuf, String),
  #[error("`{0}` places an `<Embed>` and hydrates, but the browser has no still to render it with; place the `<Embed>` in a page or a component that does not hydrate")]
  EmbedInIsland(String),
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
  /// An image or a font the build could not derive, or a `[fonts]` key it could not place.
  #[error("assets: {0}")]
  Assets(String),
  #[error("{0}")]
  Types(String),
  #[error("bundle: {0}")]
  Bundle(String),
  #[error("{0}")]
  Tool(String),
  #[error("typecheck: {0}")]
  Typecheck(String),
  /// `[build] strict` and what it refused, one cause per line.
  #[error("strict: {0}")]
  Strict(String),
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
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Report {
  pub routes: Vec<(String, String)>,
  pub sources: Vec<(String, String)>,
  pub actions: Vec<(String, String)>,
  /// `METHOD pattern` and the `route.ts` that exports it.
  pub handlers: Vec<(String, String)>,
  /// `middleware.ts` when the app has one.
  pub middleware: Option<String>,
  /// The named themes from `styles/themes/`, the shell's among them for a site.
  pub themes: Vec<String>,
  /// Each layout or page that sets a theme, with the theme and whether it is forced.
  pub theme_of: Vec<(String, String)>,
  /// `theme.ts` when the app has one.
  pub theme_body: Option<String>,
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
  /// Module, how many of its render-path calls and how many of its static subtrees the server computes for the browser and how many of those subtrees keep a fallback in the bundle.
  pub hoisted: Vec<(String, usize, usize, usize)>,
  /// Components placed as islands in server mode and how many handlers each answers.
  pub islands: Vec<(String, usize)>,
  /// Each page or layout whose state, handlers and effects moved into an island beside it: the route module, the island and what it took.
  pub extracted: Vec<(String, String, Vec<String>)>,
  /// Each page or layout that holds state composition cannot and stays a framework root, with the reason.
  pub kept: Vec<(String, String)>,
  /// Each export under `ext/` as `file#name` and whether it is `lowered`, `native render` or `native body`.
  pub extensions: Vec<(String, String)>,
  /// Each derived key with `lowered` and the file deriving it, else with `browser` and `file:line:column: reason`.
  pub derives: Vec<(String, String, String)>,
  /// Per module, a render-path call the browser still makes after hoisting, as `file:line:column`, or a handler it runs as written, as `file:line:column: reason`.
  pub browser: Vec<(String, String)>,
  /// A policy in the configuration that breaks the page outright, and what to do about it: the CSP findings `fsr doctor` reports.
  pub csp: Vec<(String, String)>,
  pub services: Vec<(String, String)>,
  pub schemas: Vec<(String, String)>,
  pub types: Vec<(String, String)>,
  /// An image the markup named: its file, then its size and how many variants it gets.
  pub images: Vec<(String, String)>,
  /// A face the font directory holds or a remote stylesheet, one line each.
  pub fonts: Vec<String>,
  /// Each `<Embed>` whose id the page computes under `[media] embed_stills = "local"`: its module and the provider, which serves the still or none.
  pub embeds: Vec<(String, String)>,
  /// When a stylesheet imports Tailwind: how many classes the plan renders. Each module with a
  /// class value the build cannot list comes with that value.
  pub tailwind: Option<(usize, Vec<(String, String)>)>,
  /// What converting an `.mdx` or `.md` file left out: the file, the line and column, and why.
  pub mdx: Vec<(String, usize, usize, String)>,
  /// A named theme setting a token the stylesheet reads at build, one line each.
  pub theme_warnings: Vec<String>,
  /// The plan file as written and as it would be without whitespace, in bytes.
  /// The build writes it pretty so it reads and diffs; the second number is
  /// what a deployment would ship.
  pub plan: Option<usize>,
}

/// The report with what lowered as well as what reaches the browser, `--verbose`.
pub struct Verbose<'r>(&'r Report);

impl Report {
  /// Every row, the modules and bodies that lowered among them.
  pub fn verbose(&self) -> Verbose<'_> {
    Verbose(self)
  }
}

/// What renders or runs in the browser and what the build warns about; everything else is under `Report::verbose`.
impl fmt::Display for Report {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.write(f, false)
  }
}

impl fmt::Display for Verbose<'_> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    self.0.write(f, true)
  }
}

impl Report {
  fn write(&self, f: &mut fmt::Formatter<'_>, verbose: bool) -> fmt::Result {
    if verbose {
      section(f, "routes", &self.routes, "")?;
      section(f, "layouts", &self.layouts, "")?;
      section(f, "slots", &self.slots, "")?;
      section(f, "intercepts", &self.intercepts, "")?;
      section(f, "sources", &self.sources, "lowered")?;
      section(f, "actions", &self.actions, "lowered")?;
      section(f, "handlers", &self.handlers, "lowered")?;
    }
    if let Some(module) = self.middleware.as_ref().filter(|_| verbose) {
      writeln!(f, "{:<9} {:<22} {:<11} {module}", "middleware", "middleware", "lowered")?;
    }
    if verbose && !self.themes.is_empty() {
      writeln!(f, "{:<9} {}", "themes", self.themes.join(", "))?;
    }
    for (module, theme) in self.theme_of.iter().filter(|_| verbose) {
      writeln!(f, "{:<9} {module:<34} {theme}", "theme")?;
    }
    if let Some(module) = self.theme_body.as_ref().filter(|_| verbose) {
      writeln!(f, "{:<9} {:<22} {:<11} {module}", "theme.ts", "theme", "lowered")?;
    }
    if let Some((path, keys, imports, differ)) = &self.shell {
      if verbose {
        writeln!(f, "{:<9} {path}: {keys} store keys, {imports} imports", "shell")?;
      }
      if !differ.is_empty() {
        let label = if verbose { "" } else { "shell" };
        writeln!(f, "{label:<9} {} mapped differently here; the shell's mapping serves at mount", differ.join(", "))?;
      }
    }
    for (i, (module, owner, detail)) in self.components.iter().filter(|(_, owner, _)| verbose || owner == "client" || owner == "foreign").enumerate() {
      let label = if i == 0 { "rendered" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {owner:<11} {detail}")?;
    }
    if verbose {
      for (i, (source, detail)) in self.images.iter().enumerate() {
        let label = if i == 0 { "image" } else { "" };
        writeln!(f, "{label:<9} {source:<34} {detail}")?;
      }
      for (i, line) in self.fonts.iter().enumerate() {
        let label = if i == 0 { "font" } else { "" };
        writeln!(f, "{label:<9} {line}")?;
      }
      for (module, provider) in &self.embeds {
        let shows = if provider == "youtube" { "so YouTube serves the still" } else { "so the placeholder shows the title alone" };
        writeln!(f, "{:<9} {module:<34} {provider}: the id is computed, {shows}", "embed")?;
      }
    }
    if let Some((count, unlisted)) = &self.tailwind {
      if verbose {
        writeln!(f, "{:<9} {count} classes in {}", "tailwind", tailwind::CLASSES_FILE)?;
      }
      for (module, value) in unlisted {
        writeln!(f, "{:<9} {module:<34} {}: a class value the build cannot list, {value}", "warning", tailwind::UNLISTED)?;
        writeln!(f, "{:<9} list its values in the stylesheet with @source inline(\"...\")", "")?;
      }
    }
    for (file, line, column, message) in &self.mdx {
      writeln!(f, "{:<9} {file}:{line}:{column}: {message}", "warning")?;
    }
    for warning in &self.theme_warnings {
      writeln!(f, "{:<9} {warning}", "warning")?;
    }
    for (i, cause) in self.causes.iter().enumerate() {
      let label = if i == 0 { "client" } else { "" };
      writeln!(f, "{label:<9} {:<34} {}", cause.at, cause.message)?;
      if let Some(hint) = &cause.hint {
        writeln!(f, "{:<9} {hint}", "")?;
      }
      let pages = cause.pages.len();
      writeln!(f, "{:<9} {pages} module{} render{} in the browser for it", "", if pages == 1 { "" } else { "s" }, if pages == 1 { "s" } else { "" })?;
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
    for (i, (module, island, holds)) in self.extracted.iter().filter(|_| verbose).enumerate() {
      let label = if i == 0 { "extracted" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {island} ({})", holds.join(", "))?;
    }
    for (i, (module, why)) in self.kept.iter().enumerate() {
      let label = if i == 0 { "kept" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {why}")?;
    }
    for (i, (module, handlers)) in self.islands.iter().filter(|_| verbose).enumerate() {
      let label = if i == 0 { "islands" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {:<11} {handlers} handler{}", "server", if *handlers == 1 { "" } else { "s" })?;
    }
    for (i, (key, owner, detail)) in self.derives.iter().filter(|(_, owner, _)| verbose || owner != "lowered").enumerate() {
      let label = if i == 0 { "derived" } else { "" };
      writeln!(f, "{label:<9} {key:<34} {owner:<11} {detail}")?;
    }
    if verbose {
      section(f, "extensions", &self.extensions, "")?;
    }
    for (i, (module, site)) in self.browser.iter().enumerate() {
      let label = if i == 0 { "browser" } else { "" };
      writeln!(f, "{label:<9} {module:<34} {site}")?;
    }
    for (what, remedy) in &self.csp {
      writeln!(f, "{:<9} {what}", "csp")?;
      writeln!(f, "{:<9} {remedy}", "")?;
    }
    for (i, (module, values, chunks, fallbacks)) in self.hoisted.iter().filter(|_| verbose).enumerate() {
      let label = if i == 0 { "hoisted" } else { "" };
      let mut parts = Vec::new();
      if *values > 0 {
        parts.push(format!("{values} value{}", if *values == 1 { "" } else { "s" }));
      }
      if *chunks > 0 {
        parts.push(format!("{chunks} subtree{}", if *chunks == 1 { "" } else { "s" }));
      }
      if *fallbacks > 0 {
        parts.push(format!("{fallbacks} with a fallback"));
      }
      writeln!(f, "{label:<9} {module:<34} {}", parts.join(", "))?;
    }
    for (i, (service, document)) in self.services.iter().filter(|_| verbose).enumerate() {
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
    if verbose {
      section(f, "schemas", &self.schemas, "")?;
      section(f, "types", &self.types, "")?;
    }
    if let Some(written) = self.plan.filter(|_| verbose) {
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

#[non_exhaustive]
pub struct Options {
  /// The application as a site: every id the build emits prefixed
  /// `<name>:` and every pattern under `at`. Read from `[site]` beside the app.
  pub site: Option<SiteOptions>,
  /// The module the document renders through; every route's root node.
  pub shell: String,
  /// The slot of the shell a page lands in.
  pub slot: String,
  /// Refuses a component that renders in the browser alone: one that did not lower, a `.vue` file left foreign and a plugin missing from PATH. `[build] strict` in the configuration or `--strict`.
  pub strict: bool,
}

impl Options {
  pub fn site(mut self, site: Option<SiteOptions>) -> Self {
    self.site = site;
    self
  }

  pub fn shell(mut self, shell: String) -> Self {
    self.shell = shell;
    self
  }

  pub fn slot(mut self, slot: String) -> Self {
    self.slot = slot;
    self
  }

  pub fn strict(mut self, strict: bool) -> Self {
    self.strict = strict;
    self
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SiteOptions {
  pub name: String,
  pub at: String,
  /// The shell's `generated/shell.json` the site is built against, resolved.
  pub shell: Option<PathBuf>,
}

impl SiteOptions {
  /// `SiteOptions` with `name` and `at` and every other field at its default.
  pub fn new(name: String, at: String) -> Self {
    Self { name, at, shell: Default::default() }
  }

  pub fn shell(mut self, shell: Option<PathBuf>) -> Self {
    self.shell = shell;
    self
  }
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
    options.strict = config_beside(app).and_then(|config| config.build).and_then(|build| build.strict).unwrap_or(false);
    options
  }
}

/// A service name without its site prefix: the name a test mocks it by,
/// since a site's bodies call `<name>:<service>` and its tests are written
/// against `<service>`.
pub fn unprefixed(service: &str) -> &str {
  service.rsplit_once(':').map(|(_, rest)| rest).unwrap_or(service)
}

/// The configuration beside `app`, when it names this app directory. One that does not load is
/// `None` here and refused by [`build_with`].
pub(crate) fn config_beside(app: &Path) -> Option<snapfire_fsr_host::config::Config> {
  let root = serve::project_root(app);
  let config = snapfire_fsr_host::config::Config::load(&root).ok()?;
  let given = app.canonicalize().ok()?;
  (config.app.canonicalize().ok()? == given).then_some(config)
}

/// The `[site]` section of the configuration beside `app`, when one names
/// this app directory.
pub fn site_beside(app: &Path) -> Option<SiteOptions> {
  let config = config_beside(app)?;
  let root = config.root.clone();
  config.site.map(|s| SiteOptions { name: s.name, at: s.at, shell: s.shell.as_ref().map(|rel| root.join(rel)) })
}

/// The shell contract a shell's build emits as `generated/shell.json` and a
/// site's build reads: the store keys the shell's loaders seed with their
/// types as the browser sees them, the import map it serves, the exact
/// version of each framework it vendors and the fsr version that wrote it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
  /// The shell's named themes, from its `styles/themes/`, which a site's layouts may name too.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub themes: Vec<String>,
  /// The tokens the shell's root theme defines, which a site's named themes may override.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tokens: Vec<String>,
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
      strict: false,
    }
  }
}

/// Everything a build produces. `files` are paths relative to the app
/// directory with their contents; `write` puts them on disk.
#[non_exhaustive]
pub struct Built {
  pub manifest: Manifest,
  pub contract: Contract,
  pub report: Report,
  pub files: Vec<(String, String)>,
  /// The session defaults every body was lowered with, so a test lowers its target the same way.
  pub defaults: SessionDefaults,
  /// The route modules the browser mounts, as files, which is all of `routes/` a bundle compiles.
  pub browser_routes: Vec<String>,
  /// Every file the browser loads: the ones it loads by name, the routes among them, and every local file they import. The bundle and `fsr test` compile these.
  pub browser_files: Vec<String>,
  /// The packages the build needs declarations for that `types/` lacks, from the reading the report's `types` rows came from.
  pub missing_declarations: Vec<String>,
  /// Files the bundle overlay holds alone: islands split out of pages and the copies a second JSX runtime compiles.
  pub added: Vec<String>,
  /// By route composition file: the local files it imports that the browser never loads, such as a server-mode island it places. `fsr test` compiles them with the composition.
  pub composition_files: Vec<(String, Vec<String>)>,
  /// What the build derived from the images and fonts, written as `generated/assets.json`.
  pub assets: snapfire_fsr_host::assets::AssetsManifest,
  /// What answered the lowerer's asset questions and defines the map the bundle is compiled by;
  /// it keeps growing as the bundle reports references the lowering never saw.
  pub(crate) resolver: std::rc::Rc<assets::Resolver>,
}

impl Built {
  /// Defines the asset paths the bundle reported and the lowering never saw, then recomputes
  /// the manifest and the map; the two files to write come back.
  pub fn adopt(&mut self, paths: &[String]) -> Result<Vec<(String, String)>, BuildError> {
    self.resolver.reconcile(paths)?;
    Ok(self.refresh_assets())
  }

  /// Recomputes the asset manifest and the map from the resolver, after the bundle reported
  /// references the lowering never saw; the two files to write come back.
  pub fn refresh_assets(&mut self) -> Vec<(String, String)> {
    let fonts = self.assets.fonts.clone();
    self.assets = assets::manifest(&self.resolver, fonts);
    let refreshed = vec![
      (snapfire_fsr_host::assets::ASSETS_FILE.to_owned(), serde_json::to_string_pretty(&self.assets).expect("an assets manifest serializes") + "\n"),
      (assets::MAP_FILE.to_owned(), assets::map_text(&self.resolver, &self.assets.fonts)),
    ];
    for (name, content) in &refreshed {
      match self.files.iter_mut().find(|(n, _)| n == name) {
        Some(entry) => entry.1 = content.clone(),
        None => self.files.push((name.clone(), content.clone())),
      }
    }
    refreshed
  }
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
/// The failure kinds a route may name an error module for, as `FailureKind`
/// spells them, paired with the filename infix `error.<infix>.tsx` uses.

/// What a segment falls back to when its loader fails: an error module and
/// the per-kind ones beside it.
#[derive(Default, Clone)]
struct Boundary {
  module: Option<String>,
  kinds: Vec<(String, String)>,
}

impl Boundary {
  fn read(dir: &Path, rel: &str) -> Self {
    Self {
      module: ["error.tsx", "error.ts"].iter().find(|f| dir.join(f).is_file()).map(|f| format!("{rel}/{f}#default")),
      kinds: error_kind_modules(dir, rel),
    }
  }

  fn is_empty(&self) -> bool {
    self.module.is_none() && self.kinds.is_empty()
  }

  /// This one when it names anything, the outer one otherwise. A route that
  /// declares any boundary owns every kind, so an `error.not-found.tsx` of
  /// its own is not silently paired with the root's `error.tsx`.
  fn or(&self, outer: &Boundary) -> Boundary {
    match self.is_empty() {
      true => outer.clone(),
      false => self.clone(),
    }
  }

  fn modules(&self) -> impl Iterator<Item = &String> {
    self.module.iter().chain(self.kinds.iter().map(|(_, m)| m))
  }
}

/// `error.not-found.tsx` and its kin beside `error.tsx`, as (kind, module).
fn error_kind_modules(dir: &Path, rel: &str) -> Vec<(String, String)> {
  let mut found = Vec::new();
  for kind in snapfire_fsr_core::ext::FailureKind::ALL {
    let infix = kind.as_str().replace('_', "-");
    for ext in ["tsx", "ts"] {
      let file = format!("error.{infix}.{ext}");
      if dir.join(&file).is_file() {
        found.push((kind.as_str().to_owned(), format!("{rel}/{file}#default")));
        break;
      }
    }
  }
  found
}

/// Builds `app` with plugins started for this build alone; [`build_with`] takes a command's pool.
pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError> {
  build_with(app, options, &mut plugins::Plugins::new())
}

/// Builds `app`, reading each framework component through `plugins`, the pool the command keeps for
/// the compiler it drives as well.
pub fn build_with(app: &Path, options: &Options, plugins: &mut plugins::Plugins) -> Result<Built, BuildError> {
  build_in(app, options, plugins, &mut None)
}

/// `build_with`, or with `skipped` given, a build that records each residue in a loader, an action, a handler, middleware or an extension there and goes on without that part rather than stopping at it.
pub(crate) fn build_in(app: &Path, options: &Options, plugins: &mut plugins::Plugins, skipped: &mut Option<Vec<snapfire_fsr_lower::Residue>>) -> Result<Built, BuildError> {
  macro_rules! lowered {
    ($e:expr) => {
      match $e {
        Ok(value) => value,
        Err(LowerError::Residue(residue)) if skipped.is_some() => {
          if let Some(all) = skipped.as_mut() {
            all.push(residue);
          }
          Default::default()
        }
        Err(e) => return Err(e.into()),
      }
    };
  }
  let routes_dir = app.join("routes");
  if !routes_dir.is_dir() {
    return Err(BuildError::NoRoutes(app.to_path_buf()));
  }

  match snapfire_fsr_host::config::Config::load(serve::project_root(app)) {
    Ok(_) | Err(snapfire_fsr_host::HostError::NoConfig(_)) => {}
    Err(e) => return Err(BuildError::Configuration(e.to_string())),
  }
  let mut report = Report::default();
  if let Some(config) = config_beside(app) {
    report.csp = doctor::csp(&config).into_iter().map(|f| (f.what, f.remedy)).collect();
  }
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
  for ty in builtin_types() {
    schemas.types.insert(ty.name, ty.def);
  }
  for schema in sorted_files(&app.join("schemas"), ".ts")? {
    let file = schema.file_name().unwrap_or_default().to_string_lossy().to_string();
    let rel = format!("schemas/{file}");
    let text = std::fs::read_to_string(&schema).map_err(|e| BuildError::Io(schema.clone(), e))?;
    for ty in read_schema(&rel, &text)? {
      if ty.name == UPLOAD {
        return Err(BuildError::DuplicateType { name: ty.name, first: "the host's own".to_owned(), second: rel });
      }
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
  let sections = assets::Sections::of(app);
  let public_path = bundle_base(options.site.as_ref());
  let resolver = std::rc::Rc::new(assets::Resolver::new(app, &public_path, sections.clone()));
  let mut set = ComponentSet::new(app).with_defaults(defaults.clone()).provide(snapfire_fsr_lower::HEAD_MODULE, HEAD_HELPERS).with_elements(elements.iter().cloned().collect()).with_assets(resolver.clone(), sections.images.rewrite);
  let mdx_settings = mdx::settings(&config_beside(app).map(|config| config.mdx).unwrap_or_default())?;
  let mut mdx_files: Vec<(String, String)> = Vec::new();
  #[cfg(feature = "mdx")]
  let mdx_installed = {
    let class_attr = match types::jsx_source(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? {
      "react" => snapfire_fsr_mdx::ClassAttr::ClassName,
      _ => snapfire_fsr_mdx::ClassAttr::Class,
    };
    let (installed_set, installed) = mdx::install(app, set, &mdx_settings, class_attr)?;
    set = installed_set;
    if let Some(content) = mdx::collections(app, &installed)? {
      set = set.provide(mdx::CONTENT_MODULE, content.clone());
      mdx_files.push((mdx::CONTENT_MODULE.to_owned(), content));
    }
    mdx_files.push((mdx::DECLARATIONS_FILE.to_owned(), mdx::DECLARATIONS.to_owned()));
    installed
  };
  let _ = &mdx_settings;
  describe_foreign(app, &mut set, &mut report, plugins)?;
  for file in sorted_files(&app.join(EXT_DIR), ".ts")? {
    let rel = format!("{EXT_DIR}/{}", file.file_name().unwrap_or_default().to_string_lossy());
    report.extensions.extend(lowered!(set.lower_extensions(&rel)));
  }
  for (_, module) in &elements {
    set.lower(module)?;
    let refused = |reason: String| BuildError::ElementTemplate { module: module.clone(), reason };
    let Some((_, component)) = set.components.iter_mut().find(|(m, _)| m == module) else { continue };
    if component.owner.hydrates() {
      return Err(refused("it holds state or a handler, which belong in the element's class".to_owned()));
    }
    component.shadow = snapfire_fsr_ir::ShadowRoot::take(&mut component.render).map_err(|e| refused(e.to_string()))?;
  }

  let mut routes = Vec::new();
  let mut handler_routes = Vec::new();
  discover(&routes_dir, &routes_dir, &mut routes, &mut handler_routes)?;
  routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));
  handler_routes.sort_by(|a, b| a.pattern.cmp(&b.pattern));

  let root_boundary = Boundary::read(&routes_dir, "routes");

  let not_found_module = ["not-found.tsx", "not-found.ts"]
    .iter()
    .find(|f| routes_dir.join(f).is_file())
    .map(|f| format!("routes/{f}#default"));

  let mut entries = Vec::new();
  let mut sources = Vec::new();
  let mut actions = Vec::new();
  let mut islands: Vec<String> = Vec::new();
  let mut templates: Vec<PathBuf> = Vec::new();
  for module in root_boundary.modules().chain(not_found_module.iter()) {
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
      let body = lowered!(set.lower_loader(&loader_module));
      let meta = lowered!(set.lower_meta(&loader_module));
      refuse_inline_scripts(&loader_module, meta.as_ref())?;
      let store = lowered!(set.lower_store(&loader_module));
      let defaults = lowered!(set.lower_defaults(&loader_module));
      if lowered!(set.lower_paths(&loader_module)).is_some() {
        return Err(BuildError::PathsOffPage(loader_module));
      }
      sources.push(SourceEntry::lowered(id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults));
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
        let body = lowered!(set.lower_loader(&loader_module));
        let meta = lowered!(set.lower_meta(&loader_module));
        refuse_inline_scripts(&loader_module, meta.as_ref())?;
        let store = lowered!(set.lower_store(&loader_module));
      let defaults = lowered!(set.lower_defaults(&loader_module));
        if lowered!(set.lower_paths(&loader_module)).is_some() {
          return Err(BuildError::PathsOffPage(loader_module));
        }
        sources.push(SourceEntry::lowered(slot_id.clone(), loader_module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults));
        report.sources.push((slot_id.clone(), loader_module));
        Some(slot_id.clone())
      } else {
        None
      };
      let slot_actions = slot_dir.join("actions.ts");
      if slot_actions.is_file() {
        let module = format!("{slot_rel}/actions.ts");
        for lowered in lowered!(set.lower_actions(&module)) {
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
      let error = Boundary::read(&slot_dir, &slot_rel);
      for module in [Some(&page), loading.as_ref()].into_iter().flatten().chain(error.modules()) {
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
    let page_source = format!("{rel}/{page_file}");
    let page_meta = match snapfire_fsr_lower::component::is_transformed(&page_file) {
      true => lowered!(set.lower_meta(&page_source)),
      false => None,
    };
    let source = if loader.is_file() {
      let module = format!("{rel}/page.loader.ts");
      let body = lowered!(set.lower_loader(&module));
      let meta = match (lowered!(set.lower_meta(&module)), page_meta) {
        (Some(_), Some(_)) => return Err(BuildError::MetaTwice { module, page: page_source }),
        (meta, page_meta) => meta.or(page_meta),
      };
      refuse_inline_scripts(&module, meta.as_ref())?;
      let store = lowered!(set.lower_store(&module));
      let defaults = lowered!(set.lower_defaults(&module));
      let paths = lowered!(set.lower_paths(&module));
      if paths.is_some() && !route.pattern.contains('{') {
        return Err(BuildError::PathsWithoutParameter { module, pattern: route.pattern.clone() });
      }
      sources.push(SourceEntry::lowered(route.id.clone(), module.clone(), body).with_meta(meta).with_store(store).with_defaults(defaults).with_paths(paths));
      report.sources.push((route.id.clone(), module));
      Some(route.id.clone())
    } else if let Some(meta) = page_meta {
      refuse_inline_scripts(&page_source, Some(&meta))?;
      let empty = vec![snapfire_fsr_ir::ast::Stmt::Return(snapfire_fsr_ir::ast::Expr::Object(Vec::new()))];
      sources.push(SourceEntry::lowered(route.id.clone(), page_source.clone(), empty).with_meta(Some(meta)));
      report.sources.push((route.id.clone(), page_source));
      Some(route.id.clone())
    } else {
      None
    };

    let actions_file = route.dir.join("actions.ts");
    if actions_file.is_file() {
      let module = format!("{rel}/actions.ts");
      for lowered in lowered!(set.lower_actions(&module)) {
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

    let local = Boundary::read(&route.dir, &rel);
    let boundary = local.or(&root_boundary);
    let loading = ["loading.tsx", "loading.ts"]
      .iter()
      .find(|f| route.dir.join(f).is_file())
      .map(|f| format!("{rel}/{f}#default"));

    for module in [Some(&page), loading.as_ref()].into_iter().flatten().chain(local.modules()) {
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
      error: boundary.module.clone(),
      error_kinds: boundary.kinds.clone(),
      cache_key: Some(page),
      children: Vec::new(),
      keep: Vec::new(),
    };
    let content = wrap_in_layouts(content, &wrapping, &root_boundary);
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
        error: boundary.module.clone(),
        error_kinds: boundary.kinds.clone(),
        cache_key: Some(module.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      };
      let plan = intercept_plan(variant, &slot, &wrapping[..=declaring], &root_boundary);
      report.intercepts.push((format!("{} into {slot}", route.pattern), module));
      intercepts.push(RouteEntry { pattern: route.pattern.clone(), plan: shell_over(options, plan) });
    }
  }

  let mut handlers = Vec::new();
  for route in &handler_routes {
    let rel = route.dir.strip_prefix(app).unwrap_or(&route.dir);
    let rel = rel.to_string_lossy().replace('\\', "/");
    let module = format!("{rel}/route.ts");
    for lowered in lowered!(set.lower_handlers(&module)) {
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
    Some(lowered!(set.lower_middleware("middleware.ts")))
  } else {
    None
  };

  let mut not_found = not_found_module.map(|module| {
    let wrapping: Vec<&LayoutInfo> = layouts.iter().filter(|l| l.dir == routes_dir).collect();
    let content = Node { id: 0, module: module.clone(), source: None, deferred: false, fallback: None, error: root_boundary.module.clone(), error_kinds: root_boundary.kinds.clone(), cache_key: Some(module), children: Vec::new(), keep: Vec::new() };
    shell_over(options, wrap_in_layouts(content, &wrapping, &root_boundary))
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
  // A page or layout holding state, handlers or effects is split before it is
  // lowered: they move with the markup that uses them into an island beside
  // it, which only the bundle overlay holds.
  let mut generated: Vec<(String, String)> = Vec::new();
  for module in islands.iter().filter(|m| m.starts_with("routes/") && !m.contains(".tera#")) {
    match set.extract_route(module) {
      Ok(Some(extracted)) => {
        report.extracted.push((module.clone(), extracted.island.clone(), extracted.holds));
        generated.push((extracted.file, extracted.source));
        generated.push(extracted.page);
      }
      Ok(None) => {}
      Err(why) => report.kept.push((module.clone(), why)),
    }
  }
  for module in &islands {
    if module.starts_with("routes/") && !module.contains(".tera#") {
      match set.lower_route(module) {
        Ok(Some(extracted)) => {
          report.extracted.push((module.clone(), extracted.island.clone(), extracted.holds));
          generated.retain(|(file, _)| *file != extracted.page.0);
          generated.push((extracted.file, extracted.source));
          generated.push(extracted.page);
          continue;
        }
        Ok(None) => continue,
        // A page or layout is composition and never renders in the browser,
        // so a residue nothing could move into an island stops the build. An
        // island split out of a page is not composition.
        Err(e) if !generated.iter().any(|(file, _)| module.split_once('#').is_some_and(|(f, _)| f == file)) => return Err(e.into()),
        Err(_) => {}
      }
    }
    lower_into(&mut set, module, &mut report)?;
  }
  let (generated_images, generated_heads) = generated_images(app, &routes_dir, &routes, &mut set, &mut sources)?;
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
  set.settle_proofs();
  set.settle_fills();
  for rewrite in &mut set.rewrites {
    rewrite.module = format!("{}{}", options.prefix(), rewrite.module);
    report.hoisted.push((rewrite.module.clone(), rewrite.sites.len(), rewrite.chunks.len(), rewrite.chunks.iter().filter(|c| !c.proven).count()));
  }
  let mut rewritten = set.rewritten();
  for (file, source) in &generated {
    if !rewritten.iter().any(|(f, _)| f == file) {
      rewritten.push((file.clone(), source.clone()));
    }
  }
  report.hoisted.sort();
  report.browser = set.remaining.iter().chain(set.browser_texts.iter()).map(|(module, site)| (format!("{}{module}", options.prefix()), site.clone())).collect();
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
  // What renders each lowered module in the browser, which picks its adapter.
  let mut owners: HashMap<String, Owner> = HashMap::new();
  for (module, residue) in std::mem::take(&mut set.foreign_residue) {
    let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
    report.components.push((module.clone(), "foreign".to_owned(), at.clone()));
    match report.foreign.iter_mut().find(|c| c.at == at && c.message == residue.message) {
      Some(cause) => cause.pages.push((module, String::new())),
      None => report.foreign.push(Cause { at, message: residue.message, hint: residue.hint, pages: vec![(module, String::new())] }),
    }
  }
  for (module, component) in std::mem::take(&mut set.components) {
    let detail = match component.owner {
      Owner::Vue => component.owner.as_str().to_owned(),
      Owner::React => String::new(),
      Owner::Fsr => "static".to_owned(),
    };
    report.components.push((module.clone(), "lowered".to_owned(), detail));
    if component.owner.hydrates() && !snapfire_fsr_ir::ast::embed_players(&component.render).is_empty() {
      return Err(BuildError::EmbedInIsland(module));
    }
    if sections.media.embed_stills == snapfire_fsr_host::config::EmbedStills::Local {
      for player in snapfire_fsr_ir::ast::computed_embed_players(&component.render) {
        if let Some(provider) = snapfire_fsr_lower::assets::embed_provider(&player) {
          report.embeds.push((module.clone(), provider.to_owned()));
        }
      }
    }
    if !component.owner.hydrates() {
      static_modules.push(module.clone());
    }
    owners.insert(module.clone(), component.owner);
    for placement in island_modules(&component.render) {
      if placement.define && !defines.contains(&placement.module) {
        defines.push(placement.module.clone());
      }
      if !placement.server && !islands.contains(&placement.module) {
        islands.push(placement.module);
      }
    }
    let head = set.heads.get(&module).map(|rows| rows.iter().map(|row| snapfire_fsr_plan::HeadRow { tag: row.tag.clone(), attrs: row.attrs.clone() }).collect()).unwrap_or_default();
    components.push(ComponentEntry { module, body: component, head });
  }
  // A component that does not lower renders in the browser alone, as a
  // component of the application's JSX framework. FSR is no JSX framework,
  // so an application without one has nothing to run it with.
  let uses_react = types::serves_react(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? || owners.values().any(|owner| *owner == Owner::React);
  for (module, residue) in std::mem::take(&mut set.browser_only) {
    let at = format!("{}:{}:{}", residue.file, residue.line, residue.column);
    if !uses_react {
      return Err(BuildError::NoJsxFramework { module, at, reason: residue.message });
    }
    let chain = residue.chain();
    blame(&mut report, &module, at, residue.message, residue.hint, chain);
    owners.insert(module.clone(), Owner::React);
    if !islands.contains(&module) {
      islands.push(module);
    }
  }
  if options.strict {
    if let Some(refused) = strict_refusal(&report) {
      return Err(BuildError::Strict(refused));
    }
  }
  // A component a page places as an island is mounted because the page asked
  // for it, whatever its own markup would need.
  let placements: Vec<Placement> = components.iter().flat_map(|entry| island_modules(&entry.body.render)).collect();
  if let Some(both) = placements.iter().find(|p| p.server && placements.iter().any(|q| !q.server && q.module == p.module)) {
    return Err(BuildError::ServerIsland { module: both.module.clone(), reason: "it is placed in server mode and also as a browser island, and a module has one owner".to_owned() });
  }
  let mut placed: Vec<String> = placements.into_iter().filter(|p| !p.server).map(|p| p.module).collect();
  // A component a framework hydrates that composition renders inline is
  // placed as an island by the renderer, so it mounts like any other.
  for entry in components.iter().filter(|entry| entry.body.owner == Owner::Fsr) {
    for module in inline_components(&entry.body.render) {
      if owners.get(&module).is_some_and(|owner| owner.hydrates()) && !placed.contains(&module) {
        if !islands.contains(&module) {
          islands.push(module.clone());
        }
        placed.push(module);
      }
    }
  }
  for module in std::mem::take(&mut set.mounted) {
    if !islands.contains(&module) {
      islands.push(module);
    }
  }
  static_modules.retain(|module| !placed.contains(module));
  for (file, text) in dialect_rewrites(app, &mut set, &islands, &static_modules, &defines, &owners, &rewritten)? {
    match rewritten.iter_mut().find(|(f, _)| *f == file) {
      Some((_, held)) => *held = text,
      None => rewritten.push((file, text)),
    }
  }
  if types::jsx_source(app, &crate::xwpm::Layout::of_site(app, options.site.as_ref())?)? == "react" {
    for (file, text) in foreign_rewrites(app, &mut set, &rewritten)? {
      match rewritten.iter_mut().find(|(f, _)| *f == file) {
        Some((_, held)) => *held = text,
        None => rewritten.push((file, text)),
      }
    }
  }
  let mut exported: Vec<String> = Vec::new();
  for module in &islands {
    let Some((file, name)) = module.split_once('#') else { continue };
    if name == "default" || snapfire_fsr_lower::component::is_foreign(file) || snapfire_fsr_lower::component::is_transformed(file) || generated.iter().any(|(f, _)| f == file) {
      continue;
    }
    let source = match rewritten.iter().find(|(f, _)| f == file) {
      Some((_, text)) => text.clone(),
      None => match std::fs::read_to_string(app.join(file)) {
        Ok(text) => text,
        Err(_) => continue,
      },
    };
    if !snapfire_fsr_lower::declares_unexported(file, &source, name) {
      continue;
    }
    let text = format!("{source}\nexport {{ {name} }};\n");
    match rewritten.iter_mut().find(|(f, _)| f == file) {
      Some((_, held)) => *held = text,
      None => rewritten.push((file.to_owned(), text)),
    }
    exported.push(module.clone());
  }
  for (module, _, detail) in &mut report.components {
    if placed.contains(module) && detail == "static" {
      detail.clear();
    }
  }
  report.components.sort();
  let layout = crate::xwpm::Layout::of_site(app, options.site.as_ref())?;
  let mut vue_declarations: Vec<(String, String)> = set.described().filter_map(|(file, described)| snapfire_fsr_lower::vue::props_declaration(file, described).map(|declaration| (snapfire_fsr_lower::vue::props_declaration_path(file), declaration))).collect();
  vue_declarations.sort();
  let shim = types::foreign_shim(app, &set.foreign);
  let mut browser_route_files: Vec<String> = report
    .components
    .iter()
    .filter(|(module, owner, detail)| module.starts_with("routes/") && (owner == "client" || detail != "static"))
    .filter_map(|(module, _, _)| module.split_once('#').map(|(file, _)| file.to_owned()))
    .collect();
  browser_route_files.sort();
  browser_route_files.dedup();
  let islands_generated: Vec<&String> = report.extracted.iter().filter_map(|(_, island, _)| island.split_once('#').map(|(file, _)| file)).map(|file| generated.iter().find(|(f, _)| f == file).map(|(f, _)| f)).flatten().collect();
  let generated_files: Vec<String> = islands_generated.into_iter().cloned().collect();
  browser_route_files.retain(|file| !generated_files.contains(file));
  let mut roots = browser_roots(app, &islands, &static_modules, &browser_route_files, &generated_files, &public_path);
  let function_modules = set.function_modules();
  for file in &function_modules {
    if !roots.contains(file) {
      roots.push(file.clone());
    }
  }
  if !uses_react {
    refuse_unrendered(&mut set, &rewritten, &roots)?;
  }
  let mut browser_files: Vec<String> = Vec::new();
  for root in &roots {
    for file in set.local_reach(root) {
      if !browser_files.contains(&file) {
        browser_files.push(file);
      }
    }
  }
  let derives = browser_derives(&mut set, &browser_files, &mut report)?;
  report.causes.sort_by(|a, b| (&a.at, &a.message).cmp(&(&b.at, &b.message)));
  for cause in &mut report.causes {
    cause.pages.sort();
  }

  let session_type = session_import.as_ref().map(|_| "Session");
  let prefix = options.prefix();
  let config = public_of(app);
  let mut image_sources: Vec<String> = generated_images.iter().filter_map(|image| image.source.clone()).filter(|id| !routes.iter().any(|r| r.id == *id) && !layout_ids.contains(id)).collect();
  image_sources.sort();
  image_sources.dedup();
  let client = client_module(&contract, session_type, &routes, &layout_ids, &image_sources, &sources, &actions, &prefix, &set.consts, &config, &natives);
  let shell = match options.site.as_ref().and_then(|site| site.shell.as_ref()) {
    Some(path) => Some((path, ShellContract::read(path)?)),
    None => None,
  };
  let mut theme_names = theme_files(&app.join(&sections.dirs.styles).join(THEMES_DIR))?;
  for name in shell.iter().flat_map(|(_, contract)| contract.themes.iter()) {
    if !theme_names.contains(name) {
      theme_names.push(name.clone());
    }
  }
  theme_names.sort();
  report.themes = theme_names.clone();
  let root_tokens = themes::root_tokens(&app.join(&sections.dirs.styles))?;
  let named_themes = themes::named_themes(app, &sections.dirs.styles)?;
  let shell_tokens = shell.as_ref().map(|(_, contract)| contract.tokens.clone());
  let written_themes = themes::write(&sections.dirs.styles, &named_themes, &root_tokens, shell_tokens.as_deref())?;
  report.theme_warnings = written_themes.warnings.clone();
  let mut theme_entries: Vec<snapfire_fsr_plan::ThemeEntry> = Vec::new();
  let mut route_modules: Vec<String> = Vec::new();
  for plan in entries.iter().map(|e| &e.plan).chain(intercepts.iter().map(|e| &e.plan)).chain(not_found.iter()) {
    plan_modules(plan, &mut route_modules);
  }
  for module in route_modules {
    let Some(file) = module.strip_suffix("#default") else { continue };
    let stem = Path::new(file).file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ts = file.ends_with(".tsx") || file.ends_with(".ts");
    if !file.starts_with("routes/") || !ts || !(stem == "layout" || stem.starts_with("page")) {
      continue;
    }
    let Some(theme) = set.theme_export(file)? else { continue };
    if !theme_names.contains(&theme.name) {
      let known = if theme_names.is_empty() { format!("none: add {}/{THEMES_DIR}/{}.css", sections.dirs.styles, theme.name) } else { theme_names.join(", ") };
      return Err(BuildError::UnknownTheme { module, name: theme.name, known });
    }
    report.theme_of.push((module.clone(), if theme.force { format!("{} forced", theme.name) } else { theme.name.clone() }));
    theme_entries.push(snapfire_fsr_plan::ThemeEntry { module, name: theme.name, force: theme.force });
  }
  let theme_body = if app.join("theme.ts").is_file() {
    report.theme_body = Some("theme.ts".to_owned());
    Some(lowered!(set.lower_theme("theme.ts")))
  } else {
    None
  };
  check_vendor_urls(app, &layout)?;
  check_shell_urls(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  let frameworks = vendored_frameworks(app, &layout, shell.as_ref().map(|(path, contract)| (path.as_path(), contract)))?;
  plugin_compilers(app, &layout)?;
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
  // What a deferred segment promises a first wave: the keys its `store` is
  // inferred to return.
  let mut sources = sources;
  for source in &mut sources {
    let Some(loader) = &source.body else { continue };
    let mut keys: Option<Vec<String>> = None;
    for body in [&source.store, &source.defaults].into_iter().flatten() {
      match seeded(&contract, session_type, &set.consts, &config, loader, body) {
        Some(fields) => keys.get_or_insert_with(Vec::new).extend(fields.into_iter().map(|(key, _)| key)),
        None => {
          keys = None;
          break;
        }
      }
    }
    if let Some(keys) = &mut keys {
      keys.sort();
      keys.dedup();
    }
    source.store_keys = keys;
  }
  let lowered: std::collections::HashSet<&str> = components.iter().map(|entry| entry.module.as_str()).collect();
  let mut reads = std::collections::BTreeMap::new();
  for module in islands.iter().filter(|module| !lowered.contains(module.as_str())) {
    let file = module.split('#').next().unwrap_or(module);
    let keys = set.store_reads(file);
    if !keys.is_empty() {
      reads.insert(module.clone(), keys);
    }
  }
  let mut social = social_images(app, &routes_dir, &resolver)?;
  social.extend(generated_heads);
  let mut manifest = Manifest::new(entries).with_heads(social).with_images(generated_images).with_sources(sources).with_actions(actions).with_components(components).with_clients(clients).with_not_found(not_found).with_handlers(handlers).with_middleware(middleware).with_intercepts(intercepts).with_consts(set.consts.clone()).with_frameworks(frameworks.clone()).with_theme(theme_body, theme_entries);
  manifest.reads = reads;
  let mut composition_files: Vec<(String, Vec<String>)> = Vec::new();
  for component in manifest.components.iter().filter(|c| c.body.owner == snapfire_fsr_ir::Owner::Fsr && c.module.starts_with("routes/")) {
    let Some((file, _)) = component.module.split_once('#') else { continue };
    if composition_files.iter().any(|(f, _)| f == file) {
      continue;
    }
    let reached: Vec<String> = set.local_reach(file).into_iter().filter(|f| f != file && !browser_files.contains(f)).collect();
    composition_files.push((file.to_owned(), reached));
  }
  manifest.derives = derives;
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

  resolver.scan(&app.join(&sections.dirs.images), &sections.dirs.images);
  let (fonts, font_lines) = assets::fonts(app, &public_path, &sections)?;
  let assets_manifest = assets::manifest(&resolver, fonts);
  for entry in &assets_manifest.entries {
    let detail = if entry.passthrough {
      format!("{}x{}, served as it is", entry.width, entry.height)
    } else {
      format!("{}x{}, {} variants", entry.width, entry.height, entry.variants.len())
    };
    report.images.push((entry.source.clone(), detail));
  }
  report.fonts = font_lines;
  for (path, why) in resolver.refused.borrow().iter() {
    report.images.push((path.clone(), format!("not read: {why}")));
  }
  let mut tailwind_file = None;
  let mut cards_file = None;
  if tailwind::wanted(&app.join(&sections.dirs.styles)) {
    #[allow(unused_mut)]
    let mut classes = tailwind::collect(&manifest);
    #[cfg(feature = "mdx")]
    classes.listed.retain(|class, _| !mdx_installed.log.borrow().classes.contains(class));
    report.tailwind = Some((classes.listed.len(), classes.unlisted.clone()));
    tailwind_file = Some(classes.file());
    if !manifest.images.is_empty() {
      let table = tailwind::card_table(&app.join(&sections.dirs.styles), &classes)?;
      cards_file = Some(serde_json::to_string_pretty(&table).expect("a card table serializes") + "\n");
    }
  }
  let plan_text = manifest.to_sexpr();
  report.plan = Some(plan_text.len());
  let mut files = vec![(PLAN_FILE.to_owned(), plan_text)];
  files.extend(tailwind_file.map(|text| (tailwind::CLASSES_FILE.to_owned(), text)));
  files.extend(cards_file.map(|text| (tailwind::CARDS_FILE.to_owned(), text)));
  files.extend(contracts.into_iter().map(|(rel, c)| (rel, c.to_json() + "\n")));
  files.extend(rewritten.into_iter().filter(|(file, _)| !snapfire_fsr_lower::component::is_transformed(file)).map(|(file, source)| (format!("{}/{file}", dev::BUNDLE_OVERLAY), source)));
  files.extend(mdx_files);
  files.extend(written_themes.files);
  let mut bundle_files = generated_files.clone();
  bundle_files.extend(written_themes.added);
  #[cfg(feature = "mdx")]
  {
    let log = mdx_installed.log.borrow().clone();
    for file in &log.files {
      let copy = mdx::browser_copy(app, file, &mdx_installed)?;
      let at = format!("{}/{file}", dev::BUNDLE_OVERLAY);
      files.retain(|(f, _)| *f != at);
      files.push((at, copy));
    }
    report.mdx = log.warnings.clone();
  }
  match &options.site {
    None => {
      let mut shell_contract = shell_contract(app, &contract, session_type, &manifest.sources, &set.consts, &config, &frameworks)?;
      shell_contract.themes = report.themes.clone();
      shell_contract.tokens = root_tokens.names();
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
  let split: Vec<String> = report.extracted.iter().filter_map(|(_, island, _)| island.split_once('#').map(|(file, _)| file.to_owned())).collect();
  let registry = islands_module(&islands, &static_modules, &defines, &owners, &split, &exported, &function_modules, options)?;
  check_island_imports(app, &layout, shell.as_ref().map(|(_, contract)| contract), &islands, &static_modules, &defines, &owners)?;
  files.extend([
    ("generated/uploads.d.ts".to_owned(), UPLOAD_DECLARATION.to_owned()),
    ("generated/assets.d.ts".to_owned(), ASSET_DECLARATIONS.to_owned()),
    (snapfire_fsr_host::assets::ASSETS_FILE.to_owned(), serde_json::to_string_pretty(&assets_manifest).expect("an assets manifest serializes") + "\n"),
    (assets::MAP_FILE.to_owned(), assets::map_text(&resolver, &assets_manifest.fonts)),
    ("generated/native.d.ts".to_owned(), native::declarations(&natives)),
    ("generated/services.d.ts".to_owned(), declarations),
    ("generated/elements.d.ts".to_owned(), types::element_declarations(app, &layout, &elements)?),
    ("generated/head.ts".to_owned(), HEAD_HELPERS.to_owned()),
    ("generated/fsr.ts".to_owned(), ctx_module(&routes.iter().chain(handler_routes.iter()).cloned().collect::<Vec<_>>(), session_import.as_deref(), &config, &report.themes)),
    ("generated/islands.ts".to_owned(), registry),
    ("generated/client.ts".to_owned(), client),
    ("generated/testing.ts".to_owned(), testing_module()),
    ("tsconfig.json".to_owned(), types::tsconfig(app, true, shim.is_some(), !vue_declarations.is_empty())?),
    ("tsconfig.build.json".to_owned(), types::tsconfig_build(app, &browser_files, &bundle_files, types::jsx_source(app, &layout)?)),
  ]);
  if let Some(shim) = shim {
    files.push((format!("{}/{}", layout.types.trim_end_matches('/'), types::FOREIGN_SHIM), shim));
  }
  files.extend(vue_declarations);
  let mut report = report;
  let declarations = types::Declarations::read(app)?;
  report.types = declarations.status();
  let missing_declarations = declarations.missing();
  claimed(&report, &routes, &handler_routes, &layout_ids)?;
  public_claimed(app, routes.iter().chain(&handler_routes).map(|r| r.pattern.as_str()).chain(manifest.images.iter().map(|i| i.pattern.as_str())))?;
  Ok(Built { manifest, contract, report, files, defaults, browser_routes: browser_route_files, browser_files, missing_declarations, added: generated_files, composition_files, assets: assets_manifest, resolver })
}

/// The files the browser loads by name: every island the registry mounts and
/// every module an `<Island define>` names, the route modules it mounts and
/// the islands split out of pages, the document's entry (`[document] entry`
/// under `public_path`, else `src/main`) and each browser half under `ext/`.
/// A server island and a template nothing mounts are not among them.
/// The `derive` calls the browser's files make, as plan rows, each listed in
/// the report. A key derived twice or derived keys that feed each other fail
/// the build.
fn browser_derives(set: &mut ComponentSet, browser_files: &[String], report: &mut Report) -> Result<Vec<snapfire_fsr_plan::DeriveEntry>, BuildError> {
  let mut rows: Vec<(String, snapfire_fsr_plan::DeriveEntry)> = Vec::new();
  for file in browser_files.iter().filter(|file| [".ts", ".tsx", ".js", ".jsx"].iter().any(|ext| file.ends_with(ext))) {
    for lowered in set.lower_derives(file)? {
      if let Some((first, _)) = rows.iter().find(|(_, row)| row.key == lowered.key) {
        return Err(BuildError::DerivedTwice { key: lowered.key, first: first.clone(), second: file.clone() });
      }
      rows.push((file.clone(), snapfire_fsr_plan::DeriveEntry { key: lowered.key, sources: lowered.sources, body: lowered.body, at: lowered.at, why: lowered.why }));
    }
  }
  if let Some(keys) = derive_cycle(rows.iter().map(|(_, row)| row)) {
    return Err(BuildError::DeriveCycle { keys });
  }
  rows.sort_by(|a, b| a.1.key.cmp(&b.1.key));
  for (file, row) in &rows {
    report.derives.push(match (&row.at, &row.why) {
      (Some(at), Some(why)) => (row.key.clone(), "browser".to_owned(), format!("{at}: {why}")),
      _ => (row.key.clone(), "lowered".to_owned(), file.clone()),
    });
  }
  Ok(rows.into_iter().map(|(_, row)| row).collect())
}

/// A chain of derived keys that comes back to its first, when one does.
fn derive_cycle<'a>(rows: impl Iterator<Item = &'a snapfire_fsr_plan::DeriveEntry> + Clone) -> Option<Vec<String>> {
  fn walk<'a>(key: &str, rows: &[&'a snapfire_fsr_plan::DeriveEntry], path: &mut Vec<String>, done: &mut std::collections::HashSet<String>) -> Option<Vec<String>> {
    if let Some(at) = path.iter().position(|k| k == key) {
      let mut cycle = path[at..].to_vec();
      cycle.push(key.to_owned());
      return Some(cycle);
    }
    if done.contains(key) {
      return None;
    }
    let row = rows.iter().find(|row| row.key == key)?;
    path.push(key.to_owned());
    for source in &row.sources {
      if let Some(cycle) = walk(source, rows, path, done) {
        return Some(cycle);
      }
    }
    path.pop();
    done.insert(key.to_owned());
    None
  }
  let rows: Vec<&snapfire_fsr_plan::DeriveEntry> = rows.collect();
  let mut done = std::collections::HashSet::new();
  rows.iter().find_map(|row| walk(&row.key, &rows, &mut Vec::new(), &mut done))
}

fn browser_roots(app: &Path, islands: &[String], static_modules: &[String], route_files: &[String], generated: &[String], public_path: &str) -> Vec<String> {
  let mut roots: Vec<String> = Vec::new();
  let mut add = |file: String| {
    if !roots.contains(&file) {
      roots.push(file);
    }
  };
  for module in islands.iter().filter(|m| !static_modules.contains(m)) {
    if let Some((file, _)) = module.split_once('#') {
      add(file.to_owned());
    }
  }
  for file in route_files.iter().chain(generated) {
    add(file.clone());
  }
  let configured = config_beside(app).and_then(|config| config.document.entry).and_then(|url| url.strip_prefix(public_path.trim_end_matches('/')).map(|rest| rest.trim_start_matches('/').to_owned()));
  let entry = configured.as_deref().unwrap_or("src/main.js");
  let stem = entry.strip_suffix(".js").unwrap_or(entry);
  if let Some(file) = [".ts", ".tsx", ".js", ".jsx"].iter().map(|ext| format!("{stem}{ext}")).find(|file| app.join(file).is_file()) {
    add(file);
  }
  let mut pending = vec![app.join("ext")];
  let mut halves = Vec::new();
  while let Some(dir) = pending.pop() {
    for path in std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|entry| entry.path()) {
      let name = path.to_string_lossy();
      if path.is_dir() {
        pending.push(path);
      } else if [".ts", ".tsx", ".js", ".jsx"].iter().any(|ext| name.ends_with(ext)) && !name.ends_with(".d.ts") {
        halves.push(path.strip_prefix(app).unwrap_or(&path).to_string_lossy().replace('\\', "/"));
      }
    }
  }
  halves.sort();
  for half in halves {
    add(half);
  }
  roots
}

/// In an application with no JSX framework, refuses a file the browser would
/// load that writes JSX or imports the template dialect, which only the
/// server resolves there, naming the root that reaches it.
fn refuse_unrendered(set: &mut ComponentSet, rewritten: &[(String, String)], roots: &[String]) -> Result<(), BuildError> {
  let dialect = format!("\"{TEMPLATE_SPECIFIER}\"");
  for root in roots {
    for file in set.local_reach(root) {
      let reason = if set.writes_jsx(&file) {
        "writes JSX"
      } else if set.imports_value_from(&file, TEMPLATE_SPECIFIER) || rewritten.iter().any(|(f, text)| *f == file && text.contains(&dialect)) {
        "imports the template dialect"
      } else {
        continue;
      };
      return Err(BuildError::Unrendered { file, root: root.clone(), reason });
    }
  }
  Ok(())
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

/// `build`, then the typecheck over the build's own `generated/`, written
/// first so the types the checker reads are the ones this build produced.
/// `report` runs between the two, while the checker is still to come.
pub fn check(app: &Path, options: &Options, typecheck: &typecheck::Typecheck, report: impl FnOnce(&Built)) -> Result<(Built, Option<typecheck::Checked>), BuildError> {
  let built = build(app, options)?;
  write_generated(app, &built)?;
  types::refresh_embedded(app)?;
  report(&built);
  let checked = typecheck::run(app, typecheck)?;
  Ok((built, checked))
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
/// for the browser, under `.fsr/bundle`. `write` includes them; this is for a
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
/// The keys a `store` body returns and their types, inferred from the data its
/// loader returns; `None` when the return is not a record the build can read.
fn seeded(contract: &Contract, session: Option<&str>, consts: &Consts, config: &[(String, infer::Ts)], loader: &snapfire_fsr_ir::Body, store: &snapfire_fsr_ir::Body) -> Option<Vec<(String, infer::Ts)>> {
  let data = infer::Inferer { contract, session, input: None, input_type: None, consts, config, natives: &[] }.returns(loader);
  match (infer::Inferer { contract, session, input: None, input_type: Some(data), consts, config, natives: &[] }.returns(store)) {
    infer::Ts::Record(fields) => Some(fields.into_iter().collect()),
    _ => None,
  }
}

fn shell_contract(app: &Path, contract: &Contract, session: Option<&str>, sources: &[SourceEntry], consts: &Consts, config: &[(String, infer::Ts)], frameworks: &std::collections::BTreeMap<String, String>) -> Result<ShellContract, BuildError> {
  let mut store = std::collections::BTreeMap::new();
  for source in sources {
    let Some(loader) = &source.body else { continue };
    for body in [&source.store, &source.defaults].into_iter().flatten() {
      for (key, ty) in seeded(contract, session, consts, config, loader, body).unwrap_or_default() {
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
  Ok(ShellContract { version: SHELL_CONTRACT_VERSION, store, imports, frameworks: frameworks.clone(), fsr: env!("CARGO_PKG_VERSION").to_owned(), themes: Vec::new(), tokens: Vec::new() })
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

/** A script the page loads once per document, whether it arrives with the first paint or a navigation: now, once the main thread is idle or on the visitor's first interaction. Its origin is added to the policy's `script-src`. */
export function scriptTag(src: string, when?: "load" | "idle" | "interaction"): HeadEl {
  return { tag: "script", src, when };
}

/** An Open Graph property: `og("title", t)` is `<meta property="og:title">`. */
export function og(property: string, content: string): HeadEl {
  return { tag: "meta", property: `og:${property}`, content };
}

/** A Twitter card property: `twitter("card", c)` is `<meta name="twitter:card">`. */
export function twitter(name: string, content: string): HeadEl {
  return { tag: "meta", name: `twitter:${name}`, content };
}

/** A preload of the page's largest image, written into the head so the fetch starts before the stylesheet and the modules parse. `image` is an imported image; `sizes` is what the picture renders at. */
export function preloadImage(image: { src: string; sources?: { type: string; srcset: string }[] }, sizes: string): HeadEl {
  const first = image.sources != null && image.sources.length > 0 ? image.sources[0] : null;
  if (first == null) {
    return { tag: "link", rel: "preload", as: "image", href: image.src, fetchpriority: "high" };
  }
  return { tag: "link", rel: "preload", as: "image", type: first.type, imagesrcset: first.srcset, imagesizes: sizes, fetchpriority: "high" };
}

/** The canonical URL of this page. */
export function canonical(href: string): HeadEl {
  return { tag: "link", rel: "canonical", href };
}

/** When this page last changed, a W3C date such as `2026-06-01` or `2026-06-01T09:30:00Z`: `<meta property="article:modified_time">`, which a sitemap under `[crawl]` writes as the page's `lastmod`. */
export function modified(at: string): HeadEl {
  return { tag: "meta", property: "article:modified_time", content: at };
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
fn ctx_module(routes: &[Route], session_import: Option<&str>, config: &[(String, infer::Ts)], themes: &[String]) -> String {
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
    "}\n\nexport interface Address {\n  path: string;\n  params: Record<string, string>;\n  query: Record<string, string>;\n}\n\nexport interface Ctx<P extends keyof Routes = keyof Routes> {\n  params: Routes[P];\n  query: Record<string, string>;\n  /** The path this request matched, query excluded, locale prefix included. Empty under an action, whose path is the action endpoint rather than the document's. */\n  path: string;\n  session: Session;\n  identity: Identity | null;\n  locale: string;\n  /** The host the request named, when `[server] hosts` lists it; null when it does not, null whenever that key is unset. */\n  host: string | null;\n  /** `document.origin` for this deployment, the scheme and host it is reached over; null when the configuration names none. The same on every request, so a body reading it still prerenders. */\n  origin: string | null;\n  /** The navigation's own request when this render is an intercept, null otherwise. A layout the document keeps loads under the document's request, so `params`, `query` and `path` are the document's there and this is the navigation's. */\n  address: Address | null;\n  /** `[public]` from the configuration, one field per key, typed from the value written there. */\n  config: Config;\n  services: Services;\n  /** The application's own Rust, in this process. A method the build read as `fn` answers a value; an `async fn` answers a promise. */\n  native: Natives;\n  now: bigint;\n}\n\n/** What an action, a route handler or middleware can do to the session beyond reading and writing its keys. */\nexport interface SessionControl {\n  /** Moves the session's end to `seconds` from now once the body commits; the cookie and the store follow. Nothing extends on its own. */\n  extend(seconds: number): void;\n}\n\nexport interface ActionCtx<Input = void, P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  input: Input;\n  session: Session & SessionControl;\n}\n\n/** A `route.ts` handler's context: a loader's plus `session.extend`. */\nexport interface HandlerCtx<P extends keyof Routes = keyof Routes> extends Ctx<P> {\n  session: Session & SessionControl;\n}\n\nexport interface RequestLine {\n  method: string;\n  path: string;\n  /** Whether the request is a navigation's payload rather than a document; absent under a body test. */\n  payload?: boolean;\n}\n\nexport interface MiddlewareCtx extends Ctx {\n  request: RequestLine;\n  session: Session & SessionControl;\n}\n\nexport interface Meta {\n  title?: string;\n  /** `title` is written as given rather than through `[document] title_template`. */\n  absolute?: boolean;\n  description?: string;\n  head?: HeadEl[];\n}\n\nexport interface MetaCtx<Data> {\n  data: Data;\n}\n\nexport type DataOf<Load> = Load extends (...args: never[]) => Promise<infer Data> ? Data : never;\n\nexport interface MiddlewareResult {\n  redirect?: string;\n  rewrite?: string;\n  status?: number;\n  body?: unknown;\n  headers?: Record<string, string>;\n}\n\nexport function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): (ctx: ActionCtx<Input>) => Promise<Out> {\n  return declare<Input, Out>(body as never) as never;\n}\n",
  );
  let names: Vec<String> = themes.iter().map(|t| format!("{t:?}")).collect();
  let _ = writeln!(out, "\n/** A named theme: one per file under `styles/themes/` or the root theme as `default`. */\nexport type Theme = {};", if names.is_empty() { "never".to_owned() } else { names.join(" | ") });
  out.push_str("\n/** Light or dark, written as `<html data-sf-scheme>` and independent of the theme. */\nexport type Scheme = \"light\" | \"dark\";\n\n/** The theme the route's layouts and page set: the deepest one, unless a level above it forces its own. */\nexport interface RouteTheme {\n  name: Theme | null;\n  forced: boolean;\n}\n\nexport interface ThemeCtx extends Ctx {\n  theme: RouteTheme;\n}\n\n/** What `theme.ts` decides for the document: no `name` is the root theme, no `scheme` leaves the attribute off. */\nexport interface ThemeChoice {\n  name?: Theme | null;\n  scheme?: Scheme | null;\n}\n");
  out.push_str("\nexport type { HeadEl } from \"./head\";\n");
  out
}

/// `Upload` as TypeScript sees it. A schema names the type without importing
/// it, the way it names `Uint8Array`, so the declaration is global: the file
/// has no import and no export, which is what makes it one.
/// `generated/assets.d.ts`: what an import of an image, a font or a media file
/// is, which the bundle turns into a URL and, for an image, its dimensions.
const ASSET_DECLARATIONS: &str = r#"// Written by `fsr build`. An imported image is its URL and dimensions; an imported font or media file is its URL.
declare module "*.png" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.jpg" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.jpeg" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.gif" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.webp" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.avif" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.svg" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.ico" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.bmp" { const asset: import("@snapfire/fsr-authoring/template").ImageAsset; export default asset; }
declare module "*.woff2" { const url: string; export default url; }
declare module "*.woff" { const url: string; export default url; }
declare module "*.ttf" { const url: string; export default url; }
declare module "*.otf" { const url: string; export default url; }
declare module "*.mp4" { const url: string; export default url; }
declare module "*.m4v" { const url: string; export default url; }
declare module "*.webm" { const url: string; export default url; }
declare module "*.ogv" { const url: string; export default url; }
declare module "*.mov" { const url: string; export default url; }
declare module "*.mp3" { const url: string; export default url; }
declare module "*.m4a" { const url: string; export default url; }
declare module "*.aac" { const url: string; export default url; }
declare module "*.oga" { const url: string; export default url; }
declare module "*.ogg" { const url: string; export default url; }
declare module "*.opus" { const url: string; export default url; }
declare module "*.wav" { const url: string; export default url; }
declare module "*.vtt" { const url: string; export default url; }
"#;

const UPLOAD_DECLARATION: &str = r#"// Generated by fsr build. Do not edit.

/** One file part posted to an action as `multipart/form-data`. The host builds it from the part: a schema declares a field as `Upload` and the body receives the name the browser sent, the type it claimed, the length and the bytes. */
interface Upload {
  filename: string;
  content_type: string;
  size: bigint;
  bytes: Uint8Array;
}
"#;

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
fn client_module(contract: &Contract, session: Option<&str>, routes: &[Route], layouts: &[String], images: &[String], sources: &[SourceEntry], actions: &[ActionEntry], prefix: &str, consts: &Consts, config: &[(String, infer::Ts)], natives: &native::Natives) -> String {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  out.push_str("import { action as call } from \"@snapfire/fsr-client\";\n\n");
  out.push_str(&typescript::type_declarations(contract, Flavour::Client));

  for route in routes {
    let props = match sources.iter().find(|s| s.id == route.id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None, input_type: None, consts, config, natives: &natives.modules }.returns(body).print(Flavour::Client),
      None => "{}".to_owned(),
    };
    let _ = writeln!(out, "export type {} = {props};", props_name(&route.id));
  }
  for id in layouts.iter().chain(images) {
    let props = match sources.iter().find(|s| s.id == *id).and_then(|s| s.body.as_ref()) {
      Some(body) => infer::Inferer { contract, session, input: None, input_type: None, consts, config, natives: &natives.modules }.returns(body).print(Flavour::Client),
      None => "{}".to_owned(),
    };
    let _ = writeln!(out, "export type {} = {props};", props_name(id));
  }
  if !routes.is_empty() || !layouts.is_empty() || !images.is_empty() {
    out.push('\n');
  }

  let mut tree: Vec<(Vec<String>, String)> = Vec::new();
  for action in actions {
    let Some(body) = &action.body else { continue };
    let returns = infer::Inferer { contract, session, input: action.input.as_deref(), input_type: None, consts, config, natives: &natives.modules }.returns(body).print(Flavour::Client);
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
  let named: Vec<&str> = natives.types.iter().map(|t| t.name.as_str()).filter(|name| !contract.types.contains_key(*name) && names_type(&out, name)).collect();
  if !named.is_empty() {
    let first = "import { action as call } from \"@snapfire/fsr-client\";\n";
    let at = out.find(first).map_or(0, |i| i + first.len());
    out.insert_str(at, &format!("import type {{ {} }} from \"./native\";\n", named.join(", ")));
  }
  out
}

/// Whether `text` mentions `name` as a whole identifier.
fn names_type(text: &str, name: &str) -> bool {
  let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
  text.match_indices(name).any(|(at, _)| !text[..at].ends_with(ident) && !text[at + name.len()..].starts_with(ident))
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
    let _ = writeln!(out, "{indent}{}: {{", object_key(&group));
    let mut next = prefix.to_vec();
    next.push(group);
    write_action_tree(out, entries, &next, depth + 1);
    let _ = writeln!(out, "{indent}}},");
  }
  for (path, value) in entries {
    if path.len() == prefix.len() + 1 && path[..prefix.len()] == *prefix {
      let _ = writeln!(out, "{indent}{}: {value},", object_key(&path[prefix.len()]));
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
  let mut answered: std::collections::HashMap<String, &Path> = std::collections::HashMap::new();
  for route in routes.iter().chain(handler_routes) {
    if let Some(first) = answered.insert(matched_shape(&route.pattern), &route.dir) {
      return Err(BuildError::ClaimedPattern { pattern: route.pattern.clone(), first: first.display().to_string(), second: route.dir.display().to_string() });
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

/// Refuses a file under the public root whose URL a route, a generated image,
/// `[crawl]`'s `robots.txt` or sitemap or the root icon also answers. A
/// pattern with a parameter is not compared: the file wins it in every mode.
fn public_claimed<'a>(app: &Path, patterns: impl Iterator<Item = &'a str>) -> Result<(), BuildError> {
  let Some(config) = config_beside(app) else { return Ok(()) };
  let Some(root) = config.statics.iter().find(|s| s.route.trim_end_matches('/').is_empty()) else { return Ok(()) };
  let dir = config.resolve(&root.dir);
  let mut answered: Vec<(String, String)> = patterns.filter(|p| !p.contains('{')).map(|p| (p.to_owned(), "a route".to_owned())).collect();
  if let Some(crawl) = &config.crawl {
    answered.extend(crawl.robots.iter().map(|p| (p.clone(), "[crawl] robots".to_owned())));
    answered.extend(crawl.sitemap.iter().map(|p| (p.clone(), "[crawl] sitemap".to_owned())));
  }
  if let Some(icon) = &config.favicon {
    answered.push(("/favicon.ico".to_owned(), format!("the icons root's {icon}")));
  }
  let mut stack = vec![dir.clone()];
  while let Some(at) = stack.pop() {
    let Ok(entries) = std::fs::read_dir(&at) else { continue };
    for entry in entries.filter_map(Result::ok) {
      let path = entry.path();
      if path.is_dir() {
        stack.push(path);
        continue;
      }
      let rel = path.strip_prefix(&dir).unwrap_or(&path).to_string_lossy().replace('\\', "/");
      let url = format!("/{rel}");
      if let Some((_, by)) = answered.iter().find(|(p, _)| p.trim_end_matches('/') == url) {
        return Err(BuildError::PublicClaimed { file: format!("{}/{rel}", root.dir.trim_end_matches('/')), path: url, by: by.clone() });
      }
    }
  }
  Ok(())
}

/// A pattern with its parameters' names left out, which is what two patterns
/// match the same URLs by.
fn matched_shape(pattern: &str) -> String {
  pattern
    .split('/')
    .map(|segment| match segment.strip_prefix('{') {
      Some(param) if param.starts_with('*') => "{*}",
      Some(_) => "{}",
      None => segment,
    })
    .collect::<Vec<_>>()
    .join("/")
}

/// `$root` is `RootProps`, `product.$id` is `ProductIdProps`, `admin.users` is
/// `AdminUsersProps`, `(shop).cart` is `ShopCartProps`. A parameter's `$`
/// marker and a group's parentheses are not part of the name. A name that
/// would start with a digit, `030-intro`, starts with `Route` instead, since a
/// TypeScript identifier cannot: `Route030IntroProps`.
fn props_name(id: &str) -> String {
  let mut name = String::new();
  for part in id.split(['.', '-', '_']) {
    let mut chars = part.trim_start_matches(['$', '(']).trim_end_matches(')').chars();
    if let Some(first) = chars.next() {
      name.extend(first.to_uppercase());
      name.push_str(chars.as_str());
    }
  }
  if name.starts_with(|c: char| c.is_ascii_digit()) {
    name.insert_str(0, "Route");
  }
  name + "Props"
}

/// An object key as written: bare when it is an identifier, quoted otherwise,
/// so `sign-up` and `030-intro` are `"sign-up"` and `"030-intro"`.
fn object_key(name: &str) -> String {
  let identifier = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_' || c == '$') && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
  match identifier {
    true => name.to_owned(),
    false => serde_json::to_string(name).expect("a string serializes"),
  }
}

/// Every module a component places as an island, in tree order.
/// Every component a template renders inline, an island's children included.
fn inline_components(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<String> {
  use snapfire_fsr_ir::Tmpl;
  fn walk(tmpl: &Tmpl, out: &mut Vec<String>) {
    match tmpl {
      Tmpl::Component { module, children, .. } => {
        if !out.contains(module) {
          out.push(module.clone());
        }
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Baked { children, .. } | Tmpl::Island { children, .. } | Tmpl::Element { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
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

/// One `<Island>` a template places.
struct Placement {
  module: String,
  /// Placed with `define`: a custom element the browser imports.
  define: bool,
  /// Placed with `mode="server"`: the server renders every step and no adapter mounts it.
  server: bool,
}

fn island_modules(tmpl: &snapfire_fsr_ir::Tmpl) -> Vec<Placement> {
  use snapfire_fsr_ir::Tmpl;
  let mut out = Vec::new();
  fn walk(tmpl: &Tmpl, out: &mut Vec<Placement>) {
    match tmpl {
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| walk(c, out)),
      Tmpl::Island { module, children, define, mode, .. } => {
        out.push(Placement { module: module.clone(), define: *define, server: mode.as_deref() == Some(snapfire_fsr_ir::render::SERVER_MODE) });
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

/// Every file an island reaches with the dialect's placements pointed at the
/// island's adapter, by path in the bundle overlay.
fn dialect_rewrites(app: &Path, set: &mut ComponentSet, islands: &[String], static_modules: &[String], defines: &[String], owners: &HashMap<String, Owner>, rewritten: &[(String, String)]) -> Result<Vec<(String, String)>, BuildError> {
  let dialect = format!("\"{TEMPLATE_SPECIFIER}\"");
  let mut out: Vec<(String, String)> = Vec::new();
  let mut seen: Vec<String> = Vec::new();
  for module in islands.iter().filter(|m| !static_modules.contains(m) && !defines.contains(m)) {
    let Some(file) = module.split_once('#').map(|(file, _)| file) else { continue };
    if snapfire_fsr_lower::component::is_foreign(file) {
      continue;
    }
    let island = adapter_for(module, owners)?;
    for reached in set.local_reach(file) {
      if seen.contains(&reached) {
        continue;
      }
      seen.push(reached.clone());
      let source = match rewritten.iter().find(|(f, _)| *f == reached) {
        Some((_, text)) => text.clone(),
        None => {
          let path = app.join(&reached);
          std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path, e))?
        }
      };
      if source.contains(&dialect) {
        out.push((reached, source.replace(&dialect, &format!("\"{}\"", island.module))));
      }
    }
  }
  Ok(out)
}

/// Every JSX file under `app` that imports a component another framework
/// mounts, with each such import replaced by a `foreign` component of React's
/// adapter, by path in the bundle overlay. React's `jsx` takes no other
/// framework's component, so `<Chart />` from `Chart.vue` places it by id.
fn foreign_rewrites(app: &Path, set: &mut ComponentSet, rewritten: &[(String, String)]) -> Result<Vec<(String, String)>, BuildError> {
  let mut files = Vec::new();
  jsx_files(app, app, &mut files)?;
  let mut out = Vec::new();
  for file in files {
    let imports = set.foreign_imports(&file);
    if imports.is_empty() {
      continue;
    }
    let mut source = match rewritten.iter().find(|(f, _)| *f == file) {
      Some((_, text)) => text.clone(),
      None => {
        let path = app.join(&file);
        std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path, e))?
      }
    };
    for import in imports {
      let consts: String = import.bindings.iter().map(|(local, module)| format!("const {local} = __sfForeign({module:?});")).collect::<Vec<_>>().join(" ");
      source = source.replacen(&import.text, &consts, 1);
    }
    out.push((file, format!("import {{ foreign as __sfForeign }} from \"{}\";\n{source}", REACT.module)));
  }
  Ok(out)
}

/// The `.tsx` and `.jsx` files under `dir`, relative to `app`, leaving out
/// what the build and the vendor step write.
fn jsx_files(app: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), BuildError> {
  let mut entries: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.flatten().map(|e| e.path()).collect();
  entries.sort();
  for path in entries {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if path.is_dir() {
      if name.starts_with('.') || (path.parent() == Some(app) && ["generated", "dist", "types", "vendor", "node_modules"].contains(&name.as_str())) {
        continue;
      }
      jsx_files(app, &path, out)?;
    } else if name.ends_with(".tsx") || name.ends_with(".jsx") {
      if let Ok(relative) = path.strip_prefix(app) {
        out.push(relative.to_string_lossy().replace('\\', "/"));
      }
    }
  }
  Ok(())
}

/// A client module that mounts one framework's components: the three exports
/// the registry names and the bare specifiers the module imports, which the
/// page's import map has to supply.
pub(crate) struct Adapter {
  pub(crate) module: &'static str,
  mounter: &'static str,
  patcher: &'static str,
  unmounter: &'static str,
  /// Hides a mounted island in place while activity holds its page, where the framework can.
  pauser: Option<&'static str>,
  needs: &'static [&'static str],
}

pub(crate) const REACT: Adapter = Adapter { module: "@snapfire/fsr-client/react", mounter: "reactMounter", patcher: "reactPatcher", unmounter: "reactUnmounter", pauser: Some("reactPauser"), needs: &["react", "react-dom/client"] };

pub(crate) const VUE: Adapter = Adapter { module: "@snapfire/fsr-client/vue", mounter: "vueMounter", patcher: "vuePatcher", unmounter: "vueUnmounter", pauser: None, needs: &["vue"] };

pub(crate) const SVELTE: Adapter = Adapter { module: "@snapfire/fsr-client/svelte", mounter: "svelteMounter", patcher: "sveltePatcher", unmounter: "svelteUnmounter", pauser: None, needs: &["svelte", "svelte/internal/client"] };

const ADAPTERS: &[&Adapter] = &[&REACT, &VUE, &SVELTE];

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
    "svelte" => crate::direction::SVELTE,
    _ => "<version>",
  }
}

/// The adapter a module is registered with. A source the lowerer reads is
/// React's; a foreign one belongs to the framework whose plugin claims its
/// extension, when the client has an adapter for that framework.
fn adapter_for(module: &str, owners: &HashMap<String, Owner>) -> Result<&'static Adapter, BuildError> {
  match owners.get(module) {
    Some(Owner::Vue) => return Ok(&VUE),
    Some(Owner::React | Owner::Fsr) => return Ok(&REACT),
    None => {}
  }
  let file = module.split_once('#').map(|(file, _)| file).unwrap_or(module);
  if !snapfire_fsr_lower::component::is_foreign(file) {
    return Ok(&REACT);
  }
  let ext = Path::new(file).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
  match snapfire_compiler_wire::claimed(&ext) {
    Some("vue") => Ok(&VUE),
    Some("svelte") => Ok(&SVELTE),
    Some(ext) => Err(BuildError::NoAdapter { module: module.to_owned(), ext: ext.to_owned() }),
    None => Err(BuildError::UnknownComponent { module: module.to_owned() }),
  }
}

/// `generated/islands.ts`: one `registerIsland` per module discovery named
/// and per component a page or layout places as an island, so the browser
/// mounts exactly what the plan file refers to. A module nothing mounts is
/// left out and a mounter nothing registers is never imported, which is what
/// keeps a framework off a page that has no component of it.
fn islands_module(islands: &[String], static_modules: &[String], defines: &[String], owners: &HashMap<String, Owner>, generated: &[String], exported: &[String], modules: &[String], options: &Options) -> Result<String, BuildError> {
  let mut out = String::from("// Generated by fsr build. Do not edit.\n\n");
  let registered: Vec<&String> = islands.iter().filter(|m| !static_modules.contains(m)).collect();
  let defining = registered.iter().any(|m| defines.contains(m));
  let mut client_names = vec!["registerIsland"];
  if defining {
    client_names.insert(0, "defineMounter");
  }
  if !modules.is_empty() {
    client_names.push("registerModule");
  }
  let _ = writeln!(out, "import {{ {} }} from \"@snapfire/fsr-client\";", client_names.join(", "));
  let mut imported: Vec<(&'static str, Vec<&'static str>)> = Vec::new();
  for module in registered.iter().filter(|m| !defines.contains(**m)) {
    let adapter = adapter_for(module, owners)?;
    let names = match imported.iter_mut().find(|(seen, _)| *seen == adapter.module) {
      Some((_, names)) => names,
      None => {
        imported.push((adapter.module, Vec::new()));
        &mut imported.last_mut().unwrap().1
      }
    };
    for name in [adapter.mounter, adapter.patcher, adapter.unmounter].into_iter().chain(adapter.pauser) {
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
    let adapter = adapter_for(module, owners)?;
    if generated.iter().any(|file| file == path) {
      out.push_str("  // @ts-ignore: the bundle overlay alone holds this module, which the build split out of its page\n");
    } else if snapfire_fsr_lower::component::is_transformed(path) {
      out.push_str("  // @ts-ignore: the bundle overlay holds the module the build wrote from this file\n");
    } else if exported.contains(module) {
      out.push_str("  // @ts-ignore: the bundle overlay exports this component, which its file declares without exporting\n");
    }
    let pause = adapter.pauser.map(|name| format!(", pause: {name}")).unwrap_or_default();
    let _ = writeln!(out, "  registerIsland(\"{prefix}{module}\", {{ loader: () => import(\"../{js}\").then((m) => m.{export}), mount: {}, patch: {}, unmount: {}{pause} }});", adapter.mounter, adapter.patcher, adapter.unmounter);
  }
  // A module the browser imports a function prop from, by the file's own path: a function prop's module id carries no site prefix.
  for file in modules {
    let js = match snapfire_fsr_lower::component::is_foreign(file) {
      true => file.clone(),
      false => file.rsplit_once('.').map(|(stem, _)| format!("{stem}.js")).unwrap_or_else(|| format!("{file}.js")),
    };
    out.push_str("  // @ts-ignore: the bundle overlay exports what the build hoisted out of this file\n");
    let _ = writeln!(out, "  registerModule(\"{file}\", () => import(\"../{js}\"));");
  }
  out.push_str("}\n");
  Ok(out)
}

/// Refuses a registry the page's import map cannot supply: each adapter the
/// registry imports and the specifiers that adapter imports, looked up in the
/// app's map and, for a site, the shell's. An app with no readable map has
/// nothing to check against.
fn check_island_imports(app: &Path, layout: &crate::xwpm::Layout, shell: Option<&ShellContract>, islands: &[String], static_modules: &[String], defines: &[String], owners: &HashMap<String, Owner>) -> Result<(), BuildError> {
  let Some(served) = served_specifiers(app, layout, shell) else {
    return Ok(());
  };
  let mut checked: Vec<&str> = Vec::new();
  for module in islands.iter().filter(|m| !static_modules.contains(m) && !defines.contains(m)) {
    let adapter = adapter_for(module, owners)?;
    let remedy = || crate::direction::for_adapter(adapter.module).map(|d| format!("; `fsr use <app dir> {}` writes it", d.name)).unwrap_or_default();
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

/// The dialect's template module, which an island's bundle copy imports from its own adapter instead.
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
/// The Svelte majors the client adapter mounts.
const SVELTE_MAJORS: &[u64] = &[5];

fn svelte_major_supported(version: &str) -> bool {
  types::range::Version::parse(version).is_some_and(|v| SVELTE_MAJORS.contains(&v.major))
}

/// Svelte's compiler inside its package.
const SVELTE_COMPILER: &str = "compiler/index.js";

/// Per plugin extension, the compiler of the version the application vendors,
/// which the plugin boots in place of the one it carries so the output calls
/// the runtime the application serves. Fetched into `.fsr/compilers` the first
/// time and read from there after; the compiler is never vendored or shipped.
pub(crate) fn plugin_compilers(app: &Path, layout: &crate::xwpm::Layout) -> Result<Vec<(&'static str, PathBuf)>, BuildError> {
  let manifest = crate::vendor::VendorManifest::read(app, layout)?;
  let Some(record) = manifest.packages.get("svelte") else {
    return Ok(Vec::new());
  };
  Ok(vec![("svelte", crate::published::compiler(app, "svelte", &record.version, SVELTE_COMPILER)?)])
}

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
      Some(version) if package == "svelte" && !svelte_major_supported(&version) => {
        let majors: Vec<String> = SVELTE_MAJORS.iter().map(u64::to_string).collect();
        return Err(BuildError::SvelteMajor { version, supported: majors_text(&majors) });
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
fn describe_foreign(app: &Path, set: &mut ComponentSet, report: &mut Report, plugins: &mut plugins::Plugins) -> Result<(), BuildError> {
  use snapfire_compiler_wire::{Outcome, Unit};
  let mut by_ext: std::collections::BTreeMap<&'static str, Vec<PathBuf>> = std::collections::BTreeMap::new();
  for dir in types::source_dirs(app) {
    collect_plugin_files(&app.join(dir), &mut by_ext);
  }
  // A missing compiler is refused with the vendored frameworks; here it only
  // decides which compiler reads a component.
  let compilers = crate::xwpm::Layout::of(app).ok().and_then(|layout| plugin_compilers(app, &layout).ok()).unwrap_or_default();
  for (ext, mut files) in by_ext {
    files.sort();
    let options = snapfire_compiler_wire::Options { compiler: compilers.iter().find(|(e, _)| *e == ext).map(|(_, path)| path.to_string_lossy().into_owned()), ..Default::default() };
    let mut units = Vec::new();
    for path in &files {
      let filename = path.strip_prefix(app).unwrap_or(path).to_string_lossy().replace('\\', "/");
      let source = std::fs::read_to_string(path).map_err(|e| BuildError::Io(path.clone(), e))?;
      units.push(Unit { filename, path: path.to_string_lossy().into_owned(), source, options: options.clone(), files: Default::default() });
    }
    for round in 0..2 {
      let outcomes = match plugins.describe(ext, units.clone()) {
        Ok(outcomes) => outcomes,
        Err(plugins::Refusal::Missing { binary, hint }) => {
          report.plugins.push(format!("`{binary}` is not on PATH, so no `.{ext}` component was read or compiled; `{hint}` puts it there, and until it does the bundle stops on the module the registry imports"));
          break;
        }
        Err(plugins::Refusal::Failed(why)) => return Err(BuildError::Plugin(why)),
      };
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
              set.undescribed(unit.filename, format!("{} asked for {} again after being given it", plugins.name(ext), files.join(", ")));
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
          Outcome::Ok(_) => return Err(BuildError::Plugin(format!("{} compiled `{}` where it was asked to describe it", plugins.name(ext), unit.filename))),
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

  fn node(&self, children: Vec<Child>, keep: Vec<String>, error: &Boundary) -> Node {
    Node {
      id: 0,
      module: self.module.clone(),
      source: self.source.clone(),
      deferred: false,
      fallback: None,
      error: error.module.clone(),
      error_kinds: error.kinds.clone(),
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
  error: Boundary,
}

impl SlotInfo {
  fn child(&self, error: &Boundary) -> Child {
    let boundary = self.error.or(error);
    Child {
      slot: self.name.clone(),
      node: Node {
        id: 0,
        module: self.page.clone(),
        source: self.source.clone(),
        deferred: self.loading.is_some(),
        fallback: self.loading.clone(),
        error: boundary.module,
        error_kinds: boundary.kinds,
        cache_key: Some(self.page.clone()),
        children: Vec::new(),
        keep: Vec::new(),
      },
    }
  }
}

/// Nests `content` under each layout, outermost first, each layout's
/// parallel slots beside it. Ids are assigned afterwards by `renumber`.
/// Where the named themes live under the styles directory.
pub const THEMES_DIR: &str = "themes";

/// The attribute on `<html>` a named theme is selected by.
pub const THEME_ATTRIBUTE: &str = "data-sf-theme";

/// The name the root theme answers to once an app has named themes, so a part of a page can take
/// the root's values back inside another theme.
pub const ROOT_THEME: &str = "default";

/// The names of the themes in `dir` (one per `<name>.css`) with [`ROOT_THEME`] when there is one.
/// A `default.css` is refused.
pub fn theme_files(dir: &Path) -> Result<Vec<String>, BuildError> {
  let mut names: Vec<String> = std::fs::read_dir(dir)
    .into_iter()
    .flatten()
    .flatten()
    .map(|e| e.path())
    .filter(|p| p.extension().is_some_and(|e| e == "css"))
    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
    .collect();
  if names.iter().any(|n| n == ROOT_THEME) {
    return Err(BuildError::ReservedTheme { file: dir.join(format!("{ROOT_THEME}.css")).display().to_string() });
  }
  if !names.is_empty() {
    names.push(ROOT_THEME.to_owned());
  }
  names.sort();
  Ok(names)
}

/// Every module a plan node and its children render, the node's own first.
fn plan_modules(node: &Node, out: &mut Vec<String>) {
  if !out.contains(&node.module) {
    out.push(node.module.clone());
  }
  for child in &node.children {
    plan_modules(&child.node, out);
  }
}

fn wrap_in_layouts(content: Node, wrapping: &[&LayoutInfo], error: &Boundary) -> Node {
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
fn intercept_plan(variant: Node, slot: &str, wrapping: &[&LayoutInfo], error: &Boundary) -> Node {
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
    error_kinds: Vec::new(),
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
/// What `[build] strict` refuses in `report`: each cause that leaves a component to render in the browser alone, with the components it leaves there.
fn strict_refusal(report: &Report) -> Option<String> {
  if report.causes.is_empty() && report.foreign.is_empty() && report.plugins.is_empty() {
    return None;
  }
  let mut out = String::from("these components render in the browser alone, which `[build] strict` refuses; make each lower or drop `strict`");
  for (kind, causes) in [("client", &report.causes), ("foreign", &report.foreign)] {
    for cause in causes {
      out.push_str(&format!("\n  {kind:<8} {}: {}", cause.at, cause.message));
      if let Some(hint) = &cause.hint {
        out.push_str(&format!("\n           {hint}"));
      }
      for (module, _) in &cause.pages {
        out.push_str(&format!("\n             {module}"));
      }
    }
  }
  for plugin in &report.plugins {
    out.push_str(&format!("\n  plugins  {plugin}"));
  }
  Some(out)
}

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

/// The line and reason left on every element whose handler did not lower and
/// on every placement handed a function that did not, in document order.
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
      Tmpl::Component { props, children, .. } => {
        out.extend(props.iter().filter_map(|e| match e {
          Entry::Field(n, Expr::Lit(Lit::Str(why))) if n == snapfire_fsr_ir::render::UNLOWERED_ATTR => Some(why.clone()),
          _ => None,
        }));
        children.iter().for_each(|c| walk(c, out));
      }
      Tmpl::Baked { children, .. } | Tmpl::Island { children, .. } | Tmpl::Fragment(children) => children.iter().for_each(|c| walk(c, out)),
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

/// The image files a crawler's card reads beside a route, by stem: Open Graph's and Twitter's.
const SOCIAL_IMAGES: [(&str, &str); 2] = [("opengraph-image", "og"), ("twitter-image", "twitter")];

/// What a social card's crawler decodes.
const SOCIAL_IMAGE_TYPES: [(&str, &str); 5] = [("png", "image/png"), ("jpg", "image/jpeg"), ("jpeg", "image/jpeg"), ("gif", "image/gif"), ("webp", "image/webp")];

/// Whether a card image file is a component that draws it rather than the image itself.
fn is_generated_image(path: &Path) -> bool {
  path.extension().is_some_and(|e| e == "tsx" || e == "ts")
}

/// The size a generated card image is drawn at when its module exports none,
/// the one Open Graph recommends.
const GENERATED_IMAGE_SIZE: (u32, u32) = (1200, 630);

/// One generated card image file, lowered once whatever number of routes it covers.
struct DrawnImage {
  module: String,
  source: Option<String>,
  width: u32,
  height: u32,
  content_type: String,
  ext: &'static str,
  hash: String,
  alt: Option<String>,
  images: Option<snapfire_fsr_ir::Body>,
}

fn draw_image(app: &Path, routes_dir: &Path, dir: &Path, file: &Path, stem: &str, set: &mut ComponentSet, sources: &mut Vec<SourceEntry>) -> Result<DrawnImage, BuildError> {
  let refuse = |why: String| BuildError::SocialImage(file.to_path_buf(), why);
  let rel = file.strip_prefix(app).unwrap_or(file).to_string_lossy().replace('\\', "/");
  let module = format!("{rel}#default");
  set.lower(&module).map_err(|e| refuse(format!("it draws the image on the server and must lower: {e}")))?;
  let Some((_, component)) = set.components.iter().find(|(m, _)| *m == module) else {
    return Err(refuse("it draws the image on the server and must lower".to_owned()));
  };
  if component.owner.hydrates() {
    return Err(refuse("it holds state or a handler, and an image is drawn once with nothing to run in a browser".to_owned()));
  }
  if !island_modules(&component.render).is_empty() {
    return Err(refuse("it places an island, and an image has no browser to mount one in".to_owned()));
  }
  let hash = snapfire_fsr_assets::hash::of(format!("{component:?}").as_bytes());
  let exports = set.image_exports(&rel).map_err(|e| refuse(e.to_string()))?;
  let (width, height) = match (exports.width, exports.height) {
    (Some(w), Some(h)) => (w, h),
    _ => GENERATED_IMAGE_SIZE,
  };
  if width > snapfire_fsr_ogimage::MAX_SIDE || height > snapfire_fsr_ogimage::MAX_SIDE {
    return Err(refuse(format!("`size` is {width}x{height}; each side is at most {}", snapfire_fsr_ogimage::MAX_SIDE)));
  }
  let content_type = exports.content_type.unwrap_or_else(|| "image/png".to_owned());
  let ext = match content_type.as_str() {
    "image/png" => "png",
    "image/jpeg" => "jpg",
    other => return Err(refuse(format!("`contentType` is `{other}`; a generated image is `image/png` or `image/jpeg`"))),
  };
  let alt_file = dir.join(format!("{stem}.alt.txt"));
  let alt = match exports.alt {
    Some(alt) => Some(alt),
    None if alt_file.is_file() => Some(std::fs::read_to_string(&alt_file).map_err(|e| BuildError::Io(alt_file.clone(), e))?.trim().to_owned()),
    None => None,
  };
  let images = set.lower_images(&rel).map_err(|e| refuse(e.to_string()))?;
  let loader = dir.join(format!("{stem}.loader.ts"));
  let source = if loader.is_file() {
    let loader_rel = loader.strip_prefix(app).unwrap_or(&loader).to_string_lossy().replace('\\', "/");
    let body = set.lower_loader(&loader_rel).map_err(|e| BuildError::SocialImage(loader.clone(), format!("an image's loader must lower: {e}")))?;
    let (_, dir_id) = pattern_of(routes_dir, dir)?;
    let id = if dir_id.is_empty() { stem.to_owned() } else { format!("{dir_id}.{stem}") };
    sources.push(SourceEntry::lowered(id.clone(), loader_rel, body));
    Some(id)
  } else {
    None
  };
  Ok(DrawnImage { module, source, width, height, content_type, ext, hash, alt, images })
}

/// Lowers every `opengraph-image.tsx` and `twitter-image.tsx` under
/// `routes/` into a component and writes, for each page it covers, the image
/// row the host answers its URL from and, unless the module lists its images,
/// the head rows that name it on the page. A page takes the nearest image at
/// or above its directory; a static image nearer than a component wins. The
/// component's props are its own loader's data when `<stem>.loader.ts` sits
/// beside it, else the data of the page or layout beside it, with `params`.
/// A page under the image's directory gets the same: the page's loader there
/// runs for the params the two share, and the layout's when no page has one.
fn generated_images(app: &Path, routes_dir: &Path, routes: &[Route], set: &mut ComponentSet, sources: &mut Vec<SourceEntry>) -> Result<(Vec<snapfire_fsr_plan::ImageEntry>, Vec<snapfire_fsr_plan::ModuleHead>), BuildError> {
  let mut drawn: HashMap<PathBuf, DrawnImage> = HashMap::new();
  let mut images = Vec::new();
  let mut heads = Vec::new();
  for route in routes {
    let Some(page) = page_file(&route.dir)? else { continue };
    let rel_dir = route.dir.strip_prefix(app).unwrap_or(&route.dir).to_string_lossy().replace('\\', "/");
    let page_module = format!("{rel_dir}/{page}#default");
    let mut head = Vec::new();
    for (stem, prefix) in SOCIAL_IMAGES {
      let mut found = None;
      for dir in route.dir.ancestors() {
        if !dir.starts_with(routes_dir) {
          break;
        }
        if let Some(file) = ["tsx", "ts"].iter().map(|ext| dir.join(format!("{stem}.{ext}"))).find(|f| f.is_file()) {
          found = Some((dir.to_path_buf(), file));
          break;
        }
        if SOCIAL_IMAGE_TYPES.iter().any(|(ext, _)| dir.join(format!("{stem}.{ext}")).is_file()) {
          break;
        }
      }
      let Some((dir, file)) = found else { continue };
      if !drawn.contains_key(&file) {
        let image = draw_image(app, routes_dir, &dir, &file, stem, set, sources)?;
        drawn.insert(file.clone(), image);
      }
      let image = &drawn[&file];
      let source = match &image.source {
        Some(own) => Some(own.clone()),
        None if dir == route.dir => sources.iter().find(|s| s.id == route.id).map(|s| s.id.clone()),
        None => {
          let (_, dir_id) = pattern_of(routes_dir, &dir)?;
          let layout = if dir == routes_dir { "layout".to_owned() } else { format!("{dir_id}.layout") };
          let beside = routes.iter().find(|r| r.dir == dir).map(|r| r.id.clone());
          sources.iter().find(|s| Some(&s.id) == beside.as_ref()).or_else(|| sources.iter().find(|s| s.id == layout)).map(|s| s.id.clone())
        }
      };
      let base = if route.pattern == "/" { String::new() } else { route.pattern.clone() };
      let pattern = match image.images {
        Some(_) => format!("{base}/{stem}/{{{}}}", snapfire_fsr_plan::IMAGE_PARAM),
        None => format!("{base}/{stem}.{}", image.ext),
      };
      if image.images.is_none() {
        let (key, field) = if prefix == "og" { ("property", "og:image") } else { ("name", "twitter:image") };
        let row = |name: String, content: String| snapfire_fsr_plan::HeadRow { tag: "meta".to_owned(), attrs: vec![(key.to_owned(), name), ("content".to_owned(), content)] };
        head.push(row(field.to_owned(), format!("{}/{stem}.{}?v={}", snapfire_fsr_runtime::PATH_MARK, image.ext, image.hash)));
        head.push(row(format!("{field}:width"), image.width.to_string()));
        head.push(row(format!("{field}:height"), image.height.to_string()));
        head.push(row(format!("{field}:type"), image.content_type.clone()));
        if let Some(alt) = &image.alt {
          head.push(row(format!("{field}:alt"), alt.clone()));
        }
      }
      images.push(snapfire_fsr_plan::ImageEntry {
        pattern,
        module: image.module.clone(),
        source,
        width: image.width,
        height: image.height,
        content_type: image.content_type.clone(),
        page: page_module.clone(),
        card: prefix.to_owned(),
        hash: image.hash.clone(),
        alt: image.alt.clone(),
        images: image.images.clone(),
      });
    }
    if !head.is_empty() {
      heads.push(snapfire_fsr_plan::ModuleHead { module: page_module, head });
    }
  }
  Ok((images, heads))
}

/// The head rows of every `opengraph-image.<ext>` and `twitter-image.<ext>`
/// under `routes/`, covering every route under its directory, a nearer image
/// replacing them. They go on the module of the directory's layout, which
/// every route beneath it renders, or without one on each page beneath it
/// that no nearer image covers. `<stem>.alt.txt` beside one is its alt text.
/// Each image is served as its hashed original.
fn social_images(app: &Path, routes: &Path, resolver: &assets::Resolver) -> Result<Vec<snapfire_fsr_plan::ModuleHead>, BuildError> {
  let mut found = Vec::new();
  image_dirs(app, routes, resolver, &mut found)?;
  let module = |dir: &Path, file: String| format!("{}/{file}#default", dir.strip_prefix(app).unwrap_or(dir).to_string_lossy().replace('\\', "/"));
  let mut out = Vec::new();
  for (dir, head) in &found {
    if let Some(file) = layout_file(dir)? {
      out.push(snapfire_fsr_plan::ModuleHead { module: module(dir, file), head: head.clone() });
      continue;
    }
    let mut pages = Vec::new();
    pages_under(dir, &mut pages)?;
    let covered: Vec<&PathBuf> = pages.iter().filter(|page| !found.iter().any(|(other, _)| other != dir && other.starts_with(dir) && page.starts_with(other))).collect();
    if covered.is_empty() {
      return Err(BuildError::SocialImage(dir.clone(), "a card image covers the routes under its directory, and there is none".to_owned()));
    }
    for page in covered {
      let file = page_file(page)?.expect("a page directory holds a page");
      out.push(snapfire_fsr_plan::ModuleHead { module: module(page, file), head: head.clone() });
    }
  }
  Ok(out)
}

/// Every directory under `dir` holding a card image, with the rows it writes.
fn image_dirs(app: &Path, dir: &Path, resolver: &assets::Resolver, out: &mut Vec<(PathBuf, Vec<snapfire_fsr_plan::HeadRow>)>) -> Result<(), BuildError> {
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.filter_map(|e| e.ok().map(|e| e.path())).collect();
  children.sort();
  let mut head = Vec::new();
  for (stem, prefix) in SOCIAL_IMAGES {
    let found: Vec<&PathBuf> = children.iter().filter(|p| p.is_file() && p.file_stem().is_some_and(|s| s == stem) && !is_generated_image(p)).collect();
    let generated = children.iter().find(|p| p.file_stem().is_some_and(|s| s == stem) && is_generated_image(p));
    if let (Some(file), Some(_)) = (found.first(), generated) {
      return Err(BuildError::SocialImage(dir.to_path_buf(), format!("`{}` and `{stem}.tsx` both draw its {stem}; keep one", file.display())));
    }
    if let Some(file) = generated {
      let mut pages = Vec::new();
      pages_under(dir, &mut pages)?;
      if pages.is_empty() {
        return Err(BuildError::SocialImage(file.clone(), "a generated card image covers the pages under its directory, and there is none".to_owned()));
      }
    }
    let file = match found.as_slice() {
      [] => continue,
      [one] => *one,
      [first, second, ..] => return Err(BuildError::SocialImage(dir.to_path_buf(), format!("`{}` and `{}` are both its {stem}; keep one", first.display(), second.display()))),
    };
    let ext = file.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let Some((_, mime)) = SOCIAL_IMAGE_TYPES.iter().find(|(e, _)| *e == ext) else {
      return Err(BuildError::SocialImage(file.clone(), format!("a card image is png, jpg, gif or webp, which a crawler decodes, not `{ext}`")));
    };
    let rel = file.strip_prefix(app).unwrap_or(file).to_string_lossy().replace('\\', "/");
    let Some(facts) = resolver.original(&rel) else {
      let why = resolver.refused.borrow().iter().rev().find(|(p, _)| *p == rel).map(|(_, why)| why.clone()).unwrap_or_else(|| "not an image this build reads".to_owned());
      return Err(BuildError::SocialImage(file.clone(), why));
    };
    let (key, field) = if prefix == "og" { ("property", "og:image") } else { ("name", "twitter:image") };
    let row = |name: String, content: String| snapfire_fsr_plan::HeadRow { tag: "meta".to_owned(), attrs: vec![(key.to_owned(), name), ("content".to_owned(), content)] };
    head.push(row(field.to_owned(), facts.src));
    head.push(row(format!("{field}:width"), facts.width.to_string()));
    head.push(row(format!("{field}:height"), facts.height.to_string()));
    head.push(row(format!("{field}:type"), (*mime).to_owned()));
    let alt = dir.join(format!("{stem}.alt.txt"));
    if alt.is_file() {
      let text = std::fs::read_to_string(&alt).map_err(|e| BuildError::Io(alt.clone(), e))?;
      head.push(row(format!("{field}:alt"), text.trim().to_owned()));
    }
  }
  if !head.is_empty() {
    out.push((dir.to_path_buf(), head));
  }
  for child in children.iter().filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "slots")) {
    image_dirs(app, child, resolver, out)?;
  }
  Ok(())
}

/// `dir` and every directory under it that holds a page.
fn pages_under(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), BuildError> {
  if page_file(dir)?.is_some() {
    out.push(dir.to_path_buf());
  }
  let mut children: Vec<PathBuf> = std::fs::read_dir(dir).map_err(|e| BuildError::Io(dir.to_path_buf(), e))?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "slots")).collect();
  children.sort();
  for child in children {
    pages_under(&child, out)?;
  }
  Ok(())
}

/// The one page file in `dir`: `page.tsx`, `page.ts` or a `page.<ext>` for an
/// extension in `TEMPLATE_EXTENSIONS`. Two of them is a refusal naming both,
/// and a template is a refusal when this fsr was built without its feature,
/// so a template never becomes a silent 404.
fn page_file(dir: &Path) -> Result<Option<String>, BuildError> {
  let found = route_file(dir, &["page.tsx", "page.ts", "page.mdx", "page.md"], "page")?;
  if let Some(file) = found.as_deref().filter(|f| snapfire_fsr_lower::component::is_transformed(f)) {
    if !mdx::BUILT {
      return Err(mdx::unbuilt(&dir.join(file)));
    }
  }
  Ok(found)
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
    if let Some(group) = name.strip_prefix('(').and_then(|n| n.strip_suffix(')')) {
      if group.is_empty() || !group.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
        return Err(BuildError::Segment { path: dir.to_path_buf(), name });
      }
      id_parts.push(name);
      continue;
    }
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

/// A head script with inline code is refused: a navigation inserts head rows
/// as data the client loads by `src`, so the code would run on a document load
/// and never after a navigation.
fn refuse_inline_scripts(module: &str, meta: Option<&snapfire_fsr_ir::Body>) -> Result<(), BuildError> {
  match meta.is_some_and(|body| snapfire_fsr_ir::ast::head_scripts(body).iter().any(|script| script.inline)) {
    true => Err(BuildError::InlineScript(module.to_owned())),
    false => Ok(()),
  }
}
