use snapfire_fsr_core::{Params, PlanNode};

/// Comparable segment identity for navigation. Same key across two responses
/// means the segment's DOM and island state survive; different key means the
/// region is replaced from that point down. Content changes are not identity
/// changes: revalidation is a separate mechanism.
pub trait SegmentKeyer: Send + Sync {
  fn key(&self, plan: &PlanNode, params: &Params, query: &Params) -> String;
}

/// Module plus every matched param and every query pair, since a loader may
/// read either. A custom resolver with narrower dependencies pairs with a
/// narrower keyer.
pub struct DefaultKeyer;

impl SegmentKeyer for DefaultKeyer {
  fn key(&self, plan: &PlanNode, params: &Params, query: &Params) -> String {
    let mut key = plan.module.to_string();
    let mut pairs: Vec<String> = params.iter().map(|(k, v)| format!("{k}={v}")).collect();
    pairs.sort_unstable();
    let mut query_pairs: Vec<String> = query.iter().map(|(k, v)| format!("{k}={v}")).collect();
    query_pairs.sort_unstable();
    pairs.extend(query_pairs);
    if !pairs.is_empty() {
      key.push('?');
      key.push_str(&pairs.join("&"));
    }
    key
  }
}

/// The sidecar the assembler emits beside the payload tree. `path` locates the
/// segment's subtree relative to its parent segment's node ([] means the whole
/// node, [i] means child i of a Seq). A deferred segment is slot-addressed
/// instead and carries no path.
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentInfo {
  pub key: String,
  /// The slot this segment fills in its parent; empty at the root.
  pub name: String,
  pub path: Vec<u32>,
  pub slot: Option<u32>,
  pub children: Vec<SegmentInfo>,
  /// Slots of this segment the payload leaves unfilled and the browser keeps
  /// as they stand.
  pub keep: Vec<String>,
}

impl SegmentInfo {
  pub fn keep_of(plan: &PlanNode) -> Vec<String> {
    plan.keep.iter().map(|k| k.0.clone()).collect()
  }
}
