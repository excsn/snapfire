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
/// node in it reads, and the store keys any component in it reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtreeReads {
  pub class: Static,
  pub store_keys: Vec<String>,
}

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
