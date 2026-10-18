//! Stylesheets read as a tree of rules, at-rules and declarations, enough to find a theme's
//! tokens and a utility's declarations in text a person or Tailwind wrote.

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
  Rule { selector: String, nodes: Vec<Node> },
  At { name: String, params: String, nodes: Vec<Node> },
  Declaration { property: String, value: String },
}

/// The stylesheet as rules, at-rules and declarations. Comments are dropped; a statement at-rule
/// such as `@layer a, b;` has no body.
pub fn parse(css: &str) -> Vec<Node> {
  let chars: Vec<char> = css.chars().collect();
  let mut at = 0;
  block(&chars, &mut at)
}

fn block(chars: &[char], at: &mut usize) -> Vec<Node> {
  let mut nodes = Vec::new();
  let mut prelude = String::new();
  let mut parens = 0usize;
  while *at < chars.len() {
    let c = chars[*at];
    match c {
      '/' if chars.get(*at + 1) == Some(&'*') => {
        *at += 2;
        while *at + 1 < chars.len() && !(chars[*at] == '*' && chars[*at + 1] == '/') {
          *at += 1;
        }
        *at += 2;
        continue;
      }
      '"' | '\'' => {
        prelude.push(c);
        *at += 1;
        while *at < chars.len() && chars[*at] != c {
          if chars[*at] == '\\' {
            prelude.push(chars[*at]);
            *at += 1;
          }
          if let Some(&next) = chars.get(*at) {
            prelude.push(next);
          }
          *at += 1;
        }
        prelude.push(c);
      }
      '(' => {
        parens += 1;
        prelude.push(c);
      }
      ')' => {
        parens = parens.saturating_sub(1);
        prelude.push(c);
      }
      '\\' => {
        prelude.push(c);
        if let Some(&next) = chars.get(*at + 1) {
          prelude.push(next);
          *at += 1;
        }
      }
      '{' if parens == 0 => {
        *at += 1;
        let children = block(chars, at);
        let head = prelude.trim().to_owned();
        prelude.clear();
        nodes.push(match head.strip_prefix('@') {
          Some(rest) => {
            let (name, params) = rest.split_once(|c: char| c.is_whitespace() || c == '(').map(|(n, _)| (n, &head[n.len() + 1..])).unwrap_or((rest, ""));
            Node::At { name: format!("@{name}"), params: params.trim().to_owned(), nodes: children }
          }
          None => Node::Rule { selector: head, nodes: children },
        });
        continue;
      }
      '}' if parens == 0 => {
        statement(&prelude, &mut nodes);
        *at += 1;
        return nodes;
      }
      ';' if parens == 0 => {
        statement(&prelude, &mut nodes);
        prelude.clear();
      }
      c => prelude.push(c),
    }
    *at += 1;
  }
  statement(&prelude, &mut nodes);
  nodes
}

fn statement(text: &str, nodes: &mut Vec<Node>) {
  let text = text.trim();
  if text.is_empty() {
    return;
  }
  if let Some(rest) = text.strip_prefix('@') {
    let (name, params) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
    nodes.push(Node::At { name: format!("@{name}"), params: params.trim().to_owned(), nodes: Vec::new() });
  } else if let Some((property, value)) = text.split_once(':') {
    nodes.push(Node::Declaration { property: property.trim().to_owned(), value: value.split_whitespace().collect::<Vec<_>>().join(" ") });
  }
}
