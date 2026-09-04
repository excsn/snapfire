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

/// Island ids must be unique per response, and a streamed response serializes
/// the initial tree and each late slot separately, so the counter lives in a
/// session that spans them.
#[derive(Default)]
pub struct HtmlSession {
  next_island: u32,
}

impl HtmlSession {
  pub fn new() -> Self {
    Self::default()
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
          Some(rendered) => self.write(rendered, out),
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

/// One-shot form for a tree with no streamed continuation.
pub fn html_serialize(node: &Node) -> String {
  HtmlSession::new().serialize(node)
}
