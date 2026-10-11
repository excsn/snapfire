//! The stock host: `config/`, `generated/plan.json` and `generated/contracts/`
//! become a `tower::Service` over `http` types. hyper serves it, axum nests
//! it, actix reaches it through the `actix` feature's shim.

pub mod assets;
pub mod client;
pub mod config;
pub mod locale;
mod bundle;
mod live;
mod prerender;
mod render;
mod auth;
mod actions;
mod edge;
mod remote;
mod report;
mod serve;
pub mod shell;
pub mod trace;

#[cfg(feature = "actix")]
pub mod actix;

#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "ws")]
pub mod socket;

#[cfg(feature = "tera")]
pub mod tera;

use std::collections::HashMap;
use std::convert::Infallible;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::StreamExt;
use futures_util::stream::BoxStream;
use http::{HeaderValue, Method, Request, Response, StatusCode, header};
use http_body_util::{BodyExt, StreamBody};
use snapfire_fsr::{App, AppBuilder, BindError, IntoPlan, Owner, Report};
use snapfire_fsr_auth::{Auth, AuthError, DevProvider, IdentityProvider};
use snapfire_fsr_core::{Data, ModuleId, Node, Params, PlanNode, Value, ValueMap};
use snapfire_fsr_ir::HandlerRef;
use snapfire_fsr_plan::{Child as PlanChild, Manifest, Node as PlanFileNode, RouteEntry, RowOwner, renumber};
use snapfire_fsr_runtime::ActionHandler;
use snapfire_fsr_runtime::{
  ActionError, Address, AssembleError, CsrfHandle, DataSource, Evaluator, FibreCache, Head, Identity, LoadError, Locale,
  CacheEntry, Chunk, IslandEvent, Matcher, Metadata, NoCache, NodeCache, Origin, RequestCtx, Resolver, SessionCell, WarmLoads, WarmRenders, assemble, assemble_under, html_stream,
  parse_query, wire_stream,
};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Credentials, DeclaredService, HttpTransport, IdentityInterceptor, MockTransport,
  NoCredentials, Services, TraceInterceptor, Transport,
};
use snapfire_fsr_session::{CookieCodec, CsrfScheme, HmacCodec, Keyring, MemorySessionStore, Opened, SessionConfig, SessionId, SessionStore, Sessions, TokenCell};
use tower::ServiceExt;
use tower_http::services::ServeDir;

pub use config::{
  AuthSection, BearerKey, ClientConfig, Config, DataCacheSection, Loader, MountConfig, SecretOptions, SiteSection,
  SitesSection,
};
pub use locale::{Locales, LocalesSection, Resolution};
pub use remote::{ServiceProvider, ServiceSessionStore};
pub use report::{HostReport, SiteReport, TlsReport};
pub use serve::HostService;
pub use edge::{percent_decoded};
pub use actions::{IslandBody, IslandStep, Preflight, PreflightAction, island_body, island_step};
pub use auth::{referer_path};
pub use prerender::{LOADS_FILE, RENDERS_FILE, WRITTEN_FILE};
pub use snapfire_fsr_core::ext::{self, Ambient, Catalogs, Fail, FailureKind, Reach};

use bundle::*;
use edge::*;
use actions::*;
use auth::*;
use render::*;
use prerender::*;
use live::*;
use serve::*;

/// The encodings a payload request may name in `enc`; the wire's `V` row
/// names the one it got.
pub const PAYLOAD_ENCODINGS: &[&str] = &["json"];

/// The extensions of a route or layout file the host renders through a
/// template evaluator rather than the lowered tree: `routes/board/page.tera`
/// is the module `routes/board/page.tera#default` and no component is lowered
/// for it. The CLI discovers such files by this list and the host refuses a
/// plan naming one no evaluator answers.
pub const TEMPLATE_EXTENSIONS: &[&str] = &["tera"];

/// Whether a module's path ends in one of `TEMPLATE_EXTENSIONS`.
pub fn is_template_module(module: &ModuleId) -> bool {
  TEMPLATE_EXTENSIONS.iter().any(|ext| module.path.ends_with(&format!(".{ext}")))
}

/// Directories directly under the app no template is read from.
const TEMPLATE_SKIPPED: &[&str] = &["vendor", "dist", "generated", "node_modules", "tests", "types"];

/// Every template under `app` the host reads at boot, sorted: each file
/// ending in one of `TEMPLATE_EXTENSIONS` outside a dot directory and the
/// `TEMPLATE_SKIPPED` ones.
pub fn template_files(app: &Path) -> Result<Vec<PathBuf>, HostError> {
  fn collect(app: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), HostError> {
    let entries = std::fs::read_dir(dir).map_err(|e| HostError::Io(dir.to_path_buf(), e))?;
    for path in entries.filter_map(|e| e.ok().map(|e| e.path())) {
      let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
      if path.is_dir() {
        if dir == app && TEMPLATE_SKIPPED.contains(&name.as_str()) || name.starts_with('.') {
          continue;
        }
        collect(app, &path, out)?;
      } else if TEMPLATE_EXTENSIONS.iter().any(|ext| name.ends_with(&format!(".{ext}"))) {
        out.push(path);
      }
    }
    Ok(())
  }
  let mut out = Vec::new();
  collect(app, app, &mut out)?;
  out.sort();
  Ok(out)
}

/// The response body: a stream of chunks, the same one the runtime produces.
pub type Body = http_body_util::combinators::UnsyncBoxBody<Bytes, std::io::Error>;

#[derive(Debug, thiserror::Error)]
pub enum HostError {
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("{0}: {1}")]
  Config(PathBuf, String),
  #[error("no configuration under {0}: expected `config/`, or `app.toml` beside it")]
  NoConfig(PathBuf),
  #[error("`{0}` is not a valid value: `{1}`")]
  Value(String, String),
  #[error(transparent)]
  Bind(#[from] BindError),
  #[error("{document}: {error}")]
  Import {
    document: String,
    error: snapfire_fsr_service::ImportError,
  },
  #[error("clients.{0}: {1}")]
  Transport(String, String),
  #[error("service `{0}`: {1}")]
  Service(String, String),
  #[error("{0}: {1}")]
  Contract(PathBuf, String),
  #[error("no route matches `{0}`")]
  NotFound(String),
  #[error("no slot named `{0}` on this route")]
  NoSlot(String),
  #[error(transparent)]
  Assemble(#[from] AssembleError),
  #[error("server modules in the bundle: {0}")]
  Leak(String),
  #[error("site `{0}`: {1}")]
  Mount(String, String),
  #[error("`{0}`: paths: {1}")]
  Paths(String, String),
  #[error("the plan names `{0}`, a template no evaluator answers; build with the `tera` feature or register one with `HostBuilder::evaluator`")]
  Uncovered(String),
  #[error("the plan names `{0}` and no such template is under the app")]
  TemplateMissing(String),
  #[error("templates under {0}: {1}")]
  Template(PathBuf, String),
}

/// What a request for a route is answered with: the document, the navigator's
/// payload or one segment of the route as bare markup. A fragment is chosen
/// by `__fragment` in the query: alone it is the page, `__fragment=<slot>` a
/// parallel slot by name, wherever it sits on the route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderMode {
  Html,
  Payload,
  Fragment(Option<String>),
}

/// The fragment `raw_query` asks for, when it asks for one.
pub fn fragment_of(raw_query: &str) -> Option<Option<String>> {
  form_urlencoded::parse(raw_query.as_bytes())
    .find(|(key, _)| key == "__fragment")
    .map(|(_, slot)| (!slot.is_empty()).then(|| slot.into_owned()))
}

/// Whether `raw_query` asks for the navigator's payload.
pub fn payload_of(raw_query: &str) -> bool {
  form_urlencoded::parse(raw_query.as_bytes()).any(|(key, _)| key == "__payload")
}

/// `location` with the fragment `slot` names asked for again, so a form posted
/// from a fragment is answered with one.
pub fn with_fragment(location: &str, slot: Option<&str>) -> String {
  let joiner = if location.contains('?') { '&' } else { '?' };
  match slot {
    Some(name) => format!("{location}{joiner}__fragment={name}"),
    None => format!("{location}{joiner}__fragment"),
  }
}

pub struct Host {
  live: parking_lot::RwLock<Arc<Tables>>,
  sessions: Sessions,
  /// The development reload signal, one topic with one key. A reader that
  /// cannot keep up loses a signal rather than the whole stream, which is
  /// what an idempotent "something changed" wants.
  changed: Option<Reload>,
  /// The request traces, when a collector was installed. Read by
  /// `/__fsr/traces` under development and by nothing else.
  traces: Option<trace::Traces>,
  /// Topics the application publishes, read by every open `/_sf/live`
  /// stream. Always present: pushing is the application's, not the dev loop's.
  /// A stream subscribes its own receiver to the topics it asked for, so the
  /// channel does the filtering and a stream is never woken for a topic it
  /// did not ask about.
  topics: Topics,
  /// Who may follow which topic. Absent, any topic may be followed by
  /// anyone, which is right for a board on a wall and wrong for a room.
  topic_rule: Option<TopicRule>,
  /// Every open socket by topic and what the application makes of what one
  /// sends. Without a handler `/_sf/socket` is a 404, since a socket nobody
  /// answers is a socket that does nothing.
  #[cfg(feature = "ws")]
  sockets: Arc<socket::Sockets>,
  #[cfg(feature = "ws")]
  socket_handler: Option<socket::SocketHandler>,
  reloader: Option<Reloader>,
  /// How the artifact was read, for a `reload` with no reloader to read it
  /// again. Absent when the host was built from an artifact in memory.
  loader: Option<Loader>,
  /// What `reload_sites` rebuilds from: the shell's own artifact as it was
  /// when the process booted, never reread. Absent when the builder carried
  /// something a rebuild from the artifact alone cannot reproduce.
  shell_inputs: Option<Arc<Artifact>>,
  /// Whether the builder was given a `Mount` by hand, which a reread of the
  /// shell's artifact would drop.
  mounted: bool,
  /// How the mounted sites are read again, given a builder the host made from
  /// `shell_inputs`. The step takes a builder rather than making one, so
  /// nothing it does can reach the shell's files.
  sites_mounter: Option<SitesMounter>,
  /// Held across a `/__fsr/sites/reload`, so repeating the request waits on the
  /// reload in flight rather than reading every artifact a second time.
  #[cfg(feature = "sites_reload")]
  sites_reload: parking_lot::Mutex<()>,
  csrf_always: bool,
  report_listen: String,
  /// The most bytes a request body may carry, `server.max_body`.
  max_body: usize,
  /// The most bytes one uploaded part may carry, `server.max_upload`; `0` is
  /// no cap beyond `max_body`.
  max_upload: usize,
  /// The hosts a request's `Host` is matched against, `server.hosts`,
  /// lowercased once at boot. Empty, the header is never read.
  hosts: Vec<String>,
  /// `document.origin`, checked at boot, which the canonical link carries.
  origin: Option<String>,
  /// Whether `serve_listener` negotiates HTTP/2 as well as HTTP/1.1 on a
  /// connection, `server.http2`.
  http2: bool,
  /// The certificate the listener presents, `[server.tls]`; absent, the
  /// listener is plain TCP.
  #[cfg(feature = "tls")]
  tls: Option<Arc<tls::Tls>>,
  /// The signal that re-reads it, `server.tls.reload`; `None` for `none`.
  #[cfg(feature = "tls")]
  tls_reload: Option<String>,
  /// The `[session]` settings the running `Sessions` were built from, the
  /// keys aside; a reload whose settings differ is refused, since the store
  /// outlives it.
  session_shape: String,
  /// The ring built from `session.key` and `session.previous_keys`, which a
  /// reload sets to the list as it now reads. Absent when the application
  /// handed the builder its own.
  keyring: Option<Arc<Keyring>>,
  /// The ring the cookies sign with, whichever made it, which also signs the
  /// `x-sf-request` token.
  trace_ring: Arc<Keyring>,
}

/// How a host rebuilds its tables on `Host::reload`: the builder for the
/// application as it now stands on disk.
pub type Reloader = Box<dyn Fn() -> Result<HostBuilder, HostError> + Send + Sync>;

/// How a host mounts its sites again on `Host::reload_sites`. It is handed a
/// builder the host already made from the shell's frozen inputs and returns it
/// with the sites of the moment mounted, so a sites reload cannot reread the
/// shell however the step is written.
pub type SitesMounter = Box<dyn Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync>;

/// A built application as the host reads it: its configuration, its plan and
/// the contracts its build wrote. `Host::from` reads one through a `Loader`
/// and a `Mount` carries one.
#[derive(Debug, Clone)]
pub struct Artifact {
  pub config: Config,
  pub plan: String,
  pub contract: Option<Contract>,
}

impl Artifact {
  /// Reads the plan file and the contracts directory `config` names, the
  /// latter merged with `Contract::merge` file by file when it exists.
  pub fn of(config: Config) -> Result<Self, HostError> {
    let plan_path = config.resolve(&config.server.plan);
    let plan = std::fs::read_to_string(&plan_path).map_err(|e| HostError::Io(plan_path, e))?;
    let contract = read_contracts(&config.resolve(&config.server.contracts))?;
    Ok(Artifact { config, plan, contract })
  }
}

/// Whether this visitor may follow this topic, asked once per topic when a
/// stream opens. The session is the one the request's cookie names and the
/// identity is whatever signed into it.
pub type TopicRule = Arc<dyn Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync>;

/// Everything a request reads that a reload replaces. A request loads the
/// current set once at the edge and keeps it for its lifetime, so a reload
/// mid-request changes nothing for that request.
struct Tables {
  app: App,
  head: Head,
  /// `document.title` and `document.title_template` as configured; `head`
  /// carries their literals and a keyed one is read per request under its locale.
  title: config::Text,
  title_template: Option<config::Text>,
  /// `[public]` as values, cloned into every request's `ctx.config`.
  public: ValueMap,
  /// `document.origin`, checked at boot, read by every request's `ctx.origin`.
  origin: Option<String>,
  /// The bundle's build facts file, read for its id when `dev` is on; the
  /// plain head is what `prerender` writes.
  dev_bundle: Option<PathBuf>,
  statics: Vec<(String, ServeDir)>,
  /// The static path `/favicon.ico` is looked up as, `config.favicon`.
  favicon: Option<String>,
  /// The `Cache-Control` a static answer carries, from `server.static_max_age`.
  /// Absent when that is `0` or when `dev` is on, which sends `no-cache` instead.
  static_cache: Option<HeaderValue>,
  /// Whether [`client::ROUTE`] is answered out of the binary, which it is
  /// unless a static root claims the prefix.
  client: bool,
  /// Whether that route answers with the minified build, `document.client`
  /// resolved against `server.dev`.
  client_minified: bool,
  /// The `Content-Security-Policy` an HTML response carries, `document.csp`
  /// with the import map's source substituted. `None` sends no policy.
  csp: Option<HeaderValue>,
  /// The same for `Content-Security-Policy-Report-Only`, `document.csp_report_only`.
  csp_report_only: Option<HeaderValue>,
  /// The nonce on the development refresh script, named by both policies.
  dev_nonce: Option<String>,
  /// `trace.expose`: the path prefixes whose requests a client may read its
  /// own trace of.
  trace_expose: Vec<String>,
  prerendered: Option<PathBuf>,
  /// The memo the app's runtime reads a warmable source's data from, held
  /// here so a warm pass swaps its contents in before it renders anything.
  warm: Arc<WarmLoads>,
  /// The render memo, with what a build rendered in front of the live cache;
  /// a render pass records into it.
  renders: Arc<WarmRenders>,
  locales: Locales,
  catalogs: Arc<Catalogs>,
  auth: Option<Mounted>,
  /// The mounted sites, longest prefix first.
  sites: Vec<SiteTables>,
  report: Arc<HostReport>,
}

/// What a request under a site's prefix reads beyond the merged tables: the
/// site's middleware, run after the shell's, plus what its documents add
/// to the head.
struct SiteTables {
  name: String,
  at: String,
  middleware: Option<Arc<dyn ActionHandler>>,
  styles: Vec<String>,
  entry: Option<String>,
  /// The site's own modules to preload, minus what the shell already covers.
  preload: Vec<String>,
}

impl Tables {
  /// The head with a keyed title or template read under `locale`; `None`
  /// when neither is keyed and `head` already holds the text.
  fn head_for(&self, locale: &str) -> Option<Head> {
    if !self.title.is_keyed() && !self.title_template.as_ref().is_some_and(config::Text::is_keyed) {
      return None;
    }
    let mut head = self.head.clone();
    head.title = self.title.resolve(&self.catalogs, locale);
    head.template = self.title_template.as_ref().map(|template| template.resolve(&self.catalogs, locale));
    Some(head)
  }

  /// The mounted site whose prefix covers `path`, longest first.
  fn site_for(&self, path: &str) -> Option<&SiteTables> {
    self.sites.iter().find(|s| s.covers(path))
  }
}

impl SiteTables {
  fn covers(&self, path: &str) -> bool {
    path == self.at || path.strip_prefix(&self.at).is_some_and(|rest| rest.starts_with('/'))
  }
}

/// A site's artifact as the host mounts it: the site's own configuration,
/// read through the shell's ladder, its plan and its contracts, all already
/// namespaced by its build. Where it came from is the caller's business;
/// `artifact`, `version` and `hash` are carried into the report.
pub struct Mount {
  pub name: String,
  pub version: String,
  pub hash: String,
  pub allow_engine: bool,
  /// The site as read from its directory, `artifact.config.root`, by
  /// `Loader::mount` on the shell's loader or a loader of the caller's own.
  pub artifact: Artifact,
}

impl Mount {
  pub fn new(
    name: impl Into<String>,
    version: impl Into<String>,
    hash: impl Into<String>,
    allow_engine: bool,
    artifact: Artifact,
  ) -> Self {
    Self {
      name: name.into(),
      version: version.into(),
      hash: hash.into(),
      allow_engine,
      artifact,
    }
  }
}

/// The identity flow the host serves under `/auth/`, plus the application's
/// login page, where `begin` sends the browser and where a GET seeds the
/// flow so a typed URL can still post to the callback.
struct Mounted {
  auth: Auth,
  login_path: String,
}

/// What a cookie carries between the calls of one identity journey: the
/// session id the flow is keyed by and the custody its state lives in. A
/// caller driving [`Host::auth_call`] holds one per session.
#[derive(Clone)]
pub struct AuthFlow {
  id: SessionId,
  tokens: TokenCell,
}

impl AuthFlow {
  pub fn new() -> Self {
    Self {
      id: SessionId::generate(),
      tokens: TokenCell::new(ValueMap::default()),
    }
  }
}

impl Default for AuthFlow {
  fn default() -> Self {
    Self::new()
  }
}

/// A `#[service]` block the builder was given: its contract, merged with
/// what the contracts directory holds, plus the transport that answers it.
struct RustService {
  name: String,
  rust_type: String,
  contract: Contract,
  transport: Arc<dyn Transport>,
}

pub struct HostBuilder {
  /// The trace collector, when one was installed. Served under development.
  traces: Option<trace::Traces>,
  artifact: Artifact,
  loader: Option<Loader>,
  app: Option<AppBuilder>,
  services: Option<Arc<Services>>,
  rust_services: Vec<RustService>,
  csrf: Option<Arc<dyn CsrfScheme>>,
  codec: Option<Arc<dyn CookieCodec>>,
  keyring: Option<Arc<Keyring>>,
  transport_override: Option<Arc<dyn Transport>>,
  store: Option<Arc<dyn SessionStore>>,
  shell: Option<Arc<dyn Evaluator>>,
  prerendered: Option<PathBuf>,
  identity: Option<Arc<dyn IdentityProvider>>,
  reloader: Option<Reloader>,
  sites_mounter: Option<SitesMounter>,
  topic_rule: Option<TopicRule>,
  #[cfg(feature = "ws")]
  sockets: Option<Arc<socket::Sockets>>,
  #[cfg(feature = "ws")]
  socket_handler: Option<socket::SocketHandler>,
  mounts: Vec<Mount>,
  /// Overrides `server.http2` for a host built in Rust.
  http2: Option<bool>,
  /// Whether a route, source, action, handler, evaluator, extension, native
  /// module or `#[service]` was registered on the builder, which a rebuild
  /// from the artifact alone would drop.
  hand_built: bool,
  pending: Option<HostError>,
}

impl std::fmt::Debug for HostBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("HostBuilder")
      .field("root", &self.artifact.config.root)
      .finish_non_exhaustive()
  }
}

/// Every `*.json` in `dir` in name order, merged into one contract; `None`
/// when there is no such directory. A type or service defined twice names the
/// file that repeats it.
fn read_contracts(dir: &std::path::Path) -> Result<Option<Contract>, HostError> {
  if !dir.is_dir() {
    return Ok(None);
  }
  let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
    .map_err(|e| HostError::Io(dir.to_path_buf(), e))?
    .flatten()
    .map(|e| e.path())
    .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "json"))
    .collect();
  files.sort();
  let mut contract = Contract::new();
  for path in files {
    let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
    let part = Contract::from_json(&text).map_err(|e| HostError::Contract(path.clone(), e.to_string()))?;
    contract
      .merge(part, &path.file_name().unwrap_or_default().to_string_lossy())
      .map_err(|e| HostError::Contract(path.clone(), e.to_string()))?;
  }
  Ok(Some(contract))
}

impl Host {
  /// The stock entry point: a project root holding `config/`, a `config/`
  /// directory or one configuration file. Everything else is inferred from
  /// the app directory it names.
  ///
  /// Takes a path or a `Loader`; a path is `Loader::at(path)`. The builder
  /// keeps the loader, so `Host::reload` reads the artifact again the same way.
  pub fn from(loader: impl Into<Loader>) -> Result<HostBuilder, HostError> {
    let loader = loader.into();
    let mut builder = Self::from_artifact(loader.load()?)?;
    builder.loader = Some(loader);
    Ok(builder)
  }

  /// `Host::from` on the current directory.
  pub fn from_cwd() -> Result<HostBuilder, HostError> {
    Self::from(".")
  }

  /// `from_artifact` over the plan file and the contracts directory `config` names.
  pub fn from_config(config: Config) -> Result<HostBuilder, HostError> {
    Self::from_artifact(Artifact::of(config)?)
  }

  /// A builder over an artifact already in memory, which is how `fsr test`
  /// renders a route a spec loads. No loader is kept, so the host reloads
  /// only through a reloader.
  pub fn from_artifact(artifact: Artifact) -> Result<HostBuilder, HostError> {
    let config = &artifact.config;
    if config.server.render.is_some() {
      return Err(HostError::Config(
        config.resolve(&config.server.plan),
        "`server.render` is gone: every page renders on the server and every island in the browser, so take the line out".to_owned(),
      ));
    }
    let app = App::from_manifest(&artifact.plan)?;
    Ok(HostBuilder {
      artifact,
      loader: None,
      traces: None,
      app: Some(app),
      services: None,
      rust_services: Vec::new(),
      csrf: None,
      codec: None,
      keyring: None,
      transport_override: None,
      store: None,
      shell: None,
      prerendered: None,
      identity: None,
      reloader: None,
      sites_mounter: None,
      topic_rule: None,
      #[cfg(feature = "ws")]
      sockets: None,
      #[cfg(feature = "ws")]
      socket_handler: None,
      mounts: Vec::new(),
      http2: None,
      hand_built: false,
      pending: None,
    })
  }

  /// What the host bound, as of the last reload.
  pub fn report(&self) -> Arc<HostReport> {
    self.tables().report.clone()
  }

  /// The current tables, taken once per request.
  fn tables(&self) -> Arc<Tables> {
    self.live.read().clone()
  }

  /// The lowered components and their interpreter, for stepping an island in
  /// server mode outside a request; `None` when nothing was lowered.
  pub fn lowered(&self) -> Option<Arc<snapfire_fsr_ir::IrEvaluator>> {
    self.tables().app.lowered.clone()
  }

  /// The locales the host serves and how it resolves a request's.
  pub fn locales(&self) -> Locales {
    self.tables().locales.clone()
  }

  /// `[public]` as every request's `ctx.config` sees it.
  pub fn public(&self) -> ValueMap {
    self.tables().public.clone()
  }

  /// The message catalogs loaded from `locales/`, when the application has any.
  pub fn catalogs(&self) -> Option<Arc<Catalogs>> {
    let catalogs = &self.tables().catalogs;
    (!catalogs.is_empty()).then(|| catalogs.clone())
  }

  /// Rebuilds the tables and swaps them in; a request in flight finishes on
  /// the tables it started with. The sessions stay: a reload that changes
  /// `[session]` is refused.
  ///
  /// With a reloader, the builder it returns is the rebuild. Without one the
  /// artifact is read again through the loader `Host::from` kept and the
  /// sites mounter runs over it, which is enough for a host built from a
  /// configuration alone; a host given services, a store, a scheme, an
  /// evaluator, an identity provider, a prerendered directory or a mount by
  /// hand needs a reloader that adds those again.
  pub fn reload(&self) -> Result<Arc<HostReport>, HostError> {
    if let Some(reloader) = &self.reloader {
      return self.reload_with(reloader()?);
    }
    let refuse = |why: &str| HostError::Value("reload".to_owned(), why.to_owned());
    let loader = self.loader.as_ref().ok_or_else(|| {
      refuse("the host was built from an artifact in memory, so there is nothing to reread; `HostBuilder::reloader` names how to rebuild")
    })?;
    if self.shell_inputs.is_none() {
      return Err(refuse(
        "the host was built with more than a configuration, so rereading the artifact cannot rebuild it; `HostBuilder::reloader` names how",
      ));
    }
    if self.mounted && self.sites_mounter.is_none() {
      return Err(refuse(
        "the host was given a mount by hand, which rereading the artifact would drop; `HostBuilder::reloader` names how to rebuild",
      ));
    }
    let mut builder = Host::from(loader.clone())?;
    if let Some(mounter) = &self.sites_mounter {
      builder = mounter(builder)?;
    }
    self.reload_with(builder)
  }

  /// Rebuilds the tables with the sites read again and the shell left exactly
  /// as this process booted it, then swaps them in.
  ///
  /// The shell's configuration, plan and contracts are the values held since
  /// boot, so a half-written `plan.json` or an edited configuration on disk
  /// cannot reach the tables through this call: only the artifacts the
  /// `[sites]` table names are read. Everything else is [`Host::reload`]'s: the
  /// tables are rebuilt whole and checked the way a boot checks them, a refused
  /// rebuild leaves the running tables serving and the swap is atomic.
  ///
  /// Needs [`HostBuilder::sites_mounter`]. A host whose builder installed
  /// services, a session store, a CSRF scheme, an evaluator, an identity
  /// provider or a prerendered directory or registered anything on the app by
  /// hand has none, since those cannot be rebuilt from the shell's artifact;
  /// such a host reloads through a reloader instead.
  pub fn reload_sites(&self) -> Result<Arc<HostReport>, HostError> {
    let refuse = |why: &str| HostError::Value("sites.reload".to_owned(), why.to_owned());
    let mounter = self
      .sites_mounter
      .as_ref()
      .ok_or_else(|| refuse("no sites_mounter; `HostBuilder::sites_mounter` names how to mount them"))?;
    let inputs = self.shell_inputs.as_ref().ok_or_else(|| {
      refuse(
        "the host was built with more than a configuration, so its shell cannot be rebuilt from memory; use reload",
      )
    })?;
    let builder = Host::from_artifact((**inputs).clone())?;
    self.reload_with(mounter(builder)?)
  }

  /// `POST /__fsr/sites/reload`: [`Host::reload_sites`] as an answer. One
  /// reload runs at a time, so a caller that repeats the request waits for the
  /// one in flight rather than starting a second read of every artifact.
  ///
  /// The route is an administrative one and the host does not guard it. Keeping
  /// `/__fsr/` off the public internet is the deployment's job, at whatever
  /// sits in front of the host.
  #[cfg(feature = "sites_reload")]
  fn sites_reload_response(&self) -> Response<Body> {
    let _turn = self.sites_reload.lock();
    match self.reload_sites() {
      Ok(report) => {
        let sites: Vec<serde_json::Value> = report
          .sites
          .iter()
          .map(|s| serde_json::json!({ "name": s.name, "at": s.at, "version": s.version, "hash": s.hash }))
          .collect();
        json_response(StatusCode::OK, &serde_json::json!({ "reloaded": true, "sites": sites }))
      }
      // The candidate was refused and the running tables still serve it, so the
      // fault is in what the caller published rather than in the host.
      Err(e) => json_response(
        StatusCode::CONFLICT,
        &serde_json::json!({ "reloaded": false, "error": e.to_string() }),
      ),
    }
  }

  /// `reload` over a builder the caller made.
  pub fn reload_with(&self, builder: HostBuilder) -> Result<Arc<HostReport>, HostError> {
    let (tables, config) = builder.assemble()?;
    let shape = session_shape(&config);
    if shape != self.session_shape {
      return Err(HostError::Value(
        "session".to_owned(),
        "changed since the host was built; restart to apply it".to_owned(),
      ));
    }
    if let Some(ring) = &self.keyring {
      match &config.session.key {
        Some(key) if key == config::SESSION_KEY_PLACEHOLDER => {
          return Err(placeholder_session_key(&config));
        }
        Some(_) => ring.replace(session_keys(&config)),
        None if config.dev() => {}
        None => return Err(missing_session_key(&config)),
      }
    }
    let report = tables.report.clone();
    *self.live.write() = Arc::new(tables);
    self.changed();
    Ok(report)
  }

}

/// A `mock` client's transport from its responses file: an object of method
/// name to the response in the payload's JSON encoding or to
/// `{"$fail": {"kind": "<failure kind>", "message": "..."}}` for a failure.
fn mock_transport(
  config: &Config,
  name: &str,
  client: &ClientConfig,
  prefix: &str,
) -> Result<Option<(Arc<dyn Transport>, String)>, HostError> {
  if !client.is_mock() {
    return Ok(None);
  }
  let file = client.responses_file(name);
  let path = config.resolve(&file);
  let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
  let json: serde_json::Value =
    serde_json::from_str(&text).map_err(|e| HostError::Config(path.clone(), e.to_string()))?;
  let Some(entries) = json.as_object() else {
    return Err(HostError::Config(
      path,
      "expected an object of method name to response".to_owned(),
    ));
  };
  let mut mock = MockTransport::new();
  for (method, response) in entries {
    let key = format!("{prefix}{name}.{method}");
    match response.get("$fail") {
      Some(fail) => {
        let kind = match fail.get("kind").and_then(|k| k.as_str()).unwrap_or("internal") {
          "unauthorized" => FailureKind::Unauthorized,
          "not_found" => FailureKind::NotFound,
          "invalid" => FailureKind::Invalid,
          "conflict" => FailureKind::Conflict,
          "timeout" => FailureKind::Timeout,
          "unavailable" => FailureKind::Unavailable,
          "internal" => FailureKind::Internal,
          other => {
            return Err(HostError::Config(
              path,
              format!("{method}: unknown failure kind `{other}`"),
            ));
          }
        };
        let message = fail
          .get("message")
          .and_then(|m| m.as_str())
          .unwrap_or("mocked failure")
          .to_owned();
        mock = mock.fails(key, kind, message);
      }
      None => {
        let value = snapfire_fsr_payload::json_to_value(response)
          .map_err(|e| HostError::Config(path.clone(), format!("{method}: {e}")))?;
        mock = mock.returns(key, value);
      }
    }
  }
  Ok(Some((Arc::new(mock), file)))
}

/// A client's contract as the host merges it: prefixed with the site's name
/// when the application is a site, since the build prefixed the bodies.
fn site_contract(contract: &Contract, config: &Config) -> Contract {
  match &config.site {
    Some(site) => contract.namespaced(&site.name),
    None => contract.clone(),
  }
}

struct StaticRootResolved {
  route: String,
  dir: PathBuf,
}

/// The clients of one configuration: their contracts merged in, one
/// transport each unless the caller overrides transports and the report
/// rows. Names carry the configuration's site prefix, the build's spelling.
fn clients_of(
  config: &Config,
  build_transports: bool,
  contract: &mut Contract,
  transports: &mut Vec<(String, Arc<dyn Transport>)>,
  service_rows: &mut Vec<(String, String, String)>,
  bearer_rows: &mut Vec<(String, String)>,
) -> Result<(), HostError> {
  let prefix = config.site.as_ref().map(SiteSection::prefix).unwrap_or_default();
  for (name, client) in &config.clients {
    let named = format!("{prefix}{name}");
    if let Some(key) = client.bearer.as_ref().and_then(BearerKey::key) {
      bearer_rows.push((named.clone(), key.to_owned()));
    }
    let document = client
      .document
      .clone()
      .unwrap_or_else(|| format!("clients/{name}.openapi.json"));
    let path = config.resolve(&document);
    if document.ends_with(".proto") {
      let imported =
        snapfire_fsr_service::import_proto(&path, name).map_err(|error| HostError::Import { document, error })?;
      let site_contract = site_contract(&imported.contract, config);
      contract.types.extend(site_contract.types.clone());
      contract.services.extend(site_contract.services.clone());
      if let Some((transport, file)) = mock_transport(config, name, client, &prefix)? {
        transports.push((named.clone(), transport));
        service_rows.push((named.clone(), "mock".to_owned(), file));
        continue;
      }
      let base_url = client.base_url.clone().unwrap_or_default();
      if build_transports {
        let transport = snapfire_fsr_service::GrpcTransport::new(&base_url, &imported)
          .map_err(|e| HostError::Transport(name.clone(), e))?;
        transports.push((named.clone(), Arc::new(transport)));
      }
      service_rows.push((named.clone(), "grpc".to_owned(), base_url));
      continue;
    }
    let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
    let imported = snapfire_fsr_service::import(&text, name).map_err(|error| HostError::Import { document, error })?;
    let site_contract = site_contract(&imported.contract, config);
    contract.types.extend(site_contract.types.clone());
    contract.services.extend(site_contract.services.clone());
    if let Some((transport, file)) = mock_transport(config, name, client, &prefix)? {
      transports.push((named.clone(), transport));
      service_rows.push((named.clone(), "mock".to_owned(), file));
      continue;
    }
    let base_url = client.base_url.clone().unwrap_or_default();
    let mut transport = HttpTransport::new(&base_url);
    for (path, route) in &imported.routes {
      transport = transport.route(path.clone(), route.clone());
    }
    transports.push((named.clone(), Arc::new(transport)));
    service_rows.push((named.clone(), "http".to_owned(), base_url));
  }
  Ok(())
}

/// The shell's import map with a site's entries added where the shell has
/// none: the shell pins the runtime, a site brings only what it adds.
fn merge_import_maps(shell: Option<&str>, site: &str) -> String {
  let mut merged: serde_json::Value = shell
    .and_then(|s| serde_json::from_str(s).ok())
    .unwrap_or_else(|| serde_json::json!({ "imports": {} }));
  let theirs: serde_json::Value = serde_json::from_str(site).unwrap_or_else(|_| serde_json::json!({ "imports": {} }));
  if let (Some(ours), Some(theirs)) = (
    merged.get_mut("imports").and_then(|i| i.as_object_mut()),
    theirs.get("imports").and_then(|i| i.as_object()),
  ) {
    for (key, value) in theirs {
      ours.entry(key.clone()).or_insert_with(|| value.clone());
    }
  }
  merged.to_string()
}

/// The shell's root layout: the node under the document's content slot when
/// it is `routes/layout.tsx#default`.
fn shell_root_layout(shell: &Manifest, shell_module: &str) -> Option<PlanFileNode> {
  shell.routes.iter().chain(shell.intercepts.iter()).find_map(|entry| {
    if entry.plan.module != shell_module {
      return None;
    }
    let content = entry.plan.children.iter().find(|c| c.slot == "content")?;
    (content.node.module == "routes/layout.tsx#default").then(|| content.node.clone())
  })
}

/// Nests every route and intercept of a site under the shell's root layout,
/// so one tree carries the shell's header above the site's pages. Without a
/// root layout the site's subtree sits under the document directly.
fn graft(shell: &Manifest, site: &mut Manifest, shell_module: &str) {
  let layout = shell_root_layout(shell, shell_module);
  let regraft = |entry: &mut RouteEntry, keep_rest: bool| {
    let Some(content) = entry
      .plan
      .children
      .iter()
      .find(|c| c.slot == "content")
      .map(|c| c.node.clone())
    else {
      return;
    };
    let inner = match &layout {
      Some(layout) => {
        let mut grafted = layout.clone();
        for child in &mut grafted.children {
          if child.slot == "content" {
            child.node = content.clone();
          }
        }
        if keep_rest {
          grafted.keep = grafted
            .children
            .iter()
            .filter(|c| c.slot != "content")
            .map(|c| c.slot.clone())
            .collect();
        }
        grafted
      }
      None => content,
    };
    let mut plan = PlanFileNode {
      id: 0,
      module: shell_module.to_owned(),
      source: None,
      deferred: false,
      fallback: None,
      error: None,
      error_kinds: Vec::new(),
      cache_key: None,
      children: vec![PlanChild {
        slot: "content".to_owned(),
        node: inner,
      }],
      keep: Vec::new(),
    };
    renumber(&mut plan, &mut 0);
    entry.plan = plan;
  };
  for entry in &mut site.routes {
    regraft(entry, false);
  }
  for entry in &mut site.intercepts {
    regraft(entry, true);
  }
}

impl HostBuilder {
  fn app_mut(&mut self, f: impl FnOnce(AppBuilder) -> AppBuilder) -> &mut Self {
    if let Some(app) = self.app.take() {
      self.app = Some(f(app));
    }
    self.hand_built = true;
    self
  }

  /// Replaces every client transport with one, keeping the contract the
  /// configuration names. Tests use it; so does a host that reaches its
  /// services some other way.
  pub fn services_over(mut self, transport: Arc<dyn Transport>) -> Self {
    self.transport_override = Some(transport);
    self
  }

  pub fn services(mut self, services: Arc<Services>) -> Self {
    self.services = Some(services);
    self
  }

  /// A service written in Rust, a `#[service]` block, served in process
  /// under its own name through the same registry, interceptors and cache a
  /// client goes through. Its contract joins the contracts directory's; a
  /// `fsr build` that read the block writes the same contract there and one
  /// that disagrees refuses to boot. `services_over` leaves it in place.
  pub fn service<T>(mut self, service: Arc<T>) -> Self
  where
    T: Transport + DeclaredService + 'static,
  {
    self.hand_built = true;
    self.rust_services.push(RustService {
      name: T::NAME.to_owned(),
      rust_type: std::any::type_name::<T>().to_owned(),
      contract: T::contract(),
      transport: service,
    });
    self
  }

  /// The CSRF scheme, in place of the one `session.csrf_scheme` names: one
  /// of the three `snapfire_fsr_session` ships or the application's own
  /// `CsrfScheme`.
  pub fn csrf(mut self, scheme: Arc<dyn CsrfScheme>) -> Self {
    self.csrf = Some(scheme);
    self
  }

  /// The cookie codec, in place of `HmacCodec` over the configuration's keys:
  /// the application's own signing or encryption of the session id.
  /// `session.key` and `session.previous_keys` then feed only a `derived`
  /// CSRF scheme. A builder given one is not rebuildable by a sites reload.
  pub fn codec(mut self, codec: Arc<dyn CookieCodec>) -> Self {
    self.codec = Some(codec);
    self
  }

  /// The keyring the stock codec and a `derived` scheme sign with, held and
  /// rotated by the application, in place of one built from `session.key`
  /// and `session.previous_keys`; a reload leaves it alone. A builder given
  /// one is not rebuildable by a sites reload.
  pub fn keyring(mut self, ring: Arc<Keyring>) -> Self {
    self.keyring = Some(ring);
    self
  }

  pub fn session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
    self.store = Some(store);
    self
  }

  /// Who may follow which topic on `/_sf/live`, asked once per topic as a
  /// stream opens; a refusal is 403 naming the topic. Without a rule any
  /// topic may be followed by anyone, which is right for a board on a wall
  /// and wrong for a room.
  pub fn topics<F>(mut self, rule: F) -> Self
  where
    F: Fn(&str, &SessionCell, Option<&Identity>) -> bool + Send + Sync + 'static,
  {
    self.topic_rule = Some(Arc::new(rule));
    self
  }

  /// The socket registry to use, for an application that must hold it before
  /// the host exists: a transport of its own pushes into the same one the
  /// host serves from. Without this the host makes its own.
  #[cfg(feature = "ws")]
  pub fn sockets(mut self, sockets: Arc<socket::Sockets>) -> Self {
    self.sockets = Some(sockets);
    self
  }

  /// What the application makes of what a page sends over `/_sf/socket`:
  /// called when a connection joins a topic, once per row it sends and when
  /// it leaves and whatever it answers goes out to that topic as store rows.
  /// Without one the endpoint is a 404. `HostBuilder::topics` still decides
  /// who may open the socket at all.
  #[cfg(feature = "ws")]
  pub fn socket<F>(mut self, handler: F) -> Self
  where
    F: Fn(&socket::Who, socket::On) -> socket::Reply + Send + Sync + 'static,
  {
    self.socket_handler = Some(Arc::new(handler));
    self
  }

  /// Negotiates HTTP/2 as well as HTTP/1.1 on a served connection, which
  /// `server.http2` also sets. The listener carries no TLS, so this is h2c.
  pub fn http2(mut self, on: bool) -> Self {
    self.http2 = Some(on);
    self
  }

  /// The identity provider behind `/auth/login`, `/auth/callback` and
  /// `/auth/logout`, in place of the one `[auth]` names. The login page is
  /// `auth.login` when the section is written, `/login` otherwise.
  pub fn identity(mut self, provider: Arc<dyn IdentityProvider>) -> Self {
    self.identity = Some(provider);
    self
  }

  /// The Rust half of a native pair: `name` is `module.member`, the name its
  /// `native(..)` declaration under `ext/` gives and `reach` what that
  /// declaration says. A plan calling a name nothing registers refuses to build.
  pub fn extension<F>(mut self, name: impl Into<String>, reach: Reach, f: F) -> Self
  where
    F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static,
  {
    let name = name.into();
    self.app_mut(move |app| app.extension(name, reach, f));
    self
  }

  /// The evaluator for the document module, replacing the stock shell.
  /// Where prerendered documents are read from, overriding `server.prerender`.
  pub fn prerendered(mut self, dir: impl Into<PathBuf>) -> Self {
    self.prerendered = Some(dir.into());
    self
  }

  pub fn shell(mut self, evaluator: Arc<dyn Evaluator>) -> Self {
    self.shell = Some(evaluator);
    self
  }

  pub fn route(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.route(pattern, plan));
    self
  }

  pub fn not_found(mut self, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.not_found(plan));
    self
  }

  pub fn middleware<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.middleware(f));
    self
  }

  pub fn middleware_override<F, Fut>(mut self, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.middleware_override(f));
    self
  }

  pub fn handler<F, Fut>(mut self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.handler(method, pattern, f));
    self
  }

  pub fn handler_override<F, Fut>(mut self, method: impl Into<String>, pattern: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.handler_override(method, pattern, f));
    self
  }

  pub fn route_override(mut self, pattern: impl Into<String>, plan: impl IntoPlan) -> Self {
    self.app_mut(|app| app.route_override(pattern, plan));
    self
  }

  pub fn source<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    self.app_mut(|app| app.source(name, f));
    self
  }

  pub fn source_override<F, Fut>(mut self, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Data, LoadError>> + Send + 'static,
  {
    self.app_mut(|app| app.source_override(name, f));
    self
  }

  /// The trace collector this host serves `/__fsr/traces` from under
  /// development. `trace::install` returns what goes here.
  pub fn traces(mut self, traces: Option<trace::Traces>) -> Self {
    self.traces = traces;
    self
  }

  /// Registers the application's own Rust under the name a body reaches it
  /// with, `ctx.native.<name>.<method>()`.
  pub fn native(mut self, name: impl Into<String>, module: Arc<dyn snapfire_fsr_runtime::Native>) -> Self {
    let name = name.into();
    self.app_mut(move |app| app.native(name, module));
    self
  }

  pub fn source_impl(mut self, name: impl Into<String>, source: Arc<dyn DataSource>) -> Self {
    self.app_mut(|app| app.source_impl(name, source));
    self
  }

  /// Describes the segment whose data source is `name`, title and
  /// description, once its data has loaded; the innermost described segment
  /// on a plan wins.
  pub fn meta(mut self, name: impl Into<String>, meta: Arc<dyn Metadata>) -> Self {
    self.app_mut(|app| app.meta(name, meta));
    self
  }

  pub fn action<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.action(id, f));
    self
  }

  /// One handler of an island a template renders. `module` is what the
  /// placement names and `name` is what its markup binds with
  /// `data-sf-on="click:<name>"`; the handler answers with the state the
  /// module is rendered from next.
  pub fn island_handler<F, Fut>(mut self, module: impl Into<String>, name: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, IslandEvent) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.island_handler(module, name, f));
    self
  }

  pub fn action_override<F, Fut>(mut self, id: impl Into<String>, f: F) -> Self
  where
    F: Fn(RequestCtx, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, ActionError>> + Send + 'static,
  {
    self.app_mut(|app| app.action_override(id, f));
    self
  }

  pub fn evaluator<P>(mut self, predicate: P, evaluator: Arc<dyn Evaluator>) -> Self
  where
    P: Fn(&ModuleId) -> bool + Send + Sync + 'static,
  {
    self.app_mut(|app| app.evaluator(predicate, evaluator));
    self
  }

  /// Mounts a site under the prefix its own `[site]` names: its routes join
  /// the shell's under the shell's root layout, its ids stay prefixed, its
  /// clients register under `<name>:`, its middleware runs after the shell's.
  pub fn mount(mut self, mount: Mount) -> Self {
    self.mounts.push(mount);
    self
  }

  /// The configuration this builder was made from.
  pub fn config(&self) -> &Config {
    &self.artifact.config
  }

  /// How the artifact was read, `None` for a builder made from one in memory.
  /// A sites mounter reads each site through `Loader::mount` on it, so a
  /// site's secrets decrypt the way the shell's do.
  pub fn loader(&self) -> Option<&Loader> {
    self.loader.as_ref()
  }

  /// How `Host::reload` rebuilds the tables: a builder for the application as
  /// it stands on disk, with whatever this builder was given added again.
  /// How the sites are mounted again on [`Host::reload_sites`]. The step is
  /// handed a builder the host made from the shell's boot inputs and returns it
  /// with the sites of the moment mounted; it never makes a builder of its own,
  /// so it cannot reread the shell's configuration or plan.
  pub fn sites_mounter<F>(mut self, f: F) -> Self
  where
    F: Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync + 'static,
  {
    self.sites_mounter = Some(Box::new(f));
    self
  }

  pub fn reloader<F>(mut self, f: F) -> Self
  where
    F: Fn() -> Result<HostBuilder, HostError> + Send + Sync + 'static,
  {
    self.reloader = Some(Box::new(f));
    self
  }

  pub fn build(mut self) -> Result<Host, HostError> {
    let traces = self.traces.take();
    let reloader = self.reloader.take();
    let chosen_csrf = self.csrf.take();
    let chosen_codec = self.codec.take();
    let chosen_ring = self.keyring.take();
    let sites_mounter = self.sites_mounter.take();
    // A sites reload rebuilds the shell from these three alone, so a builder
    // carrying anything else they cannot reproduce keeps none of them and the
    // call is refused by name rather than quietly dropping what it held.
    let rebuildable = self.services.is_none()
      && self.transport_override.is_none()
      && self.store.is_none()
      && self.shell.is_none()
      && self.prerendered.is_none()
      && self.identity.is_none()
      && chosen_csrf.is_none()
      && chosen_codec.is_none()
      && chosen_ring.is_none()
      && !self.hand_built;
    let shell_inputs = rebuildable.then(|| Arc::new(self.artifact.clone()));
    let loader = self.loader.take();
    let mounted = !self.mounts.is_empty();
    let store = self.store.take();
    let topic_rule = self.topic_rule.take();
    #[cfg(feature = "ws")]
    let socket_handler = self.socket_handler.take();
    #[cfg(feature = "ws")]
    let sockets = self.sockets.take().unwrap_or_else(|| Arc::new(socket::Sockets::new()));
    let http2 = self.http2;
    let (tables, mut config) = self.assemble()?;
    if config.session.key.is_none() && config.dev() {
      config.session.key = Some(snapfire_fsr_session::random_token());
    }
    if config.session.key.as_deref() == Some(config::SESSION_KEY_PLACEHOLDER) {
      return Err(placeholder_session_key(&config));
    }
    let ttl = config.session_ttl()?;
    let store: Arc<dyn SessionStore> = match store {
      Some(store) => store,
      None => match config.session.store.as_str() {
        "memory" => Arc::new(MemorySessionStore::new(config.session.capacity)),
        "service" => {
          let client = format!(
            "{}{}",
            config.site.as_ref().map(SiteSection::prefix).unwrap_or_default(),
            config.session.client.clone().unwrap_or_default()
          );
          Arc::new(ServiceSessionStore::new(tables.app.services.clone(), client, ttl))
        }
        other => return Err(HostError::Value("session.store".to_owned(), other.to_owned())),
      },
    };
    let (ring, owned_ring) = match chosen_ring {
      Some(ring) => (ring, None),
      None => {
        if config.session.key.is_none() {
          return Err(missing_session_key(&config));
        }
        let ring = Arc::new(Keyring::from_keys(session_keys(&config)));
        (ring.clone(), Some(ring))
      }
    };
    let scheme: Arc<dyn CsrfScheme> = match chosen_csrf {
      Some(scheme) => scheme,
      None => match config.session.csrf_scheme.as_str() {
        "derived" => Arc::new(snapfire_fsr_session::Derived::over(ring.clone())),
        "session" => Arc::new(snapfire_fsr_session::PerSession::new()),
        _ => Arc::new(snapfire_fsr_session::SingleUse::new(config.session.csrf_outstanding as usize)),
      },
    };
    let codec: Arc<dyn CookieCodec> = match chosen_codec {
      Some(codec) => codec,
      None => Arc::new(HmacCodec::over(ring.clone())),
    };
    let sessions = Sessions::with_codec(
      store,
      config.session.key.as_deref().unwrap_or_default().as_bytes(),
      codec,
      SessionConfig {
        ttl,
        secure: config.session.secure,
        ..SessionConfig::default()
      },
    )
    .with_csrf(scheme);
    let changed = config.dev().then(|| Reload::new(16));
    let http2 = http2.unwrap_or(config.server.http2);
    #[cfg(not(feature = "tls"))]
    if config.server.tls.is_some() {
      return Err(HostError::Config(
        config.root.clone(),
        "[server.tls] needs the host's `tls` feature, which is off by default".to_owned(),
      ));
    }
    #[cfg(feature = "tls")]
    let (tls, tls_reload) = match &config.server.tls {
      Some(section) => {
        let reload = match section.reload.as_str() {
          "none" => None,
          name @ ("hup" | "usr1" | "usr2") => Some(name.to_owned()),
          other => {
            let why = format!("server.tls.reload `{other}` is not a signal; hup, usr1, usr2 or none");
            return Err(HostError::Config(config.root.clone(), why));
          }
        };
        let (cert, key) = section.files(&config.root);
        let alpn = section.alpn.clone().unwrap_or_else(|| default_alpn(http2));
        (Some(Arc::new(tls::Tls::load(cert, key, alpn)?)), reload)
      }
      None => (None, None),
    };
    Ok(Host {
      traces,
      live: parking_lot::RwLock::new(Arc::new(tables)),
      sessions,
      changed,
      topics: Topics::new(64),
      topic_rule,
      #[cfg(feature = "ws")]
      sockets,
      #[cfg(feature = "ws")]
      socket_handler,
      reloader,
      loader,
      shell_inputs,
      mounted,
      sites_mounter,
      #[cfg(feature = "sites_reload")]
      sites_reload: parking_lot::Mutex::new(()),
      csrf_always: config.session.csrf == "always",
      session_shape: session_shape(&config),
      keyring: owned_ring,
      trace_ring: ring,
      max_body: config.server.max_body,
      max_upload: config.server.max_upload,
      hosts: config.server.hosts.iter().map(|h| h.to_lowercase()).collect(),
      origin: config.origin()?,
      http2,
      #[cfg(feature = "tls")]
      tls,
      #[cfg(feature = "tls")]
      tls_reload,
      report_listen: config.server.listen,
    })
  }

  /// Everything but the sessions: the tables a request reads, checked the
  /// way a boot checks them and the configuration they came from.
  fn assemble(mut self) -> Result<(Tables, Config), HostError> {
    if let Some(e) = self.pending.take() {
      return Err(e);
    }
    let Artifact { config, plan, contract: mut own_contract } = self.artifact;

    config.session_ttl()?;
    if !matches!(config.session.store.as_str(), "memory" | "service") {
      return Err(HostError::Value(
        "session.store".to_owned(),
        config.session.store.clone(),
      ));
    }
    leaks(&config, &plan)?;

    let mut service_rows = Vec::new();
    let mut bearer_rows: Vec<(String, String)> = Vec::new();
    let mut contract = own_contract.clone().unwrap_or_default();
    let mut transports: Vec<(String, Arc<dyn Transport>)> = Vec::new();
    let build_clients = self.services.is_none();
    if build_clients {
      clients_of(
        &config,
        self.transport_override.is_none(),
        &mut contract,
        &mut transports,
        &mut service_rows,
        &mut bearer_rows,
      )?;
    }
    let mut rust_transports: Vec<(String, Arc<dyn Transport>)> = Vec::new();
    for rust in std::mem::take(&mut self.rust_services) {
      contract.adopt(rust.contract, &rust.rust_type).map_err(|e| {
        HostError::Service(rust.name.clone(), format!("{e}; the contracts directory and the Rust disagree, so run fsr build"))
      })?;
      service_rows.push((rust.name.clone(), "rust".to_owned(), rust.rust_type));
      rust_transports.push((rust.name, rust.transport));
    }

    let manifest =
      Manifest::from_text(&plan).map_err(|e| HostError::Config(config.resolve(&config.server.plan), e.to_string()))?;
    let shell_module = config.document.shell.clone();
    let mut app = self.app.take().expect("the builder holds its app until build");
    let mut taken: Vec<String> = manifest.routes.iter().map(|r| r.pattern.clone()).collect();
    let mut sites = Vec::new();
    let mut site_reports = Vec::new();
    let mut statics: Vec<StaticRootResolved> = config
      .statics
      .iter()
      .map(|root| StaticRootResolved {
        route: root.route.trim_end_matches('/').to_owned(),
        dir: config.resolve(&root.dir),
      })
      .collect();
    if let Some(dir) = &config.shell_vendor {
      if !statics.iter().any(|s| s.route == "/static/js/vendor") {
        statics.push(StaticRootResolved {
          route: "/static/js/vendor".to_owned(),
          dir: dir.clone(),
        });
      }
    }
    let serve_client = !statics.iter().any(|s| s.route == client::ROUTE);
    let client_minified = config.document.client.minified(config.dev());
    let mut import_map = match &config.document.import_map {
      Some(rel) => {
        let path = config.resolve(rel);
        let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?;
        Some(bundle_urls(&client_urls(&text, serve_client && client_minified), minified_bundle(&config)))
      }
      None => None,
    };
    let preload = match config.document.module_preload {
      true => preload_set(&config, import_map.as_deref(), minified_bundle(&config)),
      false => Vec::new(),
    };
    for mount in std::mem::take(&mut self.mounts) {
      let site = mount.artifact.config.site.clone().ok_or_else(|| {
        HostError::Mount(
          mount.name.clone(),
          "the artifact's configuration has no [site] section".to_owned(),
        )
      })?;
      if site.name != mount.name {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!("the artifact is the site `{}`", site.name),
        ));
      }
      if sites.iter().any(|s: &SiteTables| s.at == site.at)
        || taken
          .iter()
          .any(|r| *r == site.at || r.starts_with(&format!("{}/", site.at)))
      {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!("`{}` is already served", site.at),
        ));
      }
      let mut site_manifest =
        Manifest::from_text(&mount.artifact.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      let engine_rows: Vec<String> = site_manifest
        .sources
        .iter()
        .filter(|r| r.owner == RowOwner::Engine)
        .map(|r| r.id.clone())
        .chain(
          site_manifest
            .actions
            .iter()
            .filter(|r| r.owner == RowOwner::Engine)
            .map(|r| r.id.clone()),
        )
        .chain(
          site_manifest
            .handlers
            .iter()
            .filter(|r| r.owner == RowOwner::Engine)
            .map(|r| r.id.clone()),
        )
        .collect();
      if !engine_rows.is_empty() && !mount.allow_engine {
        return Err(HostError::Mount(
          mount.name.clone(),
          format!(
            "engine-owned rows {}; set allow_engine = true to mount them",
            engine_rows.join(", ")
          ),
        ));
      }
      leaks(&mount.artifact.config, &mount.artifact.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      let middleware = site_manifest.middleware.take().map(snapfire_fsr::middleware_from);
      graft(&manifest, &mut site_manifest, &shell_module);
      site_manifest.not_found = None;
      taken.extend(site_manifest.routes.iter().map(|r| r.pattern.clone()));
      app
        .mount_manifest(&site_manifest.to_json())
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      if let Some(site_contract) = &mount.artifact.contract {
        contract
          .merge(site_contract.clone(), &format!("site {}", mount.name))
          .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      if build_clients {
        clients_of(
          &mount.artifact.config,
          self.transport_override.is_none(),
          &mut contract,
          &mut transports,
          &mut service_rows,
          &mut bearer_rows,
        )
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      let mut ignored = Vec::new();
      for root in &mount.artifact.config.statics {
        let route = root.route.trim_end_matches('/').to_owned();
        if route.starts_with(&site.at) && !statics.iter().any(|s| s.route == route) {
          statics.push(StaticRootResolved {
            route,
            dir: mount.artifact.config.resolve(&root.dir),
          });
        } else {
          ignored.push(format!("static {route}"));
        }
      }
      for section in ["session", "auth", "locales", "cache"] {
        let set = match section {
          "session" => true,
          "auth" => mount.artifact.config.auth.is_some(),
          "locales" => mount.artifact.config.locales.is_some(),
          _ => mount.artifact.config.cache.is_some(),
        };
        if set {
          ignored.push(section.to_owned());
        }
      }
      if let Some(rel) = &mount.artifact.config.document.import_map {
        let path = mount.artifact.config.resolve(rel);
        let theirs = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?;
        let merged = merge_import_maps(import_map.as_deref(), &theirs);
        let merged = bundle_urls(&merged, minified_under(mount.artifact.config.bundle.as_ref(), config.dev()));
        import_map = Some(client_urls(&merged, serve_client && client_minified));
      }
      site_reports.push(SiteReport {
        name: mount.name.clone(),
        at: site.at.clone(),
        artifact: mount.artifact.config.root.clone(),
        version: mount.version.clone(),
        hash: mount.hash.clone(),
        ignored,
      });
      let site_bundle = minified_under(mount.artifact.config.bundle.as_ref(), config.dev());
      sites.push(SiteTables {
        name: mount.name.clone(),
        at: site.at.clone(),
        middleware,
        styles: minified_urls(mount.artifact.config.document.styles.clone().unwrap_or_default(), site_bundle),
        entry: mount.artifact.config.document.entry.as_deref().map(|entry| minified_url(entry, site_bundle)),
        preload: match config.document.module_preload {
          true => {
            let mut theirs = preload_set(&mount.artifact.config, import_map.as_deref(), site_bundle);
            theirs.retain(|url| !preload.contains(url));
            theirs
          }
          false => Vec::new(),
        },
      });
    }
    sites.sort_by(|a, b| b.at.len().cmp(&a.at.len()).then(a.at.cmp(&b.at)));
    let app_contract = if site_reports.is_empty() {
      own_contract.take()
    } else {
      Some(contract.clone())
    };

    let services = match self.services {
      Some(services) => services,
      None => {
        let mut builder = Services::builder()
          .contract(contract)
          .intercept(Arc::new(TraceInterceptor::new()))
          .intercept(Arc::new(IdentityInterceptor::new()));
        if let Some(cache) = &config.cache {
          if let Some(data) = &cache.data {
            builder = builder.data_cache(data.capacity.unwrap_or(cache.capacity));
          }
        }
        let mut by_key: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for (client, key) in &bearer_rows {
          by_key.entry(key.clone()).or_default().push(client.clone());
        }
        for (key, clients) in by_key {
          builder = builder.intercept(Arc::new(CredentialInterceptor::bearer(key).only(clients)));
        }
        match &self.transport_override {
          Some(transport) => builder = builder.default_transport(transport.clone()),
          None => {
            for (name, transport) in transports {
              builder = builder.transport(name, transport);
            }
          }
        }
        for (name, transport) in rust_transports {
          builder = builder.transport(name, transport);
        }
        builder
          .try_build()
          .map_err(|e| HostError::Value("cache.data".to_owned(), e.to_string()))?
      }
    };

    let shell_path = config.document.shell.split('#').next().unwrap_or("shell").to_owned();
    let shell: Arc<dyn Evaluator> = self.shell.take().unwrap_or_else(|| Arc::new(shell::DocumentShell { slot_order: config.store.slot_order.clone() }));
    app = app.slot_order(config.store.slot_order.clone());
    if let Some(contract) = app_contract {
      app = app.contract(contract);
    }
    let (live, cache_row): (Arc<dyn NodeCache>, Option<(u64, String)>) = match (config.cache_ttl()?, &config.cache) {
      (Some(ttl), Some(section)) => (Arc::new(FibreCache::bounded(section.capacity, ttl)), Some((section.capacity, section.ttl.clone()))),
      _ => (Arc::new(NoCache), None),
    };
    let locales = match &config.locales {
      Some(section) => Locales::from_section(section).map_err(|e| {
        HostError::Config(
          config.sources.first().cloned().unwrap_or_else(|| config.root.clone()),
          e,
        )
      })?,
      None => Locales::single(),
    };
    let catalogs = Arc::new(
      locale::load_catalogs(&config.app, &locales.default).map_err(|e| HostError::Config(config.root.clone(), e))?,
    );
    if !catalogs.is_empty() {
      app = app.catalogs(catalogs.clone());
    }
    let catalog_rows = catalogs.rows();
    app = app.bearer_services(bearer_rows.iter().map(|(client, _)| client.clone()));
    let prerendered = self
      .prerendered
      .take()
      .or_else(|| config.server.prerender.as_deref().map(|rel| config.resolve(rel)));
    let warm = Arc::new(WarmLoads::new(
      prerendered
        .as_ref()
        .map(|dir| warm_from_file(&dir.join(LOADS_FILE)))
        .unwrap_or_default(),
    ));
    let warmed_count = warm.len();
    app = app.loads(warm.clone());
    let renders = Arc::new(WarmRenders::new(
      prerendered
        .as_ref()
        .map(|dir| renders_from_file(&dir.join(RENDERS_FILE)))
        .unwrap_or_default(),
      live,
    ));
    let rendered_count = renders.len();
    app = app.cache(renders.clone());
    let extension_rows: Vec<String> = app
      .extensions()
      .names()
      .into_iter()
      .filter(|name| {
        !snapfire_fsr_ir::STANDARD
          .iter()
          .any(|(m, n, _)| format!("{m}.{n}") == *name)
      })
      .collect();
    let app = app.services(services);
    #[cfg(not(feature = "tera"))]
    let stock_templates: Option<Vec<String>> = None;
    let assets_manifest = crate::assets::AssetsManifest::read(&config.app);
    #[cfg(feature = "tera")]
    let (app, stock_templates) = match tera::evaluator(&config.app, assets_manifest.as_ref())? {
      Some((evaluator, names)) if !app.covers(&ModuleId::new("probe.tera", "default")) => (app.evaluator(is_template_module, Arc::new(evaluator)), Some(names)),
      _ => (app, None),
    };
    let app = app.evaluator(move |m: &ModuleId| m.path == shell_path, shell).build()?;
    let unanswered_routes: Vec<String> = config.routes.iter().filter(|declared| !app.report.routes.iter().any(|(pattern, _)| pattern == &declared.pattern)).map(|declared| declared.pattern.clone()).collect();
    for (index, _) in app.patterns.iter().enumerate() {
      let Some(plan) = app.resolver.resolve(snapfire_fsr_runtime::EntryId(index as u32), &Params::new()) else { continue };
      templates_answered(&plan, &app.runtime.evaluators, stock_templates.as_deref())?;
    }
    for plan in app.intercepts.all().chain(app.not_found.iter()) {
      templates_answered(plan, &app.runtime.evaluators, stock_templates.as_deref())?;
    }

    let styles = minified_urls(config.document.styles.clone().unwrap_or_default(), minified_bundle(&config));
    let entry = config.document.entry.as_deref().map(|entry| minified_url(entry, minified_bundle(&config)));
    let mut head = shell::head(
      config.document.title.literal(),
      &styles,
      import_map.as_deref(),
      &preload,
      entry.as_deref(),
    );
    head.template = config.document.title_template.as_ref().map(|template| template.literal().to_owned());
    head.head = config.document.head_meta()?.head;
    let font_css = assets_manifest.as_ref().map(|m| m.fonts.css.clone()).filter(|css| !css.is_empty());
    if let Some(manifest) = &assets_manifest {
      head.head.extend(asset_head_rows(manifest));
    }
    head.origin = config.origin()?;
    let dev = config.dev();
    let dev_bundle = dev.then(|| config.app.join("dist/.snapfire-build.json"));

    // The development refresh script is inline and carries the bundle id it was
    // rendered against, so its text changes per request and no source computed
    // at boot covers it.
    // The host owns the sources only it can know: the inline import map's hash,
    // and in development the nonce on its own refresh script, whose text carries
    // the bundle id it was rendered against and so has no stable hash.
    let dev_nonce = dev.then(dev_nonce);
    let empty_icon = config.document.head.iter().any(|t| t.get("href").is_some_and(|href| href == "data:,"));
    let compose = |declared: &config::Csp| {
      let mut policy = declared.clone();
      if empty_icon {
        policy.widen("img-src", "data:");
      }
      if let Some(map) = import_map.as_deref() {
        policy.widen("script-src", import_map_csp(map));
      }
      if let Some(nonce) = &dev_nonce {
        policy.widen("script-src", format!("'nonce-{nonce}'"));
      }
      if let Some(css) = &font_css {
        policy.widen("style-src", import_map_csp(css));
      }
      // A rendered `style` attribute has no hash a boot-time policy can name,
      // and a fragment swapped in by client navigation is checked against the
      // first document's policy, so attributes are admitted on their own
      // directive; `<style>` and `<script>` stay as declared.
      if !policy.declares("style-src-attr") && (policy.declares("style-src") || policy.declares("default-src")) {
        policy.add("style-src-attr", "'unsafe-inline'");
      }
      if let Some(manifest) = &assets_manifest {
        for (directive, source) in asset_csp_sources(manifest) {
          policy.widen(directive, source);
        }
      }
      policy.header().and_then(|text| HeaderValue::from_str(&text).ok())
    };
    let csp = config.document.csp.as_ref().and_then(&compose);
    let csp_report_only = config.document.csp_report_only.as_ref().and_then(&compose);
    let static_rows: Vec<(String, PathBuf)> = statics.iter().map(|s| (s.route.clone(), s.dir.clone())).collect();
    // Longest route first, so the most specific root answers a path whatever
    // order the file, the inference and the mounts named them in.
    statics.sort_by(|a, b| b.route.len().cmp(&a.route.len()));
    let public: ValueMap = {
      let mut fields = snapfire_fsr_core::Fields::default();
      for (key, value) in &config.public {
        fields.insert(key.clone(), value.to_value());
      }
      fields.into()
    };
    let statics: Vec<(String, ServeDir)> = statics.into_iter().map(|s| (s.route, ServeDir::new(s.dir))).collect();
    let static_cache = match config.server.static_max_age {
      None => Some(HeaderValue::from_static("no-cache")),
      Some(0) => None,
      Some(seconds) => HeaderValue::from_str(&format!("public, max-age={seconds}")).ok(),
    };

    let locale_rows = match &config.locales {
      Some(_) => {
        let mut rows = vec![locales.default.clone()];
        rows.extend(locales.supported.iter().filter(|t| **t != locales.default).cloned());
        rows
      }
      None => Vec::new(),
    };
    let prefix = config.site.as_ref().map(SiteSection::prefix).unwrap_or_default();
    let auth = match (self.identity.take(), &config.auth) {
      (Some(provider), section) => {
        let login_path = section
          .as_ref()
          .map(|s| s.login.clone())
          .unwrap_or_else(|| "/login".to_owned());
        Some((
          Mounted {
            auth: Auth::new(provider),
            login_path,
          },
          "custom".to_owned(),
        ))
      }
      (None, Some(section)) => {
        let provider: Arc<dyn IdentityProvider> = match section.provider.as_str() {
          "file" => {
            let users = config
              .config_dir()
              .join(section.users.as_deref().unwrap_or("auth.toml"));
            Arc::new(DevProvider::from_toml(&section.login, &users).map_err(|e| HostError::Config(users.clone(), e))?)
          }
          "service" => {
            let client = format!("{prefix}{}", section.client.clone().unwrap_or_default());
            Arc::new(ServiceProvider::new(
              app.services.clone(),
              client,
              section.login.clone(),
            ))
          }
          other => return Err(HostError::Value("auth.provider".to_owned(), other.to_owned())),
        };
        let name = match &section.client {
          Some(client) if section.provider == "service" => format!("service via {client}"),
          _ => section.provider.clone(),
        };
        Some((
          Mounted {
            auth: Auth::new(provider),
            login_path: section.login.clone(),
          },
          name,
        ))
      }
      (None, None) => None,
    };
    let auth_row = auth
      .as_ref()
      .map(|(mounted, name)| (name.clone(), mounted.login_path.clone()));
    let auth = auth.map(|(mounted, _)| mounted);
    let report = HostReport {
      app: app.report.clone(),
      unanswered_routes,
      http2: self.http2.unwrap_or(config.server.http2),
      tls: config.server.tls.as_ref().map(|section| {
        let (cert, key) = section.files(&config.root);
        TlsReport {
          cert,
          key,
          alpn: section
            .alpn
            .clone()
            .unwrap_or_else(|| default_alpn(self.http2.unwrap_or(config.server.http2))),
          reload: (section.reload != "none").then(|| section.reload.clone()),
        }
      }),
      services: service_rows,
      session: (config.session.store == "service").then(|| config.session.client.clone().unwrap_or_default()),
      cached: app
        .services
        .data_cache()
        .map(|cache| {
          cache
            .policies()
            .into_iter()
            .map(|(method, f)| {
              let mut policy = format!("ttl {} {}", f.ttl, f.scope.as_str());
              if let Some(stale) = &f.stale {
                policy.push_str(&format!(", stale {stale}"));
              }
              if !f.tags.is_empty() {
                policy.push_str(&format!(" [{}]", f.tags.join(", ")));
              }
              (method, policy)
            })
            .collect()
        })
        .unwrap_or_default(),
      writers: app
        .services
        .data_cache()
        .map(|cache| {
          cache
            .writers()
            .into_iter()
            .map(|(method, tags)| (method, format!("[{}]", tags.join(", "))))
            .collect()
        })
        .unwrap_or_default(),
      statics: static_rows,
      client: serve_client.then(|| (client::ROUTE, client::FILES.len(), client::bytes(client_minified), client_minified)),
      prerender: prerendered.clone(),
      warmed: warmed_count,
      rendered: rendered_count,
      cache: cache_row,
      dev,
      locales: locale_rows,
      hosts: config.server.hosts.iter().map(|h| h.to_lowercase()).collect(),
      origin: config.origin()?,
      catalogs: catalog_rows,
      auth: auth_row,
      bearer: bearer_rows,
      extensions: extension_rows,
      site: config.site.as_ref().map(|s| (s.name.clone(), s.at.clone())),
      sites: site_reports,
      import_map_csp: import_map.as_deref().map(import_map_csp),
      csp: csp.as_ref().and_then(|v| v.to_str().ok()).map(str::to_owned),
      csp_report_only: csp_report_only.as_ref().and_then(|v| v.to_str().ok()).map(str::to_owned),
      config: config.sources.clone(),
      inferred: config.inferred.clone(),
      public: config.public.iter().map(|(k, v)| (k.clone(), v.to_string())).collect(),
      ignored: config.ignored.clone(),
    };
    Ok((
      Tables {
        app,
        head,
        title: config.document.title.clone(),
        title_template: config.document.title_template.clone(),
        public,
        origin: config.origin()?,
        dev_bundle,
        statics,
        favicon: config.favicon.clone(),
        static_cache,
        client: serve_client,
        client_minified,
        csp,
        csp_report_only,
        dev_nonce,
        trace_expose: config.trace.as_ref().map(|t| t.expose.clone()).unwrap_or_default(),
        prerendered,
        warm,
        renders,
        locales,
        catalogs,
        auth,
        sites,
        report: Arc::new(report),
      },
      config,
    ))
  }
}

/// The `[session]` settings as one string, compared across a reload.
fn session_shape(config: &Config) -> String {
  let s = &config.session;
  format!(
    "{} {:?} {} {} {} {:?}",
    s.store, s.client, s.ttl, s.secure, s.csrf, s.capacity
  )
}

/// What a trace token signs. The prefix keeps a token from verifying as any
/// other value the same ring signs, a session id among them.
fn trace_token_input(id: u64) -> String {
  format!("fsr.trace:{id}")
}

/// Whether `path` is `prefix` or sits beneath it; `/` covers every path.
fn under_prefix(path: &str, prefix: &str) -> bool {
  let prefix = prefix.trim_end_matches('/');
  prefix.is_empty() || path == prefix || path.strip_prefix(prefix).is_some_and(|rest| rest.starts_with('/'))
}

fn placeholder_session_key(config: &Config) -> HostError {
  HostError::Config(
    config.config_dir(),
    "`session.key` is still the placeholder; set C5_SESSION__KEY or replace it with a generated key".to_owned(),
  )
}

fn missing_session_key(config: &Config) -> HostError {
  HostError::Config(
    config.config_dir(),
    "`session.key` is not set outside development; set C5_SESSION__KEY or write it in a configuration file the host reads".to_owned(),
  )
}

/// `session.key` first, then `session.previous_keys` in order.
fn session_keys(config: &Config) -> Vec<Vec<u8>> {
  config
    .session
    .key
    .iter()
    .chain(config.session.previous_keys.iter())
    .map(|k| k.as_bytes().to_vec())
    .collect()
}

/// Refuses a bundle that carries a server module. The plan's sources,
/// actions and handlers name their modules, `middleware.ts` is implicit,
/// and `app/<path>.ts` bundles to `dist/<path>.js`; any such output in the
/// build facts or any output importing one, is a leak. No facts file means
/// no bundle to check.
fn leaks(config: &Config, plan: &str) -> Result<(), HostError> {
  let facts = config.app.join("dist/.snapfire-build.json");
  let Ok(text) = std::fs::read_to_string(&facts) else {
    return Ok(());
  };
  let json: serde_json::Value =
    serde_json::from_str(&text).map_err(|e| HostError::Config(facts.clone(), e.to_string()))?;
  let manifest = Manifest::from_text(plan)
    .map_err(|e| HostError::Config(config.resolve(&config.server.plan), e.to_string()))?;
  let plan = serde_json::to_value(&manifest).expect("a manifest serializes");
  let found = leaked_outputs(&plan, &json);
  if found.is_empty() {
    Ok(())
  } else {
    Err(HostError::Leak(found.join(", ")))
  }
}

fn server_output(module: &str) -> String {
  let stem = module
    .strip_suffix(".tsx")
    .or_else(|| module.strip_suffix(".ts"))
    .unwrap_or(module);
  format!("{stem}.js")
}

/// Every bundle output that is a server module or imports one, each with
/// the reason.
fn leaked_outputs(plan: &serde_json::Value, facts: &serde_json::Value) -> Vec<String> {
  let mut server: std::collections::BTreeMap<String, String> = Default::default();
  for (table, what) in [
    ("sources", "a loader"),
    ("actions", "an actions module"),
    ("handlers", "a route handler"),
  ] {
    for row in plan[table].as_array().into_iter().flatten() {
      if let Some(module) = row["module"].as_str() {
        server.insert(server_output(module), format!("{what}, {module}"));
      }
    }
  }
  if !plan["middleware"].is_null() {
    server.insert("middleware.js".to_owned(), "the middleware, middleware.ts".to_owned());
  }
  let mut found = Vec::new();
  for output in facts["outputs"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|o| o.as_str())
  {
    if let Some(reason) = server.get(output) {
      found.push(format!("{output} is {reason}"));
    }
  }
  if let Some(graph) = facts["graph"].as_object() {
    for (importer, imports) in graph {
      for imported in imports.as_array().into_iter().flatten().filter_map(|i| i.as_str()) {
        if server.contains_key(imported) {
          found.push(format!("{importer} imports {imported}"));
        }
      }
    }
  }
  found
}

impl Host {
  pub fn owner_of_source(&self, name: &str) -> Option<Owner> {
    self
      .report()
      .app
      .sources
      .iter()
      .find(|(n, _)| n == name)
      .map(|(_, o)| *o)
  }
}
