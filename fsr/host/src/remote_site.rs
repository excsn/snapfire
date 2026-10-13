//! A site in its own process. The shell renders the site's segment from the
//! payload rows one of its hosts answers and forwards its statics, actions
//! and handlers as bytes.

use std::sync::Arc;

use bytes::Bytes;
use futures_util::future::BoxFuture;
use futures_util::stream::{self, StreamExt};
use http::{header, HeaderMap, Method, Request, Response, StatusCode};
use http_body_util::{BodyExt, StreamBody};
use parking_lot::RwLock;
use snapfire_fsr_core::ext::FailureKind;
use snapfire_fsr_payload::DecodeError;
use snapfire_fsr_runtime::wire::{Row, RowReader};
use snapfire_fsr_runtime::{EntryId, LoadError, Matcher, MatchitMatcher, RemoteSegment, RemoteWave, RequestCtx};

use crate::config::MountConfig;
use crate::upstreams::Upstreams;
use crate::{fragment_of, payload_of, text_response, Body, HostError};

/// The headers a forwarded request carries over and a forwarded answer brings
/// back; everything else stays on its side of the seam.
const FORWARDED: &[header::HeaderName] = &[
  header::ACCEPT,
  header::ACCEPT_LANGUAGE,
  header::CONTENT_TYPE,
  header::IF_NONE_MATCH,
  header::IF_MODIFIED_SINCE,
  header::RANGE,
];
const RETURNED: &[header::HeaderName] = &[
  header::CONTENT_TYPE,
  header::CONTENT_LENGTH,
  header::CACHE_CONTROL,
  header::ETAG,
  header::LAST_MODIFIED,
  header::LOCATION,
  header::ACCESS_CONTROL_ALLOW_ORIGIN,
  header::CONTENT_RANGE,
  header::ACCEPT_RANGES,
  header::VARY,
];

/// The payload content type a site host answers `__payload` with.
pub const PAYLOAD_TYPE: &str = "application/x-sf-payload+json";

/// How long the list of paths a site answers itself is trusted before a
/// miss fetches it again, so a release that adds a handler is seen.
const ANSWERS_FRESH: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Default)]
struct Learned {
  entry: Option<String>,
  styles: Vec<String>,
  answers: Option<(MatchitMatcher, std::time::Instant)>,
}

struct Inner {
  name: String,
  at: String,
  secret: Option<String>,
  upstreams: Upstreams,
  client: reqwest::Client,
  learned: RwLock<Learned>,
}

/// `x-sf-mount`: the site's name; with a secret, a dot and the HMAC-SHA256
/// over the request's path, session id and subject, hex, so the site knows
/// the identity headers came from its shell.
fn mount_header(name: &str, secret: Option<&str>, path: &str, session: Option<&str>, subject: Option<&str>) -> String {
  match secret {
    Some(secret) => format!("{name}.{}", mount_signature(secret, path, session.unwrap_or(""), subject.unwrap_or(""))),
    None => name.to_owned(),
  }
}

fn mount_signature(secret: &str, path: &str, session: &str, subject: &str) -> String {
  use hmac::Mac;
  let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).expect("any key length");
  mac.update(path.as_bytes());
  mac.update(b"\n");
  mac.update(session.as_bytes());
  mac.update(b"\n");
  mac.update(subject.as_bytes());
  mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn mount_signature_verifies(secret: &str, path: &str, session: &str, subject: &str, signature: &str) -> bool {
  snapfire_fsr_session::constant_time_eq(&mount_signature(secret, path, session, subject), signature)
}

#[derive(Clone)]
pub struct RemoteSite(Arc<Inner>);

impl RemoteSite {
  pub fn new(name: &str, mount: &MountConfig) -> Result<Self, HostError> {
    let at = mount.at.clone().ok_or_else(|| HostError::Mount(name.to_owned(), "a site over addresses needs at".to_owned()))?;
    let timeout = mount.timeout();
    let client = reqwest::Client::builder()
      .connect_timeout(timeout)
      .read_timeout(timeout)
      .redirect(reqwest::redirect::Policy::none())
      .build()
      .map_err(|e| HostError::Mount(name.to_owned(), format!("no http client: {e}")))?;
    Ok(Self(Arc::new(Inner {
      name: name.to_owned(),
      at,
      secret: mount.secret.clone(),
      upstreams: Upstreams::new(mount.over.clone(), mount.retry()),
      client,
      learned: RwLock::new(Learned::default()),
    })))
  }

  pub fn name(&self) -> &str {
    &self.0.name
  }

  pub fn at(&self) -> &str {
    &self.0.at
  }

  pub fn addresses(&self) -> &[String] {
    self.0.upstreams.addresses()
  }

  /// The addresses down now, by index into [`RemoteSite::addresses`].
  pub fn down(&self) -> Vec<usize> {
    self.0.upstreams.down()
  }

  /// The module its islands need, once a wave has named it.
  pub fn entry(&self) -> Option<String> {
    self.0.learned.read().entry.clone()
  }

  pub fn styles(&self) -> Vec<String> {
    self.0.learned.read().styles.clone()
  }

  /// Whether `path` is one of the site's static files.
  pub fn covers_static(&self, path: &str) -> bool {
    path.strip_prefix(&self.0.at).is_some_and(|rest| rest.starts_with("/static/"))
  }

  /// Whether a request under the site's prefix is one the site answers
  /// itself rather than a page the shell renders, judged from the request
  /// alone: anything but a `GET` or `HEAD` and a `GET` that asks for neither
  /// a document nor a payload. A `GET` for a path the site lists as its own
  /// is [`RemoteSite::answers_itself`].
  pub fn forwards(&self, method: &Method, headers: &HeaderMap, raw_query: &str) -> bool {
    if method != Method::GET && method != Method::HEAD {
      return true;
    }
    if payload_of(raw_query) || fragment_of(raw_query).is_some() {
      return false;
    }
    let accept = headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("*/*");
    !accept.contains("text/html") && !accept.contains("*/*")
  }

  /// Whether `/_sf/action/<id>` names one of the site's actions.
  pub fn owns_action(&self, id: &str) -> bool {
    id.strip_prefix(&self.0.name).is_some_and(|rest| rest.starts_with(':'))
  }

  /// The request as bytes to one of the site's hosts and its answer as
  /// bytes back, streamed. A host that does not answer rests and the next
  /// is not tried on the same request, so a client sees the failure.
  pub async fn forward(&self, req: Request<Bytes>, session: Option<&str>, subject: Option<&str>) -> Response<Body> {
    let Some(lease) = self.0.upstreams.pick() else {
      return text_response(StatusCode::BAD_GATEWAY, format!("site {}: no address", self.0.name));
    };
    let path_and_query = req.uri().path_and_query().map(|p| p.as_str().to_owned()).unwrap_or_else(|| "/".to_owned());
    let method = reqwest::Method::from_bytes(req.method().as_str().as_bytes()).expect("an http method");
    let mut outbound = self.0.client.request(method, format!("{}{path_and_query}", lease.address));
    for name in FORWARDED {
      if let Some(value) = req.headers().get(name) {
        outbound = outbound.header(name.as_str(), value.as_bytes());
      }
    }
    for (name, value) in req.headers() {
      if name.as_str().starts_with("x-sf-") {
        outbound = outbound.header(name.as_str(), value.as_bytes());
      }
    }
    if let Some(subject) = subject {
      outbound = outbound.header("x-sf-subject", subject);
    }
    if let Some(session) = session {
      outbound = outbound.header("x-sf-session", session);
    }
    let path = req.uri().path().to_owned();
    outbound = outbound.header("x-sf-mount", mount_header(&self.0.name, self.0.secret.as_deref(), &path, session, subject));
    let body = req.into_body();
    if !body.is_empty() {
      outbound = outbound.body(body);
    }
    let answer = match outbound.send().await {
      Ok(answer) => answer,
      Err(e) => {
        self.0.upstreams.failed(&lease);
        tracing::warn!(target: "fsr::remote", site = %self.0.name, address = %lease.address, error = %e, "forward failed");
        return text_response(StatusCode::BAD_GATEWAY, format!("site {}: {} did not answer", self.0.name, lease.address));
      }
    };
    self.0.upstreams.recovered(&lease);
    let status = StatusCode::from_u16(answer.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut response = Response::builder().status(status);
    for name in RETURNED {
      if let Some(value) = answer.headers().get(name.as_str()) {
        response = response.header(name, value.as_bytes());
      }
    }
    let stream = answer.bytes_stream().map(|chunk| chunk.map(http_body::Frame::data).map_err(std::io::Error::other));
    response.body(StreamBody::new(stream).boxed_unsync()).expect("a response from an answer")
  }

  /// Whether `path` is one of the GET patterns the site answers itself, its
  /// route handlers and its generated images, as its `/__fsr/sites` lists
  /// them. The list is fetched on first use and again on a miss once it is
  /// older than [`ANSWERS_FRESH`]; a site that does not answer is judged by
  /// the list held, or by none.
  pub async fn answers_itself(&self, path: &str) -> bool {
    {
      let learned = self.0.learned.read();
      if let Some((matcher, at)) = &learned.answers {
        if matcher.match_path(path).is_some() {
          return true;
        }
        if at.elapsed() < ANSWERS_FRESH {
          return false;
        }
      }
    }
    let Some(matcher) = self.fetch_answers().await else {
      return self.0.learned.read().answers.as_ref().is_some_and(|(m, _)| m.match_path(path).is_some());
    };
    let hit = matcher.match_path(path).is_some();
    self.0.learned.write().answers = Some((matcher, std::time::Instant::now()));
    hit
  }

  async fn fetch_answers(&self) -> Option<MatchitMatcher> {
    let lease = self.0.upstreams.pick()?;
    let answer = match self.0.client.get(format!("{}/__fsr/sites", lease.address)).send().await {
      Ok(answer) => answer,
      Err(e) => {
        self.0.upstreams.failed(&lease);
        tracing::warn!(target: "fsr::remote", site = %self.0.name, address = %lease.address, error = %e, "the site's status did not answer");
        return None;
      }
    };
    self.0.upstreams.recovered(&lease);
    let json: serde_json::Value = serde_json::from_slice(&answer.bytes().await.ok()?).ok()?;
    let mut matcher = MatchitMatcher::new();
    for (i, pattern) in json["site"]["answers"].as_array().into_iter().flatten().filter_map(|p| p.as_str()).enumerate() {
      if let Err(e) = matcher.insert(pattern, EntryId(i as u32)) {
        tracing::warn!(target: "fsr::remote", site = %self.0.name, %pattern, error = %e, "a path the site answers is not a pattern");
      }
    }
    Some(matcher)
  }

  fn learn(&self, wave: &RemoteWave) {
    let mut learned = self.0.learned.write();
    if wave.entry.is_some() {
      learned.entry = wave.entry.clone();
    }
    if !wave.styles.is_empty() {
      learned.styles = wave.styles.clone();
    }
  }
}

fn failure(name: &str, message: impl Into<String>, kind: FailureKind) -> LoadError {
  LoadError::new(name, message).with_kind(kind)
}

/// The rows of an answer's body as they arrive, each chunk's lines read as
/// they complete.
fn rows_of(name: String, answer: reqwest::Response) -> futures_util::stream::BoxStream<'static, Result<Row, DecodeError>> {
  struct Reading {
    body: futures_util::stream::BoxStream<'static, reqwest::Result<Bytes>>,
    reader: Option<RowReader>,
    queue: std::collections::VecDeque<Result<Row, DecodeError>>,
    name: String,
  }
  let reading = Reading { body: answer.bytes_stream().boxed(), reader: Some(RowReader::new()), queue: Default::default(), name };
  stream::unfold(reading, |mut r| async move {
    loop {
      if let Some(next) = r.queue.pop_front() {
        return Some((next, r));
      }
      let Some(reader) = r.reader.as_mut() else { return None };
      match r.body.next().await {
        Some(Ok(chunk)) => match reader.push(&chunk) {
          Ok(rows) => r.queue.extend(rows.into_iter().map(Ok)),
          Err(e) => {
            r.reader = None;
            r.queue.push_back(Err(e));
          }
        },
        Some(Err(e)) => {
          r.reader = None;
          r.queue.push_back(Err(DecodeError(format!("site {}: the answer broke off: {e}", r.name))));
        }
        None => {
          let reader = r.reader.take().expect("a reader until the body ends");
          match reader.finish() {
            Ok(Some(row)) => r.queue.push_back(Ok(row)),
            Ok(None) => {}
            Err(e) => r.queue.push_back(Err(e)),
          }
        }
      }
    }
  })
  .boxed()
}

impl RemoteSegment for RemoteSite {
  fn fetch(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<RemoteWave, LoadError>> {
    let site = self.clone();
    let path = ctx.path.clone();
    let mut query: String = form_urlencoded::Serializer::new(String::new()).extend_pairs(ctx.query.iter()).finish();
    if !query.is_empty() {
      query.push('&');
    }
    query.push_str("__payload");
    let locale = ctx.locale.tag.clone();
    let document = ctx.document.clone();
    let subject = ctx.session.identity().map(|identity| identity.subject);
    let session = ctx.session_id.clone();
    Box::pin(async move {
      let inner = &site.0;
      let Some(lease) = inner.upstreams.pick() else {
        return Err(failure(&inner.name, "no address", FailureKind::Unavailable));
      };
      let mut request = inner
        .client
        .get(format!("{}{path}?{query}", lease.address))
        .header("x-sf-catalog", locale.as_str())
        .header("x-sf-mount", mount_header(&inner.name, inner.secret.as_deref(), &path, session.as_deref(), subject.as_deref()));
      if let Some(from) = &document {
        request = request.header("x-sf-from", from.as_str());
      }
      if let Some(subject) = &subject {
        request = request.header("x-sf-subject", subject.as_str());
      }
      if let Some(session) = &session {
        request = request.header("x-sf-session", session.as_str());
      }
      let answer = match request.send().await {
        Ok(answer) => answer,
        Err(e) => {
          inner.upstreams.failed(&lease);
          let kind = if e.is_timeout() { FailureKind::Timeout } else { FailureKind::Unavailable };
          return Err(failure(&inner.name, format!("{} did not answer: {e}", lease.address), kind));
        }
      };
      let status = answer.status();
      if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        inner.upstreams.failed(&lease);
        return Err(failure(&inner.name, format!("{} answered {status}", lease.address), FailureKind::Unavailable));
      }
      inner.upstreams.recovered(&lease);
      if status == reqwest::StatusCode::NOT_FOUND {
        return Err(failure(&inner.name, format!("no route at {path}"), FailureKind::NotFound));
      }
      if !status.is_success() {
        return Err(failure(&inner.name, format!("{} answered {status}", lease.address), FailureKind::Internal));
      }
      let content_type = answer.headers().get(reqwest::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).unwrap_or("");
      if !content_type.starts_with(PAYLOAD_TYPE) {
        return Err(failure(&inner.name, format!("{} answered {content_type:?}, not a payload", lease.address), FailureKind::Internal));
      }
      let wave = RemoteWave::from_rows(&inner.name, rows_of(inner.name.clone(), answer)).await?;
      site.learn(&wave);
      Ok(wave)
    })
  }
}
