use std::cmp::Ordering;

use futures_util::future::BoxFuture;
use snapfire_fsr_core::Data;

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// What a segment seeds the browser's store with once its data is known.
/// Registered under the data source's id; every seeding segment of a route
/// contributes, an inner one winning a key an outer one also sets.
pub trait Seeds: Send + Sync {
  fn seed(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Data, LoadError>>;

  /// The keys the segment seeds as defaults: laid under every segment's seed in the merge and, in the browser, under a write an island holds, so a visitor's choice survives the route seeding again. Empty unless the seed has defaults.
  fn defaults(&self, _ctx: &RequestCtx, _data: &Data) -> BoxFuture<'static, Result<Data, LoadError>> {
    Box::pin(async { Ok(Data::default()) })
  }

  /// Every key a seed can hold, known before the data is: what a deferred segment promises the
  /// first wave. `None` when the keys depend on the data; such a segment promises nothing.
  fn keys(&self) -> Option<Vec<String>> {
    None
  }
}

/// What one segment seeded, placed by the slot names from the root down to
/// it, so the store is a merge by position whatever order the segments
/// arrive in: the browser holds one per segment and drops it with the segment.
#[derive(Debug, Clone, PartialEq)]
pub struct Contribution {
  /// The segment's key, the one its sidecar entry and its region carry.
  pub segment: String,
  pub path: Vec<String>,
  pub values: Data,
  /// The segment's defaults: under every segment's `values` in the merge and, in the browser, under a write an island holds.
  pub defaults: Data,
  /// The keys a deferred segment will seed once it resolves, on a promise, which carries no
  /// values; empty on a seed. The seed that lands for the segment replaces it.
  pub awaits: Vec<String>,
}

impl Contribution {
  pub fn seeded(segment: impl Into<String>, path: Vec<String>, values: Data) -> Self {
    Self { segment: segment.into(), path, values, defaults: Data::default(), awaits: Vec::new() }
  }

  pub fn with_defaults(mut self, defaults: Data) -> Self {
    self.defaults = defaults;
    self
  }

  pub fn promised(segment: impl Into<String>, path: Vec<String>, awaits: Vec<String>) -> Self {
    Self { segment: segment.into(), path, values: Data::default(), defaults: Data::default(), awaits }
  }
}

/// Where two contributions stand: a deeper one is later, so an inner segment
/// wins a key an outer one also sets; at one depth the slot names decide,
/// so two parallel slots settle a shared key the same way on every load.
/// `slot_order` is the application's say over that tie: a slot named in it
/// comes after every slot that is not, and the later name in it wins;
/// slots it leaves out stand in name order among themselves.
pub fn contribution_order(a: &[String], b: &[String], slot_order: &[String]) -> Ordering {
  a.len().cmp(&b.len()).then_with(|| {
    for (x, y) in a.iter().zip(b) {
      if x != y {
        return slot_rank(x, slot_order).cmp(&slot_rank(y, slot_order));
      }
    }
    Ordering::Equal
  })
}

fn slot_rank<'a>(name: &'a str, slot_order: &[String]) -> (Option<usize>, &'a str) {
  (slot_order.iter().position(|s| s == name), name)
}

/// The store `contributions` merge to: every segment's defaults in [`contribution_order`], then every segment's values in the same order, so a value wins a key a default also holds wherever each sits.
pub fn merge_contributions(contributions: &[Contribution], slot_order: &[String]) -> Data {
  let mut ordered: Vec<&Contribution> = contributions.iter().collect();
  ordered.sort_by(|a, b| contribution_order(&a.path, &b.path, slot_order));
  let mut out = Data::default();
  for contribution in &ordered {
    out.extend(contribution.defaults.clone());
  }
  for contribution in &ordered {
    out.extend(contribution.values.clone());
  }
  out
}

/// The keys of `contributions` whose value is not known yet: a promise awaits the key and outranks,
/// by [`contribution_order`], every seed that holds it. A promise whose segment has seeded is kept. A reader of one renders once the promise is
/// kept, since the value it would render now is about to be replaced.
pub fn pending_keys(contributions: &[Contribution], slot_order: &[String]) -> std::collections::BTreeSet<String> {
  let mut pending = std::collections::BTreeSet::new();
  for promise in contributions.iter().filter(|c| !c.awaits.is_empty()) {
    if contributions.iter().any(|c| c.segment == promise.segment && c.awaits.is_empty()) {
      continue;
    }
    for key in &promise.awaits {
      let outranked = contributions
        .iter()
        .filter(|c| c.segment != promise.segment && c.values.contains_key(key))
        .any(|seed| contribution_order(&seed.path, &promise.path, slot_order) != Ordering::Less);
      if !outranked {
        pending.insert(key.clone());
      }
    }
  }
  pending
}

/// What computes a derived key from the store it reads under the request:
/// the host's evaluation of the lowered body, which reads the request's locale. `None` when the body fails at run time, where the
/// browser's compute would throw; the key is then left out.
pub type Compute = std::sync::Arc<dyn Fn(&RequestCtx, &Data) -> Option<snapfire_fsr_core::Value> + Send + Sync>;

/// A store key the browser's `derive` computes from others, which the server
/// computes the same way so the first paint holds it. Without a compute, one
/// the build could not lower, the server never holds the key and treats it as
/// pending, so every reader is held rather than painted from a fallback.
#[derive(Clone)]
pub struct Derived {
  pub key: String,
  pub sources: Vec<String>,
  pub compute: Option<Compute>,
}

impl std::fmt::Debug for Derived {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Derived").field("key", &self.key).field("sources", &self.sources).field("computed", &self.compute.is_some()).finish()
  }
}

/// `derives` ordered so a key comes after every derived key it reads. A cycle,
/// which the build refuses, keeps its declaration order.
pub fn derive_order(derives: Vec<Derived>) -> Vec<Derived> {
  let mut pending = derives;
  let mut ordered: Vec<Derived> = Vec::with_capacity(pending.len());
  while !pending.is_empty() {
    let ready = pending.iter().position(|d| d.sources.iter().all(|source| !pending.iter().any(|other| other.key == *source && other.key != d.key))).unwrap_or(0);
    ordered.push(pending.remove(ready));
  }
  ordered
}

/// The merged `store` completed by `derives`, each in [`derive_order`]: a
/// derived value replaces what a segment seeded for the key, as it does in
/// the browser, where a derived value reads ahead of the merge.
pub fn apply_derives(ctx: &RequestCtx, store: &mut Data, derives: &[Derived]) {
  for derived in derives {
    match derived.compute.as_ref().and_then(|compute| compute(ctx, store)) {
      Some(value) => {
        store.insert(derived.key.clone(), value);
      }
      None => {
        store.shift_remove(&derived.key);
      }
    }
  }
}

/// `pending` with a derived key in it exactly when its value is not known yet:
/// it has no compute or a pending key feeds it. A promise seeding a derived
/// key does not hold it, since the derived value beats the seed.
pub fn derived_pending(pending: &mut std::collections::BTreeSet<String>, derives: &[Derived]) {
  for derived in derives {
    if derived.compute.is_none() || derived.sources.iter().any(|source| pending.contains(source)) {
      pending.insert(derived.key.clone());
    } else {
      pending.remove(&derived.key);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_core::Value;

  fn data(pairs: &[(&str, i64)]) -> Data {
    pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::Int(*v as i128))).collect()
  }

  #[test]
  fn a_default_sits_under_every_value_wherever_each_segment_stands() {
    let layout = Contribution::seeded("layout", vec![], data(&[("a", 1)])).with_defaults(data(&[("b", 10), ("c", 10)]));
    let page = Contribution::seeded("page", vec!["main".to_owned()], data(&[("b", 2)])).with_defaults(data(&[("a", 20), ("c", 20)]));
    let merged = merge_contributions(&[page, layout], &[]);
    assert_eq!(merged.get("a"), Some(&Value::Int(1)), "the layout's value wins over the page's default although the page is deeper");
    assert_eq!(merged.get("b"), Some(&Value::Int(2)), "the page's value wins over the layout's default");
    assert_eq!(merged.get("c"), Some(&Value::Int(20)), "two defaults merge by position, the deeper winning");
  }
}
