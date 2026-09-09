use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Data, Node};

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// One element a segment puts in the head: its tag, its attributes in the
/// order written and, for a tag that takes content, its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadEl {
  pub tag: String,
  pub attrs: Vec<(String, String)>,
  pub children: Option<String>,
}

impl HeadEl {
  /// What makes two entries the same element, so an inner segment replaces an
  /// outer one rather than emitting both: the naming attribute a head element
  /// is identified by in practice, or every attribute when it has none.
  /// `sizes` and `media` qualify it, since a document carries several icons
  /// under one `rel` and several stylesheets under one `media`.
  pub fn identity(&self) -> (String, String) {
    let of = |name: &str| self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    for name in ["rel", "name", "property", "http-equiv", "itemprop", "id"] {
      if let Some(value) = of(name) {
        let qualifier: String = ["sizes", "media"]
          .iter()
          .filter_map(|q| of(q).map(|v| format!(" {q}={v}")))
          .collect();
        return (format!("{}[{name}]", self.tag), format!("{value}{qualifier}"));
      }
    }
    let mut all: Vec<String> = self.attrs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    all.sort();
    (self.tag.clone(), all.join("&"))
  }

  pub fn render(&self, out: &mut String) {
    out.push('<');
    out.push_str(&self.tag);
    for (name, value) in &self.attrs {
      out.push(' ');
      out.push_str(name);
      out.push_str("=\"");
      out.push_str(&escape(value));
      out.push('"');
    }
    out.push('>');
    if let Some(children) = &self.children {
      out.push_str(children);
      out.push_str("</");
      out.push_str(&self.tag);
      out.push('>');
    }
  }
}

/// What a route says about itself once its data is known: the document's
/// title, its description and the head elements it contributes. Absent fields
/// leave the host's defaults in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Meta {
  pub title: Option<String>,
  pub description: Option<String>,
  pub head: Vec<HeadEl>,
}

impl Meta {
  pub fn is_empty(&self) -> bool {
    self.title.is_none() && self.description.is_none() && self.head.is_empty()
  }

  /// Folds an inner segment over this one: a title or description it sets
  /// wins, and each of its head elements replaces the one of the same
  /// identity, in place, or is appended when nothing matches. Outermost is
  /// folded first, so the innermost segment has the last word.
  pub fn merge(&mut self, inner: Meta) {
    if inner.title.is_some() {
      self.title = inner.title;
    }
    if inner.description.is_some() {
      self.description = inner.description;
    }
    for element in inner.head {
      match self.head.iter().position(|held| held.identity() == element.identity()) {
        Some(at) => self.head[at] = element,
        None => self.head.push(element),
      }
    }
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
  /// Stylesheets this response's routes need beyond the document's own, a
  /// mounted site's; the payload carries them as a `C` row and a document
  /// renders them after everything else, so a site's rules win over the
  /// shell's. A navigation into a site adds them and one out takes them away,
  /// which is why they are a field rather than markup inside `rest`.
  pub styles: Vec<String>,
  /// The message catalog for this response's locale as JSON, when the
  /// browser needs it; the payload carries it as a `D` row.
  pub catalog: Option<String>,
  /// The head elements every document carries, from `[document.head]` and
  /// what the host inferred. A segment's `meta` folds over these.
  pub head: Vec<HeadEl>,
  /// `scheme://host` for this deployment, `[document] origin`. A crawler reads
  /// `rel=canonical` and `rel=alternate` as absolute URLs only, so a path on
  /// either is prefixed with this. Absent, an href is written as given.
  pub origin: Option<String>,
}

/// How a response's own stylesheets are written into a document, and the mark
/// the browser reconciles them by: a link the client did not put there is one
/// it must not take away on a navigation.
pub const STYLE_OPEN: &str = "<link rel=\"stylesheet\" data-sf-css=\"\" href=\"";

impl Head {
  pub fn new(title: impl Into<String>, rest: Node) -> Self {
    Self {
      title: title.into(),
      description: None,
      rest,
      entry: None,
      styles: Vec::new(),
      catalog: None,
      head: Vec::new(),
      origin: None,
    }
  }

  /// Every `rel` whose href a crawler reads as an absolute URL, so a path on
  /// one is not a weaker version of the tag but an ignored one.
  const ABSOLUTE_RELS: [&'static str; 2] = ["canonical", "alternate"];

  /// The head node for a document: `rest`, then the title and description
  /// with `meta` overriding the defaults, then the response's own stylesheets
  /// last so they override what the document already links.
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
    let mut merged = Meta {
      title: None,
      description: None,
      head: self.head.clone(),
    };
    merged.merge(meta.clone());
    if let Some(origin) = &self.origin {
      for element in &mut merged.head {
        let rel = element.attrs.iter().find(|(k, _)| k == "rel").map(|(_, v)| v.as_str());
        if !rel.is_some_and(|rel| Self::ABSOLUTE_RELS.contains(&rel)) {
          continue;
        }
        for (name, value) in element.attrs.iter_mut() {
          if name == "href" && is_path(value) {
            *value = format!("{origin}{value}");
          }
        }
      }
    }
    for element in &merged.head {
      element.render(&mut tail);
    }
    for href in &self.styles {
      tail.push_str(STYLE_OPEN);
      tail.push_str(&escape(href));
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
    Self {
      title: String::new(),
      description: None,
      rest,
      entry: None,
      styles: Vec::new(),
      catalog: None,
      head: Vec::new(),
      origin: None,
    }
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

/// A path on this deployment's own origin, which is what an origin may be put
/// in front of: `/` first and not a scheme-relative `//host`.
fn is_path(href: &str) -> bool {
  href.starts_with('/') && !href.starts_with("//")
}

fn escape(text: &str) -> String {
  text
    .replace('&', "&amp;")
    .replace('<', "&lt;")
    .replace('>', "&gt;")
    .replace('"', "&quot;")
}
