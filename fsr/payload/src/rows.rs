use serde_json::{json, Value as Json};
use snapfire_fsr_core::{Html, ModuleId, Node, SlotId, Value};

use crate::json::{json_to_value, value_to_json, DecodeError};
use crate::FORMAT_VERSION;

fn err(msg: impl Into<String>) -> DecodeError {
  DecodeError(msg.into())
}

pub fn node_to_row_json(node: &Node) -> Json {
  match node {
    Node::Text(v) => json!(["t", v]),
    Node::Raw(v) => json!(["r", v.0]),
    Node::Seq(items) => {
      let children: Vec<Json> = items.iter().map(node_to_row_json).collect();
      json!(["q", children])
    }
    Node::Client { module, props, children, ssr } => {
      let ch: Vec<Json> = children.iter().map(node_to_row_json).collect();
      json!(["c", {
        "m": module.to_string(),
        "p": value_to_json(&Value::Map(props.clone())),
        "ch": ch,
        "s": ssr.as_ref().map(|n| node_to_row_json(n)),
      }])
    }
    Node::Pending { slot, fallback } => json!(["p", slot.0, node_to_row_json(fallback)]),
  }
}

pub fn row_json_to_node(json: &Json) -> Result<Node, DecodeError> {
  let row = json.as_array().ok_or_else(|| err("node row must be an array"))?;
  let kind = row.first().and_then(Json::as_str).ok_or_else(|| err("node row missing kind"))?;
  match kind {
    "t" => Ok(Node::text(row.get(1).and_then(Json::as_str).ok_or_else(|| err("`t` row needs a string"))?.to_owned())),
    "r" => Ok(Node::Raw(Html(row.get(1).and_then(Json::as_str).ok_or_else(|| err("`r` row needs a string"))?.to_owned()))),
    "q" => {
      let items = row.get(1).and_then(Json::as_array).ok_or_else(|| err("`q` row needs an array"))?;
      let mut out = Vec::with_capacity(items.len());
      for item in items {
        out.push(row_json_to_node(item)?);
      }
      Ok(Node::Seq(out))
    }
    "c" => {
      let body = row.get(1).and_then(Json::as_object).ok_or_else(|| err("`c` row needs an object"))?;
      let module: ModuleId = body
        .get("m")
        .and_then(Json::as_str)
        .ok_or_else(|| err("`c` row needs `m`"))?
        .parse()
        .map_err(|e| err(format!("{e}")))?;
      let props = match json_to_value(body.get("p").ok_or_else(|| err("`c` row needs `p`"))?)? {
        Value::Map(map) => map,
        _ => return Err(err("`c` row `p` must decode to a map")),
      };
      let mut children = Vec::new();
      if let Some(ch) = body.get("ch").and_then(Json::as_array) {
        for item in ch {
          children.push(row_json_to_node(item)?);
        }
      }
      let ssr = match body.get("s") {
        None | Some(Json::Null) => None,
        Some(v) => Some(Box::new(row_json_to_node(v)?)),
      };
      Ok(Node::Client { module, props, children, ssr })
    }
    "p" => {
      let slot = row.get(1).and_then(Json::as_u64).ok_or_else(|| err("`p` row needs a slot id"))?;
      let fallback = row_json_to_node(row.get(2).ok_or_else(|| err("`p` row needs a fallback"))?)?;
      Ok(Node::Pending { slot: SlotId(slot as u32), fallback: Box::new(fallback) })
    }
    other => Err(err(format!("unknown node row kind `{other}`"))),
  }
}

/// The wire encoding of one complete page: a version row, then the tree row.
/// Slot resolution rows join in the streaming phase.
pub fn serialize_page(node: &Node) -> String {
  let mut out = String::new();
  out.push_str(&format!("V {}\n", json!({ "fmt": FORMAT_VERSION, "enc": "json" })));
  out.push_str(&format!("N {}\n", node_to_row_json(node)));
  out
}
