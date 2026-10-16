//! Utilities as plain declarations, for a renderer that matches no selectors and reads a small
//! CSS subset: the card images a host draws. Variables are replaced by their values, `calc()` of
//! numbers and one unit is worked out and colors are written in sRGB.

use std::collections::HashMap;

use crate::ast::{CtxValue, Node};
use crate::compile;
use crate::utils::escape::escape;
use crate::Compiler;

/// What one candidate means on a card: its declarations in order, with the card width they start
/// to apply at when the candidate carries a breakpoint variant.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct CardRule {
  pub candidate: String,
  /// In CSS pixels, a `rem` taken as 16 of them.
  pub min_width: Option<f32>,
  pub declarations: Vec<(String, String)>,
}

const REM: f32 = 16.0;

impl Compiler {
  /// The card declarations of each candidate that has some, in Tailwind's output order, so a
  /// later rule overrides an earlier one the way it would in a stylesheet. A candidate whose
  /// variants need a selector or a media feature other than the width, `hover:` or `dark:`
  /// among them, has none.
  pub fn card_rules<'c>(&self, candidates: impl IntoIterator<Item = &'c str>) -> Vec<CardRule> {
    let compiled = compile::compile_candidates(candidates, &self.ds, true, &mut |_| {});
    let mut initial: HashMap<String, String> = HashMap::new();
    for (node, _) in &compiled.nodes {
      property_initials(node, &mut initial);
    }
    let mut rules = Vec::new();
    for (node, raw) in &compiled.nodes {
      let mut rule = CardRule { candidate: raw.clone(), min_width: None, declarations: Vec::new() };
      let selector = format!(".{}", escape(raw));
      if !collect(node, &selector, true, &mut rule) || rule.declarations.is_empty() {
        continue;
      }
      let locals: HashMap<String, String> = rule.declarations.iter().filter(|(p, _)| p.starts_with("--")).cloned().collect();
      rule.declarations = rule
        .declarations
        .into_iter()
        .filter(|(p, _)| !p.starts_with("--"))
        .map(|(p, v)| {
          let value = self.resolve(&v, &locals, &initial);
          (p, card_value(&value))
        })
        .filter(|(_, v)| !v.is_empty())
        .flat_map(|(p, v)| physical(&p, &v))
        .collect();
      if !rule.declarations.is_empty() {
        rules.push(rule);
      }
    }
    rules
  }

  /// `value` with every `var()` replaced: the candidate's own custom properties first, then the
  /// theme, then a registered property's initial value, then the fallback.
  fn resolve(&self, value: &str, locals: &HashMap<String, String>, initial: &HashMap<String, String>) -> String {
    let mut out = value.to_owned();
    for _ in 0..16 {
      let Some(at) = out.find("var(") else { break };
      let Some(end) = matching(&out, at + 3) else { break };
      let inner = &out[at + 4..end];
      let (name, fallback) = match top_comma(inner) {
        Some(comma) => (inner[..comma].trim(), Some(inner[comma + 1..].trim())),
        None => (inner.trim(), None),
      };
      let found = locals.get(name).cloned().or_else(|| self.ds.theme.get(&[name])).or_else(|| initial.get(name).cloned()).or_else(|| fallback.map(str::to_owned)).unwrap_or_default();
      out = format!("{}{found}{}", &out[..at], &out[end + 1..]);
    }
    out
  }
}

/// A logical property as the physical ones it means on a left-to-right card, which knows no
/// writing direction: `padding-inline: 1rem 2rem` as `padding-left` and `padding-right`.
fn physical(property: &str, value: &str) -> Vec<(String, String)> {
  let sides = |start: &str, end: &str| -> Vec<(String, String)> {
    let parts: Vec<&str> = split_top(value, ' ').into_iter().filter(|p| !p.is_empty()).collect();
    let first = parts.first().copied().unwrap_or(value);
    let second = parts.get(1).copied().unwrap_or(first);
    vec![(start.to_owned(), first.to_owned()), (end.to_owned(), second.to_owned())]
  };
  let one = |to: &str| vec![(to.to_owned(), value.to_owned())];
  match property {
    "padding-inline" => sides("padding-left", "padding-right"),
    "padding-block" => sides("padding-top", "padding-bottom"),
    "margin-inline" => sides("margin-left", "margin-right"),
    "margin-block" => sides("margin-top", "margin-bottom"),
    "inset-inline" => sides("left", "right"),
    "inset-block" => sides("top", "bottom"),
    "padding-inline-start" => one("padding-left"),
    "padding-inline-end" => one("padding-right"),
    "padding-block-start" => one("padding-top"),
    "padding-block-end" => one("padding-bottom"),
    "margin-inline-start" => one("margin-left"),
    "margin-inline-end" => one("margin-right"),
    "margin-block-start" => one("margin-top"),
    "margin-block-end" => one("margin-bottom"),
    "inset-inline-start" => one("left"),
    "inset-inline-end" => one("right"),
    _ => vec![(property.to_owned(), value.to_owned())],
  }
}

/// Collects the declarations of `node` when everything around them is the candidate's own class
/// or a minimum-width media query. False when anything else wraps them.
fn collect(node: &Node, selector: &str, top: bool, rule: &mut CardRule) -> bool {
  match node {
    Node::Rule { selector: own, nodes } => {
      let accepted = if top { own == selector } else { own == "&" };
      accepted && nodes.iter().all(|child| collect(child, selector, false, rule))
    }
    Node::AtRule { name, params, nodes } if name == "@media" => match min_width(params) {
      Some(width) => {
        rule.min_width = Some(rule.min_width.map_or(width, |w| w.max(width)));
        nodes.iter().all(|child| collect(child, selector, false, rule))
      }
      None => false,
    },
    Node::AtRule { .. } => false,
    Node::Declaration { property, value: Some(value), .. } => {
      rule.declarations.push((property.clone(), value.clone()));
      true
    }
    Node::Declaration { .. } | Node::Comment(_) | Node::AtRoot { .. } => true,
    Node::Context { context, nodes } => {
      if context.get("reference").is_some_and(CtxValue::truthy) {
        return true;
      }
      nodes.iter().all(|child| collect(child, selector, top, rule))
    }
  }
}

/// The initial value of each `@property` a compiled rule registers.
fn property_initials(node: &Node, out: &mut HashMap<String, String>) {
  match node {
    Node::AtRule { name, params, nodes } if name == "@property" => {
      for child in nodes {
        if let Node::Declaration { property, value: Some(value), .. } = child
          && property == "initial-value"
        {
          out.insert(params.trim().to_owned(), value.clone());
        }
      }
    }
    other => {
      for child in other.children().into_iter().flatten() {
        property_initials(child, out);
      }
    }
  }
}

/// The width a `@media` condition starts at: `(width >= 40rem)` or `(min-width: 40rem)`.
fn min_width(params: &str) -> Option<f32> {
  let params = params.trim();
  let inner = params.strip_prefix('(')?.strip_suffix(')')?.trim();
  let length = inner.strip_prefix("width >=").or_else(|| inner.strip_prefix("min-width:"))?.trim();
  pixels(length)
}

fn pixels(length: &str) -> Option<f32> {
  if let Some(n) = length.strip_suffix("rem") {
    return n.trim().parse::<f32>().ok().map(|n| n * REM);
  }
  length.strip_suffix("px").and_then(|n| n.trim().parse::<f32>().ok())
}

/// The index of the `)` closing the `(` at `open`.
fn matching(text: &str, open: usize) -> Option<usize> {
  let mut depth = 0;
  for (i, c) in text[open..].char_indices() {
    match c {
      '(' => depth += 1,
      ')' => {
        depth -= 1;
        if depth == 0 {
          return Some(open + i);
        }
      }
      _ => {}
    }
  }
  None
}

fn top_comma(text: &str) -> Option<usize> {
  let mut depth = 0;
  for (i, c) in text.char_indices() {
    match c {
      '(' => depth += 1,
      ')' => depth -= 1,
      ',' if depth == 0 => return Some(i),
      _ => {}
    }
  }
  None
}

/// A resolved value in the card subset: `calc(infinity * 1px)` as a large length, `calc()` of
/// numbers and one unit worked out, `oklch()` and `color-mix(in oklab, … transparent)` in sRGB.
pub(crate) fn card_value(value: &str) -> String {
  let mut out = value.replace("calc(infinity * 1px)", "9999px");
  out = rewrite_calls(&out, "oklch(", |args| oklch(args).map(|(r, g, b, a)| srgb(r, g, b, a)));
  out = rewrite_calls(&out, "color-mix(", mix);
  out = rewrite_calls(&out, "calc(", calc);
  out
}

/// `text` with each call of `function` replaced by what `rewrite` makes of its arguments, the
/// call kept where `rewrite` makes nothing of it. Inner calls are rewritten first.
fn rewrite_calls(text: &str, function: &str, rewrite: impl Fn(&str) -> Option<String> + Copy) -> String {
  let mut out = String::with_capacity(text.len());
  let mut rest = text;
  while let Some(at) = rest.find(function) {
    let open = at + function.len() - 1;
    let Some(close) = matching(rest, open) else { break };
    let args = rewrite_calls(&rest[open + 1..close], function, rewrite);
    out.push_str(&rest[..at]);
    match rewrite(&args) {
      Some(replaced) => out.push_str(&replaced),
      None => {
        out.push_str(function);
        out.push_str(&args);
        out.push(')');
      }
    }
    rest = &rest[close + 1..];
  }
  out.push_str(rest);
  out
}

fn number(token: &str) -> Option<f32> {
  match token.strip_suffix('%') {
    Some(pct) => pct.parse::<f32>().ok().map(|n| n / 100.0),
    None => token.parse::<f32>().ok(),
  }
}

/// `L C H [/ A]` of an `oklch()` as gamma-encoded sRGB channels from 0 to 1 with the alpha.
fn oklch(args: &str) -> Option<(f32, f32, f32, f32)> {
  let (channels, alpha) = match args.split_once('/') {
    Some((c, a)) => (c, number(a.trim())?),
    None => (args, 1.0),
  };
  let parts: Vec<&str> = channels.split_whitespace().collect();
  let [l, c, h] = parts.as_slice() else { return None };
  let l = number(l)?;
  let c = c.parse::<f32>().ok()?;
  let h = h.trim_end_matches("deg").parse::<f32>().ok()?.to_radians();
  let (a, b) = (c * h.cos(), c * h.sin());
  let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
  let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
  let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;
  let (l3, m3, s3) = (l_ * l_ * l_, m_ * m_ * m_, s_ * s_ * s_);
  let r = 4.076_741_7 * l3 - 3.307_711_6 * m3 + 0.230_969_94 * s3;
  let g = -1.268_438 * l3 + 2.609_757_4 * m3 - 0.341_319_38 * s3;
  let bl = -0.004_196_086_3 * l3 - 0.703_418_6 * m3 + 1.707_614_7 * s3;
  let gamma = |x: f32| {
    let x = x.clamp(0.0, 1.0);
    if x <= 0.003_130_8 { 12.92 * x } else { 1.055 * x.powf(1.0 / 2.4) - 0.055 }
  };
  Some((gamma(r), gamma(g), gamma(bl), alpha))
}

fn srgb(r: f32, g: f32, b: f32, a: f32) -> String {
  let byte = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
  match a >= 1.0 {
    true => format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b)),
    false => format!("rgba({}, {}, {}, {})", byte(r), byte(g), byte(b), (a.clamp(0.0, 1.0) * 1000.0).round() / 1000.0),
  }
}

/// `in oklab, <color> P%, transparent`, the shape an opacity modifier compiles to, as the color
/// at that opacity. Any other mix is left as written.
fn mix(args: &str) -> Option<String> {
  let parts: Vec<&str> = split_top(args, ',');
  let [space, color, other] = parts.as_slice() else { return None };
  if space.trim() != "in oklab" || other.trim() != "transparent" {
    return None;
  }
  let color = color.trim();
  let (color, share) = match color.rsplit_once(' ') {
    Some((c, p)) if p.ends_with('%') => (c.trim(), number(p)?),
    _ => (color, 1.0),
  };
  let (r, g, b, a) = parse_srgb(color)?;
  Some(srgb(r, g, b, a * share))
}

fn split_top(text: &str, at: char) -> Vec<&str> {
  let mut out = Vec::new();
  let mut depth = 0;
  let mut start = 0;
  for (i, c) in text.char_indices() {
    match c {
      '(' => depth += 1,
      ')' => depth -= 1,
      c if c == at && depth == 0 => {
        out.push(&text[start..i]);
        start = i + 1;
      }
      _ => {}
    }
  }
  out.push(&text[start..]);
  out
}

/// A color this module writes, `#rrggbb` or `rgba(r, g, b, a)`, back as channels and alpha.
fn parse_srgb(color: &str) -> Option<(f32, f32, f32, f32)> {
  if let Some(hex) = color.strip_prefix('#') {
    let full: String = match hex.len() {
      3 => hex.chars().flat_map(|c| [c, c]).collect(),
      6 => hex.to_owned(),
      _ => return None,
    };
    let channel = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok().map(|v| v as f32 / 255.0);
    return Some((channel(0)?, channel(2)?, channel(4)?, 1.0));
  }
  let inner = color.strip_prefix("rgba(").or_else(|| color.strip_prefix("rgb("))?.strip_suffix(')')?;
  let parts: Vec<f32> = inner.split(',').map(|p| p.trim().parse::<f32>().ok()).collect::<Option<_>>()?;
  match parts.as_slice() {
    [r, g, b] => Some((r / 255.0, g / 255.0, b / 255.0, 1.0)),
    [r, g, b, a] => Some((r / 255.0, g / 255.0, b / 255.0, *a)),
    _ => None,
  }
}

/// A `calc()` of numbers and at most one unit, multiplied and divided only by numbers, as one
/// value. Anything else stays a `calc()`.
fn calc(args: &str) -> Option<String> {
  let tokens: Vec<&str> = args.split_whitespace().collect();
  let mut unit: Option<&str> = None;
  let mut value: Option<f32> = None;
  let mut op = '*';
  let mut adding = false;
  for (i, token) in tokens.iter().enumerate() {
    if i % 2 == 1 {
      op = token.chars().next()?;
      if !matches!(op, '*' | '/' | '+' | '-') || token.len() != 1 {
        return None;
      }
      adding |= matches!(op, '+' | '-');
      continue;
    }
    let split = token.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(token.len());
    let (n, u) = token.split_at(split);
    let n = n.parse::<f32>().ok()?;
    if !u.is_empty() {
      if unit.is_some_and(|known| known != u) || (op == '/' && i > 0) {
        return None;
      }
      unit = Some(u);
    }
    value = Some(match value {
      None => n,
      Some(v) => match op {
        '*' => v * n,
        '/' => v / n,
        '+' => v + n,
        _ => v - n,
      },
    });
  }
  if adding && unit.is_some() && tokens.iter().step_by(2).any(|t| !t.ends_with(unit.unwrap_or(""))) {
    return None;
  }
  let value = value?;
  let rounded = (value * 10_000.0).round() / 10_000.0;
  Some(format!("{rounded}{}", unit.unwrap_or("")))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn oklch_is_written_in_srgb() {
    assert_eq!(card_value("oklch(100% 0 0)"), "#ffffff");
    assert_eq!(card_value("oklch(0% 0 0)"), "#000000");
    assert_eq!(card_value("oklch(62.3% 0.214 259.815)"), "#2b7fff");
    assert_eq!(card_value("oklch(62.3% 0.214 259.815 / 50%)"), "rgba(43, 127, 255, 0.5)");
  }

  #[test]
  fn an_opacity_modifier_becomes_an_alpha() {
    assert_eq!(card_value("color-mix(in oklab, oklch(100% 0 0) 25%, transparent)"), "rgba(255, 255, 255, 0.25)");
    assert_eq!(card_value("color-mix(in oklab, #112233 50%, transparent)"), "rgba(17, 34, 51, 0.5)");
    assert_eq!(card_value("color-mix(in srgb, red 50%, blue)"), "color-mix(in srgb, red 50%, blue)");
  }

  #[test]
  fn calc_of_one_unit_is_worked_out() {
    assert_eq!(card_value("calc(.25rem * 4)"), "1rem");
    assert_eq!(card_value("calc(2.5 / 2.25)"), "1.1111");
    assert_eq!(card_value("calc(infinity * 1px)"), "9999px");
    assert_eq!(card_value("calc(100% - 2rem)"), "calc(100% - 2rem)");
  }

  #[test]
  fn breakpoints_become_widths() {
    assert_eq!(min_width("(width >= 40rem)"), Some(640.0));
    assert_eq!(min_width("(min-width: 900px)"), Some(900.0));
    assert_eq!(min_width("(prefers-color-scheme: dark)"), None);
  }
}
