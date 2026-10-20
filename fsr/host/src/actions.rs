//! Actions, route handlers and template islands: the middleware preflight, the call paths a request and a Rust caller share and the form and multipart bodies they read.

use super::*;

/// What the middleware decided for a request. `headers` join the response
/// whatever the decision.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Preflight {
  pub action: PreflightAction,
  pub headers: Vec<(String, String)>,
}

impl Preflight {
  pub fn new(action: PreflightAction, headers: Vec<(String, String)>) -> Self {
    Self { action, headers }
  }
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
    // A number written in a body lowers to a float, so `status: 301` arrives as
    // `F64(301.0)` rather than `Int(301)`.
    let status = match map.get("status") {
      None | Some(Value::Null) => None,
      Some(Value::Int(n)) => {
        Some(u16::try_from(*n).map_err(|_| format!("middleware `status` {n} is not an HTTP status"))?)
      }
      Some(Value::UInt(n)) => {
        Some(u16::try_from(*n).map_err(|_| format!("middleware `status` {n} is not an HTTP status"))?)
      }
      Some(Value::F64(n)) if n.fract() == 0.0 && *n >= 0.0 && *n <= f64::from(u16::MAX) => Some(*n as u16),
      Some(Value::F32(n)) if n.fract() == 0.0 && *n >= 0.0 && *n <= f32::from(u16::MAX) => Some(*n as u16),
      Some(Value::F64(n)) => return Err(format!("middleware `status` {n} is not an HTTP status")),
      Some(Value::F32(n)) => return Err(format!("middleware `status` {n} is not an HTTP status")),
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

pub(crate) fn kind_of(value: &Value) -> &'static str {
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

/// What one step of an island in server mode produced: the state after the
/// handler, the island's markup rendered from it with handler markers, and
/// the actions the handler called, in order, for the caller to dispatch.
#[non_exhaustive]
pub struct IslandStep {
  pub state: ValueMap,
  /// The store keys the handler wrote, with their values.
  pub store: ValueMap,
  pub html: String,
  pub acts: Vec<(String, Value)>,
}

impl IslandStep {
  /// The JSON a step answers with: `{ state, store, html, revalidate }`,
  /// `store` the keys the handler wrote for the browser to set and
  /// `revalidate` true when an action ran, so the browser refreshes the
  /// page's data.
  pub fn json(&self) -> serde_json::Value {
    serde_json::json!({ "state": snapfire_fsr_payload::value_to_json(&Value::Map(self.state.clone())), "store": snapfire_fsr_payload::value_to_json(&Value::Map(self.store.clone())), "html": self.html, "revalidate": !self.acts.is_empty() })
  }
}

/// What a step posted, before either path has judged it: the props the island
/// was mounted with, the state it holds, the handler that fired and the event
/// the browser saw. A lowered component wants an index and a map; a template
/// island wants a name and whatever its handlers agreed on, so the judging is
/// each path's own.
#[non_exhaustive]
pub struct IslandBody {
  pub props: ValueMap,
  pub state: Value,
  pub handler: Value,
  pub event: Value,
}

/// Reads a step's body. `locale` joins the props the way it does for a page,
/// so a template or a component renders in the document's language.
pub fn island_body(body: &[u8], locale: &str) -> Result<IslandBody, (StatusCode, serde_json::Value)> {
  let input = match serde_json::from_slice::<serde_json::Value>(body)
    .map_err(|e| e.to_string())
    .and_then(|json| snapfire_fsr_payload::json_to_value(&json).map_err(|e| e.to_string()))
  {
    Ok(Value::Map(map)) => map,
    Ok(_) => {
      return Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "an island step is an object" }),
      ));
    }
    Err(e) => {
      return Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": format!("invalid island step: {e}") }),
      ));
    }
  };
  let mut props = match input.get("props") {
    Some(Value::Map(map)) => map.clone(),
    None | Some(Value::Null) => ValueMap::default(),
    Some(_) => {
      return Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "props must be an object" }),
      ));
    }
  };
  if !locale.is_empty() {
    props.entry("locale".to_owned()).or_insert_with(|| Value::str(locale.to_owned()));
  }
  Ok(IslandBody {
    props,
    state: input.get("state").cloned().unwrap_or(Value::Null),
    handler: input.get("handler").cloned().unwrap_or(Value::Null),
    event: input.get("event").cloned().unwrap_or(Value::Null),
  })
}

/// One round trip of an island in server mode: the body is `{ props, state,
/// handler, event }`; `handler` is the index of the handler that fired or
/// null to render as is. Answers the state after the handler, the island's
/// markup rendered from it and the actions the handler called, which the
/// caller dispatches before answering; a failure is the status and JSON body
/// to answer with.
pub fn island_step(
  lowered: Option<&snapfire_fsr_ir::IrEvaluator>,
  module: &str,
  body: &[u8],
  locale: &str,
) -> Result<IslandStep, (StatusCode, serde_json::Value)> {
  let Some(evaluator) = lowered else {
    return Err((
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": "no lowered component" }),
    ));
  };
  let components = evaluator.components();
  let Some(component) = components.get(module).cloned() else {
    return Err((
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": format!("`{module}` is not a lowered component") }),
    ));
  };
  let posted = island_body(body, locale)?;
  let state = match posted.state {
    Value::Map(map) => map,
    Value::Null => ValueMap::default(),
    _ => {
      return Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "state must be an object" }),
      ));
    }
  };
  if let Some(unknown) = state.keys().find(|k| !k.contains('/') && !component.state.contains(k)) {
    return Err((
      StatusCode::BAD_REQUEST,
      serde_json::json!({ "kind": "invalid", "message": format!("`{unknown}` is not state of `{module}`") }),
    ));
  }
  let handler = match &posted.handler {
    Value::Null => None,
    Value::Int(i) if *i >= 0 => Some(HandlerRef::own(*i as usize)),
    Value::F64(f) if *f >= 0.0 && f.fract() == 0.0 => Some(HandlerRef::own(*f as usize)),
    Value::Str(token) if HandlerRef::parse(token.as_str()).is_some() => HandlerRef::parse(token.as_str()),
    _ => {
      return Err((
        StatusCode::BAD_REQUEST,
        serde_json::json!({ "kind": "invalid", "message": "a lowered component's handler is an index or `<path>/<index>` for a component inside it" }),
      ));
    }
  };
  if let Some(own) = handler.as_ref().filter(|h| h.path.is_empty() && h.index >= component.handlers.len()) {
    return Err((
      StatusCode::NOT_FOUND,
      serde_json::json!({ "kind": "not_found", "message": format!("`{module}` has no handler {}", own.index) }),
    ));
  }
  match evaluator
    .interpreter()
    .island_step(module, &component, &posted.props, &state, handler, &posted.event, &components)
  {
    Ok(stepped) => {
      let nodes = Node::Seq(snapfire_fsr_ir::rendered_nodes(&stepped.rendered));
      if let Some(slot) = slot_in(&nodes) {
        let message = format!("`{module}` renders the slot `{slot}`, which a step has no child to fill");
        return Err((StatusCode::INTERNAL_SERVER_ERROR, serde_json::json!({ "kind": "internal", "message": message })));
      }
      let html = snapfire_fsr_payload::html_serialize(&nodes);
      Ok(IslandStep { state: stepped.state, store: stepped.store, html, acts: stepped.acts })
    }
    Err(fail) => Err((
      StatusCode::from_u16(fail.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
      serde_json::json!({ "kind": fail.kind.as_str(), "message": fail.message }),
    )),
  }
}

/// The first slot anywhere under `node`. The serializer writes nothing for
/// one, which in a step's answer would take the slot's content out of the
/// document.
pub(crate) fn slot_in(node: &Node) -> Option<&str> {
  match node {
    Node::Slot(name) => Some(&name.0),
    Node::Seq(items) | Node::Placed { nodes: items, .. } => items.iter().find_map(slot_in),
    Node::Client { children, ssr, .. } => ssr.as_deref().and_then(slot_in).or_else(|| children.iter().find_map(slot_in)),
    Node::Pending { fallback, .. } => slot_in(fallback),
    Node::Text(_) | Node::Raw(_) => None,
  }
}

/// Whether a form post asked for the action's value rather than its page back.
/// A browser navigating a form takes the redirect; a caller that named JSON in
/// `Accept`, which `upload` in the client does, is answered the value.
pub(crate) fn wants_json(headers: &http::HeaderMap) -> bool {
  headers
    .get(header::ACCEPT)
    .and_then(|v| v.to_str().ok())
    .is_some_and(|accept| accept.split(',').any(|part| part.trim().starts_with("application/json")))
}

/// The boundary a `multipart/form-data` content type names, or `None` when the
/// header is some other type.
pub(crate) fn multipart_boundary(headers: &http::HeaderMap) -> Option<String> {
  let raw = headers.get(header::CONTENT_TYPE)?.to_str().ok()?;
  multer::parse_boundary(raw).ok()
}

/// What one refused part says, so the caller answers the same way for a part
/// too large as for one whose type the deployment does not accept.
pub(crate) struct PartRefused {
  pub(crate) field: String,
  pub(crate) why: String,
}

/// Reads a buffered `multipart/form-data` body into the fields an action or a
/// handler receives: a part with no filename is text, one with a filename is an
/// `Upload`. The whole request is already in memory, bounded by
/// `server.max_body`, so this is bounded by it too; `max_upload` caps one part
/// below that.
pub(crate) async fn multipart_params(body: &[u8], boundary: &str, max_upload: usize) -> Result<Result<ValueMap, PartRefused>, String> {
  let owned = bytes::Bytes::copy_from_slice(body);
  let stream = futures_util::stream::once(async move { Ok::<_, std::io::Error>(owned) });
  let mut parts = multer::Multipart::new(stream, boundary.to_owned());
  let mut fields = ValueMap::default();

  while let Some(part) = parts.next_field().await.map_err(|e| e.to_string())? {
    let Some(name) = part.name().map(str::to_owned) else { continue };
    let filename = part.file_name().map(str::to_owned);
    let content_type = part.content_type().map(|m| m.to_string());
    let bytes = part.bytes().await.map_err(|e| e.to_string())?;

    match filename {
      None => {
        let text = String::from_utf8_lossy(&bytes).into_owned();
        fields.insert(name, Value::str(text));
      }
      Some(filename) => {
        if max_upload > 0 && bytes.len() > max_upload {
          return Ok(Err(PartRefused {
            field: name,
            why: format!("{} bytes, over the host's server.max_upload of {max_upload}", bytes.len()),
          }));
        }
        let mut upload = ValueMap::default();
        upload.insert("filename".to_owned(), Value::str(filename));
        upload.insert("content_type".to_owned(), Value::str(content_type.unwrap_or_else(|| "application/octet-stream".to_owned())));
        upload.insert("size".to_owned(), Value::Int(bytes.len() as i128));
        upload.insert("bytes".to_owned(), Value::Bytes(bytes.to_vec()));
        fields.insert(name, Value::Map(upload));
      }
    }
  }
  Ok(Ok(fields))
}

pub(crate) fn form_params(body: &[u8]) -> ValueMap {
  form_urlencoded::parse(body)
    .map(|(k, v)| (k.into_owned(), Value::str(v.into_owned())))
    .collect()
}

pub(crate) fn form_field(body: &[u8], name: &str) -> Option<String> {
  match form_params(body).get(name) {
    Some(Value::Str(value)) => Some(value.to_string()),
    _ => None,
  }
}

impl Host {
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

  pub(crate) async fn preflight_in(
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
        Value::Bool(payload_of(raw_query)),
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
        false,
      )
      .await
  }

  /// `text` says the body was a form, whose values are all strings, so the
  /// input is read against the handler's declared type before it is checked.
  pub(crate) async fn call_handler_in(
    &self,
    t: &Tables,
    method: &str,
    path: &str,
    raw_query: &str,
    incoming: Incoming,
    locale: &Locale,
    mut input: Value,
    text: bool,
  ) -> Result<Value, ActionError> {
    let Some(found) = t.app.handlers.match_request(method, path) else {
      return Err(ActionError::new(
        snapfire_fsr_runtime::FailureKind::NotFound,
        format!("no handler for {} {path}", method.to_ascii_uppercase()),
      ));
    };
    if text {
      t.app.conform_text_input(&found.id, &mut input);
    }
    let ctx = self.ctx(t, incoming, found.params, parse_query(raw_query), path, locale.clone());
    t.app.handlers.dispatch(&found.id, ctx, input).await
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

  /// The answer to a step that failed. An internal failure is logged with
  /// the module and its message, which outside development the answer leaves
  /// out. Any other kind describes the request and keeps its message.
  pub(crate) fn step_failed(&self, module: &str, status: StatusCode, json: &serde_json::Value) -> Response<Body> {
    if json["kind"] != "internal" {
      return json_response(status, json);
    }
    let message = json["message"].as_str().unwrap_or_default();
    tracing::error!(target: "fsr::island", module, error = message, "the island step failed");
    if self.changed.is_some() {
      return json_response(status, json);
    }
    json_response(status, &serde_json::json!({ "kind": "internal", "message": "the island step failed" }))
  }

  /// One round trip of an island a template renders. The handler the markup
  /// named answers with the state to render from next, then the module is
  /// rendered again through its own evaluator with that state beside the
  /// props. A step whose handler wrote the session asks the page to
  /// revalidate, since what the rest of it renders from has moved.
  pub(crate) async fn template_island(
    &self,
    t: &Tables,
    module: &str,
    body: &[u8],
    incoming: Incoming,
    visit: &Resolution,
  ) -> Response<Body> {
    let posted = match island_body(body, &visit.locale.tag) {
      Ok(posted) => posted,
      Err((status, json)) => return json_response(status, &json),
    };
    let name = match &posted.handler {
      Value::Null => None,
      Value::Str(name) => Some(name.to_string()),
      _ => {
        return json_response(
          StatusCode::BAD_REQUEST,
          &serde_json::json!({ "kind": "invalid", "message": "a template island's handler is a name" }),
        );
      }
    };
    let session = incoming.session.clone();
    let was_dirty = session.is_dirty();
    let mut state = posted.state.clone();
    if let Some(name) = name {
      if !t.app.islands.names(module).iter().any(|held| *held == name) {
        return json_response(
          StatusCode::NOT_FOUND,
          &serde_json::json!({ "kind": "not_found", "message": format!("`{module}` has no handler `{name}`") }),
        );
      }
      let ctx = self.ctx(t, incoming, Params::new(), Params::new(), &visit.path, visit.locale.clone());
      let event = IslandEvent::new(Value::Map(posted.props.clone()), state.clone(), posted.event.clone());
      match t.app.islands.dispatch(module, &name, ctx, event).await {
        Ok(next) => state = next,
        Err(e) => {
          let status = StatusCode::from_u16(e.kind.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
          return self.step_failed(module, status, &serde_json::json!({ "kind": e.kind.as_str(), "message": e.message }));
        }
      }
    }
    let id: ModuleId = match module.parse() {
      Ok(id) => id,
      Err(e) => {
        return json_response(
          StatusCode::BAD_REQUEST,
          &serde_json::json!({ "kind": "invalid", "message": format!("island module id: {e}") }),
        );
      }
    };
    let data = snapfire_fsr_runtime::island_data(&posted.props, &state);
    let mut chunks = t.app.runtime.evaluators.select(&id).evaluate(&id, &data);
    let mut nodes = Vec::new();
    while let Some(chunk) = chunks.next().await {
      match chunk {
        Ok(Chunk::Node(node)) => nodes.push(node),
        Ok(Chunk::Slot(slot)) => {
          let message = format!("`{module}` emitted the slot `{}`, which an island has no child to fill", slot.0);
          return self.step_failed(module, StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({ "kind": "internal", "message": message }));
        }
        Err(e) => {
          return self.step_failed(module, StatusCode::INTERNAL_SERVER_ERROR, &serde_json::json!({ "kind": "internal", "message": e.to_string() }));
        }
      }
    }
    let html = snapfire_fsr_payload::html_serialize(&Node::Seq(nodes));
    let revalidate = !was_dirty && session.is_dirty();
    json_response(
      StatusCode::OK,
      &serde_json::json!({ "state": snapfire_fsr_payload::value_to_json(&state), "html": html, "revalidate": revalidate }),
    )
  }

  pub(crate) async fn dispatch_action(
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
}
