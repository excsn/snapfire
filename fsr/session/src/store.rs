use std::time::Duration;

use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_runtime::{unix_now, Identity};

use crate::SessionId;

#[derive(Debug, Clone, Default)]
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
