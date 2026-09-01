use std::time::Duration;

use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_core::ValueMap;
use snapfire_fsr_runtime::Identity;

use crate::SessionId;

#[derive(Debug, Clone, Default)]
pub struct SessionRecord {
  pub data: ValueMap,
  pub identity: Option<Identity>,
}

pub trait SessionStore: Send + Sync {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>>;
  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, ()>;
  fn delete(&self, id: &SessionId) -> BoxFuture<'_, ()>;
}

pub struct MemorySessionStore {
  cache: fibre_cache::Cache<String, SessionRecord>,
}

impl MemorySessionStore {
  pub fn new(capacity: u64, ttl: Duration) -> Self {
    let cache = fibre_cache::CacheBuilder::default()
      .capacity(capacity)
      .time_to_idle(ttl)
      .build()
      .expect("session cache build");
    Self { cache }
  }
}

impl SessionStore for MemorySessionStore {
  fn load(&self, id: &SessionId) -> BoxFuture<'_, Option<SessionRecord>> {
    let record = self.cache.fetch(&id.0).map(|arc| (*arc).clone());
    Box::pin(ready(record))
  }

  fn save(&self, id: &SessionId, record: SessionRecord) -> BoxFuture<'_, ()> {
    self.cache.insert(id.0.clone(), record, 1);
    Box::pin(ready(()))
  }

  fn delete(&self, id: &SessionId) -> BoxFuture<'_, ()> {
    self.cache.invalidate(&id.0);
    Box::pin(ready(()))
  }
}
