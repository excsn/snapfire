//! The stock host: `config/`, `generated/plan.json` and `generated/contracts/`
//! become a `tower::Service` over `http` types. hyper serves it, axum nests
//! it, actix reaches it through the `actix` feature's shim.

mod actions;
mod after;
pub mod assets;
mod auth;
mod builder;
mod bundle;
mod card;
mod crawl;
pub mod client;
pub mod config;
pub mod conformance;
mod edge;
mod live;
pub mod locale;
mod mount;
mod prerender;
mod pwa;
mod probe;
mod remote;
mod render;
mod report;
mod serve;
mod theme;
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

/// Where the build writes what each Tailwind class means on a card image, relative to the app.
pub const CARD_CLASSES_FILE: &str = "generated/tailwind.cards.json";

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
pub use actions::{IslandBody, IslandStep, island_body, island_step};
pub use auth::referer_path;
pub use builder::HostBuilder;
pub use edge::{middleware_request, percent_decoded};
pub use locale::{Locales, LocalesSection, Resolution};
pub use mount::{Artifact, Mount};
pub use prerender::{BUILD_FILE, LOADS_FILE, RENDERS_FILE, WRITTEN_FILE};
pub use probe::{Candidate, Probe, ReloadCheck};
pub use remote::{ServiceProvider, ServiceSessionStore};
pub use report::{Brief, CrawlReport, HostReport, SiteReport, TlsReport};
pub use serve::{HostService, shutdown_signal};
pub use snapfire_fsr_core::ext::{self, Ambient, Catalogs, Fail, FailureKind, Reach};

use actions::*;
use auth::*;
use builder::*;
use bundle::*;
use edge::*;
use live::*;
use mount::*;
use prerender::*;
use render::*;
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
#[non_exhaustive]
pub enum HostError {
  #[error("{0}: {1}")]
  Io(PathBuf, std::io::Error),
  #[error("{0}: {1}")]
  Config(PathBuf, String),
  #[error("no configuration under {0}: expected `config/`, or `app.toml` beside it")]
  NoConfig(PathBuf),
  #[error("{0}")]
  Sitemap(String),
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
  #[error("reload refused by a probe: {0}")]
  Probe(String),
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
#[non_exhaustive]
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

/// Whether `raw_query` asks to preview the prerendered page under `dev`.
pub fn preview_of(raw_query: &str) -> bool {
  form_urlencoded::parse(raw_query.as_bytes()).any(|(key, _)| key == "__prerendered")
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
  sites_reload: tokio::sync::Mutex<()>,
  /// What a Rust host checks a candidate with before a reload swaps it in.
  reload_checks: Vec<Arc<dyn ReloadCheck>>,
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
  /// `server.shutdown_grace`.
  shutdown_grace: std::time::Duration,
  /// Set by `begin_drain`, which turns `server.ready` to 503.
  draining: std::sync::atomic::AtomicBool,
  /// The work requests left to run after their responses, still running.
  afters: Arc<after::AfterTasks>,
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
  /// The document's own stylesheets as its head links them, which a dev event
  /// names so an open page can add a sheet a build added.
  styles: Vec<String>,
  /// The stylesheets each route segment's module links on the documents that render it, by URL.
  sheets: HashMap<String, Vec<String>>,
  statics: Vec<(String, ServeDir, PathBuf)>,
  /// The static path `/favicon.ico` is looked up as, `config.favicon`.
  favicon: Option<String>,
  /// The `Cache-Control` a static answer carries, from `server.static_max_age`.
  /// Absent when that is `0` or when `dev` is on, which sends `no-cache` instead.
  static_cache: Option<HeaderValue>,
  /// Per bundle route served outside development, each hashed name against the output it names.
  hashed_names: Vec<(String, HashMap<String, String>)>,
  /// The manifest and worker `[pwa]` answers.
  pwa: Option<pwa::Pwa>,
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
  /// `server.health` and `server.ready`, `None` when empty.
  health: Option<String>,
  ready: Option<String>,
  prerendered: Option<PathBuf>,
  /// What generated card images are drawn with; `None` when the plan has none.
  cards: Option<card::Cards>,
  /// The build `prerender::build_of` names for these tables, which a
  /// prerender writes beside its documents.
  build: String,
  /// Whether `prerendered` holds this build's documents: false when the
  /// directory was written by another build, until a `prerender` from this
  /// one replaces them.
  prerender_current: std::sync::atomic::AtomicBool,
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
  /// `[crawl]` against this host, when configured.
  crawl: Option<crawl::Crawl>,
  /// Every mounted site's `[site] probe` list, by site name.
  probes: Vec<(String, Probe)>,
  report: Arc<HostReport>,
  /// The shell's lowered `theme.ts`.
  theme: Option<Arc<dyn ActionHandler>>,
  /// The theme each layout or page sets, the mounted sites' among them: its name and whether it
  /// is forced.
  themes: HashMap<String, (String, bool)>,
}

/// What a request under a site's prefix reads beyond the merged tables: the
/// site's middleware, run after the shell's, plus what its documents add
/// to the head.
struct SiteTables {
  name: String,
  at: String,
  middleware: Option<Arc<dyn ActionHandler>>,
  /// The site's lowered `theme.ts`, run after the shell's.
  theme: Option<Arc<dyn ActionHandler>>,
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

  fn entry(&self) -> Option<String> {
    self.entry.clone()
  }

  fn styles(&self) -> Vec<String> {
    self.styles.clone()
  }
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
      reload_checks: Vec::new(),
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
