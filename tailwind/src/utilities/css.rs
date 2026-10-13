//! `@utility`: utilities a stylesheet defines, static or functional with `--value()` and
//! `--modifier()`.

use std::sync::LazyLock;

use regex::Regex;

use crate::ast::Node;
use crate::candidate::{Modifier, UtilityValue};
use crate::design_system::DesignSystem;
use crate::theme::ThemeOptions;
use crate::utilities::Compiled;
use crate::utils::escape::unescape;
use crate::utils::infer::{DataType, infer_data_type, is_positive_integer, is_valid_spacing_multiplier};
use crate::utils::js::{is_integer, number_to_string, to_number};
use crate::utils::segment::segment;
use crate::value_parser::{self, ValueNode};
use crate::walk::{Action, walk};

const BARE_VALUE_DATA_TYPES: &[&str] = &["number", "integer", "ratio", "percentage"];

static UTILITY_ROOT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?[a-z][a-zA-Z0-9_-]*").unwrap());

pub fn is_valid_static_utility_name(name: &str) -> bool {
  let Some(m) = UTILITY_ROOT.find(name) else { return false };
  let root = m.as_str();
  let value = &name[root.len()..];
  if value.is_empty() {
    return !root.ends_with('-');
  }

  let vb = value.as_bytes();
  let root_last = root.as_bytes().last().copied().unwrap_or(0);
  let mut seen_slash = false;
  for i in 0..vb.len() {
    let c = vb[i];
    match c {
      b'%' => {
        if i != vb.len() - 1 {
          return false;
        }
        let prev = if i > 0 { vb[i - 1] } else { root_last };
        if !prev.is_ascii_digit() {
          return false;
        }
      }
      b'/' => {
        if i == vb.len() - 1 {
          return false;
        }
        if seen_slash {
          return false;
        }
        seen_slash = true;
      }
      b'.' => {
        let prev = if i > 0 { vb[i - 1] } else { root_last };
        if !prev.is_ascii_digit() {
          return false;
        }
        let next = vb.get(i + 1).copied().unwrap_or(0);
        if !next.is_ascii_digit() {
          return false;
        }
      }
      b'_' | b'-' => continue,
      c if c.is_ascii_alphanumeric() => continue,
      _ => return false,
    }
  }
  true
}

pub fn is_valid_functional_utility_name(name: &str) -> bool {
  let Some(name) = name.strip_suffix("-*") else {
    return false;
  };
  let Some(m) = UTILITY_ROOT.find(name) else { return false };
  m.as_str().len() == name.len()
}

static ESCAPED_STAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\\*").unwrap());
static TWO_KEYS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--(.*?)\s--(.*?)").unwrap());
static WHITESPACE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static REPEATED_STAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(-\*){2,}").unwrap());

fn normalize_arg(arg: &str) -> String {
  let arg = ESCAPED_STAR.replace_all(arg, "*");
  let arg = TWO_KEYS.replace_all(&arg, "--$1-*--$2");
  let arg = WHITESPACE.replace_all(&arg, "");
  let mut arg = REPEATED_STAR.replace_all(&arg, "-*").into_owned();
  if arg.starts_with("--") && !arg.contains('(') && !arg.contains("-*") {
    arg.push_str("-*");
  }
  arg
}

/// Rewrites the `--value(…)` and `--modifier(…)` arguments in a body into one spelling and
/// warns about a bare value type outside `number`, `integer`, `ratio` and `percentage`.
fn normalize_body(body: &mut Vec<Node>, warnings: &mut Vec<String>) {
  walk(body, &mut |child, _| {
    let Node::Declaration { value: Some(value), .. } = child else {
      return Action::Continue;
    };
    if !value.contains("--value(") && !value.contains("--modifier(") {
      return Action::Continue;
    }
    let mut ast = value_parser::parse(value);
    value_parser::walk(&mut ast, &mut |f, _| {
      let ValueNode::Function { value: name, nodes } = f else {
        return Action::Continue;
      };
      if name == "--spacing" {
        return Action::Continue;
      }
      if name != "--value" && name != "--modifier" {
        return Action::Continue;
      }
      let inner = value_parser::to_css(nodes);
      let args: Vec<String> = segment(&inner, b',').into_iter().map(normalize_arg).collect();
      *nodes = value_parser::parse(&args.join(","));
      let function = ValueNode::Function {
        value: name.clone(),
        nodes: nodes.clone(),
      };
      for node in nodes.iter() {
        let ValueNode::Word(word) = node else { continue };
        let quoted = (word.starts_with('"') || word.starts_with('\'')) && word.len() >= 2 && word.ends_with(&word[..1]);
        if quoted
          || word.starts_with("--")
          || (word.starts_with('[') && word.ends_with(']'))
          || BARE_VALUE_DATA_TYPES.contains(&word.as_str())
        {
          continue;
        }
        warnings.push(format!(
          "Unsupported bare value data type: \"{word}\".\nOnly valid data types are: {}.\n",
          BARE_VALUE_DATA_TYPES
            .iter()
            .map(|t| format!("\"{t}\""))
            .collect::<Vec<_>>()
            .join(", ")
        ));
        let mut copy = vec![function.clone()];
        value_parser::walk(&mut copy, &mut |n, _| match n {
          ValueNode::Word(w) if w == word => Action::ReplaceSkip(vec![ValueNode::word("\u{b6}")]),
          _ => Action::Continue,
        });
        let marked = value_parser::to_css(&copy);
        let offset = marked.find('\u{b6}').map(|i| marked[..i].chars().count()).unwrap_or(0);
        warnings.push(format!(
          "```css\n{}\n{}{}\n```",
          value_parser::to_css(std::slice::from_ref(&function)),
          " ".repeat(offset),
          "^".repeat(word.chars().count())
        ));
      }
      Action::Continue
    });
    *value = value_parser::to_css(&ast);
    Action::Continue
  });
}

pub fn create_css_utility(ds: &mut DesignSystem, raw_name: &str, index: usize) {
  let name = unescape(raw_name);

  if is_valid_functional_utility_name(&name) {
    let mut warnings = Vec::new();
    normalize_body(&mut ds.css_utilities.borrow_mut()[index], &mut warnings);
    ds.warnings.borrow_mut().extend(warnings);
    let root = name[..name.len() - 2].to_owned();
    ds.utilities.functional(&root, move |candidate, ds| {
      let mut body: Vec<Node> = ds.css_utilities.borrow()[index].clone();
      let value = candidate.value();
      let modifier = candidate.modifier();

      let mut used_value = false;
      let mut resolved_value = false;
      let mut used_modifier = false;
      let mut resolved_modifier = false;
      let mut resolved_ratio = false;

      resolve_body(
        &mut body,
        value,
        modifier,
        ds,
        &mut used_value,
        &mut resolved_value,
        &mut used_modifier,
        &mut resolved_modifier,
        &mut resolved_ratio,
      );

      if !used_value || !resolved_value {
        return Compiled::Invalid;
      }
      if used_modifier && !resolved_modifier && modifier.is_some() {
        return Compiled::Invalid;
      }
      if resolved_ratio && resolved_modifier {
        return Compiled::Invalid;
      }
      if modifier.is_some() && !resolved_ratio && !resolved_modifier {
        return Compiled::Invalid;
      }
      if resolved_ratio {
        remove_marked(&mut body);
      } else {
        unmark(&mut body);
      }
      Compiled::Nodes(body)
    });
    return;
  }

  if is_valid_static_utility_name(&name) {
    ds.utilities.static_utility(&name, move |_, ds| {
      Compiled::Nodes(ds.css_utilities.borrow()[index].clone())
    });
  }
}

const RESOLVED_MARK: &str = "\u{0}tw-value";

#[allow(clippy::too_many_arguments)]
fn resolve_body(
  body: &mut Vec<Node>,
  value: Option<&UtilityValue>,
  modifier: Option<&Modifier>,
  ds: &DesignSystem,
  used_value: &mut bool,
  resolved_value: &mut bool,
  used_modifier: &mut bool,
  resolved_modifier: &mut bool,
  resolved_ratio: &mut bool,
) {
  let mut root = vec![Node::AtRule {
    name: "@utility".to_owned(),
    params: String::new(),
    nodes: std::mem::take(body),
  }];
  walk(&mut root, &mut |node, visit| {
    if visit.css_parent().is_none() {
      return Action::Continue;
    }
    let Node::Declaration {
      property,
      value: Some(declared),
      ..
    } = node
    else {
      return Action::Continue;
    };

    let mut remove = false;
    let mut mark = false;
    let mut ast = value_parser::parse(declared);
    value_parser::walk(&mut ast, &mut |f, _| {
      let ValueNode::Function { value: name, nodes } = f else {
        return Action::Continue;
      };
      if name == "--value" {
        *used_value = true;
        match resolve_value_function(Arg::Value(value), nodes, ds) {
          Some((resolved, ratio)) => {
            *resolved_value = true;
            if ratio {
              *resolved_ratio = true;
            } else {
              mark = true;
            }
            Action::ReplaceSkip(resolved)
          }
          None => {
            remove = true;
            Action::Stop
          }
        }
      } else if name == "--modifier" {
        *used_modifier = true;
        match resolve_value_function(Arg::Modifier(modifier), nodes, ds) {
          Some((resolved, _)) => {
            *resolved_modifier = true;
            Action::ReplaceSkip(resolved)
          }
          None => {
            remove = true;
            Action::Stop
          }
        }
      } else {
        Action::Continue
      }
    });

    if remove {
      return Action::ReplaceSkip(Vec::new());
    }
    *declared = value_parser::to_css(&ast);
    if mark {
      property.insert_str(0, RESOLVED_MARK);
    }
    Action::Continue
  });
  if let Some(Node::AtRule { nodes, .. }) = root.pop() {
    *body = nodes;
  }
}

fn remove_marked(body: &mut Vec<Node>) {
  walk(body, &mut |node, _| match node {
    Node::Declaration { property, .. } if property.starts_with(RESOLVED_MARK) => Action::ReplaceSkip(Vec::new()),
    _ => Action::Continue,
  });
}

fn unmark(body: &mut Vec<Node>) {
  walk(body, &mut |node, _| {
    if let Node::Declaration { property, .. } = node
      && let Some(rest) = property.strip_prefix(RESOLVED_MARK)
    {
      *property = rest.to_owned();
    }
    Action::Continue
  });
}

enum Arg<'a> {
  Value(Option<&'a UtilityValue>),
  Modifier(Option<&'a Modifier>),
}

fn resolve_value_function(arg: Arg, args: &[ValueNode], ds: &DesignSystem) -> Option<(Vec<ValueNode>, bool)> {
  let (named, arbitrary, data_type, fraction) = match &arg {
    Arg::Value(None) | Arg::Modifier(None) => {
      for a in args {
        if let ValueNode::Function { value, nodes } = a
          && value == "--default"
        {
          return Some((nodes.clone(), false));
        }
      }
      return None;
    }
    Arg::Value(Some(UtilityValue::Named { value, fraction })) => {
      (Some(value.as_str()), None, None, Some(fraction.as_deref()))
    }
    Arg::Value(Some(UtilityValue::Arbitrary { value, data_type })) => {
      (None, Some(value.as_str()), Some(data_type.as_deref()), None)
    }
    Arg::Modifier(Some(Modifier::Named(value))) => (Some(value.as_str()), None, None, None),
    Arg::Modifier(Some(Modifier::Arbitrary(value))) => (None, Some(value.as_str()), None, None),
  };

  for a in args {
    let ValueNode::Word(w) = a else { continue };

    if let Some(named) = named {
      let quoted = (w.starts_with('\'') || w.starts_with('"')) && w.len() >= 2 && w.ends_with(&w[..1]);
      if quoted && &w[1..w.len() - 1] == named {
        return Some((value_parser::parse(named), false));
      }
      if !quoted && w.starts_with("--") {
        if let Some(key) = w.strip_suffix("-*") {
          if let Some(resolved) = ds.theme.resolve(Some(named), &[key], ThemeOptions::NONE) {
            return Some((value_parser::parse(&resolved), false));
          }
        } else {
          let mut keys: Vec<&str> = w.split("-*").collect();
          if keys.len() <= 1 {
            continue;
          }
          let namespace = keys.remove(0);
          if let Some((_, extra)) = ds.theme.resolve_with(named, &[namespace], &keys)
            && let Some(last) = keys.last()
            && let Some((_, v)) = extra.iter().find(|(k, _)| k == last)
          {
            return Some((value_parser::parse(v), false));
          }
        }
        continue;
      }
      if quoted {
        continue;
      }
      if !BARE_VALUE_DATA_TYPES.contains(&w.as_str()) {
        continue;
      }
      let resolved = if w == "ratio" {
        match fraction {
          Some(f) => f,
          None => Some(named),
        }
      } else {
        Some(named)
      };
      let Some(resolved) = resolved else { continue };
      let Some(ty) = DataType::parse(w).and_then(|t| infer_data_type(resolved, &[t])) else {
        continue;
      };
      match ty {
        DataType::Ratio => {
          let parts = segment(resolved, b'/');
          let lhs = to_number(parts[0]);
          let rhs = to_number(parts.get(1).copied().unwrap_or(""));
          let integer = |n: f64| is_integer(n) && n >= 0.0 && is_positive_integer(&number_to_string(n));
          if !integer(lhs) || !integer(rhs) {
            continue;
          }
          return Some((
            value_parser::parse(&format!(
              "{} / {}",
              parts[0].trim(),
              parts.get(1).copied().unwrap_or("").trim()
            )),
            true,
          ));
        }
        DataType::Number if !is_valid_spacing_multiplier(resolved) => continue,
        DataType::Percentage if !is_positive_integer(&resolved[..resolved.len() - 1]) => continue,
        _ => {}
      }
      return Some((value_parser::parse(resolved), false));
    }

    if let Some(arbitrary) = arbitrary
      && w.starts_with('[')
      && w.ends_with(']')
    {
      let wanted = &w[1..w.len() - 1];
      if wanted == "*" {
        return Some((value_parser::parse(arbitrary), false));
      }
      if let Some(Some(hint)) = data_type {
        if hint != wanted {
          continue;
        }
        return Some((value_parser::parse(arbitrary), false));
      }
      if let Some(t) = DataType::parse(wanted)
        && infer_data_type(arbitrary, &[t]).is_some()
      {
        return Some((value_parser::parse(arbitrary), false));
      }
    }
  }
  None
}

#[cfg(test)]
mod tests {
  use super::{is_valid_functional_utility_name, is_valid_static_utility_name};

  /// `utilities.test.ts`: `custom utilities > valid static utility name`.
  #[test]
  fn static_names() {
    for (name, valid) in [
      ("foo", true),
      ("foo-123", true),
      ("foo-2.5", true),
      ("-foo", true),
      ("foo-bar", true),
      ("foo_bar", true),
      ("foo-50%", true),
      ("foo-1/2", true),
      ("foo-sm/8", true),
      ("foo-4/snug", true),
      ("foo_", true),
      ("foo/bar", true),
      ("Foo", false),
      ("-Foo", false),
      ("foo-", false),
      ("foo-1/", false),
      ("foo-p%", false),
      ("foo.bar", false),
      ("foo-1..5", false),
      ("foo..bar", false),
      ("foo/bar/baz", false),
    ] {
      assert_eq!(is_valid_static_utility_name(name), valid, "{name}");
    }
  }

  /// `utilities.test.ts`: `custom utilities > valid functional name`.
  #[test]
  fn functional_names() {
    for (name, valid) in [
      ("foo", false),
      ("foo-*", true),
      ("foo--*", true),
      ("-foo-*", true),
      ("foo-bar-*", true),
      ("foo_bar-*", true),
      ("Foo-*", false),
      ("-Foo-*", false),
      ("foo!-*", false),
      ("foo-[…]", false),
    ] {
      assert_eq!(is_valid_functional_utility_name(name), valid, "{name}");
    }
  }
}
