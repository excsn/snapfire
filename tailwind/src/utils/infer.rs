//! Data type inference for arbitrary values, after Tailwind's `infer-data-type.ts` and
//! `is-color.ts`.

use std::sync::LazyLock;

use regex::Regex;

use crate::utils::js::{is_integer, number_to_string, to_number};
use crate::utils::math::has_math_fn;
use crate::utils::segment::segment;

const NAMED_COLORS: &[&str] = &[
  "black",
  "silver",
  "gray",
  "white",
  "maroon",
  "red",
  "purple",
  "fuchsia",
  "green",
  "lime",
  "olive",
  "yellow",
  "navy",
  "blue",
  "teal",
  "aqua",
  "aliceblue",
  "antiquewhite",
  "aquamarine",
  "azure",
  "beige",
  "bisque",
  "blanchedalmond",
  "blueviolet",
  "brown",
  "burlywood",
  "cadetblue",
  "chartreuse",
  "chocolate",
  "coral",
  "cornflowerblue",
  "cornsilk",
  "crimson",
  "cyan",
  "darkblue",
  "darkcyan",
  "darkgoldenrod",
  "darkgray",
  "darkgreen",
  "darkgrey",
  "darkkhaki",
  "darkmagenta",
  "darkolivegreen",
  "darkorange",
  "darkorchid",
  "darkred",
  "darksalmon",
  "darkseagreen",
  "darkslateblue",
  "darkslategray",
  "darkslategrey",
  "darkturquoise",
  "darkviolet",
  "deeppink",
  "deepskyblue",
  "dimgray",
  "dimgrey",
  "dodgerblue",
  "firebrick",
  "floralwhite",
  "forestgreen",
  "gainsboro",
  "ghostwhite",
  "gold",
  "goldenrod",
  "greenyellow",
  "grey",
  "honeydew",
  "hotpink",
  "indianred",
  "indigo",
  "ivory",
  "khaki",
  "lavender",
  "lavenderblush",
  "lawngreen",
  "lemonchiffon",
  "lightblue",
  "lightcoral",
  "lightcyan",
  "lightgoldenrodyellow",
  "lightgray",
  "lightgreen",
  "lightgrey",
  "lightpink",
  "lightsalmon",
  "lightseagreen",
  "lightskyblue",
  "lightslategray",
  "lightslategrey",
  "lightsteelblue",
  "lightyellow",
  "limegreen",
  "linen",
  "magenta",
  "mediumaquamarine",
  "mediumblue",
  "mediumorchid",
  "mediumpurple",
  "mediumseagreen",
  "mediumslateblue",
  "mediumspringgreen",
  "mediumturquoise",
  "mediumvioletred",
  "midnightblue",
  "mintcream",
  "mistyrose",
  "moccasin",
  "navajowhite",
  "oldlace",
  "olivedrab",
  "orange",
  "orangered",
  "orchid",
  "palegoldenrod",
  "palegreen",
  "paleturquoise",
  "palevioletred",
  "papayawhip",
  "peachpuff",
  "peru",
  "pink",
  "plum",
  "powderblue",
  "rebeccapurple",
  "rosybrown",
  "royalblue",
  "saddlebrown",
  "salmon",
  "sandybrown",
  "seagreen",
  "seashell",
  "sienna",
  "skyblue",
  "slateblue",
  "slategray",
  "slategrey",
  "snow",
  "springgreen",
  "steelblue",
  "tan",
  "thistle",
  "tomato",
  "turquoise",
  "violet",
  "wheat",
  "whitesmoke",
  "yellowgreen",
  "transparent",
  "currentcolor",
  "canvas",
  "canvastext",
  "linktext",
  "visitedtext",
  "activetext",
  "buttonface",
  "buttontext",
  "buttonborder",
  "field",
  "fieldtext",
  "highlight",
  "highlighttext",
  "selecteditem",
  "selecteditemtext",
  "mark",
  "marktext",
  "graytext",
  "accentcolor",
  "accentcolortext",
];

static IS_COLOR_FN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"(?i)^(rgba?|hsla?|hwb|color|(ok)?(lab|lch)|light-dark|color-mix|--alpha)\(").unwrap());

pub fn is_named_color(value: &str) -> bool {
  let lower = value.to_lowercase();
  NAMED_COLORS.contains(&lower.as_str())
}

pub fn is_color(value: &str) -> bool {
  value.starts_with('#') || IS_COLOR_FN.is_match(value) || is_named_color(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DataType {
  Color,
  Length,
  Percentage,
  Ratio,
  Number,
  Integer,
  Url,
  Position,
  BgSize,
  LineWidth,
  Image,
  FamilyName,
  GenericName,
  AbsoluteSize,
  RelativeSize,
  Angle,
  Vector,
}

impl DataType {
  pub fn parse(name: &str) -> Option<DataType> {
    Some(match name {
      "color" => DataType::Color,
      "length" => DataType::Length,
      "percentage" => DataType::Percentage,
      "ratio" => DataType::Ratio,
      "number" => DataType::Number,
      "integer" => DataType::Integer,
      "url" => DataType::Url,
      "position" => DataType::Position,
      "bg-size" => DataType::BgSize,
      "line-width" => DataType::LineWidth,
      "image" => DataType::Image,
      "family-name" => DataType::FamilyName,
      "generic-name" => DataType::GenericName,
      "absolute-size" => DataType::AbsoluteSize,
      "relative-size" => DataType::RelativeSize,
      "angle" => DataType::Angle,
      "vector" => DataType::Vector,
      _ => return None,
    })
  }

  pub fn name(self) -> &'static str {
    match self {
      DataType::Color => "color",
      DataType::Length => "length",
      DataType::Percentage => "percentage",
      DataType::Ratio => "ratio",
      DataType::Number => "number",
      DataType::Integer => "integer",
      DataType::Url => "url",
      DataType::Position => "position",
      DataType::BgSize => "bg-size",
      DataType::LineWidth => "line-width",
      DataType::Image => "image",
      DataType::FamilyName => "family-name",
      DataType::GenericName => "generic-name",
      DataType::AbsoluteSize => "absolute-size",
      DataType::RelativeSize => "relative-size",
      DataType::Angle => "angle",
      DataType::Vector => "vector",
    }
  }

  fn check(self, value: &str) -> bool {
    match self {
      DataType::Color => is_color(value),
      DataType::Length => is_length(value),
      DataType::Percentage => is_percentage(value),
      DataType::Ratio => is_fraction(value),
      DataType::Number => is_number(value),
      DataType::Integer => is_positive_integer(value),
      DataType::Url => is_url(value),
      DataType::Position => is_background_position(value),
      DataType::BgSize => is_background_size(value),
      DataType::LineWidth => is_line_width(value),
      DataType::Image => is_image(value),
      DataType::FamilyName => is_family_name(value),
      DataType::GenericName => is_generic_name(value),
      DataType::AbsoluteSize => is_absolute_size(value),
      DataType::RelativeSize => matches!(value, "larger" | "smaller"),
      DataType::Angle => IS_ANGLE.is_match(value),
      DataType::Vector => IS_VECTOR.is_match(value),
    }
  }
}

/// The first of `types` that `value` is valid as. `None` for a `var(…)`.
pub fn infer_data_type(value: &str, types: &[DataType]) -> Option<DataType> {
  if value.starts_with("var(") {
    return None;
  }
  types.iter().copied().find(|t| t.check(value))
}

const NUMBER: &str = r"[+-]?[0-9]*\.?[0-9]+(?:[eE][+-]?[0-9]+)?";
const LENGTH_UNITS: &str =
  "cm|mm|Q|in|pc|pt|px|em|ex|ch|rem|lh|rlh|vw|vh|vmin|vmax|vb|vi|svw|svh|lvw|lvh|dvw|dvh|cqw|cqh|cqi|cqb|cqmin|cqmax";

static IS_URL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^url\(.*\)$").unwrap());
static IS_IMAGE_FN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^(?:element|image|cross-fade|image-set)\(").unwrap());
static IS_GRADIENT_FN: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^(repeating-)?(conic|linear|radial)-gradient\(").unwrap());
static IS_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{NUMBER}$")).unwrap());
static IS_PERCENTAGE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{NUMBER}%$")).unwrap());
static IS_FRACTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!(r"^{NUMBER}\s*/\s*{NUMBER}$")).unwrap());
static IS_LENGTH: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{NUMBER}({LENGTH_UNITS})$")).unwrap());
static IS_LENGTH_FN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)^(--spacing)\(").unwrap());
static IS_ANGLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{NUMBER}(deg|rad|grad|turn)$")).unwrap());
static IS_VECTOR: LazyLock<Regex> = LazyLock::new(|| Regex::new(&format!("^{NUMBER} +{NUMBER} +{NUMBER}$")).unwrap());

fn is_url(value: &str) -> bool {
  IS_URL.is_match(value)
}

fn is_line_width(value: &str) -> bool {
  segment(value, b' ')
    .iter()
    .all(|v| is_length(v) || is_number(v) || matches!(*v, "thin" | "medium" | "thick"))
}

fn is_image(value: &str) -> bool {
  let mut count = 0;
  for part in segment(value, b',') {
    if part.starts_with("var(") {
      continue;
    }
    if is_url(part) || IS_GRADIENT_FN.is_match(part) || IS_IMAGE_FN.is_match(part) {
      count += 1;
      continue;
    }
    return false;
  }
  count > 0
}

fn is_generic_name(value: &str) -> bool {
  matches!(
    value,
    "serif"
      | "sans-serif"
      | "monospace"
      | "cursive"
      | "fantasy"
      | "system-ui"
      | "ui-serif"
      | "ui-sans-serif"
      | "ui-monospace"
      | "ui-rounded"
      | "math"
      | "emoji"
      | "fangsong"
  )
}

fn is_family_name(value: &str) -> bool {
  let mut count = 0;
  for part in segment(value, b',') {
    if part.as_bytes().first().is_some_and(u8::is_ascii_digit) {
      return false;
    }
    if part.starts_with("var(") {
      continue;
    }
    count += 1;
  }
  count > 0
}

fn is_absolute_size(value: &str) -> bool {
  matches!(
    value,
    "xx-small" | "x-small" | "small" | "medium" | "large" | "x-large" | "xx-large" | "xxx-large"
  )
}

pub fn is_number(value: &str) -> bool {
  IS_NUMBER.is_match(value) || has_math_fn(value)
}

pub fn is_percentage(value: &str) -> bool {
  IS_PERCENTAGE.is_match(value) || has_math_fn(value)
}

pub fn is_fraction(value: &str) -> bool {
  IS_FRACTION.is_match(value) || has_math_fn(value)
}

pub fn is_length(value: &str) -> bool {
  IS_LENGTH.is_match(value) || IS_LENGTH_FN.is_match(value) || has_math_fn(value)
}

fn is_background_position(value: &str) -> bool {
  let mut count = 0;
  for part in segment(value, b' ') {
    if matches!(part, "center" | "top" | "right" | "bottom" | "left") {
      count += 1;
      continue;
    }
    if part.starts_with("var(") {
      continue;
    }
    if is_length(part) || is_percentage(part) {
      count += 1;
      continue;
    }
    return false;
  }
  count > 0
}

fn is_background_size(value: &str) -> bool {
  let mut count = 0;
  for size in segment(value, b',') {
    if size == "cover" || size == "contain" {
      count += 1;
      continue;
    }
    let values = segment(size, b' ');
    if values.len() != 1 && values.len() != 2 {
      return false;
    }
    if values.iter().all(|v| *v == "auto" || is_length(v) || is_percentage(v)) {
      count += 1;
    }
  }
  count > 0
}

/// Whether `value` is a whole number at or above zero, written the way JavaScript prints it.
pub fn is_positive_integer(value: &str) -> bool {
  let num = to_number(value);
  is_integer(num) && num >= 0.0 && number_to_string(num) == value
}

pub fn is_strict_positive_integer(value: &str) -> bool {
  let num = to_number(value);
  is_integer(num) && num > 0.0 && number_to_string(num) == value
}

pub fn is_valid_spacing_multiplier(value: &str) -> bool {
  is_multiple_of(value, 0.25)
}

pub fn is_valid_opacity_value(value: &str) -> bool {
  is_multiple_of(value, 0.25)
}

fn is_multiple_of(value: &str, divisor: f64) -> bool {
  let num = to_number(value);
  num >= 0.0 && num % divisor == 0.0 && number_to_string(num) == value
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn infers() {
    assert_eq!(infer_data_type("12px", &[DataType::Length]), Some(DataType::Length));
    assert_eq!(
      infer_data_type("#fff", &[DataType::Length, DataType::Color]),
      Some(DataType::Color)
    );
    assert_eq!(infer_data_type("var(--x)", &[DataType::Color]), None);
    assert_eq!(
      infer_data_type("45deg", &[DataType::Angle, DataType::Vector]),
      Some(DataType::Angle)
    );
    assert_eq!(
      infer_data_type("1 0 0", &[DataType::Angle, DataType::Vector]),
      Some(DataType::Vector)
    );
    assert!(is_positive_integer("12"));
    assert!(!is_positive_integer("012"));
    assert!(!is_positive_integer("1.5"));
    assert!(is_valid_spacing_multiplier("1.5"));
    assert!(!is_valid_spacing_multiplier("1.3"));
    assert!(!is_valid_spacing_multiplier("1.50"));
  }
}
