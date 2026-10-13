use crate::value_parser::{self, ValueNode};
use crate::walk::Action;

/// The custom properties `raw` reads through `var(…)`.
pub fn extract_used_variables(raw: &str) -> Vec<String> {
  let mut variables = Vec::new();
  let mut ast = value_parser::parse(raw);
  value_parser::walk(&mut ast, &mut |node, _| {
    let ValueNode::Function { value, nodes } = node else {
      return Action::Continue;
    };
    if value != "var" {
      return Action::Continue;
    }
    value_parser::walk(nodes, &mut |child, _| {
      if let ValueNode::Word(w) = child
        && w.starts_with("--")
      {
        variables.push(w.clone());
      }
      Action::Continue
    });
    Action::Skip
  });
  variables
}
