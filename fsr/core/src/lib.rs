/// The largest side a card image may have, in pixels: what the build checks a declared size against and the renderer draws up to.
pub const CARD_MAX_SIDE: u32 = 8192;

pub mod duration;
pub mod ext;
pub mod json;
pub mod lock;
mod fingerprint;
pub mod module_id;
pub mod node;
pub mod plan;
pub mod time;
pub mod fields;
pub mod value;

pub use duration::parse_duration;
pub use fingerprint::Fingerprint;
pub use module_id::ModuleId;
pub use node::{is_place_name, Html, Node, Place, SlotId};
pub use plan::{CacheKey, DataSourceId, NodeId, PlanNode, SlotName};
pub use value::{collect_cycles, reaches, Collection, Fields, Items, Props, RefKind, Text, TypedArray, Value, ValueHasher, ValueMap, ValueSeq, ValueStr};

pub type Params = indexmap::IndexMap<String, String>;
pub type Data = value::ValueMap;
pub mod dynamic;
pub use dynamic::{Dynamic, DynamicError};

/// How much stack [`on_enough_stack`] leaves `f`. It has to cover the deepest a caller goes between two calls of it: an expression nested in the source, each level a frame of the interpreter, which is many times larger in a debug build.
const STACK_MARGIN: usize = if cfg!(debug_assertions) { 1 << 20 } else { 256 << 10 };

/// What a thread's stack grows by once less than [`STACK_MARGIN`] is left.
const STACK_GROWTH: usize = 8 * STACK_MARGIN;

/// `f` with room to recurse. How much stack it needs depends on the machine's frames and how much its thread was given, so it is read here and never assumed.
pub fn on_enough_stack<T>(f: impl FnOnce() -> T) -> T {
  // Miri cannot read the stack pointer and has no stack limit.
  if cfg!(miri) {
    return f();
  }
  stacker::maybe_grow(STACK_MARGIN, STACK_GROWTH, f)
}
