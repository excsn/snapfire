//! OpenID Connect sign-in as an `IdentityProvider`: the authorization code flow with PKCE against
//! any issuer's discovery document (OpenID Connect Core 1.0 section 3.1, RFC 7636), the ID token
//! verified against the issuer's published keys and the tokens refreshed with its refresh token.

mod jwt;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use ring::rand::{SecureRandom, SystemRandom};
use serde::Deserialize;
use serde_json::{Map, Value as Json};
use snapfire_fsr_auth::{AuthError, AuthOutcome, IdentityProvider, Intent, Next, Start};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{unix_now, Identity};

pub use jwt::{sign_es256, verify, Jwk, JwkSet, Refused};

/// How far a token's times may disagree with this clock.
pub const CLOCK_SKEW_SECS: u64 = 60;

/// How long a discovery document is used before it is fetched again.
pub const DISCOVERY_TTL: Duration = Duration::from_secs(3600);

/// The least time between two fetches of the key set by default, so tokens naming unknown keys
/// cannot make the host hammer the issuer. An issuer publishes a key before it signs with it.
pub const JWKS_REFETCH: Duration = Duration::from_secs(60);

/// Claims the protocol defines for the token itself rather than the person, left out of
/// `Identity::claims`.
const PROTOCOL_CLAIMS: &[&str] = &["iss", "aud", "exp", "iat", "nbf", "nonce", "at_hash", "c_hash", "auth_time", "azp", "jti", "sid"];

/// How the client proves itself at the token endpoint.
#[derive(Clone)]
#[non_exhaustive]
pub enum ClientSecret {
  /// A public client: PKCE alone.
  None,
  /// `client_secret` sent in the form (`client_secret_post`).
  Post(String),
  /// A JWT signed ES256 with the key the issuer issued, minted per request: Apple's form.
  SignedJwt { team_id: String, key_id: String, pkcs8: Vec<u8>, audience: String },
}

impl std::fmt::Debug for ClientSecret {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(match self {
      Self::None => "None",
      Self::Post(_) => "Post(***)",
      Self::SignedJwt { .. } => "SignedJwt(***)",
    })
  }
}

/// One issuer as a client of it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct OidcConfig {
  /// The issuer URL; its discovery document is `<issuer>/.well-known/openid-configuration`. An
  /// issuer naming `{tenantid}` (Microsoft's multi-tenant ones) is matched with the token's `tid`.
  pub issuer: String,
  pub client_id: String,
  pub client_secret: ClientSecret,
  /// The callback the issuer sends the browser back to, `<origin>/auth/callback`.
  pub redirect_uri: String,
  pub scopes: Vec<String>,
  /// `form_post` for an issuer that posts the callback (Apple); the issuer's default when absent.
  pub response_mode: Option<String>,
  /// The claim names an application reads, each taken from the issuer's claim of the second name.
  pub rename: BTreeMap<String, String>,
  /// The least time between two fetches of the key set, `JWKS_REFETCH` unless set.
  pub key_refetch: Duration,
}

impl OidcConfig {
  pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self {
    Self {
      issuer: issuer.into().trim_end_matches('/').to_owned(),
      client_id: client_id.into(),
      client_secret: ClientSecret::None,
      redirect_uri: redirect_uri.into(),
      scopes: vec!["openid".to_owned(), "email".to_owned(), "profile".to_owned()],
      response_mode: None,
      rename: BTreeMap::new(),
      key_refetch: JWKS_REFETCH,
    }
  }

  pub fn key_refetch(mut self, every: Duration) -> Self {
    self.key_refetch = every;
    self
  }

  pub fn secret(mut self, secret: ClientSecret) -> Self {
    self.client_secret = secret;
    self
  }

  pub fn scopes(mut self, scopes: Vec<String>) -> Self {
    self.scopes = scopes;
    self
  }

  pub fn response_mode(mut self, mode: Option<String>) -> Self {
    self.response_mode = mode;
    self
  }

  pub fn rename(mut self, rename: BTreeMap<String, String>) -> Self {
    self.rename = rename;
    self
  }

  /// Google, `https://accounts.google.com`.
  pub fn google(client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self {
    Self::new("https://accounts.google.com", client_id, redirect_uri)
  }

  /// Microsoft Entra ID: `tenant` is a tenant id or domain. `common`, `organizations` and
  /// `consumers` name the multi-tenant endpoints.
  pub fn microsoft(tenant: &str, client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self {
    Self::new(format!("https://login.microsoftonline.com/{tenant}/v2.0"), client_id, redirect_uri)
  }

  /// Sign in with Apple: the callback is posted, the scopes are `name` and `email` and the client
  /// secret is a JWT signed with the key Apple issued, `team_id` and `key_id` naming it.
  pub fn apple(client_id: impl Into<String>, redirect_uri: impl Into<String>, team_id: impl Into<String>, key_id: impl Into<String>, pkcs8: Vec<u8>) -> Self {
    let secret = ClientSecret::SignedJwt { team_id: team_id.into(), key_id: key_id.into(), pkcs8, audience: "https://appleid.apple.com".to_owned() };
    Self::new("https://appleid.apple.com", client_id, redirect_uri)
      .scopes(vec!["openid".to_owned(), "name".to_owned(), "email".to_owned()])
      .response_mode(Some("form_post".to_owned()))
      .secret(secret)
  }
}

#[derive(Debug, Clone, Deserialize)]
struct Discovery {
  issuer: String,
  authorization_endpoint: String,
  token_endpoint: String,
  jwks_uri: String,
}

#[derive(Debug, Deserialize)]
struct TokenAnswer {
  #[serde(default)]
  id_token: Option<String>,
  #[serde(default)]
  access_token: Option<String>,
  #[serde(default)]
  refresh_token: Option<String>,
  #[serde(default)]
  expires_in: Option<u64>,
  #[serde(default)]
  error: Option<String>,
  #[serde(default)]
  error_description: Option<String>,
}

struct Keys {
  set: Vec<Jwk>,
  fetched: Option<Instant>,
}

/// An issuer as an `IdentityProvider`. `begin` redirects to its authorization endpoint with a
/// fresh `state`, `nonce` and PKCE challenge; `advance` takes the callback, exchanges the code and
/// verifies the ID token; `refresh` trades the refresh token. An enrolment is refused: linking an
/// issuer's account to someone already signed in needs a store of who is who, which an auth
/// service holds and the host does not.
pub struct OidcProvider {
  config: OidcConfig,
  http: reqwest::Client,
  discovery: Mutex<Option<(Arc<Discovery>, Instant)>>,
  keys: Mutex<Keys>,
}

fn random(bytes: usize) -> String {
  let mut buffer = vec![0u8; bytes];
  SystemRandom::new().fill(&mut buffer).expect("the system random source answers");
  URL_SAFE_NO_PAD.encode(buffer)
}

fn text(map: &ValueMap, key: &str) -> Option<String> {
  match map.get(key) {
    Some(Value::Str(text)) => Some(text.to_string()),
    _ => None,
  }
}

fn json_value(json: &Json) -> Value {
  match json {
    Json::Null => Value::Null,
    Json::Bool(b) => Value::Bool(*b),
    Json::Number(n) => match (n.as_i64(), n.as_u64()) {
      (Some(i), _) => Value::int(i),
      (None, Some(u)) => Value::UInt(u as u128),
      _ => Value::F64(n.as_f64().unwrap_or_default()),
    },
    Json::String(s) => Value::str(s.clone()),
    Json::Array(items) => Value::seq(items.iter().map(json_value).collect::<Vec<_>>()),
    Json::Object(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), json_value(v))).collect()),
  }
}

fn invalid(why: impl Into<String>) -> AuthError {
  AuthError::Invalid(why.into())
}

impl OidcProvider {
  pub fn new(config: OidcConfig) -> Self {
    Self { config, http: reqwest::Client::new(), discovery: Mutex::new(None), keys: Mutex::new(Keys { set: Vec::new(), fetched: None }) }
  }

  pub fn config(&self) -> &OidcConfig {
    &self.config
  }

  async fn discovery(&self) -> Result<Arc<Discovery>, AuthError> {
    if let Some((doc, at)) = self.discovery.lock().clone() {
      if at.elapsed() < DISCOVERY_TTL {
        return Ok(doc);
      }
    }
    let url = format!("{}/.well-known/openid-configuration", self.config.issuer);
    let doc: Discovery = self.http.get(&url).send().await.and_then(|r| r.error_for_status()).map_err(|e| invalid(format!("{url}: {e}")))?.json().await.map_err(|e| invalid(format!("{url}: {e}")))?;
    let doc = Arc::new(doc);
    *self.discovery.lock() = Some((doc.clone(), Instant::now()));
    Ok(doc)
  }

  /// The key set, fetched when it never was or, with `again`, when the last fetch is older than
  /// `key_refetch`.
  async fn keys(&self, again: bool) -> Result<Vec<Jwk>, AuthError> {
    {
      let keys = self.keys.lock();
      let fresh = keys.fetched.is_some_and(|at| !again || at.elapsed() < self.config.key_refetch);
      if fresh {
        return Ok(keys.set.clone());
      }
    }
    let doc = self.discovery().await?;
    let set: JwkSet = self.http.get(&doc.jwks_uri).send().await.and_then(|r| r.error_for_status()).map_err(|e| invalid(format!("{}: {e}", doc.jwks_uri)))?.json().await.map_err(|e| invalid(format!("{}: {e}", doc.jwks_uri)))?;
    let mut keys = self.keys.lock();
    keys.set = set.keys;
    keys.fetched = Some(Instant::now());
    Ok(keys.set.clone())
  }

  fn secret_fields(&self, form: &mut Vec<(&'static str, String)>) -> Result<(), AuthError> {
    match &self.config.client_secret {
      ClientSecret::None => {}
      ClientSecret::Post(secret) => form.push(("client_secret", secret.clone())),
      ClientSecret::SignedJwt { team_id, key_id, pkcs8, audience } => {
        let now = unix_now();
        let claims = serde_json::json!({ "iss": team_id, "iat": now, "exp": now + 300, "aud": audience, "sub": self.config.client_id });
        form.push(("client_secret", jwt::sign_es256(pkcs8, key_id, &claims).map_err(invalid)?));
      }
    }
    Ok(())
  }

  async fn token_request(&self, mut form: Vec<(&'static str, String)>) -> Result<TokenAnswer, AuthError> {
    let doc = self.discovery().await?;
    form.push(("client_id", self.config.client_id.clone()));
    self.secret_fields(&mut form)?;
    let response = self.http.post(&doc.token_endpoint).form(&form).send().await.map_err(|e| invalid(format!("{}: {e}", doc.token_endpoint)))?;
    let status = response.status();
    let answer: TokenAnswer = response.json().await.map_err(|e| invalid(format!("{}: {e}", doc.token_endpoint)))?;
    match (&answer.error, status.is_success()) {
      (None, true) => Ok(answer),
      (Some(error), _) if error == "invalid_grant" || error == "access_denied" => Err(AuthError::Denied(answer.error_description.clone().unwrap_or_else(|| error.clone()))),
      (error, _) => Err(invalid(format!("the token endpoint answered {status}: {}", error.clone().or(answer.error_description.clone()).unwrap_or_default()))),
    }
  }

  /// The ID token's claims once its signature, issuer, audience, times and nonce check out.
  async fn id_claims(&self, token: &str, nonce: &str) -> Result<Map<String, Json>, AuthError> {
    let claims = match jwt::verify(token, &self.keys(false).await?) {
      Err(Refused::UnknownKey(_)) => jwt::verify(token, &self.keys(true).await?),
      other => other,
    }
    .map_err(|e| invalid(format!("the ID token: {e}")))?;
    let doc = self.discovery().await?;
    let claim = |name: &str| claims.get(name).and_then(Json::as_str).map(str::to_owned);
    let expected = match claim("tid") {
      Some(tid) => doc.issuer.replace("{tenantid}", &tid),
      None => doc.issuer.clone(),
    };
    if claim("iss").as_deref() != Some(expected.as_str()) {
      return Err(invalid(format!("the ID token's issuer is {:?}, not {expected}", claim("iss"))));
    }
    let audiences: Vec<&str> = match claims.get("aud") {
      Some(Json::String(aud)) => vec![aud.as_str()],
      Some(Json::Array(auds)) => auds.iter().filter_map(Json::as_str).collect(),
      _ => Vec::new(),
    };
    if !audiences.contains(&self.config.client_id.as_str()) {
      return Err(invalid("the ID token was issued for another client"));
    }
    if audiences.len() > 1 && claim("azp").as_deref() != Some(self.config.client_id.as_str()) {
      return Err(invalid("the ID token's authorized party is another client"));
    }
    let now = unix_now();
    let time = |name: &str| claims.get(name).and_then(Json::as_u64);
    if time("exp").is_none_or(|exp| exp + CLOCK_SKEW_SECS < now) {
      return Err(AuthError::Denied("the ID token has expired".to_owned()));
    }
    if time("nbf").is_some_and(|nbf| nbf > now + CLOCK_SKEW_SECS) || time("iat").is_some_and(|iat| iat > now + CLOCK_SKEW_SECS) {
      return Err(invalid("the ID token is dated in the future"));
    }
    if claim("nonce").as_deref() != Some(nonce) {
      return Err(invalid("the ID token's nonce is not this sign-in's"));
    }
    Ok(claims)
  }

  fn identity(&self, claims: &Map<String, Json>) -> Result<Identity, AuthError> {
    let subject = claims.get("sub").and_then(Json::as_str).ok_or_else(|| invalid("the ID token names no subject"))?.to_owned();
    let mut kept: ValueMap = claims.iter().filter(|(name, _)| name.as_str() != "sub" && !PROTOCOL_CLAIMS.contains(&name.as_str())).map(|(name, value)| (name.clone(), json_value(value))).collect();
    for (ours, theirs) in &self.config.rename {
      if let Some(value) = claims.get(theirs) {
        kept.insert(ours.clone(), json_value(value));
      }
    }
    Ok(Identity::new(subject, kept))
  }
}

fn tokens_of(answer: &TokenAnswer) -> ValueMap {
  let mut tokens = ValueMap::default();
  if let Some(access) = &answer.access_token {
    tokens.insert("access_token".to_owned(), Value::str(access.clone()));
  }
  if let Some(refresh) = &answer.refresh_token {
    tokens.insert("refresh_token".to_owned(), Value::str(refresh.clone()));
  }
  if let Some(expires_in) = answer.expires_in {
    tokens.insert("expires_at".to_owned(), Value::int((unix_now() + expires_in) as i64));
  }
  tokens
}

impl IdentityProvider for OidcProvider {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      if start.intent == Intent::Enrol {
        return Err(invalid("linking an issuer's account to a signed-in identity needs an auth service, which holds who is who"));
      }
      let doc = self.discovery().await?;
      let (state, nonce, verifier) = (random(32), random(32), random(32));
      let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()));
      let mut query = form_urlencoded::Serializer::new(String::new());
      query
        .append_pair("response_type", "code")
        .append_pair("client_id", &self.config.client_id)
        .append_pair("redirect_uri", &self.config.redirect_uri)
        .append_pair("scope", &self.config.scopes.join(" "))
        .append_pair("state", &state)
        .append_pair("nonce", &nonce)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256");
      if let Some(mode) = &self.config.response_mode {
        query.append_pair("response_mode", mode);
      }
      let separator = if doc.authorization_endpoint.contains('?') { '&' } else { '?' };
      let mut kept = ValueMap::default();
      for (key, value) in [("state", state), ("nonce", nonce), ("verifier", verifier)] {
        kept.insert(key.to_owned(), Value::str(value));
      }
      Ok(Next::redirect(format!("{}{separator}{}", doc.authorization_endpoint, query.finish()), kept))
    })
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      if let Some(error) = text(&params, "error") {
        return Err(AuthError::Denied(text(&params, "error_description").unwrap_or(error)));
      }
      let (Some(sent), Some(kept)) = (text(&params, "state"), text(&state, "state")) else {
        return Err(invalid("the callback carries no state"));
      };
      if !snapfire_fsr_session::constant_time_eq(&sent, &kept) {
        return Err(invalid("the callback's state is not this sign-in's"));
      }
      let code = text(&params, "code").ok_or_else(|| invalid("the callback carries no code"))?;
      let verifier = text(&state, "verifier").ok_or_else(|| invalid("a sign-in without its verifier"))?;
      let answer = self
        .token_request(vec![("grant_type", "authorization_code".to_owned()), ("code", code), ("redirect_uri", self.config.redirect_uri.clone()), ("code_verifier", verifier)])
        .await?;
      let id_token = answer.id_token.clone().ok_or_else(|| invalid("the token endpoint answered no ID token"))?;
      let claims = self.id_claims(&id_token, &text(&state, "nonce").unwrap_or_default()).await?;
      Ok(Next::Complete(AuthOutcome::new(self.identity(&claims)?, tokens_of(&answer))))
    })
  }

  fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
    Box::pin(async move {
      let Some(refresh) = text(&tokens, "refresh_token") else {
        return Ok(None);
      };
      match self.token_request(vec![("grant_type", "refresh_token".to_owned()), ("refresh_token", refresh)]).await {
        Ok(answer) => Ok(Some(tokens_of(&answer))),
        Err(AuthError::Denied(_)) => Ok(None),
        Err(error) => Err(error),
      }
    })
  }
}
