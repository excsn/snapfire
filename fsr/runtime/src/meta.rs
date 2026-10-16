use futures_util::future::BoxFuture;
use snapfire_fsr_core::{Data, Node};

use crate::ctx::RequestCtx;
use crate::data::LoadError;

/// One element a segment puts in the head: its tag, its attributes in the
/// order written and, for a tag that takes content, its text.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HeadEl {
  pub tag: String,
  pub attrs: Vec<(String, String)>,
  pub children: Option<String>,
}

impl HeadEl {
  pub fn new(tag: String, attrs: Vec<(String, String)>, children: Option<String>) -> Self {
    Self { tag, attrs, children }
  }
}

/// When a head script loads, as a row's `when` names it.
pub const SCRIPT_WHEN: [&str; 3] = ["load", "idle", "interaction"];

impl HeadEl {
  /// A `script` row as the document carries it: inert, `type="text/plain"`
  /// with its `src` under `data-src`, its `type` under `data-type` and its
  /// timing under `data-sf-script`, for the client's `loadScript` to load
  /// once per document whether it arrived with the first paint or a
  /// navigation. A script inserted as markup by a navigation never runs, so
  /// a live tag would work on first load and silently not after. `Err` names
  /// what the row got wrong: no `src`, an inline body or an unknown `when`.
  pub fn script(attrs: Vec<(String, String)>, children: Option<String>) -> Result<HeadEl, String> {
    if children.is_some() {
      return Err("a head `script` takes a `src`, not inline code; put the code in a module `src/main.ts` imports".to_owned());
    }
    let mut src = None;
    let mut when = "load".to_owned();
    let mut rest = Vec::new();
    for (key, value) in attrs {
      match key.as_str() {
        "src" => src = Some(value),
        "when" => when = value,
        "type" => rest.push(("data-type".to_owned(), value)),
        _ => rest.push((key, value)),
      }
    }
    let Some(src) = src else {
      return Err("a head `script` needs a `src`".to_owned());
    };
    if !SCRIPT_WHEN.contains(&when.as_str()) {
      return Err(format!("a head `script`'s `when` is one of {}, got `{when}`", SCRIPT_WHEN.join(", ")));
    }
    let mut out = vec![("type".to_owned(), "text/plain".to_owned()), ("data-sf-script".to_owned(), when), ("data-src".to_owned(), src)];
    out.extend(rest);
    Ok(HeadEl { tag: "script".to_owned(), attrs: out, children: None })
  }

  /// The `link` rels that name a resource rather than a role: a document
  /// carries one of these per href, so the href is part of what the element
  /// is. Every other rel (`canonical`, `icon`, `manifest`, `alternate`) is a
  /// role an inner segment overrides by naming it again.
  const RESOURCE_RELS: [&'static str; 6] = ["stylesheet", "preconnect", "dns-prefetch", "preload", "modulepreload", "prefetch"];

  /// What makes two entries the same element, so an inner segment replaces an
  /// outer one rather than emitting both: the naming attribute a head element
  /// is identified by in practice or every attribute when it has none.
  /// `sizes`, `media`, `type` and `hreflang` qualify it, since a document
  /// carries several icons under one `rel`, several stylesheets under one
  /// `media` and one `alternate` per language; a resource rel is qualified
  /// by its href as well.
  pub(crate) fn attr(&self, name: &str) -> Option<&str> {
    self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
  }

  /// A script row `script` made, the inert marker the client loads.
  pub fn is_script(&self) -> bool {
    self.tag == "script" && self.attr("data-sf-script").is_some()
  }

  /// A `link` whose `rel` includes `icon`.
  pub fn is_icon(&self) -> bool {
    self.tag == "link" && self.attr("rel").is_some_and(|rel| rel.split_whitespace().any(|r| r == "icon"))
  }

  /// The icon the host links when an application serves none, `href="data:,"`,
  /// which a document drops once it carries any other icon.
  pub fn is_empty_icon(&self) -> bool {
    self.is_icon() && self.attr("href") == Some("data:,")
  }

  pub fn identity(&self) -> (String, String) {
    let of = |name: &str| self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    for name in ["rel", "name", "property", "http-equiv", "itemprop", "id"] {
      if let Some(value) = of(name) {
        let mut qualifier: String = ["sizes", "media", "type", "hreflang"]
          .iter()
          .filter_map(|q| of(q).map(|v| format!(" {q}={v}")))
          .collect();
        if name == "rel" && self.tag == "link" && Self::RESOURCE_RELS.contains(&value) {
          if let Some(href) = of("href") {
            qualifier.push_str(&format!(" href={href}"));
          }
        }
        return (format!("{}[{name}]", self.tag), format!("{value}{qualifier}"));
      }
    }
    let mut all: Vec<String> = self.attrs.iter().map(|(k, v)| format!("{k}={v}")).collect();
    all.sort();
    (self.tag.clone(), all.join("&"))
  }

  /// The HTML void elements, which take no end tag. Every other element is
  /// closed whether or not it has children: a `script` with only a `src` left
  /// open swallows the rest of the document as script text.
  const VOID: [&'static str; 13] = ["area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr"];

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
    }
    if !Self::VOID.contains(&self.tag.as_str()) {
      out.push_str("</");
      out.push_str(&self.tag);
      out.push('>');
    }
  }
}

/// In a module head row's value, the request's path without a trailing
/// slash: `{fsr:path}/opengraph-image.png` names the image beside the page
/// being rendered, which a row written once for the module cannot know.
pub const PATH_MARK: &str = "{fsr:path}";

/// What a route says about itself once its data is known: the document's
/// title, its description and the head elements it contributes. Absent fields
/// leave the host's defaults in place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Meta {
  pub title: Option<String>,
  /// `title` is written as given rather than through the head's template.
  pub absolute: bool,
  pub description: Option<String>,
  pub head: Vec<HeadEl>,
}

impl Meta {
  pub fn title(mut self, title: Option<String>) -> Self {
    self.title = title;
    self
  }

  pub fn absolute(mut self, absolute: bool) -> Self {
    self.absolute = absolute;
    self
  }

  pub fn description(mut self, description: Option<String>) -> Self {
    self.description = description;
    self
  }

  pub fn head(mut self, head: Vec<HeadEl>) -> Self {
    self.head = head;
    self
  }
}

impl Meta {
  pub fn is_empty(&self) -> bool {
    self.title.is_none() && self.description.is_none() && self.head.is_empty()
  }

  /// Adds `og:title` and `og:description` to `head` from the document's
  /// title and description, each only where no element already names it.
  pub fn card_text(head: &mut Vec<HeadEl>, title: &str, description: Option<&str>) {
    let rows = [("og:title", Some(title)), ("og:description", description)];
    for (property, content) in rows {
      let Some(content) = content.filter(|c| !c.is_empty()) else { continue };
      if head.iter().any(|e| e.tag == "meta" && e.attr("property") == Some(property)) {
        continue;
      }
      head.push(HeadEl {
        tag: "meta".to_owned(),
        attrs: vec![("property".to_owned(), property.to_owned()), ("content".to_owned(), content.to_owned())],
        children: None,
      });
    }
  }

  /// Folds an inner segment over this one: a title or description it sets
  /// wins and each of its head elements replaces the one of the same
  /// identity in place. An element matching nothing is appended. Outermost is
  /// folded first, so the innermost segment has the last word.
  pub fn merge(&mut self, inner: Meta) {
    if inner.title.is_some() {
      self.title = inner.title;
      self.absolute = inner.absolute;
    }
    if inner.description.is_some() {
      self.description = inner.description;
    }
    let mut added: Vec<(String, String)> = Vec::new();
    for element in inner.head {
      let identity = element.identity();
      match self.head.iter().position(|held| held.identity() == identity) {
        Some(_) if added.contains(&identity) => self.head.push(element),
        Some(at) => {
          self.head[at] = element;
          added.push(identity);
        }
        None => {
          self.head.push(element);
          added.push(identity);
        }
      }
    }
  }
}

/// The head rows a module's card images list at request time: the field
/// they are, `og:image` or `twitter:image`, and one group of rows per image.
/// They replace every row of that field the page already carries.
pub trait HeadRows: Send + Sync {
  fn rows(&self, ctx: &RequestCtx) -> BoxFuture<'static, Result<(String, Vec<HeadEl>), LoadError>>;
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
#[non_exhaustive]
pub struct Head {
  pub title: String,
  /// How a segment's title is written into the document: `{page}` is the
  /// segment's title and `{title}` the default. Absent, a segment's title is
  /// written as given.
  pub template: Option<String>,
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
  /// The head elements every document carries, the ones the host inferred
  /// from `icons/`. A segment's `meta` folds over these.
  pub head: Vec<HeadEl>,
  /// `scheme://host` for this deployment, `[document] origin`. A crawler reads
  /// `rel=canonical` and `rel=alternate` as absolute URLs only, so a path on
  /// either is prefixed with this. Absent, an href is written as given.
  pub origin: Option<String>,
  /// The document's theme and scheme for this request, when the application
  /// has themes; the payload's eager `H` row carries both, null included, so
  /// a navigation sets or clears `<html data-sf-theme>` and `data-sf-scheme`.
  pub theme: Option<DocumentTheme>,
}

/// What a request decided for `<html data-sf-theme>` and `<html data-sf-scheme>`:
/// no name is the root theme and no scheme leaves the attribute off. `forced` is
/// `<html data-sf-theme-forced>`: the route forces its theme and the document kept it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentTheme {
  pub name: Option<String>,
  pub scheme: Option<String>,
  pub forced: bool,
}

impl DocumentTheme {
  pub fn name(mut self, name: Option<String>) -> Self {
    self.name = name;
    self
  }

  pub fn scheme(mut self, scheme: Option<String>) -> Self {
    self.scheme = scheme;
    self
  }

  pub fn forced(mut self, forced: bool) -> Self {
    self.forced = forced;
    self
  }
}

/// How a response's own stylesheets are written into a document and the mark
/// the browser reconciles them by: a link the client did not put there is one
/// it must not take away on a navigation.
pub const STYLE_OPEN: &str = "<link rel=\"stylesheet\" data-sf-css=\"\" href=\"";

/// How a response's `entry`, the module its islands need beyond the
/// document's own, is written into a document.
pub const ENTRY_OPEN: &str = "<script type=\"module\" src=\"";

impl Head {
  pub fn new(title: impl Into<String>, rest: Node) -> Self {
    Self {
      title: title.into(),
      template: None,
      description: None,
      rest,
      entry: None,
      styles: Vec::new(),
      catalog: None,
      head: Vec::new(),
      origin: None,
      theme: None,
    }
  }

  /// Every `rel` whose href a crawler reads as an absolute URL, so a path on
  /// one is not a weaker version of the tag but an ignored one.
  const ABSOLUTE_RELS: [&'static str; 2] = ["canonical", "alternate"];

  /// Every `meta` whose `content` a social card's crawler fetches as a URL,
  /// by its `property` or `name`.
  const ABSOLUTE_CONTENT: [&'static str; 7] = ["og:image", "og:image:url", "og:image:secure_url", "og:url", "og:video", "og:audio", "twitter:image"];

  /// The head node for a document: `rest`, then the title and description
  /// with `meta` overriding the defaults, then the response's own stylesheets
  /// last so they override what the document already links.
  /// The document's title under `meta`: the segment's title through the
  /// template unless it is absolute, the default when it names none.
  pub fn title_for(&self, meta: &Meta) -> String {
    match &meta.title {
      Some(page) => self.titled(page, meta.absolute),
      None => self.title.clone(),
    }
  }

  /// Writes `meta`'s title through the template, so what the payload or a
  /// streamed segment carries is the title as the document shows it.
  pub fn settle(&self, meta: &mut Meta) {
    if let Some(page) = &meta.title {
      meta.title = Some(self.titled(page, meta.absolute));
    }
    meta.absolute = false;
  }

  fn titled(&self, page: &str, absolute: bool) -> String {
    match &self.template {
      Some(template) if !absolute => fill(template, page, &self.title),
      _ => page.to_owned(),
    }
  }

  pub fn node(&self, meta: &Meta) -> Node {
    let title = self.title_for(meta);
    let description = meta.description.as_deref().or(self.description.as_deref());
    let mut tail = String::new();
    if !title.is_empty() {
      tail.push_str("<title>");
      tail.push_str(&escape(&title));
      tail.push_str("</title>");
    }
    if let Some(description) = description {
      tail.push_str("<meta name=\"description\" content=\"");
      tail.push_str(&escape(description));
      tail.push_str("\">");
    }
    let mut merged = Meta {
      head: self.head.clone(),
      ..Meta::default()
    };
    merged.merge(meta.clone());
    if merged.head.iter().any(|e| e.is_icon() && !e.is_empty_icon()) {
      merged.head.retain(|e| !e.is_empty_icon());
    }
    Meta::card_text(&mut merged.head, &title, description);
    if let Some(origin) = &self.origin {
      for element in &mut merged.head {
        let attr = |key: &str| element.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        let field = if attr("rel").is_some_and(|rel| Self::ABSOLUTE_RELS.contains(&rel)) {
          "href"
        } else if element.tag == "meta" && attr("property").or(attr("name")).is_some_and(|name| Self::ABSOLUTE_CONTENT.contains(&name)) {
          "content"
        } else {
          continue;
        };
        for (name, value) in element.attrs.iter_mut() {
          if name == field && is_path(value) {
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
    if let Some(entry) = &self.entry {
      tail.push_str(ENTRY_OPEN);
      tail.push_str(&escape(entry));
      tail.push_str("\"></script>");
    }
    if tail.is_empty() {
      return self.rest.clone();
    }
    Node::Seq(vec![self.rest.clone(), Node::raw(tail)])
  }
}

/// `template` with `{page}` and `{title}` filled, each read once so a title
/// holding either token is not expanded again.
fn fill(template: &str, page: &str, title: &str) -> String {
  let mut out = String::with_capacity(template.len() + page.len() + title.len());
  let mut rest = template;
  while let Some(at) = rest.find('{') {
    out.push_str(&rest[..at]);
    let tail = &rest[at..];
    if let Some(after) = tail.strip_prefix("{page}") {
      out.push_str(page);
      rest = after;
    } else if let Some(after) = tail.strip_prefix("{title}") {
      out.push_str(title);
      rest = after;
    } else {
      out.push('{');
      rest = &tail[1..];
    }
  }
  out.push_str(rest);
  out
}

impl From<Node> for Head {
  fn from(rest: Node) -> Self {
    Self {
      title: String::new(),
      template: None,
      description: None,
      rest,
      entry: None,
      styles: Vec::new(),
      catalog: None,
      head: Vec::new(),
      origin: None,
      theme: None,
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

#[cfg(test)]
mod tests {
  use super::*;

  fn el(tag: &str, attrs: &[(&str, &str)]) -> HeadEl {
    HeadEl {
      tag: tag.to_owned(),
      attrs: attrs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
      children: None,
    }
  }

  fn merged(outer: Vec<HeadEl>, inner: Vec<HeadEl>) -> Vec<HeadEl> {
    let mut meta = Meta { head: outer, ..Meta::default() };
    meta.merge(Meta { head: inner, ..Meta::default() });
    meta.head
  }

  fn rendered(el: &HeadEl) -> String {
    let mut out = String::new();
    el.render(&mut out);
    out
  }

  #[test]
  fn a_script_with_only_a_src_is_closed() {
    let script = el("script", &[("defer", ""), ("src", "/a.js")]);
    assert_eq!(rendered(&script), r#"<script defer="" src="/a.js"></script>"#);
  }

  fn tail(head: &Head, meta: &Meta) -> String {
    match head.node(meta) {
      Node::Seq(parts) => match parts.last() {
        Some(Node::Raw(html)) => html.0.clone(),
        other => panic!("{other:?}"),
      },
      other => panic!("{other:?}"),
    }
  }

  #[test]
  fn a_card_image_path_is_written_against_the_origin() {
    let mut head = Head::new("t", Node::raw(""));
    head.origin = Some("https://shop.example".to_owned());
    let meta = Meta {
      head: vec![
        el("meta", &[("property", "og:image"), ("content", "/static/og.1a2b3c4d.png")]),
        el("meta", &[("name", "twitter:image"), ("content", "https://cdn.example/t.png")]),
        el("meta", &[("property", "og:image:alt"), ("content", "/not/a/url")]),
      ],
      ..Meta::default()
    };
    let html = tail(&head, &meta);
    assert!(html.contains(r#"content="https://shop.example/static/og.1a2b3c4d.png""#), "{html}");
    assert!(html.contains(r#"content="https://cdn.example/t.png""#), "an absolute one is left as written: {html}");
    assert!(html.contains(r#"content="/not/a/url""#), "only a property a crawler fetches: {html}");
  }

  #[test]
  fn the_empty_icon_gives_way_to_any_other_icon() {
    let mut head = Head::new("t", Node::raw(""));
    head.head = vec![el("link", &[("rel", "icon"), ("href", "data:,")])];
    assert!(tail(&head, &Meta::default()).contains(r#"<link rel="icon" href="data:,">"#));

    let typed = Meta { head: vec![el("link", &[("rel", "icon"), ("type", "image/png"), ("href", "/i.png")])], ..Meta::default() };
    let html = tail(&head, &typed);
    assert!(html.contains("/i.png") && !html.contains("data:,"), "{html}");
  }

  #[test]
  fn a_void_element_takes_no_end_tag() {
    assert_eq!(rendered(&el("meta", &[("name", "robots"), ("content", "index")])), r#"<meta name="robots" content="index">"#);
    assert_eq!(rendered(&el("link", &[("rel", "icon"), ("href", "/i.png")])), r#"<link rel="icon" href="/i.png">"#);
    assert_eq!(rendered(&el("base", &[("href", "/")])), r#"<base href="/">"#);
  }

  #[test]
  fn a_resource_rel_is_one_element_per_href() {
    let outer = vec![el("link", &[("rel", "preconnect"), ("href", "https://fonts.example")])];
    let inner = vec![
      el("link", &[("rel", "preconnect"), ("href", "https://cdn.example")]),
      el("link", &[("rel", "stylesheet"), ("href", "/a.css")]),
      el("link", &[("rel", "stylesheet"), ("href", "/b.css")]),
    ];
    assert_eq!(merged(outer, inner).len(), 4);
  }

  #[test]
  fn a_repeated_resource_href_replaces_in_place() {
    let outer = vec![el("link", &[("rel", "stylesheet"), ("href", "/a.css")])];
    let inner = vec![el("link", &[("rel", "stylesheet"), ("href", "/a.css"), ("media", "print")])];
    let head = merged(outer.clone(), inner);
    assert_eq!(head.len(), 2, "media still qualifies");
    let head = merged(outer, vec![el("link", &[("rel", "stylesheet"), ("href", "/a.css")])]);
    assert_eq!(head.len(), 1);
  }

  #[test]
  fn a_role_rel_is_overridden_by_naming_it_again() {
    let outer = vec![el("link", &[("rel", "canonical"), ("href", "/outer")])];
    let inner = vec![el("link", &[("rel", "canonical"), ("href", "/inner")])];
    let head = merged(outer, inner);
    assert_eq!(head.len(), 1);
    assert_eq!(head[0].attrs[1].1, "/inner");
  }

  #[test]
  fn alternates_differ_by_hreflang_and_icons_by_type() {
    let outer = vec![
      el("link", &[("rel", "alternate"), ("hreflang", "fr"), ("href", "/fr")]),
      el("link", &[("rel", "icon"), ("type", "image/svg+xml"), ("href", "/favicon.svg")]),
    ];
    let inner = vec![
      el("link", &[("rel", "alternate"), ("hreflang", "de"), ("href", "/de")]),
      el("link", &[("rel", "icon"), ("type", "image/x-icon"), ("href", "/favicon.ico")]),
      el("link", &[("rel", "icon"), ("type", "image/svg+xml"), ("href", "/other.svg")]),
    ];
    let head = merged(outer, inner);
    assert_eq!(head.len(), 4);
    assert_eq!(head[1].attrs[2].1, "/other.svg", "the same rel and type replaces");
  }

  #[test]
  fn a_script_row_becomes_an_inert_marker_and_rides_the_head_row() {
    let row = |attrs: &[(&str, &str)], children: Option<&str>| HeadEl::script(attrs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(), children.map(str::to_owned));
    let script = row(&[("src", "https://maps.example/w.js"), ("when", "idle"), ("type", "module"), ("crossorigin", "anonymous")], None).unwrap();
    let mut html = String::new();
    script.render(&mut html);
    assert_eq!(html, "<script type=\"text/plain\" data-sf-script=\"idle\" data-src=\"https://maps.example/w.js\" data-type=\"module\" crossorigin=\"anonymous\"></script>");
    assert!(script.is_script());
    assert_eq!(row(&[("src", "/a.js")], None).unwrap().attrs[1].1, "load", "no `when` loads with the page");
    assert!(row(&[("when", "idle")], None).unwrap_err().contains("needs a `src`"));
    assert!(row(&[("src", "/a.js")], Some("alert(1)")).unwrap_err().contains("not inline code"));
    assert!(row(&[("src", "/a.js"), ("when", "soon")], None).unwrap_err().contains("load, idle, interaction"));

    let meta = Meta { head: vec![script, el("link", &[("rel", "preconnect"), ("href", "https://maps.example")])], ..Meta::default() };
    let json = crate::stream::meta_to_json(&meta);
    assert_eq!(json["scripts"].as_array().map(Vec::len), Some(1), "only the script is a script row: {json}");
    assert_eq!(json["scripts"][0]["data-src"], "https://maps.example/w.js");
    assert_eq!(json["head"].as_array().map(Vec::len), Some(1), "the link rides as a head element: {json}");
    let back = crate::wire::meta_from_json(&json).unwrap();
    assert_eq!(back.head.len(), 2);
    assert!(back.head[0].is_script() && back.head[0].attrs == meta.head[0].attrs, "the head row read back keeps its scripts: {back:?}");
    assert_eq!(back.head[1], meta.head[1], "and its other head elements, a card image's meta among them");
  }
}
