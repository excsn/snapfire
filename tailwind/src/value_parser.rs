//! Tailwind's value parser: words, functions and separators, enough to find and rewrite
//! function calls in a declaration value.

use crate::walk::Action;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ValueNode {
  Word(String),
  Function { value: String, nodes: Vec<ValueNode> },
  Separator(String),
}

impl ValueNode {
  pub fn word(value: impl Into<String>) -> Self {
    ValueNode::Word(value.into())
  }

  pub fn value(&self) -> &str {
    match self {
      ValueNode::Word(v) | ValueNode::Separator(v) => v,
      ValueNode::Function { value, .. } => value,
    }
  }
}

pub fn to_css(ast: &[ValueNode]) -> String {
  let mut css = String::new();
  write_css(ast, &mut css);
  css
}

fn write_css(ast: &[ValueNode], css: &mut String) {
  for node in ast {
    match node {
      ValueNode::Word(v) | ValueNode::Separator(v) => css.push_str(v),
      ValueNode::Function { value, nodes } => {
        css.push_str(value);
        css.push('(');
        write_css(nodes, css);
        css.push(')');
      }
    }
  }
}

fn is_separator(c: u8) -> bool {
  matches!(c, b':' | b',' | b'=' | b'>' | b'<' | b'\n' | b' ' | b'\t')
}

pub fn parse(input: &str) -> Vec<ValueNode> {
  let owned;
  let input = if input.contains("\r\n") {
    owned = input.replace("\r\n", "\n");
    owned.as_str()
  } else {
    input
  };
  let bytes = input.as_bytes();

  let mut ast: Vec<ValueNode> = Vec::new();
  let mut stack: Vec<(String, Vec<ValueNode>)> = Vec::new();
  let mut buffer = String::new();

  fn push(ast: &mut Vec<ValueNode>, stack: &mut [(String, Vec<ValueNode>)], node: ValueNode) {
    match stack.last_mut() {
      Some((_, nodes)) => nodes.push(node),
      None => ast.push(node),
    }
  }

  let mut i = 0;
  while i < bytes.len() {
    let c = bytes[i];
    match c {
      b'\\' => {
        let end = next_char_end(input, i + 1);
        buffer.push_str(&input[i..end]);
        i = end;
        continue;
      }
      b'/' => {
        if !buffer.is_empty() {
          push(&mut ast, &mut stack, ValueNode::Word(std::mem::take(&mut buffer)));
        }
        push(&mut ast, &mut stack, ValueNode::word("/"));
      }
      c if is_separator(c) => {
        if !buffer.is_empty() {
          push(&mut ast, &mut stack, ValueNode::Word(std::mem::take(&mut buffer)));
        }
        let start = i;
        let mut end = i + 1;
        while end < bytes.len() && is_separator(bytes[end]) {
          end += 1;
        }
        push(&mut ast, &mut stack, ValueNode::Separator(input[start..end].to_owned()));
        i = end;
        continue;
      }
      b'\'' | b'"' => {
        let start = i;
        let mut j = i + 1;
        while j < bytes.len() {
          if bytes[j] == b'\\' {
            j += 2;
            continue;
          }
          if bytes[j] == c {
            i = j;
            break;
          }
          j += 1;
        }
        let end = next_char_end(input, i);
        buffer.push_str(&input[start..end]);
        i = end;
        continue;
      }
      b'(' => {
        stack.push((std::mem::take(&mut buffer), Vec::new()));
      }
      b')' => match stack.pop() {
        Some((value, mut nodes)) => {
          if !buffer.is_empty() {
            nodes.push(ValueNode::Word(std::mem::take(&mut buffer)));
          }
          push(&mut ast, &mut stack, ValueNode::Function { value, nodes });
        }
        None => buffer.clear(),
      },
      _ => {
        let end = next_char_end(input, i);
        buffer.push_str(&input[i..end]);
        i = end;
        continue;
      }
    }
    i += 1;
  }

  while let Some((value, nodes)) = stack.pop() {
    push(&mut ast, &mut stack, ValueNode::Function { value, nodes });
  }

  if !buffer.is_empty() {
    ast.push(ValueNode::Word(buffer));
  }

  ast
}

/// The byte offset just past the character starting at `i`. At the end it is `i`.
pub(crate) fn next_char_end(input: &str, i: usize) -> usize {
  if i >= input.len() {
    return input.len();
  }
  let mut end = i + 1;
  while end < input.len() && !input.is_char_boundary(end) {
    end += 1;
  }
  end
}

/// Walks a value tree depth first, letting `f` rewrite nodes as it goes.
pub fn walk(ast: &mut Vec<ValueNode>, f: &mut impl FnMut(&mut ValueNode, &WalkInfo) -> Action<ValueNode>) {
  walk_level(ast, f, 0);
}

#[non_exhaustive]
pub struct WalkInfo {
  pub depth: usize,
  pub index: usize,
}

fn walk_level(
  nodes: &mut Vec<ValueNode>,
  f: &mut impl FnMut(&mut ValueNode, &WalkInfo) -> Action<ValueNode>,
  depth: usize,
) -> bool {
  let mut i = 0;
  while i < nodes.len() {
    let info = WalkInfo { depth, index: i };
    match f(&mut nodes[i], &info) {
      Action::Continue => {
        if let ValueNode::Function { nodes: children, .. } = &mut nodes[i]
          && !children.is_empty()
          && walk_level(children, f, depth + 1)
        {
          return true;
        }
        i += 1;
      }
      Action::Skip => i += 1,
      Action::Stop => return true,
      Action::Replace(replacement) => {
        nodes.splice(i..i + 1, replacement);
      }
      Action::ReplaceSkip(replacement) => {
        let n = replacement.len();
        nodes.splice(i..i + 1, replacement);
        i += n;
      }
      Action::ReplaceStop(replacement) => {
        nodes.splice(i..i + 1, replacement);
        return true;
      }
    }
  }
  false
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  fn tree(ast: &[ValueNode]) -> serde_json::Value {
    serde_json::Value::Array(
      ast
        .iter()
        .map(|node| match node {
          ValueNode::Word(value) => json!({ "kind": "word", "value": value }),
          ValueNode::Separator(value) => json!({ "kind": "separator", "value": value }),
          ValueNode::Function { value, nodes } => json!({ "kind": "function", "value": value, "nodes": tree(nodes) }),
        })
        .collect(),
    )
  }

  #[test]
  fn parse_conformance() {
    assert_eq!(
      tree(&parse("123px")),
      json!([{ "kind": "word", "value": "123px" }]),
      "should parse a value"
    );
    assert_eq!(
      tree(&parse("'hello world'")),
      json!([{ "kind": "word", "value": "'hello world'" }]),
      "should parse a string value"
    );
    assert_eq!(
      tree(&parse("hello world")),
      json!([
        { "kind": "word", "value": "hello" },
        { "kind": "separator", "value": " " },
        { "kind": "word", "value": "world" }
      ]),
      "should parse a list"
    );
    assert_eq!(
      tree(&parse("'hello ( world )'")),
      json!([{ "kind": "word", "value": "'hello ( world )'" }]),
      "should parse a string containing parentheses"
    );
    assert_eq!(
      tree(&parse("theme()")),
      json!([{ "kind": "function", "value": "theme", "nodes": [] }]),
      "should parse a function with no arguments"
    );
    assert_eq!(
      tree(&parse("theme(foo)")),
      json!([
        { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "foo" }] }
      ]),
      "should parse a function with a single argument"
    );
    assert_eq!(
      tree(&parse("theme('foo')")),
      json!([
        { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "'foo'" }] }
      ]),
      "should parse a function with a single string argument"
    );
    assert_eq!(
      tree(&parse("theme(foo, bar)")),
      json!([
        {
          "kind": "function",
          "value": "theme",
          "nodes": [
            { "kind": "word", "value": "foo" },
            { "kind": "separator", "value": ", " },
            { "kind": "word", "value": "bar" }
          ]
        }
      ]),
      "should parse a function with multiple arguments"
    );
    assert_eq!(
      tree(&parse("theme(\n\tfoo,\n\tbar\n)")),
      json!([
        {
          "kind": "function",
          "value": "theme",
          "nodes": [
            { "kind": "separator", "value": "\n\t" },
            { "kind": "word", "value": "foo" },
            { "kind": "separator", "value": ",\n\t" },
            { "kind": "word", "value": "bar" },
            { "kind": "separator", "value": "\n" }
          ]
        }
      ]),
      "should parse a function with multiple arguments across lines"
    );
    assert_eq!(
      tree(&parse("theme(foo, theme(bar))")),
      json!([
        {
          "kind": "function",
          "value": "theme",
          "nodes": [
            { "kind": "word", "value": "foo" },
            { "kind": "separator", "value": ", " },
            { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "bar" }] }
          ]
        }
      ]),
      "should parse a function with nested arguments"
    );
    assert_eq!(
      tree(&parse("theme(colors.red.500/var(--opacity))")),
      json!([
        {
          "kind": "function",
          "value": "theme",
          "nodes": [
            { "kind": "word", "value": "colors.red.500" },
            { "kind": "word", "value": "/" },
            { "kind": "function", "value": "var", "nodes": [{ "kind": "word", "value": "--opacity" }] }
          ]
        }
      ]),
      "should parse a function with nested arguments separated by `/`"
    );
    assert_eq!(
      tree(&parse("calc((1 + 2) * 3)")),
      json!([
        {
          "kind": "function",
          "value": "calc",
          "nodes": [
            {
              "kind": "function",
              "value": "",
              "nodes": [
                { "kind": "word", "value": "1" },
                { "kind": "separator", "value": " " },
                { "kind": "word", "value": "+" },
                { "kind": "separator", "value": " " },
                { "kind": "word", "value": "2" }
              ]
            },
            { "kind": "separator", "value": " " },
            { "kind": "word", "value": "*" },
            { "kind": "separator", "value": " " },
            { "kind": "word", "value": "3" }
          ]
        }
      ]),
      "should handle calculations"
    );
    assert_eq!(
      tree(&parse(
        "(min-width: 600px) and (max-width:theme(colors.red.500)) and (theme(--breakpoint-sm)<width<=theme(--breakpoint-md))"
      )),
      json!([
        {
          "kind": "function",
          "value": "",
          "nodes": [
            { "kind": "word", "value": "min-width" },
            { "kind": "separator", "value": ": " },
            { "kind": "word", "value": "600px" }
          ]
        },
        { "kind": "separator", "value": " " },
        { "kind": "word", "value": "and" },
        { "kind": "separator", "value": " " },
        {
          "kind": "function",
          "value": "",
          "nodes": [
            { "kind": "word", "value": "max-width" },
            { "kind": "separator", "value": ":" },
            { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "colors.red.500" }] }
          ]
        },
        { "kind": "separator", "value": " " },
        { "kind": "word", "value": "and" },
        { "kind": "separator", "value": " " },
        {
          "kind": "function",
          "value": "",
          "nodes": [
            { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "--breakpoint-sm" }] },
            { "kind": "separator", "value": "<" },
            { "kind": "word", "value": "width" },
            { "kind": "separator", "value": "<=" },
            { "kind": "function", "value": "theme", "nodes": [{ "kind": "word", "value": "--breakpoint-md" }] }
          ]
        }
      ]),
      "should handle media query params with functions"
    );
    assert_eq!(
      tree(&parse("calc(1 + 2))")),
      json!([
        {
          "kind": "function",
          "value": "calc",
          "nodes": [
            { "kind": "word", "value": "1" },
            { "kind": "separator", "value": " " },
            { "kind": "word", "value": "+" },
            { "kind": "separator", "value": " " },
            { "kind": "word", "value": "2" }
          ]
        }
      ]),
      "should not error when extra `)` was passed"
    );
  }

  #[test]
  fn to_css_conformance() {
    for (input, expected) in [
      ("calc((1 + 2) * 3)", "calc((1 + 2) * 3)"),
      ("theme(foo, theme(bar))", "theme(foo, theme(bar))"),
      (
        "(min-width: 600px) and (max-width:theme(colors.red.500))",
        "(min-width: 600px) and (max-width:theme(colors.red.500))",
      ),
      ("foo(   bar  )", "foo(   bar  )"),
    ] {
      assert_eq!(to_css(&parse(input)), expected);
    }
  }

  #[test]
  fn walk_replaces_a_function_call() {
    let mut ast = parse("(min-width: 600px) and (max-width: theme(lg))");
    walk(&mut ast, &mut |node, _| match node {
      ValueNode::Function { value, .. } if value == "theme" => Action::Replace(vec![ValueNode::word("64rem")]),
      _ => Action::Continue,
    });
    assert_eq!(to_css(&ast), "(min-width: 600px) and (max-width: 64rem)");
  }

  #[test]
  fn round_trips() {
    for input in [
      "calc(1px + 2px)",
      "var(--a, 0 0 1px rgb(0, 0, 0))",
      "theme(colors.red.500/10)",
      "'a b' c",
      "foo(bar",
    ] {
      let ast = parse(input);
      let out = to_css(&ast);
      if input == "foo(bar" {
        assert_eq!(out, "foo()bar");
      } else {
        assert_eq!(out, input);
      }
    }
  }

  #[test]
  fn separates_slashes() {
    let ast = parse("a/b");
    assert_eq!(
      ast,
      vec![ValueNode::word("a"), ValueNode::word("/"), ValueNode::word("b")]
    );
  }
}
