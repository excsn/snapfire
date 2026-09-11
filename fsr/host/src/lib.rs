//! The stock host: `config/`, `generated/plan.json` and `generated/contracts/`
//! become a `tower::Service` over `http` types. hyper serves it, axum nests
//! it, actix reaches it through the `actix` feature's shim.

pub mod client;
pub mod config;
pub mod locale;
mod remote;
pub mod shell;
pub mod trace;

#[cfg(feature = "actix")]
pub mod actix;

#[cfg(feature = "tls")]
pub mod tls;

#[cfg(feature = "ws")]
pub mod socket;

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
use snapfire_fsr_plan::{Child as PlanChild, Manifest, Node as PlanFileNode, RouteEntry, RowOwner, renumber};
use snapfire_fsr_runtime::ActionHandler;
use snapfire_fsr_runtime::{
  ActionError, AssembleError, DataSource, Evaluator, FailureKind, FibreCache, Head, Identity, LoadError, Locale,
  Matcher, Metadata, RequestCtx, Resolver, SessionCell, WarmLoads, assemble, html_stream, parse_query, wire_stream,
};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Credentials, HttpTransport, IdentityInterceptor, MockTransport, NoCredentials,
  Services, TraceInterceptor, Transport,
};
use snapfire_fsr_session::{MemorySessionStore, Opened, SessionConfig, SessionId, SessionStore, Sessions, TokenCell};
use tower::ServiceExt;
use tower_http::services::ServeDir;

pub use config::{
  AuthSection, BearerKey, ClientConfig, Config, DataCacheSection, MountConfig, SiteSection, SitesSection,
};
pub use locale::{Locales, LocalesSection, Resolution};
pub use remote::{ServiceProvider, ServiceSessionStore};

/// The encodings a payload request may name in `enc`; the wire's `V` row
/// names the one it got.
pub const PAYLOAD_ENCODINGS: &[&str] = &["json"];

/// What a warm pass writes into the prerender directory and a boot reads back:
/// every memoizable source's data under the key a request composes for it.
pub const LOADS_FILE: &str = "loads.json";

fn warm_to_json(warmed: &HashMap<String, Data>) -> String {
  let mut rows: Vec<(&String, &Data)> = warmed.iter().collect();
  rows.sort_by(|a, b| a.0.cmp(b.0));
  let obj: serde_json::Map<String, serde_json::Value> = rows
    .into_iter()
    .map(|(key, data)| {
      (
        key.clone(),
        snapfire_fsr_payload::value_to_json(&Value::Map(data.clone())),
      )
    })
    .collect();
  serde_json::to_string(&serde_json::Value::Object(obj)).expect("a value map serializes")
}

/// What a build warmed, or nothing when the file is absent or unreadable: a
/// warm pass is an optimization, so a bad file costs loads rather than a boot.
fn warm_from_file(path: &Path) -> HashMap<String, Data> {
  let Ok(text) = std::fs::read_to_string(path) else {
    return HashMap::new();
  };
  let Ok(serde_json::Value::Object(obj)) = serde_json::from_str::<serde_json::Value>(&text) else {
    tracing::warn!(target: "fsr::load", file = %path.display(), "warmed loads are not an object");
    return HashMap::new();
  };
  let mut out = HashMap::new();
  for (key, json) in obj {
    match snapfire_fsr_payload::json_to_value(&json) {
      Ok(Value::Map(data)) => {
        out.insert(key, data);
      }
      _ => tracing::warn!(target: "fsr::load", key = %key, "warmed load is not a value map"),
    }
  }
  out
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
  #[error("{0}: {1}")]
  Contract(PathBuf, String),
  #[error("no route matches `{0}`")]
  NotFound(String),
  #[error(transparent)]
  Assemble(#[from] AssembleError),
  #[error("server modules in the bundle: {0}")]
  Leak(String),
  #[error("site `{0}`: {1}")]
  Mount(String, String),
}

/// What the middleware decided for a request. `headers` join the response
/// whatever the decision.
#[derive(Debug, Clone, PartialEq)]
pub struct Preflight {
  pub action: PreflightAction,
  pub headers: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PreflightAction {
  Continue,
  /// Serve `path` in place of the one asked for; the location is unchanged.
  Rewrite(String),
  Redirect {
    to: String,
    status: u16,
  },
  /// Answer with `status` and `body`: text when a string, JSON otherwise, empty when null.
  Respond {
    status: u16,
    body: Value,
  },
}

impl Preflight {
  pub fn pass() -> Self {
    Self {
      action: PreflightAction::Continue,
      headers: Vec::new(),
    }
  }

  /// Reads the value a middleware returned. Null or an empty map continues;
  /// `redirect` wins over `status`, which wins over `rewrite`; `headers` is a
  /// map of strings applied in every case.
  pub fn from_value(value: &Value) -> Result<Self, String> {
    let map = match value {
      Value::Null => return Ok(Self::pass()),
      Value::Map(map) => map,
      other => {
        return Err(format!(
          "middleware returned {}; expected nothing or an object",
          kind_of(other)
        ));
      }
    };
    let mut headers = Vec::new();
    if let Some(given) = map.get("headers") {
      let Value::Map(given) = given else {
        return Err("middleware `headers` must be an object of strings".to_owned());
      };
      for (name, value) in given {
        let Value::Str(value) = value else {
          return Err(format!("middleware header `{name}` must be a string"));
        };
        headers.push((name.clone(), value.to_string()));
      }
    }
    let status = match map.get("status") {
      None | Some(Value::Null) => None,
      Some(Value::Int(n)) => {
        Some(u16::try_from(*n).map_err(|_| format!("middleware `status` {n} is not an HTTP status"))?)
      }
      Some(other) => {
        return Err(format!(
          "middleware `status` must be a number, found {}",
          kind_of(other)
        ));
      }
    };
    let text = |key: &str| -> Result<Option<String>, String> {
      match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.to_string())),
        Some(other) => Err(format!("middleware `{key}` must be a string, found {}", kind_of(other))),
      }
    };
    let action = if let Some(to) = text("redirect")? {
      PreflightAction::Redirect {
        to,
        status: status.unwrap_or(307),
      }
    } else if let Some(status) = status {
      PreflightAction::Respond {
        status,
        body: map.get("body").cloned().unwrap_or(Value::Null),
      }
    } else if let Some(path) = text("rewrite")? {
      PreflightAction::Rewrite(path)
    } else {
      PreflightAction::Continue
    };
    Ok(Self { action, headers })
  }
}

fn kind_of(value: &Value) -> &'static str {
  match value {
    Value::Null => "null",
    Value::Bool(_) => "a boolean",
    Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) => "a number",
    Value::Str(_) => "a string",
    Value::Seq(_) => "an array",
    Value::Map(_) => "an object",
    _ => "a value",
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
  Html,
  Payload,
}

/// What the host bound: the application's report plus the services it reaches
/// and the static roots it serves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostReport {
  pub app: Report,
  /// Whether a served connection negotiates HTTP/2 as well as HTTP/1.1.
  pub http2: bool,
  /// What the listener presents, when `[server.tls]` is configured.
  pub tls: Option<TlsReport>,
  /// Service, `http`, `grpc` or `mock`, base URL or responses file.
  pub services: Vec<(String, String, String)>,
  /// The client the sessions live behind, when the store is `service`.
  pub session: Option<String>,
  /// Method and policy for every method the data cache answers.
  pub cached: Vec<(String, String)>,
  /// Method and tags for every method that drops cached answers.
  pub writers: Vec<(String, String)>,
  pub statics: Vec<(String, PathBuf)>,
  /// The prefix the embedded client answers, how many modules it holds and
  /// what they come to. `None` when a static root claims the prefix instead.
  pub client: Option<(&'static str, usize, usize)>,
  /// Where prerendered documents are read from, when configured.
  pub prerender: Option<PathBuf>,
  /// How many of `app.warmable`'s keys the prerender directory answered at
  /// boot. Zero with sources listed means the warm pass has not run.
  pub warmed: usize,
  /// The render memo's capacity and lifetime, when configured.
  pub cache: Option<(u64, String)>,
  /// Whether the document carries the live-refresh script and the host
  /// answers `/__fsr/events` and `/__fsr/changed`.
  pub dev: bool,
  /// The configured locales, the default first; empty without a `[locales]` section.
  pub locales: Vec<String>,
  /// The hosts a request's `Host` is matched against, `server.hosts`; empty
  /// when the header is never read.
  pub hosts: Vec<String>,
  /// `document.origin`, which a canonical or alternate href is absolute against.
  pub origin: Option<String>,
  /// The identity provider and its login page, when one is mounted.
  pub auth: Option<(String, String)>,
  /// Client, custody key: which clients send a bearer token.
  pub bearer: Vec<(String, String)>,
  /// The native pairs registered beside the standard library, by name.
  pub extensions: Vec<String>,
  /// Per locale with a catalog under `locales/`, how many keys its file holds.
  pub catalogs: Vec<(String, usize)>,
  /// The application's own `[site]`: its name and prefix, when it is one.
  pub site: Option<(String, String)>,
  /// The sites mounted under this host: name, prefix, artifact, version and hash.
  pub sites: Vec<SiteReport>,
  pub config: Vec<PathBuf>,
  pub inferred: Vec<String>,
}

/// The configured certificate, what the handshake offers and what re-reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TlsReport {
  pub cert: PathBuf,
  pub key: PathBuf,
  pub alpn: Vec<String>,
  /// The signal that re-reads the files; `None` for `none`.
  pub reload: Option<String>,
}

/// One mounted site in the report.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteReport {
  pub name: String,
  pub at: String,
  pub artifact: PathBuf,
  pub version: String,
  pub hash: String,
  /// Rows of the site's configuration the shell ignored, `session` and the like.
  pub ignored: Vec<String>,
}

impl std::fmt::Display for HostReport {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.app)?;
    if let Some((name, at)) = &self.site {
      writeln!(f, "{:<9} {name:<22} at {at}", "site")?;
    }
    for (i, site) in self.sites.iter().enumerate() {
      let label = if i == 0 { "sites" } else { "" };
      writeln!(
        f,
        "{label:<9} {:<22} at {} from {} {} {}",
        site.name,
        site.at,
        site.artifact.display(),
        site.version,
        site.hash
      )?;
      if !site.ignored.is_empty() {
        writeln!(
          f,
          "{:<9} {:<22} ignored [{}], the shell's",
          "",
          site.name,
          site.ignored.join(", ")
        )?;
      }
    }
    for (i, (service, kind, url)) in self.services.iter().enumerate() {
      let label = if i == 0 { "services" } else { "" };
      writeln!(f, "{label:<9} {service:<22} {kind:<11} {url}")?;
    }
    for (i, (route, dir)) in self.statics.iter().enumerate() {
      let label = if i == 0 { "static" } else { "" };
      writeln!(f, "{label:<9} {route:<22} {}", dir.display())?;
    }
    if let Some((route, files, bytes)) = self.client {
      writeln!(f, "{:<9} {route:<22} {files} modules, {} KiB from the binary", "client", bytes / 1024)?;
    }
    for (i, (pattern, anonymous)) in self
      .app
      .prerenderable
      .iter()
      .map(|p| (p, false))
      .chain(self.app.prerenderable_anonymous.iter().map(|p| (p, true)))
      .enumerate()
    {
      let label = if i == 0 { "prerender" } else { "" };
      let who = if anonymous { " for anonymous visitors" } else { "" };
      match &self.prerender {
        Some(dir) => writeln!(f, "{label:<9} {pattern:<22} {}{who}", dir.display())?,
        None => writeln!(f, "{label:<9} {pattern:<22} not configured{who}")?,
      }
    }
    for (i, source) in self.app.warmable.iter().enumerate() {
      match (i, self.warmed) {
        (0, 0) => writeln!(f, "{:<9} {source:<22} not warmed", "warm")?,
        (0, n) => writeln!(f, "{:<9} {source:<22} {n} loads memoized", "warm")?,
        _ => writeln!(f, "{:<9} {source}", "")?,
      }
    }
    if let Some((capacity, ttl)) = &self.cache {
      writeln!(f, "{:<9} {capacity} entries, ttl {ttl}", "cache")?;
    }
    for (i, (method, policy)) in self.cached.iter().enumerate() {
      let label = if i == 0 { "cached" } else { "" };
      writeln!(f, "{label:<9} {method:<22} {policy}")?;
    }
    for (i, (method, tags)) in self.writers.iter().enumerate() {
      let label = if i == 0 { "writes" } else { "" };
      writeln!(f, "{label:<9} {method:<22} {tags}")?;
    }
    if let Some(client) = &self.session {
      writeln!(f, "{:<9} service via {client}", "session")?;
    }
    if self.dev {
      writeln!(
        f,
        "{:<9} live refresh on /__fsr/events, told by POST /__fsr/changed",
        "dev"
      )?;
    }
    if let Some(origin) = &self.origin {
      writeln!(f, "{:<9} {origin}, which canonical and alternate hrefs are absolute against", "origin")?;
    }
    for (i, host) in self.hosts.iter().enumerate() {
      let label = if i == 0 { "hosts" } else { "" };
      writeln!(f, "{label:<9} {host}")?;
    }
    if !self.hosts.is_empty() {
      writeln!(
        f,
        "{:<9} ctx.host reads the request's Host against these; the server in front must set it; a client otherwise names its own",
        ""
      )?;
    }
    if self.http2 {
      match &self.tls {
        Some(tls) => writeln!(
          f,
          "{:<9} beside http/1.1, chosen by alpn [{}]",
          "http2",
          tls.alpn.join(", ")
        )?,
        None => writeln!(
          f,
          "{:<9} h2c beside http/1.1; a browser wants alpn over tls, which a proxy in front terminates",
          "http2"
        )?,
      }
    }
    if let Some(tls) = &self.tls {
      writeln!(f, "{:<9} {} with {}", "tls", tls.cert.display(), tls.key.display())?;
      match &tls.reload {
        Some(signal) => writeln!(f, "{:<9} re-read on SIG{}", "", signal.to_uppercase())?,
        None => writeln!(f, "{:<9} re-read on nothing; a new certificate needs a restart", "")?,
      }
    }
    if let Some((default, others)) = self.locales.split_first() {
      let rest = if others.is_empty() {
        String::new()
      } else {
        format!(", {}", others.join(", "))
      };
      writeln!(f, "{:<9} {default} (default, unprefixed){rest}", "locales")?;
    }
    if let Some((provider, login)) = &self.auth {
      writeln!(
        f,
        "{:<9} {provider}, login page {login}, routes /auth/login, /auth/callback and /auth/logout",
        "auth"
      )?;
      if self.bearer.is_empty() {
        writeln!(f, "{:<9} none; no client carries a token", "bearer")?;
      }
    }
    for (i, (client, key)) in self.bearer.iter().enumerate() {
      let label = if i == 0 { "bearer" } else { "" };
      writeln!(f, "{label:<9} {client:<22} {key}")?;
    }
    for (i, name) in self.extensions.iter().enumerate() {
      let label = if i == 0 { "natives" } else { "" };
      writeln!(f, "{label:<9} {name:<22} rust")?;
    }
    if !self.catalogs.is_empty() {
      let rows: Vec<String> = self
        .catalogs
        .iter()
        .map(|(tag, n)| format!("{tag} {n} key{}", if *n == 1 { "" } else { "s" }))
        .collect();
      writeln!(f, "{:<9} {}", "catalogs", rows.join(", "))?;
    }
    for (i, source) in self.config.iter().enumerate() {
      let label = if i == 0 { "config" } else { "" };
      writeln!(f, "{label:<9} {}", source.display())?;
    }
    for (i, item) in self.inferred.iter().enumerate() {
      let label = if i == 0 { "inferred" } else { "" };
      writeln!(f, "{label:<9} {item}")?;
    }
    Ok(())
  }
}

/// The publish half and the receiver every `/_sf/live` stream clones. A
/// cloned receiver has a mailbox of its own and starts empty, so a stream
/// opened now never sees what was published before it.
struct Topics {
  tx: fibre::spmc::topic::AsyncTopicSender<String, ()>,
  rx: fibre::spmc::topic::AsyncTopicReceiver<String, ()>,
}

impl Topics {
  fn new(mailbox: usize) -> Self {
    let (tx, rx) = fibre::spmc::topic::channel_async(mailbox);
    Self { tx, rx }
  }
}

/// The same shape for the one signal the dev loop sends.
struct Reload {
  tx: fibre::spmc::topic::AsyncTopicSender<(), ()>,
  rx: fibre::spmc::topic::AsyncTopicReceiver<(), ()>,
}

impl Reload {
  fn new(mailbox: usize) -> Self {
    let (tx, rx) = fibre::spmc::topic::channel_async(mailbox);
    Self { tx, rx }
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
  /// Every open socket by topic, and what the application makes of what one
  /// sends. Without a handler `/_sf/socket` is a 404, since a socket nobody
  /// answers is a socket that does nothing.
  #[cfg(feature = "ws")]
  sockets: Arc<socket::Sockets>,
  #[cfg(feature = "ws")]
  socket_handler: Option<socket::SocketHandler>,
  reloader: Option<Reloader>,
  /// What `reload_sites` rebuilds from: the shell's own inputs as they were
  /// when the process booted, never reread. Absent when the builder carried
  /// something a rebuild from these three cannot reproduce.
  shell_inputs: Option<Arc<ShellInputs>>,
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
  /// The `[session]` settings the running `Sessions` were built from; a
  /// reload whose settings differ is refused, since the store outlives it.
  session_shape: String,
}

/// How a host rebuilds its tables on `Host::reload`: the builder for the
/// application as it now stands on disk.
pub type Reloader = Box<dyn Fn() -> Result<HostBuilder, HostError> + Send + Sync>;

/// How a host mounts its sites again on `Host::reload_sites`. It is handed a
/// builder the host already made from the shell's frozen inputs and returns it
/// with the sites of the moment mounted, so a sites reload cannot reread the
/// shell however the step is written.
pub type SitesMounter = Box<dyn Fn(HostBuilder) -> Result<HostBuilder, HostError> + Send + Sync>;

/// The shell's own inputs, held from the boot that built the host so a sites
/// reload rebuilds against them rather than against the disk.
struct ShellInputs {
  config: Config,
  plan: String,
  contract: Option<Contract>,
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
  /// The bundle's build facts file, read for its id when `dev` is on; the
  /// plain head is what `prerender` writes.
  dev_bundle: Option<PathBuf>,
  statics: Vec<(String, ServeDir)>,
  /// Whether [`client::ROUTE`] is answered out of the binary, which it is
  /// unless a static root claims the prefix.
  client: bool,
  prerendered: Option<PathBuf>,
  /// The memo the app's runtime reads a warmable source's data from, held
  /// here so a warm pass swaps its contents in before it renders anything.
  warm: Arc<WarmLoads>,
  locales: Locales,
  catalogs: Arc<snapfire_fsr_ir::Catalogs>,
  auth: Option<Mounted>,
  /// The mounted sites, longest prefix first.
  sites: Vec<SiteTables>,
  report: Arc<HostReport>,
}

/// What a request under a site's prefix reads beyond the merged tables: the
/// site's middleware, run after the shell's, and what its documents add to
/// the head.
struct SiteTables {
  name: String,
  at: String,
  middleware: Option<Arc<dyn ActionHandler>>,
  styles: Vec<String>,
  entry: Option<String>,
}

impl Tables {
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
  pub artifact: PathBuf,
  pub version: String,
  pub hash: String,
  pub allow_engine: bool,
  pub config: Config,
  pub plan: String,
  pub contract: Option<Contract>,
}

impl Mount {
  /// Reads the artifact at `artifact`, a project directory with its `config/`
  /// beside its app, the way `Host::from` reads one.
  pub fn load(
    name: impl Into<String>,
    artifact: impl Into<PathBuf>,
    version: impl Into<String>,
    hash: impl Into<String>,
    allow_engine: bool,
  ) -> Result<Self, HostError> {
    let artifact = artifact.into();
    let config = Config::load(&artifact)?;
    let plan_path = config.resolve(&config.server.plan);
    let plan = std::fs::read_to_string(&plan_path).map_err(|e| HostError::Io(plan_path, e))?;
    let contract = read_contracts(&config.resolve(&config.server.contracts))?;
    Ok(Self {
      name: name.into(),
      artifact,
      version: version.into(),
      hash: hash.into(),
      allow_engine,
      config,
      plan,
      contract,
    })
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

/// What a request carries into a body beyond its session: the CSRF token
/// minted for it and the token custody the service layer reads. Bodies see
/// the token as the `csrf_token` prop and never see the custody.
#[derive(Clone)]
struct Incoming {
  session: SessionCell,
  csrf: Option<String>,
  credentials: Arc<dyn Credentials>,
  /// The locale whose catalog the navigator already holds, `x-sf-catalog`,
  /// so a payload for that locale carries no `D` row.
  held_catalog: Option<String>,
  /// The request's `Host`, already matched against `server.hosts`.
  host: Option<String>,
}

impl Incoming {
  fn anonymous(session: SessionCell) -> Self {
    Self {
      session,
      csrf: None,
      credentials: Arc::new(NoCredentials),
      held_catalog: None,
      host: None,
    }
  }
}

pub struct HostBuilder {
  /// The trace collector, when one was installed. Served under development.
  traces: Option<trace::Traces>,
  config: Config,
  plan: String,
  contract: Option<Contract>,
  app: Option<AppBuilder>,
  services: Option<Arc<Services>>,
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
  pending: Option<HostError>,
}

impl std::fmt::Debug for HostBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("HostBuilder")
      .field("root", &self.config.root)
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
  pub fn from(path: impl AsRef<std::path::Path>) -> Result<HostBuilder, HostError> {
    let config = Config::load(path)?;
    Self::from_config(config)
  }

  /// `Host::from` on the current directory.
  pub fn from_cwd() -> Result<HostBuilder, HostError> {
    Self::from(".")
  }

  /// The files `located` names, for a binary that adds one with `Located::extra`.
  pub fn from_located(located: config::Located) -> Result<HostBuilder, HostError> {
    Self::from_config(Config::load_located(located)?)
  }

  pub fn from_config(config: Config) -> Result<HostBuilder, HostError> {
    let plan_path = config.resolve(&config.server.plan);
    let plan = std::fs::read_to_string(&plan_path).map_err(|e| HostError::Io(plan_path, e))?;
    let contract = read_contracts(&config.resolve(&config.server.contracts))?;
    Self::from_config_with(config, plan, contract)
  }

  /// `from_config` over a plan file and a contract already in memory, for a
  /// tool that built them and never wrote them.
  pub fn from_config_with(config: Config, plan: String, contract: Option<Contract>) -> Result<HostBuilder, HostError> {
    let app = match config.server.render.as_str() {
      "rust" => App::from_manifest(&plan)?,
      "islands" => App::from_manifest(&plan)?.islands_only(true),
      other => {
        return Err(HostError::Config(
          config.resolve(&config.server.plan),
          format!("`server.render` is `rust` or `islands`, not `{other}`"),
        ));
      }
    };
    Ok(HostBuilder {
      config,
      plan,
      contract,
      traces: None,
      app: Some(app),
      services: None,
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

  /// The message catalogs loaded from `locales/`, when the application has any.
  pub fn catalogs(&self) -> Option<Arc<snapfire_fsr_ir::Catalogs>> {
    let catalogs = &self.tables().catalogs;
    (!catalogs.is_empty()).then(|| catalogs.clone())
  }

  /// Rebuilds the tables through the builder's reloader and swaps them in;
  /// a request in flight finishes on the tables it started with. The
  /// sessions stay: a reload that changes `[session]` is refused.
  pub fn reload(&self) -> Result<Arc<HostReport>, HostError> {
    let reloader = self.reloader.as_ref().ok_or_else(|| {
      HostError::Value(
        "reload".to_owned(),
        "no reloader; `HostBuilder::reloader` names how to rebuild".to_owned(),
      )
    })?;
    self.reload_with(reloader()?)
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
  /// services, a session store, an evaluator, an identity provider or a
  /// prerendered directory has none, since those cannot be rebuilt from the
  /// shell's three inputs; such a host reloads through `reload` instead.
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
    let builder = Host::from_config_with(inputs.config.clone(), inputs.plan.clone(), inputs.contract.clone())?;
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
    let report = tables.report.clone();
    *self.live.write() = Arc::new(tables);
    self.changed();
    Ok(report)
  }

  /// Renders a route. `path` may carry a locale prefix and its query string.
  /// The session is the caller's, so a test can hand in one it prepared.
  pub async fn render(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_in(&t, &visit, raw_query, mode, Incoming::anonymous(session))
      .await
  }

  async fn render_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    mode: RenderMode,
    incoming: Incoming,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let (plan, params) = self
      .plan_for(t, &visit.path)
      .ok_or_else(|| HostError::NotFound(visit.path.clone()))?;
    self
      .render_plan(t, &plan, params, parse_query(raw_query), mode, incoming, visit)
      .await
  }

  /// The plan a route resolves `path` to, with its params.
  fn plan_for(&self, t: &Tables, path: &str) -> Option<(PlanNode, Params)> {
    let matched = t.app.matcher.match_path(path)?;
    let plan = t.app.resolver.resolve(matched.entry, &matched.params)?;
    Some((plan, matched.params))
  }

  /// The intercept a soft navigation to `path` renders: of the route's
  /// `page.<slot>.tsx` plans, in file order, the first whose slot `into`
  /// names, or, without `into`, the first whose layouts the route of `from`
  /// reaches down to the one declaring its slot. `path` and `from` are paths
  /// without their query.
  pub fn intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)> {
    let t = self.tables();
    self.intercept_in(&t, path, from, into)
  }

  fn intercept_in(&self, t: &Tables, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)> {
    let (plans, params) = t.app.intercepts.plans_for(path)?;
    let from_plan = match into {
      Some(_) => None,
      None => Some(self.plan_for(t, from?)?.0),
    };
    let chosen = plans.into_iter().find(|plan| match (into, &from_plan) {
      (Some(slot), _) => intercept_slot(plan).as_deref() == Some(slot),
      (None, Some(from_plan)) => shares_layouts(plan, from_plan),
      (None, None) => false,
    })?;
    Some((chosen, params))
  }

  /// The payload for a soft navigation to `path` from `from`: the intercept
  /// when one applies, the route's own tree otherwise. `path` may carry a
  /// locale prefix and its query; `from` is the document's `pathname` plus
  /// `search`, prefix included.
  pub async fn render_navigation(
    &self,
    path: &str,
    from: Option<&str>,
    into: Option<&str>,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_navigation_in(&t, &visit, raw_query, from, into, Incoming::anonymous(session))
      .await
  }

  async fn render_navigation_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    from: Option<&str>,
    into: Option<&str>,
    incoming: Incoming,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let from_bare = from
      .map(|f| f.split_once('?').map(|(p, _)| p).unwrap_or(f))
      .map(|f| t.locales.resolve(f, None, None).path);
    match self.intercept_in(t, &visit.path, from_bare.as_deref(), into) {
      Some((plan, params)) => {
        self
          .render_plan(
            t,
            &plan,
            params,
            parse_query(raw_query),
            RenderMode::Payload,
            incoming,
            visit,
          )
          .await
      }
      None => self.render_in(t, visit, raw_query, RenderMode::Payload, incoming).await,
    }
  }

  /// The application's not-found tree for a path no route matches, or `None`
  /// when it has none. `params.path` carries the path the tree is answering,
  /// its locale prefix stripped.
  pub async fn render_not_found(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<Option<BoxStream<'static, String>>, HostError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .render_not_found_in(&t, &visit, raw_query, mode, Incoming::anonymous(session))
      .await
  }

  async fn render_not_found_in(
    &self,
    t: &Tables,
    visit: &Resolution,
    raw_query: &str,
    mode: RenderMode,
    incoming: Incoming,
  ) -> Result<Option<BoxStream<'static, String>>, HostError> {
    let Some(plan) = &t.app.not_found else { return Ok(None) };
    let mut params = Params::new();
    params.insert("path".to_owned(), visit.path.clone());
    Ok(Some(
      self
        .render_plan(t, plan, params, parse_query(raw_query), mode, incoming, visit)
        .await?,
    ))
  }

  /// The head a request renders under: the boot head, plus the live-refresh
  /// script when `dev` is on and a canonical link when a prefixed request
  /// asked for the default locale.
  async fn render_plan(
    &self,
    t: &Tables,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    visit: &Resolution,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let mut extra = Vec::new();
    if let Some(facts) = &t.dev_bundle {
      extra.push(snapfire_fsr_core::Node::raw(shell::dev_script(&bundle_id(facts))));
    }
    if visit.prefixed && visit.locale.is_default {
      extra.push(snapfire_fsr_core::Node::raw(shell::canonical(self.origin.as_deref(), &visit.path)));
    }
    let site = t.site_for(&visit.path);
    if let Some(entry) = site.and_then(|s| s.entry.as_deref()) {
      extra.push(snapfire_fsr_core::Node::raw(shell::site_entry(entry)));
    }
    let styles: Vec<String> = site.map(|s| s.styles.clone()).unwrap_or_default();
    let catalog = t.catalogs.json(&visit.locale.tag);
    let mut payload_catalog = None;
    if let Some(json) = &catalog {
      match mode {
        RenderMode::Html => extra.push(snapfire_fsr_core::Node::raw(shell::catalog_script(
          &visit.locale.tag,
          json,
        ))),
        RenderMode::Payload => {
          if incoming.held_catalog.as_deref() != Some(visit.locale.tag.as_str()) {
            payload_catalog = Some(json.to_string());
          }
        }
      }
    }
    if extra.is_empty() && styles.is_empty() && payload_catalog.is_none() {
      return self
        .render_plan_with(
          t,
          plan,
          params,
          query,
          mode,
          incoming,
          &visit.requested(),
          &visit.locale,
          &t.head,
        )
        .await;
    }
    let mut head = t.head.clone();
    let mut parts = vec![t.head.rest.clone()];
    parts.extend(extra);
    head.rest = snapfire_fsr_core::Node::Seq(parts);
    head.entry = site.and_then(|s| s.entry.clone());
    head.styles = styles;
    head.catalog = payload_catalog;
    self
      .render_plan_with(
        t,
        plan,
        params,
        query,
        mode,
        incoming,
        &visit.requested(),
        &visit.locale,
        &head,
      )
      .await
  }

  async fn render_plan_with(
    &self,
    t: &Tables,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    path: &str,
    locale: &Locale,
    head: &Head,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let ctx = self.ctx(t, incoming, params, query, path, locale.clone());
    let assembly = assemble(&t.app.runtime, plan, &ctx, head).await?;
    Ok(match mode {
      RenderMode::Html => Box::pin(html_stream(assembly)),
      RenderMode::Payload => Box::pin(wire_stream(assembly)),
    })
  }

  /// Renders to one string, for tests.
  pub async fn render_navigation_to_string(
    &self,
    path: &str,
    from: Option<&str>,
    into: Option<&str>,
    session: SessionCell,
  ) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render_navigation(path, from, into, session).await?.collect().await;
    Ok(chunks.concat())
  }

  pub async fn render_to_string(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render(path, mode, session).await?.collect().await;
    Ok(chunks.concat())
  }

  /// The patterns one render serves for every request: no parameter, every
  /// source lowered and reading nothing of the request, no page or layout
  /// reading its `identity` or `csrf_token` prop.
  pub fn prerenderable(&self) -> Vec<String> {
    self.tables().app.prerenderable.clone()
  }

  /// The patterns one anonymous render serves for every anonymous request:
  /// their only request reads are the identity, a page or layout's `identity`
  /// prop or a call through a client whose `bearer` is set. `prerender`
  /// writes them too; the file serves a visitor with no identity and a
  /// signed-in one is rendered live.
  pub fn prerenderable_anonymous(&self) -> Vec<String> {
    self.tables().app.prerenderable_anonymous.clone()
  }

  /// Drops every cached subtree of the plan node keyed `plan_key`, a module
  /// name for a lowered page or layout, and says how many went. Zero when
  /// nothing was cached under it or no cache is configured.
  pub async fn invalidate(&self, plan_key: &str) -> usize {
    self.tables().app.invalidate(plan_key).await
  }

  /// The service registry the routes call through, for a Rust host that
  /// calls a backend outside a request.
  pub fn services(&self) -> Arc<Services> {
    self.tables().app.services.clone()
  }

  /// Drops every data cache answer under the named tags, the out-of-band
  /// counterpart of a method that `writes` them.
  pub fn invalidate_tags<I, S>(&self, tags: I)
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    self.tables().app.services.invalidate_tags(tags);
  }

  /// Warms every memoizable load, then renders every prerenderable route once
  /// per locale, anonymously, writing the document as `<out>/<path>/index.html`
  /// and the payload beside it as `index.payload`; `/` lands at the top of
  /// `out`. A locale other than the default lands under its tag,
  /// `<out>/fr_FR/about/index.html`. Returns what was written, each path with
  /// its prefix.
  ///
  /// The warm pass runs first and its result replaces whatever an earlier one
  /// left, so a rerun writes documents from the loads it just took rather than
  /// from the file it booted with.
  pub async fn prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError> {
    let t = self.tables();
    let mut written = Vec::new();
    let warmed = self.warm(&t).await?;
    t.warm.replace(warmed.clone());
    if !warmed.is_empty() {
      std::fs::create_dir_all(out).map_err(|e| HostError::Io(out.to_path_buf(), e))?;
      let file = out.join(LOADS_FILE);
      std::fs::write(&file, warm_to_json(&warmed)).map_err(|e| HostError::Io(file.clone(), e))?;
      written.push((LOADS_FILE.to_owned(), file));
    }
    for pattern in t
      .app
      .prerenderable
      .iter()
      .chain(t.app.prerenderable_anonymous.iter())
      .cloned()
      .collect::<Vec<_>>()
    {
      for tag in t.locales.supported.clone() {
        let locale = t.locales.locale(&tag);
        let root = if locale.is_default {
          out.to_path_buf()
        } else {
          out.join(&tag)
        };
        let dir = root.join(pattern.trim_matches('/'));
        std::fs::create_dir_all(&dir).map_err(|e| HostError::Io(dir.clone(), e))?;
        let served = if locale.is_default {
          pattern.clone()
        } else {
          format!("/{tag}{}", pattern.trim_end_matches('/'))
        };
        for (mode, name) in [(RenderMode::Html, "index.html"), (RenderMode::Payload, "index.payload")] {
          let (plan, params) = self
            .plan_for(&t, &pattern)
            .ok_or_else(|| HostError::NotFound(pattern.clone()))?;
          let chunks = self
            .render_plan_with(
              &t,
              &plan,
              params,
              Params::new(),
              mode,
              Incoming::anonymous(SessionCell::default()),
              &served,
              &locale,
              &t.head,
            )
            .await?;
          let text: String = chunks.collect::<Vec<_>>().await.concat();
          let file = dir.join(name);
          std::fs::write(&file, text).map_err(|e| HostError::Io(file.clone(), e))?;
          written.push((served.clone(), file));
        }
      }
    }
    Ok(written)
  }

  /// Runs every warmable source once per locale with nothing of a request
  /// behind it, keyed the way a request will key it. A source that fails is
  /// left out rather than written as a failure: a request loads it live and
  /// degrades to its error node the way it does without a warm pass.
  async fn warm(&self, t: &Tables) -> Result<HashMap<String, Data>, HostError> {
    let mut warmed = HashMap::new();
    for name in &t.app.warmable {
      let id = snapfire_fsr_core::DataSourceId(name.clone());
      let Some(source) = t.app.runtime.sources.get(&id) else {
        continue;
      };
      for tag in t.locales.supported.clone() {
        let locale = t.locales.locale(&tag);
        let ctx = self.ctx(
          t,
          Incoming::anonymous(SessionCell::default()),
          Params::new(),
          Params::new(),
          "/",
          locale,
        );
        let Some(key) = t.app.runtime.load_keyer.key(&id, &ctx) else {
          continue;
        };
        if warmed.contains_key(&key) {
          continue;
        }
        match source.load(&ctx).await {
          Ok(data) => {
            warmed.insert(key, data);
          }
          Err(e) => tracing::warn!(target: "fsr::load", source = %name, error = %e, "warm load failed"),
        }
      }
    }
    Ok(warmed)
  }

  /// The prerendered text for `path` in `mode`, when the prerender directory
  /// holds one. `path` may carry a locale prefix. The query string is
  /// ignored: a prerenderable route reads none.
  pub fn prerendered(&self, path: &str, mode: RenderMode) -> Option<String> {
    let t = self.tables();
    let path = path.split_once('?').map(|(p, _)| p).unwrap_or(path);
    let visit = t.locales.resolve(path, None, None);
    self.prerendered_in(&t, &visit.path, mode, &visit.locale, true)
  }

  /// `anonymous` says the request carries no identity; a route prerendered
  /// for anonymous visitors only serves its file then.
  fn prerendered_in(
    &self,
    t: &Tables,
    path: &str,
    mode: RenderMode,
    locale: &Locale,
    anonymous: bool,
  ) -> Option<String> {
    let dir = t.prerendered.as_ref()?;
    if !anonymous
      && t
        .app
        .prerenderable_anonymous
        .iter()
        .any(|pattern| pattern.trim_end_matches('/') == path.trim_end_matches('/'))
    {
      return None;
    }
    let root = if locale.is_default {
      dir.clone()
    } else {
      dir.join(&locale.tag)
    };
    let name = match mode {
      RenderMode::Html => "index.html",
      RenderMode::Payload => "index.payload",
    };
    std::fs::read_to_string(root.join(path.trim_matches('/')).join(name)).ok()
  }

  /// Runs the middleware for a request, with `{ method, path, payload }` as its input,
  /// the path stripped of its locale prefix, the locale in `ctx.locale` and
  /// the query string decoded into `ctx.query`. Without middleware every
  /// request continues.
  pub async fn preflight(&self, method: &str, path: &str, session: SessionCell) -> Result<Preflight, ActionError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .preflight_in(
        &t,
        method,
        &visit.path,
        raw_query,
        Incoming::anonymous(session),
        &visit.locale,
      )
      .await
  }

  async fn preflight_in(
    &self,
    t: &Tables,
    method: &str,
    path: &str,
    raw_query: &str,
    incoming: Incoming,
    locale: &Locale,
  ) -> Result<Preflight, ActionError> {
    let internal = |message: String| ActionError::new(snapfire_fsr_runtime::FailureKind::Internal, message);
    let request = |path: &str, site: Option<&SiteTables>| {
      let mut request = ValueMap::default();
      request.insert("method".to_owned(), Value::str(method.to_ascii_uppercase()));
      request.insert("path".to_owned(), Value::str(path.to_owned()));
      request.insert(
        "payload".to_owned(),
        Value::Bool(raw_query.split('&').any(|p| p == "__payload")),
      );
      request.insert(
        "site".to_owned(),
        site.map(|s| Value::str(s.name.clone())).unwrap_or(Value::Null),
      );
      Value::Map(request)
    };
    let mut headers = Vec::new();
    let mut current = path.to_owned();
    let mut action = PreflightAction::Continue;
    if let Some(middleware) = &t.app.middleware {
      let ctx = self.ctx(
        t,
        incoming.clone(),
        Params::new(),
        parse_query(raw_query),
        &current,
        locale.clone(),
      );
      let value = middleware.call(ctx, request(&current, t.site_for(&current))).await?;
      let preflight = Preflight::from_value(&value).map_err(internal)?;
      headers.extend(preflight.headers);
      match preflight.action {
        PreflightAction::Continue => {}
        PreflightAction::Rewrite(to) => {
          current = to.split('?').next().unwrap_or(&to).to_owned();
          action = PreflightAction::Rewrite(to);
        }
        other => return Ok(Preflight { action: other, headers }),
      }
    }
    if let Some(site) = t.site_for(&current) {
      if let Some(middleware) = &site.middleware {
        let ctx = self.ctx(
          t,
          incoming,
          Params::new(),
          parse_query(raw_query),
          &current,
          locale.clone(),
        );
        let value = middleware.call(ctx, request(&current, Some(site))).await?;
        let preflight = Preflight::from_value(&value).map_err(internal)?;
        headers.extend(preflight.headers);
        match preflight.action {
          PreflightAction::Continue => {}
          PreflightAction::Rewrite(to) => {
            let to_path = to.split('?').next().unwrap_or(&to);
            if !site.covers(to_path) {
              return Err(internal(format!(
                "site `{}` rewrote to `{to}`, outside {}",
                site.name, site.at
              )));
            }
            action = PreflightAction::Rewrite(to);
          }
          other => return Ok(Preflight { action: other, headers }),
        }
      }
    }
    Ok(Preflight { action, headers })
  }

  /// The handler matching `method` and `path`, run with `input` as the
  /// request body. `path` may carry a locale prefix and a query string.
  /// `NotFound` when no handler matches.
  pub async fn call_handler(
    &self,
    method: &str,
    path: &str,
    session: SessionCell,
    input: Value,
  ) -> Result<Value, ActionError> {
    let t = self.tables();
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = t.locales.resolve(bare, None, None);
    self
      .call_handler_in(
        &t,
        method,
        &visit.path,
        raw_query,
        Incoming::anonymous(session),
        &visit.locale,
        input,
      )
      .await
  }

  async fn call_handler_in(
    &self,
    t: &Tables,
    method: &str,
    path: &str,
    raw_query: &str,
    incoming: Incoming,
    locale: &Locale,
    input: Value,
  ) -> Result<Value, ActionError> {
    let Some(found) = t.app.handlers.match_request(method, path) else {
      return Err(ActionError::new(
        snapfire_fsr_runtime::FailureKind::NotFound,
        format!("no handler for {} {path}", method.to_ascii_uppercase()),
      ));
    };
    let ctx = self.ctx(t, incoming, found.params, parse_query(raw_query), path, locale.clone());
    t.app.handlers.dispatch(&found.id, ctx, input).await
  }

  /// The identity routes against a session cell rather than a cookie, for a
  /// runner that holds the session itself: `fsr test`. `None` when no
  /// provider is mounted or the path is not one of the three. The identity
  /// the flow settles lands in `session`, which is the cell a spec then
  /// renders pages with; the cookie the routes would set is returned as a
  /// header and is the caller's to ignore. `flow` carries what a cookie
  /// carries between the calls of one journey, so it is one per session.
  pub async fn auth_call(
    &self,
    flow: &AuthFlow,
    method: &str,
    path: &str,
    query: &str,
    body: &[u8],
    headers: &[(String, String)],
    session: SessionCell,
  ) -> Option<(u16, Vec<(String, String)>, String)> {
    let tables = self.tables();
    let mounted = tables.auth.as_ref()?;
    if !path.starts_with("/auth/") {
      return None;
    }
    let method = Method::from_bytes(method.as_bytes()).ok()?;
    let uri = if query.is_empty() {
      path.to_owned()
    } else {
      format!("{path}?{query}")
    };
    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
      builder = builder.header(name, value);
    }
    let request = builder.body(Bytes::copy_from_slice(body)).ok()?;
    let opened = Opened {
      id: flow.id.clone(),
      cell: session,
      tokens: flow.tokens.clone(),
      fresh: true,
    };
    let response = self.auth_route(mounted, &request, &opened, path, query).await?;
    let (parts, body) = response.into_parts();
    let headers = parts
      .headers
      .iter()
      .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_owned(), v.to_owned())))
      .collect();
    let bytes = http_body_util::BodyExt::collect(body).await.ok()?.to_bytes();
    Some((
      parts.status.as_u16(),
      headers,
      String::from_utf8_lossy(&bytes).into_owned(),
    ))
  }

  /// `call_action_in` under the default locale.
  pub async fn call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError> {
    let locale = self.tables().locales.default_locale();
    self.call_action_in(id, session, locale, input).await
  }

  /// Runs an action with `locale` as its `ctx.locale`, which at the edge is
  /// the locale of the document that called it.
  pub async fn call_action_in(
    &self,
    id: &str,
    session: SessionCell,
    locale: Locale,
    input: Value,
  ) -> Result<Value, ActionError> {
    let t = self.tables();
    self
      .dispatch_action(&t, id, Incoming::anonymous(session), "", locale, input)
      .await
  }

  async fn dispatch_action(
    &self,
    t: &Tables,
    id: &str,
    incoming: Incoming,
    path: &str,
    locale: Locale,
    input: Value,
  ) -> Result<Value, ActionError> {
    let ctx = self.ctx(t, incoming, Params::new(), Params::new(), path, locale);
    t.app.actions.dispatch(id, ctx, input).await
  }

  /// The context a body runs in: the services bound to the session's identity
  /// and the request's custody, the token as `csrf`.
  fn ctx(
    &self,
    t: &Tables,
    incoming: Incoming,
    params: Params,
    query: Params,
    path: &str,
    locale: Locale,
  ) -> RequestCtx {
    let services = t.app.services.bind(incoming.session.identity(), incoming.credentials);
    RequestCtx {
      params,
      query,
      path: path.to_owned(),
      session: incoming.session,
      locale,
      host: incoming.host,
      csrf: incoming.csrf,
      services,
      natives: snapfire_fsr_runtime::NativeHandle::new(t.app.natives.clone()),
    }
  }

  /// What a request at the edge carries: the session, its custody and, once
  /// the session is identified, a CSRF token. An anonymous request carries no
  /// token so its renders share the memo; the token joins the memo key.
  fn incoming_holding(&self, opened: &Opened, held_catalog: Option<String>, host: Option<String>) -> Incoming {
    let mut incoming = self.incoming(opened, host);
    incoming.held_catalog = held_catalog;
    incoming
  }

  /// The request's `Host` when `server.hosts` lists it. Absent that key nothing
  /// is read. A header naming an unlisted host answers `None`, so a client
  /// cannot choose what a body sees.
  fn matched_host(&self, headers: &header::HeaderMap) -> Option<String> {
    if self.hosts.is_empty() {
      return None;
    }
    let asked = headers.get(header::HOST)?.to_str().ok()?.to_lowercase();
    self.hosts.iter().find(|h| **h == asked).cloned()
  }

  fn incoming(&self, opened: &Opened, host: Option<String>) -> Incoming {
    let csrf = (self.csrf_always || opened.cell.identity().is_some()).then(|| self.sessions.csrf_token(&opened.id));
    Incoming {
      session: opened.cell.clone(),
      csrf,
      credentials: Arc::new(opened.tokens.clone()),
      held_catalog: None,
      host,
    }
  }

  /// The whole edge for one request: static roots, the action route, then a
  /// page in either mode, with the session opened from the cookie and
  /// persisted into the response.
  /// Tells every open development document that something changed, so each
  /// refreshes its route in place. Nothing happens when `dev` is off.
  pub fn changed(&self) {
    if let Some(reload) = &self.changed {
      let _ = reload.tx.send((), ());
    }
  }

  /// Every open socket, by topic: what a backend pushes into a wave nobody
  /// typed into, and what a presence count is read from.
  #[cfg(feature = "ws")]
  pub fn sockets(&self) -> Arc<socket::Sockets> {
    self.sockets.clone()
  }

  /// `GET /_sf/socket?topic=x` upgraded: the topic rule decides whether this
  /// visitor may open it at all, the handshake is answered inline and the
  /// connection is served once hyper hands the upgraded stream over.
  #[cfg(feature = "ws")]
  fn upgrade(&self, mut req: Request<Bytes>, opened: &snapfire_fsr_session::Opened) -> Response<Body> {
    let Some(handler) = self.socket_handler.clone() else {
      return text_response(StatusCode::NOT_FOUND, "no socket handler is registered".to_owned());
    };
    let topic = req
      .uri()
      .query()
      .and_then(|query| {
        form_urlencoded::parse(query.as_bytes())
          .find(|(key, _)| key == "topic")
          .map(|(_, value)| value.into_owned())
      })
      .unwrap_or_default();
    if topic.is_empty() {
      return text_response(StatusCode::BAD_REQUEST, "no topic: /_sf/socket?topic=a".to_owned());
    }
    let identity = opened.cell.identity();
    if let Some(rule) = &self.topic_rule {
      if !rule(&topic, &opened.cell, identity.as_ref()) {
        return text_response(StatusCode::FORBIDDEN, format!("not yours to open: {topic}"));
      }
    }
    let key = req
      .headers()
      .get("sec-websocket-key")
      .and_then(|v| v.to_str().ok())
      .map(str::to_owned);
    let (Some(key), Some(upgrading)) = (key, req.extensions_mut().remove::<hyper::upgrade::OnUpgrade>()) else {
      return text_response(StatusCode::BAD_REQUEST, "not a websocket upgrade".to_owned());
    };
    let accept = tokio_tungstenite::tungstenite::handshake::derive_accept_key(key.as_bytes());

    let who = socket::Who {
      topic,
      session: opened.cell.clone(),
      identity,
      connection: 0,
    };
    let sockets = self.sockets.clone();
    tokio::spawn(async move {
      match upgrading.await {
        Ok(upgraded) => {
          let io = hyper_util::rt::TokioIo::new(upgraded);
          let stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
            io,
            tokio_tungstenite::tungstenite::protocol::Role::Server,
            None,
          )
          .await;
          socket::serve(stream, sockets, handler, who).await;
        }
        Err(e) => tracing::debug!(target: "fsr::host", error = %e, "upgrade failed"),
      }
    });

    Response::builder()
      .status(StatusCode::SWITCHING_PROTOCOLS)
      .header(header::CONNECTION, "Upgrade")
      .header(header::UPGRADE, "websocket")
      .header("sec-websocket-accept", accept)
      .body(Body::default())
      .expect("a handshake response")
  }

  /// Tells every `/_sf/live` stream watching `topic` that it changed. What a
  /// listener does with it is the client's: the stock one revalidates the
  /// route it is showing, so a loader runs again and the page follows.
  /// Nothing is sent to a stream that did not ask for the topic, and a
  /// publish with no listeners costs a send into an empty channel.
  pub fn publish(&self, topic: impl Into<String>) {
    let _ = self.topics.tx.send(topic.into(), ());
  }

  /// The stream behind `GET /_sf/live?topics=a,b`: one `data: {"topic":"a"}`
  /// per publish of a topic in the list, until the client goes away. A
  /// comment frame opens it so the browser sees the connection established
  /// before anything is published.
  fn live_events(&self, topics: Vec<String>) -> Response<Body> {
    let rx = self.topics.rx.clone();
    for topic in &topics {
      rx.subscribe(topic.clone());
    }
    let opened = futures_util::stream::once(async {
      Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from_static(b": open\n\n")))
    });
    let events = futures_util::stream::unfold(rx, |rx| async move {
      let (topic, ()) = rx.recv().await.ok()?;
      let json = serde_json::json!({ "topic": topic }).to_string();
      Some((Ok(http_body::Frame::data(Bytes::from(format!("data: {json}\n\n")))), rx))
    });
    Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "text/event-stream")
      .header(header::CACHE_CONTROL, "no-cache")
      .body(StreamBody::new(opened.chain(events)).boxed_unsync())
      .expect("an event stream")
  }

  /// A server-sent event stream: one event on open and one per `changed`
  /// call, each `data: {"bundle":"<id>"}` with the bundle id of that moment,
  /// until the client goes away.
  fn events(&self, t: &Tables) -> Response<Body> {
    let (Some(reload), Some(facts)) = (&self.changed, t.dev_bundle.clone()) else {
      return text_response(StatusCode::NOT_FOUND, "dev is off".to_owned());
    };
    let rx = reload.rx.clone();
    rx.subscribe(());
    let event = move || {
      Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(format!(
        "data: {{\"bundle\":\"{}\"}}\n\n",
        bundle_id(&facts)
      ))))
    };
    let greeting = event();
    let opened = futures_util::stream::once(async move { greeting });
    let changes = futures_util::stream::unfold((rx, event), |(rx, event)| async move {
      rx.recv().await.ok()?;
      Some((event(), (rx, event)))
    });
    Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "text/event-stream")
      .header(header::CACHE_CONTROL, "no-cache")
      .body(StreamBody::new(opened.chain(changes)).boxed_unsync())
      .expect("an event stream")
  }

  pub async fn handle(&self, req: Request<Bytes>) -> Response<Body> {
    // The root of this request's trace. Everything a collector keeps for the
    // request hangs off it; with nothing listening it is an atomic load.
    let root = tracing::info_span!(
      target: "fsr::trace",
      "request",
      fibre.root = true,
      method = %req.method(),
      path = %req.uri().path(),
      status = tracing::field::Empty,
      fibre.outcome = tracing::field::Empty,
    );
    let answered = tracing::Instrument::instrument(self.handle_in(req), root.clone()).await;
    root.record("status", answered.status().as_u16());
    root.record("fibre.outcome", if answered.status().is_success() { "ok" } else { "error" });
    answered
  }

  async fn handle_in(&self, req: Request<Bytes>) -> Response<Body> {
    if req.body().len() > self.max_body {
      return self.too_large();
    }
    let t = self.tables();
    let path = req.uri().path().to_owned();

    if path == "/__fsr/sites" && req.method() == Method::GET {
      let sites: Vec<serde_json::Value> = t
        .report
        .sites
        .iter()
        .map(|s| serde_json::json!({ "name": s.name, "at": s.at, "version": s.version, "hash": s.hash }))
        .collect();
      return json_response(StatusCode::OK, &serde_json::json!({ "sites": sites }));
    }
    // The route exists only for an application that installed a sites mounter;
    // for any other host it is not a route at all.
    #[cfg(feature = "sites_reload")]
    if path == "/__fsr/sites/reload" && req.method() == Method::POST && self.sites_mounter.is_some() {
      return self.sites_reload_response();
    }
    if self.changed.is_some() {
      // The traces the collector kept, newest last. Development only: what a
      // source cost is nothing a production client should read.
      if path == "/__fsr/traces" && req.method() == Method::GET {
        let held = self.traces.as_ref().map(|t| t.recent(50)).unwrap_or_default();
        return json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&trace::to_value(&held)));
      }
      if path == "/__fsr/events" && req.method() == Method::GET {
        return self.events(&t);
      }
      if path == "/__fsr/changed" && req.method() == Method::POST {
        self.changed();
        return Response::builder()
          .status(StatusCode::NO_CONTENT)
          .body(Body::default())
          .expect("an empty response");
      }
      if path == "/__fsr/reload" && req.method() == Method::POST {
        return match self.reload() {
          Ok(report) => text_response(StatusCode::OK, report.to_string()),
          Err(e) => text_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
      }
    }

    if t.client {
      if let Some(name) = path.strip_prefix(client::ROUTE).and_then(|rest| rest.strip_prefix('/')) {
        if let Some(body) = client::get(name) {
          return js_response(body, self.changed.is_some());
        }
      }
    }

    for (route, dir) in &t.statics {
      if let Some(rest) = path.strip_prefix(route.as_str()) {
        if rest.is_empty() || rest.starts_with('/') {
          let mut inner = Request::builder()
            .method(req.method().clone())
            .uri(if rest.is_empty() { "/" } else { rest });
          for (name, value) in req.headers() {
            inner = inner.header(name, value);
          }
          let inner = inner.body(Bytes::new()).expect("a request rebuilt from a request");
          return match dir.clone().oneshot(inner).await {
            Ok(response) => {
              let mut response = response.map(|b| b.map_err(std::io::Error::other).boxed_unsync());
              if self.changed.is_some() {
                response
                  .headers_mut()
                  .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
              }
              response
            }
            Err(never) => match never {},
          };
        }
      }
    }

    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let cookie = header("cookie");
    let accept_language = header("accept-language");
    let opened = self.sessions.open(cookie.as_deref()).await;

    #[cfg(feature = "ws")]
    if path == "/_sf/socket" && req.method() == Method::GET {
      return self.upgrade(req, &opened);
    }
    if path == "/_sf/live" && req.method() == Method::GET {
      let topics: Vec<String> = req
        .uri()
        .query()
        .map(|query| {
          form_urlencoded::parse(query.as_bytes())
            .filter(|(key, _)| key == "topics")
            .flat_map(|(_, value)| value.split(',').map(|t| t.trim().to_owned()).collect::<Vec<_>>())
            .filter(|t| !t.is_empty())
            .collect()
        })
        .unwrap_or_default();
      if topics.is_empty() {
        return text_response(StatusCode::BAD_REQUEST, "no topics: /_sf/live?topics=a,b".to_owned());
      }
      if let Some(rule) = &self.topic_rule {
        let identity = opened.cell.identity();
        if let Some(refused) = topics
          .iter()
          .find(|topic| !rule(topic, &opened.cell, identity.as_ref()))
        {
          return text_response(StatusCode::FORBIDDEN, format!("not yours to follow: {refused}"));
        }
      }
      return self.live_events(topics);
    }

    let is_action = req.method() == Method::POST && path.starts_with("/_sf/action/");
    let visit = if is_action {
      let from = header("x-sf-from")
        .map(|f| f.split('?').next().unwrap_or(&f).to_owned())
        .unwrap_or_else(|| "/".to_owned());
      let resolved = t.locales.resolve(&from, cookie.as_deref(), accept_language.as_deref());
      Resolution {
        locale: resolved.locale,
        path: path.clone(),
        prefixed: false,
        set_cookie: None,
      }
    } else {
      t.locales.resolve(&path, cookie.as_deref(), accept_language.as_deref())
    };
    let framework_owned = visit.path.starts_with("/_sf/")
      || visit.path.starts_with("/__fsr/")
      || (t.auth.is_some() && visit.path.starts_with("/auth/"));
    if visit.prefixed && framework_owned {
      return text_response(StatusCode::NOT_FOUND, format!("no route: {path}"));
    }
    let raw_query = req.uri().query().unwrap_or("").to_owned();
    let mut response = self.handle_resolved(&t, req, &opened, visit.clone(), raw_query).await;
    if let Some(set_cookie) = &visit.set_cookie {
      if let Ok(value) = HeaderValue::from_str(set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
    }
    response
  }

  /// The request past the statics and the locale: the middleware, then the
  /// action route, a handler or a page.
  async fn handle_resolved(
    &self,
    t: &Tables,
    req: Request<Bytes>,
    opened: &snapfire_fsr_session::Opened,
    visit: Resolution,
    raw_query: String,
  ) -> Response<Body> {
    let path = visit.path.clone();
    if let Some(mounted) = &t.auth {
      if let Some(response) = self.auth_route(mounted, &req, opened, &path, &raw_query).await {
        return response;
      }
    }
    let asked = if raw_query.is_empty() {
      path.clone()
    } else {
      format!("{path}?{raw_query}")
    };
    let preflight = match self
      .preflight_in(
        t,
        req.method().as_str(),
        &path,
        &raw_query,
        self.incoming(opened, self.matched_host(req.headers())),
        &visit.locale,
      )
      .await
    {
      Ok(preflight) => preflight,
      Err(e) => {
        return json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        );
      }
    };
    let (path, target) = match &preflight.action {
      PreflightAction::Continue => (path, asked),
      PreflightAction::Rewrite(to) => {
        let (to_path, to_query) = to.split_once('?').unwrap_or((to.as_str(), ""));
        let query = if to_query.is_empty() {
          raw_query.clone()
        } else {
          to_query.to_owned()
        };
        let target = if query.is_empty() {
          to_path.to_owned()
        } else {
          format!("{to_path}?{query}")
        };
        (to_path.to_owned(), target)
      }
      PreflightAction::Redirect { to, status } => {
        let mut response = Response::builder()
          .status(StatusCode::from_u16(*status).unwrap_or(StatusCode::TEMPORARY_REDIRECT))
          .header(header::LOCATION, to.as_str())
          .body(Body::default())
          .expect("a redirect");
        with_headers(&mut response, &preflight.headers);
        self.set_cookie(opened, &mut response).await;
        return response;
      }
      PreflightAction::Respond { status, body } => {
        let status = StatusCode::from_u16(*status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = match body {
          Value::Null => Response::builder()
            .status(status)
            .body(Body::default())
            .expect("an empty response"),
          Value::Str(text) => text_response(status, text.to_string()),
          other => json_response(status, &snapfire_fsr_payload::value_to_json(other)),
        };
        with_headers(&mut response, &preflight.headers);
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    };
    let mut response = self.respond(t, req, opened, path, target, &raw_query, &visit).await;
    with_headers(&mut response, &preflight.headers);
    response
  }

  async fn respond(
    &self,
    t: &Tables,
    req: Request<Bytes>,
    opened: &snapfire_fsr_session::Opened,
    path: String,
    target: String,
    raw_query: &str,
    visit: &Resolution,
  ) -> Response<Body> {
    if req.method() == Method::POST {
      if let Some(module) = path.strip_prefix("/_sf/island/").map(percent_decoded) {
        let (status, json) = island_step(t.app.lowered.as_deref(), &module, req.body(), &visit.locale.tag);
        let mut response = json_response(status, &json);
        self.set_cookie(opened, &mut response).await;
        return response;
      }
      if let Some(id) = path.strip_prefix("/_sf/action/").map(percent_decoded) {
        let id = id.as_str();
        let is_form = req
          .headers()
          .get(header::CONTENT_TYPE)
          .and_then(|v| v.to_str().ok())
          .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));
        let input = if is_form {
          let mut fields = form_params(req.body());
          let token = match fields.shift_remove("_csrf") {
            Some(Value::Str(token)) => token.to_string(),
            _ => String::new(),
          };
          if !self.sessions.verify_csrf(&opened.id, &token) {
            return text_response(StatusCode::FORBIDDEN, "csrf verification failed".to_owned());
          }
          Value::Map(fields)
        } else {
          match serde_json::from_slice::<serde_json::Value>(req.body())
            .map_err(|e| e.to_string())
            .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
          {
            Ok(value) => value,
            Err(e) => {
              return json_response(
                StatusCode::BAD_REQUEST,
                &serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") }),
              );
            }
          }
        };
        let mut response = match self
          .dispatch_action(t, id, self.incoming(opened, self.matched_host(req.headers())), &visit.path, visit.locale.clone(), input)
          .await
        {
          Ok(_) if is_form => {
            let back = req
              .headers()
              .get(header::REFERER)
              .and_then(|v| v.to_str().ok())
              .and_then(referer_path)
              .unwrap_or_else(|| "/".to_owned());
            see_other(&back)
          }
          Ok(value) => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
          Err(e) => json_response(
            StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
          ),
        };
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    }

    if t.app.handlers.match_request(req.method().as_str(), &path).is_some() {
      let input = if req.body().is_empty() {
        Value::Null
      } else {
        match serde_json::from_slice::<serde_json::Value>(req.body())
          .map_err(|e| e.to_string())
          .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
        {
          Ok(value) => value,
          Err(e) => {
            return json_response(
              StatusCode::BAD_REQUEST,
              &serde_json::json!({ "kind": "invalid", "message": format!("invalid request body: {e}") }),
            );
          }
        }
      };
      let (target_path, target_query) = target.split_once('?').unwrap_or((target.as_str(), ""));
      let mut response = match self
        .call_handler_in(
          t,
          req.method().as_str(),
          target_path,
          target_query,
          self.incoming(opened, self.matched_host(req.headers())),
          &visit.locale,
          input,
        )
        .await
      {
        Ok(value) => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
        Err(e) => json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        ),
      };
      self.set_cookie(opened, &mut response).await;
      return response;
    }

    let mode = if raw_query.split('&').any(|p| p == "__payload") {
      RenderMode::Payload
    } else {
      RenderMode::Html
    };
    if mode == RenderMode::Payload {
      if let Some(asked) = parse_query(raw_query)
        .get("enc")
        .filter(|enc| !PAYLOAD_ENCODINGS.contains(&enc.as_str()))
      {
        return text_response(
          StatusCode::NOT_ACCEPTABLE,
          format!(
            "unsupported payload encoding `{asked}`; the encodings are {}",
            PAYLOAD_ENCODINGS.join(", ")
          ),
        );
      }
    }
    tracing::info!(target: "fsr::host", path = %path, payload = (mode == RenderMode::Payload), "request");
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let (from, into) = match mode {
      RenderMode::Payload => (header("x-sf-from"), header("x-sf-into")),
      RenderMode::Html => (None, None),
    };
    let held_catalog = header("x-sf-catalog");
    let intercepted = (from.is_some() || into.is_some())
      && self
        .intercept_in(
          t,
          &path,
          from.as_deref().map(|f| f.split('?').next().unwrap_or(f)),
          into.as_deref(),
        )
        .is_some();

    if req.method() == Method::GET && !intercepted {
      if let Some(text) = self.prerendered_in(t, &path, mode, &visit.locale, opened.cell.identity().is_none()) {
        let content_type = match mode {
          RenderMode::Html => "text/html; charset=utf-8",
          RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
        };
        let mut response = Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, content_type)
          .header("x-sf-prerendered", "1")
          .body(
            http_body_util::Full::new(Bytes::from(text))
              .map_err(|never: std::convert::Infallible| match never {})
              .boxed_unsync(),
          )
          .expect("a response with a valid header");
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    }

    let (target_path, target_query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let target_visit = Resolution {
      locale: visit.locale.clone(),
      path: target_path.to_owned(),
      prefixed: visit.prefixed,
      set_cookie: None,
    };
    let rendered = if intercepted {
      self
        .render_navigation_in(
          t,
          &target_visit,
          target_query,
          from.as_deref(),
          into.as_deref(),
          self.incoming_holding(opened, held_catalog.clone(), self.matched_host(req.headers())),
        )
        .await
    } else {
      self
        .render_in(
          t,
          &target_visit,
          target_query,
          mode,
          self.incoming_holding(opened, held_catalog.clone(), self.matched_host(req.headers())),
        )
        .await
    };
    let rendered = match rendered {
      Ok(chunks) => Ok((StatusCode::OK, chunks)),
      Err(HostError::NotFound(path)) => match self
        .render_not_found_in(
          t,
          &target_visit,
          target_query,
          mode,
          self.incoming_holding(opened, held_catalog.clone(), self.matched_host(req.headers())),
        )
        .await
      {
        Ok(Some(chunks)) => Ok((StatusCode::NOT_FOUND, chunks)),
        Ok(None) => return text_response(StatusCode::NOT_FOUND, format!("no route: {path}")),
        Err(e) => Err(e),
      },
      Err(e) => Err(e),
    };
    match rendered {
      Ok((status, chunks)) => {
        let content_type = match mode {
          RenderMode::Html => "text/html; charset=utf-8",
          RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
        };
        let body = StreamBody::new(chunks.map(|c| Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(c)))));
        let mut response = Response::builder()
          .status(status)
          .header(header::CONTENT_TYPE, content_type)
          .body(body.boxed_unsync())
          .expect("a response with a valid header");
        self.set_cookie(opened, &mut response).await;
        response
      }
      Err(e) => text_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
  }

  async fn set_cookie(&self, opened: &Opened, response: &mut Response<Body>) {
    let written = if self.csrf_always {
      self.sessions.establish(opened).await
    } else {
      self.sessions.persist(opened).await
    };
    let set_cookie = match written {
      Ok(set_cookie) => set_cookie,
      Err(error) => {
        // The response still goes out: the reader is served, and the cookie
        // is withheld rather than naming a session the store does not hold.
        tracing::error!(target: "fsr::session", error = %error, "the session was not saved");
        return;
      }
    };
    if let Some(set_cookie) = set_cookie {
      if let Ok(value) = HeaderValue::from_str(&set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
    }
  }

  /// The framework-owned identity routes, plus the seeding a GET of the
  /// login page does so a typed URL can still post to the callback. `None`
  /// when `path` is none of them and the request goes on to the middleware.
  /// Logout answers without persisting: the record is gone and the cookie
  /// expires in the same response.
  async fn auth_route(
    &self,
    mounted: &Mounted,
    req: &Request<Bytes>,
    opened: &Opened,
    path: &str,
    raw_query: &str,
  ) -> Option<Response<Body>> {
    let query = parse_query(raw_query);
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let referer = header("referer").as_deref().and_then(referer_path);
    let asked = query.get("return_to").and_then(|p| same_origin_path(p));
    match (req.method(), path) {
      (&Method::GET, "/auth/login") => {
        let return_to = asked.or(referer).unwrap_or_else(|| "/".to_owned());
        let redirect = mounted.auth.login(opened, &return_to).await;
        let mut response = see_other(&redirect);
        self.set_cookie(opened, &mut response).await;
        Some(response)
      }
      (&Method::GET | &Method::POST, "/auth/callback") => {
        let params = match callback_params(req, &query) {
          Ok(params) => params,
          Err(message) => return Some(text_response(StatusCode::BAD_REQUEST, message)),
        };
        let pending = mounted.auth.pending_return_to(opened);
        let mut response = match mounted.auth.callback(opened, params).await {
          Ok(destination) => see_other(&destination),
          Err(AuthError::Denied(_)) => {
            let back: String = pending
              .map(|p| form_urlencoded::byte_serialize(p.as_bytes()).collect())
              .unwrap_or_default();
            see_other(&format!("{}?error=denied&return_to={back}", mounted.login_path))
          }
          Err(e @ AuthError::Invalid(_)) => text_response(StatusCode::BAD_REQUEST, e.to_string()),
        };
        self.set_cookie(opened, &mut response).await;
        Some(response)
      }
      (&Method::POST, "/auth/logout") => {
        let token = form_field(req.body(), "_csrf")
          .or_else(|| header("x-sf-csrf"))
          .unwrap_or_default();
        if !self.sessions.verify_csrf(&opened.id, &token) {
          return Some(text_response(
            StatusCode::FORBIDDEN,
            "csrf verification failed".to_owned(),
          ));
        }
        mounted.auth.logout(opened);
        let expire = match self.sessions.destroy(opened).await {
          Ok(expire) => expire,
          Err(error) => {
            tracing::error!(target: "fsr::session", error = %error, "the session was not destroyed");
            return Some(text_response(
              StatusCode::INTERNAL_SERVER_ERROR,
              "signing out failed".to_owned(),
            ));
          }
        };
        let mut response = see_other("/");
        if let Ok(value) = HeaderValue::from_str(&expire) {
          response.headers_mut().append(header::SET_COOKIE, value);
        }
        Some(response)
      }
      (&Method::GET, login) if login == mounted.login_path => {
        let from_elsewhere = referer.filter(|r| r.split('?').next() != Some(mounted.login_path.as_str()));
        let return_to = asked.or(from_elsewhere).unwrap_or_else(|| "/".to_owned());
        mounted.auth.ensure_flow(opened, &return_to).await;
        None
      }
      _ => None,
    }
  }

  /// Serves on `listen` with hyper. `Host::listen` is the configured address.
  pub async fn serve(self: Arc<Self>, listen: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    self.serve_listener(listener).await
  }

  /// Serves an already bound listener, which is how a test picks port zero.
  pub async fn serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    #[cfg(all(unix, feature = "tls"))]
    self.clone().watch_for_reload();
    loop {
      let (stream, _) = listener.accept().await?;
      let host = self.clone();
      tokio::spawn(async move { host.serve_connection(stream).await });
    }
  }

  async fn serve_connection(self: Arc<Self>, stream: tokio::net::TcpStream) {
    #[cfg(feature = "tls")]
    if let Some(tls) = self.tls.clone() {
      let stream = match tls.acceptor().accept(stream).await {
        Ok(stream) => stream,
        Err(e) => {
          tracing::debug!(target: "fsr::host", error = %e, "handshake failed");
          return;
        }
      };
      if let Some(e) = serve_io(self, hyper_util::rt::TokioIo::new(stream)).await {
        tracing::debug!(target: "fsr::host", error = %e, "connection ended");
      }
      return;
    }
    if let Some(e) = serve_io(self, hyper_util::rt::TokioIo::new(stream)).await {
      tracing::debug!(target: "fsr::host", error = %e, "connection ended");
    }
  }

  /// Re-reads the certificate and its key and swaps what the next handshake
  /// presents; connections already up are untouched and a file that will not
  /// read leaves the running certificate in place. Nothing without
  /// `[server.tls]`. The configured signal calls this, and so may a caller.
  #[cfg(feature = "tls")]
  pub fn reload_tls(&self) -> Result<(), HostError> {
    match &self.tls {
      Some(tls) => tls.reload(),
      None => Ok(()),
    }
  }

  #[cfg(all(unix, feature = "tls"))]
  fn watch_for_reload(self: Arc<Self>) {
    use tokio::signal::unix::{SignalKind, signal};
    let Some(name) = self.tls_reload.clone() else { return };
    let kind = match name.as_str() {
      "hup" => SignalKind::hangup(),
      "usr1" => SignalKind::user_defined1(),
      "usr2" => SignalKind::user_defined2(),
      _ => return,
    };
    tokio::spawn(async move {
      let mut stream = match signal(kind) {
        Ok(stream) => stream,
        Err(e) => {
          tracing::warn!(target: "fsr::host", error = %e, "cannot listen for SIG{}", name.to_uppercase());
          return;
        }
      };
      while stream.recv().await.is_some() {
        match self.reload_tls() {
          Ok(()) => tracing::info!(target: "fsr::host", "certificate reloaded"),
          Err(e) => tracing::warn!(target: "fsr::host", error = %e, "certificate not reloaded; the running one stands"),
        }
      }
    });
  }

  pub fn listen(&self) -> &str {
    &self.report_listen
  }

  fn too_large(&self) -> Response<Body> {
    text_response(
      StatusCode::PAYLOAD_TOO_LARGE,
      format!("request body over {} bytes, the host's server.max_body", self.max_body),
    )
  }
}

/// What the handshake offers when `server.tls.alpn` says nothing: HTTP/2
/// first when the host negotiates it, HTTP/1.1 alone when it does not.
fn default_alpn(http2: bool) -> Vec<String> {
  match http2 {
    true => vec!["h2".to_owned(), "http/1.1".to_owned()],
    false => vec!["http/1.1".to_owned()],
  }
}

/// One connection, whatever it is carried over: the same edge, with HTTP/2
/// negotiated beside HTTP/1.1 when `server.http2` is on. The error is the
/// connection's, already formatted, or `None` when it closed cleanly.
async fn serve_io<I>(host: Arc<Host>, io: I) -> Option<String>
where
  I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
  let http2 = host.http2;
  let service = hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
    let host = host.clone();
    async move {
      let (parts, body) = req.into_parts();
      let bytes = match http_body_util::Limited::new(body, host.max_body).collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(e) if e.is::<http_body_util::LengthLimitError>() => return Ok::<_, Infallible>(host.too_large()),
        Err(_) => Bytes::new(),
      };
      Ok::<_, Infallible>(host.handle(Request::from_parts(parts, bytes)).await)
    }
  });
  if http2 {
    hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
      .serve_connection_with_upgrades(io, service)
      .await
      .err()
      .map(|e| e.to_string())
  } else {
    hyper::server::conn::http1::Builder::new()
      .serve_connection(io, service)
      .with_upgrades()
      .await
      .err()
      .map(|e| e.to_string())
  }
}

/// What tells one bundle from the next: a hash over the content of every
/// output the build facts list, source maps aside, so a rebundle that wrote
/// the same modules keeps its id and an edited module changes it. `-` when
/// there is no bundle.
fn bundle_id(facts: &Path) -> String {
  use std::hash::{Hash, Hasher};
  let Ok(text) = std::fs::read_to_string(facts) else {
    return "-".to_owned();
  };
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else {
    return "-".to_owned();
  };
  let dir = facts.parent().unwrap_or(Path::new("."));
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  for output in json["outputs"]
    .as_array()
    .into_iter()
    .flatten()
    .filter_map(|o| o.as_str())
  {
    if output.ends_with(".map") || output.ends_with(".snapfire-build.json") {
      continue;
    }
    output.hash(&mut hasher);
    std::fs::read(dir.join(output)).unwrap_or_default().hash(&mut hasher);
  }
  format!("{:016x}", hasher.finish())
}

fn with_headers(response: &mut Response<Body>, headers: &[(String, String)]) {
  for (name, value) in headers {
    if let (Ok(name), Ok(value)) = (
      header::HeaderName::from_bytes(name.as_bytes()),
      HeaderValue::from_str(value),
    ) {
      response.headers_mut().append(name, value);
    }
  }
}

/// A `mock` client's transport from its responses file: an object of method
/// name to the response in the payload's JSON encoding, or to
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
/// transport each unless the caller overrides transports, and the report
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

fn json_response(status: StatusCode, json: &serde_json::Value) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "application/json")
    .body(
      http_body_util::Full::new(Bytes::from(json.to_string()))
        .map_err(|never| match never {})
        .boxed_unsync(),
    )
    .expect("a json response")
}

/// The slot an intercept plan fills: the child of the node that keeps the
/// page which the plan fills instead. A node that keeps other slots but still
/// fills its own `content` is a layout on the way down, not the one declaring
/// the slot, so the walk continues through it.
fn intercept_slot(plan: &PlanNode) -> Option<String> {
  if plan.keep.iter().any(|name| name.0 == "content") {
    return plan
      .children
      .iter()
      .find(|(name, _)| !plan.keep.contains(name))
      .map(|(name, _)| name.0.clone());
  }
  plan.children.iter().find_map(|(_, child)| intercept_slot(child))
}

/// True when every layout on the intercept plan's spine, down to the one
/// declaring its slot, is the same module at the same depth on `from`.
fn shares_layouts(intercept: &PlanNode, from: &PlanNode) -> bool {
  if intercept.module != from.module {
    return false;
  }
  if intercept.keep.iter().any(|name| name.0 == "content") {
    return true;
  }
  let next = intercept.children.iter().find(|(name, _)| name.0 == "content");
  let from_next = from.children.iter().find(|(name, _)| name.0 == "content");
  match (next, from_next) {
    (Some((_, a)), Some((_, b))) => shares_layouts(a, b),
    _ => false,
  }
}

/// One round trip of an island in server mode: the body is `{ props, state,
/// handler, event }`; `handler` is the index of the handler that fired or
/// null to render as is. Answers `{ state, html }`: the state after the
/// handler and the island's markup rendered from it, with handler markers.
pub fn island_step(
  lowered: Option<&snapfire_fsr_ir::IrEvaluator>,
  module: &str,
  body: &[u8],
  locale: &str,
) -> (StatusCode, serde_json::Value) {
  let Some(evaluator) = lowered else {
    return (
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": "no lowered component" }),
    );
  };
  let components = evaluator.components();
  let Some(component) = components.get(module).cloned() else {
    return (
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": format!("`{module}` is not a lowered component") }),
    );
  };
  let input = match serde_json::from_slice::<serde_json::Value>(body)
    .map_err(|e| e.to_string())
    .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
  {
    Ok(Value::Map(map)) => map,
    Ok(_) => {
      return (
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "an island step is an object" }),
      );
    }
    Err(e) => {
      return (
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": format!("invalid island step: {e}") }),
      );
    }
  };
  let mut props = match input.get("props") {
    Some(Value::Map(map)) => map.clone(),
    None | Some(Value::Null) => ValueMap::default(),
    Some(_) => {
      return (
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "props must be an object" }),
      );
    }
  };
  let state = match input.get("state") {
    Some(Value::Map(map)) => map.clone(),
    None | Some(Value::Null) => ValueMap::default(),
    Some(_) => {
      return (
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "state must be an object" }),
      );
    }
  };
  if let Some(unknown) = state.keys().find(|k| !component.state.contains(k)) {
    return (
      StatusCode::BAD_REQUEST,
      serde_json::json!({ "kind": "invalid", "message": format!("`{unknown}` is not state of `{module}`") }),
    );
  }
  let handler = match input.get("handler") {
    None | Some(Value::Null) => None,
    Some(Value::Int(i)) if *i >= 0 => Some(*i as usize),
    Some(Value::F64(f)) if *f >= 0.0 && f.fract() == 0.0 => Some(*f as usize),
    Some(_) => {
      return (
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "handler must be an index" }),
      );
    }
  };
  if handler.is_some_and(|h| h >= component.handlers.len()) {
    return (
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": format!("`{module}` has no handler {}", handler.unwrap_or(0)) }),
    );
  }
  let event = input.get("event").cloned().unwrap_or(Value::Null);
  if !locale.is_empty() {
    props
      .entry("locale".to_owned())
      .or_insert_with(|| Value::str(locale.to_owned()));
  }
  match evaluator
    .interpreter()
    .island_step(module, &component, &props, &state, handler, &event, &components)
  {
    Ok(stepped) => {
      let html = snapfire_fsr_payload::html_serialize(&Node::Seq(snapfire_fsr_ir::rendered_nodes(&stepped.rendered)));
      (
        StatusCode::OK,
        serde_json::json!({ "state": snapfire_fsr_payload::value_to_json(&Value::Map(stepped.state)), "html": html }),
      )
    }
    Err(fail) => (
      StatusCode::from_u16(fail.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
      serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message }),
    ),
  }
}

/// One module of the embedded client. `no_cache` for a development host, whose
/// client changes when the binary it is built beside does.
fn js_response(body: &'static str, no_cache: bool) -> Response<Body> {
  let mut response = Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, client::MEDIA_TYPE);
  if no_cache {
    response = response.header(header::CACHE_CONTROL, "no-cache");
  }
  response
    .body(
      http_body_util::Full::new(Bytes::from_static(body.as_bytes()))
        .map_err(|never| match never {})
        .boxed_unsync(),
    )
    .expect("a script response")
}

fn text_response(status: StatusCode, text: String) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(
      http_body_util::Full::new(Bytes::from(text))
        .map_err(|never| match never {})
        .boxed_unsync(),
    )
    .expect("a text response")
}

fn see_other(location: &str) -> Response<Body> {
  match Response::builder()
    .status(StatusCode::SEE_OTHER)
    .header(header::LOCATION, location)
    .body(Body::default())
  {
    Ok(response) => response,
    Err(_) => text_response(StatusCode::BAD_REQUEST, format!("`{location}` is not a location")),
  }
}

/// A path on this origin: `/` first and not a scheme-relative `//host`.
fn same_origin_path(candidate: &str) -> Option<String> {
  let own = candidate.starts_with('/') && !candidate.starts_with("//") && !candidate.starts_with("/\\");
  own.then(|| candidate.to_owned())
}

/// The path and query of a `Referer`, whether it came absolute or bare.
fn referer_path(referer: &str) -> Option<String> {
  if referer.starts_with('/') {
    return same_origin_path(referer);
  }
  let rest = referer.split_once("://")?.1;
  same_origin_path(rest.find('/').map(|i| &rest[i..]).unwrap_or("/"))
}

/// A path segment with its `%XX` escapes decoded, since the client encodes
/// an action id and a site's carries a colon.
fn percent_decoded(segment: &str) -> String {
  let bytes = segment.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' && i + 2 < bytes.len() {
      if let Ok(byte) = u8::from_str_radix(&segment[i + 1..i + 3], 16) {
        out.push(byte);
        i += 3;
        continue;
      }
    }
    out.push(bytes[i]);
    i += 1;
  }
  String::from_utf8(out).unwrap_or_else(|_| segment.to_owned())
}

fn form_params(body: &[u8]) -> ValueMap {
  form_urlencoded::parse(body)
    .map(|(k, v)| (k.into_owned(), Value::str(v.into_owned())))
    .collect()
}

fn form_field(body: &[u8], name: &str) -> Option<String> {
  match form_params(body).get(name) {
    Some(Value::Str(value)) => Some(value.to_string()),
    _ => None,
  }
}

/// What the provider's callback receives: the query on a GET, a form or a
/// JSON object on a POST.
fn callback_params(req: &Request<Bytes>, query: &Params) -> Result<ValueMap, String> {
  if req.method() == Method::GET {
    return Ok(query.iter().map(|(k, v)| (k.clone(), Value::str(v.clone()))).collect());
  }
  let content_type = req
    .headers()
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("");
  if content_type.starts_with("application/json") {
    let json: serde_json::Value =
      serde_json::from_slice(req.body()).map_err(|e| format!("invalid callback body: {e}"))?;
    return match snapfire_fsr_payload::json_to_value(&json) {
      Ok(Value::Map(map)) => Ok(map),
      Ok(_) => Err("the callback body must be an object".to_owned()),
      Err(e) => Err(format!("invalid callback body: {e}")),
    };
  }
  Ok(form_params(req.body()))
}

impl HostBuilder {
  fn app_mut(&mut self, f: impl FnOnce(AppBuilder) -> AppBuilder) -> &mut Self {
    if let Some(app) = self.app.take() {
      self.app = Some(f(app));
    }
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
  /// it leaves, and whatever it answers goes out to that topic as store rows.
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
  /// `native(..)` declaration under `ext/` gives, and `reach` what that
  /// declaration says. A plan calling a name nothing registers refuses to build.
  pub fn extension<F>(mut self, name: impl Into<String>, reach: snapfire_fsr_ir::Reach, f: F) -> Self
  where
    F: Fn(&snapfire_fsr_ir::Ambient, &[Value]) -> Result<Value, snapfire_fsr_ir::Fail> + Send + Sync + 'static,
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
    &self.config
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
    let sites_mounter = self.sites_mounter.take();
    // A sites reload rebuilds the shell from these three alone, so a builder
    // carrying anything else they cannot reproduce keeps none of them and the
    // call is refused by name rather than quietly dropping what it held.
    let rebuildable = self.services.is_none()
      && self.transport_override.is_none()
      && self.store.is_none()
      && self.shell.is_none()
      && self.prerendered.is_none()
      && self.identity.is_none();
    let shell_inputs = (sites_mounter.is_some() && rebuildable).then(|| {
      Arc::new(ShellInputs {
        config: self.config.clone(),
        plan: self.plan.clone(),
        contract: self.contract.clone(),
      })
    });
    let store = self.store.take();
    let topic_rule = self.topic_rule.take();
    #[cfg(feature = "ws")]
    let socket_handler = self.socket_handler.take();
    #[cfg(feature = "ws")]
    let sockets = self.sockets.take().unwrap_or_else(|| Arc::new(socket::Sockets::new()));
    let http2 = self.http2;
    let (tables, config) = self.assemble()?;
    let ttl = config.session_ttl()?;
    let store: Arc<dyn SessionStore> = match store {
      Some(store) => store,
      None => match config.session.store.as_str() {
        "memory" => Arc::new(MemorySessionStore::new(config.session.capacity, ttl)),
        "service" => {
          let client = format!(
            "{}{}",
            config.site.as_ref().map(SiteSection::prefix).unwrap_or_default(),
            config.session.client.clone().unwrap_or_default()
          );
          Arc::new(ServiceSessionStore::new(tables.app.services.clone(), client))
        }
        other => return Err(HostError::Value("session.store".to_owned(), other.to_owned())),
      },
    };
    let sessions = Sessions::new(
      store,
      config.session.key.as_bytes(),
      SessionConfig {
        ttl,
        secure: config.session.secure,
        ..SessionConfig::default()
      },
    );
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
      shell_inputs,
      sites_mounter,
      #[cfg(feature = "sites_reload")]
      sites_reload: parking_lot::Mutex::new(()),
      csrf_always: config.session.csrf == "always",
      session_shape: session_shape(&config),
      max_body: config.server.max_body,
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
  /// way a boot checks them, and the configuration they came from.
  fn assemble(mut self) -> Result<(Tables, Config), HostError> {
    if let Some(e) = self.pending.take() {
      return Err(e);
    }
    let config = self.config;
    let plan = self.plan;

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
    let mut contract = self.contract.clone().unwrap_or_default();
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
    let mut import_map = match &config.document.import_map {
      Some(rel) => {
        let path = config.resolve(rel);
        Some(std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?)
      }
      None => None,
    };
    for mount in std::mem::take(&mut self.mounts) {
      let site = mount.config.site.clone().ok_or_else(|| {
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
        Manifest::from_text(&mount.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
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
      leaks(&mount.config, &mount.plan).map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      let middleware = site_manifest.middleware.take().map(snapfire_fsr::middleware_from);
      graft(&manifest, &mut site_manifest, &shell_module);
      site_manifest.not_found = None;
      taken.extend(site_manifest.routes.iter().map(|r| r.pattern.clone()));
      app
        .mount_manifest(&site_manifest.to_json())
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      if let Some(site_contract) = &mount.contract {
        contract
          .merge(site_contract.clone(), &format!("site {}", mount.name))
          .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      if build_clients {
        clients_of(
          &mount.config,
          self.transport_override.is_none(),
          &mut contract,
          &mut transports,
          &mut service_rows,
          &mut bearer_rows,
        )
        .map_err(|e| HostError::Mount(mount.name.clone(), e.to_string()))?;
      }
      let mut ignored = Vec::new();
      for root in &mount.config.statics {
        let route = root.route.trim_end_matches('/').to_owned();
        if route.starts_with(&site.at) && !statics.iter().any(|s| s.route == route) {
          statics.push(StaticRootResolved {
            route,
            dir: mount.config.resolve(&root.dir),
          });
        } else {
          ignored.push(format!("static {route}"));
        }
      }
      for section in ["session", "auth", "locales", "cache"] {
        let set = match section {
          "session" => true,
          "auth" => mount.config.auth.is_some(),
          "locales" => mount.config.locales.is_some(),
          _ => mount.config.cache.is_some(),
        };
        if set {
          ignored.push(section.to_owned());
        }
      }
      if let Some(rel) = &mount.config.document.import_map {
        let path = mount.config.resolve(rel);
        let theirs = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?;
        import_map = Some(merge_import_maps(import_map.as_deref(), &theirs));
      }
      site_reports.push(SiteReport {
        name: mount.name.clone(),
        at: site.at.clone(),
        artifact: mount.artifact.clone(),
        version: mount.version.clone(),
        hash: mount.hash.clone(),
        ignored,
      });
      sites.push(SiteTables {
        name: mount.name.clone(),
        at: site.at.clone(),
        middleware,
        styles: mount.config.document.styles.clone().unwrap_or_default(),
        entry: mount.config.document.entry.clone(),
      });
    }
    sites.sort_by(|a, b| b.at.len().cmp(&a.at.len()).then(a.at.cmp(&b.at)));
    let app_contract = if site_reports.is_empty() {
      self.contract.take()
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
        builder
          .try_build()
          .map_err(|e| HostError::Value("cache.data".to_owned(), e.to_string()))?
      }
    };

    let shell_path = config.document.shell.split('#').next().unwrap_or("shell").to_owned();
    let shell: Arc<dyn Evaluator> = self.shell.take().unwrap_or_else(|| Arc::new(shell::DocumentShell));
    if let Some(contract) = app_contract {
      app = app.contract(contract);
    }
    let cache_row = match (config.cache_ttl()?, &config.cache) {
      (Some(ttl), Some(section)) => {
        app = app.cache(Arc::new(FibreCache::bounded(section.capacity, ttl)));
        Some((section.capacity, section.ttl.clone()))
      }
      _ => None,
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
    let app = app
      .services(services)
      .evaluator(move |m: &ModuleId| m.path == shell_path, shell)
      .build()?;

    let styles = config.document.styles.clone().unwrap_or_default();
    let mut head = shell::head(
      &config.document.title,
      &styles,
      import_map.as_deref(),
      config.document.entry.as_deref(),
    );
    head.head = config.document.head_meta()?.head;
    head.origin = config.origin()?;
    let dev = config.dev();
    let dev_bundle = dev.then(|| config.app.join("dist/.snapfire-build.json"));

    let serve_client = !statics.iter().any(|s| s.route == client::ROUTE);
    let static_rows: Vec<(String, PathBuf)> = statics.iter().map(|s| (s.route.clone(), s.dir.clone())).collect();
    let statics: Vec<(String, ServeDir)> = statics.into_iter().map(|s| (s.route, ServeDir::new(s.dir))).collect();

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
      client: serve_client.then(|| (client::ROUTE, client::FILES.len(), client::bytes())),
      prerender: prerendered.clone(),
      warmed: warmed_count,
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
      config: config.sources.clone(),
      inferred: config.inferred.clone(),
    };
    Ok((
      Tables {
        app,
        head,
        dev_bundle,
        statics,
        client: serve_client,
        prerendered,
        warm,
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
    "{} {:?} {} {} {} {} {:?}",
    s.store, s.client, s.key, s.ttl, s.secure, s.csrf, s.capacity
  )
}

/// Refuses a bundle that carries a server module. The plan's sources,
/// actions and handlers name their modules, `middleware.ts` is implicit,
/// and `app/<path>.ts` bundles to `dist/<path>.js`; any such output in the
/// build facts, or any output importing one, is a leak. No facts file means
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

/// Every bundle output that is a server module, or imports one, each with
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

/// `tower::Service` over the host, for hyper, axum or any tower stack.
#[derive(Clone)]
pub struct HostService(pub Arc<Host>);

impl<B> tower::Service<Request<B>> for HostService
where
  B: http_body::Body + Send + 'static,
  B::Data: Send,
  B::Error: std::fmt::Debug,
{
  type Response = Response<Body>;
  type Error = Infallible;
  type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

  fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
    Poll::Ready(Ok(()))
  }

  fn call(&mut self, req: Request<B>) -> Self::Future {
    let host = self.0.clone();
    Box::pin(async move {
      let (parts, body) = req.into_parts();
      let bytes = match body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => Bytes::new(),
      };
      Ok(host.handle(Request::from_parts(parts, bytes)).await)
    })
  }
}

impl Host {
  pub fn service(self: &Arc<Self>) -> HostService {
    HostService(self.clone())
  }

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
