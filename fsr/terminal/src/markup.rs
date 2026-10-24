use crate::{Boxed, Inline, Node, Text};

/// One parsed element: its tag, its attributes and what it holds.
struct Element {
  tag: String,
  attrs: Vec<(String, String)>,
  children: Vec<Piece>,
}

enum Piece {
  Element(Element),
  Text(String),
}

/// The `ink-*` markup a lowered component renders, as the elements it stands
/// for. Comments and `data-sf-*` markers are skipped; any other tag is refused.
pub fn from_markup(markup: &str) -> Result<Node, String> {
  let mut at = 0;
  let pieces = pieces(markup, &mut at, None)?;
  let mut nodes = Vec::new();
  for piece in pieces {
    nodes.push(node(piece)?);
  }
  Ok(match nodes.len() {
    1 => nodes.remove(0),
    _ => {
      let mut column = Boxed::new().column();
      column.children = nodes;
      Node::Box(column)
    }
  })
}

fn pieces(markup: &str, at: &mut usize, closing: Option<&str>) -> Result<Vec<Piece>, String> {
  let mut out = Vec::new();
  while *at < markup.len() {
    let rest = &markup[*at..];
    if let Some(after) = rest.strip_prefix("<!--") {
      let end = after.find("-->").ok_or("an unclosed comment")?;
      *at += 4 + end + 3;
    } else if let Some(after) = rest.strip_prefix("</") {
      let end = after.find('>').ok_or("an unclosed end tag")?;
      let tag = &after[..end];
      if closing != Some(tag) {
        return Err(format!("`</{tag}>` closes nothing open"));
      }
      *at += 2 + end + 1;
      return Ok(out);
    } else if rest.starts_with('<') {
      let end = rest.find('>').ok_or("an unclosed tag")?;
      let (tag, attrs) = open_tag(&rest[1..end])?;
      *at += end + 1;
      let children = pieces(markup, at, Some(&tag))?;
      out.push(Piece::Element(Element { tag, attrs, children }));
    } else {
      let end = rest.find('<').unwrap_or(rest.len());
      out.push(Piece::Text(unescape(&rest[..end])));
      *at += end;
    }
  }
  match closing {
    Some(tag) => Err(format!("`<{tag}>` is never closed")),
    None => Ok(out),
  }
}

fn open_tag(inside: &str) -> Result<(String, Vec<(String, String)>), String> {
  let inside = inside.trim_end_matches('/').trim();
  let tag_end = inside.find(char::is_whitespace).unwrap_or(inside.len());
  let tag = inside[..tag_end].to_owned();
  let mut attrs = Vec::new();
  let mut rest = inside[tag_end..].trim_start();
  while !rest.is_empty() {
    let name_end = rest.find(|c: char| c == '=' || c.is_whitespace()).unwrap_or(rest.len());
    let name = rest[..name_end].to_owned();
    rest = rest[name_end..].trim_start();
    let value = match rest.strip_prefix('=') {
      Some(after) => {
        let after = after.trim_start();
        let Some(quoted) = after.strip_prefix('"') else { return Err(format!("`{name}` on `<{tag}>` has an unquoted value")) };
        let close = quoted.find('"').ok_or_else(|| format!("`{name}` on `<{tag}>` is never closed"))?;
        rest = quoted[close + 1..].trim_start();
        unescape(&quoted[..close])
      }
      None => "true".to_owned(),
    };
    attrs.push((name, value));
  }
  Ok((tag, attrs))
}

fn unescape(text: &str) -> String {
  text.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#x27;", "'").replace("&#39;", "'").replace("&amp;", "&")
}

fn node(piece: Piece) -> Result<Node, String> {
  match piece {
    Piece::Text(text) => Ok(Node::Text(Text::plain(&text))),
    Piece::Element(el) => match el.tag.as_str() {
      "ink-box" => boxed(el).map(Node::Box),
      "ink-spacer" => Ok(Node::Box(Boxed::new().grow(1.0))),
      "ink-text" | "ink-newline" => text(el).map(Node::Text),
      other => Err(format!("`<{other}>` is not an Ink element")),
    },
  }
}

fn boxed(el: Element) -> Result<Boxed, String> {
  let mut out = Boxed::new();
  for (written, value) in &el.attrs {
    match written.as_str() {
      "ink-ref" => out.id = Some(value.clone()),
      _ => {
        crate::props::apply(crate::props::BOX_PROPS, "Box", written, value, &mut out.style)?;
      }
    }
  }
  for child in el.children {
    out.children.push(node(child)?);
  }
  Ok(out)
}

fn text(el: Element) -> Result<Text, String> {
  let mut out = Text::new();
  if el.tag == "ink-newline" {
    return Ok(out.str("\n"));
  }
  for (written, value) in &el.attrs {
    crate::props::apply(crate::props::TEXT_PROPS, "Text", written, value, &mut out)?;
  }
  for child in el.children {
    match child {
      Piece::Text(text) => out.children.push(Inline::Str(text)),
      Piece::Element(el) if el.tag == "ink-text" || el.tag == "ink-newline" => out.children.push(Inline::Text(text(el)?)),
      Piece::Element(el) => return Err(format!("`<{}>` inside a Text, which holds text alone", el.tag)),
    }
  }
  Ok(out)
}
