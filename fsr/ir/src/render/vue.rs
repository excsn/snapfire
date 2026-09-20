//! What Vue's server renderer writes, so a lowered Vue component's markup is
//! what Vue's client hydrates over. Checked against `@vue/server-renderer`
//! and `@vue/shared` at v3.5.13.

use snapfire_fsr_core::{Value, ValueMap};

use crate::interp::{stringify, truthy, Fail};

/// A Vue major the renderer writes markup for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum VueMajor {
  #[default]
  V3,
}

impl VueMajor {
  pub const ALL: [VueMajor; 1] = [Self::V3];

  /// The major of a version such as `3.5.13`; `None` for one the renderer has no rules for.
  pub fn of(version: &str) -> Option<Self> {
    match version.split('.').next()? {
      "3" => Some(Self::V3),
      _ => None,
    }
  }

  pub fn number(self) -> u32 {
    match self {
      Self::V3 => 3,
    }
  }
}

/// Opens a fragment: a multi-root component, a `v-for` list, slot content or
/// a branch holding more than one element.
pub const FRAGMENT_OPEN: &str = "<!--[-->";
pub const FRAGMENT_CLOSE: &str = "<!--]-->";
/// Where a `v-if` chain rendered no branch.
pub const EMPTY: &str = "<!---->";

/// `@vue/shared`'s `isBooleanAttr`: present or absent rather than valued.
const BOOLEAN: &[&str] = &["itemscope", "allowfullscreen", "formnovalidate", "ismap", "nomodule", "novalidate", "readonly", "async", "autofocus", "autoplay", "controls", "default", "defer", "disabled", "hidden", "inert", "loop", "open", "required", "reversed", "scoped", "seamless", "checked", "muted", "multiple", "selected"];

pub(super) fn is_boolean(name: &str) -> bool {
  BOOLEAN.contains(&name)
}

/// `@vue/shared`'s `escapeHtml`, which text and attributes share.
pub(super) fn escape(input: &str, out: &mut String) {
  let bytes = input.as_bytes();
  let Some(first) = bytes.iter().position(|b| matches!(b, b'"' | b'&' | b'\'' | b'<' | b'>')) else {
    out.push_str(input);
    return;
  };
  out.push_str(&input[..first]);
  let mut last = first;
  for (offset, byte) in bytes[first..].iter().enumerate() {
    let replacement = match byte {
      b'"' => "&quot;",
      b'&' => "&amp;",
      b'\'' => "&#39;",
      b'<' => "&lt;",
      b'>' => "&gt;",
      _ => continue,
    };
    let i = first + offset;
    out.push_str(&input[last..i]);
    out.push_str(replacement);
    last = i + 1;
  }
  out.push_str(&input[last..]);
}

/// `toDisplayString`: what an interpolation shows for a value.
pub(super) fn display(value: &Value) -> Result<String, Fail> {
  Ok(match value {
    Value::Null => String::new(),
    Value::Seq(_) | Value::Map(_) => {
      let mut out = String::new();
      json(value, 0, &mut out)?;
      out
    }
    other => stringify(other)?,
  })
}

/// `JSON.stringify(value, null, 2)`.
fn json(value: &Value, depth: usize, out: &mut String) -> Result<(), Fail> {
  match value {
    Value::Null => out.push_str("null"),
    Value::Str(s) => json_string(s, out),
    Value::Seq(items) => {
      if items.is_empty() {
        out.push_str("[]");
        return Ok(());
      }
      out.push('[');
      for (i, item) in items.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        out.push('\n');
        out.push_str(&"  ".repeat(depth + 1));
        json(item, depth + 1, out)?;
      }
      out.push('\n');
      out.push_str(&"  ".repeat(depth));
      out.push(']');
    }
    Value::Map(map) => {
      if map.is_empty() {
        out.push_str("{}");
        return Ok(());
      }
      out.push('{');
      for (i, (key, item)) in map.iter().enumerate() {
        if i > 0 {
          out.push(',');
        }
        out.push('\n');
        out.push_str(&"  ".repeat(depth + 1));
        json_string(key, out);
        out.push_str(": ");
        json(item, depth + 1, out)?;
      }
      out.push('\n');
      out.push_str(&"  ".repeat(depth));
      out.push('}');
    }
    other => out.push_str(&stringify(other)?),
  }
  Ok(())
}

fn json_string(text: &str, out: &mut String) {
  out.push('"');
  for c in text.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
      c => out.push(c),
    }
  }
  out.push('"');
}

/// `normalizeClass`: a string as it stands, an array's items in turn, an
/// object's keys whose values are truthy, joined by one space.
pub(super) fn class_text(value: &Value) -> Result<String, Fail> {
  let mut out = String::new();
  class_into(value, &mut out)?;
  Ok(out.trim().to_owned())
}

fn class_into(value: &Value, out: &mut String) -> Result<(), Fail> {
  match value {
    Value::Str(s) => {
      if !s.is_empty() {
        out.push_str(s);
        out.push(' ');
      }
    }
    Value::Seq(items) => {
      for item in items {
        let mut piece = String::new();
        class_into(item, &mut piece)?;
        let piece = piece.trim();
        if !piece.is_empty() {
          out.push_str(piece);
          out.push(' ');
        }
      }
    }
    Value::Map(map) => {
      for (key, held) in map {
        if truthy(held) {
          out.push_str(key);
          out.push(' ');
        }
      }
    }
    _ => {}
  }
  Ok(())
}

/// `normalizeStyle` then `stringifyStyle`: `name:value;` per entry, a string
/// item parsed as inline CSS, a camelCase name hyphenated, a number written
/// as it stands.
pub(super) fn style_text(value: &Value) -> Result<String, Fail> {
  match value {
    Value::Str(s) => Ok(s.to_string()),
    other => {
      let mut styles = ValueMap::default();
      style_into(other, &mut styles);
      let mut out = String::new();
      for (name, held) in &styles {
        let text = match held {
          Value::Str(s) => s.to_string(),
          Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) => stringify(held)?,
          _ => continue,
        };
        if name.starts_with("--") {
          out.push_str(name);
        } else {
          hyphenate(name, &mut out);
        }
        out.push(':');
        out.push_str(&text);
        out.push(';');
      }
      Ok(out)
    }
  }
}

fn style_into(value: &Value, into: &mut ValueMap) {
  match value {
    Value::Seq(items) => {
      for item in items {
        style_into(item, into);
      }
    }
    Value::Map(map) => {
      for (name, held) in map {
        into.insert(name.clone(), held.clone());
      }
    }
    Value::Str(text) => {
      for (name, held) in parse_style(text) {
        into.insert(name, Value::str(held));
      }
    }
    _ => {}
  }
}

/// `parseStringStyle`: declarations split on `;` outside parentheses, each
/// `name:value` trimmed.
fn parse_style(text: &str) -> Vec<(String, String)> {
  let mut out = Vec::new();
  let mut depth = 0usize;
  let mut start = 0;
  let bytes = text.as_bytes();
  let mut pieces: Vec<&str> = Vec::new();
  for (i, b) in bytes.iter().enumerate() {
    match b {
      b'(' => depth += 1,
      b')' => depth = depth.saturating_sub(1),
      b';' if depth == 0 => {
        pieces.push(&text[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  pieces.push(&text[start..]);
  for piece in pieces {
    let Some((name, value)) = piece.split_once(':') else { continue };
    let (name, value) = (name.trim(), value.trim());
    if !name.is_empty() {
      out.push((name.to_owned(), value.to_owned()));
    }
  }
  out
}

fn hyphenate(name: &str, out: &mut String) {
  for c in name.chars() {
    if c.is_ascii_uppercase() {
      out.push('-');
      out.push(c.to_ascii_lowercase());
    } else {
      out.push(c);
    }
  }
}

/// One attribute the way Vue's renderer prints it. `class` and `style` are
/// always written, normalised from whatever they hold. A boolean attribute is
/// bare when its value is truthy or the empty string. A `true` elsewhere is
/// a static attribute with no value, written bare; a string or a number is
/// written with its value; anything else writes nothing.
pub(super) fn attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  match name {
    "class" => {
      let text = match value {
        Value::Str(s) => s.to_string(),
        other => class_text(other)?,
      };
      write(name, &text, out);
      return Ok(());
    }
    "style" => {
      write(name, &style_text(value)?, out);
      return Ok(());
    }
    _ => {}
  }
  if is_boolean(name) {
    if truthy(value) || matches!(value, Value::Str(s) if s.is_empty()) {
      out.push(' ');
      out.push_str(name);
    }
    return Ok(());
  }
  match value {
    Value::Null | Value::Bool(false) => {}
    Value::Bool(true) => {
      out.push(' ');
      out.push_str(name);
    }
    Value::Str(s) => write(name, s, out),
    Value::Int(_) | Value::UInt(_) | Value::F32(_) | Value::F64(_) => write(name, &stringify(value)?, out),
    _ => {}
  }
  Ok(())
}

fn write(name: &str, text: &str, out: &mut String) {
  out.push(' ');
  out.push_str(name);
  out.push_str("=\"");
  escape(text, out);
  out.push('"');
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn class_follows_normalize_class() {
    let map = |pairs: &[(&str, bool)]| Value::Map(pairs.iter().map(|(k, v)| ((*k).to_owned(), Value::Bool(*v))).collect());
    assert_eq!(class_text(&Value::str("a  b")).unwrap(), "a  b");
    assert_eq!(class_text(&map(&[("on", true), ("off", false)])).unwrap(), "on");
    assert_eq!(class_text(&Value::seq(vec![map(&[("on", false)]), Value::str("btn")])).unwrap(), "btn");
    assert_eq!(class_text(&Value::seq(vec![map(&[("on", true)]), Value::str("btn")])).unwrap(), "on btn");
    assert_eq!(class_text(&Value::Null).unwrap(), "");
  }

  #[test]
  fn style_follows_stringify_style() {
    let map: ValueMap = [("fontSize".to_owned(), Value::Int(12)), ("--gap".to_owned(), Value::str("4px")), ("color".to_owned(), Value::Null)].into_iter().collect();
    assert_eq!(style_text(&Value::Map(map)).unwrap(), "font-size:12;--gap:4px;");
    assert_eq!(style_text(&Value::seq(vec![Value::str("color: red; background: url(a;b)"), Value::Map([("color".to_owned(), Value::str("blue"))].into_iter().collect())])).unwrap(), "color:blue;background:url(a;b);");
    assert_eq!(style_text(&Value::str("display:none")).unwrap(), "display:none");
  }

  #[test]
  fn an_interpolation_shows_what_to_display_string_shows() {
    assert_eq!(display(&Value::Null).unwrap(), "");
    assert_eq!(display(&Value::Bool(true)).unwrap(), "true");
    assert_eq!(display(&Value::F64(1.5)).unwrap(), "1.5");
    assert_eq!(display(&Value::seq(vec![Value::Int(1), Value::str("a")])).unwrap(), "[\n  1,\n  \"a\"\n]");
    let map: ValueMap = [("k".to_owned(), Value::Map(ValueMap::default()))].into_iter().collect();
    assert_eq!(display(&Value::Map(map)).unwrap(), "{\n  \"k\": {}\n}");
  }

  #[test]
  fn an_attribute_is_bare_valued_or_absent() {
    let printed = |name: &str, value: Value| {
      let mut out = String::new();
      attribute(name, &value, &mut out).unwrap();
      out
    };
    assert_eq!(printed("disabled", Value::Bool(true)), " disabled");
    assert_eq!(printed("disabled", Value::str("")), " disabled");
    assert_eq!(printed("disabled", Value::Bool(false)), "");
    assert_eq!(printed("title", Value::str("a \"b\" & 'c'")), " title=\"a &quot;b&quot; &amp; &#39;c&#39;\"");
    assert_eq!(printed("title", Value::Null), "");
    assert_eq!(printed("data-v-1", Value::Bool(true)), " data-v-1");
    assert_eq!(printed("tabindex", Value::Int(0)), " tabindex=\"0\"");
    assert_eq!(printed("class", Value::seq(vec![Value::Null])), " class=\"\"");
  }
}
