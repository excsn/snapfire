//! A session store and an identity provider that live behind a client, so the
//! host holds neither accounts nor sessions in its own memory. The client's
//! contract declares `getSession`, `putSession` and `deleteSession` for the
//! store and `authenticate` for the provider; the record travels as one string
//! in the payload's JSON encoding, so the service stores an opaque blob.

use std::future::ready;
use std::sync::Arc;

use futures_util::future::BoxFuture;
use snapfire_fsr_auth::{AuthError, AuthOutcome, Begin, IdentityProvider};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{FailureKind, Identity, ServiceError};
use snapfire_fsr_service::Services;
use snapfire_fsr_session::{SessionId, SessionRecord, SessionStore};

pub struct ServiceSessionStore {
  services: Arc<Services>,
  client: String,
}

impl ServiceSessionStore {
  pub fn new(services: Arc<Services>, client: impl Into<String>) -> Self {
    Self { services, client: client.into() }
  }

  fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.services.bind_anonymous().call(&self.client, method, args)
  }
}

fn id_args(id: &SessionId) -> ValueMap {
  let mut args = ValueMap::new();
  args.insert("id".to_owned(), Value::Str(id.0.clone()));
  args
}

/// The record as one JSON string in the payload encoding.
pub fn encode_record(record: &SessionRecord) -> String {
  let mut map = ValueMap::new();
  map.insert("data".to_owned(), Value::Map(record.data.clone()));
  map.insert("identity".to_owned(), match &record.identity {
    Some(identity) => Value::Map(identity_map(identity)),
    None => Value::Null,
  });
  map.insert("tokens".to_owned(), Value::Map(record.tokens.clone()));
  value_to_json(&Value::Map(map)).to_string()
}

pub fn decode_record(text: &str) -> Option<SessionRecord> {
  let json: serde_json::Value = serde_json::from_str(text).ok()?;
  let Value::Map(mut map) = json_to_value(&json).ok()? else { return None };
  let data = match map.shift_remove("data") {
    Some(Value::Map(data)) => data,
    _ => ValueMap::new(),
  };
  let identity = match map.shift_remove("identity") {
    Some(Value::Map(identity)) => identity_of(&identity),
    _ => None,
  };
  let tokens = match map.shift_remove("tokens") {
    Some(Value::Map(tokens)) => tokens,
    _ => ValueMap::new(),
  };
  Some(SessionRecord { data, identity, tokens })
}

fn identity_map(identity: &Identity) -> ValueMap {
  let mut map = ValueMap::new();
  map.insert("subject".to_owned(), Value::Str(identity.subject.clone()));
  map.insert("claims".to_owned(), Value::Map(identity.claims.clone()));
  map
}

fn identity_of(map: &ValueMap) -> Option<Identity> {
  let subject = match map.get("subject") {
    Some(Value::Str(subject)) => subject.clone(),
    _ => return None,
  };
  let claims = match map.get("claims") {
    Some(Value::Map(claims)) => claims.clone(),
    _ => ValueMap::new(),
  };
  Some(Identity { subject, claims })
}

impl SessionStore for ServiceSessionStore {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let call = self.call("getSession", id_args(id));
    Box::pin(async move {
      match call.await {
        Ok(Value::Map(map)) => match map.get("record") {
          Some(Value::Str(text)) => decode_record(text),
          _ => None,
        },
        Ok(Value::Str(text)) => decode_record(&text),
        Ok(_) => None,
        Err(error) => {
          if error.kind != FailureKind::NotFound {
            log::warn!("session store: getSession failed: {error}");
          }
          None
        }
      }
    })
  }

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, ()> {
    let mut args = id_args(id);
    args.insert("record".to_owned(), Value::Str(encode_record(&record)));
    let call = self.call("putSession", args);
    Box::pin(async move {
      if let Err(error) = call.await {
        log::warn!("session store: putSession failed: {error}");
      }
    })
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, ()> {
    let call = self.call("deleteSession", id_args(id));
    Box::pin(async move {
      if let Err(error) = call.await {
        if error.kind != FailureKind::NotFound {
          log::warn!("session store: deleteSession failed: {error}");
        }
      }
    })
  }
}

/// Sends the login form's `user` and `password` to the client's `authenticate`
/// and takes `subject`, `claims` and `access_token` from the answer.
pub struct ServiceProvider {
  services: Arc<Services>,
  client: String,
  login_path: String,
}

impl ServiceProvider {
  pub fn new(services: Arc<Services>, client: impl Into<String>, login_path: impl Into<String>) -> Self {
    Self { services, client: client.into(), login_path: login_path.into() }
  }
}

fn param(params: &ValueMap, key: &str) -> Option<String> {
  match params.get(key) {
    Some(Value::Str(s)) => Some(s.clone()),
    _ => None,
  }
}

impl IdentityProvider for ServiceProvider {
  fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin> {
    let encoded: String = form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    let redirect = format!("{}?return_to={}", self.login_path, encoded);
    Box::pin(ready(Begin { redirect, state: ValueMap::new() }))
  }

  fn callback(&self, params: ValueMap, _state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>> {
    let mut args = ValueMap::new();
    let (user, password) = (param(&params, "user"), param(&params, "password"));
    let call = match (user, password) {
      (Some(user), Some(password)) => {
        args.insert("user".to_owned(), Value::Str(user));
        args.insert("password".to_owned(), Value::Str(password));
        Some(self.services.bind_anonymous().call(&self.client, "authenticate", args))
      }
      _ => None,
    };
    Box::pin(async move {
      let Some(call) = call else { return Err(AuthError::Invalid("missing user or password".to_owned())) };
      let answer = match call.await {
        Ok(Value::Map(answer)) => answer,
        Ok(other) => return Err(AuthError::Invalid(format!("authenticate answered {other:?}"))),
        Err(error) => {
          return Err(match error.kind {
            FailureKind::Unauthorized | FailureKind::NotFound | FailureKind::Invalid => AuthError::Denied(error.message.clone()),
            _ => AuthError::Invalid(error.to_string()),
          })
        }
      };
      let identity = identity_of(&answer).ok_or_else(|| AuthError::Invalid("authenticate answered without a subject".to_owned()))?;
      let mut tokens = ValueMap::new();
      if let Some(Value::Str(token)) = answer.get("access_token") {
        tokens.insert("access_token".to_owned(), Value::Str(token.clone()));
      }
      Ok(AuthOutcome { identity, tokens })
    })
  }
}
