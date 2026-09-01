use crate::module_id::ModuleId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SlotName(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DataSourceId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(pub String);

#[derive(Debug, Clone, PartialEq)]
pub struct PlanNode {
  pub id: NodeId,
  pub module: ModuleId,
  pub data_source: Option<DataSourceId>,
  pub deferred: bool,
  /// The segment's loading module, rendered from params alone.
  pub fallback: Option<ModuleId>,
  pub cache_key: Option<CacheKey>,
  pub children: Vec<(SlotName, PlanNode)>,
}

impl PlanNode {
  pub fn new(id: NodeId, module: ModuleId) -> Self {
    Self {
      id,
      module,
      data_source: None,
      deferred: false,
      fallback: None,
      cache_key: None,
      children: Vec::new(),
    }
  }
}
