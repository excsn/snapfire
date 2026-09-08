//! A route's plan tree: a node, its optional parts and the slots beneath it.

use snapfire_fsr_ir::sexpr::Sx;

use super::shape::*;
use super::shape::Res;
use crate::{Child, Node};

pub(super) fn node_to_sx(node: &Node) -> Sx {
  let mut rest = vec![sym(node.id.to_string()), sym(node.module.clone())];
  rest.extend(opt_form("source", &node.source));
  if node.deferred {
    rest.push(form("deferred", vec![]));
  }
  rest.extend(opt_form("fallback", &node.fallback));
  rest.extend(opt_form("error", &node.error));
  rest.extend(opt_form("cache-key", &node.cache_key));
  if !node.keep.is_empty() {
    rest.push(form("keep", node.keep.iter().map(|k| sym(k.clone())).collect()));
  }
  for child in &node.children {
    rest.push(form("slot", vec![sym(child.slot.clone()), node_to_sx(&child.node)]));
  }
  form("node", rest)
}

pub(super) fn node_from_sx(sx: &Sx) -> Res<Node> {
  let items = as_list(sx)?;
  if head_of(sx)? != "node" {
    return Err(err("a plan node is `(node ...)`"));
  }
  if items.len() < 3 {
    return Err(err("a plan node needs an id and a module"));
  }
  let mut node = Node {
    id: as_sym(&items[1])?.parse().map_err(|_| err("a node id is a number"))?,
    module: as_sym(&items[2])?,
    source: None,
    deferred: false,
    fallback: None,
    error: None,
    cache_key: None,
    children: Vec::new(),
    keep: Vec::new(),
  };
  for section in &items[3..] {
    let inner = as_list(section)?;
    let one = || -> Res<String> {
      inner.get(1).ok_or_else(|| err("this node section needs a value")).and_then(as_sym)
    };
    match head_of(section)?.as_str() {
      "source" => node.source = Some(one()?),
      "deferred" => node.deferred = true,
      "fallback" => node.fallback = Some(one()?),
      "error" => node.error = Some(one()?),
      "cache-key" => node.cache_key = Some(one()?),
      "keep" => node.keep = inner[1..].iter().map(as_sym).collect::<Res<_>>()?,
      "slot" => {
        if inner.len() != 3 {
          return Err(err("a slot is `(slot name node)`"));
        }
        node.children.push(Child { slot: as_sym(&inner[1])?, node: node_from_sx(&inner[2])? });
      }
      other => return Err(err(format!("`{other}` is not a node section"))),
    }
  }
  Ok(node)
}
