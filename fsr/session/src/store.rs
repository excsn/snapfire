use std::time::Duration;

use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_runtime::Identity;

use crate::SessionId;

#[derive(Debug, Clone, Default)]
pub struct SessionRecord {
  pub data: ValueMap,
  pub identity: Option<Identity>,
  pub tokens: ValueMap,
}

/// Why a store could not write. A read has no error: a record that cannot be
/// fetched is indistinguishable from one that was never there, and both mean
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
  pub fn new(capacity: u64, ttl: Duration) -> Self {
    Self::with_cache(idle_cache(capacity, ttl, None))
  }

  /// `shards` is rounded up to the next power of two by `fibre_cache`, whose
  /// own default is derived from the CPU count. Capacity is accounted across
  /// all shards, so this trades lock contention against the fixed per-shard
  /// policy and timer structures, never against usable capacity.
  pub fn sharded(capacity: u64, ttl: Duration, shards: usize) -> Self {
    Self::with_cache(idle_cache(capacity, ttl, Some(shards)))
  }

  /// The escape hatch, for an eviction listener, a hasher or a timer preset
  /// the constructors above do not reach.
  pub fn with_cache(cache: fibre_cache::Cache<String, SessionRecord>) -> Self {
    Self { cache }
  }
}

fn idle_cache(
  capacity: u64,
  ttl: Duration,
  shards: Option<usize>,
) -> fibre_cache::Cache<String, SessionRecord> {
  let mut builder = fibre_cache::CacheBuilder::default().capacity(capacity).time_to_idle(ttl);
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

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, Result<(), StoreError>> {
    self.cache.insert(id.0.clone(), record, 1);
    Box::pin(ready(Ok(())))
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, Result<(), StoreError>> {
    self.cache.invalidate(&id.0);
    Box::pin(ready(Ok(())))
  }
}
