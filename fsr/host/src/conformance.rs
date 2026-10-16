//! The requests a hostile page or client sends, against every route a host
//! answers before an application's pages, each answer judged by the
//! contract of the route's class.

use std::fmt;

use bytes::Bytes;
use http::{header, Method, Request, StatusCode, Version};

use crate::Host;

/// The authority every probe names in `Host`, which makes it the origin a
/// page of this site sends.
pub const AUTHORITY: &str = "app.example";

/// What a route promises about who may call it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Class {
  /// Changes state for the visitor whose session it carries: refused from
  /// another origin and refused a body a page on another origin can send
  /// without a preflight.
  BrowserWrite,
  /// A route handler's write: refused from another origin. Its body is
  /// whatever the application's own `fetch` sends, which for a string is
  /// `text/plain`.
  HandlerWrite,
  /// Called by `fsr dev` alone, which sends no `Origin`: refused whenever a
  /// browser sent it.
  DevControl,
  /// Called from another origin by design, as an identity provider posts a
  /// callback.
  CrossOrigin,
  /// Changes nothing.
  Read,
}

/// One route as the probes call it: `body` and `content_type` are a request
/// the route accepts from this site's own pages.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Route {
  pub method: Method,
  pub path: String,
  pub class: Class,
  pub body: Bytes,
  pub content_type: Option<String>,
}

impl Route {
  pub fn read(path: impl Into<String>) -> Self {
    Self { method: Method::GET, path: path.into(), class: Class::Read, body: Bytes::new(), content_type: None }
  }

  /// A browser call that changes state, sent as JSON.
  pub fn write(path: impl Into<String>, json: &str) -> Self {
    Self { method: Method::POST, path: path.into(), class: Class::BrowserWrite, body: Bytes::from(json.to_owned()), content_type: Some("application/json".to_owned()) }
  }

  /// A browser call that changes state, sent as a form.
  pub fn form(path: impl Into<String>, fields: &str) -> Self {
    Self { content_type: Some("application/x-www-form-urlencoded".to_owned()), body: Bytes::from(fields.to_owned()), ..Self::write(path, "") }
  }

  pub fn action(id: &str, json: &str) -> Self {
    Self::write(format!("/_sf/action/{}", id.replace('#', "%23")), json)
  }

  /// A route handler answering `POST` with a JSON body.
  pub fn handler(path: impl Into<String>, json: &str) -> Self {
    Self::write(path, json).with_class(Class::HandlerWrite)
  }

  pub fn island(module: &str, json: &str) -> Self {
    Self::write(format!("/_sf/island/{}", module.replace('#', "%23")), json)
  }

  fn with_class(mut self, class: Class) -> Self {
    self.class = class;
    self
  }
}

/// An answer that broke its route's contract.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Breach {
  pub method: Method,
  pub path: String,
  pub probe: &'static str,
  pub status: u16,
  pub wanted: &'static str,
}

impl fmt::Display for Breach {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{} {} with {}: {}, wanted {}", self.method, self.path, self.probe, self.status, self.wanted)
  }
}

/// The routes the framework itself serves on this host: the sites listing,
/// the client modules and live topics always; the development
/// routes when `dev` is on; the sign-in routes when `[auth]` names a provider.
pub fn framework_routes(host: &Host) -> Vec<Route> {
  let t = host.tables();
  let mut routes = vec![Route::read("/__fsr/sites"), Route::read("/_sf/live?topics=probe"), Route::read("/__fsr/trace/none")];
  if t.client {
    routes.push(Route::read(format!("{}/index.js", crate::client::ROUTE)));
  }
  if host.changed.is_some() {
    routes.push(Route::read("/__fsr/traces"));
    routes.push(Route::read("/__fsr/events"));
    for path in ["/__fsr/changed", "/__fsr/failed", "/__fsr/reload"] {
      routes.push(Route::write(path, "").with_class(Class::DevControl));
    }
  }
  if t.auth.is_some() {
    routes.push(Route::read("/auth/login"));
    routes.push(Route::form("/auth/logout", "_csrf=probe"));
    routes.push(Route::form("/auth/callback", "code=probe&state=probe").with_class(Class::CrossOrigin));
  }
  routes
}

/// Sends every probe to every route plus a socket upgrade from another
/// origin. Returns each answer its class's contract refuses; no probe may
/// be answered with a 5xx.
pub async fn check(host: &Host, routes: &[Route]) -> Vec<Breach> {
  let mut breaches = Vec::new();
  for route in routes {
    for probe in probes(route, host.max_body) {
      let status = host.handle(probe.request).await.status();
      let wanted = if status.is_server_error() {
        Some("no 5xx")
      } else {
        match probe.want {
          Want::Answered => None,
          Want::Refused if !status.is_client_error() => Some("a 4xx refusal"),
          Want::Refused => None,
          Want::Accepted if status == StatusCode::FORBIDDEN || status == StatusCode::UNSUPPORTED_MEDIA_TYPE => Some("not refused, since this site's own page sent it"),
          Want::Accepted => None,
        }
      };
      if let Some(wanted) = wanted {
        breaches.push(Breach { method: route.method.clone(), path: route.path.clone(), probe: probe.name, status: status.as_u16(), wanted });
      }
    }
  }
  breaches.extend(socket(host).await);
  breaches
}

enum Want {
  /// Any answer short of a 5xx.
  Answered,
  Refused,
  Accepted,
}

struct Probe {
  name: &'static str,
  request: Request<Bytes>,
  want: Want,
}

fn probes(route: &Route, max_body: usize) -> Vec<Probe> {
  let build = |origin: Option<&str>, content_type: Option<&str>, body: Bytes| {
    let mut request = Request::builder().method(route.method.clone()).uri(&route.path).header(header::HOST, AUTHORITY);
    if let Some(origin) = origin {
      request = request.header(header::ORIGIN, origin);
    }
    if let Some(ct) = content_type {
      request = request.header(header::CONTENT_TYPE, ct);
    }
    request.body(body).expect("a probe request")
  };
  let own = format!("https://{AUTHORITY}");
  let sibling = format!("https://sub.{AUTHORITY}");
  let ct = route.content_type.as_deref();
  let body = route.body.clone();
  let writes = route.method != Method::GET;
  let refused_from_elsewhere = matches!(route.class, Class::BrowserWrite | Class::HandlerWrite | Class::DevControl);
  let foreign = if refused_from_elsewhere { Want::Refused } else { Want::Answered };
  let mut out = vec![
    Probe { name: "a foreign Origin", request: build(Some("https://evil.example"), ct, body.clone()), want: foreign },
    Probe { name: "a sibling subdomain's Origin", request: build(Some(&sibling), ct, body.clone()), want: if refused_from_elsewhere { Want::Refused } else { Want::Answered } },
    Probe { name: "HTTP/1.0", request: { let mut r = build(None, ct, body.clone()); *r.version_mut() = Version::HTTP_10; r }, want: Want::Answered },
    Probe { name: "no Host", request: { let mut r = build(None, ct, body.clone()); r.headers_mut().remove(header::HOST); r }, want: Want::Answered },
  ];
  match route.class {
    // A form carries a token the probe cannot forge, so only a JSON body is
    // one this site's page is known to send.
    Class::BrowserWrite if ct == Some("application/json") => out.push(Probe { name: "this site's Origin", request: build(Some(&own), ct, body.clone()), want: Want::Accepted }),
    Class::BrowserWrite => {}
    Class::HandlerWrite => out.push(Probe { name: "this site's Origin", request: build(Some(&own), Some("text/plain;charset=UTF-8"), body.clone()), want: Want::Accepted }),
    Class::DevControl => out.push(Probe { name: "this site's Origin", request: build(Some(&own), ct, body.clone()), want: Want::Refused }),
    Class::CrossOrigin | Class::Read => {}
  }
  if writes {
    let text_plain = if route.class == Class::BrowserWrite { Want::Refused } else { Want::Answered };
    out.push(Probe { name: "a text/plain body", request: build(None, Some("text/plain"), body.clone()), want: text_plain });
    out.push(Probe { name: "no content type", request: build(None, None, body.clone()), want: if route.class == Class::BrowserWrite { Want::Refused } else { Want::Answered } });
    out.push(Probe { name: "a malformed body", request: build(None, ct, Bytes::from_static(b"{\"unclosed\": [")), want: Want::Answered });
    out.push(Probe { name: "an empty body", request: build(None, ct, Bytes::new()), want: Want::Answered });
    out.push(Probe { name: "a body past max_body", request: build(None, ct, Bytes::from(vec![b'a'; max_body + 1])), want: Want::Refused });
  } else {
    out.push(Probe { name: "a POST", request: { let mut r = build(None, Some("application/json"), Bytes::from_static(b"{}")); *r.method_mut() = Method::POST; r }, want: Want::Answered });
  }
  out
}

/// A socket upgrade from another origin is refused.
async fn socket(host: &Host) -> Vec<Breach> {
  let request = Request::get("/_sf/socket")
    .header(header::HOST, AUTHORITY)
    .header(header::ORIGIN, "https://evil.example")
    .header(header::CONNECTION, "Upgrade")
    .header(header::UPGRADE, "websocket")
    .header(header::SEC_WEBSOCKET_VERSION, "13")
    .header(header::SEC_WEBSOCKET_KEY, "dGhlIHNhbXBsZSBub25jZQ==")
    .body(Bytes::new())
    .expect("an upgrade request");
  let status = host.handle(request).await.status();
  if status == StatusCode::SWITCHING_PROTOCOLS || status.is_server_error() {
    return vec![Breach { method: Method::GET, path: "/_sf/socket".to_owned(), probe: "an upgrade from a foreign Origin", status: status.as_u16(), wanted: "a 4xx refusal" }];
  }
  Vec::new()
}
