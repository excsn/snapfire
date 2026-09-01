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
}

impl Resolver for TableResolver {
  fn resolve(&self, entry: EntryId, _params: &Params) -> Option<PlanNode> {
    self.plans.get(&entry).cloned()
  }
}
