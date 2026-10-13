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

/// Tailwind v4.3.2's `replace-shadow-colors.test.ts`.
#[cfg(test)]
mod tests {
  use super::replace_shadow_colors;
  use crate::utilities::replace_alpha;

  fn plain(input: &str) -> String {
    replace_shadow_colors(input, &mut |color| format!("var(--tw-shadow-color, {color})"))
  }

  fn half(input: &str) -> String {
    replace_shadow_colors(input, &mut |color| {
      format!("var(--tw-shadow-color, {})", replace_alpha(color, "50%"))
    })
  }

  #[test]
  fn without_replacer() {
    for (input, expected) in [
      ("var(--my-shadow)", "var(--my-shadow)"),
      ("1px var(--my-shadow)", "1px var(--my-shadow)"),
      (
        "1px 1px var(--my-color)",
        "1px 1px var(--tw-shadow-color, var(--my-color))",
      ),
      ("0 0 0 var(--my-color)", "0 0 0 var(--tw-shadow-color, var(--my-color))"),
      ("1px 2px", "1px 2px var(--tw-shadow-color, currentcolor)"),
      ("1px 2px 3px", "1px 2px 3px var(--tw-shadow-color, currentcolor)"),
      (
        "1px 2px 3px 4px",
        "1px 2px 3px 4px var(--tw-shadow-color, currentcolor)",
      ),
      (
        "var(--my-shadow), 1px 1px var(--my-color), 0 0 1px var(--my-color)",
        "var(--my-shadow), 1px 1px var(--tw-shadow-color, var(--my-color)), 0 0 1px var(--tw-shadow-color, var(--my-color))",
      ),
    ] {
      assert_eq!(plain(input), expected, "{input}");
    }
  }

  #[test]
  fn finds_the_color_regardless_of_its_position() {
    for x in ["calc(var(--spacing) * 1)", "1", "--spacing(1)"] {
      for y in ["calc(var(--spacing) * 2)", "2", "--spacing(2)"] {
        for blur in ["calc(var(--spacing) * 3)", "3", "--spacing(3)"] {
          for spread in ["calc(var(--spacing) * 4)", "4", "--spacing(4)"] {
            for color in [
              "black",
              "rgb(0, 0, 0)",
              "#000",
              "--alpha(var(--color) / 50%)",
              "var(--uknown-color)",
            ] {
              let expected_color = format!("var(--tw-shadow-color, {color})");
              for input in [
                format!("{x} {color} {y} {blur} {spread}"),
                format!("{x} {y} {color} {blur} {spread}"),
                format!("{x} {y} {blur} {color} {spread}"),
              ] {
                assert_eq!(plain(&input), input.replacen(color, &expected_color, 1), "{input}");
              }
            }
          }
        }
      }
    }
  }

  #[test]
  fn finds_the_color_after_variables() {
    for color in ["black", "#000", "rgb(0, 0, 0)", "--alpha(var(--color) / 50%)"] {
      let expected_color = format!("var(--tw-shadow-color, {color})");
      for input in [
        format!("var(--x) var(--y) {color}"),
        format!("var(--x) var(--y) var(--blur) {color}"),
        format!("var(--x) var(--y) var(--blur) var(--spread) {color}"),
      ] {
        assert_eq!(plain(&input), input.replacen(color, &expected_color, 1), "{input}");
      }
    }
  }

  #[test]
  fn with_replacer() {
    for (input, expected) in [
      (
        "1px 1px var(--my-color)",
        "1px 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%))",
      ),
      (
        "1px 2px 3px 4px",
        "1px 2px 3px 4px var(--tw-shadow-color, oklab(from currentcolor l a b / 50%))",
      ),
      (
        "var(--my-shadow), 1px 1px var(--my-color), 0 0 1px var(--my-color)",
        "var(--my-shadow), 1px 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%)), 0 0 1px var(--tw-shadow-color, oklab(from var(--my-color) l a b / 50%))",
      ),
    ] {
      assert_eq!(half(input), expected, "{input}");
    }
  }
}
