/// `text` as JSON after taking out what `tsconfig.json` allows beyond it:
/// `//` and `/* */` comments and a comma before a closing bracket.
pub(crate) fn lenient_json(text: &str) -> Option<serde_json::Value> {
  let bytes = text.as_bytes();
  let mut out = String::with_capacity(text.len());
  let mut i = 0;
  let mut in_string = false;
  while i < bytes.len() {
    let c = bytes[i];
    if in_string {
      if c == b'\\' && i + 1 < bytes.len() {
        out.push_str(&text[i..i + 2]);
        i += 2;
        continue;
      }
      if c == b'"' {
        in_string = false;
      }
      let len = utf8_len(c);
      out.push_str(&text[i..i + len]);
      i += len;
      continue;
    }
    match c {
      b'"' => {
        in_string = true;
        out.push('"');
        i += 1;
      }
      b'/' if bytes.get(i + 1) == Some(&b'/') => {
        while i < bytes.len() && bytes[i] != b'\n' {
          i += 1;
        }
      }
      b'/' if bytes.get(i + 1) == Some(&b'*') => {
        i += 2;
        while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
      }
      b',' => {
        let next = past_comments(text, i + 1);
        if !matches!(next, Some('}') | Some(']')) {
          out.push(',');
        }
        i += 1;
      }
      _ => {
        let len = utf8_len(c);
        out.push_str(&text[i..i + len]);
        i += len;
      }
    }
  }
  serde_json::from_str(&out).ok()
}

/// The first character at or after `from` that is neither whitespace nor inside a comment.
fn past_comments(text: &str, from: usize) -> Option<char> {
  let bytes = text.as_bytes();
  let mut i = from;
  loop {
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
      i += 1;
    }
    match (bytes.get(i), bytes.get(i + 1)) {
      (Some(b'/'), Some(b'/')) => {
        while i < bytes.len() && bytes[i] != b'\n' {
          i += 1;
        }
      }
      (Some(b'/'), Some(b'*')) => {
        i += 2;
        while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
          i += 1;
        }
        i += 2;
      }
      _ => return text.get(i..).and_then(|rest| rest.chars().next()),
    }
  }
}

fn utf8_len(first: u8) -> usize {
  match first {
    0xF0..=0xFF => 4,
    0xE0..=0xEF => 3,
    0xC0..=0xDF => 2,
    _ => 1,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn comments_and_trailing_commas_are_taken_out_but_not_from_strings() {
    let value = lenient_json("{\n  // a comment\n  \"a\": \"//not\", /* b */\n  \"b\": [1, 2,],\n}").unwrap();
    assert_eq!(value, serde_json::json!({ "a": "//not", "b": [1, 2] }));
  }

  #[test]
  fn a_trailing_comma_before_a_comment_is_taken_out() {
    let value = lenient_json("{\n  \"a\": 1,\n  // \"b\": 2\n}").unwrap();
    assert_eq!(value, serde_json::json!({ "a": 1 }));
  }
}
