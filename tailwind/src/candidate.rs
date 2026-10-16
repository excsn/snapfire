//! Candidates: a class name split into its variants, its utility and that utility's value
//! and modifier.

use std::rc::Rc;

use crate::design_system::DesignSystem;
use crate::utils::decode::{decode_arbitrary_value, is_valid_arbitrary};
use crate::utils::segment::segment;
use crate::value_parser::{self, ValueNode};
use crate::variants::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum UtilityValue {
  /// `bg-[#0088cc]`, `bg-[color:var(--x)]`, `bg-(--x)`
  Arbitrary { data_type: Option<String>, value: String },
  /// `bg-red-500`, `w-1/2` with its fraction kept beside the value
  Named { value: String, fraction: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Modifier {
  Arbitrary(String),
  Named(String),
}

impl Modifier {
  pub fn value(&self) -> &str {
    match self {
      Modifier::Arbitrary(v) | Modifier::Named(v) => v,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum VariantValue {
  Arbitrary(String),
  Named(String),
}

impl VariantValue {
  pub fn value(&self) -> &str {
    match self {
      VariantValue::Arbitrary(v) | VariantValue::Named(v) => v,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Variant {
  Arbitrary {
    selector: String,
    relative: bool,
  },
  Static {
    root: String,
  },
  Functional {
    root: String,
    value: Option<VariantValue>,
    modifier: Option<Modifier>,
  },
  Compound {
    root: String,
    modifier: Option<Modifier>,
    variant: Rc<Variant>,
  },
}

impl Variant {
  pub fn root(&self) -> Option<&str> {
    match self {
      Variant::Arbitrary { .. } => None,
      Variant::Static { root } | Variant::Functional { root, .. } | Variant::Compound { root, .. } => Some(root),
    }
  }

  pub fn modifier(&self) -> Option<&Modifier> {
    match self {
      Variant::Functional { modifier, .. } | Variant::Compound { modifier, .. } => modifier.as_ref(),
      _ => None,
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CandidateKind {
  /// `[color:red]`
  Arbitrary {
    property: String,
    value: String,
    modifier: Option<Modifier>,
  },
  /// `underline`
  Static { root: String },
  /// `bg-red-500`
  Functional {
    root: String,
    value: Option<UtilityValue>,
    modifier: Option<Modifier>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Candidate {
  pub kind: CandidateKind,
  pub variants: Vec<Rc<Variant>>,
  pub important: bool,
  pub raw: String,
}

impl Candidate {
  pub fn root(&self) -> Option<&str> {
    match &self.kind {
      CandidateKind::Static { root } | CandidateKind::Functional { root, .. } => Some(root),
      CandidateKind::Arbitrary { .. } => None,
    }
  }

  pub fn modifier(&self) -> Option<&Modifier> {
    match &self.kind {
      CandidateKind::Arbitrary { modifier, .. } | CandidateKind::Functional { modifier, .. } => modifier.as_ref(),
      CandidateKind::Static { .. } => None,
    }
  }

  pub fn value(&self) -> Option<&UtilityValue> {
    match &self.kind {
      CandidateKind::Functional { value, .. } => value.as_ref(),
      _ => None,
    }
  }
}

fn is_valid_named_value(value: &str) -> bool {
  !value.is_empty()
    && value
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'%' | b'-'))
}

pub fn parse_candidate(input: &str, ds: &DesignSystem) -> Vec<Candidate> {
  let mut out = Vec::new();
  let mut raw_variants = segment(input, b':');

  if let Some(prefix) = &ds.theme.prefix {
    if raw_variants.len() == 1 {
      return out;
    }
    if raw_variants[0] != prefix {
      return out;
    }
    raw_variants.remove(0);
  }

  let mut base = raw_variants.pop().expect("at least one segment");

  let mut variants = Vec::new();
  for raw in raw_variants.iter().rev() {
    match ds.parse_variant(raw) {
      Some(v) => variants.push(v),
      None => return out,
    }
  }

  let mut important = false;
  if base.ends_with('!') {
    important = true;
    base = &base[..base.len() - 1];
  } else if let Some(rest) = base.strip_prefix('!') {
    important = true;
    base = rest;
  }

  if ds.utilities.has(base, crate::utilities::Kind::Static) && !base.contains('[') {
    out.push(Candidate {
      kind: CandidateKind::Static { root: base.to_owned() },
      variants: variants.clone(),
      important,
      raw: input.to_owned(),
    });
  }

  let parts = segment(base, b'/');
  if parts.len() > 2 && !parts[2].is_empty() {
    return out;
  }
  let mut without_modifier = parts[0];
  let modifier_segment = parts.get(1).copied();

  let parsed_modifier = modifier_segment.and_then(parse_modifier);
  if modifier_segment.is_some() && parsed_modifier.is_none() {
    return out;
  }

  if without_modifier.starts_with('[') {
    if !without_modifier.ends_with(']') {
      return out;
    }
    let second = without_modifier.as_bytes().get(1).copied().unwrap_or(0);
    if second != b'-' && !second.is_ascii_lowercase() {
      return out;
    }
    without_modifier = &without_modifier[1..without_modifier.len() - 1];

    let Some(idx) = without_modifier.find(':') else {
      return out;
    };
    if idx == 0 || idx == without_modifier.len() - 1 {
      return out;
    }
    let property = &without_modifier[..idx];
    let value = decode_arbitrary_value(&without_modifier[idx + 1..]);
    if !is_valid_arbitrary(&value) {
      return out;
    }
    out.push(Candidate {
      kind: CandidateKind::Arbitrary {
        property: property.to_owned(),
        value,
        modifier: parsed_modifier,
      },
      variants,
      important,
      raw: input.to_owned(),
    });
    return out;
  }

  let roots: Vec<(String, Option<String>)> = if without_modifier.ends_with(']') {
    let Some(idx) = without_modifier.find("-[") else {
      return out;
    };
    let root = &without_modifier[..idx];
    if !ds.utilities.has(root, crate::utilities::Kind::Functional) {
      return out;
    }
    vec![(root.to_owned(), Some(without_modifier[idx + 1..].to_owned()))]
  } else if without_modifier.ends_with(')') {
    let Some(idx) = without_modifier.find("-(") else {
      return out;
    };
    let root = &without_modifier[..idx];
    if !ds.utilities.has(root, crate::utilities::Kind::Functional) {
      return out;
    }
    let inner = &without_modifier[(idx + 2).min(without_modifier.len() - 1)..without_modifier.len() - 1];
    let parts = segment(inner, b':');
    let (data_type, value) = if parts.len() == 2 {
      (Some(parts[0]), parts[1])
    } else {
      (None, inner)
    };
    if !value.starts_with("--") {
      return out;
    }
    if !is_valid_arbitrary(value) {
      return out;
    }
    let value = match data_type {
      None => format!("[var({value})]"),
      Some(t) => format!("[{t}:var({value})]"),
    };
    vec![(root.to_owned(), Some(value))]
  } else {
    find_roots(without_modifier, |root| {
      ds.utilities.has(root, crate::utilities::Kind::Functional)
    })
  };

  for (root, value) in roots {
    let Some(value) = value else {
      out.push(Candidate {
        kind: CandidateKind::Functional {
          root,
          value: None,
          modifier: parsed_modifier.clone(),
        },
        variants: variants.clone(),
        important,
        raw: input.to_owned(),
      });
      continue;
    };

    let utility_value = if let Some(start) = value.find('[') {
      if !value.ends_with(']') {
        return out;
      }
      let mut arbitrary = decode_arbitrary_value(&value[start + 1..value.len() - 1]);
      if !is_valid_arbitrary(&arbitrary) {
        continue;
      }

      let mut typehint: Option<String> = None;
      for (i, b) in arbitrary.bytes().enumerate() {
        if b == b':' {
          typehint = Some(arbitrary[..i].to_owned());
          arbitrary = arbitrary[i + 1..].to_owned();
          break;
        }
        if b == b'-' || b.is_ascii_lowercase() {
          continue;
        }
        break;
      }

      if arbitrary.is_empty() || arbitrary.trim().is_empty() {
        continue;
      }
      if typehint.as_deref() == Some("") {
        continue;
      }
      UtilityValue::Arbitrary {
        data_type: typehint,
        value: arbitrary,
      }
    } else {
      let fraction = match (modifier_segment, &parsed_modifier) {
        (None, _) | (_, Some(Modifier::Arbitrary(_))) => None,
        (Some(segment), _) => Some(format!("{value}/{segment}")),
      };
      if !is_valid_named_value(&value) {
        continue;
      }
      UtilityValue::Named { value, fraction }
    };

    out.push(Candidate {
      kind: CandidateKind::Functional {
        root,
        value: Some(utility_value),
        modifier: parsed_modifier.clone(),
      },
      variants: variants.clone(),
      important,
      raw: input.to_owned(),
    });
  }

  out
}

fn parse_modifier(modifier: &str) -> Option<Modifier> {
  if modifier.starts_with('[') && modifier.ends_with(']') && modifier.len() >= 2 {
    let value = decode_arbitrary_value(&modifier[1..modifier.len() - 1]);
    if !is_valid_arbitrary(&value) {
      return None;
    }
    if value.is_empty() || value.trim().is_empty() {
      return None;
    }
    return Some(Modifier::Arbitrary(value));
  }

  if modifier.starts_with('(') && modifier.ends_with(')') && modifier.len() >= 2 {
    let inner = &modifier[1..modifier.len() - 1];
    if !inner.starts_with("--") {
      return None;
    }
    if !is_valid_arbitrary(inner) {
      return None;
    }
    return Some(Modifier::Arbitrary(decode_arbitrary_value(&format!("var({inner})"))));
  }

  if !is_valid_named_value(modifier) {
    return None;
  }
  Some(Modifier::Named(modifier.to_owned()))
}

pub fn parse_variant(variant: &str, ds: &DesignSystem) -> Option<Variant> {
  if variant.starts_with('[') && variant.ends_with(']') && variant.len() >= 2 {
    if variant.as_bytes().get(1) == Some(&b'@') && variant.contains('&') {
      return None;
    }
    let mut selector = decode_arbitrary_value(&variant[1..variant.len() - 1]);
    if !is_valid_arbitrary(&selector) {
      return None;
    }
    if selector.is_empty() || selector.trim().is_empty() {
      return None;
    }
    let relative = matches!(selector.as_bytes()[0], b'>' | b'+' | b'~');
    if !relative && !selector.starts_with('@') && !selector.contains('&') {
      selector = format!("&:is({selector})");
    }
    return Some(Variant::Arbitrary { selector, relative });
  }

  let parts = segment(variant, b'/');
  if parts.len() > 2 && !parts[2].is_empty() {
    return None;
  }
  let without_modifier = parts[0];
  let mut modifier = parts.get(1).map(|s| (*s).to_owned());

  for (root, value) in find_roots(without_modifier, |root| ds.variants.has(root)) {
    match ds.variants.kind(&root) {
      Some(VariantKind::Static) => {
        if value.is_some() || modifier.is_some() {
          return None;
        }
        return Some(Variant::Static { root });
      }
      Some(VariantKind::Functional) => {
        let parsed_modifier = modifier.as_deref().and_then(parse_modifier);
        if modifier.is_some() && parsed_modifier.is_none() {
          return None;
        }

        let Some(value) = value else {
          return Some(Variant::Functional {
            root,
            value: None,
            modifier: parsed_modifier,
          });
        };

        if value.ends_with(']') {
          if !value.starts_with('[') {
            continue;
          }
          let arbitrary = decode_arbitrary_value(&value[1..value.len() - 1]);
          if !is_valid_arbitrary(&arbitrary) {
            return None;
          }
          if arbitrary.is_empty() || arbitrary.trim().is_empty() {
            return None;
          }
          return Some(Variant::Functional {
            root,
            value: Some(VariantValue::Arbitrary(arbitrary)),
            modifier: parsed_modifier,
          });
        }

        if value.ends_with(')') {
          if !value.starts_with('(') {
            continue;
          }
          let arbitrary = decode_arbitrary_value(&value[1..value.len() - 1]);
          if !is_valid_arbitrary(&arbitrary) {
            return None;
          }
          if arbitrary.is_empty() || arbitrary.trim().is_empty() {
            return None;
          }
          if !arbitrary.starts_with("--") {
            return None;
          }
          return Some(Variant::Functional {
            root,
            value: Some(VariantValue::Arbitrary(format!("var({arbitrary})"))),
            modifier: parsed_modifier,
          });
        }

        if !is_valid_named_value(&value) {
          continue;
        }
        return Some(Variant::Functional {
          root,
          value: Some(VariantValue::Named(value)),
          modifier: parsed_modifier,
        });
      }
      Some(VariantKind::Compound) => {
        let mut value = value?;
        if let Some(m) = &modifier
          && (root == "not" || root == "has" || root == "in")
        {
          value = format!("{value}/{m}");
          modifier = None;
        }
        let sub = ds.parse_variant(&value)?;
        if !ds.variants.compounds_with(&root, &sub) {
          return None;
        }
        let parsed_modifier = modifier.as_deref().and_then(parse_modifier);
        if modifier.is_some() && parsed_modifier.is_none() {
          return None;
        }
        return Some(Variant::Compound {
          root,
          modifier: parsed_modifier,
          variant: sub,
        });
      }
      None => {}
    }
  }

  None
}

fn find_roots(input: &str, exists: impl Fn(&str) -> bool) -> Vec<(String, Option<String>)> {
  let mut roots = Vec::new();
  if exists(input) {
    roots.push((input.to_owned(), None));
  }

  let mut idx = input.rfind('-');
  while let Some(i) = idx {
    if i == 0 {
      break;
    }
    let maybe_root = &input[..i];
    if exists(maybe_root) {
      let value = &input[i + 1..];
      if value.is_empty() {
        break;
      }
      if maybe_root == "@" && exists("@") && input.as_bytes()[i] == b'-' {
        break;
      }
      roots.push((maybe_root.to_owned(), Some(value.to_owned())));
    }
    idx = input[..i].rfind('-');
  }

  if input.starts_with('@') && exists("@") {
    roots.push(("@".to_owned(), Some(input[1..].to_owned())));
  }

  roots
}

const SYMBOLS: &[&str] = &["~", ">", "+", "-", "*", "/"];

fn is_symbol(node: &ValueNode) -> bool {
  SYMBOLS.contains(&node.value())
}

/// Tailwind's `printArbitraryValue`: the inside of `[…]` written back as a class name, with
/// spaces around operators dropped and spaces as underscores.
fn print_arbitrary_value(input: &str) -> String {
  fn level(nodes: &mut Vec<ValueNode>) {
    let mut drop = vec![false; nodes.len()];
    let mut i = 0;
    while i < nodes.len() {
      match &nodes[i] {
        ValueNode::Word(w) if SYMBOLS.contains(&w.as_str()) => {
          let previous_space = i >= 1 && matches!(&nodes[i - 1], ValueNode::Separator(s) if s == " ");
          let next_space = matches!(nodes.get(i + 1), Some(ValueNode::Separator(s)) if s == " ");
          let previous_symbol = i >= 2 && is_symbol(&nodes[i - 2]);
          let next_symbol = nodes.get(i + 2).is_some_and(is_symbol);
          if previous_space && next_space && !previous_symbol && !next_symbol {
            drop[i - 1] = true;
            drop[i + 1] = true;
          }
        }
        ValueNode::Separator(s) if !s.is_empty() && s.trim().is_empty() => {
          if i == 0 || i == nodes.len() - 1 {
            drop[i] = true;
          }
        }
        ValueNode::Separator(s) if s.trim() == "," => nodes[i] = ValueNode::Separator(",".to_owned()),
        ValueNode::Function { value, .. } if value.starts_with("--") => {
          let wrap = i > 0
            && !matches!(&nodes[i - 1], ValueNode::Separator(s) if s == ",")
            && !(i >= 2 && !is_symbol(&nodes[i - 2]));
          if wrap {
            let inner = nodes[i].clone();
            nodes[i] = ValueNode::Function {
              value: String::new(),
              nodes: vec![inner],
            };
            i += 1;
            continue;
          }
        }
        _ => {}
      }
      if let ValueNode::Function { nodes: children, .. } = &mut nodes[i] {
        level(children);
      }
      i += 1;
    }
    let mut index = 0;
    nodes.retain(|_| {
      let keep = !drop[index];
      index += 1;
      keep
    });
  }

  fn escape_underscore(value: &str) -> String {
    value.replace('_', "\\_").replace(' ', "_")
  }

  fn escape_all(nodes: &mut [ValueNode]) {
    for node in nodes {
      match node {
        ValueNode::Function { value, nodes } => {
          let is_url = value == "url" || value.ends_with("_url");
          *value = escape_underscore(value);
          if !is_url {
            escape_all(nodes);
          }
        }
        ValueNode::Separator(v) => *v = escape_underscore(v),
        ValueNode::Word(w) => {
          if !w.starts_with("--") {
            *w = escape_underscore(w);
          }
        }
      }
    }
  }

  let mut ast = value_parser::parse(input);
  level(&mut ast);
  escape_all(&mut ast);
  value_parser::to_css(&ast)
}

fn simplify_arbitrary_variant(input: &str) -> String {
  let ast = value_parser::parse(input);
  if let [
    ValueNode::Word(amp),
    ValueNode::Separator(colon),
    ValueNode::Function { value, nodes },
  ] = ast.as_slice()
    && amp == "&"
    && colon == ":"
    && value == "is"
  {
    return value_parser::to_css(nodes);
  }
  input.to_owned()
}

fn is_var(value: &str) -> bool {
  let ast = value_parser::parse(value);
  matches!(ast.as_slice(), [ValueNode::Function { value, .. }] if value == "var")
}

fn bracketed(value: &str) -> (String, char, char) {
  if is_var(value) {
    (value[4..value.len() - 1].to_owned(), '(', ')')
  } else {
    (value.to_owned(), '[', ']')
  }
}

pub fn print_modifier(modifier: Option<&Modifier>) -> String {
  match modifier {
    None => String::new(),
    Some(Modifier::Arbitrary(value)) => {
      let (inner, open, close) = bracketed(value);
      format!("/{open}{}{close}", print_arbitrary_value(&inner))
    }
    Some(Modifier::Named(value)) => format!("/{value}"),
  }
}

pub fn print_variant(variant: &Variant) -> String {
  match variant {
    Variant::Static { root } => root.clone(),
    Variant::Arbitrary { selector, .. } => {
      format!("[{}]", print_arbitrary_value(&simplify_arbitrary_variant(selector)))
    }
    Variant::Functional { root, value, modifier } => {
      let mut base = root.clone();
      let dash = if root != "@" { "-" } else { "" };
      match value {
        Some(VariantValue::Arbitrary(v)) => {
          let (inner, open, close) = bracketed(v);
          base.push_str(&format!("{dash}{open}{}{close}", print_arbitrary_value(&inner)));
        }
        Some(VariantValue::Named(v)) => base.push_str(&format!("{dash}{v}")),
        None => {}
      }
      base.push_str(&print_modifier(modifier.as_ref()));
      base
    }
    Variant::Compound {
      root,
      modifier,
      variant,
    } => format!("{root}-{}{}", print_variant(variant), print_modifier(modifier.as_ref())),
  }
}

/// A candidate written back as a class name, in Tailwind's canonical spelling.
pub fn print_candidate(ds: &DesignSystem, candidate: &Candidate) -> String {
  let mut parts: Vec<String> = candidate.variants.iter().rev().map(|v| print_variant(v)).collect();
  if let Some(prefix) = &ds.theme.prefix {
    parts.insert(0, prefix.clone());
  }

  let mut base = String::new();
  match &candidate.kind {
    CandidateKind::Static { root } => base.push_str(root),
    CandidateKind::Functional { root, value, modifier } => {
      base.push_str(root);
      match value {
        Some(UtilityValue::Arbitrary { data_type, value }) => {
          let (inner, open, close) = bracketed(value);
          match data_type {
            Some(t) => base.push_str(&format!("-{open}{t}:{}{close}", print_arbitrary_value(&inner))),
            None => base.push_str(&format!("-{open}{}{close}", print_arbitrary_value(&inner))),
          }
        }
        Some(UtilityValue::Named { value, .. }) => base.push_str(&format!("-{value}")),
        None => {}
      }
      base.push_str(&print_modifier(modifier.as_ref()));
    }
    CandidateKind::Arbitrary {
      property,
      value,
      modifier,
    } => {
      base.push_str(&format!("[{property}:{}]", print_arbitrary_value(value)));
      base.push_str(&print_modifier(modifier.as_ref()));
    }
  }
  if candidate.important {
    base.push('!');
  }
  parts.push(base);
  parts.join(":")
}

#[cfg(test)]
mod tests {
  use serde_json::{Value, json};

  use super::*;
  use crate::theme::Theme;
  use crate::utilities::{Compiled, Utilities};
  use crate::variants::{Compounds, Variants};

  fn modifier(m: &Option<Modifier>) -> Value {
    match m {
      None => Value::Null,
      Some(Modifier::Arbitrary(v)) => json!({ "kind": "arbitrary", "value": v }),
      Some(Modifier::Named(v)) => json!({ "kind": "named", "value": v }),
    }
  }

  fn variant(v: &Variant) -> Value {
    match v {
      Variant::Arbitrary { selector, relative } => {
        json!({ "kind": "arbitrary", "relative": relative, "selector": selector })
      }
      Variant::Static { root } => json!({ "kind": "static", "root": root }),
      Variant::Functional {
        root,
        value,
        modifier: m,
      } => json!({
        "kind": "functional",
        "modifier": modifier(m),
        "root": root,
        "value": match value {
          None => Value::Null,
          Some(VariantValue::Arbitrary(v)) => json!({ "kind": "arbitrary", "value": v }),
          Some(VariantValue::Named(v)) => json!({ "kind": "named", "value": v }),
        },
      }),
      Variant::Compound {
        root,
        modifier: m,
        variant: sub,
      } => json!({ "kind": "compound", "modifier": modifier(m), "root": root, "variant": variant(sub) }),
    }
  }

  fn candidate(c: &Candidate) -> Value {
    let variants: Vec<Value> = c.variants.iter().map(|v| variant(v)).collect();
    match &c.kind {
      CandidateKind::Arbitrary {
        property,
        value,
        modifier: m,
      } => json!({
        "important": c.important, "kind": "arbitrary", "modifier": modifier(m), "property": property, "raw": c.raw, "value": value, "variants": variants,
      }),
      CandidateKind::Static { root } => {
        json!({ "important": c.important, "kind": "static", "raw": c.raw, "root": root, "variants": variants })
      }
      CandidateKind::Functional {
        root,
        value,
        modifier: m,
      } => json!({
        "important": c.important,
        "kind": "functional",
        "modifier": modifier(m),
        "raw": c.raw,
        "root": root,
        "value": match value {
          None => Value::Null,
          Some(UtilityValue::Arbitrary { data_type, value }) => json!({ "dataType": data_type, "kind": "arbitrary", "value": value }),
          Some(UtilityValue::Named { value, fraction }) => json!({ "fraction": fraction, "kind": "named", "value": value }),
        },
        "variants": variants,
      }),
    }
  }

  /// A value as vitest's pretty-format prints it: keys sorted, strings unescaped, trailing commas.
  fn pretty_format(value: &Value, depth: usize) -> String {
    let indent = "  ".repeat(depth + 1);
    let close = "  ".repeat(depth);
    match value {
      Value::Null => "null".to_owned(),
      Value::Bool(b) => b.to_string(),
      Value::Number(n) => n.to_string(),
      Value::String(s) => format!("\"{s}\""),
      Value::Array(items) if items.is_empty() => "[]".to_owned(),
      Value::Array(items) => {
        let body: String = items
          .iter()
          .map(|v| format!("{indent}{},\n", pretty_format(v, depth + 1)))
          .collect();
        format!("[\n{body}{close}]")
      }
      Value::Object(map) if map.is_empty() => "{}".to_owned(),
      Value::Object(map) => {
        let body: String = map
          .iter()
          .map(|(k, v)| format!("{indent}\"{k}\": {},\n", pretty_format(v, depth + 1)))
          .collect();
        format!("{{\n{body}{close}}}")
      }
    }
  }

  #[derive(serde::Deserialize)]
  struct ParseFixture {
    test: String,
    candidate: String,
    utilities: Vec<(String, String)>,
    variants: Vec<(String, String, String)>,
    prefix: Option<String>,
    expected: String,
  }

  fn compounds(text: &str) -> Compounds {
    let mut out = Compounds::NEVER;
    for part in text.split('|') {
      out = out
        | match part.trim() {
          "Compounds.StyleRules" => Compounds::STYLE_RULES,
          "Compounds.AtRules" => Compounds::AT_RULES,
          _ => Compounds::NEVER,
        };
    }
    out
  }

  /// `candidate.test.ts`: each parse against the registrations its test makes.
  #[test]
  fn tailwind_candidate_parses() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/candidate-parse.json");
    let fixtures: Vec<ParseFixture> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut failures = Vec::new();
    for f in &fixtures {
      let mut ds = DesignSystem::new(Theme::default());
      ds.theme.prefix = f.prefix.clone();
      ds.utilities = Utilities::default();
      for (kind, name) in &f.utilities {
        if kind == "static" {
          ds.utilities.static_utility(name, |_, _| Compiled::Nodes(Vec::new()));
        } else {
          ds.utilities.functional(name, |_, _| Compiled::Nodes(Vec::new()));
        }
      }
      ds.variants = Variants::default();
      for (kind, name, with) in &f.variants {
        let noop: crate::variants::ApplyFn = std::rc::Rc::new(|_, _, _| Some(()));
        match kind.as_str() {
          "static" => ds.variants.static_variant(name, noop, None, None),
          "functional" => ds.variants.functional(name, noop, None),
          _ => ds.variants.compound(name, compounds(with), noop),
        }
      }
      let parsed = Value::Array(parse_candidate(&f.candidate, &ds).iter().map(candidate).collect());
      let printed = pretty_format(&parsed, 0);
      if printed != f.expected {
        failures.push(format!(
          "{} ({}):\n--- got\n{printed}\n--- expected\n{}",
          f.test, f.candidate, f.expected
        ));
      }
    }
    assert!(
      failures.is_empty(),
      "{} of {} differ:\n{}",
      failures.len(),
      fixtures.len(),
      failures.join("\n")
    );
  }
}
