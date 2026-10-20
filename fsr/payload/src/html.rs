use serde_json::Value as Json;
use snapfire_fsr_core::{ModuleId, Node, Props, Value};

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

  pub fn serialize(&mut self, node: &Node) -> String {
    let mut out = String::new();
    self.write(node, &mut out);
    out
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
      Node::Client { module, props, children, ssr } => {
        let id = self.next_island;
        self.next_island += 1;
        out.push_str(&format!("<sf-i id=\"sf-i{id}\" data-sf-module=\"{module}\">"));
        match ssr {
          Some(rendered) => {
            let name = match props.get(REGION_KEY) {
              Some(Value::Str(key)) => key.to_string(),
              _ => format!("sf-i{id}"),
            };
            let outer = std::mem::replace(&mut self.island, name);
            self.write(rendered, out);
            self.island = outer;
          }
          None => {
            for child in children {
              self.write(child, out);
            }
          }
        }
        out.push_str("</sf-i>");
        let props_json = script_safe_json(&value_to_json(&Value::Map(props.clone())));
        out.push_str(&format!(
          "<script type=\"application/json\" data-sf-props=\"sf-i{id}\">{props_json}</script>"
        ));
      }
      Node::Pending { slot, fallback } => {
        out.push_str(&format!("<div data-sf-slot=\"{}\">", slot.0));
        self.write(fallback, out);
        out.push_str("</div>");
      }
      Node::Slot(_) => {}
      Node::Placed { nodes, .. } => {
        let mut content = String::new();
        for node in nodes {
          self.write(node, &mut content);
        }
        let mut open = format!("<template data-sf-l=\"{}\"", self.segment.replace('&', "&amp;").replace('"', "&quot;"));
        if !self.island.is_empty() {
          open.push_str(&format!(" data-sf-island=\"{}\"", self.island.replace('&', "&amp;").replace('"', "&quot;")));
        }
        open.push_str("></template>");
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

  /// The open tag with a fresh island id and the close tag with the props
  /// script, for a caller that writes the island's children itself.
  pub fn client_wrapper(&mut self, module: &ModuleId, props: &Props) -> (String, String) {
    let id = self.next_island;
    self.next_island += 1;
    let props_json = script_safe_json(&value_to_json(&Value::Map(props.clone())));
    (
      format!("<sf-i id=\"sf-i{id}\" data-sf-module=\"{module}\">"),
      format!("</sf-i><script type=\"application/json\" data-sf-props=\"sf-i{id}\">{props_json}</script>"),
    )
  }
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
