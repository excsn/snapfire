//! The props Ink's `Box` and `Text` take, one row each: the markup reader sets
//! a prop through its row and [`prop_declarations`] declares it from the same row.

use crate::{Align, Border, BoxStyle, Color, Direction, Extent, Justify, Text, Wrap};

/// One prop: its name as Ink spells it and the value it takes, with what writing it does.
pub struct Prop<S: 'static> {
  pub name: &'static str,
  pub value: Value<S>,
}

/// What a prop's value is and how it lands on the element it styles.
pub enum Value<S: 'static> {
  /// A whole number of cells.
  Cells(fn(&mut S, u16)),
  /// A number of cells, fractions kept.
  Edge(fn(&mut S, f32)),
  /// A number of cells, rounded toward zero.
  Offset(fn(&mut S, i32)),
  /// Whole cells or a percentage of the parent.
  Extent(fn(&mut S, Extent)),
  Number(fn(&mut S, f32)),
  /// On unless written `false`.
  Flag(fn(&mut S, bool)),
  Color(fn(&mut S, Option<Color>)),
  /// One of these words, each with what it sets.
  Words(&'static [(&'static str, fn(&mut S))]),
}

/// The props of `Box`.
pub static BOX_PROPS: &[Prop<BoxStyle>] = &[
  Prop {
    name: "flexDirection",
    value: Value::Words(&[
      ("row", |s| s.direction = Direction::Row),
      ("column", |s| s.direction = Direction::Column),
      ("row-reverse", |s| s.direction = Direction::RowReverse),
      ("column-reverse", |s| s.direction = Direction::ColumnReverse),
    ]),
  },
  Prop { name: "gap", value: Value::Cells(|s, n| s.gap = n) },
  Prop { name: "columnGap", value: Value::Cells(|s, n| s.gap = n) },
  Prop { name: "rowGap", value: Value::Cells(|s, n| s.gap = n) },
  Prop { name: "padding", value: Value::Edge(|s, n| s.padding = [n; 4]) },
  Prop { name: "paddingX", value: Value::Edge(|s, n| (s.padding[1], s.padding[3]) = (n, n)) },
  Prop { name: "paddingY", value: Value::Edge(|s, n| (s.padding[0], s.padding[2]) = (n, n)) },
  Prop { name: "paddingTop", value: Value::Edge(|s, n| s.padding[0] = n) },
  Prop { name: "paddingRight", value: Value::Edge(|s, n| s.padding[1] = n) },
  Prop { name: "paddingBottom", value: Value::Edge(|s, n| s.padding[2] = n) },
  Prop { name: "paddingLeft", value: Value::Edge(|s, n| s.padding[3] = n) },
  Prop { name: "margin", value: Value::Edge(|s, n| s.margin = [n; 4]) },
  Prop { name: "marginX", value: Value::Edge(|s, n| (s.margin[1], s.margin[3]) = (n, n)) },
  Prop { name: "marginY", value: Value::Edge(|s, n| (s.margin[0], s.margin[2]) = (n, n)) },
  Prop { name: "marginTop", value: Value::Edge(|s, n| s.margin[0] = n) },
  Prop { name: "marginRight", value: Value::Edge(|s, n| s.margin[1] = n) },
  Prop { name: "marginBottom", value: Value::Edge(|s, n| s.margin[2] = n) },
  Prop { name: "marginLeft", value: Value::Edge(|s, n| s.margin[3] = n) },
  Prop { name: "width", value: Value::Extent(|s, e| s.width = Some(e)) },
  Prop { name: "height", value: Value::Extent(|s, e| s.height = Some(e)) },
  Prop { name: "flexGrow", value: Value::Number(|s, n| s.grow = n) },
  Prop { name: "flexShrink", value: Value::Number(|s, n| s.shrink = Some(n)) },
  Prop { name: "flexBasis", value: Value::Extent(|s, e| s.basis = Some(e)) },
  Prop {
    name: "justifyContent",
    value: Value::Words(&[
      ("flex-start", |s| s.justify = Some(Justify::Start)),
      ("center", |s| s.justify = Some(Justify::Center)),
      ("flex-end", |s| s.justify = Some(Justify::End)),
      ("space-between", |s| s.justify = Some(Justify::SpaceBetween)),
      ("space-around", |s| s.justify = Some(Justify::SpaceAround)),
      ("space-evenly", |s| s.justify = Some(Justify::SpaceEvenly)),
    ]),
  },
  Prop {
    name: "alignItems",
    value: Value::Words(&[
      ("flex-start", |s| s.align = Some(Align::Start)),
      ("center", |s| s.align = Some(Align::Center)),
      ("flex-end", |s| s.align = Some(Align::End)),
      ("stretch", |s| s.align = Some(Align::Stretch)),
    ]),
  },
  Prop {
    name: "borderStyle",
    value: Value::Words(&[
      ("single", |s| s.border = Some(Border::Single)),
      ("double", |s| s.border = Some(Border::Double)),
      ("round", |s| s.border = Some(Border::Round)),
      ("bold", |s| s.border = Some(Border::Bold)),
      ("singleDouble", |s| s.border = Some(Border::SingleDouble)),
      ("doubleSingle", |s| s.border = Some(Border::DoubleSingle)),
      ("classic", |s| s.border = Some(Border::Classic)),
      ("arrow", |s| s.border = Some(Border::Arrow)),
    ]),
  },
  Prop { name: "borderColor", value: Value::Color(|s, c| s.border_color = c) },
  Prop { name: "overflow", value: Value::Words(&[("visible", |s| (s.overflow_x, s.overflow_y) = (false, false)), ("hidden", |s| (s.overflow_x, s.overflow_y) = (true, true))]) },
  Prop { name: "overflowX", value: Value::Words(&[("visible", |s| s.overflow_x = false), ("hidden", |s| s.overflow_x = true)]) },
  Prop { name: "overflowY", value: Value::Words(&[("visible", |s| s.overflow_y = false), ("hidden", |s| s.overflow_y = true)]) },
  Prop { name: "contentOffsetX", value: Value::Offset(|s, n| s.content_offset.0 = n) },
  Prop { name: "contentOffsetY", value: Value::Offset(|s, n| s.content_offset.1 = n) },
];

/// The props of `Text`.
pub static TEXT_PROPS: &[Prop<Text>] = &[
  Prop { name: "color", value: Value::Color(|t, c| t.style.color = c) },
  Prop { name: "backgroundColor", value: Value::Color(|t, c| t.style.background = c) },
  Prop { name: "bold", value: Value::Flag(|t, on| t.style.bold = on) },
  Prop { name: "dimColor", value: Value::Flag(|t, on| t.style.dim = on) },
  Prop { name: "italic", value: Value::Flag(|t, on| t.style.italic = on) },
  Prop { name: "underline", value: Value::Flag(|t, on| t.style.underline = on) },
  Prop { name: "strikethrough", value: Value::Flag(|t, on| t.style.strikethrough = on) },
  Prop { name: "inverse", value: Value::Flag(|t, on| t.style.inverse = on) },
  Prop {
    name: "wrap",
    value: Value::Words(&[
      ("wrap", |t| t.wrap = Wrap::Wrap),
      ("truncate", |t| t.wrap = Wrap::Truncate),
      ("truncate-end", |t| t.wrap = Wrap::Truncate),
      ("truncate-start", |t| t.wrap = Wrap::TruncateStart),
      ("truncate-middle", |t| t.wrap = Wrap::TruncateMiddle),
    ]),
  },
];

/// `paddingLeft` as `padding-left`, the attribute the markup carries a prop as.
pub(crate) fn kebab(name: &str) -> String {
  let mut out = String::with_capacity(name.len() + 4);
  for ch in name.chars() {
    if ch.is_ascii_uppercase() {
      out.push('-');
      out.push(ch.to_ascii_lowercase());
    } else {
      out.push(ch);
    }
  }
  out
}

/// Sets the prop written as `attribute`, kebab-case or as Ink spells it, on `target`; `Ok(false)` for an attribute none of `props` is.
pub(crate) fn apply<S>(props: &[Prop<S>], element: &str, attribute: &str, value: &str, target: &mut S) -> Result<bool, String> {
  let name = kebab(attribute);
  let Some(prop) = props.iter().find(|prop| kebab(prop.name) == name) else { return Ok(false) };
  let wrong = |what: &str| format!("`{name}` on a {element} is {what}, not `{value}`");
  match &prop.value {
    Value::Cells(set) => set(target, value.parse().map_err(|_| wrong("a number of cells"))?),
    Value::Edge(set) => set(target, value.parse::<f32>().ok().filter(|n| n.is_finite()).ok_or_else(|| wrong("a number of cells"))?),
    Value::Offset(set) => set(target, value.parse::<f64>().ok().filter(|n| n.is_finite()).map(|n| n.trunc() as i32).ok_or_else(|| wrong("a number of cells"))?),
    Value::Extent(set) => {
      let extent = match value.strip_suffix('%') {
        Some(share) => share.trim().parse::<f32>().ok().filter(|p| p.is_finite() && *p >= 0.0).map(Extent::Percent),
        None => value.parse::<u16>().ok().map(Extent::Cells),
      };
      set(target, extent.ok_or_else(|| wrong("cells or a percentage"))?)
    }
    Value::Number(set) => set(target, value.parse().map_err(|_| wrong("a number"))?),
    Value::Flag(set) => set(target, value != "false"),
    Value::Color(set) => set(target, Color::parse(value)),
    Value::Words(words) => {
      let Some((_, set)) = words.iter().find(|(word, _)| *word == value) else {
        let named: Vec<String> = words.iter().map(|(word, _)| format!("`{word}`")).collect();
        return Err(wrong(&format!("one of {}", named.join(", "))));
      };
      set(target)
    }
  }
  Ok(true)
}

/// The TypeScript type of a prop's value.
fn declared<S>(value: &Value<S>) -> String {
  match value {
    Value::Cells(_) | Value::Edge(_) | Value::Offset(_) | Value::Number(_) => "number".to_owned(),
    Value::Extent(_) => "Extent".to_owned(),
    Value::Flag(_) => "boolean".to_owned(),
    Value::Color(_) => "string".to_owned(),
    Value::Words(words) => words.iter().map(|(word, _)| format!("\"{word}\"")).collect::<Vec<_>>().join(" | "),
  }
}

fn interface<S>(name: &str, doc: &str, fixed: &[&str], props: &[Prop<S>]) -> String {
  let mut out = format!("/** {doc} */\nexport interface {name} {{\n");
  for line in fixed {
    out.push_str(&format!("  {line}\n"));
  }
  for prop in props {
    out.push_str(&format!("  {}?: {};\n", prop.name, declared(&prop.value)));
  }
  out.push_str("}\n");
  out
}

/// The TypeScript of `Extent`, `BoxProps` and `TextProps`, from [`BOX_PROPS`] and [`TEXT_PROPS`]; `ReactNode` is the importing file's.
pub fn prop_declarations() -> String {
  let mut out = String::from("/** A number of cells or a percentage of the parent, such as `\"50%\"`. */\nexport type Extent = number | `${number}%`;\n\n");
  out.push_str(&interface("BoxProps", "The props of a Box the terminal lays out and draws.", &["children?: ReactNode;", "key?: string | number;", "ref?: unknown;"], BOX_PROPS));
  out.push('\n');
  out.push_str(&interface("TextProps", "The props of a Text the terminal draws.", &["children?: ReactNode;", "key?: string | number;"], TEXT_PROPS));
  out
}
