//! Compiling candidates into rules: the utility's declarations, important applied, every
//! variant wrapped around them and the result sorted the way Tailwind sorts.

use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use crate::ast::{Node, at_rule, decl, rule};
use crate::candidate::{Candidate, CandidateKind, Variant};
use crate::design_system::DesignSystem;
use crate::property_order;
use crate::utilities::{Compiled, as_color};
use crate::utils::compare::compare;
use crate::utils::escape::escape;
use crate::variants::Variants;
use crate::walk::{Action, walk};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PropertySort {
  pub order: Vec<usize>,
  pub count: usize,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct CompiledRule {
  pub node: Node,
  pub property_sort: PropertySort,
}

#[non_exhaustive]
pub struct CompiledCandidates {
  /// Each rule with the raw candidate that made it, in output order.
  pub nodes: Vec<(Node, String)>,
}

pub fn compile_candidates<'a>(
  raw_candidates: impl IntoIterator<Item = &'a str>,
  ds: &DesignSystem,
  respect_important: bool,
  on_invalid: &mut dyn FnMut(&str),
) -> CompiledCandidates {
  let mut matches: Vec<(String, Rc<Vec<Candidate>>)> = Vec::new();
  let mut seen = std::collections::HashSet::new();

  for raw in raw_candidates {
    if ds.invalid_candidates.borrow().contains(raw) {
      on_invalid(raw);
      continue;
    }
    let candidates = ds.parse_candidate(raw);
    if candidates.is_empty() {
      on_invalid(raw);
      continue;
    }
    if seen.insert(raw.to_owned()) {
      matches.push((raw.to_owned(), candidates));
    }
  }

  let flags = respect_important;
  let order = ds.variant_order();

  struct Sorted {
    node: Node,
    sort: PropertySort,
    variants_wide: Vec<u64>,
    candidate: String,
  }

  let mut nodes: Vec<Sorted> = Vec::new();
  for (raw, candidates) in &matches {
    let mut found = false;
    for (index, candidate) in candidates.iter().enumerate() {
      let rules = ds.compile_ast_nodes(raw, index, candidate, flags);
      if rules.is_empty() {
        continue;
      }
      found = true;
      for compiled in rules.iter() {
        let mut bits: Vec<u64> = Vec::new();
        for variant in &candidate.variants {
          let position = order.get(&(Rc::as_ptr(variant) as usize)).copied().unwrap_or(0);
          let word = position / 64;
          if bits.len() <= word {
            bits.resize(word + 1, 0);
          }
          bits[word] |= 1u64 << (position % 64);
        }
        nodes.push(Sorted {
          node: compiled.node.clone(),
          sort: compiled.property_sort.clone(),
          variants_wide: bits,
          candidate: raw.clone(),
        });
      }
    }
    if !found {
      on_invalid(raw);
    }
  }

  fn compare_bits(a: &[u64], z: &[u64]) -> Ordering {
    let len = a.len().max(z.len());
    for i in (0..len).rev() {
      let x = a.get(i).copied().unwrap_or(0);
      let y = z.get(i).copied().unwrap_or(0);
      if x != y {
        return x.cmp(&y);
      }
    }
    Ordering::Equal
  }

  nodes.sort_by(|a, z| {
    let by_variants = compare_bits(&a.variants_wide, &z.variants_wide);
    if by_variants != Ordering::Equal {
      return by_variants;
    }
    let mut offset = 0;
    while offset < a.sort.order.len() && offset < z.sort.order.len() && a.sort.order[offset] == z.sort.order[offset] {
      offset += 1;
    }
    let ap = a.sort.order.get(offset).map(|v| *v as f64).unwrap_or(f64::INFINITY);
    let zp = z.sort.order.get(offset).map(|v| *v as f64).unwrap_or(f64::INFINITY);
    let diff = ap - zp;
    if diff != 0.0 && !diff.is_nan() {
      return if diff < 0.0 { Ordering::Less } else { Ordering::Greater };
    }
    if z.sort.count != a.sort.count {
      return z.sort.count.cmp(&a.sort.count);
    }
    compare(&a.candidate, &z.candidate)
  });

  CompiledCandidates {
    nodes: nodes.into_iter().map(|s| (s.node, s.candidate)).collect(),
  }
}

pub fn compile_ast_nodes(candidate: &Candidate, ds: &DesignSystem, respect_important: bool) -> Vec<CompiledRule> {
  let asts = compile_base_utility(candidate, ds);
  if asts.is_empty() {
    return Vec::new();
  }

  let important = ds.important && respect_important;
  let selector = format!(".{}", escape(&candidate.raw));
  let mut rules = Vec::new();

  for mut nodes in asts {
    let property_sort = property_sort(&nodes);
    if candidate.important || important {
      apply_important(&mut nodes);
    }
    let mut node = Node::Rule {
      selector: selector.clone(),
      nodes,
    };
    for variant in &candidate.variants {
      if apply_variant(&mut node, variant, &ds.variants, ds, 0).is_none() {
        return Vec::new();
      }
    }
    rules.push(CompiledRule { node, property_sort });
  }
  rules
}

pub fn apply_variant(
  node: &mut Node,
  variant: &Rc<Variant>,
  variants: &Variants,
  ds: &DesignSystem,
  depth: usize,
) -> Option<()> {
  if let Variant::Arbitrary { selector, relative } = variant.as_ref() {
    if *relative && depth == 0 {
      return None;
    }
    let children = std::mem::take(node.children_mut()?);
    *node.children_mut()? = vec![rule(selector, children)];
    return Some(());
  }

  let entry = variants.get(variant.root()?)?;
  let apply = entry.apply.clone();

  if let Variant::Compound { root, variant: sub, .. } = variant.as_ref() {
    let mut isolated = at_rule("@slot", "", Vec::new());
    apply_variant(&mut isolated, sub, variants, ds, depth + 1)?;

    let children = isolated.children_mut()?;
    if root == "not" && children.len() > 1 {
      return None;
    }
    for child in children.iter_mut() {
      if !child.is_rule_or_at_rule() {
        return None;
      }
      apply(child, variant, ds)?;
    }

    let body = std::mem::take(node.children_mut()?);
    let mut isolated_children = std::mem::take(children);
    walk(&mut isolated_children, &mut |child, _| {
      if child.is_rule_or_at_rule() && child.children().is_some_and(|c| c.is_empty()) {
        *child.children_mut().expect("a rule") = body.clone();
        return Action::Skip;
      }
      Action::Continue
    });
    *node.children_mut()? = isolated_children;
    return Some(());
  }

  apply(node, variant, ds)
}

fn is_fallback(types: &[String]) -> bool {
  types.len() > 1 && types.iter().any(|t| t == "any")
}

fn compile_base_utility(candidate: &Candidate, ds: &DesignSystem) -> Vec<Vec<Node>> {
  if let CandidateKind::Arbitrary {
    property,
    value,
    modifier,
  } = &candidate.kind
  {
    let value = match modifier {
      Some(modifier) => as_color(value, Some(modifier), &ds.theme),
      None => Some(value.clone()),
    };
    return match value {
      Some(value) => vec![vec![decl(property.clone(), value)]],
      None => Vec::new(),
    };
  }

  let Some(root) = candidate.root() else {
    return Vec::new();
  };
  let utilities = ds.utilities.get(root);
  let kind = match candidate.kind {
    CandidateKind::Static { .. } => crate::utilities::Kind::Static,
    _ => crate::utilities::Kind::Functional,
  };

  let mut asts = Vec::new();
  for fallback_pass in [false, true] {
    for utility in utilities.iter().filter(|u| is_fallback(&u.types) == fallback_pass) {
      if utility.kind != kind {
        continue;
      }
      match (utility.compile)(candidate, ds) {
        Compiled::Skip => continue,
        Compiled::Invalid => {
          if !utility.types.is_empty() {
            return asts;
          }
          continue;
        }
        Compiled::Nodes(nodes) => asts.push(nodes),
      }
    }
    if !asts.is_empty() {
      return asts;
    }
  }
  asts
}

fn apply_important(ast: &mut [Node]) {
  for node in ast {
    match node {
      Node::AtRoot { .. } => continue,
      Node::Declaration { important, .. } => *important = true,
      Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } => apply_important(nodes),
      _ => {}
    }
  }
}

fn property_sort(nodes: &[Node]) -> PropertySort {
  let mut order = BTreeSet::new();
  let mut count = 0;
  let mut queue: std::collections::VecDeque<&Node> = nodes.iter().collect();
  let mut seen_tw_sort = false;

  while let Some(node) = queue.pop_front() {
    match node {
      Node::Declaration { property, value, .. } => {
        let Some(value) = value else { continue };
        count += 1;
        if seen_tw_sort {
          continue;
        }
        if property == "--tw-sort"
          && let Some(idx) = property_order::index_of(value)
        {
          order.insert(idx);
          seen_tw_sort = true;
          continue;
        }
        if let Some(idx) = property_order::index_of(property) {
          order.insert(idx);
        }
      }
      Node::Rule { nodes, .. } | Node::AtRule { nodes, .. } => queue.extend(nodes.iter()),
      _ => {}
    }
  }

  PropertySort {
    order: order.into_iter().collect(),
    count,
  }
}

pub(crate) type VariantOrder = HashMap<usize, usize>;
