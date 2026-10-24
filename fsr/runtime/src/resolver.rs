use std::collections::HashMap;

use snapfire_fsr_core::{Params, PlanNode};

use crate::matcher::EntryId;

pub trait Resolver: Send + Sync {
  fn resolve(&self, entry: EntryId, params: &Params) -> Option<PlanNode>;
}

/// The minimal resolver: a table from entry to a prebuilt plan. Layout
/// conventions, groups and interception belong to richer resolvers.
#[derive(Default)]
pub struct TableResolver {
  plans: HashMap<EntryId, PlanNode>,
}

impl TableResolver {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn insert(&mut self, entry: EntryId, plan: PlanNode) {
    self.plans.insert(entry, plan);
  }

  /// Every plan the table holds.
  pub fn plans(&self) -> impl Iterator<Item = &PlanNode> {
    self.plans.values()
  }

  /// The most deferred segments one document can stream, which is also the highest slot number one fills: a document numbers its slots from 1.
  pub fn most_deferred(&self) -> u32 {
    fn count(node: &PlanNode) -> u32 {
      u32::from(node.deferred) + node.children.iter().map(|(_, child)| count(child)).sum::<u32>()
    }
    self.plans.values().map(count).max().unwrap_or(0)
  }
}

impl Resolver for TableResolver {
  fn resolve(&self, entry: EntryId, _params: &Params) -> Option<PlanNode> {
    self.plans.get(&entry).cloned()
  }
}
