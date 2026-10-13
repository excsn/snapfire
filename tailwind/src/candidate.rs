//! Candidates: a class name split into its variants, its utility and that utility's value
//! and modifier.

use std::rc::Rc;

use crate::design_system::DesignSystem;
use crate::utils::decode::{decode_arbitrary_value, is_valid_arbitrary};
use crate::utils::segment::segment;
use crate::variants::VariantKind;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UtilityValue {
  /// `bg-[#0088cc]`, `bg-[color:var(--x)]`, `bg-(--x)`
  Arbitrary { data_type: Option<String>, value: String },
  /// `bg-red-500`, `w-1/2` with its fraction kept beside the value
  Named { value: String, fraction: Option<String> },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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
