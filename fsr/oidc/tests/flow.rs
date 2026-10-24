use std::collections::HashMap;
use std::sync::Arc;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use parking_lot::Mutex;
use ring::rand::SystemRandom;
use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};
use serde_json::json;
use snapfire_fsr_auth::{AuthError, IdentityProvider, Next, Start};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_oidc::{sign_es256, verify, ClientSecret, Jwk, OidcConfig, OidcProvider};
use snapfire_fsr_runtime::unix_now;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct Key {
  kid: String,
  pkcs8: Vec<u8>,
  public: Vec<u8>,
}

fn key(kid: &str) -> Key {
  let rng = SystemRandom::new();
  let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap().as_ref().to_vec();
  let public = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &pkcs8, &rng).unwrap().public_key().as_ref().to_vec();
  Key { kid: kid.to_owned(), pkcs8, public }
}

fn jwk(key: &Key) -> serde_json::Value {
  json!({ "kty": "EC", "crv": "P-256", "kid": key.kid, "alg": "ES256", "use": "sig", "x": URL_SAFE_NO_PAD.encode(&key.public[1..33]), "y": URL_SAFE_NO_PAD.encode(&key.public[33..]) })
}

/// What the mock issuer does differently from a well-behaved one, set per test.
#[derive(Default)]
struct Knobs {
  challenge: Option<String>,
  nonce: Option<String>,
  aud: Option<String>,
  exp_offset: i64,
  revoked: bool,
  jwks_fetches: usize,
}

struct Issuer {
  base: String,
  signing: Mutex<Key>,
  published: Mutex<Vec<serde_json::Value>>,
  knobs: Mutex<Knobs>,
}

impl Issuer {
  async fn start() -> Arc<Self> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let signing = key("k1");
    let published = vec![jwk(&signing)];
    let issuer = Arc::new(Issuer { base, signing: Mutex::new(signing), published: Mutex::new(published), knobs: Mutex::new(Knobs { exp_offset: 300, ..Knobs::default() }) });
    let serving = issuer.clone();
    tokio::spawn(async move {
      loop {
        let Ok((mut stream, _)) = listener.accept().await else { continue };
        let issuer = serving.clone();
        tokio::spawn(async move {
          let mut buffer = Vec::new();
          let mut chunk = [0u8; 4096];
          let (head, body) = loop {
            let read = stream.read(&mut chunk).await.unwrap_or(0);
            if read == 0 {
              return;
            }
            buffer.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&buffer).to_string();
            if let Some(at) = text.find("\r\n\r\n") {
              let head = text[..at].to_owned();
              let length = head.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
              if buffer.len() >= at + 4 + length {
                break (head, String::from_utf8_lossy(&buffer[at + 4..at + 4 + length]).to_string());
              }
            }
          };
          let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
          let (status, answer) = issuer.answer(&path, &body);
          let response = format!("HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{answer}", answer.len());
          let _ = stream.write_all(response.as_bytes()).await;
        });
      }
    });
    issuer
  }

  fn answer(&self, path: &str, body: &str) -> (&'static str, String) {
    match path {
      "/.well-known/openid-configuration" => ("200 OK", json!({ "issuer": self.base, "authorization_endpoint": format!("{}/authorize", self.base), "token_endpoint": format!("{}/token", self.base), "jwks_uri": format!("{}/jwks", self.base) }).to_string()),
      "/jwks" => {
        self.knobs.lock().jwks_fetches += 1;
        ("200 OK", json!({ "keys": *self.published.lock() }).to_string())
      }
      "/token" => {
        let form: HashMap<String, String> = form_urlencoded::parse(body.as_bytes()).into_owned().collect();
        let knobs = self.knobs.lock();
        match form.get("grant_type").map(String::as_str) {
          Some("authorization_code") => {
            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
            let challenge = URL_SAFE_NO_PAD.encode(ring::digest::digest(&ring::digest::SHA256, verifier.as_bytes()));
            if form.get("code").map(String::as_str) != Some("good") || knobs.challenge.as_deref() != Some(challenge.as_str()) || form.get("client_secret").map(String::as_str) != Some("shh") {
              return ("400 Bad Request", json!({ "error": "invalid_grant", "error_description": "bad code or verifier" }).to_string());
            }
            let now = unix_now() as i64;
            let claims = json!({ "iss": self.base, "aud": knobs.aud.clone().unwrap_or_else(|| "app".to_owned()), "sub": "u-1", "email": "ada@example.com", "groups": ["ops"], "exp": now + knobs.exp_offset, "iat": now, "nonce": knobs.nonce.clone().unwrap_or_default() });
            let signing = self.signing.lock();
            let id_token = sign_es256(&signing.pkcs8, &signing.kid, &claims).unwrap();
            ("200 OK", json!({ "id_token": id_token, "access_token": "at-1", "refresh_token": "rt-1", "expires_in": 3600, "token_type": "Bearer" }).to_string())
          }
          Some("refresh_token") if !knobs.revoked && form.get("refresh_token").map(String::as_str) == Some("rt-1") => ("200 OK", json!({ "access_token": "at-2", "expires_in": 3600 }).to_string()),
          _ => ("400 Bad Request", json!({ "error": "invalid_grant" }).to_string()),
        }
      }
      _ => ("404 Not Found", "{}".to_owned()),
    }
  }

  fn config(&self) -> OidcConfig {
    let rename = [("roles".to_owned(), "groups".to_owned())].into_iter().collect();
    OidcConfig::new(&self.base, "app", "https://shop.example/auth/callback").secret(ClientSecret::Post("shh".to_owned())).rename(rename)
  }

  fn provider(&self) -> OidcProvider {
    OidcProvider::new(self.config())
  }
}

fn query(url: &str) -> HashMap<String, String> {
  form_urlencoded::parse(url.split_once('?').unwrap().1.as_bytes()).into_owned().collect()
}

fn map(pairs: &[(&str, &str)]) -> ValueMap {
  pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::str((*v).to_owned()))).collect()
}

/// Begins a sign-in, tells the issuer which challenge and nonce it will see and answers the code.
async fn sign_in(issuer: &Issuer, provider: &OidcProvider, code: &str, tamper_state: bool) -> Result<Next, AuthError> {
  let Next::Redirect { to, state } = provider.begin(Start::sign_in("/")).await? else { panic!("a redirect") };
  let sent = query(&to);
  {
    let mut knobs = issuer.knobs.lock();
    knobs.challenge = sent.get("code_challenge").cloned();
    if knobs.nonce.is_none() {
      knobs.nonce = sent.get("nonce").cloned();
    }
  }
  let returned = if tamper_state { "forged".to_owned() } else { sent["state"].clone() };
  provider.advance(map(&[("code", code), ("state", &returned)]), state).await
}

#[tokio::test]
async fn the_code_flow_redirects_with_pkce_and_completes_with_the_id_tokens_subject() {
  let issuer = Issuer::start().await;
  let provider = issuer.provider();
  let Next::Redirect { to, state } = provider.begin(Start::sign_in("/")).await.unwrap() else { panic!("a redirect") };
  assert!(to.starts_with(&format!("{}/authorize?", issuer.base)), "{to}");
  let sent = query(&to);
  assert_eq!((sent["response_type"].as_str(), sent["client_id"].as_str(), sent["code_challenge_method"].as_str()), ("code", "app", "S256"));
  assert_eq!(sent["redirect_uri"], "https://shop.example/auth/callback");
  assert_eq!(sent["scope"], "openid email profile");
  assert!(state.get("verifier").is_some() && !to.contains(&match state.get("verifier") { Some(Value::Str(v)) => v.to_string(), _ => String::new() }), "the verifier never leaves the server");

  let Next::Complete(outcome) = sign_in(&issuer, &provider, "good", false).await.unwrap() else { panic!("complete") };
  assert_eq!(outcome.identity.subject, "u-1");
  assert_eq!(outcome.identity.claims.get("email"), Some(&Value::str("ada@example.com")));
  assert_eq!(outcome.identity.claims.get("roles"), Some(&Value::seq(vec![Value::str("ops")])), "a renamed claim");
  assert!(outcome.identity.claims.get("nonce").is_none() && outcome.identity.claims.get("aud").is_none(), "protocol claims stay out");
  assert_eq!(outcome.tokens.get("access_token"), Some(&Value::str("at-1")));
  assert_eq!(outcome.tokens.get("refresh_token"), Some(&Value::str("rt-1")));
  assert!(matches!(outcome.tokens.get("expires_at"), Some(Value::Int(at)) if *at > unix_now() as i128));
}

#[tokio::test]
async fn a_callback_with_another_state_or_a_refusal_does_not_sign_in() {
  let issuer = Issuer::start().await;
  let provider = issuer.provider();
  assert!(matches!(sign_in(&issuer, &provider, "good", true).await, Err(AuthError::Invalid(_))));
  assert!(matches!(sign_in(&issuer, &provider, "bad", false).await, Err(AuthError::Denied(_))), "the issuer refused the code");
  let Next::Redirect { state, .. } = provider.begin(Start::sign_in("/")).await.unwrap() else { panic!() };
  assert!(matches!(provider.advance(map(&[("error", "access_denied"), ("error_description", "the user said no")]), state).await, Err(AuthError::Denied(why)) if why == "the user said no"));
}

#[tokio::test]
async fn an_id_token_for_another_nonce_audience_or_past_its_end_is_refused() {
  let issuer = Issuer::start().await;
  issuer.knobs.lock().nonce = Some("someone-elses".to_owned());
  let refused = sign_in(&issuer, &issuer.provider(), "good", false).await;
  assert!(matches!(&refused, Err(AuthError::Invalid(why)) if why.contains("nonce")), "{:?}", refused.err());

  let issuer = Issuer::start().await;
  issuer.knobs.lock().aud = Some("another-app".to_owned());
  assert!(matches!(sign_in(&issuer, &issuer.provider(), "good", false).await, Err(AuthError::Invalid(why)) if why.contains("another client")));

  let issuer = Issuer::start().await;
  issuer.knobs.lock().exp_offset = -600;
  assert!(matches!(sign_in(&issuer, &issuer.provider(), "good", false).await, Err(AuthError::Denied(why)) if why.contains("expired")));
}

#[tokio::test]
async fn a_rotated_key_is_fetched_once_when_a_token_names_it() {
  let issuer = Issuer::start().await;
  let provider = OidcProvider::new(issuer.config().key_refetch(std::time::Duration::ZERO));
  assert!(matches!(sign_in(&issuer, &provider, "good", false).await, Ok(Next::Complete(_))));
  assert_eq!(issuer.knobs.lock().jwks_fetches, 1);
  let rotated = key("k2");
  issuer.published.lock().push(jwk(&rotated));
  *issuer.signing.lock() = rotated;
  issuer.knobs.lock().nonce = None;
  assert!(matches!(sign_in(&issuer, &provider, "good", false).await, Ok(Next::Complete(_))), "signed with a key the cached set lacks");
  assert_eq!(issuer.knobs.lock().jwks_fetches, 2);
}

#[tokio::test]
async fn refresh_trades_the_refresh_token_and_a_revoked_one_renews_nothing() {
  let issuer = Issuer::start().await;
  let provider = issuer.provider();
  let renewed = provider.refresh(map(&[("refresh_token", "rt-1")])).await.unwrap().expect("renewed");
  assert_eq!(renewed.get("access_token"), Some(&Value::str("at-2")));
  assert_eq!(provider.refresh(ValueMap::default()).await.unwrap(), None, "no refresh token, nothing to trade");
  issuer.knobs.lock().revoked = true;
  assert_eq!(provider.refresh(map(&[("refresh_token", "rt-1")])).await.unwrap(), None);
}

#[tokio::test]
async fn enrolling_is_refused_since_linking_needs_an_auth_service() {
  let issuer = Issuer::start().await;
  let identity = snapfire_fsr_runtime::Identity::new("u-1".to_owned(), ValueMap::default());
  assert!(matches!(issuer.provider().begin(Start::enrol("/", identity)).await, Err(AuthError::Invalid(why)) if why.contains("auth service")));
}

#[test]
fn only_an_asymmetric_signature_under_a_published_key_verifies() {
  let signer = key("k1");
  let keys: Vec<Jwk> = serde_json::from_value(json!([jwk(&signer)])).unwrap();
  let token = sign_es256(&signer.pkcs8, "k1", &json!({ "sub": "u-1" })).unwrap();
  assert_eq!(verify(&token, &keys).unwrap()["sub"], "u-1");

  let (head, body) = (URL_SAFE_NO_PAD.encode(r#"{"alg":"none"}"#), URL_SAFE_NO_PAD.encode(r#"{"sub":"mallory"}"#));
  assert!(verify(&format!("{head}.{body}."), &keys).is_err(), "alg none never verifies");
  let hs = URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256","kid":"k1"}"#);
  assert!(verify(&format!("{hs}.{body}.c2ln"), &keys).is_err(), "nor does an HMAC");

  let mut parts: Vec<String> = token.split('.').map(str::to_owned).collect();
  parts[1] = URL_SAFE_NO_PAD.encode(r#"{"sub":"mallory"}"#);
  assert!(verify(&parts.join("."), &keys).is_err(), "a changed payload fails the signature");
  let stranger = key("k1");
  let forged = sign_es256(&stranger.pkcs8, "k1", &json!({ "sub": "mallory" })).unwrap();
  assert!(verify(&forged, &keys).is_err(), "a key the issuer never published");
}

#[tokio::test]
async fn within_the_refetch_floor_an_unknown_key_is_refused_without_a_fetch() {
  let issuer = Issuer::start().await;
  let provider = issuer.provider();
  assert!(matches!(sign_in(&issuer, &provider, "good", false).await, Ok(Next::Complete(_))));
  *issuer.signing.lock() = key("k9");
  issuer.knobs.lock().nonce = None;
  assert!(matches!(sign_in(&issuer, &provider, "good", false).await, Err(AuthError::Invalid(why)) if why.contains("k9")));
  assert_eq!(issuer.knobs.lock().jwks_fetches, 1, "the key set was not fetched again");
}
