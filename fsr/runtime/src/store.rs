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
