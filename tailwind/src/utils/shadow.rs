use std::sync::LazyLock;

use regex::Regex;

use crate::utils::infer::is_named_color;
use crate::utils::segment::segment;
use crate::value_parser::{self, ValueNode};
use crate::walk::Action;

const KEYWORDS: &[&str] = &["inset", "inherit", "initial", "revert", "unset"];
const LENGTH_FUNCTIONS: &[&str] = &["calc", "clamp", "max", "min", "--spacing"];
const COLOR_FUNCTIONS: &[&str] = &[
  "color",
  "color-mix",
  "contrast-color",
  "device-cmyk",
  "hsl",
  "hsla",
  "hwb",
  "lab",
  "lch",
  "light-dark",
  "oklab",
  "oklch",
  "rgb",
  "rgba",
  "--alpha",
];
static LENGTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^-?([0-9]+|\.[0-9]+)(.*?)$").unwrap());

/// Rewrites the color of each shadow in a comma separated list through `replacement`,
/// appending one when a shadow has none.
pub fn replace_shadow_colors(input: &str, replacement: &mut dyn FnMut(&str) -> String) -> String {
  let replace_node = |node: &ValueNode, replacement: &mut dyn FnMut(&str) -> String| -> Vec<ValueNode> {
    let color = value_parser::to_css(std::slice::from_ref(node));
    value_parser::parse(&replacement(&color))
  };

  let shadows: Vec<String> = segment(input, b',')
    .into_iter()
    .map(|shadow| {
      let shadow = shadow.trim();
      let mut ast = value_parser::parse(shadow);
      let mut unknown: Option<usize> = None;
      let mut unknowns = 0;
      let mut lengths = 0;
      let mut replaced = false;

      value_parser::walk(&mut ast, &mut |node, visit| match node {
        ValueNode::Word(w) => {
          let lower = w.to_lowercase();
          if KEYWORDS.contains(&lower.as_str()) {
            return Action::Continue;
          }
          if LENGTH.is_match(&lower) {
            lengths += 1;
            return Action::Continue;
          }
          if w.starts_with('#') || is_named_color(w) {
            replaced = true;
            let nodes = replace_node(node, replacement);
            return Action::ReplaceStop(nodes);
          }
          if visit.depth == 0 {
            unknown = Some(visit.index);
          }
          unknowns += 1;
          Action::Continue
        }
        ValueNode::Function { value, .. } => {
          let lower = value.to_lowercase();
          if COLOR_FUNCTIONS.contains(&lower.as_str()) {
            replaced = true;
            let nodes = replace_node(node, replacement);
            return Action::ReplaceStop(nodes);
          }
          if LENGTH_FUNCTIONS.contains(&lower.as_str()) {
            lengths += 1;
            return Action::Skip;
          }
          if visit.depth == 0 {
            unknown = Some(visit.index);
          }
          unknowns += 1;
          Action::Skip
        }
        ValueNode::Separator(_) => Action::Continue,
      });

      if replaced {
        return value_parser::to_css(&ast);
      }
      if lengths < 2 {
        return shadow.to_owned();
      }
      if unknowns == 0 {
        return format!("{shadow} {}", replacement("currentcolor"));
      }
      if unknowns == 1
        && let Some(index) = unknown
      {
        let nodes = replace_node(&ast[index], replacement);
        ast.splice(index..index + 1, nodes);
        return value_parser::to_css(&ast);
      }
      shadow.to_owned()
    })
    .collect();

  shadows.join(", ")
}
