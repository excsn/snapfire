//! The stock host: `config/`, `generated/plan.json` and `generated/contracts/`
//! become a `tower::Service` over `http` types. hyper serves it, axum nests
//! it, actix reaches it through the `actix` feature's shim.

pub mod config;
mod remote;
pub mod locale;
pub mod shell;

#[cfg(feature = "actix")]
pub mod actix;

use std::convert::Infallible;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use http::{header, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, StreamBody};
use snapfire_fsr::{App, AppBuilder, BindError, IntoPlan, Owner, Report};
use snapfire_fsr_auth::{Auth, AuthError, DevProvider, IdentityProvider};
use snapfire_fsr_core::{Data, ModuleId, Params, PlanNode, Value, ValueMap};
use snapfire_fsr_runtime::{
  assemble, html_stream, parse_query, wire_stream, ActionError, AssembleError, DataSource, Evaluator, FailureKind,
  FibreCache, Head, LoadError, Locale, Matcher, Metadata, RequestCtx, Resolver, SessionCell,
};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Credentials, HttpTransport, IdentityInterceptor, MockTransport, NoCredentials, Services, TraceInterceptor, Transport,
};
use snapfire_fsr_session::{MemorySessionStore, Opened, SessionConfig, SessionStore, Sessions};
use tower::ServiceExt;
use tower_http::services::ServeDir;

pub use config::{AuthSection, BearerKey, ClientConfig, Config};
pub use remote::{ServiceProvider, ServiceSessionStore};
pub use locale::{Locales, LocalesSection, Resolution};

/// The encodings a payload request may name in `enc`; the wire's `V` row
/// names the one it got.
pub const PAYLOAD_ENCODINGS: &[&str] = &["json"];

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
  Import { document: String, error: snapfire_fsr_service::ImportError },
  #[error("clients.{0}: {1}")]
  Transport(String, String),
  #[error("{0}: {1}")]
  Contract(PathBuf, String),
  #[error("no route matches `{0}`")]
  NotFound(String),
  #[error(transparent)]
  Assemble(#[from] AssembleError),
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
  Redirect { to: String, status: u16 },
  /// Answer with `status` and `body`: text when a string, JSON otherwise, empty when null.
  Respond { status: u16, body: Value },
}

impl Preflight {
  pub fn pass() -> Self {
    Self { action: PreflightAction::Continue, headers: Vec::new() }
  }

  /// Reads the value a middleware returned. Null or an empty map continues;
  /// `redirect` wins over `status`, which wins over `rewrite`; `headers` is a
  /// map of strings applied in every case.
  pub fn from_value(value: &Value) -> Result<Self, String> {
    let map = match value {
      Value::Null => return Ok(Self::pass()),
      Value::Map(map) => map,
      other => return Err(format!("middleware returned {}; expected nothing or an object", kind_of(other))),
    };
    let mut headers = Vec::new();
    if let Some(given) = map.get("headers") {
      let Value::Map(given) = given else { return Err("middleware `headers` must be an object of strings".to_owned()) };
      for (name, value) in given {
        let Value::Str(value) = value else { return Err(format!("middleware header `{name}` must be a string")) };
        headers.push((name.clone(), value.clone()));
      }
    }
    let status = match map.get("status") {
      None | Some(Value::Null) => None,
      Some(Value::Int(n)) => Some(u16::try_from(*n).map_err(|_| format!("middleware `status` {n} is not an HTTP status"))?),
      Some(other) => return Err(format!("middleware `status` must be a number, found {}", kind_of(other))),
    };
    let text = |key: &str| -> Result<Option<String>, String> {
      match map.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Str(s)) => Ok(Some(s.clone())),
        Some(other) => Err(format!("middleware `{key}` must be a string, found {}", kind_of(other))),
      }
    };
    let action = if let Some(to) = text("redirect")? {
      PreflightAction::Redirect { to, status: status.unwrap_or(307) }
    } else if let Some(status) = status {
      PreflightAction::Respond { status, body: map.get("body").cloned().unwrap_or(Value::Null) }
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
  /// Service, `http`, `grpc` or `mock`, base URL or responses file.
  pub services: Vec<(String, String, String)>,
  /// The client the sessions live behind, when the store is `service`.
  pub session: Option<String>,
  pub statics: Vec<(String, PathBuf)>,
  /// Where prerendered documents are read from, when configured.
  pub prerender: Option<PathBuf>,
  /// The render memo's capacity and lifetime, when configured.
  pub cache: Option<(u64, String)>,
  /// Whether the document carries the live-refresh script and the host
  /// answers `/__fsr/events` and `/__fsr/changed`.
  pub dev: bool,
  /// The configured locales, the default first; empty without a `[locales]` section.
  pub locales: Vec<String>,
  /// The identity provider and its login page, when one is mounted.
  pub auth: Option<(String, String)>,
  /// Client, custody key: which clients send a bearer token.
  pub bearer: Vec<(String, String)>,
  pub config: Vec<PathBuf>,
  pub inferred: Vec<String>,
}

impl std::fmt::Display for HostReport {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}", self.app)?;
    for (i, (service, kind, url)) in self.services.iter().enumerate() {
      let label = if i == 0 { "services" } else { "" };
      writeln!(f, "{label:<9} {service:<22} {kind:<11} {url}")?;
    }
    for (i, (route, dir)) in self.statics.iter().enumerate() {
      let label = if i == 0 { "static" } else { "" };
      writeln!(f, "{label:<9} {route:<22} {}", dir.display())?;
    }
    for (i, pattern) in self.app.prerenderable.iter().enumerate() {
      let label = if i == 0 { "prerender" } else { "" };
      match &self.prerender {
        Some(dir) => writeln!(f, "{label:<9} {pattern:<22} {}", dir.display())?,
        None => writeln!(f, "{label:<9} {pattern:<22} not configured")?,
      }
    }
    if let Some((capacity, ttl)) = &self.cache {
      writeln!(f, "{:<9} {capacity} entries, ttl {ttl}", "cache")?;
    }
    if let Some(client) = &self.session {
      writeln!(f, "{:<9} service via {client}", "session")?;
    }
    if self.dev {
      writeln!(f, "{:<9} live refresh on /__fsr/events, told by POST /__fsr/changed", "dev")?;
    }
    if let Some((default, others)) = self.locales.split_first() {
      let rest = if others.is_empty() { String::new() } else { format!(", {}", others.join(", ")) };
      writeln!(f, "{:<9} {default} (default, unprefixed){rest}", "locales")?;
    }
    if let Some((provider, login)) = &self.auth {
      writeln!(f, "{:<9} {provider}, login page {login}, routes /auth/login, /auth/callback and /auth/logout", "auth")?;
      if self.bearer.is_empty() {
        writeln!(f, "{:<9} none; no client carries a token", "bearer")?;
      }
    }
    for (i, (client, key)) in self.bearer.iter().enumerate() {
      let label = if i == 0 { "bearer" } else { "" };
      writeln!(f, "{label:<9} {client:<22} {key}")?;
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

pub struct Host {
  app: App,
  sessions: Sessions,
  head: Head,
  /// The bundle's build facts file, read for its id when `dev` is on; the
  /// plain head is what `prerender` writes.
  dev_bundle: Option<PathBuf>,
  changed: Option<tokio::sync::broadcast::Sender<()>>,
  statics: Vec<(String, ServeDir)>,
  prerendered: Option<PathBuf>,
  locales: Locales,
  auth: Option<Mounted>,
  csrf_always: bool,
  report_listen: String,
  pub report: HostReport,
}

/// The identity flow the host serves under `/auth/`, plus the application's
/// login page, where `begin` sends the browser and where a GET seeds the
/// flow so a typed URL can still post to the callback.
struct Mounted {
  auth: Auth,
  login_path: String,
}

/// What a request carries into a body beyond its session: the CSRF token
/// minted for it and the token custody the service layer reads. Bodies see
/// the token as the `csrf_token` prop and never see the custody.
#[derive(Clone)]
struct Incoming {
  session: SessionCell,
  csrf: Option<String>,
  credentials: Arc<dyn Credentials>,
}

impl Incoming {
  fn anonymous(session: SessionCell) -> Self {
    Self { session, csrf: None, credentials: Arc::new(NoCredentials) }
  }
}

pub struct HostBuilder {
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
  pending: Option<HostError>,
}

impl std::fmt::Debug for HostBuilder {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("HostBuilder").field("root", &self.config.root).finish_non_exhaustive()
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
    contract.merge(part, &path.file_name().unwrap_or_default().to_string_lossy()).map_err(|e| HostError::Contract(path.clone(), e.to_string()))?;
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
    let app = App::from_manifest(&plan)?;
    Ok(HostBuilder {
      config,
      plan,
      contract,
      app: Some(app),
      services: None,
      transport_override: None,
      store: None,
      shell: None,
      prerendered: None,
      identity: None,
      pending: None,
    })
  }

  pub fn report(&self) -> &HostReport {
    &self.report
  }

  /// The locales the host serves and how it resolves a request's.
  pub fn locales(&self) -> &Locales {
    &self.locales
  }

  /// Renders a route. `path` may carry a locale prefix and its query string.
  /// The session is the caller's, so a test can hand in one it prepared.
  pub async fn render(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.locales.resolve(bare, None, None);
    self.render_in(&visit, raw_query, mode, Incoming::anonymous(session)).await
  }

  async fn render_in(&self, visit: &Resolution, raw_query: &str, mode: RenderMode, incoming: Incoming) -> Result<BoxStream<'static, String>, HostError> {
    let (plan, params) = self.plan_for(&visit.path).ok_or_else(|| HostError::NotFound(visit.path.clone()))?;
    self.render_plan(&plan, params, parse_query(raw_query), mode, incoming, visit).await
  }

  /// The plan a route resolves `path` to, with its params.
  fn plan_for(&self, path: &str) -> Option<(PlanNode, Params)> {
    let matched = self.app.matcher.match_path(path)?;
    let plan = self.app.resolver.resolve(matched.entry, &matched.params)?;
    Some((plan, matched.params))
  }

  /// The intercept a soft navigation to `path` renders: of the route's
  /// `page.<slot>.tsx` plans, in file order, the first whose slot `into`
  /// names, or, without `into`, the first whose layouts the route of `from`
  /// reaches down to the one declaring its slot. `path` and `from` are paths
  /// without their query.
  pub fn intercept_for(&self, path: &str, from: Option<&str>, into: Option<&str>) -> Option<(PlanNode, Params)> {
    let (plans, params) = self.app.intercepts.plans_for(path)?;
    let from_plan = match into {
      Some(_) => None,
      None => Some(self.plan_for(from?)?.0),
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
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.locales.resolve(bare, None, None);
    self.render_navigation_in(&visit, raw_query, from, into, Incoming::anonymous(session)).await
  }

  async fn render_navigation_in(
    &self,
    visit: &Resolution,
    raw_query: &str,
    from: Option<&str>,
    into: Option<&str>,
    incoming: Incoming,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let from_bare = from.map(|f| f.split_once('?').map(|(p, _)| p).unwrap_or(f)).map(|f| self.locales.resolve(f, None, None).path);
    match self.intercept_for(&visit.path, from_bare.as_deref(), into) {
      Some((plan, params)) => self.render_plan(&plan, params, parse_query(raw_query), RenderMode::Payload, incoming, visit).await,
      None => self.render_in(visit, raw_query, RenderMode::Payload, incoming).await,
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
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.locales.resolve(bare, None, None);
    self.render_not_found_in(&visit, raw_query, mode, Incoming::anonymous(session)).await
  }

  async fn render_not_found_in(&self, visit: &Resolution, raw_query: &str, mode: RenderMode, incoming: Incoming) -> Result<Option<BoxStream<'static, String>>, HostError> {
    let Some(plan) = &self.app.not_found else { return Ok(None) };
    let mut params = Params::new();
    params.insert("path".to_owned(), visit.path.clone());
    Ok(Some(self.render_plan(plan, params, parse_query(raw_query), mode, incoming, visit).await?))
  }

  /// The head a request renders under: the boot head, plus the live-refresh
  /// script when `dev` is on and a canonical link when a prefixed request
  /// asked for the default locale.
  async fn render_plan(
    &self,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    visit: &Resolution,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let mut extra = Vec::new();
    if let Some(facts) = &self.dev_bundle {
      extra.push(snapfire_fsr_core::Node::raw(shell::dev_script(&bundle_id(facts))));
    }
    if visit.prefixed && visit.locale.is_default {
      extra.push(snapfire_fsr_core::Node::raw(shell::canonical(&visit.path)));
    }
    if extra.is_empty() {
      return self.render_plan_with(plan, params, query, mode, incoming, &visit.locale, &self.head).await;
    }
    let mut head = self.head.clone();
    let mut parts = vec![self.head.rest.clone()];
    parts.extend(extra);
    head.rest = snapfire_fsr_core::Node::Seq(parts);
    self.render_plan_with(plan, params, query, mode, incoming, &visit.locale, &head).await
  }

  async fn render_plan_with(
    &self,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    incoming: Incoming,
    locale: &Locale,
    head: &Head,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let ctx = self.ctx(incoming, params, query, locale.clone());
    let assembly = assemble(&self.app.runtime, plan, &ctx, head).await?;
    Ok(match mode {
      RenderMode::Html => Box::pin(html_stream(assembly)),
      RenderMode::Payload => Box::pin(wire_stream(assembly)),
    })
  }

  /// Renders to one string, for tests.
  pub async fn render_navigation_to_string(&self, path: &str, from: Option<&str>, into: Option<&str>, session: SessionCell) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render_navigation(path, from, into, session).await?.collect().await;
    Ok(chunks.concat())
  }

  pub async fn render_to_string(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render(path, mode, session).await?.collect().await;
    Ok(chunks.concat())
  }

  /// The patterns one render serves for every request: no parameter, every
  /// source lowered and reading nothing of the request, no page or layout
  /// reading its `identity` or `csrf_token` prop.
  pub fn prerenderable(&self) -> &[String] {
    &self.app.prerenderable
  }

  /// Drops every cached subtree of the plan node keyed `plan_key`, a module
  /// name for a lowered page or layout, and says how many went. Zero when
  /// nothing was cached under it or no cache is configured.
  pub async fn invalidate(&self, plan_key: &str) -> usize {
    self.app.invalidate(plan_key).await
  }

  /// Renders every prerenderable route once per locale, anonymously, writing
  /// the document as `<out>/<path>/index.html` and the payload beside it as
  /// `index.payload`; `/` lands at the top of `out`. A locale other than the
  /// default lands under its tag, `<out>/fr_FR/about/index.html`. Returns
  /// what was written, each path with its prefix.
  pub async fn prerender(&self, out: &Path) -> Result<Vec<(String, PathBuf)>, HostError> {
    let mut written = Vec::new();
    for pattern in self.app.prerenderable.clone() {
      for tag in self.locales.supported.clone() {
        let locale = self.locales.locale(&tag);
        let root = if locale.is_default { out.to_path_buf() } else { out.join(&tag) };
        let dir = root.join(pattern.trim_matches('/'));
        std::fs::create_dir_all(&dir).map_err(|e| HostError::Io(dir.clone(), e))?;
        let served = if locale.is_default { pattern.clone() } else { format!("/{tag}{}", pattern.trim_end_matches('/')) };
        for (mode, name) in [(RenderMode::Html, "index.html"), (RenderMode::Payload, "index.payload")] {
          let (plan, params) = self.plan_for(&pattern).ok_or_else(|| HostError::NotFound(pattern.clone()))?;
          let chunks = self.render_plan_with(&plan, params, Params::new(), mode, Incoming::anonymous(SessionCell::default()), &locale, &self.head).await?;
          let text: String = chunks.collect::<Vec<_>>().await.concat();
          let file = dir.join(name);
          std::fs::write(&file, text).map_err(|e| HostError::Io(file.clone(), e))?;
          written.push((served.clone(), file));
        }
      }
    }
    Ok(written)
  }

  /// The prerendered text for `path` in `mode`, when the prerender directory
  /// holds one. `path` may carry a locale prefix. The query string is
  /// ignored: a prerenderable route reads none.
  pub fn prerendered(&self, path: &str, mode: RenderMode) -> Option<String> {
    let path = path.split_once('?').map(|(p, _)| p).unwrap_or(path);
    let visit = self.locales.resolve(path, None, None);
    self.prerendered_in(&visit.path, mode, &visit.locale)
  }

  fn prerendered_in(&self, path: &str, mode: RenderMode, locale: &Locale) -> Option<String> {
    let dir = self.prerendered.as_ref()?;
    let root = if locale.is_default { dir.clone() } else { dir.join(&locale.tag) };
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
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.locales.resolve(bare, None, None);
    self.preflight_in(method, &visit.path, raw_query, Incoming::anonymous(session), &visit.locale).await
  }

  async fn preflight_in(&self, method: &str, path: &str, raw_query: &str, incoming: Incoming, locale: &Locale) -> Result<Preflight, ActionError> {
    let Some(middleware) = &self.app.middleware else { return Ok(Preflight::pass()) };
    let mut request = ValueMap::new();
    request.insert("method".to_owned(), Value::Str(method.to_ascii_uppercase()));
    request.insert("path".to_owned(), Value::Str(path.to_owned()));
    request.insert("payload".to_owned(), Value::Bool(raw_query.split('&').any(|p| p == "__payload")));
    let ctx = self.ctx(incoming, Params::new(), parse_query(raw_query), locale.clone());
    let value = middleware.call(ctx, Value::Map(request)).await?;
    Preflight::from_value(&value).map_err(|message| ActionError::new(snapfire_fsr_runtime::FailureKind::Internal, message))
  }

  /// The handler matching `method` and `path`, run with `input` as the
  /// request body. `path` may carry a locale prefix and a query string.
  /// `NotFound` when no handler matches.
  pub async fn call_handler(&self, method: &str, path: &str, session: SessionCell, input: Value) -> Result<Value, ActionError> {
    let (bare, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let visit = self.locales.resolve(bare, None, None);
    self.call_handler_in(method, &visit.path, raw_query, Incoming::anonymous(session), &visit.locale, input).await
  }

  async fn call_handler_in(&self, method: &str, path: &str, raw_query: &str, incoming: Incoming, locale: &Locale, input: Value) -> Result<Value, ActionError> {
    let Some(found) = self.app.handlers.match_request(method, path) else {
      return Err(ActionError::new(snapfire_fsr_runtime::FailureKind::NotFound, format!("no handler for {} {path}", method.to_ascii_uppercase())));
    };
    let ctx = self.ctx(incoming, found.params, parse_query(raw_query), locale.clone());
    self.app.handlers.dispatch(&found.id, ctx, input).await
  }

  /// `call_action_in` under the default locale.
  pub async fn call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError> {
    self.call_action_in(id, session, self.locales.default_locale(), input).await
  }

  /// Runs an action with `locale` as its `ctx.locale`, which at the edge is
  /// the locale of the document that called it.
  pub async fn call_action_in(&self, id: &str, session: SessionCell, locale: Locale, input: Value) -> Result<Value, ActionError> {
    self.dispatch_action(id, Incoming::anonymous(session), locale, input).await
  }

  async fn dispatch_action(&self, id: &str, incoming: Incoming, locale: Locale, input: Value) -> Result<Value, ActionError> {
    let ctx = self.ctx(incoming, Params::new(), Params::new(), locale);
    self.app.actions.dispatch(id, ctx, input).await
  }

  /// The context a body runs in: the services bound to the session's identity
  /// and the request's custody, the token as `csrf`.
  fn ctx(&self, incoming: Incoming, params: Params, query: Params, locale: Locale) -> RequestCtx {
    let services = self.app.services.bind(incoming.session.identity(), incoming.credentials);
    RequestCtx { params, query, session: incoming.session, locale, csrf: incoming.csrf, services }
  }

  /// What a request at the edge carries: the session, its custody and, once
  /// the session is identified, a CSRF token. An anonymous request carries no
  /// token so its renders share the memo; the token joins the memo key.
  fn incoming(&self, opened: &Opened) -> Incoming {
    let csrf = (self.csrf_always || opened.cell.identity().is_some()).then(|| self.sessions.csrf_token(&opened.id));
    Incoming { session: opened.cell.clone(), csrf, credentials: Arc::new(opened.tokens.clone()) }
  }

  /// The whole edge for one request: static roots, the action route, then a
  /// page in either mode, with the session opened from the cookie and
  /// persisted into the response.
  /// Tells every open development document that something changed, so each
  /// refreshes its route in place. Nothing happens when `dev` is off.
  pub fn changed(&self) {
    if let Some(tx) = &self.changed {
      let _ = tx.send(());
    }
  }

  /// A server-sent event stream: one event on open and one per `changed`
  /// call, each `data: {"bundle":"<id>"}` with the bundle id of that moment,
  /// until the client goes away.
  fn events(&self) -> Response<Body> {
    let (Some(tx), Some(facts)) = (&self.changed, self.dev_bundle.clone()) else { return text_response(StatusCode::NOT_FOUND, "dev is off".to_owned()) };
    let rx = tx.subscribe();
    let event = move || Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(format!("data: {{\"bundle\":\"{}\"}}\n\n", bundle_id(&facts)))));
    let greeting = event();
    let opened = futures_util::stream::once(async move { greeting });
    let changes = futures_util::stream::unfold((rx, event), |(mut rx, event)| async move {
      loop {
        match rx.recv().await {
          Ok(()) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => return Some((event(), (rx, event))),
          Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
        }
      }
    });
    Response::builder()
      .status(StatusCode::OK)
      .header(header::CONTENT_TYPE, "text/event-stream")
      .header(header::CACHE_CONTROL, "no-cache")
      .body(StreamBody::new(opened.chain(changes)).boxed_unsync())
      .expect("an event stream")
  }

  pub async fn handle(&self, req: Request<Bytes>) -> Response<Body> {
    let path = req.uri().path().to_owned();

    if self.changed.is_some() {
      if path == "/__fsr/events" && req.method() == Method::GET {
        return self.events();
      }
      if path == "/__fsr/changed" && req.method() == Method::POST {
        self.changed();
        return Response::builder().status(StatusCode::NO_CONTENT).body(Body::default()).expect("an empty response");
      }
    }

    for (route, dir) in &self.statics {
      if let Some(rest) = path.strip_prefix(route.as_str()) {
        if rest.is_empty() || rest.starts_with('/') {
          let mut inner = Request::builder().method(req.method().clone()).uri(if rest.is_empty() { "/" } else { rest });
          for (name, value) in req.headers() {
            inner = inner.header(name, value);
          }
          let inner = inner.body(Bytes::new()).expect("a request rebuilt from a request");
          return match dir.clone().oneshot(inner).await {
            Ok(response) => {
              let mut response = response.map(|b| b.map_err(std::io::Error::other).boxed_unsync());
              if self.changed.is_some() {
                response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
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

    let is_action = req.method() == Method::POST && path.starts_with("/_sf/action/");
    let visit = if is_action {
      let from = header("x-sf-from").map(|f| f.split('?').next().unwrap_or(&f).to_owned()).unwrap_or_else(|| "/".to_owned());
      let resolved = self.locales.resolve(&from, cookie.as_deref(), accept_language.as_deref());
      Resolution { locale: resolved.locale, path: path.clone(), prefixed: false, set_cookie: None }
    } else {
      self.locales.resolve(&path, cookie.as_deref(), accept_language.as_deref())
    };
    let framework_owned = visit.path.starts_with("/_sf/") || visit.path.starts_with("/__fsr/") || (self.auth.is_some() && visit.path.starts_with("/auth/"));
    if visit.prefixed && framework_owned {
      return text_response(StatusCode::NOT_FOUND, format!("no route: {path}"));
    }
    let raw_query = req.uri().query().unwrap_or("").to_owned();
    let mut response = self.handle_resolved(req, &opened, visit.clone(), raw_query).await;
    if let Some(set_cookie) = &visit.set_cookie {
      if let Ok(value) = HeaderValue::from_str(set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
    }
    response
  }

  /// The request past the statics and the locale: the middleware, then the
  /// action route, a handler or a page.
  async fn handle_resolved(&self, req: Request<Bytes>, opened: &snapfire_fsr_session::Opened, visit: Resolution, raw_query: String) -> Response<Body> {
    let path = visit.path.clone();
    if let Some(mounted) = &self.auth {
      if let Some(response) = self.auth_route(mounted, &req, opened, &path, &raw_query).await {
        return response;
      }
    }
    let asked = if raw_query.is_empty() { path.clone() } else { format!("{path}?{raw_query}") };
    let preflight = match self.preflight_in(req.method().as_str(), &path, &raw_query, self.incoming(opened), &visit.locale).await {
      Ok(preflight) => preflight,
      Err(e) => {
        return json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        )
      }
    };
    let (path, target) = match &preflight.action {
      PreflightAction::Continue => (path, asked),
      PreflightAction::Rewrite(to) => {
        let (to_path, to_query) = to.split_once('?').unwrap_or((to.as_str(), ""));
        let query = if to_query.is_empty() { raw_query.clone() } else { to_query.to_owned() };
        let target = if query.is_empty() { to_path.to_owned() } else { format!("{to_path}?{query}") };
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
          Value::Null => Response::builder().status(status).body(Body::default()).expect("an empty response"),
          Value::Str(text) => text_response(status, text.clone()),
          other => json_response(status, &snapfire_fsr_payload::value_to_json(other)),
        };
        with_headers(&mut response, &preflight.headers);
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    };
    let mut response = self.respond(req, opened, path, target, &raw_query, &visit).await;
    with_headers(&mut response, &preflight.headers);
    response
  }

  async fn respond(&self, req: Request<Bytes>, opened: &snapfire_fsr_session::Opened, path: String, target: String, raw_query: &str, visit: &Resolution) -> Response<Body> {
    if req.method() == Method::POST {
      if let Some(id) = path.strip_prefix("/_sf/action/") {
        let is_form = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));
        let input = if is_form {
          let mut fields = form_params(req.body());
          let token = match fields.shift_remove("_csrf") {
            Some(Value::Str(token)) => token,
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
            Err(e) => return json_response(StatusCode::BAD_REQUEST, &serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })),
          }
        };
        let mut response = match self.dispatch_action(id, self.incoming(opened), visit.locale.clone(), input).await {
          Ok(_) if is_form => {
            let back = req.headers().get(header::REFERER).and_then(|v| v.to_str().ok()).and_then(referer_path).unwrap_or_else(|| "/".to_owned());
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

    if self.app.handlers.match_request(req.method().as_str(), &path).is_some() {
      let input = if req.body().is_empty() {
        Value::Null
      } else {
        match serde_json::from_slice::<serde_json::Value>(req.body())
          .map_err(|e| e.to_string())
          .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
        {
          Ok(value) => value,
          Err(e) => return json_response(StatusCode::BAD_REQUEST, &serde_json::json!({ "kind": "invalid", "message": format!("invalid request body: {e}") })),
        }
      };
      let (target_path, target_query) = target.split_once('?').unwrap_or((target.as_str(), ""));
      let mut response = match self.call_handler_in(req.method().as_str(), target_path, target_query, self.incoming(opened), &visit.locale, input).await {
        Ok(value) => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
        Err(e) => json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        ),
      };
      self.set_cookie(opened, &mut response).await;
      return response;
    }

    let mode = if raw_query.split('&').any(|p| p == "__payload") { RenderMode::Payload } else { RenderMode::Html };
    if mode == RenderMode::Payload {
      if let Some(asked) = parse_query(raw_query).get("enc").filter(|enc| !PAYLOAD_ENCODINGS.contains(&enc.as_str())) {
        return text_response(StatusCode::NOT_ACCEPTABLE, format!("unsupported payload encoding `{asked}`; the encodings are {}", PAYLOAD_ENCODINGS.join(", ")));
      }
    }
    tracing::info!(target: "fsr::host", path = %path, payload = (mode == RenderMode::Payload), "request");
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let (from, into) = match mode {
      RenderMode::Payload => (header("x-sf-from"), header("x-sf-into")),
      RenderMode::Html => (None, None),
    };
    let intercepted = (from.is_some() || into.is_some()) && self.intercept_for(&path, from.as_deref().map(|f| f.split('?').next().unwrap_or(f)), into.as_deref()).is_some();

    if req.method() == Method::GET && !intercepted {
      if let Some(text) = self.prerendered_in(&path, mode, &visit.locale) {
        let content_type = match mode {
          RenderMode::Html => "text/html; charset=utf-8",
          RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
        };
        let mut response = Response::builder()
          .status(StatusCode::OK)
          .header(header::CONTENT_TYPE, content_type)
          .header("x-sf-prerendered", "1")
          .body(http_body_util::Full::new(Bytes::from(text)).map_err(|never: std::convert::Infallible| match never {}).boxed_unsync())
          .expect("a response with a valid header");
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    }

    let (target_path, target_query) = target.split_once('?').unwrap_or((target.as_str(), ""));
    let target_visit = Resolution { locale: visit.locale.clone(), path: target_path.to_owned(), prefixed: visit.prefixed, set_cookie: None };
    let rendered = if intercepted {
      self.render_navigation_in(&target_visit, target_query, from.as_deref(), into.as_deref(), self.incoming(opened)).await
    } else {
      self.render_in(&target_visit, target_query, mode, self.incoming(opened)).await
    };
    let rendered = match rendered {
      Ok(chunks) => Ok((StatusCode::OK, chunks)),
      Err(HostError::NotFound(path)) => match self.render_not_found_in(&target_visit, target_query, mode, self.incoming(opened)).await {
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
    let set_cookie = if self.csrf_always { self.sessions.establish(opened).await } else { self.sessions.persist(opened).await };
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
  async fn auth_route(&self, mounted: &Mounted, req: &Request<Bytes>, opened: &Opened, path: &str, raw_query: &str) -> Option<Response<Body>> {
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
            let back: String = pending.map(|p| form_urlencoded::byte_serialize(p.as_bytes()).collect()).unwrap_or_default();
            see_other(&format!("{}?error=denied&return_to={back}", mounted.login_path))
          }
          Err(e @ AuthError::Invalid(_)) => text_response(StatusCode::BAD_REQUEST, e.to_string()),
        };
        self.set_cookie(opened, &mut response).await;
        Some(response)
      }
      (&Method::POST, "/auth/logout") => {
        let token = form_field(req.body(), "_csrf").or_else(|| header("x-sf-csrf")).unwrap_or_default();
        if !self.sessions.verify_csrf(&opened.id, &token) {
          return Some(text_response(StatusCode::FORBIDDEN, "csrf verification failed".to_owned()));
        }
        mounted.auth.logout(opened);
        let expire = self.sessions.destroy(opened).await;
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

  /// Serves on `listen` with hyper, HTTP/1. `Host::listen` is the configured
  /// address.
  pub async fn serve(self: Arc<Self>, listen: &str) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(listen).await?;
    self.serve_listener(listener).await
  }

  /// Serves an already bound listener, which is how a test picks port zero.
  pub async fn serve_listener(self: Arc<Self>, listener: tokio::net::TcpListener) -> std::io::Result<()> {
    loop {
      let (stream, _) = listener.accept().await?;
      let host = self.clone();
      tokio::spawn(async move {
        let io = hyper_util::rt::TokioIo::new(stream);
        let service = hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
          let host = host.clone();
          async move {
            let (parts, body) = req.into_parts();
            let bytes = match body.collect().await {
              Ok(collected) => collected.to_bytes(),
              Err(_) => Bytes::new(),
            };
            Ok::<_, Infallible>(host.handle(Request::from_parts(parts, bytes)).await)
          }
        });
        if let Err(e) = hyper::server::conn::http1::Builder::new().serve_connection(io, service).await {
          tracing::debug!(target: "fsr::host", error = %e, "connection ended");
        }
      });
    }
  }

  pub fn listen(&self) -> &str {
    &self.report_listen
  }
}

/// What tells one bundle from the next: a hash over the content of every
/// output the build facts list, source maps aside, so a rebundle that wrote
/// the same modules keeps its id and an edited module changes it. `-` when
/// there is no bundle.
fn bundle_id(facts: &Path) -> String {
  use std::hash::{Hash, Hasher};
  let Ok(text) = std::fs::read_to_string(facts) else { return "-".to_owned() };
  let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) else { return "-".to_owned() };
  let dir = facts.parent().unwrap_or(Path::new("."));
  let mut hasher = std::collections::hash_map::DefaultHasher::new();
  for output in json["outputs"].as_array().into_iter().flatten().filter_map(|o| o.as_str()) {
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
    if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(name.as_bytes()), HeaderValue::from_str(value)) {
      response.headers_mut().append(name, value);
    }
  }
}

/// A `mock` client's transport from its responses file: an object of method
/// name to the response in the payload's JSON encoding, or to
/// `{"$fail": {"kind": "<failure kind>", "message": "..."}}` for a failure.
fn mock_transport(config: &Config, name: &str, client: &ClientConfig) -> Result<Option<(Arc<dyn Transport>, String)>, HostError> {
  if !client.is_mock() {
    return Ok(None);
  }
  let file = client.responses_file(name);
  let path = config.resolve(&file);
  let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
  let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| HostError::Config(path.clone(), e.to_string()))?;
  let Some(entries) = json.as_object() else {
    return Err(HostError::Config(path, "expected an object of method name to response".to_owned()));
  };
  let mut mock = MockTransport::new();
  for (method, response) in entries {
    let key = format!("{name}.{method}");
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
          other => return Err(HostError::Config(path, format!("{method}: unknown failure kind `{other}`"))),
        };
        let message = fail.get("message").and_then(|m| m.as_str()).unwrap_or("mocked failure").to_owned();
        mock = mock.fails(key, kind, message);
      }
      None => {
        let value = snapfire_fsr_payload::json_to_value(response).map_err(|e| HostError::Config(path.clone(), format!("{method}: {e}")))?;
        mock = mock.returns(key, value);
      }
    }
  }
  Ok(Some((Arc::new(mock), file)))
}

fn json_response(status: StatusCode, json: &serde_json::Value) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "application/json")
    .body(http_body_util::Full::new(Bytes::from(json.to_string())).map_err(|never| match never {}).boxed_unsync())
    .expect("a json response")
}

/// The slot an intercept plan fills: the child of the node that keeps the
/// page which the plan fills instead. A node that keeps other slots but still
/// fills its own `content` is a layout on the way down, not the one declaring
/// the slot, so the walk continues through it.
fn intercept_slot(plan: &PlanNode) -> Option<String> {
  if plan.keep.iter().any(|name| name.0 == "content") {
    return plan.children.iter().find(|(name, _)| !plan.keep.contains(name)).map(|(name, _)| name.0.clone());
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

fn text_response(status: StatusCode, text: String) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(http_body_util::Full::new(Bytes::from(text)).map_err(|never| match never {}).boxed_unsync())
    .expect("a text response")
}

fn see_other(location: &str) -> Response<Body> {
  match Response::builder().status(StatusCode::SEE_OTHER).header(header::LOCATION, location).body(Body::default()) {
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

fn form_params(body: &[u8]) -> ValueMap {
  form_urlencoded::parse(body).map(|(k, v)| (k.into_owned(), Value::Str(v.into_owned()))).collect()
}

fn form_field(body: &[u8], name: &str) -> Option<String> {
  match form_params(body).get(name) {
    Some(Value::Str(value)) => Some(value.clone()),
    _ => None,
  }
}

/// What the provider's callback receives: the query on a GET, a form or a
/// JSON object on a POST.
fn callback_params(req: &Request<Bytes>, query: &Params) -> Result<ValueMap, String> {
  if req.method() == Method::GET {
    return Ok(query.iter().map(|(k, v)| (k.clone(), Value::Str(v.clone()))).collect());
  }
  let content_type = req.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
  if content_type.starts_with("application/json") {
    let json: serde_json::Value = serde_json::from_slice(req.body()).map_err(|e| format!("invalid callback body: {e}"))?;
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

  /// The identity provider behind `/auth/login`, `/auth/callback` and
  /// `/auth/logout`, in place of the one `[auth]` names. The login page is
  /// `auth.login` when the section is written, `/login` otherwise.
  pub fn identity(mut self, provider: Arc<dyn IdentityProvider>) -> Self {
    self.identity = Some(provider);
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

  pub fn build(mut self) -> Result<Host, HostError> {
    if let Some(e) = self.pending.take() {
      return Err(e);
    }
    let config = self.config;
    let _ = self.plan;

    let mut service_rows = Vec::new();
    let bearer_rows: Vec<(String, String)> = config
      .clients
      .iter()
      .filter_map(|(name, client)| client.bearer.as_ref().and_then(BearerKey::key).map(|key| (name.clone(), key.to_owned())))
      .collect();
    let services = match self.services {
      Some(services) => services,
      None => {
        let mut contract = self.contract.clone().unwrap_or_default();
        let mut transports: Vec<(String, Arc<dyn Transport>)> = Vec::new();
        for (name, client) in &config.clients {
          let document = client.document.clone().unwrap_or_else(|| format!("clients/{name}.openapi.json"));
          let path = config.resolve(&document);
          if document.ends_with(".proto") {
            let imported = snapfire_fsr_service::import_proto(&path, name).map_err(|error| HostError::Import { document, error })?;
            contract.types.extend(imported.contract.types.clone());
            contract.services.extend(imported.contract.services.clone());
            if let Some((transport, file)) = mock_transport(&config, name, client)? {
              transports.push((name.clone(), transport));
              service_rows.push((name.clone(), "mock".to_owned(), file));
              continue;
            }
            let base_url = client.base_url.clone().unwrap_or_default();
            if self.transport_override.is_none() {
              let transport = snapfire_fsr_service::GrpcTransport::new(&base_url, &imported).map_err(|e| HostError::Transport(name.clone(), e))?;
              transports.push((name.clone(), Arc::new(transport)));
            }
            service_rows.push((name.clone(), "grpc".to_owned(), base_url));
            continue;
          }
          let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
          let imported = snapfire_fsr_service::import(&text, name)
            .map_err(|error| HostError::Import { document, error })?;
          contract.types.extend(imported.contract.types.clone());
          contract.services.extend(imported.contract.services.clone());
          if let Some((transport, file)) = mock_transport(&config, name, client)? {
            transports.push((name.clone(), transport));
            service_rows.push((name.clone(), "mock".to_owned(), file));
            continue;
          }
          let base_url = client.base_url.clone().unwrap_or_default();
          let mut transport = HttpTransport::new(&base_url);
          for (path, route) in &imported.routes {
            transport = transport.route(path.clone(), route.clone());
          }
          transports.push((name.clone(), Arc::new(transport)));
          service_rows.push((name.clone(), "http".to_owned(), base_url));
        }
        let mut builder = Services::builder()
          .contract(contract)
          .intercept(Arc::new(TraceInterceptor::new()))
          .intercept(Arc::new(IdentityInterceptor::new()));
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
        builder.build()
      }
    };

    let shell_path = config.document.shell.split('#').next().unwrap_or("shell").to_owned();
    let shell: Arc<dyn Evaluator> = self.shell.take().unwrap_or_else(|| Arc::new(shell::DocumentShell));
    let mut app = self.app.take().expect("the builder holds its app until build");
    if let Some(contract) = self.contract.take() {
      app = app.contract(contract);
    }
    let cache_row = match (config.cache_ttl()?, &config.cache) {
      (Some(ttl), Some(section)) => {
        app = app.cache(Arc::new(FibreCache::bounded(section.capacity, ttl)));
        Some((section.capacity, section.ttl.clone()))
      }
      _ => None,
    };
    let app = app
      .services(services)
      .evaluator(move |m: &ModuleId| m.path == shell_path, shell)
      .build()?;

    let ttl = config.session_ttl()?;
    let store: Arc<dyn SessionStore> = match self.store.take() {
      Some(store) => store,
      None => match config.session.store.as_str() {
        "memory" => Arc::new(MemorySessionStore::new(config.session.capacity, ttl)),
        "service" => {
          let client = config.session.client.clone().unwrap_or_default();
          Arc::new(ServiceSessionStore::new(app.services.clone(), client))
        }
        other => return Err(HostError::Value("session.store".to_owned(), other.to_owned())),
      },
    };
    let sessions = Sessions::new(
      store,
      config.session.key.as_bytes(),
      SessionConfig { ttl, secure: config.session.secure, ..SessionConfig::default() },
    );

    let import_map = match &config.document.import_map {
      Some(rel) => {
        let path = config.resolve(rel);
        Some(std::fs::read_to_string(&path).map_err(|e| HostError::Io(path, e))?)
      }
      None => None,
    };
    let styles = config.document.styles.clone().unwrap_or_default();
    let head = shell::head(&config.document.title, &styles, import_map.as_deref(), config.document.entry.as_deref());
    let dev = config.dev();
    let dev_bundle = dev.then(|| config.app.join("dist/.snapfire-build.json"));
    let changed = dev.then(|| tokio::sync::broadcast::channel(16).0);

    let mut statics = Vec::new();
    let mut static_rows = Vec::new();
    for root in &config.statics {
      let dir = config.resolve(&root.dir);
      static_rows.push((root.route.clone(), dir.clone()));
      statics.push((root.route.trim_end_matches('/').to_owned(), ServeDir::new(dir)));
    }

    let prerendered = self.prerendered.take().or_else(|| config.server.prerender.as_deref().map(|rel| config.resolve(rel)));
    let locales = match &config.locales {
      Some(section) => Locales::from_section(section).map_err(|e| HostError::Config(config.sources.first().cloned().unwrap_or_else(|| config.root.clone()), e))?,
      None => Locales::single(),
    };
    let locale_rows = match &config.locales {
      Some(_) => {
        let mut rows = vec![locales.default.clone()];
        rows.extend(locales.supported.iter().filter(|t| **t != locales.default).cloned());
        rows
      }
      None => Vec::new(),
    };
    let auth = match (self.identity.take(), &config.auth) {
      (Some(provider), section) => {
        let login_path = section.as_ref().map(|s| s.login.clone()).unwrap_or_else(|| "/login".to_owned());
        Some((Mounted { auth: Auth::new(provider), login_path }, "custom".to_owned()))
      }
      (None, Some(section)) => {
        let provider: Arc<dyn IdentityProvider> = match section.provider.as_str() {
          "file" => {
            let users = config.config_dir().join(section.users.as_deref().unwrap_or("auth.toml"));
            Arc::new(DevProvider::from_toml(&section.login, &users).map_err(|e| HostError::Config(users.clone(), e))?)
          }
          "service" => {
            let client = section.client.clone().unwrap_or_default();
            Arc::new(ServiceProvider::new(app.services.clone(), client, section.login.clone()))
          }
          other => return Err(HostError::Value("auth.provider".to_owned(), other.to_owned())),
        };
        let name = match &section.client {
          Some(client) if section.provider == "service" => format!("service via {client}"),
          _ => section.provider.clone(),
        };
        Some((Mounted { auth: Auth::new(provider), login_path: section.login.clone() }, name))
      }
      (None, None) => None,
    };
    let auth_row = auth.as_ref().map(|(mounted, name)| (name.clone(), mounted.login_path.clone()));
    let auth = auth.map(|(mounted, _)| mounted);
    let report = HostReport {
      app: app.report.clone(),
      services: service_rows,
      session: (config.session.store == "service").then(|| config.session.client.clone().unwrap_or_default()),
      statics: static_rows,
      prerender: prerendered.clone(),
      cache: cache_row,
      dev,
      locales: locale_rows,
      auth: auth_row,
      bearer: bearer_rows,
      config: config.sources.clone(),
      inferred: config.inferred.clone(),
    };
    Ok(Host { app, sessions, head, dev_bundle, changed, statics, prerendered, locales, auth, csrf_always: config.session.csrf == "always", report, report_listen: config.server.listen })
  }
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
    self.report.app.sources.iter().find(|(n, _)| n == name).map(|(_, o)| *o)
  }
}

