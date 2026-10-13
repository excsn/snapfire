//! Tailwind's CSS parser: rules, at-rules, declarations and license comments, with
//! whitespace collapsed outside custom properties.

use crate::Error;
use crate::ast::{Node, at_rule, comment, rule};
use crate::value_parser::next_char_end;

pub fn parse(input: &str) -> Result<Vec<Node>, Error> {
  let owned;
  let input = if let Some(rest) = input.strip_prefix('\u{FEFF}') {
    owned = format!(" {rest}");
    owned.as_str()
  } else {
    input
  };
  let bytes = input.as_bytes();
  let len = bytes.len();
  let at = |i: usize| -> u8 { if i < len { bytes[i] } else { 0 } };

  let mut ast: Vec<Node> = Vec::new();
  let mut license_comments: Vec<Node> = Vec::new();
  let mut stack: Vec<Node> = Vec::new();
  let mut buffer = String::new();
  let mut closing = String::new();

  let mut i = 0;
  while i < len {
    let c = bytes[i];

    if c == b'\r' && at(i + 1) == b'\n' {
      i += 1;
      continue;
    }

    if c == b'\\' {
      let end = next_char_end(input, i + 1);
      buffer.push_str(&input[i..end]);
      i = end;
      continue;
    } else if c == b'/' && at(i + 1) == b'*' {
      let start = i;
      let mut j = i + 2;
      while j < len {
        let p = bytes[j];
        if p == b'\\' {
          j += 2;
          continue;
        }
        if p == b'*' && at(j + 1) == b'/' {
          i = j + 1;
          break;
        }
        j += 1;
      }
      let comment_string = &input[start..next_char_end(input, i)];
      if comment_string.as_bytes().get(2) == Some(&b'!') && comment_string.len() >= 4 {
        license_comments.push(comment(&comment_string[2..comment_string.len() - 2]));
      }
    } else if c == b'\'' || c == b'"' {
      let end = parse_string(input, i, c)?;
      buffer.push_str(&input[i..next_char_end(input, end)]);
      i = end;
    } else if (c == b' ' || c == b'\n' || c == b'\t')
      && (matches!(at(i + 1), b' ' | b'\n' | b'\t') || (at(i + 1) == b'\r' && at(i + 2) == b'\n'))
    {
      i += 1;
      continue;
    } else if c == b'\n' {
      if buffer.is_empty() {
        i += 1;
        continue;
      }
      let last = *buffer.as_bytes().last().unwrap();
      if last != b' ' && last != b'\n' && last != b'\t' {
        buffer.push(' ');
      }
    } else if c == b'-' && at(i + 1) == b'-' && buffer.is_empty() {
      let mut nested = String::new();
      let start = i;
      let mut colon: Option<usize> = None;
      let mut j = i + 2;
      while j < len {
        let p = bytes[j];
        if p == b'\\' {
          j += 2;
          continue;
        } else if p == b'\'' || p == b'"' {
          j = parse_string(input, j, p)?;
        } else if p == b'/' && at(j + 1) == b'*' {
          let mut k = j + 2;
          while k < len {
            let q = bytes[k];
            if q == b'\\' {
              k += 2;
              continue;
            }
            if q == b'*' && at(k + 1) == b'/' {
              j = k + 1;
              break;
            }
            k += 1;
          }
        } else if colon.is_none() && p == b':' {
          colon = Some(buffer.len() + j - start);
        } else if p == b';' && nested.is_empty() {
          buffer.push_str(&input[start..j]);
          i = j;
          break;
        } else if p == b'(' {
          nested.push(')');
        } else if p == b'[' {
          nested.push(']');
        } else if p == b'{' {
          nested.push('}');
        } else if (p == b'}' || len - 1 == j) && nested.is_empty() {
          i = j - 1;
          buffer.push_str(&input[start..j]);
          break;
        } else if (p == b')' || p == b']' || p == b'}') && nested.as_bytes().last() == Some(&p) {
          nested.pop();
        }
        j += 1;
      }

      let Some(declaration) = parse_declaration(&buffer, colon) else {
        return Err(Error::Syntax("Invalid custom property, expected a value".to_owned()));
      };
      match stack.last_mut() {
        Some(parent) => parent.children_mut().expect("a rule").push(declaration),
        None => ast.push(declaration),
      }
      buffer.clear();
    } else if c == b';' && buffer.as_bytes().first() == Some(&b'@') {
      let node = parse_at_rule(&buffer, Vec::new());
      match stack.last_mut() {
        Some(parent) => parent.children_mut().expect("a rule").push(node),
        None => ast.push(node),
      }
      buffer.clear();
    } else if c == b';' && closing.as_bytes().last() != Some(&b')') {
      match parse_declaration(&buffer, buffer.find(':')) {
        Some(declaration) => {
          match stack.last_mut() {
            Some(parent) => parent.children_mut().expect("a rule").push(declaration),
            None => ast.push(declaration),
          }
          buffer.clear();
        }
        None => {
          if !buffer.is_empty() {
            return Err(Error::Syntax(format!("Invalid declaration: `{}`", buffer.trim())));
          }
        }
      }
    } else if c == b'{' && closing.as_bytes().last() != Some(&b')') {
      closing.push('}');
      stack.push(rule(buffer.trim(), Vec::new()));
      buffer.clear();
    } else if c == b'}' && closing.as_bytes().last() != Some(&b')') {
      if closing.is_empty() {
        return Err(Error::Syntax("Missing opening {".to_owned()));
      }
      closing.pop();

      if !buffer.is_empty() {
        if buffer.as_bytes()[0] == b'@' {
          let node = parse_at_rule(&buffer, Vec::new());
          match stack.last_mut() {
            Some(parent) => parent.children_mut().expect("a rule").push(node),
            None => ast.push(node),
          }
        } else if let Some(parent) = stack.last_mut() {
          let Some(declaration) = parse_declaration(&buffer, buffer.find(':')) else {
            return Err(Error::Syntax(format!("Invalid declaration: `{}`", buffer.trim())));
          };
          parent.children_mut().expect("a rule").push(declaration);
        }
      }

      if let Some(node) = stack.pop() {
        match stack.last_mut() {
          Some(grand) => grand.children_mut().expect("a rule").push(node),
          None => ast.push(node),
        }
      }
      buffer.clear();
    } else if c == b'(' {
      closing.push(')');
      buffer.push('(');
    } else if c == b')' {
      if closing.as_bytes().last() != Some(&b')') {
        return Err(Error::Syntax("Missing opening (".to_owned()));
      }
      closing.pop();
      buffer.push(')');
    } else {
      if buffer.is_empty() && (c == b' ' || c == b'\n' || c == b'\t') {
        i += 1;
        continue;
      }
      let end = next_char_end(input, i);
      buffer.push_str(&input[i..end]);
      i = end;
      continue;
    }
    i += 1;
  }

  if buffer.as_bytes().first() == Some(&b'@') {
    ast.push(parse_at_rule(&buffer, Vec::new()));
  }

  if !closing.is_empty()
    && let Some(parent) = stack.last()
  {
    return Err(Error::Syntax(match parent {
      Node::Rule { selector, .. } => format!("Missing closing }} at {selector}"),
      Node::AtRule { name, params, .. } => format!("Missing closing }} at {name} {params}"),
      _ => "Missing closing }".to_owned(),
    }));
  }

  if license_comments.is_empty() {
    Ok(ast)
  } else {
    license_comments.extend(ast);
    Ok(license_comments)
  }
}

/// Splits an at-rule's prelude into its name and its parameters.
pub fn parse_at_rule(buffer: &str, nodes: Vec<Node>) -> Node {
  let bytes = buffer.as_bytes();
  let mut name = buffer;
  let mut params = "";
  for i in 5..bytes.len() {
    let c = bytes[i];
    if c == b' ' || c == b'\t' || c == b'(' {
      name = &buffer[..i];
      params = &buffer[i..];
      break;
    }
  }
  at_rule(name.trim(), params.trim(), nodes)
}

fn parse_declaration(buffer: &str, colon: Option<usize>) -> Option<Node> {
  let colon = colon?.min(buffer.len());
  let after = (colon + 1).min(buffer.len());
  let important = buffer[after..].find("!important").map(|i| i + after);
  let property = buffer[..colon].trim();
  let value = buffer[after..important.unwrap_or(buffer.len())].trim();
  Some(Node::Declaration {
    property: property.to_owned(),
    value: Some(value.to_owned()),
    important: important.is_some(),
  })
}

fn parse_string(input: &str, start: usize, quote: u8) -> Result<usize, Error> {
  let bytes = input.as_bytes();
  let len = bytes.len();
  let at = |i: usize| -> u8 { if i < len { bytes[i] } else { 0 } };
  let mut i = start + 1;
  while i < len {
    let p = bytes[i];
    if p == b'\\' {
      i += 2;
      continue;
    } else if p == quote {
      return Ok(i);
    } else if p == b';' && (at(i + 1) == b'\n' || (at(i + 1) == b'\r' && at(i + 2) == b'\n')) {
      return Err(Error::Syntax(format!(
        "Unterminated string: {}{}",
        &input[start..i + 1],
        quote as char
      )));
    } else if p == b'\n' || (p == b'\r' && at(i + 1) == b'\n') {
      return Err(Error::Syntax(format!(
        "Unterminated string: {}{}",
        &input[start..i],
        quote as char
      )));
    }
    i += 1;
  }
  Ok(start)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ast::{decl, style_rule};

  #[test]
  fn parses_rules_and_declarations() {
    let ast = parse(".foo {\n  color: red;\n  --x: a  b;\n}").unwrap();
    assert_eq!(
      ast,
      vec![style_rule(".foo", vec![decl("color", "red"), decl("--x", "a  b")])]
    );
  }

  #[test]
  fn parses_at_rules() {
    let ast = parse("@tailwind utilities;\n@media (width >= 40rem) { .a { color: red } }").unwrap();
    assert_eq!(ast[0], at_rule("@tailwind", "utilities", vec![]));
    assert_eq!(
      ast[1],
      at_rule(
        "@media",
        "(width >= 40rem)",
        vec![style_rule(".a", vec![decl("color", "red")])]
      )
    );
  }

  #[test]
  fn hoists_license_comments() {
    let ast = parse(".a { color: red }\n/*! license */").unwrap();
    assert_eq!(ast[0], comment("! license "));
  }

  #[test]
  fn reports_unbalanced_braces() {
    assert!(matches!(parse(".a { color: red").unwrap_err(), Error::Syntax(m) if m == "Missing closing } at .a"));
    assert!(matches!(parse(".a } ").unwrap_err(), Error::Syntax(m) if m == "Missing opening {"));
  }
}
