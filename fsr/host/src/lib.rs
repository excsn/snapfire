//! The stock host: `config/`, `generated/plan.json` and `generated/contracts/`
//! become a `tower::Service` over `http` types. hyper serves it, axum nests
//! it, actix reaches it through the `actix` feature's shim.

pub mod config;
pub mod shell;

#[cfg(feature = "actix")]
pub mod actix;

use std::convert::Infallible;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use futures_util::stream::BoxStream;
use futures_util::StreamExt;
use http::{header, HeaderValue, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, StreamBody};
use snapfire_fsr::{App, AppBuilder, BindError, IntoPlan, Owner, Report};
use snapfire_fsr_core::{Data, ModuleId, Node, Params, PlanNode, Value};
use snapfire_fsr_runtime::{
  assemble, html_stream, parse_query, wire_stream, ActionError, AssembleError, DataSource, Evaluator,
  LoadError, Matcher, RequestCtx, Resolver, SessionCell,
};
use snapfire_fsr_service::{
  Contract, HttpTransport, IdentityInterceptor, Services, TraceInterceptor, Transport,
};
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, SessionStore, Sessions};
use tower::ServiceExt;
use tower_http::services::ServeDir;

pub use config::Config;

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
  /// Service, `http` or `grpc`, base URL.
  pub services: Vec<(String, String, String)>,
  pub statics: Vec<(String, PathBuf)>,
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
  head: Node,
  statics: Vec<(String, ServeDir)>,
  report_listen: String,
  pub report: HostReport,
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
      pending: None,
    })
  }

  pub fn report(&self) -> &HostReport {
    &self.report
  }

  /// Renders a route. `path` may carry its query string. The session is the
  /// caller's, so a test can hand in one it prepared.
  pub async fn render(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let (path, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let query = parse_query(raw_query);
    let matched = self.app.matcher.match_path(path).ok_or_else(|| HostError::NotFound(path.to_owned()))?;
    let plan = self
      .app
      .resolver
      .resolve(matched.entry, &matched.params)
      .ok_or_else(|| HostError::NotFound(path.to_owned()))?;
    self.render_plan(&plan, matched.params, query, mode, session).await
  }

  /// The application's not-found tree for a path no route matches, or `None`
  /// when it has none. `params.path` carries the path the tree is answering.
  pub async fn render_not_found(
    &self,
    path: &str,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<Option<BoxStream<'static, String>>, HostError> {
    let Some(plan) = &self.app.not_found else { return Ok(None) };
    let (path, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let mut params = Params::new();
    params.insert("path".to_owned(), path.to_owned());
    Ok(Some(self.render_plan(plan, params, parse_query(raw_query), mode, session).await?))
  }

  async fn render_plan(
    &self,
    plan: &PlanNode,
    params: Params,
    query: Params,
    mode: RenderMode,
    session: SessionCell,
  ) -> Result<BoxStream<'static, String>, HostError> {
    let services = self.app.services.bind(session.identity(), Arc::new(snapfire_fsr_service::NoCredentials));
    let ctx = RequestCtx { params, query, session, csrf: None, services };
    let assembly = assemble(&self.app.runtime, plan, &ctx, &self.head).await?;
    Ok(match mode {
      RenderMode::Html => Box::pin(html_stream(assembly)),
      RenderMode::Payload => Box::pin(wire_stream(assembly)),
    })
  }

  /// Renders to one string, for tests.
  pub async fn render_to_string(&self, path: &str, mode: RenderMode, session: SessionCell) -> Result<String, HostError> {
    let chunks: Vec<String> = self.render(path, mode, session).await?.collect().await;
    Ok(chunks.concat())
  }

  /// The handler matching `method` and `path`, run with `input` as the
  /// request body. `path` may carry a query string. `NotFound` when no
  /// handler matches.
  pub async fn call_handler(&self, method: &str, path: &str, session: SessionCell, input: Value) -> Result<Value, ActionError> {
    let (path, raw_query) = path.split_once('?').unwrap_or((path, ""));
    let Some(found) = self.app.handlers.match_request(method, path) else {
      return Err(ActionError::new(snapfire_fsr_runtime::FailureKind::NotFound, format!("no handler for {} {path}", method.to_ascii_uppercase())));
    };
    let services = self.app.services.bind(session.identity(), Arc::new(snapfire_fsr_service::NoCredentials));
    let ctx = RequestCtx { params: found.params, query: parse_query(raw_query), session, csrf: None, services };
    self.app.handlers.dispatch(&found.id, ctx, input).await
  }

  pub async fn call_action(&self, id: &str, session: SessionCell, input: Value) -> Result<Value, ActionError> {
    let services = self.app.services.bind(session.identity(), Arc::new(snapfire_fsr_service::NoCredentials));
    let ctx = RequestCtx { params: Default::default(), query: Default::default(), session, csrf: None, services };
    self.app.actions.dispatch(id, ctx, input).await
  }

  /// The whole edge for one request: static roots, the action route, then a
  /// page in either mode, with the session opened from the cookie and
  /// persisted into the response.
  pub async fn handle(&self, req: Request<Bytes>) -> Response<Body> {
    let path = req.uri().path().to_owned();

    for (route, dir) in &self.statics {
      if let Some(rest) = path.strip_prefix(route.as_str()) {
        if rest.is_empty() || rest.starts_with('/') {
          let mut inner = Request::builder().method(req.method().clone()).uri(if rest.is_empty() { "/" } else { rest });
          for (name, value) in req.headers() {
            inner = inner.header(name, value);
          }
          let inner = inner.body(Bytes::new()).expect("a request rebuilt from a request");
          return match dir.clone().oneshot(inner).await {
            Ok(response) => response.map(|b| b.map_err(std::io::Error::other).boxed_unsync()),
            Err(never) => match never {},
          };
        }
      }
    }

    let cookie = req.headers().get(header::COOKIE).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let opened = self.sessions.open(cookie.as_deref()).await;

    if req.method() == Method::POST {
      if let Some(id) = path.strip_prefix("/_sf/action/") {
        let input = match serde_json::from_slice::<serde_json::Value>(req.body())
          .map_err(|e| e.to_string())
          .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
        {
          Ok(value) => value,
          Err(e) => return json_response(StatusCode::BAD_REQUEST, &serde_json::json!({ "kind": "invalid", "message": format!("invalid action input: {e}") })),
        };
        let services = self.app.services.bind(opened.cell.identity(), Arc::new(opened.tokens.clone()));
        let ctx = RequestCtx { params: Default::default(), query: Default::default(), session: opened.cell.clone(), csrf: None, services };
        let mut response = match self.app.actions.dispatch(id, ctx, input).await {
          Ok(value) => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
          Err(e) => json_response(
            StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
          ),
        };
        self.set_cookie(&opened, &mut response).await;
        return response;
      }
    }

    let raw_query = req.uri().query().unwrap_or("");
    let target = match req.uri().path_and_query() {
      Some(pq) => pq.as_str().to_owned(),
      None => path.clone(),
    };

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
      let mut response = match self.call_handler(req.method().as_str(), &target, opened.cell.clone(), input).await {
        Ok(value) => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
        Err(e) => json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        ),
      };
      self.set_cookie(&opened, &mut response).await;
      return response;
    }

    let mode = if raw_query.split('&').any(|p| p == "__payload") { RenderMode::Payload } else { RenderMode::Html };
    tracing::info!(target: "fsr::host", path = %path, payload = (mode == RenderMode::Payload), "request");

    let rendered = match self.render(&target, mode, opened.cell.clone()).await {
      Ok(chunks) => Ok((StatusCode::OK, chunks)),
      Err(HostError::NotFound(path)) => match self.render_not_found(&target, mode, opened.cell.clone()).await {
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
        self.set_cookie(&opened, &mut response).await;
        response
      }
      Err(e) => text_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
  }

  async fn set_cookie(&self, opened: &snapfire_fsr_session::Opened, response: &mut Response<Body>) {
    if let Some(set_cookie) = self.sessions.persist(opened).await {
      if let Ok(value) = HeaderValue::from_str(&set_cookie) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
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

fn json_response(status: StatusCode, json: &serde_json::Value) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "application/json")
    .body(http_body_util::Full::new(Bytes::from(json.to_string())).map_err(|never| match never {}).boxed_unsync())
    .expect("a json response")
}

fn text_response(status: StatusCode, text: String) -> Response<Body> {
  Response::builder()
    .status(status)
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .body(http_body_util::Full::new(Bytes::from(text)).map_err(|never| match never {}).boxed_unsync())
    .expect("a text response")
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

  /// The evaluator for the document module, replacing the stock shell.
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
            if self.transport_override.is_none() {
              let transport = snapfire_fsr_service::GrpcTransport::new(&client.base_url, &imported).map_err(|e| HostError::Transport(name.clone(), e))?;
              transports.push((name.clone(), Arc::new(transport)));
            }
            service_rows.push((name.clone(), "grpc".to_owned(), client.base_url.clone()));
            continue;
          }
          let text = std::fs::read_to_string(&path).map_err(|e| HostError::Io(path.clone(), e))?;
          let imported = snapfire_fsr_service::import(&text, name)
            .map_err(|error| HostError::Import { document, error })?;
          contract.types.extend(imported.contract.types.clone());
          contract.services.extend(imported.contract.services.clone());
          let mut transport = HttpTransport::new(&client.base_url);
          for (path, route) in &imported.routes {
            transport = transport.route(path.clone(), route.clone());
          }
          transports.push((name.clone(), Arc::new(transport)));
          service_rows.push((name.clone(), "http".to_owned(), client.base_url.clone()));
        }
        let mut builder = Services::builder()
          .contract(contract)
          .intercept(Arc::new(TraceInterceptor::new()))
          .intercept(Arc::new(IdentityInterceptor::new()));
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
    let app = app
      .services(services)
      .evaluator(move |m: &ModuleId| m.path == shell_path, shell)
      .build()?;

    let ttl = config.session_ttl()?;
    let store: Arc<dyn SessionStore> = match self.store.take() {
      Some(store) => store,
      None => match config.session.store.as_str() {
        "memory" => Arc::new(MemorySessionStore::new(config.session.capacity, ttl)),
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

    let mut statics = Vec::new();
    let mut static_rows = Vec::new();
    for root in &config.statics {
      let dir = config.resolve(&root.dir);
      static_rows.push((root.route.clone(), dir.clone()));
      statics.push((root.route.trim_end_matches('/').to_owned(), ServeDir::new(dir)));
    }

    let report = HostReport {
      app: app.report.clone(),
      services: service_rows,
      statics: static_rows,
      config: config.sources.clone(),
      inferred: config.inferred.clone(),
    };
    Ok(Host { app, sessions, head, statics, report, report_listen: config.server.listen })
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

