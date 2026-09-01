use std::borrow::Cow;

use crate::module_id::ModuleId;
use crate::value::Props;

/// Trusted markup. Serialized without escaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Html(pub String);

/// Allocated by the assembler, unique per response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SlotId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
  Text(Cow<'static, str>),
  Raw(Html),
  Seq(Vec<Node>),
  Client {
    module: ModuleId,
    props: Props,
    children: Vec<Node>,
    ssr: Option<Box<Node>>,
  },
  Pending {
    slot: SlotId,
    fallback: Box<Node>,
  },
}

impl Node {
  pub fn text(v: impl Into<Cow<'static, str>>) -> Self {
    Node::Text(v.into())
  }

  pub fn raw(v: impl Into<String>) -> Self {
    Node::Raw(Html(v.into()))
  }
}
