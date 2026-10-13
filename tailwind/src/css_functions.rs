//! `--alpha()`, `--spacing()`, `--theme()` and `theme()` in declaration values and in
//! the conditions of `@media`, `@container`, `@supports` and `@custom-media`.

use std::sync::LazyLock;

use regex::Regex;

use crate::Error;
use crate::ast::Node;
use crate::design_system::DesignSystem;
use crate::features::Features;
use crate::theme::ThemeOptions;
use crate::utilities::with_alpha;
use crate::utils::compare::dimension;
use crate::utils::segment::segment;
use crate::value_parser::{self, ValueNode};
use crate::walk::{Action, walk};

static INVOCATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"--alpha\(|--spacing\(|--theme\(|theme\(").unwrap());

fn is_css_function(name: &str) -> bool {
  matches!(name, "--alpha" | "--spacing" | "--theme" | "theme")
}

pub fn substitute_functions(ast: &mut Vec<Node>, ds: &DesignSystem) -> Result<Features, Error> {
  let mut features = Features::NONE;
  let mut error = None;
  walk(ast, &mut |node, _| {
    match node {
      Node::Declaration { value: Some(value), .. } if INVOCATION.is_match(value) => {
        features |= Features::THEME_FUNCTION;
        match substitute_functions_in_value(value, false, ds) {
          Ok(v) => *value = v,
          Err(e) => {
            error = Some(e);
            return Action::Stop;
          }
        }
        return Action::Continue;
      }
      Node::AtRule { name, params, .. }
        if matches!(name.as_str(), "@media" | "@custom-media" | "@container" | "@supports")
          && INVOCATION.is_match(params) =>
      {
        features |= Features::THEME_FUNCTION;
        match substitute_functions_in_value(params, true, ds) {
          Ok(v) => *params = v,
          Err(e) => {
            error = Some(e);
            return Action::Stop;
          }
        }
      }
      _ => {}
    }
    Action::Continue
  });
  match error {
    Some(e) => Err(e),
    None => Ok(features),
  }
}

/// Evaluates the theme functions in `value`. `in_at_rule` forces `--theme()` to inline,
/// since a condition cannot read a variable.
pub fn substitute_functions_in_value(value: &str, in_at_rule: bool, ds: &DesignSystem) -> Result<String, Error> {
  let mut ast = value_parser::parse(value);
  let mut error = None;
  value_parser::walk(&mut ast, &mut |node, _| {
    let ValueNode::Function { value: name, nodes } = node else {
      return Action::Continue;
    };
    if !is_css_function(name) {
      return Action::Continue;
    }
    let inner = value_parser::to_css(nodes);
    let args: Vec<String> = segment(inner.trim(), b',')
      .into_iter()
      .map(|a| a.trim().to_owned())
      .collect();
    let result = match name.as_str() {
      "--alpha" => alpha(&args),
      "--spacing" => spacing(ds, &args),
      "--theme" => theme(ds, in_at_rule, &args),
      _ => legacy_theme(ds, &args),
    };
    match result {
      Ok(r) => Action::Replace(value_parser::parse(&r)),
      Err(e) => {
        error = Some(e);
        Action::Stop
      }
    }
  });
  match error {
    Some(e) => Err(e),
    None => Ok(value_parser::to_css(&ast)),
  }
}

fn alpha(args: &[String]) -> Result<String, Error> {
  let value = args.first().map(String::as_str).unwrap_or("");
  let parts: Vec<String> = segment(value, b'/').into_iter().map(|v| v.trim().to_owned()).collect();
  let color = parts.first().cloned().unwrap_or_default();
  let alpha = parts.get(1).cloned().unwrap_or_default();
  let example_color = if color.is_empty() {
    "var(--my-color)".to_owned()
  } else {
    color.clone()
  };
  let example_alpha = if alpha.is_empty() {
    "50%".to_owned()
  } else {
    alpha.clone()
  };
  if color.is_empty() || alpha.is_empty() {
    return Err(Error::Message(format!(
      "The --alpha(…) function requires a color and an alpha value, e.g.: `--alpha({example_color} / {example_alpha})`"
    )));
  }
  if args.len() > 1 {
    return Err(Error::Message(format!(
      "The --alpha(…) function only accepts one argument, e.g.: `--alpha({example_color} / {example_alpha})`"
    )));
  }
  Ok(with_alpha(&color, &alpha))
}

fn spacing(ds: &DesignSystem, args: &[String]) -> Result<String, Error> {
  let value = args.first().map(String::as_str).unwrap_or("");
  if value.is_empty() {
    return Err(Error::Message(
      "The --spacing(…) function requires an argument, but received none.".to_owned(),
    ));
  }
  if args.len() > 1 {
    return Err(Error::Message(format!(
      "The --spacing(…) function only accepts a single argument, but received {}.",
      args.len()
    )));
  }
  let Some(multiplier) = ds.theme.resolve(None, &["--spacing"], ThemeOptions::NONE) else {
    return Err(Error::Message(
      "The --spacing(…) function requires that the `--spacing` theme variable exists, but it was not found.".to_owned(),
    ));
  };
  if let Some((n, _)) = dimension(value) {
    if n == 0.0 {
      return Ok("0".to_owned());
    }
    if n == 1.0 {
      return Ok(multiplier);
    }
  }
  Ok(format!("calc({multiplier} * {value})"))
}

fn theme(ds: &DesignSystem, in_at_rule: bool, args: &[String]) -> Result<String, Error> {
  let mut path = args.first().cloned().unwrap_or_default();
  let fallback = &args[1.min(args.len())..];
  if !path.starts_with("--") {
    return Err(Error::Message(
      "The --theme(…) function can only be used with CSS variables from your theme.".to_owned(),
    ));
  }
  let mut inline = false;
  if path.ends_with(" inline") {
    inline = true;
    path.truncate(path.len() - 7);
  }
  if in_at_rule {
    inline = true;
  }

  let Some(resolved) = ds.resolve_theme_value(&path, inline).filter(|v| !v.is_empty()) else {
    if !fallback.is_empty() {
      return Ok(fallback.join(", "));
    }
    return Err(Error::Message(format!(
      "Could not resolve value for theme function: `theme({path})`. Consider checking if the variable name is correct or provide a fallback value to silence this error."
    )));
  };

  if fallback.is_empty() {
    return Ok(resolved);
  }
  let joined = fallback.join(", ");
  if joined == "initial" {
    return Ok(resolved);
  }
  if resolved == "initial" {
    return Ok(joined);
  }
  if resolved.starts_with("var(") || resolved.starts_with("theme(") || resolved.starts_with("--theme(") {
    let mut ast = value_parser::parse(&resolved);
    inject_fallback(&mut ast, &joined);
    return Ok(value_parser::to_css(&ast));
  }
  Ok(resolved)
}

fn legacy_theme(ds: &DesignSystem, args: &[String]) -> Result<String, Error> {
  let path = unquote(args.first().map(String::as_str).unwrap_or(""));
  let fallback = &args[1.min(args.len())..];
  let resolved = ds.resolve_theme_value(&path, true).filter(|v| !v.is_empty());
  match resolved {
    Some(v) => Ok(v),
    None if !fallback.is_empty() => Ok(fallback.join(", ")),
    None => Err(Error::Message(format!(
      "Could not resolve value for theme function: `theme({path})`. Consider checking if the path is correct or provide a fallback value to silence this error."
    ))),
  }
}

fn unquote(value: &str) -> String {
  let Some(quote) = value.chars().next().filter(|c| *c == '\'' || *c == '"') else {
    return value.to_owned();
  };
  let chars: Vec<char> = value.chars().collect();
  let mut out = String::new();
  let mut i = 1;
  while i + 1 < chars.len() {
    let c = chars[i];
    let next = chars[i + 1];
    if c == '\\' && (next == quote || next == '\\') {
      out.push(next);
      i += 2;
    } else {
      out.push(c);
      i += 1;
    }
  }
  out
}

fn inject_fallback(ast: &mut Vec<ValueNode>, fallback: &str) {
  value_parser::walk(ast, &mut |node, _| {
    let ValueNode::Function { value, nodes } = node else {
      return Action::Continue;
    };
    if value != "var" && value != "theme" && value != "--theme" {
      return Action::Continue;
    }
    if nodes.len() == 1 {
      nodes.push(ValueNode::Word(format!(", {fallback}")));
    } else if let Some(ValueNode::Word(last)) = nodes.last_mut()
      && last == "initial"
    {
      *last = fallback.to_owned();
    }
    Action::Continue
  });
}
