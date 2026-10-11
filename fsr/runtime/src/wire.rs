//! The payload's rows read back: what [`wire_stream`](crate::wire_stream)
//! wrote, one [`Row`] per line, so a host can take another host's answer
//! apart and splice it into its own.

use serde_json::Value as Json;
use snapfire_fsr_core::{Node, Value};
use snapfire_fsr_payload::{json_to_value, row_json_to_node, DecodeError, FORMAT_VERSION};

use crate::meta::Meta;
use crate::segments::SegmentInfo;
use crate::store::Contribution;
use crate::stream::segments_from_json;

fn err(message: impl Into<String>) -> DecodeError {
  DecodeError(message.into())
}

/// One line of a payload, by its tag.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
  Version { fmt: u32, enc: String },
  Tree(Node),
  Head(Meta),
  Store(Vec<Contribution>),
  Locale(String),
  Entry(String),
  Styles(Vec<String>),
  /// The catalog as it was written, a JSON object.
  Catalog(String),
  Segments(SegmentInfo),
  Fill { slot: u32, node: Node, segments: Vec<SegmentInfo> },
}

/// What `contributions_to_json` wrote, read back.
pub fn contributions_from_json(json: &Json) -> Result<Vec<Contribution>, DecodeError> {
  let rows = json.as_array().ok_or_else(|| err("a `T` row is a list"))?;
  let mut out = Vec::with_capacity(rows.len());
  for row in rows {
    let obj = row.as_object().ok_or_else(|| err("a contribution is an object"))?;
    let segment = obj.get("k").and_then(Json::as_str).ok_or_else(|| err("a contribution needs `k`"))?.to_owned();
    let mut path = Vec::new();
    for item in obj.get("p").and_then(Json::as_array).ok_or_else(|| err("a contribution needs `p`"))? {
      path.push(item.as_str().ok_or_else(|| err("`p` holds slot names"))?.to_owned());
    }
    let values = data_of(obj.get("v").ok_or_else(|| err("a contribution needs `v`"))?)?;
    let mut contribution = Contribution::seeded(segment, path, values);
    if let Some(defaults) = obj.get("d") {
      contribution = contribution.with_defaults(data_of(defaults)?);
    }
    if let Some(awaits) = obj.get("w") {
      for key in awaits.as_array().ok_or_else(|| err("`w` is a list"))? {
        contribution.awaits.push(key.as_str().ok_or_else(|| err("`w` holds keys"))?.to_owned());
      }
    }
    out.push(contribution);
  }
  Ok(out)
}

fn data_of(json: &Json) -> Result<snapfire_fsr_core::Data, DecodeError> {
  match json_to_value(json)? {
    Value::Map(map) => Ok(map),
    _ => Err(err("a store contribution is a map")),
  }
}

/// What `meta_to_json` wrote, read back: a title or a description, nothing else.
pub fn meta_from_json(json: &Json) -> Result<Meta, DecodeError> {
  let obj = json.as_object().ok_or_else(|| err("an `H` row is an object"))?;
  let mut meta = Meta::default();
  if let Some(title) = obj.get("title") {
    meta.title = Some(title.as_str().ok_or_else(|| err("`title` is a string"))?.to_owned());
  }
  if let Some(description) = obj.get("description") {
    meta.description = Some(description.as_str().ok_or_else(|| err("`description` is a string"))?.to_owned());
  }
  Ok(meta)
}

/// One line, `<tag> <json>`, as a row. A tag this reader does not know is an
/// error naming it, so a newer writer is refused rather than half read.
pub fn row_from_line(line: &str) -> Result<Row, DecodeError> {
  let line = line.strip_suffix('\n').unwrap_or(line);
  let (tag, rest) = line.split_once(' ').ok_or_else(|| err(format!("a row is `<tag> <json>`, not `{line}`")))?;
  let json = |text: &str| serde_json::from_str::<Json>(text).map_err(|e| err(format!("`{tag}` row: {e}")));
  Ok(match tag {
    "V" => {
      let body = json(rest)?;
      let fmt = body.get("fmt").and_then(Json::as_u64).ok_or_else(|| err("`V` row needs `fmt`"))? as u32;
      if fmt != FORMAT_VERSION {
        return Err(err(format!("payload format {fmt}; this reader reads {FORMAT_VERSION}")));
      }
      let enc = body.get("enc").and_then(Json::as_str).unwrap_or("json").to_owned();
      Row::Version { fmt, enc }
    }
    "N" => Row::Tree(row_json_to_node(&json(rest)?)?),
    "H" => Row::Head(meta_from_json(&json(rest)?)?),
    "T" => Row::Store(contributions_from_json(&json(rest)?)?),
    "L" => Row::Locale(json(rest)?.as_str().ok_or_else(|| err("`L` row is a string"))?.to_owned()),
    "E" => Row::Entry(json(rest)?.as_str().ok_or_else(|| err("`E` row is a string"))?.to_owned()),
    "C" => {
      let mut styles = Vec::new();
      for item in json(rest)?.as_array().ok_or_else(|| err("`C` row is a list"))? {
        styles.push(item.as_str().ok_or_else(|| err("`C` row holds URLs"))?.to_owned());
      }
      Row::Styles(styles)
    }
    "D" => {
      json(rest)?;
      Row::Catalog(rest.to_owned())
    }
    "G" => Row::Segments(segments_from_json(&json(rest)?)?),
    "S" => {
      let (slot, body) = rest.split_once(' ').ok_or_else(|| err("an `S` row is `S <slot> <json>`"))?;
      let slot: u32 = slot.parse().map_err(|_| err(format!("`S` row slot `{slot}` is not a number")))?;
      let body = json(body)?;
      let node = row_json_to_node(body.get("n").ok_or_else(|| err("an `S` row needs `n`"))?)?;
      let mut segments = Vec::new();
      if let Some(g) = body.get("g") {
        for item in g.as_array().ok_or_else(|| err("`g` is a list"))? {
          segments.push(segments_from_json(item)?);
        }
      }
      Row::Fill { slot, node, segments }
    }
    other => return Err(err(format!("unknown payload row `{other}`"))),
  })
}

/// Rows out of bytes as they arrive: a line is a row once its newline is in,
/// whatever chunk boundaries the transport chose.
#[derive(Default)]
pub struct RowReader {
  buffer: Vec<u8>,
}

impl RowReader {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Row>, DecodeError> {
    self.buffer.extend_from_slice(bytes);
    let mut rows = Vec::new();
    while let Some(end) = self.buffer.iter().position(|b| *b == b'\n') {
      let line: Vec<u8> = self.buffer.drain(..=end).collect();
      let text = std::str::from_utf8(&line).map_err(|_| err("a row is UTF-8"))?;
      rows.push(row_from_line(text)?);
    }
    Ok(rows)
  }

  /// What a stream that ended without a final newline left behind, if a row.
  pub fn finish(self) -> Result<Option<Row>, DecodeError> {
    if self.buffer.iter().all(u8::is_ascii_whitespace) {
      return Ok(None);
    }
    let text = std::str::from_utf8(&self.buffer).map_err(|_| err("a row is UTF-8"))?;
    row_from_line(text).map(Some)
  }
}
