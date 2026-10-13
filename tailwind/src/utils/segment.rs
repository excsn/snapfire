/// Splits `input` on `separator` where it is not inside parentheses, brackets, braces or a string.
pub fn segment(input: &str, separator: u8) -> Vec<&str> {
  let bytes = input.as_bytes();
  let mut stack: Vec<u8> = Vec::new();
  let mut parts = Vec::new();
  let mut last = 0;
  let len = bytes.len();
  let mut idx = 0;

  while idx < len {
    let char = bytes[idx];

    if stack.is_empty() && char == separator {
      parts.push(&input[last..idx]);
      last = idx + 1;
      idx += 1;
      continue;
    }

    match char {
      b'\\' => idx += 1,
      b'\'' | b'"' => {
        idx += 1;
        while idx < len {
          let next = bytes[idx];
          if next == b'\\' {
            idx += 2;
            continue;
          }
          if next == char {
            break;
          }
          idx += 1;
        }
      }
      b'(' => stack.push(b')'),
      b'[' => stack.push(b']'),
      b'{' => stack.push(b'}'),
      b']' | b'}' | b')' if stack.last() == Some(&char) => {
        stack.pop();
      }
      _ => {}
    }
    idx += 1;
  }

  parts.push(&input[last.min(len)..]);
  parts
}

#[cfg(test)]
mod tests {
  use super::segment;

  #[test]
  fn splits_at_the_top_level() {
    assert_eq!(
      segment("var(--a, 0 0 1px rgb(0, 0, 0)), 0 0 1px rgb(0, 0, 0)", b','),
      vec!["var(--a, 0 0 1px rgb(0, 0, 0))", " 0 0 1px rgb(0, 0, 0)"]
    );
    assert_eq!(segment("a:b:c", b':'), vec!["a", "b", "c"]);
    assert_eq!(segment("[a:b]:c", b':'), vec!["[a:b]", "c"]);
    assert_eq!(segment("", b':'), vec![""]);
    assert_eq!(segment("a\\:b:c", b':'), vec!["a\\:b", "c"]);
  }
}
