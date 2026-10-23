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
    Value::Null | Value::Undefined => String::new(),
    Value::Bool(b) => b.to_string(),
    Value::Seq(items) => items.iter().map(|item| match item {
      Value::Null | Value::Undefined => Ok(String::new()),
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
  if matches!(value, Value::Null | Value::Undefined) || (boolean && !truthy(value) && !empty) {
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

/// How the Svelte adapter's `Pieces.svelte` takes a nested island: in its tree, as a root of its own mounted later or as one another adapter mounts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Nested {
  InTree,
  Root,
  Opaque,
}

/// A Svelte island's children as `Pieces.svelte` writes them under `svelte/server`, so its adapter hydrates them in the island's own tree: the list between block markers, each item in the branch of its kind. A Svelte island among them that the server rendered at load or that the browser renders alone is an item of the tree (branch 0). An element holding one is rebuilt in the tree around it (branch 1). Any other element, a Svelte island timed to hydrate later included, is kept as the server wrote it (branch 2). A run of text and comments is `{@html}` (the else branch). The bodies of the nested islands are wrapped to match.
pub(super) fn tree(content: &str, islands: &mut [super::RenderedIsland], library: &super::Components) -> String {
  let kinds: Vec<Nested> = islands.iter().map(|island| nested(island, library)).collect();
  for (island, kind) in islands.iter_mut().zip(&kinds) {
    let html = std::mem::take(&mut island.body.html);
    island.body.html = match (kind, island.rendered) {
      (Nested::InTree, true) => wrapped(&html, "<!--[0--><!--[-->", "<!--]--><!--]-->"),
      (Nested::InTree, false) => match html.strip_prefix(super::CHILDREN_OPEN).and_then(|rest| rest.strip_suffix("</sf-s>")) {
        Some(region) => format!("<!--[1--><template data-sf-children>{region}</template><!--]-->"),
        None => "<!--[-1--><!--]-->".to_owned(),
      },
      (Nested::Root, _) => wrapped(&html, ROOT_OPEN, ROOT_CLOSE),
      (Nested::Opaque, _) => html,
    };
  }
  let mut out = String::new();
  list(content, &kinds, &mut out);
  out
}

fn nested(island: &super::RenderedIsland, library: &super::Components) -> Nested {
  if super::island_owner(&island.module, library) != Some(crate::ast::Owner::Svelte) || island.mode.is_some() {
    return Nested::Opaque;
  }
  if island.rendered {
    return match island.when.as_deref() {
      None | Some("load") => Nested::InTree,
      Some(_) => Nested::Root,
    };
  }
  let html = island.body.html.as_str();
  let whole_region = html.strip_prefix(super::CHILDREN_OPEN).is_some() && token(html, 0).1 == html.len();
  match html.is_empty() || whole_region {
    true => Nested::InTree,
    false => Nested::Opaque,
  }
}

/// `body` between `open` and `close`, a held children template after its markup staying after them.
fn wrapped(body: &str, open: &str, close: &str) -> String {
  let (markup, held) = match body.rfind(super::CHILDREN_HELD_OPEN) {
    Some(at) if body.ends_with("</template>") => body.split_at(at),
    _ => (body, ""),
  };
  format!("{open}{markup}{close}{held}")
}

fn list(content: &str, kinds: &[Nested], out: &mut String) {
  out.push_str(BLOCK_OPEN);
  let mut run = 0;
  let mut at = 0;
  while at < content.len() {
    let (kind, end) = token(content, at);
    match kind {
      Token::Run => {}
      Token::Island(index) => {
        flush(&content[run..at], out);
        match kinds.get(index).copied().unwrap_or(Nested::Opaque) {
          Nested::InTree => {
            out.push_str("<!--[0-->");
            out.push_str(&content[at..end]);
            out.push_str(BLOCK_CLOSE);
          }
          _ => opaque(&content[at..end], out),
        }
        run = end;
      }
      Token::Element { open_end, close_start } => {
        flush(&content[run..at], out);
        let inner = &content[open_end..close_start];
        if holds(inner, kinds) {
          out.push_str("<!--[1-->");
          out.push_str(EMPTY);
          out.push_str(&content[at..open_end]);
          list(inner, kinds, out);
          out.push_str(&content[close_start..end]);
          out.push_str(EMPTY);
          out.push_str(BLOCK_CLOSE);
        } else {
          opaque(&content[at..end], out);
        }
        run = end;
      }
    }
    at = end;
  }
  flush(&content[run..], out);
  out.push_str(BLOCK_CLOSE);
  out.push_str(EMPTY);
}

fn opaque(element: &str, out: &mut String) {
  out.push_str("<!--[2-->");
  out.push_str(EMPTY);
  out.push_str(element);
  out.push_str(EMPTY);
  out.push_str(BLOCK_CLOSE);
}

/// A run of text, comments and marks: `{@html}` when it writes a node, as it stands when it holds only marks.
fn flush(run: &str, out: &mut String) {
  if run.is_empty() {
    return;
  }
  let mut visible = false;
  let mut rest = run;
  while let Some(start) = rest.find('\u{0}') {
    visible |= start > 0;
    let end = rest[start + 1..].find('\u{0}').map_or(rest.len(), |e| start + e + 2);
    rest = &rest[end..];
  }
  visible |= !rest.is_empty();
  if !visible {
    out.push_str(run);
    return;
  }
  out.push_str("<!--[-1-->");
  out.push_str(EMPTY);
  out.push_str(run);
  out.push_str(EMPTY);
  out.push_str(BLOCK_CLOSE);
}

/// Whether markup holds a Svelte island its adapter renders in the tree.
fn holds(markup: &str, kinds: &[Nested]) -> bool {
  let mut rest = markup;
  while let Some(at) = rest.find(super::ISLAND_MARK) {
    let after = &rest[at + super::ISLAND_MARK.len()..];
    let end = after.find('\u{0}').unwrap_or(after.len());
    if after[..end].parse::<usize>().ok().and_then(|index| kinds.get(index)) == Some(&Nested::InTree) {
      return true;
    }
    rest = &after[end..];
  }
  false
}

const BLOCK_OPEN: &str = "<!--[-->";
const BLOCK_CLOSE: &str = "<!--]-->";
const EMPTY: &str = "<!---->";

enum Token {
  /// Text, a comment or a mark other than an island's.
  Run,
  Island(usize),
  Element { open_end: usize, close_start: usize },
}

/// The node of the markup the renderer wrote that starts at `at` and where it ends.
fn token(s: &str, at: usize) -> (Token, usize) {
  let rest = &s[at..];
  if rest.starts_with('\u{0}') {
    let end = rest[1..].find('\u{0}').map_or(s.len(), |e| at + e + 2);
    let index = rest.strip_prefix(super::ISLAND_MARK).and_then(|r| r[..r.find('\u{0}').unwrap_or(r.len())].parse::<usize>().ok());
    return (index.map_or(Token::Run, Token::Island), end);
  }
  if rest.starts_with("<!--") {
    return (Token::Run, rest.find("-->").map_or(s.len(), |e| at + e + 3));
  }
  let starts_tag = rest.starts_with('<') && rest[1..].starts_with(|c: char| c.is_ascii_alphabetic());
  if !starts_tag {
    let first = rest.chars().next().map_or(1, char::len_utf8);
    let next = rest[first..].find(['<', '\u{0}']).map_or(s.len(), |e| at + e + first);
    return (Token::Run, next);
  }
  let name_end = rest[1..].find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').map_or(rest.len(), |e| e + 1);
  let tag = rest[1..name_end].to_ascii_lowercase();
  let mut quote = None;
  let mut open_end = s.len();
  for (i, c) in rest.char_indices().skip(name_end) {
    match (quote, c) {
      (Some(q), c) if c == q => quote = None,
      (Some(_), _) => {}
      (None, '"' | '\'') => quote = Some(c),
      (None, '>') => {
        open_end = at + i + 1;
        break;
      }
      _ => {}
    }
  }
  if super::VOID.contains(&tag.as_str()) || s[..open_end].ends_with("/>") {
    return (Token::Element { open_end, close_start: open_end }, open_end);
  }
  let close = format!("</{tag}");
  if matches!(tag.as_str(), "script" | "style" | "textarea" | "title") {
    let close_start = s[open_end..].to_ascii_lowercase().find(&close).map_or(s.len(), |e| open_end + e);
    let end = s[close_start..].find('>').map_or(s.len(), |e| close_start + e + 1);
    return (Token::Element { open_end, close_start }, end);
  }
  let mut at = open_end;
  while at < s.len() {
    if s[at..].starts_with("</") {
      let end = s[at..].find('>').map_or(s.len(), |e| at + e + 1);
      return (Token::Element { open_end, close_start: at }, end);
    }
    at = token(s, at).1;
  }
  (Token::Element { open_end, close_start: s.len() }, s.len())
}
