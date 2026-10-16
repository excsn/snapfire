//! Development reloads, `/_sf/live` events, topics and the socket upgrade.

use super::*;

/// The publish half and the receiver every `/_sf/live` stream clones. A
/// cloned receiver has a mailbox of its own and starts empty, so a stream
/// opened now never sees what was published before it.
pub(crate) struct Topics {
  pub(crate) tx: fibre::spmc::topic::AsyncTopicSender<String, ()>,
  pub(crate) rx: fibre::spmc::topic::AsyncTopicReceiver<String, ()>,
}

impl Topics {
  pub(crate) fn new(mailbox: usize) -> Self {
    let (tx, rx) = fibre::spmc::topic::channel_async(mailbox);
    Self { tx, rx }
  }
}

/// The same shape for the one signal the dev loop sends, with what the last
/// build left: the bundle id, held until the next build lands, and why the
/// last build was refused, when it was.
pub(crate) struct Reload {
  pub(crate) tx: fibre::spmc::topic::AsyncTopicSender<(), ()>,
  pub(crate) rx: fibre::spmc::topic::AsyncTopicReceiver<(), ()>,
  pub(crate) state: Arc<parking_lot::RwLock<DevState>>,
}

#[derive(Default)]
pub(crate) struct DevState {
  pub(crate) bundle: Option<String>,
  pub(crate) failure: Option<String>,
}

impl Reload {
  pub(crate) fn new(mailbox: usize) -> Self {
    let (tx, rx) = fibre::spmc::topic::channel_async(mailbox);
    Self { tx, rx, state: Arc::default() }
  }
}

/// The bundle id the last build left, hashed the first time anything asks
/// after it landed.
pub(crate) fn held_bundle(state: &parking_lot::RwLock<DevState>, facts: &Path) -> String {
  if let Some(id) = state.read().bundle.clone() {
    return id;
  }
  let id = bundle_id(facts);
  state.write().bundle = Some(id.clone());
  id
}

/// One `/__fsr/events` message: the bundle id and, while the last build is refused, why.
pub(crate) fn dev_event(state: &parking_lot::RwLock<DevState>, facts: &Path) -> String {
  let bundle = held_bundle(state, facts);
  let mut event = serde_json::json!({ "bundle": bundle });
  if let Some(failure) = state.read().failure.clone() {
    event["error"] = serde_json::Value::String(failure);
  }
  format!("data: {event}\n\n")
}

/// What tells one bundle from the next: a hash over the content of every
/// output the build facts list, source maps and stylesheets aside, so a
/// rebundle that wrote the same modules keeps its id and an edited module
/// changes it. `-` when
/// there is no bundle.
pub(crate) fn bundle_id(facts: &Path) -> String {
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
    if output.ends_with(".map") || output.ends_with(".snapfire-build.json") || output.ends_with(".css") {
      continue;
    }
    output.hash(&mut hasher);
    std::fs::read(dir.join(output)).unwrap_or_default().hash(&mut hasher);
  }
  format!("{:016x}", hasher.finish())
}

impl Host {
  /// The whole edge for one request: static roots, the action route, then a
  /// page in either mode, with the session opened from the cookie and
  /// persisted into the response.
  /// Tells every open development document that something changed, so each
  /// refreshes its route in place. Nothing happens when `dev` is off.
  pub fn changed(&self) {
    if let Some(reload) = &self.changed {
      *reload.state.write() = DevState::default();
      let _ = reload.tx.send((), ());
    }
  }

  /// Tells every open development document that the last build was refused
  /// and why, which it shows over the page until a build lands. Nothing
  /// happens when `dev` is off.
  pub fn failed(&self, message: impl Into<String>) {
    if let Some(reload) = &self.changed {
      reload.state.write().failure = Some(message.into());
      let _ = reload.tx.send((), ());
    }
  }

  /// Every open socket, by topic: what a backend pushes into a wave nobody
  /// typed into and what a presence count is read from.
  #[cfg(feature = "ws")]
  pub fn sockets(&self) -> Arc<socket::Sockets> {
    self.sockets.clone()
  }

  /// `GET /_sf/socket?topic=x` upgraded: the topic rule decides whether this
  /// visitor may open it at all, the handshake is answered inline and the
  /// connection is served once hyper hands the upgraded stream over.
  #[cfg(feature = "ws")]
  pub(crate) fn upgrade(&self, mut req: Request<Bytes>, opened: &snapfire_fsr_session::Opened) -> Response<Body> {
    let Some(handler) = self.socket_handler.clone() else {
      return text_response(StatusCode::NOT_FOUND, "no socket handler is registered".to_owned());
    };
    if !upgrade_from_this_site(req.headers(), self.origin.as_deref()) {
      return text_response(StatusCode::FORBIDDEN, "a socket opens from this site's own pages".to_owned());
    }
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
  /// Nothing is sent to a stream that did not ask for the topic and a
  /// publish with no listeners costs a send into an empty channel.
  pub fn publish(&self, topic: impl Into<String>) {
    let _ = self.topics.tx.send(topic.into(), ());
  }

  /// The stream behind `GET /_sf/live?topics=a,b`: one `data: {"topic":"a"}`
  /// per publish of a topic in the list, until the client goes away. A
  /// comment frame opens it so the browser sees the connection established
  /// before anything is published.
  pub(crate) fn live_events(&self, topics: Vec<String>) -> Response<Body> {
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

  /// A server-sent event stream: one event on open and one per `changed` or
  /// `failed` call, each `data: {"bundle":"<id>"}` with the bundle id the
  /// last build left, plus `"error"` while that build is refused, until the
  /// client goes away.
  pub(crate) fn events(&self, t: &Tables) -> Response<Body> {
    let (Some(reload), Some(facts)) = (&self.changed, t.dev_bundle.clone()) else {
      return text_response(StatusCode::NOT_FOUND, "dev is off".to_owned());
    };
    let rx = reload.rx.clone();
    rx.subscribe(());
    let state = reload.state.clone();
    let event = move || Ok::<_, std::io::Error>(http_body::Frame::data(Bytes::from(dev_event(&state, &facts))));
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

  /// The `x-sf-request` token for a trace, when its request's path is one a
  /// client may read back: `<id>.<hmac>`, since ids count up from 1 and a bare
  /// one would let any client read other visitors' requests.
  pub(crate) fn trace_token(&self, t: &Tables, trace: &trace::Trace) -> Option<String> {
    let path = trace.root()?.fields.get("path")?.to_string();
    let exposed = self.changed.is_some() || t.trace_expose.iter().any(|prefix| under_prefix(&path, prefix));
    if !exposed {
      return None;
    }
    let signature = self.trace_ring.sign(trace_token_input(trace.id).as_bytes());
    (!signature.is_empty()).then(|| format!("{}.{signature}", trace.id))
  }

  /// The finished trace a token names, while the collector's ring still holds it.
  pub(crate) fn trace_by_token(&self, token: &str) -> Option<trace::Trace> {
    let (id, signature) = token.split_once('.')?;
    let id: u64 = id.parse().ok()?;
    self.trace_ring.verify(trace_token_input(id).as_bytes(), signature)?;
    let traces = self.traces.as_ref()?;
    traces.recent(traces.len()).into_iter().find(|trace| trace.id == id)
  }
}

/// Whether a request that acts on the visitor's session came from this site's own pages. A browser sends `Origin` on a WebSocket handshake and on any cross-origin POST and carries the visitor's cookie with it, so another origin, a sibling subdomain included, is refused. A client that sends no `Origin` is not a browser carrying someone else's cookie.
pub(crate) fn upgrade_from_this_site(headers: &hyper::HeaderMap, configured: Option<&str>) -> bool {
  let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) else {
    return true;
  };
  let authority = |url: &str| url.split_once("://").map(|(_, rest)| rest.split('/').next().unwrap_or(rest).to_ascii_lowercase());
  let Some(theirs) = authority(origin) else {
    return false;
  };
  let host = headers.get("host").and_then(|v| v.to_str().ok()).map(str::to_ascii_lowercase);
  host.as_deref() == Some(theirs.as_str()) || configured.and_then(authority).as_deref() == Some(theirs.as_str())
}

#[cfg(test)]
mod upgrade_origin_tests {
  use super::upgrade_from_this_site;

  fn headers(pairs: &[(&'static str, &'static str)]) -> hyper::HeaderMap {
    let mut map = hyper::HeaderMap::new();
    for (k, v) in pairs {
      map.insert(*k, v.parse().unwrap());
    }
    map
  }

  #[test]
  fn only_this_sites_pages_open_a_socket() {
    assert!(upgrade_from_this_site(&headers(&[("host", "app.example"), ("origin", "https://app.example")]), None));
    assert!(upgrade_from_this_site(&headers(&[("host", "127.0.0.1:3000"), ("origin", "http://127.0.0.1:3000")]), None));
    assert!(upgrade_from_this_site(&headers(&[("host", "internal:8080"), ("origin", "https://app.example")]), Some("https://app.example")), "behind a proxy the configured origin is this site");
    assert!(!upgrade_from_this_site(&headers(&[("host", "app.example"), ("origin", "https://evil.example")]), None));
    assert!(!upgrade_from_this_site(&headers(&[("host", "app.example"), ("origin", "https://sub.app.example")]), Some("https://app.example")), "a sibling subdomain is another origin");
    assert!(!upgrade_from_this_site(&headers(&[("host", "app.example"), ("origin", "null")]), None), "an opaque origin is refused");
    assert!(upgrade_from_this_site(&headers(&[("host", "app.example")]), None), "a non-browser client sends no Origin");
  }
}
