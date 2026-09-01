use snapfire_fsr_core::{CacheKey, DataSourceId, ModuleId, NodeId, PlanNode, SlotName};

use crate::BindError;

/// A route's plan, written the way it reads. Node ids are assigned in tree
/// order at build time, so nothing here numbers anything.
pub struct Plan {
  module: String,
  source: Option<String>,
  deferred: bool,
  fallback: Option<String>,
  error: Option<String>,
  cache_key: Option<String>,
  children: Vec<(String, Plan)>,
}

impl Plan {
  pub fn of(module: impl Into<String>) -> Self {
    Self {
      module: module.into(),
      source: None,
      deferred: false,
      fallback: None,
      error: None,
      cache_key: None,
      children: Vec::new(),
    }
  }

  /// The data source that loads this node, named the way the plan file names it.
  pub fn source(mut self, name: impl Into<String>) -> Self {
    self.source = Some(name.into());
    self
  }

  /// Streams instead of blocking the first response. Pair it with `fallback`.
  pub fn deferred(mut self) -> Self {
    self.deferred = true;
    self
  }

  pub fn fallback(mut self, module: impl Into<String>) -> Self {
    self.fallback = Some(module.into());
    self
  }

  /// Rendered in place of this node when its loader fails.
  pub fn error(mut self, module: impl Into<String>) -> Self {
    self.error = Some(module.into());
    self
  }

  pub fn cache_key(mut self, key: impl Into<String>) -> Self {
    self.cache_key = Some(key.into());
    self
  }

  pub fn slot(mut self, name: impl Into<String>, child: Plan) -> Self {
    self.children.push((name.into(), child));
    self
  }

  fn assemble(self, next: &mut u32) -> Result<PlanNode, BindError> {
    let id = *next;
    *next += 1;

    let module = parse(&self.module)?;
    let mut node = PlanNode::new(NodeId(id), module);
    node.data_source = self.source.map(DataSourceId);
    node.deferred = self.deferred;
    node.cache_key = self.cache_key.map(CacheKey);
    node.fallback = self.fallback.as_deref().map(parse).transpose()?;
    node.error = self.error.as_deref().map(parse).transpose()?;

    for (slot, child) in self.children {
      node.children.push((SlotName(slot), child.assemble(next)?));
    }
    Ok(node)
  }
}

fn parse(module: &str) -> Result<ModuleId, BindError> {
  module.parse().map_err(|_| BindError::Module { module: module.to_owned() })
}

/// What a route accepts: the builder, or a `PlanNode` built by hand.
pub trait IntoPlan {
  fn into_plan(self) -> Result<PlanNode, BindError>;
}

impl IntoPlan for Plan {
  fn into_plan(self) -> Result<PlanNode, BindError> {
    let mut next = 0;
    self.assemble(&mut next)
  }
}

impl IntoPlan for PlanNode {
  fn into_plan(self) -> Result<PlanNode, BindError> {
    Ok(self)
  }
}
