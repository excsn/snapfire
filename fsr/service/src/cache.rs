//! The data cache: a method whose contract declares `cache` is answered from
//! memory for its `ttl`, one `fibre_cache` per distinct policy so the lifetime
//! and the stale window are native. Tag generations are folded into the key,
//! so a method that `writes` a tag bumps its generation and every entry under
//! it becomes unreachable without an index. A miss runs the caller's own
//! call, credentials and all; only the background refresh of a `stale` window
//! runs anonymously, which is why `stale` needs `shared` scope. A failure is
//! never served: a failed refresh keeps the last answer.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use fibre_cache::{AsyncCache, CacheBuilder};
use futures_util::future::BoxFuture;
use indexmap::IndexMap;
use parking_lot::RwLock;
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

use crate::call::{Call, NoCredentials};
use crate::contract::{Contract, Freshness, Scope};
use crate::interceptor::{Chain, Interceptor, Next};

#[derive(Debug, thiserror::Error)]
pub enum DataCacheError {
  #[error("{method}: cache.ttl `{ttl}` is not a duration")]
  Ttl { method: String, ttl: String },
  #[error("{method}: cache.stale `{stale}` is not a duration")]
  Stale { method: String, stale: String },
  #[error("{method}: cache.stale needs scope = \"shared\", since a background refresh carries no identity")]
  StaleScope { method: String },
  #[error("{method}: {error}")]
  Build { method: String, error: String },
}

/// Service, method, the arguments and, under `subject` scope, who asked; the
/// tag generations at the time of the call are part of `canonical`.
#[derive(Clone, Debug)]
pub struct CallKey {
  pub service: String,
  pub method: String,
  pub args: ValueMap,
  canonical: String,
}

impl PartialEq for CallKey {
  fn eq(&self, other: &Self) -> bool {
    self.canonical == other.canonical
  }
}

impl Eq for CallKey {}

impl Hash for CallKey {
  fn hash<H: Hasher>(&self, state: &mut H) {
    self.canonical.hash(state);
  }
}

/// A deterministic rendering of a value: maps by sorted key, so two calls
/// with the same arguments in another order share an entry.
pub fn canonical(value: &Value, out: &mut String) {
  match value {
    Value::Null => out.push_str("null"),
    Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
    Value::Int(n) => out.push_str(&n.to_string()),
    Value::UInt(n) => out.push_str(&n.to_string()),
    Value::F32(f) => out.push_str(&f.to_string()),
    Value::F64(f) => out.push_str(&f.to_string()),
    Value::Str(s) => {
      out.push('"');
      out.push_str(&s.replace('\\', "\\\\").replace('"', "\\\""));
      out.push('"');
    }
    Value::Bytes(b) => {
      out.push_str("b:");
      for byte in b {
        out.push_str(&format!("{byte:02x}"));
      }
    }
    Value::TypedArray(a) => out.push_str(&format!("{a:?}")),
    Value::Seq(items) => {
      out.push('[');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        canonical(item, out);
      }
      out.push(']');
    }
    Value::Map(map) => {
      let mut keys: Vec<&String> = map.keys().collect();
      keys.sort();
      out.push('{');
      for (i, key) in keys.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        out.push_str(key);
        out.push(':');
        canonical(&map[*key], out);
      }
      out.push('}');
    }
    Value::Variant { tag, payload } => {
      out.push_str(tag);
      if let Some(payload) = payload {
        out.push('(');
        canonical(payload, out);
        out.push(')');
      }
    }
    Value::Ref { kind, id } => out.push_str(&format!("{kind:?}:{id}")),
  }
}

/// What a load left: the answer, or the failure the entry is dropped for.
pub struct Loaded(pub Result<Value, ServiceError>);

/// A cache is built on its first use rather than with the registry, since
/// `fibre_cache` binds its background tasks to the runtime that is current
/// when it is built and a host is built before any runtime runs.
struct Policy {
  freshness: Freshness,
  ttl: Duration,
  path: String,
  build: Box<dyn Fn() -> Result<AsyncCache<CallKey, Loaded>, DataCacheError> + Send + Sync>,
  cache: OnceLock<Option<AsyncCache<CallKey, Loaded>>>,
}

impl Policy {
  fn cache(&self) -> Option<&AsyncCache<CallKey, Loaded>> {
    self
      .cache
      .get_or_init(|| match (self.build)() {
        Ok(cache) => Some(cache),
        Err(error) => {
          tracing::warn!("data cache: {} runs uncached: {error}", self.path);
          None
        }
      })
      .as_ref()
  }
}

/// The rest of the chain after the cache, so a refresh can run a call the
/// cache builds itself.
pub(crate) struct Continuation {
  pub(crate) chains: IndexMap<String, Arc<Chain>>,
  pub(crate) default_chain: Option<Arc<Chain>>,
  pub(crate) index: usize,
}

impl Continuation {
  fn run(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let chain = self.chains.get(&call.service).or(self.default_chain.as_ref());
    match chain {
      Some(chain) => Next::at(chain.clone(), self.index).run(call),
      None => {
        let error = ServiceError::new(FailureKind::Unavailable, call.service, call.method, "no transport is bound for this service");
        Box::pin(async move { Err(error) })
      }
    }
  }
}

struct Inner {
  policies: OnceLock<HashMap<String, Arc<Policy>>>,
  writers: OnceLock<HashMap<String, Vec<String>>>,
  generations: RwLock<HashMap<String, u64>>,
  continuation: OnceLock<Continuation>,
  hits: AtomicU64,
  misses: AtomicU64,
}

/// The interceptor. `Clone` shares the caches, so the registry keeps one
/// handle for `invalidate_tags` and the chains keep another.
#[derive(Clone)]
pub struct DataCache {
  inner: Arc<Inner>,
}

impl DataCache {
  /// One cache per distinct `(ttl, stale)` over every method whose contract
  /// declares `cache`; every cache is bounded by `capacity` entries.
  pub fn from_contract(contract: &Contract, capacity: u64) -> Result<Self, DataCacheError> {
    let cache = Self {
      inner: Arc::new(Inner {
        policies: OnceLock::new(),
        writers: OnceLock::new(),
        generations: RwLock::new(HashMap::new()),
        continuation: OnceLock::new(),
        hits: AtomicU64::new(0),
        misses: AtomicU64::new(0),
      }),
    };
    let mut policies = HashMap::new();
    let mut writers = HashMap::new();
    for (service, def) in &contract.services {
      for (method, m) in &def.methods {
        let path = format!("{service}.{method}");
        if !m.writes.is_empty() {
          writers.insert(path.clone(), m.writes.clone());
        }
        let Some(freshness) = &m.cache else { continue };
        let ttl = snapfire_fsr_core::parse_duration(&freshness.ttl).ok_or_else(|| DataCacheError::Ttl { method: path.clone(), ttl: freshness.ttl.clone() })?;
        let stale = match &freshness.stale {
          Some(text) => {
            if freshness.scope != Scope::Shared {
              return Err(DataCacheError::StaleScope { method: path.clone() });
            }
            Some(snapfire_fsr_core::parse_duration(text).ok_or_else(|| DataCacheError::Stale { method: path.clone(), stale: text.clone() })?)
          }
          None => None,
        };
        let loader_of = cache.clone();
        let build_path = path.clone();
        let build = move || {
          let loader_of = loader_of.clone();
          let mut builder = CacheBuilder::default().capacity(capacity).time_to_live(ttl).async_loader(move |key: CallKey| {
          let cache = loader_of.clone();
          async move {
            let path = format!("{}.{}", key.service, key.method);
            let call = Call { service: key.service.clone(), method: key.method.clone(), args: key.args.clone(), identity: None, metadata: ValueMap::new(), credentials: Arc::new(NoCredentials) };
            let result = match cache.inner.continuation.get() {
              Some(continuation) => continuation.run(call).await,
              None => Err(ServiceError::new(FailureKind::Unavailable, key.service.clone(), key.method.clone(), "the data cache is not attached to a registry")),
            };
            if let Err(error) = &result {
              let held = match cache.inner.policies.get().and_then(|p| p.get(&path)).and_then(|p| p.cache()) {
                Some(cache) => cache.peek(&key).await,
                None => None,
              };
              if let Some(old) = held {
                if old.0.is_ok() {
                  tracing::warn!("data cache: refresh of {path} failed, keeping the last answer: {error}");
                  return (Loaded(old.0.clone()), 1);
                }
              }
            }
            (Loaded(result), 1)
          }
          });
          if let Some(stale) = stale {
            builder = builder.stale_while_revalidate(stale);
          }
          builder.build_async().map_err(|e| DataCacheError::Build { method: build_path.clone(), error: e.to_string() })
        };
        policies.insert(path.clone(), Arc::new(Policy { freshness: freshness.clone(), ttl, path, build: Box::new(build), cache: OnceLock::new() }));
      }
    }
    let _ = cache.inner.policies.set(policies);
    let _ = cache.inner.writers.set(writers);
    Ok(cache)
  }

  /// No method declares `cache` and none `writes`.
  pub fn is_empty(&self) -> bool {
    self.inner.policies.get().is_none_or(HashMap::is_empty) && self.inner.writers.get().is_none_or(HashMap::is_empty)
  }

  pub(crate) fn attach(&self, continuation: Continuation) {
    let _ = self.inner.continuation.set(continuation);
  }

  /// Every cached method with its policy, for a report.
  pub fn policies(&self) -> Vec<(String, Freshness)> {
    let mut rows: Vec<(String, Freshness)> = self.inner.policies.get().into_iter().flatten().map(|(path, p)| (path.clone(), p.freshness.clone())).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
  }

  /// Every method that writes tags, with them.
  pub fn writers(&self) -> Vec<(String, Vec<String>)> {
    let mut rows: Vec<(String, Vec<String>)> = self.inner.writers.get().into_iter().flatten().map(|(path, tags)| (path.clone(), tags.clone())).collect();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    rows
  }

  pub fn hits(&self) -> u64 {
    self.inner.hits.load(Ordering::Relaxed)
  }

  pub fn misses(&self) -> u64 {
    self.inner.misses.load(Ordering::Relaxed)
  }

  /// Drops every entry under the named tags by moving their generation on.
  pub fn invalidate_tags<I, S>(&self, tags: I)
  where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
  {
    let mut generations = self.inner.generations.write();
    for tag in tags {
      *generations.entry(tag.as_ref().to_owned()).or_insert(0) += 1;
    }
  }

  fn key_for(&self, call: &Call, policy: &Policy, subject: Option<&str>) -> CallKey {
    let mut canonical = format!("{}.{}|{}|", call.service, call.method, subject.unwrap_or(""));
    {
      let generations = self.inner.generations.read();
      for tag in &policy.freshness.tags {
        canonical.push_str(tag);
        canonical.push('=');
        canonical.push_str(&generations.get(tag).copied().unwrap_or(0).to_string());
        canonical.push(';');
      }
    }
    canonical.push('|');
    self::canonical(&Value::Map(call.args.clone()), &mut canonical);
    CallKey { service: call.service.clone(), method: call.method.clone(), args: call.args.clone(), canonical }
  }
}

impl Interceptor for DataCache {
  fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let path = format!("{}.{}", call.service, call.method);
    if let Some(tags) = self.inner.writers.get().and_then(|w| w.get(&path)).cloned() {
      let cache = self.clone();
      let run = next.run(call);
      return Box::pin(async move {
        let result = run.await;
        if result.is_ok() {
          cache.invalidate_tags(&tags);
        }
        result
      });
    }
    let Some(policy) = self.inner.policies.get().and_then(|p| p.get(&path)).cloned() else { return next.run(call) };
    let subject = match policy.freshness.scope {
      Scope::Private => {
        if call.identity.is_some() {
          return next.run(call);
        }
        None
      }
      Scope::Shared => None,
      Scope::Subject => call.identity.as_ref().map(|i| i.subject.clone()),
    };
    let key = self.key_for(&call, &policy, subject.as_deref());
    let inner = self.inner.clone();
    Box::pin(async move {
      let Some(cache) = policy.cache() else { return next.run(call).await };
      if let Some(loaded) = cache.fetch(&key).await {
        if let Ok(value) = &loaded.0 {
          inner.hits.fetch_add(1, Ordering::Relaxed);
          if policy.freshness.stale.is_some() {
            let _ = cache.fetch_with(&key).await;
          }
          return Ok(value.clone());
        }
      }
      inner.misses.fetch_add(1, Ordering::Relaxed);
      let result = next.run(call).await;
      if let Ok(value) = &result {
        cache.insert_with_ttl(key, Loaded(Ok(value.clone())), 1, policy.ttl).await;
      }
      result
    })
  }
}

impl std::fmt::Debug for DataCache {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let policies = self.inner.policies.get().map_or(0, HashMap::len);
    let writers = self.inner.writers.get().map_or(0, HashMap::len);
    f.debug_struct("DataCache").field("policies", &policies).field("writers", &writers).finish()
  }
}
