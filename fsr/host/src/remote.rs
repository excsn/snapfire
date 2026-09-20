//! A session store and an identity provider that live behind a client, so the
//! host holds neither accounts nor sessions in its own memory. The client's
//! contract declares `getSession`, `putSession` and `deleteSession` for the
//! store and `authenticate` for the provider; the record travels as one string
//! in the payload's JSON encoding, so the service stores an opaque blob.

use std::future::ready;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use snapfire_fsr_auth::{AuthError, AuthOutcome, Begin, IdentityProvider};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{unix_now, FailureKind, Identity, ServiceError};
use snapfire_fsr_service::Services;
use snapfire_fsr_session::{SessionId, SessionRecord, SessionStore, StoreError};

pub struct ServiceSessionStore {
  services: Arc<Services>,
  client: String,
  ttl: Duration,
}

impl ServiceSessionStore {
  /// `ttl` is the end given to a record the service stored before records
  /// carried one.
  pub fn new(services: Arc<Services>, client: impl Into<String>, ttl: Duration) -> Self {
    Self {
      services,
      client: client.into(),
      ttl,
    }
  }

  fn call(&self, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>> {
    self.services.bind_anonymous().call(&self.client, method, args)
  }
}

fn id_args(id: &SessionId) -> ValueMap {
  let mut args = ValueMap::default();
  args.insert("id".to_owned(), Value::str(id.0.clone()));
  args
}

/// The record as one JSON string in the payload encoding.
pub fn encode_record(record: &SessionRecord) -> String {
  let mut map = ValueMap::default();
  map.insert("data".to_owned(), Value::Map(record.data.clone()));
  map.insert(
    "identity".to_owned(),
    match &record.identity {
      Some(identity) => Value::Map(identity_map(identity)),
      None => Value::Null,
    },
  );
  map.insert("tokens".to_owned(), Value::Map(record.tokens.clone()));
  map.insert("csrf".to_owned(), Value::Map(record.csrf.clone()));
  map.insert("expires".to_owned(), Value::Int(record.expires as i128));
  value_to_json(&Value::Map(map)).to_string()
}

/// `expires_when_missing` is the end for a record encoded before records
/// carried one.
pub fn decode_record(text: &str, expires_when_missing: u64) -> Option<SessionRecord> {
  let json: serde_json::Value = serde_json::from_str(text).ok()?;
  let Value::Map(mut map) = json_to_value(&json).ok()? else {
    return None;
  };
  let data = match map.shift_remove("data") {
    Some(Value::Map(data)) => data,
    _ => ValueMap::default(),
  };
  let identity = match map.shift_remove("identity") {
    Some(Value::Map(identity)) => identity_of(&identity),
    _ => None,
  };
  let tokens = match map.shift_remove("tokens") {
    Some(Value::Map(tokens)) => tokens,
    _ => ValueMap::default(),
  };
  let csrf = match map.shift_remove("csrf") {
    Some(Value::Map(csrf)) => csrf,
    _ => ValueMap::default(),
  };
  let expires = match map.shift_remove("expires") {
    Some(Value::Int(at)) => u64::try_from(at).unwrap_or(0),
    Some(Value::UInt(at)) => u64::try_from(at).unwrap_or(u64::MAX),
    _ => expires_when_missing,
  };
  Some(SessionRecord { data, identity, tokens, csrf, expires })
}

fn identity_map(identity: &Identity) -> ValueMap {
  let mut map = ValueMap::default();
  map.insert("subject".to_owned(), Value::str(identity.subject.clone()));
  map.insert("claims".to_owned(), Value::Map(identity.claims.clone()));
  map
}

fn identity_of(map: &ValueMap) -> Option<Identity> {
  let subject = match map.get("subject") {
    Some(Value::Str(subject)) => subject.to_string(),
    _ => return None,
  };
  let claims = match map.get("claims") {
    Some(Value::Map(claims)) => claims.clone(),
    _ => ValueMap::default(),
  };
  Some(Identity { subject, claims })
}

impl SessionStore for ServiceSessionStore {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let call = self.call("getSession", id_args(id));
    let fallback = unix_now() + self.ttl.as_secs();
    Box::pin(async move {
      match call.await {
        Ok(Value::Map(map)) => match map.get("record") {
          Some(Value::Str(text)) => decode_record(text, fallback),
          _ => None,
        },
        Ok(Value::Str(text)) => decode_record(&text, fallback),
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

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>> {
    let mut args = id_args(id);
    args.insert("record".to_owned(), Value::str(encode_record(&record)));
    let call = self.call("putSession", args);
    Box::pin(async move {
      call.await.map(|_| ()).map_err(|error| StoreError::new(format!("putSession: {error}")))
    })
  }

  /// Deleting a record that is not there is what the caller asked for.
  fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>> {
    let call = self.call("deleteSession", id_args(id));
    Box::pin(async move {
      match call.await {
        Ok(_) => Ok(()),
        Err(error) if error.kind == FailureKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::new(format!("deleteSession: {error}"))),
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
    Self {
      services,
      client: client.into(),
      login_path: login_path.into(),
    }
  }
}

fn param(params: &ValueMap, key: &str) -> Option<String> {
  match params.get(key) {
    Some(Value::Str(s)) => Some(s.to_string()),
    _ => None,
  }
}

impl IdentityProvider for ServiceProvider {
  fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin> {
    let encoded: String = form_urlencoded::byte_serialize(return_to.as_bytes()).collect();
    let redirect = format!("{}?return_to={}", self.login_path, encoded);
    Box::pin(ready(Begin {
      redirect,
      state: ValueMap::default(),
    }))
  }

  fn callback(&self, params: ValueMap, _state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>> {
    let mut args = ValueMap::default();
    let (user, password) = (param(&params, "user"), param(&params, "password"));
    let call = match (user, password) {
      (Some(user), Some(password)) => {
        args.insert("user".to_owned(), Value::str(user));
        args.insert("password".to_owned(), Value::str(password));
        Some(self.services.bind_anonymous().call(&self.client, "authenticate", args))
      }
      _ => None,
    };
    Box::pin(async move {
      let Some(call) = call else {
        return Err(AuthError::Invalid("missing user or password".to_owned()));
      };
      let answer = match call.await {
        Ok(Value::Map(answer)) => answer,
        Ok(other) => return Err(AuthError::Invalid(format!("authenticate answered {other:?}"))),
        Err(error) => {
          return Err(match error.kind {
            FailureKind::Unauthorized | FailureKind::NotFound | FailureKind::Invalid => {
              AuthError::Denied(error.message.clone())
            }
            _ => AuthError::Invalid(error.to_string()),
          });
        }
      };
      let identity =
        identity_of(&answer).ok_or_else(|| AuthError::Invalid("authenticate answered without a subject".to_owned()))?;
      let mut tokens = ValueMap::default();
      if let Some(Value::Str(token)) = answer.get("access_token") {
        tokens.insert("access_token".to_owned(), Value::Str(token.clone()));
      }
      Ok(AuthOutcome { identity, tokens })
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_end_rides_in_the_record_and_a_record_from_before_it_gets_the_fallback() {
    let mut record = SessionRecord::default();
    record.data.insert("who".to_owned(), Value::str("alice"));
    record.expires = 1_800_000_000;
    let text = encode_record(&record);
    assert!(text.contains("\"expires\":1800000000"), "{text}");
    assert_eq!(decode_record(&text, 7).unwrap().expires, 1_800_000_000);

    let legacy = r#"{"data":{"who":"alice"},"identity":null,"tokens":{},"csrf":{}}"#;
    let decoded = decode_record(legacy, 7).unwrap();
    assert_eq!(decoded.expires, 7);
    assert_eq!(decoded.data.get("who"), Some(&Value::str("alice")));
  }
}
