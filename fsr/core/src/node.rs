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
  /// Where a child segment goes inside an island's own markup: a layout's
  /// `children`. The assembler replaces it with the child's node; it never
  /// reaches a serializer.
  Slot(crate::plan::SlotName),
  /// Content written at `target` rather than where the node sits, such as a
  /// modal at the end of the body. Where it sits names the segment that owns
  /// it, which is how a navigation that replaces the segment removes it.
  Placed {
    target: Place,
    nodes: Vec<Node>,
  },
  /// Where the content placed at `Place::Named` of this name is written: inside the element whose id is the name, which the markup around it opens and closes.
  Outlet(String),
}

/// Where a `Node::Placed` writes its content.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Place {
  /// The end of the document's body.
  Body,
  /// The element a `Node::Outlet` of this name sits in, whose id is the name.
  Named(String),
}

impl Place {
  /// The spelling the payload and a lowerer use: `body` for the body and `#` followed by the name for a named place.
  pub fn spelling(&self) -> Cow<'_, str> {
    match self {
      Place::Body => Cow::Borrowed("body"),
      Place::Named(name) => Cow::Owned(format!("#{name}")),
    }
  }

  pub fn parse(target: &str) -> Option<Place> {
    match target {
      "body" => Some(Place::Body),
      _ => target.strip_prefix('#').filter(|name| is_place_name(name)).map(|name| Place::Named(name.to_owned())),
    }
  }
}

/// One or more ASCII letters, digits, `-` and `_`, so a name is written into an attribute and a selector as it stands.
pub fn is_place_name(name: &str) -> bool {
  !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Node {
  pub fn text(v: impl Into<Cow<'static, str>>) -> Self {
    Node::Text(v.into())
  }

  pub fn raw(v: impl Into<String>) -> Self {
    Node::Raw(Html(v.into()))
  }
}
