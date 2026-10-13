//! Tailwind's CSS parser: rules, at-rules, declarations and license comments, with
//! whitespace collapsed outside custom properties.

use crate::Error;
use crate::ast::{Node, at_rule, comment, rule};
use crate::value_parser::next_char_end;

/// A syntax error's message, prefixed with `file:line:column` when the stylesheet has a name.
fn located(message: String, input: &str, from: Option<&str>, offset: usize) -> Error {
  let Some(file) = from else {
    return Error::Syntax(message);
  };
  let offset = offset.min(input.len());
  let mut start = offset;
  while start > 0 && !input.is_char_boundary(start) {
    start -= 1;
  }
  let before = &input[..start];
  let line = before.matches('\n').count() + 1;
  let line_start = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
  let column = before[line_start..].encode_utf16().count() + 1;
  Error::Syntax(format!("{file}:{line}:{column}: {message}"))
}

pub fn parse(input: &str, from: Option<&str>) -> Result<Vec<Node>, Error> {
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
  let mut starts: Vec<usize> = Vec::new();
  let mut buffer = String::new();
  let mut buffer_start = 0usize;
  let mut closing = String::new();
  let fail = |message: String, offset: usize| located(message, input, from, offset);
  let string = |start: usize, quote: u8| {
    parse_string(input, start, quote).map_err(|(message, offset)| located(message, input, from, offset))
  };

  let mut i = 0;
  while i < len {
    let c = bytes[i];

    if c == b'\r' && at(i + 1) == b'\n' {
      i += 1;
      continue;
    }

    if c == b'\\' {
      if buffer.is_empty() {
        buffer_start = i;
      }
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
      let end = string(i, c)?;
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
          j = string(j, p)?;
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
        return Err(fail("Invalid custom property, expected a value".to_owned(), start));
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
            return Err(fail(format!("Invalid declaration: `{}`", buffer.trim()), buffer_start));
          }
        }
      }
    } else if c == b'{' && closing.as_bytes().last() != Some(&b')') {
      closing.push('}');
      stack.push(rule(buffer.trim(), Vec::new()));
      starts.push(buffer_start);
      buffer.clear();
    } else if c == b'}' && closing.as_bytes().last() != Some(&b')') {
      if closing.is_empty() {
        return Err(fail("Missing opening {".to_owned(), i));
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
            return Err(fail(format!("Invalid declaration: `{}`", buffer.trim()), buffer_start));
          };
          parent.children_mut().expect("a rule").push(declaration);
        }
      }

      starts.pop();
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
        return Err(fail("Missing opening (".to_owned(), i));
      }
      closing.pop();
      buffer.push(')');
    } else {
      if buffer.is_empty() && (c == b' ' || c == b'\n' || c == b'\t') {
        i += 1;
        continue;
      }
      if buffer.is_empty() {
        buffer_start = i;
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
    let message = match parent {
      Node::Rule { selector, .. } => format!("Missing closing }} at {selector}"),
      Node::AtRule { name, params, .. } => format!("Missing closing }} at {name} {params}"),
      _ => "Missing closing }".to_owned(),
    };
    return Err(fail(message, starts.last().copied().unwrap_or(0)));
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

fn parse_string(input: &str, start: usize, quote: u8) -> Result<usize, (String, usize)> {
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
      return Err((
        format!("Unterminated string: {}{}", &input[start..i + 1], quote as char),
        start,
      ));
    } else if p == b'\n' || (p == b'\r' && at(i + 1) == b'\n') {
      return Err((
        format!("Unterminated string: {}{}", &input[start..i], quote as char),
        start,
      ));
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
    let ast = parse(".foo {\n  color: red;\n  --x: a  b;\n}", None).unwrap();
    assert_eq!(
      ast,
      vec![style_rule(".foo", vec![decl("color", "red"), decl("--x", "a  b")])]
    );
  }

  #[test]
  fn parses_at_rules() {
    let ast = parse(
      "@tailwind utilities;\n@media (width >= 40rem) { .a { color: red } }",
      None,
    )
    .unwrap();
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
    let ast = parse(".a { color: red }\n/*! license */", None).unwrap();
    assert_eq!(ast[0], comment("! license "));
  }

  #[test]
  fn reports_unbalanced_braces() {
    assert!(matches!(parse(".a { color: red", None).unwrap_err(), Error::Syntax(m) if m == "Missing closing } at .a"));
    assert!(matches!(parse(".a } ", None).unwrap_err(), Error::Syntax(m) if m == "Missing opening {"));
  }
}

#[cfg(test)]
mod conformance {
  use serde_json::{Value, json};

  use super::parse;
  use crate::ast::Node;

  fn node(n: &Node) -> Value {
    match n {
      Node::Rule { selector, nodes } => {
        json!({ "kind": "rule", "selector": selector, "nodes": nodes.iter().map(node).collect::<Vec<_>>() })
      }
      Node::AtRule { name, params, nodes } => {
        json!({ "kind": "at-rule", "name": name, "params": params, "nodes": nodes.iter().map(node).collect::<Vec<_>>() })
      }
      Node::Declaration {
        property,
        value,
        important,
      } => json!({ "kind": "declaration", "property": property, "value": value, "important": important }),
      Node::Comment(value) => json!({ "kind": "comment", "value": value }),
      Node::Context { .. } | Node::AtRoot { .. } => Value::Null,
    }
  }

  #[derive(serde::Deserialize)]
  struct Case {
    test: String,
    css: String,
    windows: bool,
    located: bool,
    ast: Option<Value>,
    error: Option<String>,
  }

  /// `css-parser.test.ts`, under Unix and Windows line endings.
  #[test]
  fn tailwind_parser_cases() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/css-parser-cases.json");
    let cases: Vec<Case> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for case in &cases {
      let unix = case.css.replace("\r\n", "\n");
      let input = if case.windows { unix.replace('\n', "\r\n") } else { unix };
      let result = parse(&input, if case.located { Some("input.css") } else { None });
      match (&case.ast, &case.error, result) {
        (Some(expected), _, Ok(ast)) => {
          let got = Value::Array(ast.iter().map(node).collect());
          if &got != expected {
            failures.push(format!("{}:\n  got      {got}\n  expected {expected}", case.test));
          }
        }
        (_, Some(expected), Err(e)) => {
          if &e.to_string() != expected {
            failures.push(format!("{}: error {e}, expected {expected}", case.test));
          }
        }
        (_, _, Ok(ast)) => failures.push(format!("{}: parsed {} nodes, expected an error", case.test, ast.len())),
        (_, _, Err(e)) => failures.push(format!("{}: {e}", case.test)),
      }
    }
    assert!(
      failures.is_empty(),
      "{} of {} differ:\n{}",
      failures.len(),
      cases.len(),
      failures.join("\n")
    );
  }
}
