use crate::{Align, Border, Boxed, Color, Direction, Extent, Inline, Justify, Node, Text, Wrap};

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
  for (name, value) in &el.attrs {
    let cells = || value.parse::<u16>().map_err(|_| format!("`{name}` on a Box is a number of cells, not `{value}`"));
    let extent = || match value.strip_suffix('%') {
      Some(share) => share.trim().parse::<f32>().ok().filter(|p| p.is_finite() && *p >= 0.0).map(Extent::Percent).ok_or_else(|| format!("`{name}` on a Box is cells or a percentage, not `{value}`")),
      None => cells().map(Extent::Cells),
    };
    let offset = || value.parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n.trunc() as i32).ok_or_else(|| format!("`{name}` on a Box is a number of cells, not `{value}`"));
    let s = &mut out.style;
    match name.as_str() {
      "flex-direction" => s.direction = if value.starts_with("column") { Direction::Column } else { Direction::Row },
      "gap" | "column-gap" | "row-gap" => s.gap = cells()?,
      "padding" => s.padding = [cells()?; 4],
      "padding-x" => (s.padding[1], s.padding[3]) = (cells()?, cells()?),
      "padding-y" => (s.padding[0], s.padding[2]) = (cells()?, cells()?),
      "padding-top" => s.padding[0] = cells()?,
      "padding-right" => s.padding[1] = cells()?,
      "padding-bottom" => s.padding[2] = cells()?,
      "padding-left" => s.padding[3] = cells()?,
      "margin" => s.margin = [cells()?; 4],
      "margin-x" => (s.margin[1], s.margin[3]) = (cells()?, cells()?),
      "margin-y" => (s.margin[0], s.margin[2]) = (cells()?, cells()?),
      "margin-top" => s.margin[0] = cells()?,
      "margin-right" => s.margin[1] = cells()?,
      "margin-bottom" => s.margin[2] = cells()?,
      "margin-left" => s.margin[3] = cells()?,
      "width" => s.width = Some(extent()?),
      "height" => s.height = Some(extent()?),
      "flex-grow" => s.grow = value.parse().map_err(|_| format!("`flex-grow` is a number, not `{value}`"))?,
      "flex-shrink" => s.shrink = Some(value.parse().map_err(|_| format!("`flex-shrink` is a number, not `{value}`"))?),
      "justify-content" => {
        s.justify = Some(match value.as_str() {
          "center" => Justify::Center,
          "flex-end" => Justify::End,
          "space-between" => Justify::SpaceBetween,
          "space-around" => Justify::SpaceAround,
          _ => Justify::Start,
        })
      }
      "align-items" => {
        s.align = Some(match value.as_str() {
          "center" => Align::Center,
          "flex-end" => Align::End,
          "stretch" => Align::Stretch,
          _ => Align::Start,
        })
      }
      "border-style" => {
        s.border = Some(match value.as_str() {
          "double" => Border::Double,
          "round" => Border::Round,
          "bold" => Border::Bold,
          "classic" => Border::Classic,
          _ => Border::Single,
        })
      }
      "border-color" => s.border_color = Color::parse(value),
      "overflow" => (s.overflow_x, s.overflow_y) = (value == "hidden", value == "hidden"),
      "overflow-x" => s.overflow_x = value == "hidden",
      "overflow-y" => s.overflow_y = value == "hidden",
      "content-offset-x" => s.content_offset.0 = offset()?,
      "content-offset-y" => s.content_offset.1 = offset()?,
      "ink-ref" => out.id = Some(value.clone()),
      _ => {}
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
  for (name, value) in &el.attrs {
    let on = value != "false";
    match name.as_str() {
      "color" => out.style.color = Color::parse(value),
      "background-color" => out.style.background = Color::parse(value),
      "bold" => out.style.bold = on,
      "dim-color" => out.style.dim = on,
      "italic" => out.style.italic = on,
      "underline" => out.style.underline = on,
      "strikethrough" => out.style.strikethrough = on,
      "inverse" => out.style.inverse = on,
      "wrap" => out.wrap = if value.starts_with("truncate") { Wrap::Truncate } else { Wrap::Wrap },
      _ => {}
    }
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
