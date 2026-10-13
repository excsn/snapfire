use crate::utils::math::add_whitespace_around_math_operators;
use crate::value_parser::{self, ValueNode};

/// Turns the inside of `[…]` into CSS: `_` becomes a space, `\_` an underscore, except in
/// `url()` and the name of a `var()`.
pub fn decode_arbitrary_value(input: &str) -> String {
  if !input.contains('(') {
    return convert_underscores(input, false);
  }

  let mut ast = value_parser::parse(input);
  decode_nodes(&mut ast);
  add_whitespace_around_math_operators(&value_parser::to_css(&ast))
}

fn convert_underscores(input: &str, keep_underscores: bool) -> String {
  let mut out = String::with_capacity(input.len());
  let mut chars = input.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '\\' && chars.peek() == Some(&'_') {
      out.push('_');
      chars.next();
    } else if c == '_' && !keep_underscores {
      out.push(' ');
    } else {
      out.push(c);
    }
  }
  out
}

fn decode_nodes(ast: &mut [ValueNode]) {
  for node in ast {
    match node {
      ValueNode::Function { value, nodes } => {
        if value == "url" || value.ends_with("_url") {
          *value = convert_underscores(value, false);
          continue;
        }
        if value == "var" || value.ends_with("_var") || value == "theme" || value.ends_with("_theme") {
          *value = convert_underscores(value, false);
          for (i, child) in nodes.iter_mut().enumerate() {
            if i == 0
              && let ValueNode::Word(w) = child
            {
              *w = convert_underscores(w, true);
              continue;
            }
            decode_nodes(std::slice::from_mut(child));
          }
          continue;
        }
        *value = convert_underscores(value, false);
        decode_nodes(nodes);
      }
      ValueNode::Separator(v) | ValueNode::Word(v) => *v = convert_underscores(v, false),
    }
  }
}

/// Whether `input` might be a valid arbitrary value: balanced `()` and `[]`, no unmatched
/// closer and no top-level `;`.
pub fn is_valid_arbitrary(input: &str) -> bool {
  let bytes = input.as_bytes();
  let mut stack: Vec<u8> = Vec::new();
  let len = bytes.len();
  let mut idx = 0;
  while idx < len {
    let c = bytes[idx];
    match c {
      b'\\' => idx += 1,
      b'\'' | b'"' => {
        idx += 1;
        while idx < len {
          let next = bytes[idx];
          if next == b'\\' {
            idx += 2;
            continue;
          }
          if next == c {
            break;
          }
          idx += 1;
        }
      }
      b'(' => stack.push(b')'),
      b'[' => stack.push(b']'),
      b']' | b'}' | b')' => {
        if stack.is_empty() {
          return false;
        }
        if stack.last() == Some(&c) {
          stack.pop();
        }
      }
      b';' if stack.is_empty() => return false,
      _ => {}
    }
    idx += 1;
  }
  true
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decodes() {
    assert_eq!(decode_arbitrary_value("1px_solid_red"), "1px solid red");
    assert_eq!(decode_arbitrary_value("a\\_b"), "a_b");
    assert_eq!(decode_arbitrary_value("var(--my_var,1px_2px)"), "var(--my_var,1px 2px)");
    assert_eq!(decode_arbitrary_value("url(/a_b.png)"), "url(/a_b.png)");
    assert_eq!(decode_arbitrary_value("calc(100%-1rem)"), "calc(100% - 1rem)");
  }

  #[test]
  fn validates() {
    assert!(is_valid_arbitrary("calc(1px)"));
    assert!(!is_valid_arbitrary("a;b"));
    assert!(!is_valid_arbitrary("a)"));
    assert!(is_valid_arbitrary("'a;b'"));
  }
}
