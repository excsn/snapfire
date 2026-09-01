use std::collections::HashMap;
use std::time::Duration;

use futures_util::future::{ready, BoxFuture};
use parking_lot::Mutex;
use snapfire_fsr_core::Node;

use crate::segments::SegmentInfo;

/// What a cache hit restores: the evaluated subtree plus its segment sidecar,
/// so navigation identity survives caching.
#[derive(Debug, Clone, PartialEq)]
pub struct CacheEntry {
  pub node: Node,
  pub segments: Vec<SegmentInfo>,
}

/// Memoizes evaluated subtrees. Keys are composed by the assembler from the
/// plan's `cache_key`, the matched params and the subtree's data fingerprint,
/// so a data change is a miss, never a stale hit. A hit skips evaluation
/// entirely: no chunk stream, no engine.
pub trait NodeCache: Send + Sync {
  fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>>;
  fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()>;
  /// Removes every entry whose plan `cache_key` matches. Tags are keys,
  /// revalidation is invalidation.
  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, ()>;
}

pub struct NoCache;

impl NodeCache for NoCache {
  fn get(&self, _key: &str) -> BoxFuture<'_, Option<CacheEntry>> {
    Box::pin(ready(None))
  }

  fn put(&self, _key: String, _entry: CacheEntry) -> BoxFuture<'_, ()> {
    Box::pin(ready(()))
  }

  fn invalidate(&self, _cache_key: &str) -> BoxFuture<'_, ()> {
    Box::pin(ready(()))
  }
}

#[derive(Default)]
pub struct MemoryCache {
  entries: Mutex<HashMap<String, CacheEntry>>,
}

impl MemoryCache {
  pub fn new() -> Self {
    Self::default()
  }
}

impl NodeCache for MemoryCache {
  fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>> {
    let hit = self.entries.lock().get(key).cloned();
    Box::pin(ready(hit))
  }

  fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()> {
    self.entries.lock().insert(key, entry);
    Box::pin(ready(()))
  }

  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, ()> {
    let prefix = format!("{cache_key}|");
    self.entries.lock().retain(|k, _| !k.starts_with(&prefix));
    Box::pin(ready(()))
  }
}

/// `fibre_cache`-backed implementation: sharded, TinyLFU-bounded, TTL-expiring.
/// A side index maps each plan `cache_key` to its composed keys so
/// invalidation stays exact without iterating the cache.
pub struct FibreCache {
  cache: fibre_cache::Cache<String, CacheEntry>,
  index: Mutex<HashMap<String, Vec<String>>>,
}

impl FibreCache {
  pub fn new(cache: fibre_cache::Cache<String, CacheEntry>) -> Self {
    Self { cache, index: Mutex::new(HashMap::new()) }
  }

  pub fn bounded(capacity: u64, ttl: Duration) -> Self {
    Self::new(bounded_cache(capacity, ttl, None))
  }

  /// `shards` is rounded up to the next power of two by `fibre_cache`, whose
  /// own default is derived from the CPU count. Capacity is accounted across
  /// all shards, so this trades lock contention against the fixed per-shard
  /// policy and timer structures, never against usable capacity.
  pub fn bounded_sharded(capacity: u64, ttl: Duration, shards: usize) -> Self {
    Self::new(bounded_cache(capacity, ttl, Some(shards)))
  }
}

fn bounded_cache(
  capacity: u64,
  ttl: Duration,
  shards: Option<usize>,
) -> fibre_cache::Cache<String, CacheEntry> {
  let mut builder = fibre_cache::CacheBuilder::default().capacity(capacity).time_to_live(ttl);
  if let Some(shards) = shards {
    builder = builder.shards(shards);
  }
  builder.build().expect("fibre_cache build")
}

fn plan_key_of(composed: &str) -> &str {
  composed.split('|').next().unwrap_or(composed)
}

impl NodeCache for FibreCache {
  fn get(&self, key: &str) -> BoxFuture<'_, Option<CacheEntry>> {
    let hit = self.cache.fetch(&key.to_owned()).map(|arc| (*arc).clone());
    Box::pin(ready(hit))
  }

  fn put(&self, key: String, entry: CacheEntry) -> BoxFuture<'_, ()> {
    self
      .index
      .lock()
      .entry(plan_key_of(&key).to_owned())
      .or_default()
      .push(key.clone());
    self.cache.insert(key, entry, 1);
    Box::pin(ready(()))
  }

  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, ()> {
    if let Some(keys) = self.index.lock().remove(cache_key) {
      for key in keys {
        self.cache.invalidate(&key);
      }
    }
    Box::pin(ready(()))
  }
}
