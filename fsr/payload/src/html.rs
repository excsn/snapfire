use serde_json::Value as Json;
use snapfire_fsr_core::{ModuleId, Node, Place, Props, Value};

use crate::json::value_to_json;

fn escape_text(input: &str, out: &mut String) {
  for c in input.chars() {
    match c {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      c => out.push(c),
    }
  }
}

/// `<` becomes < inside the JSON so `</script>` can never terminate the props tag.
fn script_safe_json(json: &Json) -> String {
  json.to_string().replace('<', "\\u003c")
}

/// Island ids must be unique per response and a streamed response serializes
/// the initial tree and each late slot separately, so the counter lives in a
/// session that spans them.
#[derive(Default)]
pub struct HtmlSession {
  next_island: u32,
  segment: String,
  /// The island whose own markup is being written, by its region key or else its id; empty outside one.
  island: String,
  placed: String,
  /// The opening delimiter of the last piece in `placed`, which the next piece joins when it opens the same way.
  last_open: String,
  /// The content placed at each named place since the last `finish`, by name in the order the names were first placed at.
  named: Vec<(String, Vec<Piece>)>,
  /// Where each `Node::Outlet` was written since the last `finish`: its offset in the string the writes went to and its name.
  outlets: Vec<(usize, String)>,
  /// How many placed pieces are being written around the current write; an outlet inside one is not recorded.
  placing: u32,
}

/// An island `HtmlSession::open_client` opened: its id and the island its placed content named before.
#[derive(Debug)]
#[must_use]
pub struct OpenClient {
  id: u32,
  outer: Option<String>,
}

/// One `Node::Placed`'s content with the segment and island it was written in.
struct Piece {
  segment: String,
  island: String,
  content: String,
}

const PLACED_END: &str = "<template data-sf-l-end></template>";

/// The prop an island's region key rides under, which names it the same way on the server and in every render the browser makes.
const REGION_KEY: &str = "$k";

impl HtmlSession {
  pub fn new() -> Self {
    Self::default()
  }

  /// Names the segment whose markup the session writes next, which marks the placed content written inside it; answers the one it replaces.
  pub fn enter_segment(&mut self, key: &str) -> String {
    std::mem::replace(&mut self.segment, key.to_owned())
  }

  /// The placed content written since the last take, each piece opened by `<template data-sf-l="KEY" data-sf-island="NAME"></template>` and closed by `<template data-sf-l-end></template>`. The opener names the segment and the island whose markup placed it, by its region key or else its id. One island's consecutive pieces are one run, so a framework that hydrates them in order finds each after the one before. Written after the document it lands at the end of the body: the parser puts a comment there at the document's level, so the delimiters are elements.
  pub fn take_placed(&mut self) -> String {
    self.last_open.clear();
    std::mem::take(&mut self.placed)
  }

  /// `node` written on its own and finished: content placed at a named place whose outlet is not in it is handed on to `take_placed`.
  pub fn serialize(&mut self, node: &Node) -> String {
    let mut out = String::new();
    self.write(node, &mut out);
    self.finish(&mut out);
    out
  }

  /// Writes `node` at the end of `out`. Every `write_into` until the next `finish` has to write to the same string, since an outlet is recorded by its offset in it.
  pub fn write_into(&mut self, node: &Node, out: &mut String) {
    self.write(node, out);
  }

  /// Writes the content placed at each named place into its outlet in `out`, the first outlet of that name when there are several. Content whose outlet `out` does not hold goes to `take_placed`, opened with a `data-sf-to` naming its place, for the browser to move there.
  pub fn finish(&mut self, out: &mut String) {
    let outlets = std::mem::take(&mut self.outlets);
    let mut named = std::mem::take(&mut self.named);
    let mut inserts: Vec<(usize, String)> = Vec::new();
    for (at, name) in outlets {
      if let Some(i) = named.iter().position(|(n, _)| *n == name) {
        let (_, pieces) = named.remove(i);
        inserts.push((at, runs(&pieces, None)));
      }
    }
    if !inserts.is_empty() {
      let extra: usize = inserts.iter().map(|(_, run)| run.len()).sum();
      let mut spliced = String::with_capacity(out.len() + extra);
      let mut from = 0;
      for (at, run) in &inserts {
        spliced.push_str(&out[from..*at]);
        spliced.push_str(run);
        from = *at;
      }
      spliced.push_str(&out[from..]);
      *out = spliced;
    }
    for (name, pieces) in named {
      self.last_open.clear();
      self.placed.push_str(&runs(&pieces, Some(&name)));
    }
  }

  fn write(&mut self, node: &Node, out: &mut String) {
    match node {
      Node::Text(v) => escape_text(v, out),
      Node::Raw(v) => out.push_str(&v.0),
      Node::Seq(items) => {
        for item in items {
          self.write(item, out);
        }
      }
      Node::Client { module, props, children, rendered } => {
        let open = self.open_client(module, props, *rendered, out);
        for child in children {
          self.write(child, out);
        }
        self.close_client(open, props, out);
      }
      Node::Pending { slot, fallback } => {
        out.push_str(&format!("<div data-sf-slot=\"{}\">", slot.0));
        self.write(fallback, out);
        out.push_str("</div>");
      }
      Node::Slot(_) => {}
      Node::Outlet(name) => {
        if self.placing == 0 {
          self.outlets.push((out.len(), name.clone()));
        }
      }
      Node::Placed { target: Place::Named(name), nodes } => {
        let content = self.placed_content(nodes);
        let piece = Piece { segment: self.segment.clone(), island: self.island.clone(), content };
        match self.named.iter_mut().find(|(n, _)| n == name) {
          Some((_, pieces)) => pieces.push(piece),
          None => self.named.push((name.clone(), vec![piece])),
        }
      }
      Node::Placed { nodes, .. } => {
        let content = self.placed_content(nodes);
        let open = opener(&self.segment, &self.island, None);
        if open == self.last_open && self.placed.ends_with(PLACED_END) {
          self.placed.truncate(self.placed.len() - PLACED_END.len());
        } else {
          self.placed.push_str(&open);
        }
        self.placed.push_str(&content);
        self.placed.push_str(PLACED_END);
        self.last_open = open;
      }
    }
  }

  fn placed_content(&mut self, nodes: &[Node]) -> String {
    self.placing += 1;
    let mut content = String::new();
    for node in nodes {
      self.write(node, &mut content);
    }
    self.placing -= 1;
    content
  }

  /// The open tag with a fresh island id and the close tag with the props
  /// script, for a caller that writes the island's children itself.
  /// Writes an island's open tag with a fresh id and, for a rendered island, makes it the island its placed content names until `close_client`. A caller writing the children itself, as the segment writer does around a child segment, writes them between the two.
  pub fn open_client(&mut self, module: &ModuleId, props: &Props, rendered: bool, out: &mut String) -> OpenClient {
    let id = self.next_island;
    self.next_island += 1;
    let mark = if rendered { " data-sf-ssr" } else { "" };
    out.push_str(&format!("<sf-i id=\"sf-i{id}\" data-sf-module=\"{module}\"{mark}>"));
    let outer = rendered.then(|| {
      let name = match props.get(REGION_KEY) {
        Some(Value::Str(key)) => key.to_string(),
        _ => format!("sf-i{id}"),
      };
      std::mem::replace(&mut self.island, name)
    });
    OpenClient { id, outer }
  }

  /// Closes the island `open` opened and writes its props script.
  pub fn close_client(&mut self, open: OpenClient, props: &Props, out: &mut String) {
    if let Some(outer) = open.outer {
      self.island = outer;
    }
    let props_json = script_safe_json(&value_to_json(&Value::Map(props.clone())));
    out.push_str(&format!("</sf-i><script type=\"application/json\" data-sf-props=\"sf-i{}\">{props_json}</script>", open.id));
  }

}

fn attr(value: &str) -> String {
  value.replace('&', "&amp;").replace('"', "&quot;")
}

fn opener(segment: &str, island: &str, to: Option<&str>) -> String {
  let mut open = format!("<template data-sf-l=\"{}\"", attr(segment));
  if !island.is_empty() {
    open.push_str(&format!(" data-sf-island=\"{}\"", attr(island)));
  }
  if let Some(to) = to {
    open.push_str(&format!(" data-sf-to=\"{to}\""));
  }
  open.push_str("></template>");
  open
}

/// `pieces` as runs, consecutive pieces of one segment and island sharing one.
fn runs(pieces: &[Piece], to: Option<&str>) -> String {
  let mut out = String::new();
  let mut last: Option<(&str, &str)> = None;
  for piece in pieces {
    let this = (piece.segment.as_str(), piece.island.as_str());
    if last == Some(this) {
      out.truncate(out.len() - PLACED_END.len());
    } else {
      out.push_str(&opener(&piece.segment, &piece.island, to));
    }
    out.push_str(&piece.content);
    out.push_str(PLACED_END);
    last = Some(this);
  }
  out
}

/// `-` and `%` are escaped so a segment key can never contain `--` and close the comment that carries it.
pub fn escape_key(key: &str) -> String {
  key.replace('%', "%25").replace('-', "%2D")
}

/// One-shot form for a tree with no streamed continuation, its placed content after it.
pub fn html_serialize(node: &Node) -> String {
  let mut session = HtmlSession::new();
  let mut out = session.serialize(node);
  out.push_str(&session.take_placed());
  out
}
