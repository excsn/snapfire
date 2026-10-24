use std::time::Duration;

use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_payload::{json_to_value, value_to_json};
use snapfire_fsr_runtime::{unix_now, Identity};

use crate::SessionId;

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct SessionRecord {
  pub data: ValueMap,
  pub identity: Option<Identity>,
  pub tokens: ValueMap,
  /// The CSRF scheme's state, read and written by the scheme alone.
  pub csrf: ValueMap,
  /// Seconds since the Unix epoch after which the record is gone: the cookie's
  /// `Max-Age` counts down to it, the layer treats a record past it as absent
  /// and a store may drop the record at it. A default record has already
  /// expired.
  pub expires: u64,
}

impl SessionRecord {
  pub fn data(mut self, data: ValueMap) -> Self {
    self.data = data;
    self
  }

  pub fn identity(mut self, identity: Option<Identity>) -> Self {
    self.identity = identity;
    self
  }

  pub fn tokens(mut self, tokens: ValueMap) -> Self {
    self.tokens = tokens;
    self
  }

  pub fn csrf(mut self, csrf: ValueMap) -> Self {
    self.csrf = csrf;
    self
  }

  pub fn expires(mut self, expires: u64) -> Self {
    self.expires = expires;
    self
  }

  /// The record as one JSON string in the payload encoding.
  pub fn encode(&self) -> String {
    let mut map = ValueMap::default();
    map.insert("data".to_owned(), Value::Map(self.data.clone()));
    map.insert(
      "identity".to_owned(),
      match &self.identity {
        Some(identity) => Value::Map(identity_map(identity)),
        None => Value::Null,
      },
    );
    map.insert("tokens".to_owned(), Value::Map(self.tokens.clone()));
    map.insert("csrf".to_owned(), Value::Map(self.csrf.clone()));
    map.insert("expires".to_owned(), Value::Int(self.expires as i128));
    value_to_json(&Value::Map(map)).to_string()
  }

  /// What [`encode`](Self::encode) wrote; `expires_when_missing` is the end for a record encoded
  /// before records carried one.
  pub fn decode(text: &str, expires_when_missing: u64) -> Option<Self> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let Value::Map(mut map) = json_to_value(&json).ok()? else {
      return None;
    };
    let mut take = |key: &str| match map.shift_remove(key) {
      Some(Value::Map(map)) => map,
      _ => ValueMap::default(),
    };
    let data = take("data");
    let tokens = take("tokens");
    let csrf = take("csrf");
    let identity = match map.shift_remove("identity") {
      Some(Value::Map(identity)) => identity_of(&identity),
      _ => None,
    };
    let expires = match map.shift_remove("expires") {
      Some(Value::Int(at)) => u64::try_from(at).unwrap_or(0),
      Some(Value::UInt(at)) => u64::try_from(at).unwrap_or(u64::MAX),
      _ => expires_when_missing,
    };
    Some(Self::default().data(data).identity(identity).tokens(tokens).csrf(csrf).expires(expires))
  }
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
  Some(Identity::new(subject, claims))
}

/// Why a store could not write. A read has no error: a record that cannot be
/// fetched is indistinguishable from one that was never there and both mean
/// the request carries on anonymous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreError(pub String);

impl StoreError {
  pub fn new(message: impl Into<String>) -> Self {
    Self(message.into())
  }
}

impl std::fmt::Display for StoreError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for StoreError {}

pub trait SessionStore: Send + Sync {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>>;
  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>>;
  fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>>;
}

pub struct MemorySessionStore {
  cache: fibre_cache::Cache<String, SessionRecord>,
}

impl MemorySessionStore {
  /// Every record lives until its own `expires`; the cache needs no policy of
  /// its own beyond the capacity.
  pub fn new(capacity: u64) -> Self {
    Self::with_cache(cache(capacity, None))
  }

  /// `shards` is rounded up to the next power of two by `fibre_cache`, whose
  /// own default is derived from the CPU count. Capacity is accounted across
  /// all shards, so this trades lock contention against the fixed per-shard
  /// policy and timer structures, never against usable capacity.
  pub fn sharded(capacity: u64, shards: usize) -> Self {
    Self::with_cache(cache(capacity, Some(shards)))
  }

  /// The escape hatch, for an eviction listener, a hasher or a timer preset
  /// the constructors above do not reach.
  pub fn with_cache(cache: fibre_cache::Cache<String, SessionRecord>) -> Self {
    Self { cache }
  }
}

fn cache(capacity: u64, shards: Option<usize>) -> fibre_cache::Cache<String, SessionRecord> {
  let mut builder = fibre_cache::CacheBuilder::default().capacity(capacity);
  if let Some(shards) = shards {
    builder = builder.shards(shards);
  }
  builder.build().expect("session cache build")
}

impl SessionStore for MemorySessionStore {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let record = self.cache.fetch(&id.0).map(|arc| (*arc).clone());
    Box::pin(ready(record))
  }

  /// A record already past its end is dropped rather than stored.
  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>> {
    let remaining = record.expires.saturating_sub(unix_now());
    if remaining == 0 {
      self.cache.invalidate(&id.0);
    } else {
      self.cache.insert_with_ttl(id.0.clone(), record, 1, Duration::from_secs(remaining));
    }
    Box::pin(ready(Ok(())))
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>> {
    self.cache.invalidate(&id.0);
    Box::pin(ready(Ok(())))
  }
}
