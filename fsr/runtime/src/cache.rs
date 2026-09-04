use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use fibre_cache::{EvictionListener, EvictionReason};
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
  /// Removes every entry whose plan `cache_key` matches and says how many
  /// went. Tags are keys, revalidation is invalidation.
  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, usize>;
}

pub struct NoCache;

impl NodeCache for NoCache {
  fn get(&self, _key: &str) -> BoxFuture<'_, Option<CacheEntry>> {
    Box::pin(ready(None))
  }

  fn put(&self, _key: String, _entry: CacheEntry) -> BoxFuture<'_, ()> {
    Box::pin(ready(()))
  }

  fn invalidate(&self, _cache_key: &str) -> BoxFuture<'_, usize> {
    Box::pin(ready(0))
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

  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, usize> {
    let prefix = format!("{cache_key}|");
    let mut entries = self.entries.lock();
    let before = entries.len();
    entries.retain(|k, _| !k.starts_with(&prefix));
    Box::pin(ready(before - entries.len()))
  }
}

/// Each plan `cache_key` to the composed keys under it.
type Index = Arc<Mutex<HashMap<String, HashSet<String>>>>;

/// `fibre_cache`-backed implementation: sharded, TinyLFU-bounded, TTL-expiring.
/// A side index maps each plan `cache_key` to its composed keys so
/// invalidation stays exact without iterating the cache. The index follows
/// the cache: a key it drops, on TTL or for room, leaves the index through the
/// eviction listener `bounded` and `bounded_sharded` install.
pub struct FibreCache {
  cache: fibre_cache::Cache<String, CacheEntry>,
  index: Index,
}

/// Keeps the side index in step with what the cache still holds.
struct IndexListener(Index);

impl EvictionListener<String, CacheEntry> for IndexListener {
  fn on_evict(&self, key: String, _value: Arc<CacheEntry>, _reason: EvictionReason) {
    let plan_key = plan_key_of(&key).to_owned();
    let mut index = self.0.lock();
    if let Some(keys) = index.get_mut(&plan_key) {
      keys.remove(&key);
      if keys.is_empty() {
        index.remove(&plan_key);
      }
    }
  }
}

impl FibreCache {
  /// Over a cache built elsewhere. With no listener on it, a key the cache
  /// drops on its own stays in the index until its plan key is invalidated;
  /// register `FibreCache::listener` on the builder to keep the two in step.
  pub fn new(cache: fibre_cache::Cache<String, CacheEntry>) -> Self {
    Self { cache, index: Index::default() }
  }

  /// `new` over a cache whose builder carried `listener`.
  pub fn with_index(cache: fibre_cache::Cache<String, CacheEntry>, index: Index) -> Self {
    Self { cache, index }
  }

  /// An index and the listener that keeps it exact, for a cache the caller
  /// builds: `CacheBuilder::eviction_listener(listener)`, then `with_index`.
  pub fn listener() -> (Index, impl EvictionListener<String, CacheEntry> + 'static) {
    let index = Index::default();
    (index.clone(), IndexListener(index))
  }

  /// Four shards, opportunistic maintenance on every insert and a timer tick
  /// of a hundredth of the TTL between 10 ms and 1 s: a node cache is written
  /// on a miss and read on every request, so contention is low and an entry
  /// should leave close to its TTL rather than at the next coarse tick.
  pub fn bounded(capacity: u64, ttl: Duration) -> Self {
    bounded_cache(capacity, ttl, Some(SHARDS))
  }

  /// `shards` is rounded up to the next power of two by `fibre_cache`, whose
  /// own default is derived from the CPU count. Capacity is accounted across
  /// all shards, so this trades lock contention against the fixed per-shard
  /// policy and timer structures, never against usable capacity.
  pub fn bounded_sharded(capacity: u64, ttl: Duration, shards: usize) -> Self {
    bounded_cache(capacity, ttl, Some(shards))
  }

  /// How many composed keys the index holds under `plan_key`.
  pub fn indexed(&self, plan_key: &str) -> usize {
    self.index.lock().get(plan_key).map_or(0, HashSet::len)
  }
}

const SHARDS: usize = 4;
const TICK_MIN: Duration = Duration::from_millis(10);
const TICK_MAX: Duration = Duration::from_secs(1);
/// `maintenance_chance` is `1 / n` per insert; 1 is every insert.
const MAINTENANCE_EVERY_INSERT: u32 = 1;

/// A hundredth of the TTL, clamped, so expiry lands within a percent of when it was asked for.
fn timer_tick(ttl: Duration) -> Duration {
  (ttl / 100).clamp(TICK_MIN, TICK_MAX)
}

fn bounded_cache(capacity: u64, ttl: Duration, shards: Option<usize>) -> FibreCache {
  let (index, listener) = FibreCache::listener();
  let mut builder = fibre_cache::CacheBuilder::default()
    .capacity(capacity)
    .time_to_live(ttl)
    .timer_tick_duration(timer_tick(ttl))
    .maintenance_chance(MAINTENANCE_EVERY_INSERT)
    .eviction_listener(listener);
  if let Some(shards) = shards {
    builder = builder.shards(shards);
  }
  FibreCache::with_index(builder.build().expect("fibre_cache build"), index)
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
    self.index.lock().entry(plan_key_of(&key).to_owned()).or_default().insert(key.clone());
    self.cache.insert(key, entry, 1);
    Box::pin(ready(()))
  }

  fn invalidate(&self, cache_key: &str) -> BoxFuture<'_, usize> {
    let keys = self.index.lock().remove(cache_key).unwrap_or_default();
    for key in &keys {
      self.cache.invalidate(key);
    }
    Box::pin(ready(keys.len()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::executor::block_on;

  fn entry() -> CacheEntry {
    CacheEntry { node: Node::raw("x"), segments: Vec::new() }
  }

  #[test]
  fn a_key_put_again_is_indexed_once() {
    let (index, listener) = FibreCache::listener();
    let inner = fibre_cache::CacheBuilder::default().capacity(100).time_to_live(Duration::from_secs(60)).eviction_listener(listener).build().expect("fibre_cache build");
    let cache = FibreCache::with_index(inner, index);
    for _ in 0..3 {
      block_on(cache.put("plan|a|f1".to_owned(), entry()));
    }
    block_on(cache.put("plan|b|f1".to_owned(), entry()));
    assert_eq!(cache.indexed("plan"), 2);
    block_on(cache.invalidate("plan"));
    assert_eq!(cache.indexed("plan"), 0);
    assert!(block_on(cache.get("plan|a|f1")).is_none());
  }

  #[test]
  fn the_tick_follows_the_ttl() {
    assert_eq!(timer_tick(Duration::from_millis(50)), Duration::from_millis(10));
    assert_eq!(timer_tick(Duration::from_secs(30)), Duration::from_millis(300));
    assert_eq!(timer_tick(Duration::from_secs(3600)), Duration::from_secs(1));
  }

  #[test]
  fn a_key_the_cache_expires_leaves_the_index() {
    let cache = FibreCache::bounded(100, Duration::from_millis(50));
    block_on(cache.put("plan|a|f1".to_owned(), entry()));
    assert_eq!(cache.indexed("plan"), 1);
    std::thread::sleep(Duration::from_millis(200));
    cache.cache.run_maintenance();
    assert!(block_on(cache.get("plan|a|f1")).is_none(), "the entry expired");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while cache.indexed("plan") != 0 && std::time::Instant::now() < deadline {
      std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(cache.indexed("plan"), 0, "the listener trimmed the index");
  }
}
