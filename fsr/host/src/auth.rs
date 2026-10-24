//! The `/auth/` routes: sign-in, callback and logout over the identity provider.

use super::*;

pub(crate) fn see_other(location: &str) -> Response<Body> {
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
pub(crate) fn same_origin_path(candidate: &str) -> Option<String> {
  let own = candidate.starts_with('/') && !candidate.starts_with("//") && !candidate.starts_with("/\\");
  own.then(|| candidate.to_owned())
}

/// The path and query of a `Referer`, whether it came absolute or bare.
/// The path of a `Referer` on this origin, which is where a form post lands
/// again. `None` for another origin.
pub fn referer_path(referer: &str) -> Option<String> {
  if referer.starts_with('/') {
    return same_origin_path(referer);
  }
  let rest = referer.split_once("://")?.1;
  same_origin_path(rest.find('/').map(|i| &rest[i..]).unwrap_or("/"))
}

/// What the provider's callback receives: the query on a GET, a form or a
/// JSON object on a POST.
pub(crate) fn callback_params(req: &Request<Bytes>, query: &Params) -> Result<ValueMap, String> {
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

impl Host {
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
      csrf: TokenCell::default(),
      fresh: true,
      stale: false,
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

  /// The framework-owned identity routes, plus the seeding a GET of the
  /// login page does so a typed URL can still post to the callback. `None`
  /// when `path` is none of them and the request goes on to the middleware.
  /// `/auth/login` begins a sign-in, `/auth/enrol` (a POST carrying the CSRF
  /// token) begins adding a method to the identity signed in and
  /// `/auth/callback` takes each answer. A request asking for JSON gets each
  /// step as JSON; any other is redirected. Logout answers without
  /// persisting: the record is gone and the cookie expires in the same response.
  pub(crate) async fn auth_route(
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
    let method = query.get("method").cloned();
    let json = header("accept").is_some_and(|accept| accept.contains("application/json"));
    match (req.method(), path) {
      (&Method::GET, "/auth/login") => {
        let return_to = asked.or(referer).unwrap_or_else(|| "/".to_owned());
        let step = mounted.auth.begin(opened, Start::sign_in(return_to).method(method)).await;
        let pending = mounted.auth.pending_return_to(opened);
        Some(self.answer_step(mounted, opened, step, json, pending).await)
      }
      (&Method::POST, "/auth/enrol") => {
        let token = form_field(req.body(), "_csrf").or_else(|| header("x-sf-csrf")).unwrap_or_default();
        if !self.sessions.verify_csrf(opened, &token) {
          return Some(text_response(StatusCode::FORBIDDEN, "csrf verification failed".to_owned()));
        }
        let Some(identity) = opened.cell.identity() else {
          return Some(text_response(StatusCode::UNAUTHORIZED, "enrolling needs a signed-in session".to_owned()));
        };
        let return_to = asked.or(referer).unwrap_or_else(|| "/".to_owned());
        let step = mounted.auth.begin(opened, Start::enrol(return_to, identity).method(method)).await;
        let pending = mounted.auth.pending_return_to(opened);
        Some(self.answer_step(mounted, opened, step, json, pending).await)
      }
      (&Method::GET | &Method::POST, "/auth/callback") => {
        let params = match callback_params(req, &query) {
          Ok(params) => params,
          Err(message) => return Some(text_response(StatusCode::BAD_REQUEST, message)),
        };
        let before = mounted.auth.pending_return_to(opened);
        let step = mounted.auth.advance(opened, params).await;
        let pending = mounted.auth.pending_return_to(opened).or(before);
        Some(self.answer_step(mounted, opened, step, json, pending).await)
      }
      (&Method::POST, "/auth/logout") => {
        let token = form_field(req.body(), "_csrf")
          .or_else(|| header("x-sf-csrf"))
          .unwrap_or_default();
        if !self.sessions.verify_csrf(opened, &token) {
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
        for cookie in [expire, self.sessions.state_cookie()] {
          if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
          }
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

  /// The response to one step, with the session's cookie. A plain request is redirected: to the
  /// identity provider, to the login page for a step the page presents, to the destination when
  /// the flow completes and back to the login page with `error=denied` on a refusal. A JSON request
  /// gets `{ next: "redirect", to }`, `{ next: "present", kind, data }`, `{ next: "complete", to }`
  /// or `{ error, message }` with the status. Signing in gives the session a new id.
  async fn answer_step(&self, mounted: &Mounted, opened: &Opened, step: Result<Step, AuthError>, json: bool, pending: Option<String>) -> Response<Body> {
    let login_page = |return_to: Option<String>, error: Option<&str>| {
      let back: String = return_to.map(|p| form_urlencoded::byte_serialize(p.as_bytes()).collect()).unwrap_or_default();
      match error {
        Some(error) => format!("{}?error={error}&return_to={back}", mounted.login_path),
        None => format!("{}?return_to={back}", mounted.login_path),
      }
    };
    let mut renewed = None;
    let mut response = match step {
      Ok(Step::Redirect(to)) if json => json_response(StatusCode::OK, &serde_json::json!({ "next": "redirect", "to": to })),
      Ok(Step::Redirect(to)) => see_other(&to),
      Ok(Step::Present { kind, data }) if json => {
        json_response(StatusCode::OK, &serde_json::json!({ "next": "present", "kind": kind, "data": snapfire_fsr_payload::value_to_json(&Value::Map(data)) }))
      }
      Ok(Step::Present { .. }) => see_other(&login_page(pending, None)),
      Ok(Step::Complete { to, signed_in }) => {
        let settled = if signed_in {
          match self.sessions.renew(opened).await {
            Ok(fresh) => {
              self.sessions.rotate_csrf(&fresh);
              renewed = Some(fresh);
              true
            }
            Err(_) => {
              mounted.auth.logout(opened);
              false
            }
          }
        } else {
          true
        };
        match (settled, json) {
          (false, _) => text_response(StatusCode::SERVICE_UNAVAILABLE, "the session store did not answer; sign in again".to_owned()),
          (true, true) => json_response(StatusCode::OK, &serde_json::json!({ "next": "complete", "to": to })),
          (true, false) => see_other(&to),
        }
      }
      Err(error) if json => {
        let kind = if matches!(error, AuthError::Denied(_)) { "denied" } else { "invalid" };
        let status = StatusCode::from_u16(error.http_status()).unwrap_or(StatusCode::BAD_REQUEST);
        json_response(status, &serde_json::json!({ "error": kind, "message": error.to_string() }))
      }
      Err(AuthError::Denied(_)) => see_other(&login_page(pending, Some("denied"))),
      Err(e) => text_response(StatusCode::BAD_REQUEST, e.to_string()),
    };
    self.set_cookie(renewed.as_ref().unwrap_or(opened), &mut response).await;
    response
  }

}
