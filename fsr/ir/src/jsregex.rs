//! JavaScript regular expressions over the `regex` crate. A pattern is
//! translated so the classes mean what they mean in JavaScript (`\d`, `\w`
//! and `\b` are ASCII, `.` stops at every line terminator) and what the crate
//! cannot match the same way, backreferences, lookaround and the sticky flag,
//! is refused with its name.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use parking_lot::Mutex;
use regex::Regex;
use snapfire_fsr_core::{Value, ValueMap};

/// The field a lowered regular expression keeps its source under.
pub const PATTERN: &str = "$regex";
/// The field it keeps its flags under.
pub const FLAGS: &str = "$flags";

/// A regular expression as a value: its source and flags.
pub fn value(pattern: &str, flags: &str) -> Value {
  let mut map = ValueMap::default();
  map.insert(PATTERN.to_owned(), Value::str(pattern));
  map.insert(FLAGS.to_owned(), Value::str(flags));
  Value::Map(map)
}

/// The source and flags of a value that is a regular expression.
pub fn of(value: &Value) -> Option<(&str, &str)> {
  let Value::Map(map) = value else { return None };
  match (map.get(PATTERN), map.get(FLAGS)) {
    (Some(Value::Str(pattern)), Some(Value::Str(flags))) => Some((pattern, flags)),
    _ => None,
  }
}

/// The pattern in the crate's syntax, or why it cannot be one.
pub fn translate(pattern: &str, flags: &str) -> Result<String, String> {
  let mut out = String::with_capacity(pattern.len() + 16);
  let mut dot_all = false;
  for flag in flags.chars() {
    match flag {
      'g' | 'd' | 'u' => {}
      'i' => out.push_str("(?i)"),
      'm' => out.push_str("(?m)"),
      's' => {
        out.push_str("(?s)");
        dot_all = true;
      }
      'y' => return Err("the sticky flag `y`".to_owned()),
      'v' => return Err("the `v` flag".to_owned()),
      other => return Err(format!("the flag `{other}`")),
    }
  }
  let chars: Vec<char> = pattern.chars().collect();
  let mut in_class = false;
  let mut i = 0;
  while i < chars.len() {
    let c = chars[i];
    match c {
      '\\' => {
        let Some(&next) = chars.get(i + 1) else { return Err("a pattern ending in `\\`".to_owned()) };
        i += 1;
        match next {
          'd' => out.push_str(if in_class { "0-9" } else { "[0-9]" }),
          'w' => out.push_str(if in_class { "A-Za-z0-9_" } else { "[A-Za-z0-9_]" }),
          'D' | 'W' if in_class => return Err(format!("`\\{next}` inside a class")),
          'D' => out.push_str("[^0-9]"),
          'W' => out.push_str("[^A-Za-z0-9_]"),
          'b' if in_class => out.push_str("\\x08"),
          'b' => out.push_str("(?-u:\\b)"),
          'B' => out.push_str("(?-u:\\B)"),
          '1'..='9' => return Err("a backreference".to_owned()),
          'k' => return Err("a named backreference".to_owned()),
          '0' => out.push_str("\\x00"),
          'c' => {
            let Some(letter) = chars.get(i + 1).filter(|l| l.is_ascii_alphabetic()) else { return Err("`\\c` without a letter".to_owned()) };
            out.push_str(&format!("\\x{:02X}", (*letter as u8) % 32));
            i += 1;
          }
          's' | 'S' | 't' | 'n' | 'r' | 'v' | 'f' | 'x' | 'u' => {
            out.push('\\');
            out.push(next);
          }
          other if other.is_ascii_alphanumeric() => out.push(other),
          other => {
            out.push('\\');
            out.push(other);
          }
        }
      }
      '[' if in_class => out.push_str("\\["),
      '[' => {
        match (chars.get(i + 1), chars.get(i + 2)) {
          (Some(']'), _) => {
            out.push_str("[^\\x00-\\x{10FFFF}]");
            i += 2;
            continue;
          }
          (Some('^'), Some(']')) => {
            out.push_str("[\\x00-\\x{10FFFF}]");
            i += 3;
            continue;
          }
          _ => {}
        }
        in_class = true;
        out.push('[');
        if chars.get(i + 1) == Some(&'^') {
          out.push('^');
          i += 1;
        }
      }
      ']' if in_class => {
        in_class = false;
        out.push(']');
      }
      '&' | '~' if in_class => {
        out.push('\\');
        out.push(c);
      }
      '.' if !in_class && !dot_all => out.push_str("[^\\n\\r\\x{2028}\\x{2029}]"),
      '(' if !in_class && chars.get(i + 1) == Some(&'?') => match (chars.get(i + 2), chars.get(i + 3)) {
        (Some('='), _) | (Some('!'), _) => return Err("a lookahead".to_owned()),
        (Some('<'), Some('=')) | (Some('<'), Some('!')) => return Err("a lookbehind".to_owned()),
        _ => out.push('('),
      },
      other => out.push(other),
    }
    i += 1;
  }
  Ok(out)
}

fn cache() -> &'static Mutex<HashMap<(String, String), Arc<Regex>>> {
  static CACHE: OnceLock<Mutex<HashMap<(String, String), Arc<Regex>>>> = OnceLock::new();
  CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The compiled expression, built once per source and flags.
pub fn compiled(pattern: &str, flags: &str) -> Result<Arc<Regex>, String> {
  let key = (pattern.to_owned(), flags.to_owned());
  if let Some(found) = cache().lock().get(&key) {
    return Ok(found.clone());
  }
  let translated = translate(pattern, flags)?;
  let regex = Arc::new(Regex::new(&translated).map_err(|e| format!("a pattern the build cannot match as JavaScript does: {e}"))?);
  let mut held = cache().lock();
  if held.len() >= 512 {
    held.clear();
  }
  held.insert(key, regex.clone());
  Ok(regex)
}

/// A UTF-8 byte offset into `s` as the UTF-16 index JavaScript reports.
pub fn utf16_index(s: &str, byte: usize) -> usize {
  s[..byte].encode_utf16().count()
}

/// `replace`'s replacement string for one match: `$$`, `$&`, `` $` ``, `$'`, `$1` to `$99` and `$<name>`, a group that did not take part as nothing.
pub fn expand(out: &mut String, replacement: &str, subject: &str, caps: &regex::Captures<'_>, names: bool) {
  let whole = caps.get(0).expect("a match has its whole");
  let bytes = replacement.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != b'$' || i + 1 >= bytes.len() {
      let ch = replacement[i..].chars().next().expect("in bounds");
      out.push(ch);
      i += ch.len_utf8();
      continue;
    }
    match bytes[i + 1] {
      b'$' => {
        out.push('$');
        i += 2;
      }
      b'&' => {
        out.push_str(whole.as_str());
        i += 2;
      }
      b'`' => {
        out.push_str(&subject[..whole.start()]);
        i += 2;
      }
      b'\'' => {
        out.push_str(&subject[whole.end()..]);
        i += 2;
      }
      b'0'..=b'9' => {
        let one = (bytes[i + 1] - b'0') as usize;
        let two = bytes.get(i + 2).filter(|b| b.is_ascii_digit()).map(|b| one * 10 + (b - b'0') as usize);
        match two.filter(|n| *n >= 1 && *n < caps.len()) {
          Some(n) => {
            out.push_str(caps.get(n).map_or("", |m| m.as_str()));
            i += 3;
          }
          None if one >= 1 && one < caps.len() => {
            out.push_str(caps.get(one).map_or("", |m| m.as_str()));
            i += 2;
          }
          None => {
            out.push('$');
            i += 1;
          }
        }
      }
      b'<' if names => match replacement[i + 2..].find('>') {
        Some(end) => {
          let name = &replacement[i + 2..i + 2 + end];
          out.push_str(caps.name(name).map_or("", |m| m.as_str()));
          i += 3 + end;
        }
        None => {
          out.push('$');
          i += 1;
        }
      },
      _ => {
        out.push('$');
        i += 1;
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_classes_mean_what_they_mean_in_javascript() {
    let re = compiled(r"\d+\w*", "").unwrap();
    assert!(!re.is_match("٣"), "an Arabic-Indic digit is no `\\d`");
    assert!(re.is_match("3x"));
    assert!(!compiled("a.b", "").unwrap().is_match("a\u{2028}b"), "`.` stops at a line terminator");
    assert!(compiled("a.b", "s").unwrap().is_match("a\nb"));
    assert!(compiled("[]a", "").is_ok() && !compiled("[]a", "").unwrap().is_match("a"), "`[]` matches nothing");
    assert!(compiled("[[a]", "").unwrap().is_match("["), "`[` inside a class is itself");
  }

  #[test]
  fn what_the_crate_cannot_match_alike_is_refused_by_name() {
    assert_eq!(translate(r"(a)\1", "").unwrap_err(), "a backreference");
    assert_eq!(translate("a(?=b)", "").unwrap_err(), "a lookahead");
    assert_eq!(translate("(?<!a)b", "").unwrap_err(), "a lookbehind");
    assert_eq!(translate("a", "y").unwrap_err(), "the sticky flag `y`");
  }
}
