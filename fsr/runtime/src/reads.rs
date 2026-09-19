use std::collections::HashMap;

use snapfire_fsr_core::PlanNode;

/// How much of the request a body or a subtree depends on: nothing, the
/// identity alone or more.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Static {
  Fixed,
  Anonymous,
  #[default]
  Dynamic,
}

/// What a subtree of a plan reads of the request: its class, the most any
/// node in it reads, the store keys any component in it reads and whether
/// any of them renders the path the request matched.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtreeReads {
  pub class: Static,
  pub store_keys: Vec<String>,
  /// A component in the subtree renders `ctx.path` or the document's path,
  /// which a `<Link>` does to mark itself the current page. The memo key
  /// carries both paths for such a subtree, since two routes of one shape
  /// would otherwise share a render whose marks name one of them.
  pub path: bool,
}

/// The prop the path the request matched rides in on, read by the renderer
/// rather than by the component: `Expr::Path` resolves through it, the way
/// `Expr::Locale` resolves through `locale`.
pub const PATH_PROP: &str = "$path";

/// The prop the document's path rides in on, beside `PATH_PROP`: the origin
/// of an intercepted navigation, else the path the request matched.
/// `Expr::Document` resolves through it.
pub const DOCUMENT_PROP: &str = "$document";

/// Subtree reads by the key `subtree_shape` gives their root: an app
/// computes one entry per plan node it can see through and the assembler
/// treats a node with no entry as `Dynamic`.
pub type Reads = HashMap<u64, SubtreeReads>;

/// The module names and slot names of a subtree in plan order, hashed. Two
/// subtrees of the same shape read the same things, since a module has one
/// loader beside it and one component.
pub fn subtree_shape(node: &PlanNode) -> u64 {
  fn walk(node: &PlanNode, h: &mut xxhash_rust::xxh3::Xxh3) {
    h.update(node.module.to_string().as_bytes());
    for (slot, child) in &node.children {
      h.update(slot.0.as_bytes());
      walk(child, h);
    }
  }
  let mut h = xxhash_rust::xxh3::Xxh3::new();
  walk(node, &mut h);
  h.digest()
}
