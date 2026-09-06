//! `text`: slugs and truncation. Both count characters as code points, the
//! way `Array.from(s)` does in the browser half.

use snapfire_fsr_core::Value;
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::UnicodeNormalization;

use crate::ext::{number, text, text_opt, Ambient, Extensions, Reach};
use crate::interp::Fail;

pub fn register(extensions: &mut Extensions) {
  extensions.register("text.slug", Reach::Render, |_, args| Ok(Value::Str(slug(text("text.slug", args, 0)?))));
  extensions.register("text.truncate", Reach::Render, truncate);
}

/// Decomposed, marks dropped, lowercased, every run outside `a-z0-9` one
/// hyphen and no hyphen at either end.
pub fn slug(s: &str) -> String {
  let mut out = String::with_capacity(s.len());
  let mut pending = false;
  for c in s.nfd().filter(|c| !is_combining_mark(*c)).flat_map(char::to_lowercase) {
    if c.is_ascii_alphanumeric() {
      if pending && !out.is_empty() {
        out.push('-');
      }
      pending = false;
      out.push(c);
    } else {
      pending = true;
    }
  }
  out
}

/// `text.truncate(s, max, ellipsis = "…")`: the first `max` characters and
/// the ellipsis when the string is longer, else the string.
fn truncate(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let what = "text.truncate";
  let s = text(what, args, 0)?;
  let max = number(what, args, 1)?.max(0.0) as usize;
  let ellipsis = text_opt(what, args, 2)?.unwrap_or("…");
  let count = s.chars().count();
  if count <= max {
    return Ok(Value::Str(s.to_owned()));
  }
  let mut out: String = s.chars().take(max).collect();
  out.push_str(ellipsis);
  Ok(Value::Str(out))
}
