//! Route discovery, the contract and the build that emits a plan file and the
//! generated TypeScript from them. `build` analyses, `write` puts the generated
//! files on disk and `emit` does both and then the browser bundle, which is the
//! whole artifact a host reads. The binary in `main.rs` is a thin front over them.

pub mod add;
pub mod bodies;
pub mod assets;
#[cfg(feature = "desktop")]
pub mod desktop;
pub mod dev;
pub mod direction;
pub mod doctor;
pub mod features;
pub mod migrate;
pub mod new;
pub mod plugins;
pub mod published;
pub mod serve;
#[cfg(feature = "web")]
pub mod server;
pub mod sites;
#[cfg(feature = "test")]
pub mod spec;
#[cfg(feature = "terminal")]
pub mod terminal;
pub mod infer;
pub mod lint;
pub mod native;
#[cfg(feature = "test")]
pub mod test;
#[cfg(feature = "web")]
pub mod bundle;
#[cfg(feature = "web")]
pub mod census;
pub mod install;
pub mod intl;
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

mod checks;
mod compile;
mod discover;
mod frameworks;
mod generated;
mod layouts;
mod rewrites;
mod shell;

pub(crate) use self::checks::*;
pub(crate) use self::compile::*;
pub(crate) use self::discover::*;
pub(crate) use self::frameworks::*;
pub(crate) use self::generated::*;
pub(crate) use self::layouts::*;
pub(crate) use self::rewrites::*;
pub(crate) use self::shell::*;

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
  #[error("the code names the locale{} {}, not in `[locales]` ({}), and `unlisted = \"error\"` refuses that; add {} to `supported` or to `formats`", if named.len() == 1 { "" } else { "s" }, named.join(", "), listed.join(", "), if named.len() == 1 { "it" } else { "them" })]
  UnlistedLocales { named: Vec<String>, listed: Vec<String> },
  #[error("no `routes/` directory under {0}")]
  NoRoutes(PathBuf),
  #[error("{path}: `{name}` is not a route segment; use a name of letters, digits, `-`, `_` and `.`, `[param]`, `[...rest]` or `(group)`")]
  Segment { path: PathBuf, name: String },
  #[error("{file} imports `{specifier}`, which TypeScript resolves to `{module}` beside the component rather than to the component; import it as `{alias}`")]
  ShadowedImport { file: String, specifier: String, module: String, alias: String },
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
  #[error("{file}:{line} imports `@snapfire/fsr/host` and the browser loads this file; a host member runs only in a host. Read the value in the route's loader `page.loader.ts` from `@snapfire/fsr/host` and pass it to the page as a prop. A member `@snapfire/fsr/client` also has is imported from there")]
  HostInBrowser { file: String, line: u32 },
  #[error("{file} draws with FSR's Ink, which runs only in a terminal; a terminal program says so with `runs = \"terminal\"` under `[app]` in config/app.toml")]
  TerminalUndeclared { file: String },
  #[error("`{module}` is a .{ext} component, but this fsr has no client adapter that mounts one")]
  NoAdapter { module: String, ext: String },
  #[error("`{module}` is not a component this build can mount: no framework claims its extension")]
  UnknownComponent { module: String },
  #[error("`{module}` mounts through `{adapter}`, but the import map does not name {missing}{remedy}")]
  IslandImports { module: String, adapter: String, missing: String, remedy: String },
  #[error("`{placed}` does not lower, so only a browser could render it: {why}")]
  BrowserOnlyInTerminal { placed: String, why: String },
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
  #[error("{package}@{version} is older than {floor}, the earliest release this fsr's renderer was checked against; `{add}`")]
  FrameworkUnchecked { package: String, version: String, floor: String, add: String },
  /// A framework plugin that started and then failed to answer.
  #[error("{0}")]
  Plugin(String),
  #[error("the shell serves `{specifier}`, but {contract} does not say which {package} it is; a shell built by this fsr records it under `frameworks`. A hand-written contract needs `\"frameworks\": {{\"{package}\": \"{version}\"}}`")]
  FrameworkShellUnrecorded { package: String, specifier: String, contract: String, version: String },
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
  #[error("{0}: the root page draws with Ink but this fsr is built without the `terminal` feature, which declares its modules")]
  TerminalFeature(PathBuf),
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
  #[error("`{name}` is not a package name, so nothing is vendored for it; {by}")]
  PackageName { name: String, by: String },
  #[error("{map} maps `{specifier}` to `{found}`, but this application serves its vendor tree from `{base}`; write `{want}` or run `fsr add` on the package, which rewrites every vendored entry")]
  VendorUrl { map: String, specifier: String, found: String, base: String, want: String },
  #[error("`{dir}/{file}` imports `{specifier}`, which {map} does not name, so the browser cannot load it; vendor the package again with `fsr add` or add `{specifier}` to the map")]
  VendorImports { dir: String, file: String, specifier: String, map: String },
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
  /// Host features the application calls for that the binary serving it lacks.
  #[error("{0}")]
  Features(String),
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

impl BuildError {
  /// A failed request to `url`, with every cause reqwest wraps, since its own message names none of them.
  pub(crate) fn http(url: String, error: reqwest::Error) -> Self {
    let error = error.without_url();
    let mut message = error.to_string();
    let mut source = std::error::Error::source(&error);
    while let Some(cause) = source {
      message.push_str(": ");
      message.push_str(&cause.to_string());
      source = cause.source();
    }
    BuildError::Http(url, message)
  }
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

/// One lowered component whose bundle copy the build rewrote, with how many of each thing it reads from the server.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hoisted {
  pub module: String,
  /// Render-path calls the server computes.
  pub values: usize,
  /// Static subtrees the server renders.
  pub subtrees: usize,
  /// Those subtrees that keep a fallback in the bundle.
  pub fallbacks: usize,
  /// Islands it places, each reading its server-rendered props.
  pub islands: usize,
  /// Keyed placements wrapped so what they place keys below them.
  pub keyed: usize,
  /// `useId` calls answered with the server's id.
  pub ids: usize,
  /// Values only the browser knows, null through hydration.
  pub browser: usize,
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
  /// What the bundle's copy of each lowered component reads from the server.
  pub hoisted: Vec<Hoisted>,
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
  /// Each `<Embed>` whose id the page computes under `[media] embed_stills = "local"`, which shows no still: its module and the provider.
  pub embeds: Vec<(String, String)>,
  /// Each `<Embed>` still `local` could not serve: the provider, the id and why.
  pub embed_failures: Vec<(String, String, String)>,
  /// Each component the browser renders alone that places an `<Embed>` under `local`, which shows no still.
  pub browser_embeds: Vec<String>,
  /// When a stylesheet imports Tailwind: how many classes the plan renders. Each module with a
  /// class value the build cannot list comes with that value.
  pub tailwind: Option<(usize, Vec<(String, String)>)>,
  /// What converting an `.mdx` or `.md` file left out: the file, the line and column, and why.
  pub mdx: Vec<(String, usize, usize, String)>,
  /// A named theme setting a token the stylesheet reads at build, one line each.
  pub theme_warnings: Vec<String>,
  /// Each locale the code names that `[locales]` does not list, with the listed locales and the default the server formats it in.
  pub locales: Vec<(String, Vec<String>, String)>,
  /// A vendored framework newer than the latest release its renderer was checked against: the
  /// package, its version and that release.
  pub unchecked: Vec<(String, String, String)>,
  /// A framework a site vendors at another version than its shell serves, which the browser never
  /// loads: the package, the site's version and the shell's.
  pub shadowed: Vec<(String, String, String)>,
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
    }
    for (module, provider) in &self.embeds {
      writeln!(f, "{:<9} {module:<34} {provider}: the id is computed, so the placeholder shows the title alone", "embed")?;
    }
    for (provider, id, why) in &self.embed_failures {
      writeln!(f, "{:<9} {provider} {id}: {why}, so the placeholder shows the title alone", "embed")?;
    }
    for module in &self.browser_embeds {
      writeln!(f, "{:<9} {module:<34} renders in the browser, which holds no local still, so the placeholder shows the title alone", "embed")?;
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
    for (tag, listed, default) in &self.locales {
      writeln!(f, "{:<9} `{tag}` is named in the code and is not in [locales] ({}); the server formats it as {default} and a browser as {tag}", "warning", listed.join(", "))?;
    }
    for (package, own, served) in &self.shadowed {
      writeln!(f, "{:<9} the site's own {package}@{own} is unused, its shell serves {served}; `fsr add` of {package}@{served} in the site drops it", "warning")?;
    }
    for (package, version, latest) in &self.unchecked {
      writeln!(f, "{:<9} {package}@{version} is newer than {latest}, the latest release this fsr's renderer was checked against; hydration may report a mismatch", "warning")?;
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
    for (i, hoisted) in self.hoisted.iter().filter(|_| verbose).enumerate() {
      let label = if i == 0 { "hoisted" } else { "" };
      let counted = |n: usize, one: &str, many: &str| (n > 0).then(|| format!("{n} {}", if n == 1 { one } else { many }));
      let parts: Vec<String> = [
        counted(hoisted.values, "value", "values"),
        counted(hoisted.subtrees, "subtree", "subtrees"),
        (hoisted.fallbacks > 0).then(|| format!("{} with a fallback", hoisted.fallbacks)),
        counted(hoisted.islands, "island", "islands"),
        counted(hoisted.keyed, "keyed placement", "keyed placements"),
        counted(hoisted.ids, "useId", "useIds"),
        counted(hoisted.browser, "browser value", "browser values"),
      ]
      .into_iter()
      .flatten()
      .collect();
      writeln!(f, "{label:<9} {:<34} {}", hoisted.module, parts.join(", "))?;
    }
    for (i, (service, document)) in self.services.iter().filter(|_| verbose).enumerate() {
      let label = if i == 0 { "services" } else { "" };
      let kind = if document.ends_with(".proto") {
        "grpc"
      } else if document.ends_with(".graphql") {
        "graphql"
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

/// The page at `/`, the one a terminal application draws.
pub const ROOT_PAGE: &str = "routes/page.tsx";

/// Whether the application under `app` runs in a terminal: `[app] runs = "terminal"` in the configuration beside it.
pub fn is_terminal_app(app: &Path) -> bool {
  config_beside(app).is_some_and(|config| config.runs == snapfire_fsr_host::config::Runs::Terminal)
}
/// The file under `CONTRACTS_DIR` holding every `#[service]` block's contract.
pub const RUST_CONTRACT: &str = "rust.json";
/// Where the build writes the plan file.
/// The directory under an app that `fsr test` and the bundle overlay write into.
pub const FSR_DIR: &str = ".fsr";

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

/// The roots a body's `file` calls reach under the host this application's configuration describes, as the stock host gives them.
#[cfg(feature = "test")]
pub(crate) fn file_roots(app: &Path) -> Option<std::sync::Arc<snapfire_fsr_core::ext::FileRoots>> {
  let config = config_beside(app)?;
  Some(std::sync::Arc::new(snapfire_fsr_core::ext::FileRoots::new(config.app.clone(), config.files.roots.iter().map(PathBuf::from).collect())))
}

/// The configuration beside `app`, when it names this app directory. One that does not load is
/// `None` here and refused by [`build_with`].
pub(crate) fn config_beside(app: &Path) -> Option<snapfire_fsr_host::config::Config> {
  let root = serve::project_root(app);
  let config = snapfire_fsr_host::config::Config::load(&root).ok()?;
  let given = app.canonicalize().ok()?;
  (config.app.canonicalize().ok()? == given).then_some(config)
}

/// Whether the host answers the client route for `app`, adding the client modules to the import map it serves: unless a `[[static]]` root takes the route.
pub(crate) fn host_serves_client(app: &Path) -> bool {
  !config_beside(app).is_some_and(|config| config.statics.iter().any(|s| s.route == snapfire_fsr_host::client::ROUTE))
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
  /// The generated files that are not text: the locale data.
  pub data: Vec<(String, Vec<u8>)>,
  /// The session defaults every body was lowered with, so a test lowers its target the same way.
  pub defaults: SessionDefaults,
  /// The route modules the browser mounts, as files, which is all of `routes/` a bundle compiles.
  pub browser_routes: Vec<String>,
  /// Every file the browser loads: the ones it loads by name, the routes among them, every local file they import and each stylesheet a route's modules import. The bundle and `fsr test` compile these.
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
      module: component_file(dir, "error").map(|f| format!("{rel}/{f}#default")),
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

/// Builds `app` with plugins started for this build alone; [`build_with`] takes a command's pool.
pub fn build(app: &Path, options: &Options) -> Result<Built, BuildError> {
  build_with(app, options, &mut plugins::Plugins::new())
}

/// Builds `app`, reading each framework component through `plugins`, the pool the command keeps for
/// the compiler it drives as well.
pub fn build_with(app: &Path, options: &Options, plugins: &mut plugins::Plugins) -> Result<Built, BuildError> {
  build_in(app, options, plugins, &mut None)
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
  written.extend(write_data(app, built)?);
  written.extend(types::refresh_embedded(app)?);
  Ok(written)
}

/// `build`, then the typecheck over the build's own `generated/`,
/// `tsconfig.json` and foreign shim, written first so what the checker reads
/// is what this build produced.
/// `report` runs between the two, while the checker is still to come.
pub fn check(app: &Path, options: &Options, typecheck: &typecheck::Typecheck, report: impl FnOnce(&Built)) -> Result<(Built, Option<typecheck::Checked>), BuildError> {
  let built = build(app, options)?;
  write_generated(app, &built)?;
  for (rel, content) in built.files.iter().filter(|(rel, _)| rel == "tsconfig.json" || rel.ends_with(&format!("/{}", types::FOREIGN_SHIM))) {
    let path = app.join(rel);
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
  }
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
  write_data(app, built)?;
  Ok(())
}

/// Writes the generated files of `built` that are not text and returns their paths. A file that already holds the same bytes is left as it is.
fn write_data(app: &Path, built: &Built) -> Result<Vec<PathBuf>, BuildError> {
  let mut written = Vec::new();
  for (rel, content) in &built.data {
    let path = app.join(rel);
    if std::fs::read(&path).is_ok_and(|held| held == *content) {
      continue;
    }
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent).map_err(|e| BuildError::Io(parent.to_path_buf(), e))?;
    }
    std::fs::write(&path, content).map_err(|e| BuildError::Io(path.clone(), e))?;
    written.push(path);
  }
  Ok(written)
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

/// One `<Island>` a template places.
struct Placement {
  module: String,
  /// Placed with `define`: a custom element the browser imports.
  define: bool,
  /// Placed with `mode="server"`: the server renders every step and no adapter mounts it.
  server: bool,
}

/// Where the Svelte versions of FSR's components are written, for a `.svelte` file's import of them.
pub(crate) const SVELTE_COMPONENTS: &str = "@generated/components";

/// A client module that mounts one framework's components: the three exports
/// the registry names and the bare specifiers the module imports, which the
/// page's import map has to supply.
pub(crate) struct Adapter {
  pub(crate) module: &'static str,
  /// What the registry calls the loader of this adapter.
  name: &'static str,
  mounter: &'static str,
  patcher: &'static str,
  unmounter: &'static str,
  /// Hides a mounted island in place while activity holds its page, where the framework can.
  pauser: Option<&'static str>,
  needs: &'static [&'static str],
}

pub(crate) const REACT: Adapter = Adapter { module: "@snapfire/fsr/client/react", name: "react", mounter: "reactMounter", patcher: "reactPatcher", unmounter: "reactUnmounter", pauser: Some("reactPauser"), needs: &["react", "react-dom/client"] };

pub(crate) const VUE: Adapter = Adapter { module: "@snapfire/fsr/client/vue", name: "vue", mounter: "vueMounter", patcher: "vuePatcher", unmounter: "vueUnmounter", pauser: None, needs: &["vue"] };

pub(crate) const SVELTE: Adapter = Adapter { module: "@snapfire/fsr/client/svelte", name: "svelte", mounter: "svelteMounter", patcher: "sveltePatcher", unmounter: "svelteUnmounter", pauser: None, needs: &["svelte", "svelte/internal/client"] };

type Rows = Vec<(String, String, String)>;

/// Where an application keeps its custom elements' shadow templates.
pub const ELEMENTS_DIR: &str = "elements";

struct LayoutInfo {
  dir: PathBuf,
  module: String,
  source: Option<String>,
  /// The parallel slots under its `slots/` directory.
  slots: Vec<SlotInfo>,
  /// The named slots its template places, `content` aside.
  placed: Vec<String>,
}

struct SlotInfo {
  name: String,
  page: String,
  source: Option<String>,
  loading: Option<String>,
  error: Boundary,
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
