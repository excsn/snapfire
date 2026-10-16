//! The design system: the theme, the utilities and variants registered from it and the
//! caches every compile shares.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::ast::Node;
use crate::candidate::{Candidate, Variant, parse_candidate, parse_variant};
use crate::compile::{CompiledRule, VariantOrder, compile_ast_nodes};
use crate::css_functions::substitute_functions;
use crate::theme::{Theme, ThemeOptions};
use crate::utilities::{Utilities, create_utilities, with_alpha};
use crate::utils::variables::extract_used_variables;
use crate::variants::{Variants, create_variants, substitute_at_variant};

/// A compiled candidate: whether `!important` from the stylesheet applies, the raw class and
/// which of its parses.
type CompiledKey = (bool, String, usize);

#[non_exhaustive]
pub struct DesignSystem {
  pub theme: Theme,
  pub utilities: Utilities,
  pub variants: Variants,
  pub invalid_candidates: RefCell<HashSet<String>>,
  pub important: bool,
  /// What Tailwind would print with `console.warn`.
  pub(crate) warnings: RefCell<Vec<String>>,
  /// The bodies of `@utility` rules, which `@apply` rewrites after they are registered.
  pub(crate) css_utilities: RefCell<Vec<Vec<Node>>>,
  pub(crate) legacy_theme: Option<crate::compat::LegacyTheme>,
  parsed_variants: RefCell<HashMap<String, Option<Rc<Variant>>>>,
  variant_list: RefCell<Vec<Rc<Variant>>>,
  parsed_candidates: RefCell<HashMap<String, Rc<Vec<Candidate>>>>,
  compiled: RefCell<HashMap<CompiledKey, Rc<Vec<CompiledRule>>>>,
  tracked: RefCell<HashSet<String>>,
}

impl DesignSystem {
  pub fn new(theme: Theme) -> DesignSystem {
    let utilities = create_utilities(&theme);
    let variants = create_variants(&theme);
    DesignSystem {
      theme,
      utilities,
      variants,
      invalid_candidates: RefCell::new(HashSet::new()),
      important: false,
      warnings: RefCell::new(Vec::new()),
      css_utilities: RefCell::new(Vec::new()),
      legacy_theme: None,
      parsed_variants: RefCell::new(HashMap::new()),
      variant_list: RefCell::new(Vec::new()),
      parsed_candidates: RefCell::new(HashMap::new()),
      compiled: RefCell::new(HashMap::new()),
      tracked: RefCell::new(HashSet::new()),
    }
  }

  pub fn parse_candidate(&self, raw: &str) -> Rc<Vec<Candidate>> {
    if let Some(found) = self.parsed_candidates.borrow().get(raw) {
      return found.clone();
    }
    let parsed = Rc::new(parse_candidate(raw, self));
    self
      .parsed_candidates
      .borrow_mut()
      .insert(raw.to_owned(), parsed.clone());
    parsed
  }

  pub fn parse_variant(&self, raw: &str) -> Option<Rc<Variant>> {
    if let Some(found) = self.parsed_variants.borrow().get(raw) {
      return found.clone();
    }
    let parsed = parse_variant(raw, self).map(Rc::new);
    if let Some(v) = &parsed {
      self.variant_list.borrow_mut().push(v.clone());
    }
    self.parsed_variants.borrow_mut().insert(raw.to_owned(), parsed.clone());
    parsed
  }

  pub(crate) fn compile_ast_nodes(
    &self,
    raw: &str,
    index: usize,
    candidate: &Candidate,
    respect_important: bool,
  ) -> Rc<Vec<CompiledRule>> {
    let key = (respect_important, raw.to_owned(), index);
    if let Some(found) = self.compiled.borrow().get(&key) {
      return found.clone();
    }
    let mut rules = compile_ast_nodes(candidate, self, respect_important);
    let mut nodes: Vec<Node> = rules.iter().map(|r| r.node.clone()).collect();
    let ok = substitute_functions(&mut nodes, self).is_ok() && substitute_at_variant(&mut nodes, self).is_ok();
    let rules = if ok {
      for (rule, node) in rules.iter_mut().zip(nodes) {
        rule.node = node;
      }
      rules
    } else {
      Vec::new()
    };
    let rules = Rc::new(rules);
    self.compiled.borrow_mut().insert(key, rules.clone());
    rules
  }

  /// Every variant parsed so far, numbered by the order they sort in; equal variants share a number.
  pub(crate) fn variant_order(&self) -> VariantOrder {
    let mut variants: Vec<Rc<Variant>> = self.variant_list.borrow().clone();
    variants.sort_by(|a, z| self.variants.compare(Some(a), Some(z), self));

    let mut order = VariantOrder::new();
    let mut previous: Option<&Rc<Variant>> = None;
    let mut index = 0usize;
    for variant in &variants {
      if let Some(prev) = previous
        && self.variants.compare(Some(prev), Some(variant), self) != std::cmp::Ordering::Equal
      {
        index += 1;
      }
      order.insert(Rc::as_ptr(variant) as usize, index);
      previous = Some(variant);
    }
    order
  }

  /// `--theme(…)` and `theme(…)`: a theme value by variable name or, for `theme()`, by the
  /// older dotted path, with an optional `/ alpha`.
  pub fn resolve_theme_value(&self, path: &str, force_inline: bool) -> Option<String> {
    if !path.starts_with("--") {
      return self.legacy_theme.as_ref().and_then(|legacy| legacy.resolve(path, self));
    }
    self.resolve_theme_variable(path, force_inline)
  }

  pub(crate) fn resolve_theme_variable(&self, path: &str, force_inline: bool) -> Option<String> {
    let (path, modifier) = match path.rfind('/') {
      Some(i) => (path[..i].trim(), Some(path[i + 1..].trim())),
      None => (path, None),
    };
    let value = self.theme.resolve(
      None,
      &[path],
      if force_inline {
        ThemeOptions::INLINE
      } else {
        ThemeOptions::NONE
      },
    )?;
    match modifier {
      Some(m) if !m.is_empty() && !value.is_empty() => Some(with_alpha(&value, m)),
      _ => Some(value),
    }
  }

  pub fn track_used_variables(&self, raw: &str) {
    if !self.tracked.borrow_mut().insert(raw.to_owned()) {
      return;
    }
    for variable in extract_used_variables(raw) {
      self.theme.mark_used_variable(&variable);
    }
  }

  /// One candidate as CSS without polyfills. `None` when it compiles to nothing.
  pub fn candidate_to_css(&self, raw: &str) -> Option<String> {
    let mut valid = true;
    let compiled = crate::compile::compile_candidates([raw], self, true, &mut |_| valid = false);
    let nodes: Vec<Node> = compiled.nodes.into_iter().map(|(node, _)| node).collect();
    let nodes = crate::ast::optimize_ast(&nodes, self, crate::ast::Polyfills::NONE);
    if !valid || nodes.is_empty() {
      return None;
    }
    Some(crate::ast::to_css(&nodes))
  }
}
