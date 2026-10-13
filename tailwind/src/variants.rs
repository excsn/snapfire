//! Variants: what `hover:`, `md:`, `group-focus:` and `[&_p]:` do to a rule and the order
//! they sort in.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::LazyLock;

use indexmap::IndexMap;
use regex::Regex;

use crate::Error;
use crate::ast::{Node, at_root, at_rule, decl, rule, style_rule};
use crate::candidate::{Variant, VariantValue};
use crate::compile::apply_variant;
use crate::design_system::DesignSystem;
use crate::features::Features;
use crate::theme::Theme;
use crate::utils::compare::compare_breakpoints;
use crate::utils::infer::is_positive_integer;
use crate::utils::segment::segment;
use crate::value_parser::{self, ValueNode};
use crate::walk::{Action, Ancestor, walk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantKind {
  Static,
  Functional,
  Compound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Compounds(pub u8);

impl Compounds {
  pub const NEVER: Compounds = Compounds(0);
  pub const AT_RULES: Compounds = Compounds(1);
  pub const STYLE_RULES: Compounds = Compounds(2);
}

impl std::ops::BitOr for Compounds {
  type Output = Compounds;
  fn bitor(self, rhs: Compounds) -> Compounds {
    Compounds(self.0 | rhs.0)
  }
}

/// Applies a variant to a rule. `None` means the variant cannot apply and the candidate
/// produces nothing.
pub type ApplyFn = Rc<dyn Fn(&mut Node, &Variant, &DesignSystem) -> Option<()>>;
pub type CompareFn = Rc<dyn Fn(&Variant, &Variant, &DesignSystem) -> Ordering>;

#[derive(Clone)]
pub struct VariantEntry {
  pub kind: VariantKind,
  pub order: u32,
  pub apply: ApplyFn,
  pub compounds_with: Compounds,
  pub compounds: Compounds,
}

#[derive(Default)]
pub struct Variants {
  pub compare_fns: HashMap<u32, CompareFn>,
  variants: IndexMap<String, VariantEntry>,
  group_order: Option<u32>,
  last_order: u32,
}

pub static IS_VALID_VARIANT_NAME: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^@?[a-z0-9][a-zA-Z0-9_-]*$").unwrap());

pub fn is_valid_variant_name(name: &str) -> bool {
  IS_VALID_VARIANT_NAME.is_match(name) && !name.ends_with('_') && !name.ends_with('-')
}

impl Variants {
  pub fn static_variant(&mut self, name: &str, apply: ApplyFn, compounds: Option<Compounds>, order: Option<u32>) {
    self.set(
      name,
      VariantKind::Static,
      apply,
      Compounds::NEVER,
      compounds.unwrap_or(Compounds::STYLE_RULES),
      order,
    );
  }

  pub fn functional(&mut self, name: &str, apply: ApplyFn, compounds: Option<Compounds>) {
    self.set(
      name,
      VariantKind::Functional,
      apply,
      Compounds::NEVER,
      compounds.unwrap_or(Compounds::STYLE_RULES),
      None,
    );
  }

  pub fn compound(&mut self, name: &str, compounds_with: Compounds, apply: ApplyFn) {
    self.set(
      name,
      VariantKind::Compound,
      apply,
      compounds_with,
      Compounds::STYLE_RULES,
      None,
    );
  }

  /// A static variant whose body is CSS with `@slot` where the rule goes.
  pub fn body_variant(&mut self, name: &str, ast: Vec<Node>) {
    let mut selectors = Vec::new();
    let mut uses_at_variant = false;
    crate::walk::walk_ref(&ast, &mut |node, _| {
      match node {
        Node::Rule { selector, .. } => selectors.push(selector.clone()),
        Node::AtRule { name, .. } if name == "@variant" => uses_at_variant = true,
        Node::AtRule { name, params, .. } if name != "@slot" => selectors.push(format!("{name} {params}")),
        _ => {}
      }
      true
    });
    let ast = Rc::new(ast);
    let compounds = compounds_for_selectors(&selectors.iter().map(String::as_str).collect::<Vec<_>>());
    self.static_variant(
      name,
      Rc::new(move |r, _, ds| {
        let mut body: Vec<Node> = (*ast).clone();
        if uses_at_variant && substitute_at_variant(&mut body, ds).is_err() {
          return None;
        }
        let nodes = std::mem::take(r.children_mut()?);
        substitute_at_slot(&mut body, &nodes);
        *r.children_mut()? = body;
        Some(())
      }),
      Some(compounds),
      None,
    );
  }

  pub fn group(&mut self, f: impl FnOnce(&mut Variants), compare: Option<CompareFn>) {
    let order = self.next_order();
    self.group_order = Some(order);
    if let Some(compare) = compare {
      self.compare_fns.insert(order, compare);
    }
    f(self);
    self.group_order = None;
  }

  pub fn has(&self, name: &str) -> bool {
    self.variants.contains_key(name)
  }

  pub fn get(&self, name: &str) -> Option<&VariantEntry> {
    self.variants.get(name)
  }

  pub fn kind(&self, name: &str) -> Option<VariantKind> {
    self.variants.get(name).map(|v| v.kind)
  }

  pub fn compounds_with(&self, parent: &str, child: &Variant) -> bool {
    let Some(parent_info) = self.variants.get(parent) else {
      return false;
    };
    let child_compounds = match child {
      Variant::Arbitrary { selector, .. } => compounds_for_selectors(&[selector.as_str()]),
      other => match self.variants.get(other.root().unwrap_or("")) {
        Some(info) => info.compounds,
        None => return false,
      },
    };
    if parent_info.kind != VariantKind::Compound {
      return false;
    }
    if child_compounds == Compounds::NEVER {
      return false;
    }
    if parent_info.compounds_with == Compounds::NEVER {
      return false;
    }
    parent_info.compounds_with.0 & child_compounds.0 != 0
  }

  pub fn compare(&self, a: Option<&Rc<Variant>>, z: Option<&Rc<Variant>>, ds: &DesignSystem) -> Ordering {
    match (a, z) {
      (None, None) => Ordering::Equal,
      (None, _) => Ordering::Less,
      (_, None) => Ordering::Greater,
      (Some(a), Some(z)) => self.compare_variants(a, z, ds),
    }
  }

  pub fn compare_variants(&self, a: &Variant, z: &Variant, ds: &DesignSystem) -> Ordering {
    if std::ptr::eq(a, z) {
      return Ordering::Equal;
    }

    match (a, z) {
      (Variant::Arbitrary { selector: sa, .. }, Variant::Arbitrary { selector: sz, .. }) => {
        return if sa < sz { Ordering::Less } else { Ordering::Greater };
      }
      (Variant::Arbitrary { .. }, _) => return Ordering::Greater,
      (_, Variant::Arbitrary { .. }) => return Ordering::Less,
      _ => {}
    }

    let a_order = self.variants.get(a.root().unwrap()).map(|v| v.order).unwrap_or(0);
    let z_order = self.variants.get(z.root().unwrap()).map(|v| v.order).unwrap_or(0);
    if a_order != z_order {
      return a_order.cmp(&z_order);
    }

    if let (
      Variant::Compound {
        variant: av,
        modifier: am,
        ..
      },
      Variant::Compound {
        variant: zv,
        modifier: zm,
        ..
      },
    ) = (a, z)
    {
      let order = self.compare_variants(av, zv, ds);
      if order != Ordering::Equal {
        return order;
      }
      return match (am, zm) {
        (Some(am), Some(zm)) => {
          if am.value() < zm.value() {
            Ordering::Less
          } else {
            Ordering::Greater
          }
        }
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
      };
    }

    if let Some(compare) = self.compare_fns.get(&a_order) {
      return compare(a, z, ds);
    }

    let (ar, zr) = (a.root().unwrap(), z.root().unwrap());
    if ar != zr {
      return if ar < zr { Ordering::Less } else { Ordering::Greater };
    }

    let a_value = match a {
      Variant::Functional { value, .. } => value.as_ref(),
      _ => None,
    };
    let z_value = match z {
      Variant::Functional { value, .. } => value.as_ref(),
      _ => None,
    };
    let Some(a_value) = a_value else { return Ordering::Less };
    let Some(z_value) = z_value else {
      return Ordering::Greater;
    };

    match (a_value, z_value) {
      (VariantValue::Arbitrary(_), VariantValue::Named(_)) => return Ordering::Greater,
      (VariantValue::Named(_), VariantValue::Arbitrary(_)) => return Ordering::Less,
      _ => {}
    }
    if a_value.value() < z_value.value() {
      Ordering::Less
    } else {
      Ordering::Greater
    }
  }

  fn set(
    &mut self,
    name: &str,
    kind: VariantKind,
    apply: ApplyFn,
    compounds_with: Compounds,
    compounds: Compounds,
    order: Option<u32>,
  ) {
    if let Some(existing) = self.variants.get_mut(name) {
      existing.kind = kind;
      existing.apply = apply;
      existing.compounds = compounds;
    } else {
      let order = match order {
        Some(order) => order,
        None => {
          self.last_order = self.next_order();
          self.last_order
        }
      };
      self.variants.insert(
        name.to_owned(),
        VariantEntry {
          kind,
          order,
          apply,
          compounds_with,
          compounds,
        },
      );
    }
  }

  fn next_order(&self) -> u32 {
    self.group_order.unwrap_or(self.last_order + 1)
  }
}

pub fn compounds_for_selectors(selectors: &[&str]) -> Compounds {
  let mut compounds = Compounds::NEVER;
  for sel in selectors {
    if sel.starts_with('@') {
      if !sel.starts_with("@media") && !sel.starts_with("@supports") && !sel.starts_with("@container") {
        return Compounds::NEVER;
      }
      compounds = compounds | Compounds::AT_RULES;
      continue;
    }
    if sel.contains("::") {
      return Compounds::NEVER;
    }
    compounds = compounds | Compounds::STYLE_RULES;
  }
  compounds
}

fn wrap_selectors(selectors: Vec<String>) -> ApplyFn {
  Rc::new(move |r, _, _| {
    let nodes = std::mem::take(r.children_mut()?);
    *r.children_mut()? = selectors.iter().map(|s| rule(s, nodes.clone())).collect();
    Some(())
  })
}

fn static_selectors(variants: &mut Variants, name: &str, selectors: &[&str], compounds: Option<Compounds>) {
  let compounds = compounds.unwrap_or_else(|| compounds_for_selectors(selectors));
  variants.static_variant(
    name,
    wrap_selectors(selectors.iter().map(|s| (*s).to_owned()).collect()),
    Some(compounds),
    None,
  );
}

fn negate_conditions(rule_name: &str, conditions: Vec<String>) -> Vec<String> {
  conditions
    .into_iter()
    .map(|condition| {
      if rule_name == "@container" {
        let mut ast = value_parser::parse(condition.trim());
        let is_word = |n: &ValueNode| matches!(n, ValueNode::Word(_));
        let is_fn = |n: &ValueNode| matches!(n, ValueNode::Function { .. });
        if !ast.is_empty() && is_fn(&ast[0]) {
          return format!("not {condition}");
        } else if ast.len() >= 3 && ast[0] == ValueNode::word("not") && is_fn(&ast[2]) {
          ast.drain(0..2);
          return value_parser::to_css(&ast);
        } else if ast.len() >= 5 && is_word(&ast[0]) && ast[2] == ValueNode::word("not") && is_fn(&ast[4]) {
          ast.drain(2..4);
          return value_parser::to_css(&ast);
        } else if ast.len() >= 3 && is_word(&ast[0]) && ast[0] != ValueNode::word("not") && is_fn(&ast[2]) {
          ast.splice(1..1, [ValueNode::Separator(" ".to_owned()), ValueNode::word("not")]);
          return value_parser::to_css(&ast);
        }
        return format!("not {condition}");
      }

      let condition = condition.trim();
      let parts = segment(condition, b' ');
      if parts[0] == "not" {
        return parts[1..].join(" ");
      }
      format!("not {condition}")
    })
    .collect()
}

fn negate_at_rule(name: &str, params: &str) -> Option<Node> {
  if !matches!(name, "@media" | "@supports" | "@container") {
    return None;
  }
  let conditions: Vec<String> = segment(params, b',').into_iter().map(str::to_owned).collect();
  if conditions.len() > 1 {
    return None;
  }
  let conditions = negate_conditions(name, conditions);
  Some(at_rule(name, conditions.join(", "), Vec::new()))
}

fn negate_selector(selector: &str) -> Option<String> {
  if selector.contains("::") {
    return None;
  }
  let selectors: Vec<String> = segment(selector, b',')
    .into_iter()
    .map(|s| s.replace('&', "*"))
    .collect();
  Some(format!("&:not({})", selectors.join(", ")))
}

fn take_rule(node: &mut Node) -> Node {
  std::mem::replace(node, Node::Comment(String::new()))
}

/// Walks a lone rule the way the compound variants do, with the rule as the walk's root.
fn walk_root(node: &mut Node, f: &mut impl FnMut(&mut Node, &crate::walk::Visit) -> Action<Node>) {
  let mut root = vec![take_rule(node)];
  walk(&mut root, f);
  *node = root.pop().expect("the walked rule");
}

fn not_variant() -> ApplyFn {
  Rc::new(|rule_node, variant, _| {
    let Variant::Compound {
      variant: sub, modifier, ..
    } = variant
    else {
      return None;
    };
    if let Variant::Arbitrary { relative: true, .. } = sub.as_ref() {
      return None;
    }
    if modifier.is_some() {
      return None;
    }

    let mut did_apply = false;
    let mut replacement: Option<Node> = None;

    walk_root(rule_node, &mut |node, visit| {
      if !node.is_rule_or_at_rule() {
        return Action::Continue;
      }
      if node.children().is_some_and(|c| !c.is_empty()) {
        return Action::Continue;
      }

      let mut at_rules: Vec<(String, String)> = Vec::new();
      let mut style_rules: Vec<String> = Vec::new();
      for ancestor in visit.path() {
        match ancestor {
          Ancestor::AtRule { name, params } => at_rules.push((name.clone(), params.clone())),
          Ancestor::Rule { selector } => style_rules.push(selector.clone()),
          _ => {}
        }
      }
      match node {
        Node::AtRule { name, params, .. } => at_rules.push((name.clone(), params.clone())),
        Node::Rule { selector, .. } => style_rules.push(selector.clone()),
        _ => {}
      }

      if at_rules.len() > 1 || style_rules.len() > 1 {
        return Action::Stop;
      }

      let mut rules = Vec::new();
      for selector in &style_rules {
        let Some(negated) = negate_selector(selector) else {
          did_apply = false;
          return Action::Stop;
        };
        rules.push(style_rule(negated, Vec::new()));
      }
      for (name, params) in &at_rules {
        let Some(negated) = negate_at_rule(name, params) else {
          did_apply = false;
          return Action::Stop;
        };
        rules.push(negated);
      }

      replacement = Some(style_rule("&", rules));
      did_apply = true;
      Action::Skip
    });

    if let Some(replacement) = replacement {
      *rule_node = replacement;
    }

    if let Node::Rule { selector, nodes } = rule_node
      && selector == "&"
      && nodes.len() == 1
    {
      let child = nodes.pop().unwrap();
      *rule_node = child;
    }

    if !did_apply { None } else { Some(()) }
  })
}

/// `group-*`, `peer-*`, `in-*` and `has-*`: rewrites every style rule's selector, refusing
/// nested style rules.
fn selector_compound(
  rewrite: impl Fn(&str, &Variant, &DesignSystem) -> Option<String> + 'static,
  needs_no_modifier: bool,
  refuse_relative: bool,
) -> ApplyFn {
  Rc::new(move |rule_node, variant, ds| {
    let Variant::Compound {
      variant: sub, modifier, ..
    } = variant
    else {
      return None;
    };
    if refuse_relative && let Variant::Arbitrary { relative: true, .. } = sub.as_ref() {
      return None;
    }
    if needs_no_modifier && modifier.is_some() {
      return None;
    }

    let mut did_apply = false;
    walk_root(rule_node, &mut |node, visit| {
      let Node::Rule { selector, .. } = node else {
        return Action::Continue;
      };
      if visit.path().iter().any(|a| matches!(a, Ancestor::Rule { .. })) {
        did_apply = false;
        return Action::Stop;
      }
      match rewrite(selector, variant, ds) {
        Some(next) => *selector = next,
        None => return Action::Continue,
      }
      did_apply = true;
      Action::Continue
    });

    if !did_apply { None } else { Some(()) }
  })
}

fn group_like(rule_name: &'static str, combinator: &'static str) -> ApplyFn {
  selector_compound(
    move |selector, variant, ds| {
      let prefix = match &ds.theme.prefix {
        Some(p) => format!("{p}\\:"),
        None => String::new(),
      };
      let variant_selector = match variant.modifier() {
        Some(m) => format!(":where(.{prefix}{rule_name}\\/{})", m.value()),
        None => format!(":where(.{prefix}{rule_name})"),
      };
      let mut selector = selector.replace('&', &variant_selector);
      if segment(&selector, b',').len() > 1 {
        selector = format!(":is({selector})");
      }
      Some(format!("&:is({selector}{combinator})"))
    },
    false,
    true,
  )
}

fn quote_attribute_value(input: &str) -> String {
  if !input.contains('=') {
    return input.to_owned();
  }
  let parts = segment(input, b'=');
  let attribute = parts[0];
  let value = parts[1..].join("=");
  let value = value.trim();
  if value.starts_with('\'') || value.starts_with('"') {
    return input.to_owned();
  }
  let chars: Vec<char> = value.chars().collect();
  if chars.len() > 1 {
    let trailing = chars[chars.len() - 1];
    if chars[chars.len() - 2] == ' ' && matches!(trailing, 'i' | 'I' | 's' | 'S') {
      let inner: String = chars[..chars.len() - 2].iter().collect();
      return format!("{attribute}=\"{inner}\" {trailing}");
    }
  }
  format!("{attribute}=\"{value}\"")
}

fn wrap_one(r: &mut Node, f: impl FnOnce(Vec<Node>) -> Node) -> Option<()> {
  let nodes = std::mem::take(r.children_mut()?);
  *r.children_mut()? = vec![f(nodes)];
  Some(())
}

fn functional_value(variant: &Variant) -> Option<&VariantValue> {
  match variant {
    Variant::Functional {
      value, modifier: None, ..
    } => value.as_ref(),
    _ => None,
  }
}

fn nth(pseudo: &'static str) -> ApplyFn {
  Rc::new(move |r, variant, _| {
    let value = functional_value(variant)?;
    if let VariantValue::Named(v) = value
      && !is_positive_integer(v)
    {
      return None;
    }
    let selector = format!("&:{pseudo}({})", value.value());
    wrap_one(r, |nodes| style_rule(selector, nodes))
  })
}

static SUPPORTS_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(?-u:[\w-])*(?-u:\s)*\(").unwrap());
static SUPPORTS_WORDS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?-u:\b)(and|or|not)(?-u:\b)").unwrap());

fn breakpoint_value(variant: &Variant, theme: &Theme, namespace: &str, allow_static: bool) -> Option<String> {
  match variant {
    Variant::Static { root } if allow_static => theme.resolve_value(Some(root), &[namespace]),
    Variant::Functional { value, modifier, .. } => {
      if allow_static && modifier.is_some() {
        return None;
      }
      let value = match value.as_ref()? {
        VariantValue::Arbitrary(v) => Some(v.clone()),
        VariantValue::Named(v) => theme.resolve_value(Some(v), &[namespace]),
      }?;
      if value.is_empty() || value.contains("var(") {
        return None;
      }
      Some(value)
    }
    _ => None,
  }
}

fn compare_breakpoint_variants(
  a: &Variant,
  z: &Variant,
  ascending: bool,
  lookup: impl Fn(&Variant) -> Option<String>,
) -> Ordering {
  if std::ptr::eq(a, z) {
    return Ordering::Equal;
  }
  let Some(a_value) = lookup(a) else {
    return if ascending { Ordering::Less } else { Ordering::Greater };
  };
  let Some(z_value) = lookup(z) else {
    return if ascending { Ordering::Greater } else { Ordering::Less };
  };
  compare_breakpoints(&a_value, &z_value, ascending)
}

pub fn create_variants(theme: &Theme) -> Variants {
  let mut variants = Variants::default();

  static_selectors(&mut variants, "*", &[":is(& > *)"], Some(Compounds::NEVER));
  static_selectors(&mut variants, "**", &[":is(& *)"], Some(Compounds::NEVER));

  variants.compound("not", Compounds::STYLE_RULES | Compounds::AT_RULES, not_variant());
  variants.compound("group", Compounds::STYLE_RULES, group_like("group", " *"));
  variants.compound("peer", Compounds::STYLE_RULES, group_like("peer", " ~ *"));

  static_selectors(&mut variants, "first-letter", &["&::first-letter"], None);
  static_selectors(&mut variants, "first-line", &["&::first-line"], None);
  static_selectors(
    &mut variants,
    "marker",
    &[
      "& *::marker",
      "&::marker",
      "& *::-webkit-details-marker",
      "&::-webkit-details-marker",
    ],
    None,
  );
  static_selectors(&mut variants, "selection", &["& *::selection", "&::selection"], None);
  static_selectors(&mut variants, "file", &["&::file-selector-button"], None);
  static_selectors(&mut variants, "placeholder", &["&::placeholder"], None);
  static_selectors(&mut variants, "backdrop", &["&::backdrop"], None);
  static_selectors(&mut variants, "details-content", &["&::details-content"], None);

  for (name, pseudo) in [("before", "&::before"), ("after", "&::after")] {
    variants.static_variant(
      name,
      Rc::new(move |v, _, _| {
        let nodes = std::mem::take(v.children_mut()?);
        let mut body = vec![
          at_root(vec![at_rule(
            "@property",
            "--tw-content",
            vec![
              decl("syntax", "\"*\""),
              decl("initial-value", "\"\""),
              decl("inherits", "false"),
            ],
          )]),
          decl("content", "var(--tw-content)"),
        ];
        body.extend(nodes);
        *v.children_mut()? = vec![style_rule(pseudo, body)];
        Some(())
      }),
      Some(Compounds::NEVER),
      None,
    );
  }

  for (name, selector) in [
    ("first", "&:first-child"),
    ("last", "&:last-child"),
    ("only", "&:only-child"),
    ("odd", "&:nth-child(odd)"),
    ("even", "&:nth-child(even)"),
    ("first-of-type", "&:first-of-type"),
    ("last-of-type", "&:last-of-type"),
    ("only-of-type", "&:only-of-type"),
    ("visited", "&:visited"),
    ("target", "&:target"),
    ("open", "&:is([open], :popover-open, :open)"),
    ("default", "&:default"),
    ("checked", "&:checked"),
    ("indeterminate", "&:indeterminate"),
    ("placeholder-shown", "&:placeholder-shown"),
    ("autofill", "&:autofill"),
    ("optional", "&:optional"),
    ("required", "&:required"),
    ("valid", "&:valid"),
    ("invalid", "&:invalid"),
    ("user-valid", "&:user-valid"),
    ("user-invalid", "&:user-invalid"),
    ("in-range", "&:in-range"),
    ("out-of-range", "&:out-of-range"),
    ("read-only", "&:read-only"),
    ("empty", "&:empty"),
    ("focus-within", "&:focus-within"),
  ] {
    static_selectors(&mut variants, name, &[selector], None);
  }

  variants.static_variant(
    "hover",
    Rc::new(|r, _, _| {
      wrap_one(r, |nodes| {
        style_rule("&:hover", vec![at_rule("@media", "(hover: hover)", nodes)])
      })
    }),
    None,
    None,
  );

  for (name, selector) in [
    ("focus", "&:focus"),
    ("focus-visible", "&:focus-visible"),
    ("active", "&:active"),
    ("enabled", "&:enabled"),
    ("disabled", "&:disabled"),
    ("inert", "&:is([inert], [inert] *)"),
  ] {
    static_selectors(&mut variants, name, &[selector], None);
  }

  variants.compound(
    "in",
    Compounds::STYLE_RULES,
    selector_compound(
      |selector, _, _| Some(format!(":where({}) &", selector.replace('&', "*"))),
      true,
      false,
    ),
  );
  variants.compound(
    "has",
    Compounds::STYLE_RULES,
    selector_compound(
      |selector, _, _| Some(format!("&:has({})", selector.replace('&', "*"))),
      true,
      false,
    ),
  );

  variants.functional(
    "aria",
    Rc::new(|r, variant, _| {
      let value = functional_value(variant)?;
      let selector = match value {
        VariantValue::Arbitrary(v) => format!("&[aria-{}]", quote_attribute_value(v)),
        VariantValue::Named(v) => format!("&[aria-{v}=\"true\"]"),
      };
      wrap_one(r, |nodes| style_rule(selector, nodes))
    }),
    None,
  );

  variants.functional(
    "data",
    Rc::new(|r, variant, _| {
      let value = functional_value(variant)?;
      let selector = format!("&[data-{}]", quote_attribute_value(value.value()));
      wrap_one(r, |nodes| style_rule(selector, nodes))
    }),
    None,
  );

  variants.functional("nth", nth("nth-child"), None);
  variants.functional("nth-last", nth("nth-last-child"), None);
  variants.functional("nth-of-type", nth("nth-of-type"), None);
  variants.functional("nth-last-of-type", nth("nth-last-of-type"), None);

  variants.functional(
    "supports",
    Rc::new(|r, variant, _| {
      let mut value = functional_value(variant)?.value().to_owned();
      if SUPPORTS_FN.is_match(&value) {
        let query = SUPPORTS_WORDS.replace_all(&value, " $1 ").into_owned();
        return wrap_one(r, |nodes| at_rule("@supports", query, nodes));
      }
      if !value.contains(':') {
        value = format!("{value}: var(--tw)");
      }
      if !value.starts_with('(') || !value.ends_with(')') {
        value = format!("({value})");
      }
      wrap_one(r, |nodes| at_rule("@supports", value, nodes))
    }),
    Some(Compounds::AT_RULES),
  );

  for (name, query) in [
    ("motion-safe", "@media (prefers-reduced-motion: no-preference)"),
    ("motion-reduce", "@media (prefers-reduced-motion: reduce)"),
    ("contrast-more", "@media (prefers-contrast: more)"),
    ("contrast-less", "@media (prefers-contrast: less)"),
  ] {
    static_selectors(&mut variants, name, &[query], None);
  }

  {
    let breakpoint = |variant: &Variant, ds: &DesignSystem| breakpoint_value(variant, &ds.theme, "--breakpoint", true);

    variants.group(
      |variants| {
        variants.functional(
          "max",
          Rc::new(move |r, variant, ds| {
            if variant.modifier().is_some() {
              return None;
            }
            let value = breakpoint(variant, ds)?;
            wrap_one(r, |nodes| at_rule("@media", format!("(width < {value})"), nodes))
          }),
          Some(Compounds::AT_RULES),
        );
      },
      Some(Rc::new(move |a, z, ds| {
        compare_breakpoint_variants(a, z, false, |v| breakpoint(v, ds))
      })),
    );

    let breakpoints: Vec<(String, String)> = theme
      .namespace("--breakpoint")
      .into_iter()
      .filter_map(|(k, v)| k.map(|k| (k, v)))
      .collect();
    variants.group(
      |variants| {
        for (key, value) in breakpoints {
          variants.static_variant(
            &key,
            Rc::new(move |r, _, _| wrap_one(r, |nodes| at_rule("@media", format!("(width >= {value})"), nodes))),
            Some(Compounds::AT_RULES),
            None,
          );
        }
        variants.functional(
          "min",
          Rc::new(move |r, variant, ds| {
            if variant.modifier().is_some() {
              return None;
            }
            let value = breakpoint(variant, ds)?;
            wrap_one(r, |nodes| at_rule("@media", format!("(width >= {value})"), nodes))
          }),
          Some(Compounds::AT_RULES),
        );
      },
      Some(Rc::new(move |a, z, ds| {
        compare_breakpoint_variants(a, z, true, |v| breakpoint(v, ds))
      })),
    );
  }

  {
    let width = |variant: &Variant, ds: &DesignSystem| breakpoint_value(variant, &ds.theme, "--container", false);
    let container = |less: bool| -> ApplyFn {
      Rc::new(move |r, variant, ds| {
        let value = width(variant, ds)?;
        let query = if less {
          format!("(width < {value})")
        } else {
          format!("(width >= {value})")
        };
        let params = match variant.modifier() {
          Some(m) => format!("{} {query}", m.value()),
          None => query,
        };
        wrap_one(r, |nodes| at_rule("@container", params, nodes))
      })
    };

    variants.group(
      |variants| variants.functional("@max", container(true), Some(Compounds::AT_RULES)),
      Some(Rc::new(move |a, z, ds| {
        compare_breakpoint_variants(a, z, false, |v| width(v, ds))
      })),
    );
    variants.group(
      |variants| {
        variants.functional("@", container(false), Some(Compounds::AT_RULES));
        variants.functional("@min", container(false), Some(Compounds::AT_RULES));
      },
      Some(Rc::new(move |a, z, ds| {
        compare_breakpoint_variants(a, z, true, |v| width(v, ds))
      })),
    );
  }

  for (name, selector) in [
    ("portrait", "@media (orientation: portrait)"),
    ("landscape", "@media (orientation: landscape)"),
    ("ltr", "&:where(:dir(ltr), [dir=\"ltr\"], [dir=\"ltr\"] *)"),
    ("rtl", "&:where(:dir(rtl), [dir=\"rtl\"], [dir=\"rtl\"] *)"),
    ("dark", "@media (prefers-color-scheme: dark)"),
    ("starting", "@starting-style"),
    ("print", "@media print"),
    ("forced-colors", "@media (forced-colors: active)"),
    ("inverted-colors", "@media (inverted-colors: inverted)"),
    ("pointer-none", "@media (pointer: none)"),
    ("pointer-coarse", "@media (pointer: coarse)"),
    ("pointer-fine", "@media (pointer: fine)"),
    ("any-pointer-none", "@media (any-pointer: none)"),
    ("any-pointer-coarse", "@media (any-pointer: coarse)"),
    ("any-pointer-fine", "@media (any-pointer: fine)"),
    ("noscript", "@media (scripting: none)"),
  ] {
    static_selectors(&mut variants, name, &[selector], None);
  }

  variants
}

/// Replaces `@slot` with `nodes` and hoists `@keyframes` and `@property` out of the rule.
pub fn substitute_at_slot(ast: &mut Vec<Node>, nodes: &[Node]) {
  walk(ast, &mut |node, _| match node {
    Node::AtRule { name, .. } if name == "@slot" => Action::ReplaceSkip(nodes.to_vec()),
    Node::AtRule {
      name,
      params,
      nodes: children,
    } if name == "@keyframes" || name == "@property" => {
      let inner = at_rule(name.clone(), params.clone(), std::mem::take(children));
      *node = at_root(vec![inner]);
      Action::Skip
    }
    _ => Action::Continue,
  });
}

/// Replaces every `@variant name { … }` with the rule the variant makes.
pub fn substitute_at_variant(ast: &mut Vec<Node>, ds: &DesignSystem) -> Result<Features, Error> {
  let mut features = Features::NONE;
  let mut error: Option<Error> = None;
  walk(ast, &mut |variant_node, _| {
    let Node::AtRule {
      name,
      params,
      nodes: body,
    } = variant_node
    else {
      return Action::Continue;
    };
    if name != "@variant" {
      return Action::Continue;
    }

    let mut out = Vec::new();
    let compound = segment(params, b',');
    let count = compound.len();
    for (idx, compound_variant) in compound.iter().enumerate() {
      let nodes = if idx == count - 1 {
        std::mem::take(body)
      } else {
        body.clone()
      };
      let mut node = style_rule("&", nodes);
      let stacked = segment(compound_variant, b':');
      for raw in stacked.iter().rev() {
        let raw = raw.trim();
        if raw.is_empty() {
          error = Some(Error::Message("Cannot use `@variant` with empty variant".to_owned()));
          return Action::Stop;
        }
        let Some(parsed) = ds.parse_variant(raw) else {
          error = Some(Error::Message(format!(
            "Cannot use `@variant` with unknown variant: {raw}"
          )));
          return Action::Stop;
        };
        if apply_variant(&mut node, &parsed, &ds.variants, ds, 0).is_none() {
          error = Some(Error::Message(format!("Cannot use `@variant` with variant: {raw}")));
          return Action::Stop;
        }
      }
      out.push(node);
    }
    features |= Features::VARIANTS;
    Action::Replace(out)
  });
  match error {
    Some(e) => Err(e),
    None => Ok(features),
  }
}

#[cfg(test)]
mod tests {
  use super::{Compounds, compounds_for_selectors};

  /// `variants.test.ts`: `compoundsForSelectors`.
  #[test]
  fn compounds() {
    let both = Compounds::STYLE_RULES | Compounds::AT_RULES;
    for (selectors, expected) in [
      (&[".foo"][..], Compounds::STYLE_RULES),
      (&["&:is(:hover)"][..], Compounds::STYLE_RULES),
      (&["@media foo"][..], Compounds::AT_RULES),
      (&["@supports foo"][..], Compounds::AT_RULES),
      (&["@container foo"][..], Compounds::AT_RULES),
      (&[".foo", "@media foo"][..], both),
      (&[".foo::before"][..], Compounds::NEVER),
      (&["@starting-style"][..], Compounds::NEVER),
      (&[".foo", "@media foo", ".foo::before"][..], Compounds::NEVER),
      (&[".foo", "@media foo", "@starting-style"][..], Compounds::NEVER),
    ] {
      assert_eq!(compounds_for_selectors(selectors), expected, "{selectors:?}");
    }
  }
}
