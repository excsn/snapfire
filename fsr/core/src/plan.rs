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
  /// The segment's error module, rendered with params plus the failure
  /// message when this segment's data source fails. Absent means the
  /// built-in error node.
  pub error: Option<ModuleId>,
  pub cache_key: Option<CacheKey>,
  pub children: Vec<(SlotName, PlanNode)>,
  /// Slots this node leaves unfilled that the browser keeps as they stand
  /// when the payload arrives: an intercepted route's plan names the page
  /// under the layout here, so a modal opens over it.
  pub keep: Vec<SlotName>,
}

impl PlanNode {
  pub fn new(id: NodeId, module: ModuleId) -> Self {
    Self {
      id,
      module,
      data_source: None,
      deferred: false,
      fallback: None,
      error: None,
      cache_key: None,
      children: Vec::new(),
      keep: Vec::new(),
    }
  }
}
