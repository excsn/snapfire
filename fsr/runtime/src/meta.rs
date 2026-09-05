use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Data, Node};

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// What a route says about itself once its data is known: the document's
/// title and description. Absent fields leave the host's defaults in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
  pub title: Option<String>,
  pub description: Option<String>,
}

impl Meta {
  pub fn is_empty(&self) -> bool {
    self.title.is_none() && self.description.is_none()
  }
}

/// Computes a segment's `Meta` from the data its source loaded. Registered
/// under the data source's id; the assembler asks the innermost segment that
/// has one.
pub trait Metadata: Send + Sync {
  fn describe(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>>;
}

/// What the shell's head slot carries: the default title, an optional
/// default description and everything else the host puts in the head. The
/// assembler writes the title and description after `rest`, taking a
/// segment's `Meta` over the defaults.
#[derive(Debug, Clone, PartialEq)]
pub struct Head {
  pub title: String,
  pub description: Option<String>,
  pub rest: Node,
  /// A module the browser must load for this response's islands beyond the
  /// document's own entry, a mounted site's; the payload carries it as an `E` row.
  pub entry: Option<String>,
}

impl Head {
  pub fn new(title: impl Into<String>, rest: Node) -> Self {
    Self { title: title.into(), description: None, rest, entry: None }
  }

  /// The head node for a document: `rest`, then the title and description
  /// with `meta` overriding the defaults.
  pub fn node(&self, meta: &Meta) -> Node {
    let title = meta.title.as_deref().unwrap_or(&self.title);
    let description = meta.description.as_deref().or(self.description.as_deref());
    let mut tail = String::new();
    if !title.is_empty() {
      tail.push_str("<title>");
      tail.push_str(&escape(title));
      tail.push_str("</title>");
    }
    if let Some(description) = description {
      tail.push_str("<meta name=\"description\" content=\"");
      tail.push_str(&escape(description));
      tail.push_str("\">");
    }
    if tail.is_empty() {
      return self.rest.clone();
    }
    Node::Seq(vec![self.rest.clone(), Node::raw(tail)])
  }
}

impl From<Node> for Head {
  fn from(rest: Node) -> Self {
    Self { title: String::new(), description: None, rest, entry: None }
  }
}

impl From<&Node> for Head {
  fn from(rest: &Node) -> Self {
    Self::from(rest.clone())
  }
}

impl From<&Head> for Head {
  fn from(head: &Head) -> Self {
    head.clone()
  }
}

fn escape(text: &str) -> String {
  text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
