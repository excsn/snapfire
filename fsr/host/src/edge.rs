//! The request edge: `handle` through `respond`, the session cookie and the responses every route shares.

use super::*;

/// What a request carries into a body beyond its session: the CSRF token
/// minted for it and the token custody the service layer reads. Bodies see
/// the token as the `csrf_token` prop and never see the custody.
#[derive(Clone)]
pub(crate) struct Incoming {
  pub(crate) session: SessionCell,
  pub(crate) csrf: CsrfHandle,
  pub(crate) credentials: Arc<dyn Credentials>,
  /// The locale whose catalog the navigator already holds, `x-sf-catalog`,
  /// so a payload for that locale carries no `D` row.
  pub(crate) held_catalog: Option<String>,
  /// The request's `Host`, already matched against `server.hosts`.
  pub(crate) host: Option<String>,
  /// Set on an intercepted render.
  pub(crate) intercept: Option<Intercept>,
  /// What the request decided for the document's theme, when the application has themes.
  pub(crate) theme: Option<snapfire_fsr_runtime::DocumentTheme>,
}

impl Incoming {
  pub(crate) fn anonymous(session: SessionCell) -> Self {
    Self {
      session,
      csrf: CsrfHandle::default(),
      credentials: Arc::new(NoCredentials),
      held_catalog: None,
      host: None,
      intercept: None,
      theme: None,
    }
  }
}

pub(crate) fn with_headers(response: &mut Response<Body>, headers: &[(String, String)]) {
  for (name, value) in headers {
    if let (Ok(name), Ok(value)) = (
      header::HeaderName::from_bytes(name.as_bytes()),
      HeaderValue::from_str(value),
    ) {
      response.headers_mut().append(name, value);
    }
  }
}

pub(crate) fn json_response(status: StatusCode, json: &serde_json::Value) -> Response<Body> {
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

/// One module of the embedded client. `no_cache` for a development host, whose
/// client changes when the binary it is built beside does; `cache` otherwise,
/// the same lifetime a `[[static]]` root answers with.
pub(crate) fn js_response(body: &'static str, no_cache: bool, cache: Option<&HeaderValue>) -> Response<Body> {
  let mut response = Response::builder()
    .status(StatusCode::OK)
    .header(header::CONTENT_TYPE, client::MEDIA_TYPE);
  if no_cache {
    response = response.header(header::CACHE_CONTROL, "no-cache");
  } else if let Some(cache) = cache {
    response = response.header(header::CACHE_CONTROL, cache.clone());
  }
  response
    .body(
      http_body_util::Full::new(Bytes::from_static(body.as_bytes()))
        .map_err(|never| match never {})
        .boxed_unsync(),
    )
    .expect("a script response")
}

/// A response a route handler answered, as it gave it: a header that is not a valid HTTP header is left out.
pub(crate) fn http_response(answered: snapfire_fsr_runtime::HttpResponse) -> Response<Body> {
  let mut builder = Response::builder().status(StatusCode::from_u16(answered.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR));
  for (name, value) in &answered.headers {
    if let (Ok(name), Ok(value)) = (header::HeaderName::from_bytes(name.as_bytes()), header::HeaderValue::from_str(value)) {
      builder = builder.header(name, value);
    }
  }
  builder
    .body(
      http_body_util::Full::new(Bytes::from(answered.body))
        .map_err(|never| match never {})
        .boxed_unsync(),
    )
    .expect("a handler's response")
}

/// Where a request stands in the middleware chain.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stage {
  App,
  Site,
  Route,
}

/// The request line a middleware reads as `request`.
pub fn middleware_request(method: &str, path: &str, query: &str, site: Option<&str>) -> Value {
  let mut request = snapfire_fsr_core::ValueMap::default();
  request.insert("method".to_owned(), Value::str(method.to_ascii_uppercase()));
  request.insert("path".to_owned(), Value::str(path.to_owned()));
  request.insert("payload".to_owned(), Value::Bool(payload_of(query)));
  request.insert("site".to_owned(), site.map(|s| Value::str(s.to_owned())).unwrap_or(Value::Null));
  Value::Map(request)
}

/// A request the route can take again: `next` may run it more than once.
fn copy_request(req: &Request<Bytes>) -> Request<Bytes> {
  let mut copy = Request::builder().method(req.method().clone()).uri(req.uri().clone()).version(req.version());
  for (name, value) in req.headers() {
    copy = copy.header(name, value);
  }
  copy.body(req.body().clone()).expect("a copy of a request that parsed")
}

/// A route's response as the head a middleware reads and the body the host keeps.
fn split_response(response: Response<Body>) -> (snapfire_fsr_runtime::HttpResponse, Body) {
  let (parts, body) = response.into_parts();
  let mut head = snapfire_fsr_runtime::HttpResponse::new(parts.status.as_u16());
  for (name, value) in &parts.headers {
    head.headers.push((name.as_str().to_owned(), String::from_utf8_lossy(value.as_bytes()).into_owned()));
  }
  (head, body)
}

/// A route's body under the head a middleware answered with.
fn joined_response(head: snapfire_fsr_runtime::HttpResponse, body: Body) -> Response<Body> {
  let mut response = Response::new(body);
  *response.status_mut() = StatusCode::from_u16(head.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
  with_headers(&mut response, &head.headers);
  response
}

pub(crate) fn text_response(status: StatusCode, text: String) -> Response<Body> {
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

/// A path segment with its `%XX` escapes decoded, since the client encodes
/// an action id and a site's carries a colon.
pub fn percent_decoded(segment: &str) -> String {
  percent_encoding::percent_decode_str(segment)
    .decode_utf8()
    .map(|decoded| decoded.into_owned())
    .unwrap_or_else(|_| segment.to_owned())
}

impl Host {
  /// The context a body runs in: the services bound to the session's identity
  /// and the request's custody, the token as `csrf`.
  pub(crate) fn ctx(
    &self,
    t: &Tables,
    incoming: Incoming,
    params: Params,
    query: Params,
    path: &str,
    locale: Locale,
  ) -> RequestCtx {
    let services = t.app.services.bind(incoming.session.identity(), incoming.credentials);
    let (document, address) = match incoming.intercept {
      Some(intercept) => (Some(intercept.document), Some(Address::default().path(path.to_owned()).params(params.clone()).query(query.clone()))),
      None => (None, None),
    };
    let mut ctx = RequestCtx::anonymous(params);
    ctx.query = query;
    ctx.path = path.to_owned();
    ctx.document = document;
    ctx.address = address;
    // Taken rather than read, so the next render does not show it again.
    ctx.failure = incoming.session.remove(snapfire_fsr_runtime::FAILURE_KEY);
    ctx.session = incoming.session;
    ctx.locale = locale;
    ctx.host = incoming.host;
    ctx.origin = t.origin.clone();
    ctx.config = t.public.clone();
    ctx.csrf = incoming.csrf;
    ctx.services = services;
    ctx.natives = snapfire_fsr_runtime::NativeHandle::new(t.app.natives.clone());
    if let Ok(after) = crate::after::AFTER.try_with(Clone::clone) {
      ctx.after = after;
    }
    ctx
  }

  /// What a request at the edge carries: the session, its custody and, once
  /// the session is identified, a CSRF token. An anonymous request carries no
  /// token so its renders share the memo; the token joins the memo key.
  pub(crate) fn incoming_themed(&self, opened: &Opened, held_catalog: Option<String>, host: Option<String>, theme: Option<snapfire_fsr_runtime::DocumentTheme>) -> Incoming {
    let mut incoming = self.incoming_holding(opened, held_catalog, host);
    incoming.theme = theme;
    incoming
  }

  pub(crate) fn incoming_holding(&self, opened: &Opened, held_catalog: Option<String>, host: Option<String>) -> Incoming {
    let mut incoming = self.incoming(opened, host);
    incoming.held_catalog = held_catalog;
    incoming
  }

  /// The request's `Host` when `server.hosts` lists it. Absent that key nothing
  /// is read. A header naming an unlisted host answers `None`, so a client
  /// cannot choose what a body sees.
  pub(crate) fn matched_host(&self, headers: &header::HeaderMap) -> Option<String> {
    if self.hosts.is_empty() {
      return None;
    }
    let asked = headers.get(header::HOST)?.to_str().ok()?.to_lowercase();
    self.hosts.iter().find(|h| **h == asked).cloned()
  }

  pub(crate) fn incoming(&self, opened: &Opened, host: Option<String>) -> Incoming {
    let csrf = match self.csrf_always || opened.cell.identity().is_some() {
      true => {
        let scheme = self.sessions.csrf_scheme();
        let held = opened.clone();
        CsrfHandle::new(scheme.memo(&held), move || {
          held.csrf.mark_issued();
          scheme.issue(&held)
        })
      }
      false => CsrfHandle::default(),
    };
    Incoming {
      session: opened.cell.clone(),
      csrf,
      credentials: Arc::new(opened.tokens.clone()),
      held_catalog: None,
      host,
      intercept: None,
      theme: None,
    }
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
    let token = root
      .id()
      .and_then(|id| self.traces.as_ref()?.of_span(id.into_u64()))
      .and_then(|trace| self.trace_token(&self.tables(), &trace));
    let after = snapfire_fsr_runtime::AfterHandle::default();
    let mut answered = crate::after::AFTER.scope(after.clone(), tracing::Instrument::instrument(self.handle_in(req), root.clone())).await;
    if let Some(value) = token.and_then(|token| HeaderValue::from_str(&token).ok()) {
      answered.headers_mut().insert("x-sf-request", value);
    }
    root.record("status", answered.status().as_u16());
    root.record("fibre.outcome", if answered.status().is_success() { "ok" } else { "error" });
    let (parts, body) = answered.into_parts();
    Response::from_parts(parts, Body::new(crate::after::Finishing::new(body, after, self.afters.clone())))
  }

  /// Runs `call` with a queue of its own for `after` and starts what it queued once it returns: a call from outside `handle`, which has no response body to wait on.
  pub(crate) async fn answering<T>(&self, call: impl std::future::Future<Output = T>) -> T {
    let after = snapfire_fsr_runtime::AfterHandle::default();
    let answered = crate::after::AFTER.scope(after.clone(), call).await;
    self.afters.start(after);
    answered
  }

  /// Resolves once the work every answered request left with `after` has finished.
  pub async fn after_settled(&self) {
    self.afters.settled().await
  }

  pub(crate) async fn handle_in(&self, req: Request<Bytes>) -> Response<Body> {
    if req.body().len() > self.max_body {
      return self.too_large();
    }
    let t = self.tables();
    let path = req.uri().path().to_owned();

    if matches!(*req.method(), Method::GET | Method::HEAD) {
      let ready = t.ready.as_deref() == Some(path.as_str());
      if ready || t.health.as_deref() == Some(path.as_str()) {
        let (status, text) = match ready && self.draining() {
          true => (StatusCode::SERVICE_UNAVAILABLE, "shutting down"),
          false => (StatusCode::OK, "ok"),
        };
        let mut response = text_response(status, text.to_owned());
        response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        return response;
      }
    }
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
      return self.sites_reload_response().await;
    }
    if let Some(token) = path.strip_prefix("/__fsr/trace/")
      && req.method() == Method::GET
      && self.traces.is_some()
      && (self.changed.is_some() || !t.trace_expose.is_empty())
    {
      return match self.trace_by_token(token) {
        Some(trace) => {
          let mut response = json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&trace::one_value(&trace)));
          response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
          response
        }
        None => text_response(StatusCode::NOT_FOUND, "no such trace".to_owned()),
      };
    }
    if self.changed.is_some() {
      // `fsr dev` calls the control routes with no `Origin`; a request
      // carrying one came from a page.
      let control = ["/__fsr/changed", "/__fsr/failed", "/__fsr/reload"].contains(&path.as_str()) && req.method() == Method::POST;
      if control && req.headers().contains_key(header::ORIGIN) {
        return text_response(StatusCode::FORBIDDEN, "a development control route is called by fsr dev, not a page".to_owned());
      }
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
      if path == "/__fsr/failed" && req.method() == Method::POST {
        self.failed(String::from_utf8_lossy(req.body()).into_owned());
        return Response::builder()
          .status(StatusCode::NO_CONTENT)
          .body(Body::default())
          .expect("an empty response");
      }
      if path == "/__fsr/reload" && req.method() == Method::POST {
        return match self.reload() {
          Ok(report) => text_response(StatusCode::OK, report.to_string()),
          Err(e) => text_response(StatusCode::CONFLICT, e.to_string()),
        };
      }
    }

    if t.client {
      if let Some(name) = path.strip_prefix(client::ROUTE).and_then(|rest| rest.strip_prefix('/')) {
        if let Some(body) = client::get(name, t.client_minified) {
          return js_response(body, self.changed.is_some(), t.static_cache.as_ref());
        }
        if let Some(body) = client::map(name) {
          let mut response = js_response(body, self.changed.is_some(), t.static_cache.as_ref());
          response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
          return response;
        }
      }
    }

    if let Some(pwa) = &t.pwa {
      if req.method() == Method::GET || req.method() == Method::HEAD {
        let answer = match path.as_str() {
          crate::pwa::MANIFEST_ROUTE => Some(("application/manifest+json", pwa.manifest.clone())),
          crate::pwa::WORKER_ROUTE => pwa.worker.clone().map(|worker| (client::MEDIA_TYPE, worker)),
          _ => None,
        };
        if let Some((kind, body)) = answer {
          return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, kind)
            .header(header::CACHE_CONTROL, "no-cache")
            .body(http_body_util::Full::new(body).map_err(|never| match never {}).boxed_unsync())
            .expect("a pwa response");
        }
      }
    }

    // A browser asks for the root icon on its own when it shows anything but
    // a document; with none to serve, it is told so without a logged 404.
    if path == "/favicon.ico" && t.favicon.is_none() && (req.method() == Method::GET || req.method() == Method::HEAD) && !t.statics.iter().any(|(route, _, root)| route.is_empty() && root.join("favicon.ico").is_file()) {
      let mut response = Response::builder().status(StatusCode::NO_CONTENT).body(Body::default()).expect("an empty response");
      let cache = if self.changed.is_some() { Some(HeaderValue::from_static("no-cache")) } else { t.static_cache.clone() };
      if let Some(value) = cache {
        response.headers_mut().insert(header::CACHE_CONTROL, value);
      }
      return response;
    }
    let lookup = match &t.favicon {
      Some(at) if path == "/favicon.ico" => at.as_str(),
      _ => path.as_str(),
    };
    for (route, dir, root) in &t.statics {
      if let Some(rest) = lookup.strip_prefix(route.as_str()) {
        if route.is_empty() && !names_file(root, rest) {
          continue;
        }
        if rest.is_empty() || rest.starts_with('/') {
          let names = t.hashed_names.iter().find(|(at, _)| at == route).map(|(_, names)| names);
          let output = names.and_then(|names| names.get(&rest[1.min(rest.len())..])).map(|output| format!("/{output}"));
          let mut inner = Request::builder()
            .method(req.method().clone())
            .uri(match &output {
              Some(output) => output.as_str(),
              None if rest.is_empty() => "/",
              None => rest,
            });
          for (name, value) in req.headers() {
            inner = inner.header(name, value);
          }
          let inner = inner.body(Bytes::new()).expect("a request rebuilt from a request");
          return match dir.clone().oneshot(inner).await {
            Ok(response) => {
              let mut response = response.map(|b| b.map_err(std::io::Error::other).boxed_unsync());
              let served = output.as_deref().unwrap_or(rest);
              if response.status().is_success() && served.rsplit('/').next().and_then(|name| name.rsplit_once('.')).is_some_and(|(_, ext)| ext != "js" && crate::config::is_module_extension(ext)) {
                response.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static(crate::client::MEDIA_TYPE));
              }
              // A hashed file is public and immutable; a font is fetched in CORS mode
              // whatever the page says, so when `[fonts] base` or `[images] base` puts the
              // file under another origin the browser needs this to use it.
              if is_hashed_name(rest) {
                response
                  .headers_mut()
                  .entry(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                  .or_insert(HeaderValue::from_static("*"));
              }
              if self.changed.is_some() {
                response
                  .headers_mut()
                  .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
              } else if t.static_cache.is_some() && is_hashed_name(rest) {
                response
                  .headers_mut()
                  .entry(header::CACHE_CONTROL)
                  .or_insert(HeaderValue::from_static("public, max-age=31536000, immutable"));
              } else if t.static_cache.is_some() && names.is_some() {
                response
                  .headers_mut()
                  .entry(header::CACHE_CONTROL)
                  .or_insert(HeaderValue::from_static("no-cache"));
              } else if let Some(cache) = &t.static_cache {
                response.headers_mut().entry(header::CACHE_CONTROL).or_insert(cache.clone());
              }
              response
            }
            Err(never) => match never {},
          };
        }
      }
    }

    if t.crawl.is_some() {
      if let Some(response) = self.crawl_response(&t, &req, &path).await {
        return response;
      }
    }
    let header = |name: &str| req.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_owned);
    let cookie = header("cookie");
    let accept_language = header("accept-language");
    let opened = tracing::Instrument::instrument(self.sessions.open(cookie.as_deref()), tracing::info_span!(target: "fsr::trace", "session", op = "open")).await;

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
  pub(crate) async fn handle_resolved(
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
    let incoming = self.incoming(opened, self.matched_host(req.headers()));
    self.through(t, &req, opened, &visit, incoming, Stage::App, path, raw_query).await
  }

  /// The request at `stage` of the chain: the application's middleware, then the middleware of the site the path belongs to, then the route. A middleware's `next` runs the stages after its own for the request it names; what it answers is the response.
  #[allow(clippy::too_many_arguments)]
  fn through<'a>(
    &'a self,
    t: &'a Tables,
    req: &'a Request<Bytes>,
    opened: &'a snapfire_fsr_session::Opened,
    visit: &'a Resolution,
    incoming: Incoming,
    stage: Stage,
    path: String,
    query: String,
  ) -> futures_util::future::BoxFuture<'a, Response<Body>> {
    Box::pin(async move {
      let site = t.site_for(&path);
      let (middleware, inner) = match stage {
        Stage::App => (t.app.middleware.clone(), Stage::Site),
        Stage::Site => (site.and_then(|s| s.middleware.clone()), Stage::Route),
        Stage::Route => {
          let target = if query.is_empty() { path.clone() } else { format!("{path}?{query}") };
          return self.respond(t, copy_request(req), opened, path, target, &query, visit).await;
        }
      };
      let Some(middleware) = middleware else {
        return self.through(t, req, opened, visit, incoming, inner, path, query).await;
      };
      let ctx = self.ctx(t, incoming.clone(), Params::new(), parse_query(&query), &path, visit.locale.clone());
      let input = middleware_request(req.method().as_str(), &path, &query, site.map(|s| s.name.as_str()));
      let bound = (stage == Stage::Site).then(|| site.map(|s| (s.name.clone(), s.at.clone()))).flatten();
      let passed = tracing::Instrument::instrument(
        snapfire_fsr_runtime::drive_middleware(
          ctx,
          &query,
          move |ctx| middleware.call(ctx, input),
          |target| {
            let incoming = incoming.clone();
            let bound = bound.clone();
            async move {
              if let Some((name, at)) = &bound {
                if !t.site_for(&target.path).is_some_and(|s| &s.name == name) {
                  return Err(snapfire_fsr_runtime::ActionError::new(snapfire_fsr_runtime::FailureKind::Internal, format!("site `{name}` handed `next` `{}`, outside {at}", target.path)));
                }
              }
              let response = self.through(t, req, opened, visit, incoming, inner, target.path, target.query).await;
              Ok(split_response(response))
            }
          },
        ),
        tracing::info_span!(target: "fsr::trace", "middleware"),
      )
      .await;
      match passed {
        Ok(snapfire_fsr_runtime::Passed::Continue) => self.through(t, req, opened, visit, incoming, inner, path, query).await,
        Ok(snapfire_fsr_runtime::Passed::Route(head, body)) => joined_response(head, body),
        Ok(snapfire_fsr_runtime::Passed::Answer(answered)) => {
          let mut response = http_response(answered);
          self.set_cookie(opened, &mut response).await;
          response
        }
        Err(e) => json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        ),
      }
    })
  }

  pub(crate) async fn respond(
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
        if !super::live::upgrade_from_this_site(req.headers(), self.origin.as_deref()) {
          return text_response(StatusCode::FORBIDDEN, "an island steps from this site's own pages".to_owned());
        }
        if !is_json(req.headers()) {
          return json_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &serde_json::json!({ "kind": "invalid", "message": "an island step takes application/json" }),
          );
        }
        let lowered = t.app.lowered.as_deref().is_some_and(|e| e.components().contains_key(&module));
        if !lowered && t.app.islands.holds(&module) {
          let incoming = self.incoming(opened, self.matched_host(req.headers()));
          let mut response = self.template_island(t, &module, req.body(), incoming, &visit).await;
          self.set_cookie(opened, &mut response).await;
          return response;
        }
        let mut response = match island_step(t.app.lowered.as_deref(), &module, req.body(), &visit.locale.tag) {
          Ok(step) => {
            let mut failed = None;
            for (id, input) in &step.acts {
              let incoming = self.incoming(opened, self.matched_host(req.headers()));
              if let Err(e) = self.dispatch_action(t, id, incoming, &visit.path, visit.locale.clone(), input.clone()).await {
                failed = Some(e);
                break;
              }
            }
            match failed {
              None => json_response(StatusCode::OK, &step.json()),
              Some(e) => json_response(
                StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
              ),
            }
          }
          Err((status, json)) => self.step_failed(&module, status, &json),
        };
        self.set_cookie(opened, &mut response).await;
        return response;
      }
      if let Some(id) = path.strip_prefix("/_sf/action/").map(percent_decoded) {
        let id = id.as_str();
        let multipart = multipart_boundary(req.headers());
        let is_form = multipart.is_some()
          || req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));
        if !super::live::upgrade_from_this_site(req.headers(), self.origin.as_deref()) {
          return text_response(StatusCode::FORBIDDEN, "an action is called from this site's own pages".to_owned());
        }
        if !is_form && !is_json(req.headers()) {
          return json_response(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            &serde_json::json!({ "kind": "invalid", "message": "an action takes application/json or a form with its _csrf token" }),
          );
        }
        let input = if is_form {
          let mut fields = match &multipart {
            Some(boundary) => match multipart_params(req.body(), boundary, self.max_upload).await {
              Ok(Ok(fields)) => fields,
              Ok(Err(refused)) => {
                return json_response(
                  StatusCode::PAYLOAD_TOO_LARGE,
                  &serde_json::json!({ "kind": "invalid", "message": format!("`{}` is {}", refused.field, refused.why) }),
                );
              }
              Err(e) => {
                return json_response(
                  StatusCode::BAD_REQUEST,
                  &serde_json::json!({ "kind": "invalid", "message": format!("invalid multipart body: {e}") }),
                );
              }
            },
            None => form_params(req.body()),
          };
          let token = match fields.shift_remove("_csrf") {
            Some(Value::Str(token)) => token.to_string(),
            _ => String::new(),
          };
          if !self.sessions.verify_csrf(opened, &token) {
            return text_response(StatusCode::FORBIDDEN, "csrf verification failed".to_owned());
          }
          let mut input = Value::Map(fields);
          t.app.conform_text_input(id, &mut input);
          input
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
          .dispatch_action_to(t, id, self.incoming(opened, self.matched_host(req.headers())), &visit.path, visit.locale.clone(), input)
          .await
        {
          Ok((_, Some(to))) if is_form && !wants_json(req.headers()) => match fragment_of(raw_query) {
            Some(slot) => see_other(&with_fragment(&to, slot.as_deref())),
            None => see_other(&to),
          },
          Ok((value, to)) if is_form && wants_json(req.headers()) => redirecting(json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)), to),
          Ok(_) if is_form => {
            let back = req
              .headers()
              .get(header::REFERER)
              .and_then(|v| v.to_str().ok())
              .and_then(referer_path)
              .unwrap_or_else(|| "/".to_owned());
            match fragment_of(raw_query) {
              Some(slot) => see_other(&with_fragment(&back, slot.as_deref())),
              None => see_other(&back),
            }
          }
          Ok((value, to)) => redirecting(json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)), to),
          // A browser that posted a form is answered the page it posted from,
          // failure or not: a JSON body is not something a visitor can read.
          // The failure waits on the session for that render.
          Err(e) if is_form && !wants_json(req.headers()) => {
            let mut failure = snapfire_fsr_core::Fields::default();
            failure.insert("kind".to_owned(), Value::str(e.kind.as_str()));
            failure.insert("message".to_owned(), Value::str(e.message.clone()));
            opened.cell.insert(snapfire_fsr_runtime::FAILURE_KEY, Value::Map(failure.into()));
            let back = req
              .headers()
              .get(header::REFERER)
              .and_then(|v| v.to_str().ok())
              .and_then(referer_path)
              .unwrap_or_else(|| "/".to_owned());
            match fragment_of(raw_query) {
              Some(slot) => see_other(&with_fragment(&back, slot.as_deref())),
              None => see_other(&back),
            }
          }
          Err(e) => json_response(
            StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
          ),
        };
        if let Some(size) = self.sessions.too_large(opened) {
          tracing::warn!(target: "fsr::session", action = id, size, "an action's session write is over session.max_bytes; nothing was saved");
          return json_response(StatusCode::PAYLOAD_TOO_LARGE, &serde_json::json!({ "kind": "invalid", "message": "the session cannot hold that much" }));
        }
        self.set_cookie(opened, &mut response).await;
        return response;
      }
    }

    if matches!(*req.method(), http::Method::GET | http::Method::HEAD) {
      if let Some((entry, params)) = t.app.images.match_path(&path) {
        return self.card_image(t, entry, params, &path, self.incoming(opened, self.matched_host(req.headers())), &visit.locale).await;
      }
    }

    if t.app.handlers.match_request(req.method().as_str(), &path).is_some() {
      let writes = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
      if writes && !super::live::upgrade_from_this_site(req.headers(), self.origin.as_deref()) {
        return text_response(StatusCode::FORBIDDEN, "a route handler takes a write from this site's own pages".to_owned());
      }
      let multipart = multipart_boundary(req.headers());
      let form = multipart.is_some()
        || req
          .headers()
          .get(header::CONTENT_TYPE)
          .and_then(|v| v.to_str().ok())
          .is_some_and(|ct| ct.starts_with("application/x-www-form-urlencoded"));
      let input = if req.body().is_empty() {
        Value::Null
      } else if form {
        let mut fields = match &multipart {
          Some(boundary) => match multipart_params(req.body(), boundary, self.max_upload).await {
            Ok(Ok(fields)) => fields,
            Ok(Err(refused)) => {
              return json_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                &serde_json::json!({ "kind": "invalid", "message": format!("`{}` is {}", refused.field, refused.why) }),
              );
            }
            Err(e) => {
              return json_response(
                StatusCode::BAD_REQUEST,
                &serde_json::json!({ "kind": "invalid", "message": format!("invalid multipart body: {e}") }),
              );
            }
          },
          None => form_params(req.body()),
        };
        let token = match fields.shift_remove("_csrf") {
          Some(Value::Str(token)) => token.to_string(),
          _ => String::new(),
        };
        if !self.sessions.verify_csrf(opened, &token) {
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
          form,
        )
        .await
      {
        Ok(value) => match snapfire_fsr_runtime::HttpResponse::from_value(&value) {
          None => json_response(StatusCode::OK, &snapfire_fsr_payload::value_to_json(&value)),
          Some(Ok(answered)) => http_response(answered),
          Some(Err(message)) => json_response(StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({ "kind": "internal", "message": message })),
        },
        Err(e) => json_response(
          StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
          &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }),
        ),
      };
      self.set_cookie(opened, &mut response).await;
      return response;
    }

    let mode = if payload_of(raw_query) {
      RenderMode::Payload
    } else if let Some(slot) = fragment_of(raw_query) {
      RenderMode::Fragment(slot)
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
      RenderMode::Html | RenderMode::Fragment(_) => (None, None),
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

    let theme = match mode {
      RenderMode::Html | RenderMode::Payload if t.themed() => {
        let (theme_path, theme_query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        Some(self.document_theme(t, theme_path, theme_query, &self.incoming(opened, self.matched_host(req.headers())), &visit.locale).await)
      }
      _ => None,
    };
    let themed = |text: String| match (&theme, &mode) {
      (Some(theme), RenderMode::Html) => crate::theme::html_with_theme(&text, theme),
      (Some(theme), RenderMode::Payload) => crate::theme::payload_with_theme(&text, theme),
      _ => text,
    };

    // Development renders live and previews what a prerender would write when the request asks.
    if self.changed.is_some() && preview_of(&raw_query) && req.method() == Method::GET && !intercepted {
      return match self.preview(t, &visit.path, &path, mode.clone(), &visit.locale).await {
        Ok(text) => {
          let text = themed(text);
          let content_type = if mode == RenderMode::Payload { "application/x-sf-payload+json; charset=utf-8" } else { "text/html; charset=utf-8" };
          let mut response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type)
            .header("x-sf-prerendered", "preview")
            .body(http_body_util::Full::new(Bytes::from(text)).map_err(|never: std::convert::Infallible| match never {}).boxed_unsync())
            .expect("a response with a valid header");
          set_csp(t, &mode, &mut response);
          response
        }
        Err(why) => text_response(StatusCode::NOT_FOUND, why),
      };
    }
    if req.method() == Method::GET && !intercepted && self.changed.is_none() {
      let found = self.prerendered_in(t, &path, &mode, &visit.locale, opened.cell.identity().is_none());
      if let Some(text) = found {
        let text = themed(text);
        let content_type = match mode {
          RenderMode::Html | RenderMode::Fragment(_) => "text/html; charset=utf-8",
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
        set_csp(t, &mode, &mut response);
        self.set_cookie(opened, &mut response).await;
        set_document_cache(opened, &mut response);
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
          self.incoming_themed(opened, held_catalog.clone(), self.matched_host(req.headers()), theme.clone()),
        )
        .await
    } else {
      self
        .render_in(
          t,
          &target_visit,
          target_query,
          mode.clone(),
          self.incoming_themed(opened, held_catalog.clone(), self.matched_host(req.headers()), theme.clone()),
        )
        .await
    };
    let rendered = match rendered {
      Ok(rendered) => Ok((status_of(rendered.failed), rendered.chunks)),
      Err(HostError::NotFound(path)) => match self
        .render_not_found_in(
          t,
          &target_visit,
          target_query,
          mode.clone(),
          self.incoming_themed(opened, held_catalog.clone(), self.matched_host(req.headers()), theme.clone()),
        )
        .await
      {
        Ok(Some(rendered)) => Ok((StatusCode::NOT_FOUND, rendered.chunks)),
        Ok(None) => return text_response(StatusCode::NOT_FOUND, format!("no route: {path}")),
        Err(e) => Err(e),
      },
      Err(e) => Err(e),
    };
    match rendered {
      Ok((status, chunks)) => {
        let content_type = match mode {
          RenderMode::Html | RenderMode::Fragment(_) => "text/html; charset=utf-8",
          RenderMode::Payload => "application/x-sf-payload+json; charset=utf-8",
        };
        let chunks = match (&theme, &mode) {
          (Some(theme), RenderMode::Html) => {
            let theme = theme.clone();
            let mut pending = true;
            chunks
              .map(move |chunk| match pending && chunk.contains("<html") {
                true => {
                  pending = false;
                  crate::theme::html_with_theme(&chunk, &theme)
                }
                false => chunk,
              })
              .boxed()
          }
          _ => chunks.boxed(),
        };
        let body = StreamBody::new(chunks.map(|c| Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(c)))));
        let mut response = Response::builder()
          .status(status)
          .header(header::CONTENT_TYPE, content_type)
          .body(body.boxed_unsync())
          .expect("a response with a valid header");
        set_csp(t, &mode, &mut response);
        self.set_cookie(opened, &mut response).await;
        set_document_cache(opened, &mut response);
        response
      }
      Err(HostError::NoSlot(name)) => text_response(StatusCode::NOT_FOUND, format!("no slot named `{name}` on this route")),
      Err(e) => {
        tracing::error!(target: "fsr::render", path = %path, error = %e, "the page did not render");
        match self.changed.is_some() {
          true => text_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
          false => text_response(StatusCode::INTERNAL_SERVER_ERROR, "the page could not be rendered".to_owned()),
        }
      }
    }
  }

  pub(crate) async fn set_cookie(&self, opened: &Opened, response: &mut Response<Body>) {
    tracing::Instrument::instrument(self.save_session(opened, response), tracing::info_span!(target: "fsr::trace", "session", op = "save")).await
  }

  pub(crate) async fn save_session(&self, opened: &Opened, response: &mut Response<Body>) {
    let changed = opened.cell.is_dirty() || opened.tokens.is_dirty();
    let written = if self.csrf_always && opened.csrf.is_issued() {
      self.sessions.establish(opened).await
    } else {
      self.sessions.persist(opened).await
    };
    let set_cookie = match written {
      Ok(set_cookie) => set_cookie,
      Err(error) => {
        // The response still goes out: the reader is served and the cookie
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
    if changed {
      if let Ok(value) = HeaderValue::from_str(&self.sessions.state_cookie()) {
        response.headers_mut().append(header::SET_COOKIE, value);
      }
    }
  }
}

/// Whether the URL path `rest` names a file under `root`, read as `ServeDir`
/// reads it: percent-decoded, with no component that leaves the directory.
fn names_file(root: &Path, rest: &str) -> bool {
  let Ok(decoded) = percent_encoding::percent_decode_str(rest).decode_utf8() else { return false };
  let mut path = root.to_path_buf();
  for part in decoded.split('/').filter(|p| !p.is_empty()) {
    if part == "." || part == ".." || part.contains('\\') {
      return false;
    }
    path.push(part);
  }
  path != root && path.is_file()
}

/// A document differs by cookie. One for a visitor holding a session or one that starts a session is theirs alone.
fn set_document_cache(opened: &Opened, response: &mut Response<Body>) {
  let headers = response.headers_mut();
  headers.append(header::VARY, HeaderValue::from_static("Cookie"));
  if !opened.fresh || headers.contains_key(header::SET_COOKIE) {
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
  }
}

/// Whether a request's body is declared `application/json`, which a page on
/// another origin cannot send without a preflight.
fn is_json(headers: &http::HeaderMap) -> bool {
  headers
    .get(header::CONTENT_TYPE)
    .and_then(|v| v.to_str().ok())
    .is_some_and(|ct| ct.split(';').next().is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json")))
}
