//! Depth-first walks over the CSS tree that can rewrite as they go, with the path of
//! ancestors each visit sees.

use crate::ast::{ContextMap, Node};

#[non_exhaustive]
pub enum Action<T> {
  Continue,
  Skip,
  Stop,
  Replace(Vec<T>),
  ReplaceSkip(Vec<T>),
  ReplaceStop(Vec<T>),
}

/// An ancestor of the node being visited, as it was when the walk entered it.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Ancestor {
  Rule { selector: String },
  AtRule { name: String, params: String },
  Context { context: ContextMap },
  AtRoot,
}

impl Ancestor {
  fn of(node: &Node) -> Option<Ancestor> {
    match node {
      Node::Rule { selector, .. } => Some(Ancestor::Rule {
        selector: selector.clone(),
      }),
      Node::AtRule { name, params, .. } => Some(Ancestor::AtRule {
        name: name.clone(),
        params: params.clone(),
      }),
      Node::Context { context, .. } => Some(Ancestor::Context {
        context: context.clone(),
      }),
      Node::AtRoot { .. } => Some(Ancestor::AtRoot),
      Node::Declaration { .. } | Node::Comment(_) => None,
    }
  }
}

pub struct Visit<'a> {
  path: &'a [Ancestor],
}

impl Visit<'_> {
  /// Every ancestor, outermost first.
  pub fn path(&self) -> &[Ancestor] {
    self.path
  }

  /// The nearest ancestor that is not a context node.
  pub fn css_parent(&self) -> Option<&Ancestor> {
    self.path.iter().rev().find(|a| !matches!(a, Ancestor::Context { .. }))
  }

  /// The context values of every enclosing context node, inner ones winning.
  pub fn context(&self) -> ContextMap {
    let mut merged = ContextMap::new();
    for ancestor in self.path {
      if let Ancestor::Context { context } = ancestor {
        for (k, v) in context {
          merged.insert(k.clone(), v.clone());
        }
      }
    }
    merged
  }
}

/// Walks `ast`, calling `f` on entry to each node.
pub fn walk(ast: &mut Vec<Node>, f: &mut impl FnMut(&mut Node, &Visit) -> Action<Node>) {
  let mut path = Vec::new();
  walk_level(ast, f, &mut path);
}

/// Walks `ast` without rewriting it.
pub fn walk_ref(ast: &[Node], f: &mut impl FnMut(&Node, &[Ancestor]) -> bool) {
  fn level(nodes: &[Node], f: &mut impl FnMut(&Node, &[Ancestor]) -> bool, path: &mut Vec<Ancestor>) -> bool {
    for node in nodes {
      if !f(node, path) {
        return true;
      }
      if let Some(children) = node.children()
        && !children.is_empty()
      {
        path.push(Ancestor::of(node).expect("a parent node"));
        let stopped = level(children, f, path);
        path.pop();
        if stopped {
          return true;
        }
      }
    }
    false
  }
  let mut path = Vec::new();
  level(ast, f, &mut path);
}

fn walk_level(
  nodes: &mut Vec<Node>,
  f: &mut impl FnMut(&mut Node, &Visit) -> Action<Node>,
  path: &mut Vec<Ancestor>,
) -> bool {
  let mut i = 0;
  while i < nodes.len() {
    let result = {
      let visit = Visit { path };
      f(&mut nodes[i], &visit)
    };
    match result {
      Action::Continue => {
        let node = &mut nodes[i];
        if node.children().is_some_and(|c| !c.is_empty()) {
          path.push(Ancestor::of(node).expect("a parent node"));
          let stopped = walk_level(node.children_mut().expect("children"), f, path);
          path.pop();
          if stopped {
            return true;
          }
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
