//! `@apply`: replacing a list of classes with the declarations they compile to.

use std::collections::{HashMap, HashSet};

use indexmap::IndexSet;

use crate::Error;
use crate::ast::{Node, at_rule, to_css};
use crate::candidate::CandidateKind;
use crate::compile::compile_candidates;
use crate::design_system::DesignSystem;
use crate::features::Features;
use crate::utils::segment::segment;
use crate::walk::{Action, walk, walk_ref};

/// The `@utility` names, without `-*`, of the store entries, in store order.
pub(crate) struct UtilityNames(pub Vec<String>);

fn check_keyframes(ast: &[Node]) -> Result<(), Error> {
  let mut found = false;
  walk_ref(ast, &mut |node, _| {
    if let Node::AtRule { name, nodes, .. } = node
      && name == "@keyframes"
    {
      walk_ref(nodes, &mut |child, _| {
        if child.is_at_rule("@apply") {
          found = true;
        }
        !found
      });
    }
    !found
  });
  if found {
    return Err(Error::Message(
      "You cannot use `@apply` inside `@keyframes`.".to_owned(),
    ));
  }
  Ok(())
}

fn dependencies(params: &str, ds: &DesignSystem) -> Vec<String> {
  let mut roots = Vec::new();
  for candidate in params.split_whitespace() {
    for parsed in ds.parse_candidate(candidate).iter() {
      match &parsed.kind {
        CandidateKind::Arbitrary { .. } => {}
        CandidateKind::Static { root } | CandidateKind::Functional { root, .. } => roots.push(root.clone()),
      }
    }
  }
  roots
}

fn has_apply(nodes: &[Node]) -> bool {
  let mut found = false;
  walk_ref(nodes, &mut |node, _| {
    if node.is_at_rule("@apply") {
      found = true;
    }
    !found
  });
  found
}

pub(crate) fn substitute_at_apply(
  ast: &mut Vec<Node>,
  ds: &DesignSystem,
  names: &UtilityNames,
) -> Result<Features, Error> {
  let mut features = Features::NONE;

  check_keyframes(ast)?;
  for body in ds.css_utilities.borrow().iter() {
    check_keyframes(body)?;
  }

  let mut deps: HashMap<usize, IndexSet<String>> = HashMap::new();
  let mut parents: Vec<usize> = Vec::new();
  for (i, body) in ds.css_utilities.borrow().iter().enumerate() {
    let mut found = Vec::new();
    walk_ref(body, &mut |node, _| {
      if let Node::AtRule { name, params, .. } = node
        && name == "@apply"
      {
        found.push(params.clone());
      }
      true
    });
    if found.is_empty() {
      continue;
    }
    parents.push(i);
    let entry = deps.entry(i).or_default();
    for params in found {
      entry.extend(dependencies(&params, ds));
    }
  }

  let mut definitions: HashMap<&str, Vec<usize>> = HashMap::new();
  for (i, name) in names.0.iter().enumerate() {
    definitions.entry(name.as_str()).or_default().push(i);
  }

  let mut seen = HashSet::new();
  let mut wip = HashSet::new();
  let mut sorted = Vec::new();

  #[allow(clippy::too_many_arguments)]
  fn visit(
    node: usize,
    path: &mut Vec<usize>,
    deps: &HashMap<usize, IndexSet<String>>,
    definitions: &HashMap<&str, Vec<usize>>,
    seen: &mut HashSet<usize>,
    wip: &mut HashSet<usize>,
    sorted: &mut Vec<usize>,
    ds: &DesignSystem,
    names: &UtilityNames,
  ) -> Result<(), Error> {
    if seen.contains(&node) {
      return Ok(());
    }
    if wip.contains(&node) {
      let position = path.iter().position(|n| *n == node).unwrap_or(0);
      let next = path[(position + 1) % path.len()];
      let bodies = ds.css_utilities.borrow();
      let mut applied = Vec::new();
      walk_ref(&bodies[node], &mut |child, _| {
        if let Node::AtRule { name, params, .. } = child
          && name == "@apply"
        {
          applied.push(params.clone());
        }
        true
      });
      for params in applied {
        for candidate in params.split_whitespace() {
          for parsed in ds.parse_candidate(candidate).iter() {
            if let Some(root) = parsed.root()
              && names.0[next] == root
            {
              return Err(Error::Message(format!(
                "You cannot `@apply` the `{candidate}` utility here because it creates a circular dependency."
              )));
            }
          }
        }
      }
      let show = |i: usize| to_css(&[at_rule("@utility", names.0[i].clone(), bodies[i].clone())]);
      return Err(Error::Message(format!(
        "Circular dependency detected:\n\n{}\nRelies on:\n\n{}",
        show(node),
        show(next)
      )));
    }

    wip.insert(node);
    if let Some(node_deps) = deps.get(&node) {
      for dependency in node_deps {
        if let Some(defs) = definitions.get(dependency.as_str()) {
          for def in defs {
            path.push(node);
            visit(*def, path, deps, definitions, seen, wip, sorted, ds, names)?;
            path.pop();
          }
        }
      }
    }
    seen.insert(node);
    wip.remove(&node);
    sorted.push(node);
    Ok(())
  }

  for parent in &parents {
    visit(
      *parent,
      &mut Vec::new(),
      &deps,
      &definitions,
      &mut seen,
      &mut wip,
      &mut sorted,
      ds,
      names,
    )?;
  }

  for index in sorted {
    let mut body = std::mem::take(&mut ds.css_utilities.borrow_mut()[index]);
    let result = substitute_in(&mut body, ds);
    ds.css_utilities.borrow_mut()[index] = body;
    result?;
  }

  if !parents.is_empty() || has_apply(ast) {
    features |= Features::AT_APPLY;
  }
  substitute_in(ast, ds)?;

  Ok(features)
}

/// Replaces every `@apply` in `nodes`. Returns whether any was replaced.
fn substitute_in(nodes: &mut Vec<Node>, ds: &DesignSystem) -> Result<bool, Error> {
  let mut error = None;
  let mut replaced = false;
  walk(nodes, &mut |child, _| {
    let Node::AtRule {
      name,
      params,
      nodes: body,
    } = child
    else {
      return Action::Continue;
    };
    if name != "@apply" {
      return Action::Continue;
    }

    let mut normal = Vec::new();
    let mut dashed = Vec::new();
    let mut candidates: Vec<String> = Vec::new();
    for part in params.split_whitespace() {
      if part.starts_with("--") {
        dashed.push(part.to_owned());
      } else {
        normal.push(part.to_owned());
      }
      if !candidates.iter().any(|c| c == part) {
        candidates.push(part.to_owned());
      }
    }
    if params.is_empty() || params.starts_with(char::is_whitespace) {
      if !candidates.iter().any(|c| c.is_empty()) {
        candidates.insert(0, String::new());
      }
      normal.insert(0, String::new());
    }

    if !dashed.is_empty() {
      if normal.is_empty() {
        return Action::Skip;
      }
      error = Some(Error::Message(format!(
        "You cannot use `@apply` with both mixins and utilities. Please move `@apply {}` into a separate rule.",
        dashed.join(" ")
      )));
      return Action::Stop;
    }

    if !body.is_empty() && !normal.is_empty() {
      error = Some(Error::Message(format!(
        "The rule `@apply {}` must not have a body.",
        normal.join(" ")
      )));
      return Action::Stop;
    }

    let mut failure: Option<Error> = None;
    let compiled = compile_candidates(candidates.iter().map(String::as_str), ds, false, &mut |candidate| {
      if failure.is_none() {
        failure = Some(invalid_apply(candidate, ds));
      }
    });
    if let Some(e) = failure {
      error = Some(e);
      return Action::Stop;
    }

    let mut out = Vec::new();
    for (node, _) in compiled.nodes {
      match node {
        Node::Rule { nodes, .. } => out.extend(nodes),
        other => out.push(other),
      }
    }
    replaced = true;
    Action::Replace(out)
  });
  match error {
    Some(e) => Err(e),
    None => Ok(replaced),
  }
}

fn list(items: &[String]) -> String {
  match items.len() {
    0 => String::new(),
    1 => items[0].clone(),
    2 => format!("{} and {}", items[0], items[1]),
    n => format!("{}, and {}", items[..n - 1].join(", "), items[n - 1]),
  }
}

fn invalid_apply(candidate: &str, ds: &DesignSystem) -> Error {
  if let Some(prefix) = &ds.theme.prefix
    && !candidate.starts_with(prefix.as_str())
  {
    return Error::Message(format!(
      "Cannot apply unprefixed utility class `{candidate}`. Did you mean `{prefix}:{candidate}`?"
    ));
  }

  if ds.invalid_candidates.borrow().contains(candidate) {
    return Error::Message(format!(
      "Cannot apply utility class `{candidate}` because it has been explicitly disabled: https://tailwindcss.com/docs/detecting-classes-in-source-files#explicitly-excluding-classes"
    ));
  }

  let mut parts: Vec<&str> = segment(candidate, b':');
  if parts.len() > 1 {
    let utility = parts.pop().unwrap();
    if ds.candidate_to_css(utility).is_some() {
      let unknown: Vec<String> = parts
        .iter()
        .filter(|variant| {
          ds.candidate_to_css(&format!("{variant}:[--tw-variant-check:1]"))
            .is_none()
        })
        .map(|variant| format!("`{variant}`"))
        .collect();
      if unknown.len() == 1 {
        return Error::Message(format!(
          "Cannot apply utility class `{candidate}` because the {} variant does not exist.",
          unknown[0]
        ));
      } else if unknown.len() > 1 {
        return Error::Message(format!(
          "Cannot apply utility class `{candidate}` because the {} variants do not exist.",
          list(&unknown)
        ));
      }
    }
  }

  if ds.theme.is_empty() {
    return Error::Message(format!(
      "Cannot apply unknown utility class `{candidate}`. Are you using CSS modules or similar and missing `@reference`? https://tailwindcss.com/docs/functions-and-directives#reference-directive"
    ));
  }

  Error::Message(format!("Cannot apply unknown utility class `{candidate}`"))
}
