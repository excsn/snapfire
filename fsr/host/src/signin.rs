//! The `/auth/` routes over the identity provider and the providers `[auth]` names.

use super::*;
use crate::auth::*;
#[cfg(feature = "oidc")]
use base64::Engine;
#[cfg(feature = "oidc")]
use snapfire_fsr_oidc::{ClientSecret, OidcConfig, OidcProvider};
#[cfg(feature = "passkey")]
use snapfire_fsr_passkey::{FilePasskeys, PasskeyConfig, Passkeys};

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
        if !self.token_holds(opened, &token) {
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
        if !self.token_holds(opened, &token) {
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
        let mut step = ValueMap::default();
        step.insert("next".to_owned(), Value::str("present"));
        step.insert("kind".to_owned(), Value::str(kind));
        step.insert("data".to_owned(), Value::Map(data));
        plain_response(StatusCode::OK, &Value::Map(step))
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

/// The provider one configured method names and how the boot report names it.
pub(crate) fn method_provider(method: &crate::config::MethodSection, config: &Config, services: &Arc<Services>, prefix: &str) -> Result<(Arc<dyn IdentityProvider>, String), HostError> {
  match method.provider.as_str() {
    "file" => Ok((Arc::new(users_of(method, config)?), "file".to_owned())),
    "service" => {
      let client = format!("{prefix}{}", method.client.clone().unwrap_or_default());
      let declared = |name: &str| services.contract().services.get(&client).is_some_and(|service| service.methods.contains_key(name));
      let steps = (declared(crate::service_provider::BEGIN_SIGN_IN), declared(crate::service_provider::ADVANCE_SIGN_IN));
      if steps.0 != steps.1 || (!steps.0 && !declared(crate::service_provider::AUTHENTICATE)) {
        return Err(HostError::Config(
          config.config_dir(),
          format!(
            "auth.client `{client}` declares neither `{}` nor both `{}` and `{}`, so nobody can sign in through it",
            crate::service_provider::AUTHENTICATE,
            crate::service_provider::BEGIN_SIGN_IN,
            crate::service_provider::ADVANCE_SIGN_IN
          ),
        ));
      }
      let name = format!("service via {}", method.client.clone().unwrap_or_default());
      Ok((Arc::new(ServiceProvider::new(services.clone(), client)), name))
    }
    #[cfg(feature = "oidc")]
    "oidc" => oidc_provider(method, config),
    #[cfg(feature = "passkey")]
    "passkey" => passkey_provider(method, config),
    #[cfg(not(feature = "oidc"))]
    "oidc" => Err(missing_feature(method, config)),
    #[cfg(not(feature = "passkey"))]
    "passkey" => Err(missing_feature(method, config)),
    other => Err(HostError::Value("auth.provider".to_owned(), other.to_owned())),
  }
}

/// A method whose provider is a host feature this build left out.
#[cfg(not(all(feature = "oidc", feature = "passkey")))]
fn missing_feature(method: &crate::config::MethodSection, config: &Config) -> HostError {
  HostError::Config(config.config_dir(), format!("auth.methods `{}`: provider = \"{}\" needs the host's `{}` feature", method.name, method.provider, method.provider))
}

/// The `file` provider's table for a method, `auth.toml` beside the configuration when it names none.
fn users_of(method: &crate::config::MethodSection, config: &Config) -> Result<DevProvider, HostError> {
  let users = config.config_dir().join(method.users.as_deref().unwrap_or("auth.toml"));
  DevProvider::from_toml(&users).map_err(|e| HostError::Config(users.clone(), e))
}

#[cfg(feature = "oidc")]
fn secret_value(config: &Config, name: &Option<String>) -> Option<String> {
  name.as_ref().and_then(|name| config.secrets.get(name)).and_then(|entry| entry.value.as_ref()).map(|value| value.expose().to_owned()).filter(|value| !value.is_empty())
}

/// An `oidc` method over its issuer. In development, while a secret it names has no value, it is a
/// stand-in signing in as any user of the method's `users` table.
#[cfg(feature = "oidc")]
fn oidc_provider(method: &crate::config::MethodSection, config: &Config) -> Result<(Arc<dyn IdentityProvider>, String), HostError> {
  let refuse = |why: String| HostError::Config(config.config_dir(), format!("auth.methods `{}`: {why}", method.name));
  let label = method.preset.clone().or_else(|| method.issuer.clone()).unwrap_or_default();
  let missing = [&method.client_secret, &method.signing_key].into_iter().flatten().find(|name| secret_value(config, &Some((*name).clone())).is_none()).cloned();
  if let Some(name) = missing {
    if !config.dev() {
      return Err(refuse(format!("`secrets.{name}.value` has no value; set C5_SECRETS__{}__VALUE", name.to_ascii_uppercase())));
    }
    return Ok((Arc::new(users_of(method, config)?.stand_in()), format!("oidc {label}, a stand-in until secrets.{name} has a value")));
  }
  let origin = match config.origin()? {
    Some(origin) => origin,
    None if config.dev() => format!("http://{}", config.server.listen),
    None => return Err(refuse("an oidc method needs `document.origin` to name its redirect URI".to_owned())),
  };
  let redirect = format!("{origin}/auth/callback");
  let client_id = method.client_id.clone().unwrap_or_default();
  let mut oidc = match method.preset.as_deref() {
    Some("google") => OidcConfig::google(client_id, redirect),
    Some("microsoft") => OidcConfig::microsoft(method.tenant.as_deref().unwrap_or("common"), client_id, redirect),
    Some("apple") => {
      let pem = secret_value(config, &method.signing_key).unwrap_or_default();
      let body: String = pem.lines().filter(|line| !line.starts_with("-----")).collect();
      let pkcs8 = base64::engine::general_purpose::STANDARD.decode(body.trim()).map_err(|e| refuse(format!("the signing key is not a PEM: {e}")))?;
      OidcConfig::apple(client_id, redirect, method.team_id.clone().unwrap_or_default(), method.key_id.clone().unwrap_or_default(), pkcs8)
    }
    _ => OidcConfig::new(method.issuer.clone().unwrap_or_default(), client_id, redirect),
  };
  if let Some(secret) = secret_value(config, &method.client_secret) {
    oidc = oidc.secret(ClientSecret::Post(secret));
  }
  if let Some(scopes) = &method.scopes {
    oidc = oidc.scopes(scopes.clone());
  }
  oidc = oidc.rename(method.claims.clone());
  Ok((Arc::new(OidcProvider::new(oidc)), format!("oidc {label}")))
}

/// A `passkey` method over its JSON file. The relying party is the origin's host; in development
/// without `document.origin` it is `localhost` on the listening port, so the site is browsed at
/// `http://localhost:<port>`, since a passkey is never bound to an IP address.
#[cfg(feature = "passkey")]
fn passkey_provider(method: &crate::config::MethodSection, config: &Config) -> Result<(Arc<dyn IdentityProvider>, String), HostError> {
  let origin = match config.origin()? {
    Some(origin) => origin,
    None if config.dev() => format!("http://localhost:{}", config.server.listen.rsplit(':').next().unwrap_or("80")),
    None => return Err(HostError::Config(config.config_dir(), format!("auth.methods `{}`: a passkey method needs `document.origin`, the site its passkeys are bound to", method.name))),
  };
  let host = origin.split_once("://").map(|(_, rest)| rest).unwrap_or(&origin);
  let rp_id = host.rsplit_once(':').filter(|(_, port)| port.chars().all(|c| c.is_ascii_digit())).map_or(host, |(name, _)| name).to_owned();
  let title = config.document.title.literal();
  let rp_name = method.rp_name.clone().or_else(|| (!title.is_empty()).then(|| title.to_owned())).unwrap_or_else(|| rp_id.clone());
  let store = config.root.join(method.store.as_deref().unwrap_or_default()).join("passkeys.json");
  let passkeys = PasskeyConfig::new(rp_id.clone(), rp_name, origin).require_verification(method.verification.as_deref() == Some("required"));
  Ok((Arc::new(Passkeys::new(passkeys, Arc::new(FilePasskeys::new(store)))), format!("passkey for {rp_id}")))
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
    return match snapfire_fsr_core::json::to_value(&json) {
      Value::Map(map) => Ok(map),
      _ => Err("the callback body must be an object".to_owned()),
    };
  }
  Ok(form_params(req.body()))
}

pub(crate) fn form_field(body: &[u8], name: &str) -> Option<String> {
  match form_params(body).get(name) {
    Some(Value::Str(value)) => Some(value.to_string()),
    _ => None,
  }
}
