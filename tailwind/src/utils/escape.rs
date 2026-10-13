/// `CSS.escape`, https://drafts.csswg.org/cssom/#serialize-an-identifier
pub fn escape(value: &str) -> String {
  let mut chars = value.chars();
  let first = value.chars().next();

  if value.len() == 1 && first == Some('-') {
    return format!("\\{value}");
  }

  let mut result = String::with_capacity(value.len() + 4);
  let mut index = 0usize;
  for c in chars.by_ref() {
    let code = c as u32;
    if code == 0 {
      result.push('\u{FFFD}');
    } else if (0x01..=0x1f).contains(&code)
      || code == 0x7f
      || (index == 0 && c.is_ascii_digit())
      || (index == 1 && c.is_ascii_digit() && first == Some('-'))
    {
      result.push('\\');
      result.push_str(&format!("{code:x}"));
      result.push(' ');
    } else if code >= 0x80 || c == '-' || c == '_' || c.is_ascii_alphanumeric() {
      result.push(c);
    } else {
      result.push('\\');
      result.push(c);
    }
    index += c.len_utf16();
  }
  result
}

/// Reverses CSS escapes: `\:` to `:` and `\31 ` to `1`.
pub fn unescape(escaped: &str) -> String {
  if !escaped.contains('\\') {
    return escaped.to_owned();
  }

  let mut out = String::with_capacity(escaped.len());
  let mut chars = escaped.char_indices().peekable();
  while let Some((_, c)) = chars.next() {
    if c != '\\' {
      out.push(c);
      continue;
    }
    let Some(&(_, next)) = chars.peek() else {
      out.push('\\');
      break;
    };
    if next.is_ascii_hexdigit() {
      let mut hex = String::new();
      while hex.len() < 6 {
        match chars.peek() {
          Some(&(_, h)) if h.is_ascii_hexdigit() => {
            hex.push(h);
            chars.next();
          }
          _ => break,
        }
      }
      let mut spaced = false;
      if let Some(&(_, ws)) = chars.peek()
        && matches!(ws, '\t' | '\n' | '\u{000C}' | '\r' | ' ')
      {
        chars.next();
        spaced = true;
      }
      if hex.len() == 1 && !spaced {
        out.push_str(&hex);
        continue;
      }
      let code = u32::from_str_radix(&hex, 16).unwrap_or(0);
      if code == 0 || code > 0x10ffff || (0xd800..=0xdfff).contains(&code) {
        out.push('\u{FFFD}');
      } else {
        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
      }
    } else {
      out.push(next);
      chars.next();
    }
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn escapes_like_css_escape() {
    assert_eq!(escape("hover:underline"), "hover\\:underline");
    assert_eq!(escape("w-1/2"), "w-1\\/2");
    assert_eq!(escape("2xl:flex"), "\\32 xl\\:flex");
    assert_eq!(escape("-"), "\\-");
    assert_eq!(escape("-1"), "-\\31 ");
    assert_eq!(escape("bg-[#0088cc]"), "bg-\\[\\#0088cc\\]");
  }

  #[test]
  fn unescapes() {
    assert_eq!(unescape("hover\\:underline"), "hover:underline");
    assert_eq!(unescape("\\32 xl"), "2xl");
    assert_eq!(unescape("foo-1\\/2"), "foo-1/2");
  }
}
