//! How Svelte's server runtime prints values, so a lowered Svelte component's
//! text and attributes are what Svelte's client hydrates over. Checked against
//! `svelte/src/escaping.js` and `svelte/src/internal/shared/attributes.js` at
//! v5.57.2. The markers around blocks are not here: the lowerer places them.

use snapfire_fsr_core::Value;

use crate::interp::{stringify, truthy, Fail};

/// A Svelte major the renderer writes markup for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum SvelteMajor {
  #[default]
  V5,
}

impl SvelteMajor {
  pub const ALL: [SvelteMajor; 1] = [Self::V5];

  /// The major of a version such as `5.57.2`; `None` for one the renderer has no rules for.
  pub fn of(version: &str) -> Option<Self> {
    match version.split('.').next()? {
      "5" => Some(Self::V5),
      _ => None,
    }
  }

  pub fn number(self) -> u32 {
    match self {
      Self::V5 => 5,
    }
  }
}

/// What `svelte/server`'s `render` writes around a component it mounts, which
/// `hydrate` looks for in the target.
pub const ROOT_OPEN: &str = "<!--[-->";
pub const ROOT_CLOSE: &str = "<!--]-->";

/// The compiler's `is_boolean_attribute` list, from `svelte/src/utils.js`.
const BOOLEAN: &[&str] = &["allowfullscreen", "async", "autofocus", "autoplay", "checked", "controls", "default", "disabled", "formnovalidate", "indeterminate", "inert", "ismap", "loop", "multiple", "muted", "nomodule", "novalidate", "open", "playsinline", "readonly", "required", "reversed", "seamless", "selected", "webkitdirectory", "defer", "disablepictureinpicture", "disableremoteplayback"];

pub(super) fn is_boolean(name: &str) -> bool {
  BOOLEAN.contains(&name)
}

/// `escape_html(value)`: content escapes `&` and `<`.
pub(super) fn escape_text(input: &str, out: &mut String) {
  escape(input, false, out);
}

/// `escape_html(value, true)`: an attribute also escapes `"`.
pub(super) fn escape_attr(input: &str, out: &mut String) {
  escape(input, true, out);
}

fn escape(input: &str, attr: bool, out: &mut String) {
  let mut last = 0;
  for (i, byte) in input.bytes().enumerate() {
    let replacement = match byte {
      b'&' => "&amp;",
      b'<' => "&lt;",
      b'"' if attr => "&quot;",
      _ => continue,
    };
    out.push_str(&input[last..i]);
    out.push_str(replacement);
    last = i + 1;
  }
  out.push_str(&input[last..]);
}

/// `String(value ?? '')`, what an expression tag shows.
pub(super) fn display(value: &Value) -> Result<String, Fail> {
  Ok(match value {
    Value::Null => String::new(),
    Value::Bool(b) => b.to_string(),
    Value::Seq(items) => items.iter().map(|item| match item {
      Value::Null => Ok(String::new()),
      other => display(other),
    }).collect::<Result<Vec<_>, _>>()?.join(","),
    Value::Map(_) => "[object Object]".to_owned(),
    other => stringify(other)?,
  })
}

/// `attr(name, value, is_boolean)`: `null` omits the attribute and so does a
/// falsy boolean other than `''`; `hidden` is boolean unless it is
/// `until-found`; `translate` writes `yes` and `no` for `true` and `false`.
/// `class` and `style` are written by `attr_class` and `attr_style`, which
/// omit an empty value.
pub(super) fn attribute(name: &str, value: &Value, out: &mut String) -> Result<(), Fail> {
  if (name == "class" || name == "style") && matches!(value, Value::Str(s) if s.is_empty()) {
    return Ok(());
  }
  attr(name, value, is_boolean(name), out)
}

/// `attr(name, value, is_boolean)`.
fn attr(name: &str, value: &Value, boolean: bool, out: &mut String) -> Result<(), Fail> {
  let boolean = boolean || (name == "hidden" && !matches!(value, Value::Str(s) if s.as_ref() == "until-found"));
  let empty = matches!(value, Value::Str(s) if s.is_empty());
  if matches!(value, Value::Null) || (boolean && !truthy(value) && !empty) {
    return Ok(());
  }
  out.push(' ');
  out.push_str(name);
  if boolean {
    out.push_str("=\"\"");
    return Ok(());
  }
  let text = match (name, value) {
    ("translate", Value::Bool(true)) => "yes".to_owned(),
    ("translate", Value::Bool(false)) => "no".to_owned(),
    _ => display(value)?,
  };
  out.push_str("=\"");
  escape_attr(&text, out);
  out.push('"');
  Ok(())
}

/// One name of the object `$.attributes` writes: a `$$` name, one Svelte
/// counts invalid and an `on` handler are left out; an `<input>`'s
/// `defaultValue` and `defaultChecked` stand for `value` and `checked` unless
/// the object holds those. `names` is every name the object holds.
pub(super) fn spread_attribute(name: &str, value: &Value, flags: u32, names: &[String], out: &mut String) -> Result<(), Fail> {
  if name.starts_with("$$") || name.is_empty() || name.chars().any(invalid_name_char) {
    return Ok(());
  }
  let lower = name.to_lowercase();
  let mut name = if flags & PRESERVE_CASE == 0 { lower.clone() } else { name.to_owned() };
  if lower.encode_utf16().count() > 2 && lower.starts_with("on") {
    return Ok(());
  }
  if flags & IS_INPUT != 0 && (name == "defaultvalue" || name == "defaultchecked") {
    name = if name == "defaultvalue" { "value" } else { "checked" }.to_owned();
    if names.iter().any(|n| n.to_lowercase() == name) {
      return Ok(());
    }
  }
  attr(&name, value, flags & NAMESPACED == 0 && is_boolean(&name), out)
}

pub(super) const DYNAMIC_ELEMENT: u32 = 8;
const NAMESPACED: u32 = 1;
const PRESERVE_CASE: u32 = 2;
const IS_INPUT: u32 = 4;

/// `INVALID_ATTR_NAME_CHAR_REGEX`: whitespace, `'`, `"`, `>`, `/`, `=` and the noncharacters.
fn invalid_name_char(c: char) -> bool {
  let code = c as u32;
  matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}') || matches!(c, '\'' | '"' | '>' | '/' | '=') || (0xfdd0..=0xfdef).contains(&code) || (code & 0xfffe) == 0xfffe
}
