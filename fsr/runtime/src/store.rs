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
  /// The keys a deferred segment will seed once it resolves, on a promise, which carries no
  /// values; empty on a seed. The seed that lands for the segment replaces it.
  pub awaits: Vec<String>,
}

impl Contribution {
  pub fn seeded(segment: impl Into<String>, path: Vec<String>, values: Data) -> Self {
    Self { segment: segment.into(), path, values, awaits: Vec::new() }
  }

  pub fn promised(segment: impl Into<String>, path: Vec<String>, awaits: Vec<String>) -> Self {
    Self { segment: segment.into(), path, values: Data::default(), awaits }
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

/// The store `contributions` merge to, each applied in [`contribution_order`].
pub fn merge_contributions(contributions: &[Contribution], slot_order: &[String]) -> Data {
  let mut ordered: Vec<&Contribution> = contributions.iter().collect();
  ordered.sort_by(|a, b| contribution_order(&a.path, &b.path, slot_order));
  let mut out = Data::default();
  for contribution in ordered {
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
