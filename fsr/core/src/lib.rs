pub mod duration;
mod fingerprint;
pub mod module_id;
pub mod node;
pub mod plan;
pub mod value;

pub use duration::parse_duration;
pub use fingerprint::Fingerprint;
pub use module_id::ModuleId;
pub use node::{Html, Node, SlotId};
pub use plan::{CacheKey, DataSourceId, NodeId, PlanNode, SlotName};
pub use value::{Props, RefKind, TypedArray, Value, ValueMap};

pub type Params = indexmap::IndexMap<String, String>;
pub type Data = value::ValueMap;
