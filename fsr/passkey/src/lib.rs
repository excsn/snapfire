//! Passkeys as an `IdentityProvider`: WebAuthn sign-in and enrolment with the relying party's
//! checks done over `ring` and the credentials kept in a `PasskeyStore`.

mod store;
pub mod webauthn;

use std::sync::Arc;

use futures_util::future::BoxFuture;
use ring::rand::{SecureRandom, SystemRandom};
use serde_json::{json, Map, Value as Json};
use snapfire_fsr_auth::{AuthError, AuthOutcome, IdentityProvider, Intent, Next, Start};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;

pub use store::{Credential, FilePasskeys, MemoryPasskeys, PasskeyStore};
use webauthn::{decode, encode, Failed, EDDSA, ES256, RS256};

/// The relying party: the site passkeys are made for.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PasskeyConfig {
  /// The domain a passkey is bound to, the origin's host or a registrable suffix of it; never an
  /// IP address.
  pub rp_id: String,
  /// What the browser's prompt calls the site.
  pub rp_name: String,
  /// The exact origin ceremonies run on, `https://shop.example`.
  pub origin: String,
  /// Whether the authenticator must verify the user (a PIN or biometric); preferred otherwise.
  pub require_verification: bool,
  /// How long the browser's prompt waits, in milliseconds.
  pub timeout_ms: u64,
}

impl PasskeyConfig {
  pub fn new(rp_id: impl Into<String>, rp_name: impl Into<String>, origin: impl Into<String>) -> Self {
    Self { rp_id: rp_id.into(), rp_name: rp_name.into(), origin: origin.into(), require_verification: false, timeout_ms: 60_000 }
  }

  pub fn require_verification(mut self, required: bool) -> Self {
    self.require_verification = required;
    self
  }
}

/// Passkeys over a store. A sign-in with no identity is discoverable: the browser offers every
/// passkey it holds for the site and the one chosen names whose it is. A sign-in that names an
/// identity, the second factor of a `Sequence`, offers only that identity's passkeys. It creates
/// the identity's first one when it holds none. An enrolment creates a passkey for whoever is signed in.
pub struct Passkeys {
  config: PasskeyConfig,
  store: Arc<dyn PasskeyStore>,
}

fn challenge() -> String {
  let mut bytes = [0u8; 32];
  SystemRandom::new().fill(&mut bytes).expect("the system random source answers");
  encode(&bytes)
}

fn json_value(json: &Json) -> Value {
  match json {
    Json::Null => Value::Null,
    Json::Bool(b) => Value::Bool(*b),
    Json::Number(n) => match n.as_i64() {
      Some(i) => Value::int(i),
      None => Value::F64(n.as_f64().unwrap_or_default()),
    },
    Json::String(s) => Value::str(s.clone()),
    Json::Array(items) => Value::seq(items.iter().map(json_value).collect::<Vec<_>>()),
    Json::Object(map) => Value::Map(map.iter().map(|(k, v)| (k.clone(), json_value(v))).collect()),
  }
}

fn value_json(value: &Value) -> Json {
  match value {
    Value::Null => Json::Null,
    Value::Bool(b) => Json::Bool(*b),
    Value::Int(i) => i64::try_from(*i).map(Json::from).unwrap_or(Json::Null),
    Value::UInt(u) => u64::try_from(*u).map(Json::from).unwrap_or(Json::Null),
    Value::F64(f) => serde_json::Number::from_f64(*f).map(Json::Number).unwrap_or(Json::Null),
    Value::Str(s) => Json::String(s.to_string()),
    Value::Seq(items) => Json::Array(items.iter().map(value_json).collect()),
    Value::Map(map) => Json::Object(map.iter().map(|(k, v)| (k.clone(), value_json(v))).collect()),
    _ => Json::Null,
  }
}

fn object(json: Json) -> ValueMap {
  match json_value(&json) {
    Value::Map(map) => map,
    _ => ValueMap::default(),
  }
}

fn text(map: &ValueMap, key: &str) -> Option<String> {
  match map.get(key) {
    Some(Value::Str(text)) => Some(text.to_string()),
    _ => None,
  }
}

fn denied(failed: Failed) -> AuthError {
  AuthError::Denied(failed.0)
}

fn store_failed(why: String) -> AuthError {
  AuthError::Invalid(format!("the passkey store: {why}"))
}

/// A field of the posted credential, base64url decoded.
fn field(credential: &Json, path: &[&str]) -> Result<Vec<u8>, AuthError> {
  let mut at = credential;
  for key in path {
    at = at.get(key).ok_or_else(|| AuthError::Invalid(format!("the credential has no `{}`", path.join("."))))?;
  }
  decode(at.as_str().ok_or_else(|| AuthError::Invalid(format!("`{}` is not a string", path.join("."))))?).map_err(|e| AuthError::Invalid(format!("`{}`: {e}", path.join("."))))
}

impl Passkeys {
  pub fn new(config: PasskeyConfig, store: Arc<dyn PasskeyStore>) -> Self {
    Self { config, store }
  }

  fn verification(&self) -> &'static str {
    if self.config.require_verification { "required" } else { "preferred" }
  }

  async fn sign_in(&self, start: &Start) -> Result<Next, AuthError> {
    let challenge = challenge();
    let mut state = Map::new();
    state.insert("stage".to_owned(), json!("get"));
    state.insert("challenge".to_owned(), json!(challenge));
    let mut allow = Vec::new();
    if let Some(identity) = &start.identity {
      let held = self.store.of_subject(&identity.subject).await.map_err(store_failed)?;
      if held.is_empty() {
        return self.enrol(identity.clone()).await;
      }
      allow = held.iter().map(|c| json!({ "type": "public-key", "id": c.id })).collect();
      state.insert("subject".to_owned(), json!(identity.subject));
    }
    let options = json!({ "publicKey": { "challenge": challenge, "rpId": self.config.rp_id, "timeout": self.config.timeout_ms, "userVerification": self.verification(), "allowCredentials": allow } });
    Ok(Next::present("webauthn.get", object(options), object(Json::Object(state))))
  }

  async fn enrol(&self, identity: Identity) -> Result<Next, AuthError> {
    let challenge = challenge();
    let held = self.store.of_subject(&identity.subject).await.map_err(store_failed)?;
    let name = match identity.claims.get("email") {
      Some(Value::Str(email)) => email.to_string(),
      _ => identity.subject.clone(),
    };
    let params: Vec<Json> = [ES256, EDDSA, RS256].iter().map(|alg| json!({ "type": "public-key", "alg": alg })).collect();
    let exclude: Vec<Json> = held.iter().map(|c| json!({ "type": "public-key", "id": c.id })).collect();
    let options = json!({ "publicKey": {
      "challenge": challenge,
      "rp": { "id": self.config.rp_id, "name": self.config.rp_name },
      "user": { "id": encode(identity.subject.as_bytes()), "name": name, "displayName": name },
      "pubKeyCredParams": params,
      "timeout": self.config.timeout_ms,
      "attestation": "none",
      "authenticatorSelection": { "residentKey": "required", "requireResidentKey": true, "userVerification": self.verification() },
      "excludeCredentials": exclude,
    } });
    let claims: Json = value_json(&Value::Map(identity.claims.clone()));
    let state = json!({ "stage": "create", "challenge": challenge, "subject": identity.subject, "claims": claims });
    Ok(Next::present("webauthn.create", object(options), object(state)))
  }

  async fn assert(&self, credential: &Json, state: &ValueMap) -> Result<Next, AuthError> {
    let id = encode(&field(credential, &["rawId"]).or_else(|_| field(credential, &["id"]))?);
    let client = field(credential, &["response", "clientDataJSON"])?;
    let data = field(credential, &["response", "authenticatorData"])?;
    let signature = field(credential, &["response", "signature"])?;
    webauthn::client_data(&client, "webauthn.get", &text(state, "challenge").unwrap_or_default(), &self.config.origin).map_err(denied)?;
    let mut held = self.store.find(&id).await.map_err(store_failed)?.ok_or_else(|| AuthError::Denied("this passkey is not enrolled here".to_owned()))?;
    if text(state, "subject").is_some_and(|subject| subject != held.subject) {
      return Err(AuthError::Denied("this passkey belongs to another identity".to_owned()));
    }
    if let Ok(handle) = field(credential, &["response", "userHandle"]) {
      if handle != held.subject.as_bytes() {
        return Err(AuthError::Denied("the passkey's user is not its credential's".to_owned()));
      }
    }
    let auth = webauthn::auth_data(&data, &self.config.rp_id, self.config.require_verification).map_err(denied)?;
    let mut message = data.clone();
    message.extend_from_slice(ring::digest::digest(&ring::digest::SHA256, &client).as_ref());
    if !held.key.verifies(&message, &signature) {
      return Err(AuthError::Denied("the passkey's signature does not verify".to_owned()));
    }
    if (auth.sign_count != 0 || held.sign_count != 0) && auth.sign_count <= held.sign_count {
      return Err(AuthError::Denied("the passkey's counter went backwards, which a cloned authenticator does".to_owned()));
    }
    held.sign_count = auth.sign_count;
    self.store.save(held.clone()).await.map_err(store_failed)?;
    let claims = match json_value(&Json::Object(held.claims.clone())) {
      Value::Map(map) => map,
      _ => ValueMap::default(),
    };
    Ok(Next::Complete(AuthOutcome::new(Identity::new(held.subject, claims), ValueMap::default())))
  }

  async fn create(&self, credential: &Json, state: &ValueMap) -> Result<Next, AuthError> {
    let id = encode(&field(credential, &["rawId"]).or_else(|_| field(credential, &["id"]))?);
    let client = field(credential, &["response", "clientDataJSON"])?;
    let object_bytes = field(credential, &["response", "attestationObject"])?;
    webauthn::client_data(&client, "webauthn.create", &text(state, "challenge").unwrap_or_default(), &self.config.origin).map_err(denied)?;
    let data = webauthn::attestation_auth_data(&object_bytes).map_err(denied)?;
    let auth = webauthn::auth_data(&data, &self.config.rp_id, self.config.require_verification).map_err(denied)?;
    let (attested_id, key) = auth.attested.ok_or_else(|| AuthError::Invalid("the attestation carries no credential".to_owned()))?;
    if encode(&attested_id) != id {
      return Err(AuthError::Invalid("the attested credential is not the one posted".to_owned()));
    }
    if self.store.find(&id).await.map_err(store_failed)?.is_some() {
      return Err(AuthError::Invalid("this passkey is already enrolled".to_owned()));
    }
    let subject = text(state, "subject").ok_or_else(|| AuthError::Invalid("an enrolment without its identity".to_owned()))?;
    let claims_value = state.get("claims").cloned().unwrap_or(Value::Map(ValueMap::default()));
    let claims = match value_json(&claims_value) {
      Json::Object(map) => map,
      _ => Map::new(),
    };
    self.store.save(Credential::new(id, subject.clone(), claims, key, auth.sign_count)).await.map_err(store_failed)?;
    let identity_claims = match claims_value {
      Value::Map(map) => map,
      _ => ValueMap::default(),
    };
    Ok(Next::Complete(AuthOutcome::new(Identity::new(subject, identity_claims), ValueMap::default())))
  }
}

impl IdentityProvider for Passkeys {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      match (start.intent, start.identity.clone()) {
        (Intent::Enrol, Some(identity)) => self.enrol(identity).await,
        (Intent::Enrol, None) => Err(AuthError::Denied("enrolling a passkey needs a signed-in session".to_owned())),
        _ => self.sign_in(&start).await,
      }
    })
  }

  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
    Box::pin(async move {
      let credential = params.get("credential").map(value_json).ok_or_else(|| AuthError::Invalid("the answer carries no credential".to_owned()))?;
      match text(&state, "stage").as_deref() {
        Some("get") => self.assert(&credential, &state).await,
        Some("create") => self.create(&credential, &state).await,
        _ => Err(AuthError::Invalid("a step without its ceremony".to_owned())),
      }
    })
  }
}
