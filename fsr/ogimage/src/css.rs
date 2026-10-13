use std::f32::consts::PI;

/// Why a declaration was not applied. `Unsupported` is a value CSS has that
/// the subset leaves out, which warns. `Invalid` is not CSS, which fails.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Bad {
  Unsupported(String),
  Invalid(String),
}

type Parsed<T> = Result<T, Bad>;

fn invalid<T>(reason: impl Into<String>) -> Parsed<T> {
  Err(Bad::Invalid(reason.into()))
}

fn unsupported<T>(reason: impl Into<String>) -> Parsed<T> {
  Err(Bad::Unsupported(reason.into()))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Color {
  pub r: f32,
  pub g: f32,
  pub b: f32,
  pub a: f32,
}

impl Color {
  pub const TRANSPARENT: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 0.0 };
  pub const BLACK: Color = Color { r: 0.0, g: 0.0, b: 0.0, a: 1.0 };

  fn rgb8(r: u8, g: u8, b: u8) -> Color {
    Color { r: r as f32 / 255.0, g: g as f32 / 255.0, b: b as f32 / 255.0, a: 1.0 }
  }

  pub fn is_visible(self) -> bool {
    self.a > 0.0
  }

  pub fn skia(self) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba(self.r.clamp(0.0, 1.0), self.g.clamp(0.0, 1.0), self.b.clamp(0.0, 1.0), self.a.clamp(0.0, 1.0)).unwrap_or(tiny_skia::Color::TRANSPARENT)
  }
}

/// A color as written: `currentColor` resolves against the element's `color`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Paint {
  Color(Color),
  Current,
}

impl Paint {
  pub fn resolve(self, current: Color) -> Color {
    match self {
      Paint::Color(c) => c,
      Paint::Current => current,
    }
  }
}

/// A length as `px` plus a percentage of whatever the property measures
/// against, which is how `calc(100% - 20px)` is carried.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Len {
  pub px: f32,
  pub pct: f32,
}

impl Len {
  pub const ZERO: Len = Len { px: 0.0, pct: 0.0 };

  pub fn px(px: f32) -> Len {
    Len { px, pct: 0.0 }
  }

  pub fn pct(pct: f32) -> Len {
    Len { px: 0.0, pct }
  }

  pub fn resolve(self, basis: f32) -> f32 {
    self.px + self.pct / 100.0 * basis
  }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LenAuto {
  Auto,
  Len(Len),
}

/// What `em`, `rem`, `vw` and `vh` mean where a value is read.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Units {
  pub em: f32,
  pub rem: f32,
  pub width: f32,
  pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Display {
  Flex,
  None,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Position {
  Relative,
  Absolute,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Direction {
  Row,
  RowReverse,
  Column,
  ColumnReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Wrap {
  NoWrap,
  Wrap,
  WrapReverse,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Align {
  Start,
  End,
  FlexStart,
  FlexEnd,
  Center,
  Stretch,
  SpaceBetween,
  SpaceAround,
  SpaceEvenly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TextAlign {
  Left,
  Right,
  Center,
  Justify,
  Start,
  End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Transform {
  None,
  Upper,
  Lower,
  Capitalize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WhiteSpace {
  Normal,
  NoWrap,
  Pre,
  PreWrap,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WordBreak {
  Normal,
  BreakAll,
  BreakWord,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LineHeight {
  Normal,
  Factor(f32),
  Px(f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Fit {
  Fill,
  Contain,
  Cover,
  None,
  ScaleDown,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Repeat {
  Repeat,
  NoRepeat,
  RepeatX,
  RepeatY,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BgSize {
  Auto,
  Cover,
  Contain,
  Explicit(LenAuto, LenAuto),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Stop {
  pub color: Paint,
  pub at: Option<Len>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LinearDir {
  Angle(f32),
  Corner(f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RadialSize {
  ClosestSide,
  FarthestSide,
  ClosestCorner,
  FarthestCorner,
  Explicit(Len, Len),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Image {
  Linear { dir: LinearDir, stops: Vec<Stop> },
  Radial { circle: bool, size: RadialSize, at: (Len, Len), stops: Vec<Stop> },
  Url(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Shadow {
  pub x: f32,
  pub y: f32,
  pub blur: f32,
  pub spread: f32,
  pub color: Paint,
  pub inset: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TransformFn {
  Translate(Len, Len),
  Rotate(f32),
  Scale(f32, f32),
  Skew(f32, f32),
  Matrix([f32; 6]),
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Decoration {
  pub underline: bool,
  pub line_through: bool,
  pub color: Option<Paint>,
}

/// One element's computed style. Inherited properties come from the parent
/// when the element does not set them; the rest start at CSS's initial values.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Style {
  pub display: Display,
  pub position: Position,
  pub inset: [LenAuto; 4],
  pub width: LenAuto,
  pub height: LenAuto,
  pub min_width: LenAuto,
  pub min_height: LenAuto,
  pub max_width: LenAuto,
  pub max_height: LenAuto,
  pub margin: [LenAuto; 4],
  pub padding: [Len; 4],
  pub border_box: bool,
  pub overflow_hidden: bool,
  pub direction: Direction,
  pub wrap: Wrap,
  pub grow: f32,
  pub shrink: f32,
  pub basis: LenAuto,
  pub align_items: Option<Align>,
  pub align_self: Option<Align>,
  pub align_content: Option<Align>,
  pub justify_content: Option<Align>,
  pub row_gap: Len,
  pub column_gap: Len,
  pub border_width: [f32; 4],
  pub border_solid: [bool; 4],
  pub border_color: [Paint; 4],
  pub radius: [(Len, Len); 4],
  pub background_color: Paint,
  pub background_images: Vec<Image>,
  pub background_size: Vec<BgSize>,
  pub background_position: Vec<(Len, Len)>,
  pub background_repeat: Vec<Repeat>,
  pub box_shadow: Vec<Shadow>,
  pub opacity: f32,
  pub transform: Vec<TransformFn>,
  pub transform_origin: (Len, Len),
  pub object_fit: Fit,
  pub object_position: (Len, Len),
  pub color: Color,
  pub font_family: Vec<String>,
  pub font_size: f32,
  pub font_weight: u16,
  pub italic: bool,
  pub line_height: LineHeight,
  pub letter_spacing: f32,
  pub text_align: TextAlign,
  pub text_transform: Transform,
  pub white_space: WhiteSpace,
  pub word_break: WordBreak,
  pub text_shadow: Vec<Shadow>,
  pub text_decoration: Decoration,
  pub ellipsis: bool,
  pub line_clamp: Option<u32>,
}

const CENTER: (Len, Len) = (Len { px: 0.0, pct: 50.0 }, Len { px: 0.0, pct: 50.0 });

impl Style {
  /// The root's style before its declarations: CSS's initial values with
  /// `display: flex`, the way satori starts every element.
  pub fn root() -> Style {
    Style {
      display: Display::Flex,
      position: Position::Relative,
      inset: [LenAuto::Auto; 4],
      width: LenAuto::Auto,
      height: LenAuto::Auto,
      min_width: LenAuto::Auto,
      min_height: LenAuto::Auto,
      max_width: LenAuto::Auto,
      max_height: LenAuto::Auto,
      margin: [LenAuto::Len(Len::ZERO); 4],
      padding: [Len::ZERO; 4],
      border_box: true,
      overflow_hidden: false,
      direction: Direction::Row,
      wrap: Wrap::NoWrap,
      grow: 0.0,
      shrink: 1.0,
      basis: LenAuto::Auto,
      align_items: None,
      align_self: None,
      align_content: None,
      justify_content: None,
      row_gap: Len::ZERO,
      column_gap: Len::ZERO,
      border_width: [3.0; 4],
      border_solid: [false; 4],
      border_color: [Paint::Current; 4],
      radius: [(Len::ZERO, Len::ZERO); 4],
      background_color: Paint::Color(Color::TRANSPARENT),
      background_images: Vec::new(),
      background_size: vec![BgSize::Auto],
      background_position: vec![(Len::ZERO, Len::ZERO)],
      background_repeat: vec![Repeat::Repeat],
      box_shadow: Vec::new(),
      opacity: 1.0,
      transform: Vec::new(),
      transform_origin: CENTER,
      object_fit: Fit::Fill,
      object_position: CENTER,
      color: Color::BLACK,
      font_family: Vec::new(),
      font_size: 16.0,
      font_weight: 400,
      italic: false,
      line_height: LineHeight::Normal,
      letter_spacing: 0.0,
      text_align: TextAlign::Start,
      text_transform: Transform::None,
      white_space: WhiteSpace::Normal,
      word_break: WordBreak::Normal,
      text_shadow: Vec::new(),
      text_decoration: Decoration::default(),
      ellipsis: false,
      line_clamp: None,
    }
  }

  /// A child's style before its declarations: the parent's inherited
  /// properties over the initial values. Text decoration is carried too,
  /// since CSS draws a parent's decoration through its descendants' text.
  pub fn child_of(parent: &Style) -> Style {
    let mut style = Style::root();
    style.color = parent.color;
    style.font_family = parent.font_family.clone();
    style.font_size = parent.font_size;
    style.font_weight = parent.font_weight;
    style.italic = parent.italic;
    style.line_height = parent.line_height;
    style.letter_spacing = parent.letter_spacing;
    style.text_align = parent.text_align;
    style.text_transform = parent.text_transform;
    style.white_space = parent.white_space;
    style.word_break = parent.word_break;
    style.text_shadow = parent.text_shadow.clone();
    style.text_decoration = parent.text_decoration;
    style
  }

  pub fn border(&self, side: usize) -> f32 {
    if self.border_solid[side] { self.border_width[side] } else { 0.0 }
  }
}

/// Splits at top-level separators, outside parentheses and quotes. `' '`
/// splits on any run of whitespace.
pub(crate) fn split_top(value: &str, sep: char) -> Vec<&str> {
  let mut parts = Vec::new();
  let mut depth = 0i32;
  let mut quote: Option<char> = None;
  let mut start = 0;
  let bytes: Vec<(usize, char)> = value.char_indices().collect();
  for &(i, c) in &bytes {
    if let Some(q) = quote {
      if c == q {
        quote = None;
      }
      continue;
    }
    match c {
      '"' | '\'' => quote = Some(c),
      '(' => depth += 1,
      ')' => depth -= 1,
      _ if depth == 0 && (if sep == ' ' { c.is_whitespace() } else { c == sep }) => {
        let part = value[start..i].trim();
        if !part.is_empty() || sep != ' ' {
          parts.push(part);
        }
        start = i + c.len_utf8();
      }
      _ => {}
    }
  }
  let last = value[start..].trim();
  if !last.is_empty() || (sep != ' ' && !parts.is_empty()) {
    parts.push(last);
  }
  parts
}

/// `name(args)` when the token is one function call.
fn function(token: &str) -> Option<(String, &str)> {
  let open = token.find('(')?;
  if !token.ends_with(')') {
    return None;
  }
  let name = &token[..open];
  if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
    return None;
  }
  Some((name.to_ascii_lowercase(), &token[open + 1..token.len() - 1]))
}

fn number(token: &str) -> Option<f32> {
  let t = token.strip_prefix('+').unwrap_or(token);
  if t.is_empty() || !t.chars().next().is_some_and(|c| c.is_ascii_digit() || c == '.' || c == '-') {
    return None;
  }
  t.parse::<f32>().ok().filter(|n| n.is_finite())
}

/// Splits `12.5px` into the number and its unit.
fn dimension(token: &str) -> Option<(f32, String)> {
  let end = token.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0) || ((c == 'e' || c == 'E') && token[i + 1..].starts_with(|n: char| n.is_ascii_digit() || n == '-')))).map(|(i, _)| i).unwrap_or(token.len());
  let n = number(&token[..end])?;
  Some((n, token[end..].to_ascii_lowercase()))
}

pub(crate) fn length(token: &str, u: &Units) -> Parsed<Len> {
  if let Some((name, args)) = function(token) {
    if name == "calc" {
      return calc(args, u);
    }
    return invalid(format!("`{name}()` is not a length"));
  }
  let Some((n, unit)) = dimension(token) else {
    return invalid(format!("`{token}` is not a length"));
  };
  let px = match unit.as_str() {
    "" if n == 0.0 => 0.0,
    "" => return invalid(format!("`{token}` needs a unit")),
    "px" => n,
    "%" => return Ok(Len::pct(n)),
    "em" => n * u.em,
    "rem" => n * u.rem,
    "vw" => n * u.width / 100.0,
    "vh" => n * u.height / 100.0,
    "vmin" => n * u.width.min(u.height) / 100.0,
    "vmax" => n * u.width.max(u.height) / 100.0,
    "pt" => n * 4.0 / 3.0,
    "pc" => n * 16.0,
    "in" => n * 96.0,
    "cm" => n * 96.0 / 2.54,
    "mm" => n * 96.0 / 25.4,
    "ch" | "ex" | "cap" | "ic" | "lh" | "rlh" | "svw" | "svh" | "lvw" | "lvh" | "dvw" | "dvh" | "cqw" | "cqh" => return unsupported(format!("the `{unit}` unit is not supported")),
    _ => return invalid(format!("`{unit}` is not a length unit")),
  };
  Ok(Len::px(px))
}

fn len_auto(token: &str, u: &Units) -> Parsed<LenAuto> {
  match token.to_ascii_lowercase().as_str() {
    "auto" => Ok(LenAuto::Auto),
    "min-content" | "max-content" | "fit-content" => unsupported(format!("`{token}` is not supported")),
    _ => length(token, u).map(LenAuto::Len),
  }
}

fn max_len(token: &str, u: &Units) -> Parsed<LenAuto> {
  if token.eq_ignore_ascii_case("none") { Ok(LenAuto::Auto) } else { len_auto(token, u) }
}

#[derive(Clone, Copy)]
struct CalcValue {
  px: f32,
  pct: f32,
  num: Option<f32>,
}

/// `calc()` over lengths, percentages and plain numbers with `+`, `-`, `*`
/// and `/`. A product needs a plain number on one side.
fn calc(expr: &str, u: &Units) -> Parsed<Len> {
  let tokens = calc_tokens(expr)?;
  let mut at = 0;
  let value = calc_sum(&tokens, &mut at, u)?;
  if at != tokens.len() {
    return invalid(format!("`calc({expr})` has trailing input"));
  }
  match value.num {
    Some(0.0) => Ok(Len::ZERO),
    Some(_) => invalid(format!("`calc({expr})` is a number, not a length")),
    None => Ok(Len { px: value.px, pct: value.pct }),
  }
}

fn calc_tokens(expr: &str) -> Parsed<Vec<String>> {
  let mut tokens = Vec::new();
  let chars: Vec<char> = expr.chars().collect();
  let mut i = 0;
  while i < chars.len() {
    let c = chars[i];
    if c.is_whitespace() {
      i += 1;
    } else if "()*/".contains(c) || ((c == '+' || c == '-') && (i + 1 >= chars.len() || chars[i + 1].is_whitespace())) {
      tokens.push(c.to_string());
      i += 1;
    } else {
      let start = i;
      while i < chars.len() && !chars[i].is_whitespace() && !"()*/".contains(chars[i]) {
        i += 1;
      }
      let word: String = chars[start..i].iter().collect();
      if word.eq_ignore_ascii_case("calc") && chars.get(i) == Some(&'(') {
        continue;
      }
      tokens.push(word);
    }
  }
  if tokens.is_empty() {
    return invalid("`calc()` is empty");
  }
  Ok(tokens)
}

fn calc_sum(t: &[String], at: &mut usize, u: &Units) -> Parsed<CalcValue> {
  let mut left = calc_product(t, at, u)?;
  while *at < t.len() && (t[*at] == "+" || t[*at] == "-") {
    let sign = if t[*at] == "+" { 1.0 } else { -1.0 };
    *at += 1;
    let right = calc_product(t, at, u)?;
    left = match (left.num, right.num) {
      (Some(a), Some(b)) => CalcValue { px: 0.0, pct: 0.0, num: Some(a + sign * b) },
      (None, None) => CalcValue { px: left.px + sign * right.px, pct: left.pct + sign * right.pct, num: None },
      _ => return invalid("`calc()` adds a number to a length"),
    };
  }
  Ok(left)
}

fn calc_product(t: &[String], at: &mut usize, u: &Units) -> Parsed<CalcValue> {
  let mut left = calc_unit(t, at, u)?;
  while *at < t.len() && (t[*at] == "*" || t[*at] == "/") {
    let divide = t[*at] == "/";
    *at += 1;
    let right = calc_unit(t, at, u)?;
    left = match (left.num, right.num, divide) {
      (Some(a), Some(b), false) => CalcValue { px: 0.0, pct: 0.0, num: Some(a * b) },
      (Some(a), Some(b), true) if b != 0.0 => CalcValue { px: 0.0, pct: 0.0, num: Some(a / b) },
      (None, Some(b), false) => CalcValue { px: left.px * b, pct: left.pct * b, num: None },
      (None, Some(b), true) if b != 0.0 => CalcValue { px: left.px / b, pct: left.pct / b, num: None },
      (Some(a), None, false) => CalcValue { px: right.px * a, pct: right.pct * a, num: None },
      (_, Some(_), true) => return invalid("`calc()` divides by zero"),
      _ => return invalid("`calc()` multiplies two lengths"),
    };
  }
  Ok(left)
}

fn calc_unit(t: &[String], at: &mut usize, u: &Units) -> Parsed<CalcValue> {
  let Some(token) = t.get(*at) else {
    return invalid("`calc()` ends early");
  };
  *at += 1;
  if token == "(" {
    let inner = calc_sum(t, at, u)?;
    if t.get(*at).map(String::as_str) != Some(")") {
      return invalid("`calc()` has an unclosed parenthesis");
    }
    *at += 1;
    return Ok(inner);
  }
  if let Some(n) = number(token) {
    return Ok(CalcValue { px: 0.0, pct: 0.0, num: Some(n) });
  }
  let len = length(token, u)?;
  Ok(CalcValue { px: len.px, pct: len.pct, num: None })
}

pub(crate) fn angle(token: &str) -> Parsed<f32> {
  let Some((n, unit)) = dimension(token) else {
    return invalid(format!("`{token}` is not an angle"));
  };
  Ok(match unit.as_str() {
    "deg" => n,
    "rad" => n * 180.0 / PI,
    "turn" => n * 360.0,
    "grad" => n * 0.9,
    "" if n == 0.0 => 0.0,
    _ => return invalid(format!("`{token}` is not an angle")),
  })
}

fn channel(token: &str, scale: f32) -> Parsed<f32> {
  if let Some(p) = token.strip_suffix('%') {
    return number(p).map(|n| n / 100.0).ok_or_else(|| Bad::Invalid(format!("`{token}` is not a percentage")));
  }
  number(token).map(|n| n / scale).ok_or_else(|| Bad::Invalid(format!("`{token}` is not a number")))
}

/// The arguments of `rgb()` or `hsl()` in either syntax: commas, else spaces
/// with `/ alpha`.
fn color_args(args: &str) -> Vec<String> {
  if args.contains(',') {
    return split_top(args, ',').into_iter().map(str::to_owned).collect();
  }
  let mut out: Vec<String> = Vec::new();
  for part in split_top(args, ' ') {
    for (i, piece) in part.split('/').enumerate() {
      if i > 0 {
        out.push("/".to_owned());
      }
      if !piece.is_empty() {
        out.push(piece.to_owned());
      }
    }
  }
  out.retain(|p| p != "/");
  out
}

pub(crate) fn color(token: &str) -> Parsed<Paint> {
  let t = token.trim();
  let lower = t.to_ascii_lowercase();
  if lower == "currentcolor" {
    return Ok(Paint::Current);
  }
  if lower == "transparent" {
    return Ok(Paint::Color(Color::TRANSPARENT));
  }
  if let Some(hex) = t.strip_prefix('#') {
    return hex_color(hex).map(Paint::Color).ok_or_else(|| Bad::Invalid(format!("`{t}` is not a hex color")));
  }
  if let Some((name, args)) = function(t) {
    let parts = color_args(args);
    return match name.as_str() {
      "rgb" | "rgba" if parts.len() == 3 || parts.len() == 4 => {
        let r = channel(&parts[0], 255.0)?;
        let g = channel(&parts[1], 255.0)?;
        let b = channel(&parts[2], 255.0)?;
        let a = if parts.len() == 4 { channel(&parts[3], 1.0)? } else { 1.0 };
        Ok(Paint::Color(Color { r, g, b, a }))
      }
      "hsl" | "hsla" if parts.len() == 3 || parts.len() == 4 => {
        let h = number(&parts[0]).or_else(|| angle(&parts[0]).ok()).ok_or_else(|| Bad::Invalid(format!("`{}` is not a hue", parts[0])))?;
        let s = channel(&parts[1], 100.0)?;
        let l = channel(&parts[2], 100.0)?;
        let a = if parts.len() == 4 { channel(&parts[3], 1.0)? } else { 1.0 };
        let (r, g, b) = hsl(h, s, l);
        Ok(Paint::Color(Color { r, g, b, a }))
      }
      "hwb" | "lab" | "lch" | "oklab" | "oklch" | "color" | "color-mix" => unsupported(format!("`{name}()` colors are not supported")),
      _ => invalid(format!("`{t}` is not a color")),
    };
  }
  named(&lower).map(Paint::Color).ok_or_else(|| Bad::Invalid(format!("`{t}` is not a color")))
}

fn hex_color(hex: &str) -> Option<Color> {
  let digit = |c: u8| (c as char).to_digit(16).map(|d| d as u8);
  let b = hex.as_bytes();
  let pair = |i: usize| Some(digit(b[i])? * 16 + digit(b[i + 1])?);
  let single = |i: usize| Some(digit(b[i])? * 17);
  let (r, g, bl, a) = match b.len() {
    3 => (single(0)?, single(1)?, single(2)?, 255),
    4 => (single(0)?, single(1)?, single(2)?, single(3)?),
    6 => (pair(0)?, pair(2)?, pair(4)?, 255),
    8 => (pair(0)?, pair(2)?, pair(4)?, pair(6)?),
    _ => return None,
  };
  Some(Color { a: a as f32 / 255.0, ..Color::rgb8(r, g, bl) })
}

fn hsl(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
  let h = h.rem_euclid(360.0) / 360.0;
  let s = s.clamp(0.0, 1.0);
  let l = l.clamp(0.0, 1.0);
  let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
  let p = 2.0 * l - q;
  let hue = |mut t: f32| {
    if t < 0.0 {
      t += 1.0;
    }
    if t > 1.0 {
      t -= 1.0;
    }
    if t < 1.0 / 6.0 {
      p + (q - p) * 6.0 * t
    } else if t < 0.5 {
      q
    } else if t < 2.0 / 3.0 {
      p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
      p
    }
  };
  (hue(h + 1.0 / 3.0), hue(h), hue(h - 1.0 / 3.0))
}

/// The CSS named colors.
const NAMED: &[(&str, u32)] = &[
  ("aliceblue", 0xf0f8ff), ("antiquewhite", 0xfaebd7), ("aqua", 0x00ffff), ("aquamarine", 0x7fffd4), ("azure", 0xf0ffff), ("beige", 0xf5f5dc), ("bisque", 0xffe4c4), ("black", 0x000000), ("blanchedalmond", 0xffebcd), ("blue", 0x0000ff), ("blueviolet", 0x8a2be2), ("brown", 0xa52a2a), ("burlywood", 0xdeb887), ("cadetblue", 0x5f9ea0), ("chartreuse", 0x7fff00), ("chocolate", 0xd2691e), ("coral", 0xff7f50), ("cornflowerblue", 0x6495ed), ("cornsilk", 0xfff8dc), ("crimson", 0xdc143c), ("cyan", 0x00ffff), ("darkblue", 0x00008b), ("darkcyan", 0x008b8b), ("darkgoldenrod", 0xb8860b), ("darkgray", 0xa9a9a9), ("darkgreen", 0x006400), ("darkgrey", 0xa9a9a9), ("darkkhaki", 0xbdb76b), ("darkmagenta", 0x8b008b), ("darkolivegreen", 0x556b2f), ("darkorange", 0xff8c00), ("darkorchid", 0x9932cc), ("darkred", 0x8b0000), ("darksalmon", 0xe9967a), ("darkseagreen", 0x8fbc8f), ("darkslateblue", 0x483d8b), ("darkslategray", 0x2f4f4f), ("darkslategrey", 0x2f4f4f), ("darkturquoise", 0x00ced1), ("darkviolet", 0x9400d3), ("deeppink", 0xff1493), ("deepskyblue", 0x00bfff), ("dimgray", 0x696969), ("dimgrey", 0x696969), ("dodgerblue", 0x1e90ff), ("firebrick", 0xb22222), ("floralwhite", 0xfffaf0), ("forestgreen", 0x228b22), ("fuchsia", 0xff00ff), ("gainsboro", 0xdcdcdc), ("ghostwhite", 0xf8f8ff), ("gold", 0xffd700), ("goldenrod", 0xdaa520), ("gray", 0x808080), ("green", 0x008000), ("greenyellow", 0xadff2f), ("grey", 0x808080), ("honeydew", 0xf0fff0), ("hotpink", 0xff69b4), ("indianred", 0xcd5c5c), ("indigo", 0x4b0082), ("ivory", 0xfffff0), ("khaki", 0xf0e68c), ("lavender", 0xe6e6fa), ("lavenderblush", 0xfff0f5), ("lawngreen", 0x7cfc00), ("lemonchiffon", 0xfffacd), ("lightblue", 0xadd8e6), ("lightcoral", 0xf08080), ("lightcyan", 0xe0ffff), ("lightgoldenrodyellow", 0xfafad2), ("lightgray", 0xd3d3d3), ("lightgreen", 0x90ee90), ("lightgrey", 0xd3d3d3), ("lightpink", 0xffb6c1), ("lightsalmon", 0xffa07a), ("lightseagreen", 0x20b2aa), ("lightskyblue", 0x87cefa), ("lightslategray", 0x778899), ("lightslategrey", 0x778899), ("lightsteelblue", 0xb0c4de), ("lightyellow", 0xffffe0), ("lime", 0x00ff00), ("limegreen", 0x32cd32), ("linen", 0xfaf0e6), ("magenta", 0xff00ff), ("maroon", 0x800000), ("mediumaquamarine", 0x66cdaa), ("mediumblue", 0x0000cd), ("mediumorchid", 0xba55d3), ("mediumpurple", 0x9370db), ("mediumseagreen", 0x3cb371), ("mediumslateblue", 0x7b68ee), ("mediumspringgreen", 0x00fa9a), ("mediumturquoise", 0x48d1cc), ("mediumvioletred", 0xc71585), ("midnightblue", 0x191970), ("mintcream", 0xf5fffa), ("mistyrose", 0xffe4e1), ("moccasin", 0xffe4b5), ("navajowhite", 0xffdead), ("navy", 0x000080), ("oldlace", 0xfdf5e6), ("olive", 0x808000), ("olivedrab", 0x6b8e23), ("orange", 0xffa500), ("orangered", 0xff4500), ("orchid", 0xda70d6), ("palegoldenrod", 0xeee8aa), ("palegreen", 0x98fb98), ("paleturquoise", 0xafeeee), ("palevioletred", 0xdb7093), ("papayawhip", 0xffefd5), ("peachpuff", 0xffdab9), ("peru", 0xcd853f), ("pink", 0xffc0cb), ("plum", 0xdda0dd), ("powderblue", 0xb0e0e6), ("purple", 0x800080), ("rebeccapurple", 0x663399), ("red", 0xff0000), ("rosybrown", 0xbc8f8f), ("royalblue", 0x4169e1), ("saddlebrown", 0x8b4513), ("salmon", 0xfa8072), ("sandybrown", 0xf4a460), ("seagreen", 0x2e8b57), ("seashell", 0xfff5ee), ("sienna", 0xa0522d), ("silver", 0xc0c0c0), ("skyblue", 0x87ceeb), ("slateblue", 0x6a5acd), ("slategray", 0x708090), ("slategrey", 0x708090), ("snow", 0xfffafa), ("springgreen", 0x00ff7f), ("steelblue", 0x4682b4), ("tan", 0xd2b48c), ("teal", 0x008080), ("thistle", 0xd8bfd8), ("tomato", 0xff6347), ("turquoise", 0x40e0d0), ("violet", 0xee82ee), ("wheat", 0xf5deb3), ("white", 0xffffff), ("whitesmoke", 0xf5f5f5), ("yellow", 0xffff00), ("yellowgreen", 0x9acd32),
];

fn named(name: &str) -> Option<Color> {
  NAMED.binary_search_by(|(n, _)| n.cmp(&name)).ok().map(|i| {
    let v = NAMED[i].1;
    Color::rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
  })
}

fn keyword<T: Copy>(value: &str, supported: &[(&str, T)], left_out: &[&str]) -> Parsed<T> {
  let lower = value.to_ascii_lowercase();
  if let Some((_, v)) = supported.iter().find(|(k, _)| *k == lower) {
    return Ok(*v);
  }
  if left_out.contains(&lower.as_str()) {
    return unsupported(format!("`{value}` is not supported"));
  }
  invalid(format!("`{value}` is not a value this property takes"))
}

/// One to four values in CSS's top, right, bottom, left order.
fn sides<T: Copy>(value: &str, mut one: impl FnMut(&str) -> Parsed<T>) -> Parsed<[T; 4]> {
  let parts = split_top(value, ' ');
  let v: Vec<T> = parts.iter().map(|p| one(p)).collect::<Result<_, _>>()?;
  Ok(match v.len() {
    1 => [v[0]; 4],
    2 => [v[0], v[1], v[0], v[1]],
    3 => [v[0], v[1], v[2], v[1]],
    4 => [v[0], v[1], v[2], v[3]],
    _ => return invalid(format!("`{value}` takes one to four values")),
  })
}

fn align(value: &str, content: bool) -> Parsed<Option<Align>> {
  let lower = value.to_ascii_lowercase();
  let stripped = lower.strip_prefix("safe ").or_else(|| lower.strip_prefix("unsafe ")).unwrap_or(&lower);
  Ok(Some(match stripped {
    "normal" | "auto" => return Ok(None),
    "start" | "self-start" | "left" => Align::Start,
    "end" | "self-end" | "right" => Align::End,
    "flex-start" => Align::FlexStart,
    "flex-end" => Align::FlexEnd,
    "center" => Align::Center,
    "stretch" => Align::Stretch,
    "space-between" if content => Align::SpaceBetween,
    "space-around" if content => Align::SpaceAround,
    "space-evenly" if content => Align::SpaceEvenly,
    "baseline" | "first baseline" | "last baseline" => return unsupported("baseline alignment is not supported"),
    _ => return invalid(format!("`{value}` is not an alignment")),
  }))
}

/// A position as `background-position` or `object-position` writes it: one
/// or two keywords or lengths, horizontal first unless a keyword says otherwise.
fn position(value: &str, u: &Units) -> Parsed<(Len, Len)> {
  let parts = split_top(value, ' ');
  let axis = |t: &str| -> Parsed<(Option<bool>, Len)> {
    Ok(match t.to_ascii_lowercase().as_str() {
      "left" => (Some(true), Len::pct(0.0)),
      "right" => (Some(true), Len::pct(100.0)),
      "top" => (Some(false), Len::pct(0.0)),
      "bottom" => (Some(false), Len::pct(100.0)),
      "center" => (None, Len::pct(50.0)),
      _ => (None, length(t, u)?),
    })
  };
  match parts.as_slice() {
    [one] => {
      let (horizontal, v) = axis(one)?;
      Ok(if horizontal == Some(false) { (Len::pct(50.0), v) } else { (v, Len::pct(50.0)) })
    }
    [a, b] => {
      let (ha, va) = axis(a)?;
      let (hb, vb) = axis(b)?;
      Ok(if ha == Some(false) || hb == Some(true) { (vb, va) } else { (va, vb) })
    }
    _ => unsupported(format!("`{value}`: a position with offsets from an edge is not supported")),
  }
}

fn shadows(value: &str, u: &Units, text: bool) -> Parsed<Vec<Shadow>> {
  if value.eq_ignore_ascii_case("none") {
    return Ok(Vec::new());
  }
  let mut out = Vec::new();
  for one in split_top(value, ',') {
    let mut lengths = Vec::new();
    let mut color_value = None;
    let mut inset = false;
    for part in split_top(one, ' ') {
      if part.eq_ignore_ascii_case("inset") && !text {
        inset = true;
      } else if let Ok(len) = length(part, u) {
        lengths.push(len.px);
      } else {
        color_value = Some(color(part)?);
      }
    }
    let max = if text { 3 } else { 4 };
    if lengths.len() < 2 || lengths.len() > max {
      return invalid(format!("`{one}` needs two to {max} lengths"));
    }
    out.push(Shadow {
      x: lengths[0],
      y: lengths[1],
      blur: lengths.get(2).copied().unwrap_or(0.0).max(0.0),
      spread: lengths.get(3).copied().unwrap_or(0.0),
      color: color_value.unwrap_or(Paint::Current),
      inset,
    });
  }
  Ok(out)
}

fn stops(parts: &[&str], u: &Units) -> Parsed<Vec<Stop>> {
  let mut out = Vec::new();
  for part in parts {
    let words = split_top(part, ' ');
    let Some(first) = words.first() else {
      return invalid("a gradient has an empty color stop");
    };
    if words.len() == 1 && length(first, u).is_ok() {
      return unsupported("color hints in a gradient are not supported");
    }
    let color_value = color(first)?;
    let positions: Vec<Len> = words[1..].iter().map(|w| length(w, u)).collect::<Result<_, _>>()?;
    match positions.as_slice() {
      [] => out.push(Stop { color: color_value, at: None }),
      [at] => out.push(Stop { color: color_value, at: Some(*at) }),
      [a, b] => {
        out.push(Stop { color: color_value, at: Some(*a) });
        out.push(Stop { color: color_value, at: Some(*b) });
      }
      _ => return invalid(format!("`{part}` has more than two positions")),
    }
  }
  if out.len() < 2 {
    return invalid("a gradient needs two color stops");
  }
  Ok(out)
}

fn image(token: &str, u: &Units) -> Parsed<Image> {
  let Some((name, args)) = function(token) else {
    return invalid(format!("`{token}` is not an image"));
  };
  let parts = split_top(args, ',');
  match name.as_str() {
    "url" => {
      let key = args.trim().trim_matches(|c| c == '"' || c == '\'');
      Ok(Image::Url(key.to_owned()))
    }
    "linear-gradient" => {
      let first = parts.first().copied().unwrap_or("");
      let lower = first.to_ascii_lowercase();
      let (dir, rest) = if let Some(to) = lower.strip_prefix("to ") {
        let mut x = 0.0;
        let mut y = 0.0;
        for w in to.split_whitespace() {
          match w {
            "left" => x = -1.0,
            "right" => x = 1.0,
            "top" => y = -1.0,
            "bottom" => y = 1.0,
            _ => return invalid(format!("`{first}` is not a gradient direction")),
          }
        }
        let dir = match (x, y) {
          (0.0, 0.0) => return invalid(format!("`{first}` is not a gradient direction")),
          (0.0, y) => LinearDir::Angle(if y < 0.0 { 0.0 } else { 180.0 }),
          (x, 0.0) => LinearDir::Angle(if x > 0.0 { 90.0 } else { 270.0 }),
          (x, y) => LinearDir::Corner(x, y),
        };
        (dir, &parts[1..])
      } else if let Ok(a) = angle(first) {
        (LinearDir::Angle(a), &parts[1..])
      } else {
        (LinearDir::Angle(180.0), &parts[..])
      };
      Ok(Image::Linear { dir, stops: stops(rest, u)? })
    }
    "radial-gradient" => {
      let first = parts.first().copied().unwrap_or("");
      let words = split_top(first, ' ');
      let starts_with_color = words.first().is_some_and(|w| color(w).is_ok());
      let mut circle = false;
      let mut size = RadialSize::FarthestCorner;
      let mut at = (Len::pct(50.0), Len::pct(50.0));
      let mut lens = Vec::new();
      if !starts_with_color {
        let mut i = 0;
        while i < words.len() {
          match words[i].to_ascii_lowercase().as_str() {
            "circle" => circle = true,
            "ellipse" => circle = false,
            "closest-side" => size = RadialSize::ClosestSide,
            "farthest-side" => size = RadialSize::FarthestSide,
            "closest-corner" => size = RadialSize::ClosestCorner,
            "farthest-corner" => size = RadialSize::FarthestCorner,
            "at" => {
              at = position(&words[i + 1..].join(" "), u)?;
              break;
            }
            w => lens.push(length(w, u).map_err(|_| Bad::Invalid(format!("`{w}` is not a radial gradient shape")))?),
          }
          i += 1;
        }
        match lens.as_slice() {
          [] => {}
          [r] => {
            circle = true;
            size = RadialSize::Explicit(*r, *r);
          }
          [rx, ry] => size = RadialSize::Explicit(*rx, *ry),
          _ => return invalid(format!("`{first}` has too many sizes")),
        }
      }
      let rest = if starts_with_color { &parts[..] } else { &parts[1..] };
      Ok(Image::Radial { circle, size, at, stops: stops(rest, u)? })
    }
    "repeating-linear-gradient" | "repeating-radial-gradient" | "conic-gradient" | "repeating-conic-gradient" | "image-set" | "cross-fade" | "element" => unsupported(format!("`{name}()` is not supported")),
    _ => invalid(format!("`{token}` is not an image")),
  }
}

fn transforms(value: &str, u: &Units) -> Parsed<Vec<TransformFn>> {
  if value.eq_ignore_ascii_case("none") {
    return Ok(Vec::new());
  }
  let mut out = Vec::new();
  for part in split_top(value, ' ') {
    let Some((name, args)) = function(part) else {
      return invalid(format!("`{part}` is not a transform function"));
    };
    let a: Vec<&str> = split_top(args, ',');
    let num = |i: usize| -> Parsed<f32> { a.get(i).and_then(|t| number(t)).ok_or_else(|| Bad::Invalid(format!("`{part}` needs a number"))) };
    let len = |i: usize| -> Parsed<Len> { a.get(i).map(|t| length(t, u)).unwrap_or(Ok(Len::ZERO)) };
    let ang = |i: usize| -> Parsed<f32> { a.get(i).map(|t| angle(t)).unwrap_or(Ok(0.0)) };
    out.push(match name.as_str() {
      "translate" => TransformFn::Translate(len(0)?, len(1)?),
      "translatex" => TransformFn::Translate(len(0)?, Len::ZERO),
      "translatey" => TransformFn::Translate(Len::ZERO, len(0)?),
      "rotate" | "rotatez" => TransformFn::Rotate(ang(0)?),
      "scale" => {
        let x = num(0)?;
        TransformFn::Scale(x, if a.len() > 1 { num(1)? } else { x })
      }
      "scalex" => TransformFn::Scale(num(0)?, 1.0),
      "scaley" => TransformFn::Scale(1.0, num(0)?),
      "skew" => TransformFn::Skew(ang(0)?, ang(1)?),
      "skewx" => TransformFn::Skew(ang(0)?, 0.0),
      "skewy" => TransformFn::Skew(0.0, ang(0)?),
      "matrix" if a.len() == 6 => TransformFn::Matrix([num(0)?, num(1)?, num(2)?, num(3)?, num(4)?, num(5)?]),
      "translate3d" | "translatez" | "scale3d" | "scalez" | "rotate3d" | "rotatex" | "rotatey" | "perspective" | "matrix3d" => return unsupported(format!("`{name}()` is a 3D transform")),
      _ => return invalid(format!("`{part}` is not a transform function")),
    });
  }
  Ok(out)
}

fn font_family(value: &str) -> Vec<String> {
  split_top(value, ',').into_iter().map(|f| f.trim().trim_matches(|c| c == '"' || c == '\'').to_owned()).filter(|f| !f.is_empty()).collect()
}

fn font_weight(value: &str, parent: u16) -> Parsed<u16> {
  Ok(match value.to_ascii_lowercase().as_str() {
    "normal" => 400,
    "bold" => 700,
    "bolder" => match parent {
      0..=349 => 400,
      350..=549 => 700,
      _ => 900,
    },
    "lighter" => match parent {
      0..=549 => 100,
      550..=749 => 400,
      _ => 700,
    },
    v => match number(v) {
      Some(n) if (1.0..=1000.0).contains(&n) => n.round() as u16,
      _ => return invalid(format!("`{value}` is not a font weight")),
    },
  })
}

fn border_parts(value: &str, u: &Units) -> Parsed<(Option<f32>, Option<bool>, Option<Paint>)> {
  let mut width = None;
  let mut solid = None;
  let mut paint = None;
  for part in split_top(value, ' ') {
    let lower = part.to_ascii_lowercase();
    match lower.as_str() {
      "none" | "hidden" => solid = Some(false),
      "solid" => solid = Some(true),
      "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset" => return unsupported(format!("`border-style: {part}` is not supported")),
      "thin" => width = Some(1.0),
      "medium" => width = Some(3.0),
      "thick" => width = Some(5.0),
      _ => {
        if let Ok(len) = length(part, u) {
          width = Some(len.px.max(0.0));
        } else {
          paint = Some(color(part)?);
        }
      }
    }
  }
  Ok((width, solid, paint))
}

fn border_width(token: &str, u: &Units) -> Parsed<f32> {
  match token.to_ascii_lowercase().as_str() {
    "thin" => Ok(1.0),
    "medium" => Ok(3.0),
    "thick" => Ok(5.0),
    _ => Ok(length(token, u)?.px.max(0.0)),
  }
}

fn border_style(token: &str) -> Parsed<bool> {
  keyword(token, &[("solid", true), ("none", false), ("hidden", false)], &["dashed", "dotted", "double", "groove", "ridge", "inset", "outset"])
}

fn radius_pair(value: &str, u: &Units) -> Parsed<(Len, Len)> {
  let parts = split_top(value, ' ');
  match parts.as_slice() {
    [r] => {
      let r = length(r, u)?;
      Ok((r, r))
    }
    [x, y] => Ok((length(x, u)?, length(y, u)?)),
    _ => invalid(format!("`{value}` is not a corner radius")),
  }
}

fn line_height(value: &str, u: &Units) -> Parsed<LineHeight> {
  if value.eq_ignore_ascii_case("normal") {
    return Ok(LineHeight::Normal);
  }
  if let Some(n) = number(value) {
    return Ok(LineHeight::Factor(n));
  }
  let len = length(value, u)?;
  Ok(if len.pct != 0.0 { LineHeight::Factor(len.pct / 100.0 + len.px / u.em) } else { LineHeight::Px(len.px) })
}

/// A list property such as `background-size`: one entry per comma.
fn list<T>(value: &str, one: impl Fn(&str) -> Parsed<T>) -> Parsed<Vec<T>> {
  split_top(value, ',').into_iter().map(one).collect()
}

fn bg_size(token: &str, u: &Units) -> Parsed<BgSize> {
  match token.to_ascii_lowercase().as_str() {
    "cover" => Ok(BgSize::Cover),
    "contain" => Ok(BgSize::Contain),
    "auto" => Ok(BgSize::Auto),
    _ => {
      let parts = split_top(token, ' ');
      match parts.as_slice() {
        [w] => Ok(BgSize::Explicit(len_auto(w, u)?, LenAuto::Auto)),
        [w, h] => Ok(BgSize::Explicit(len_auto(w, u)?, len_auto(h, u)?)),
        _ => invalid(format!("`{token}` is not a background size")),
      }
    }
  }
}

fn bg_repeat(token: &str) -> Parsed<Repeat> {
  let lower = token.to_ascii_lowercase();
  match lower.as_str() {
    "repeat" | "repeat repeat" => Ok(Repeat::Repeat),
    "no-repeat" | "no-repeat no-repeat" => Ok(Repeat::NoRepeat),
    "repeat-x" | "repeat no-repeat" => Ok(Repeat::RepeatX),
    "repeat-y" | "no-repeat repeat" => Ok(Repeat::RepeatY),
    "space" | "round" => unsupported(format!("`background-repeat: {token}` is not supported")),
    _ => invalid(format!("`{token}` is not a background repeat")),
  }
}

/// The properties a card reads. Anything else warns.
pub(crate) const PROPERTIES: &[&str] = &[
  "align-content", "align-items", "align-self", "background", "background-color", "background-image", "background-position", "background-repeat", "background-size", "border", "border-bottom", "border-bottom-color", "border-bottom-left-radius", "border-bottom-right-radius", "border-bottom-style", "border-bottom-width", "border-color", "border-left", "border-left-color", "border-left-style", "border-left-width", "border-radius", "border-right", "border-right-color", "border-right-style", "border-right-width", "border-style", "border-top", "border-top-color", "border-top-left-radius", "border-top-right-radius", "border-top-style", "border-top-width", "border-width", "bottom", "box-shadow", "box-sizing", "color", "column-gap", "display", "flex", "flex-basis", "flex-direction", "flex-flow", "flex-grow", "flex-shrink", "flex-wrap", "font-family", "font-size", "font-style", "font-weight", "gap", "height", "inset", "justify-content", "left", "letter-spacing", "line-clamp", "line-height", "margin", "margin-bottom", "margin-left", "margin-right", "margin-top", "max-height", "max-width", "min-height", "min-width", "object-fit", "object-position", "opacity", "overflow", "overflow-x", "overflow-y", "padding", "padding-bottom", "padding-left", "padding-right", "padding-top", "position", "right", "row-gap", "text-align", "text-decoration", "text-decoration-color", "text-decoration-line", "text-overflow", "text-shadow", "text-transform", "top", "transform", "transform-origin", "white-space", "width", "word-break", "overflow-wrap", "word-wrap", "-webkit-line-clamp", "-webkit-box-orient",
];

/// Whether a property is inherited, for `inherit`, `initial` and `unset`.
fn inherited(name: &str) -> bool {
  matches!(name, "color" | "font-family" | "font-size" | "font-style" | "font-weight" | "letter-spacing" | "line-height" | "text-align" | "text-shadow" | "text-transform" | "white-space" | "word-break" | "overflow-wrap" | "word-wrap")
}

/// Applies the declarations in order to `style`, which starts as
/// `Style::child_of(parent)`. `font-size` is read first, since `em` in every
/// other declaration means the element's own font size.
pub(crate) fn compute(decls: &[(String, String)], parent: &Style, root_font: f32, card: (f32, f32), on_bad: &mut dyn FnMut(&str, &str, Bad)) -> Style {
  let mut style = Style::child_of(parent);
  let parent_units = Units { em: parent.font_size, rem: root_font, width: card.0, height: card.1 };
  for (name, value) in decls.iter().rev() {
    if name.trim().eq_ignore_ascii_case("font-size") {
      match font_size(value.trim(), parent, &parent_units) {
        Ok(Some(size)) => style.font_size = size,
        Ok(None) => {}
        Err(bad) => on_bad("font-size", value, bad),
      }
      break;
    }
  }
  let units = Units { em: style.font_size, rem: root_font, width: card.0, height: card.1 };
  for (name, value) in decls {
    let name = name.trim().to_ascii_lowercase();
    let value = value.trim().trim_end_matches("!important").trim();
    if name == "font-size" || name.starts_with("--") {
      continue;
    }
    if !PROPERTIES.contains(&name.as_str()) {
      on_bad(&name, value, Bad::Unsupported(format!("`{name}` is not supported")));
      continue;
    }
    let global = value.to_ascii_lowercase();
    if matches!(global.as_str(), "inherit" | "initial" | "unset" | "revert" | "revert-layer") {
      let from = if global == "inherit" || (global != "initial" && inherited(&name)) { parent.clone() } else { Style::root() };
      copy(&name, &from, &mut style);
      continue;
    }
    if let Err(bad) = apply(&name, value, &mut style, parent, &units) {
      on_bad(&name, value, bad);
    }
  }
  style
}

fn font_size(value: &str, parent: &Style, u: &Units) -> Parsed<Option<f32>> {
  let lower = value.to_ascii_lowercase();
  if matches!(lower.as_str(), "inherit" | "unset") {
    return Ok(None);
  }
  let keyword = match lower.as_str() {
    "initial" | "medium" => Some(16.0),
    "xx-small" => Some(9.0),
    "x-small" => Some(10.0),
    "small" => Some(13.0),
    "large" => Some(18.0),
    "x-large" => Some(24.0),
    "xx-large" => Some(32.0),
    "xxx-large" => Some(48.0),
    "smaller" => Some(parent.font_size / 1.2),
    "larger" => Some(parent.font_size * 1.2),
    _ => None,
  };
  if let Some(k) = keyword {
    return Ok(Some(k));
  }
  let len = length(value, u)?;
  Ok(Some((len.px + len.pct / 100.0 * parent.font_size).max(0.0)))
}

fn copy(name: &str, from: &Style, to: &mut Style) {
  match name {
    "color" => to.color = from.color,
    "font-family" => to.font_family = from.font_family.clone(),
    "font-style" => to.italic = from.italic,
    "font-weight" => to.font_weight = from.font_weight,
    "letter-spacing" => to.letter_spacing = from.letter_spacing,
    "line-height" => to.line_height = from.line_height,
    "text-align" => to.text_align = from.text_align,
    "text-shadow" => to.text_shadow = from.text_shadow.clone(),
    "text-transform" => to.text_transform = from.text_transform,
    "white-space" => to.white_space = from.white_space,
    "word-break" | "overflow-wrap" | "word-wrap" => to.word_break = from.word_break,
    "display" => to.display = from.display,
    "position" => to.position = from.position,
    "width" => to.width = from.width,
    "height" => to.height = from.height,
    "opacity" => to.opacity = from.opacity,
    "background-color" => to.background_color = from.background_color,
    "background" | "background-image" => {
      to.background_color = from.background_color;
      to.background_images = from.background_images.clone();
    }
    "box-shadow" => to.box_shadow = from.box_shadow.clone(),
    "transform" => to.transform = from.transform.clone(),
    _ => {}
  }
}

fn apply(name: &str, value: &str, s: &mut Style, parent: &Style, u: &Units) -> Parsed<()> {
  const SIDES: [&str; 4] = ["top", "right", "bottom", "left"];
  let side = |n: &str| SIDES.iter().position(|s| *s == n);
  match name {
    "display" => {
      s.display = keyword(value, &[("flex", Display::Flex), ("none", Display::None)], &["block", "inline", "inline-block", "inline-flex", "grid", "inline-grid", "contents", "table", "table-row", "table-cell", "list-item", "flow-root"])?;
    }
    "position" => {
      s.position = keyword(value, &[("relative", Position::Relative), ("absolute", Position::Absolute), ("static", Position::Relative)], &["fixed", "sticky"])?;
    }
    "top" | "right" | "bottom" | "left" => s.inset[side(name).unwrap()] = len_auto(value, u)?,
    "inset" => s.inset = sides(value, |t| len_auto(t, u))?,
    "width" => s.width = len_auto(value, u)?,
    "height" => s.height = len_auto(value, u)?,
    "min-width" => s.min_width = len_auto(value, u)?,
    "min-height" => s.min_height = len_auto(value, u)?,
    "max-width" => s.max_width = max_len(value, u)?,
    "max-height" => s.max_height = max_len(value, u)?,
    "margin" => s.margin = sides(value, |t| len_auto(t, u))?,
    "padding" => s.padding = sides(value, |t| length(t, u))?,
    _ if name.starts_with("margin-") => s.margin[side(&name[7..]).ok_or_else(|| Bad::Unsupported(format!("`{name}` is not supported")))?] = len_auto(value, u)?,
    _ if name.starts_with("padding-") => s.padding[side(&name[8..]).ok_or_else(|| Bad::Unsupported(format!("`{name}` is not supported")))?] = length(value, u)?,
    "box-sizing" => s.border_box = keyword(value, &[("border-box", true), ("content-box", false)], &[])?,
    "overflow" | "overflow-x" | "overflow-y" => {
      let words = split_top(value, ' ');
      let mut hidden = false;
      for w in words {
        hidden |= keyword(w, &[("visible", false), ("hidden", true), ("clip", true)], &["scroll", "auto"])?;
      }
      s.overflow_hidden = hidden;
    }
    "flex-direction" => s.direction = keyword(value, &[("row", Direction::Row), ("row-reverse", Direction::RowReverse), ("column", Direction::Column), ("column-reverse", Direction::ColumnReverse)], &[])?,
    "flex-wrap" => s.wrap = keyword(value, &[("nowrap", Wrap::NoWrap), ("wrap", Wrap::Wrap), ("wrap-reverse", Wrap::WrapReverse)], &[])?,
    "flex-flow" => {
      for w in split_top(value, ' ') {
        if let Ok(d) = keyword(w, &[("row", Direction::Row), ("row-reverse", Direction::RowReverse), ("column", Direction::Column), ("column-reverse", Direction::ColumnReverse)], &[]) {
          s.direction = d;
        } else {
          s.wrap = keyword(w, &[("nowrap", Wrap::NoWrap), ("wrap", Wrap::Wrap), ("wrap-reverse", Wrap::WrapReverse)], &[])?;
        }
      }
    }
    "flex" => {
      let lower = value.to_ascii_lowercase();
      let (g, sh, b) = match lower.as_str() {
        "none" => (0.0, 0.0, LenAuto::Auto),
        "auto" => (1.0, 1.0, LenAuto::Auto),
        "initial" => (0.0, 1.0, LenAuto::Auto),
        _ => {
          let parts = split_top(value, ' ');
          let mut nums = Vec::new();
          let mut basis = None;
          for p in &parts {
            if let Some(n) = number(p) {
              if nums.len() < 2 && basis.is_none() {
                nums.push(n);
                continue;
              }
            }
            basis = Some(len_auto(p, u)?);
          }
          if nums.is_empty() && basis.is_none() {
            return invalid(format!("`{value}` is not a flex value"));
          }
          (nums.first().copied().unwrap_or(1.0), nums.get(1).copied().unwrap_or(1.0), basis.unwrap_or(if nums.is_empty() { LenAuto::Auto } else { LenAuto::Len(Len::ZERO) }))
        }
      };
      s.grow = g;
      s.shrink = sh;
      s.basis = b;
    }
    "flex-grow" => s.grow = number(value).filter(|n| *n >= 0.0).ok_or_else(|| Bad::Invalid(format!("`{value}` is not a flex factor")))?,
    "flex-shrink" => s.shrink = number(value).filter(|n| *n >= 0.0).ok_or_else(|| Bad::Invalid(format!("`{value}` is not a flex factor")))?,
    "flex-basis" => s.basis = if value.eq_ignore_ascii_case("content") { LenAuto::Auto } else { len_auto(value, u)? },
    "align-items" => s.align_items = align(value, false)?,
    "align-self" => s.align_self = align(value, false)?,
    "align-content" => s.align_content = align(value, true)?,
    "justify-content" => s.justify_content = align(value, true)?,
    "gap" => {
      let parts = split_top(value, ' ');
      match parts.as_slice() {
        [g] => {
          let g = length(g, u)?;
          s.row_gap = g;
          s.column_gap = g;
        }
        [r, c] => {
          s.row_gap = length(r, u)?;
          s.column_gap = length(c, u)?;
        }
        _ => return invalid(format!("`{value}` is not a gap")),
      }
    }
    "row-gap" => s.row_gap = length(value, u)?,
    "column-gap" => s.column_gap = length(value, u)?,
    "border" | "border-top" | "border-right" | "border-bottom" | "border-left" => {
      let (w, st, p) = border_parts(value, u)?;
      let which: Vec<usize> = if name == "border" { vec![0, 1, 2, 3] } else { vec![side(&name[7..]).unwrap()] };
      for i in which {
        s.border_width[i] = w.unwrap_or(3.0);
        s.border_solid[i] = st.unwrap_or(false);
        s.border_color[i] = p.unwrap_or(Paint::Current);
      }
    }
    "border-width" => s.border_width = sides(value, |t| border_width(t, u))?,
    "border-style" => s.border_solid = sides(value, border_style)?,
    "border-color" => s.border_color = sides(value, color)?,
    "border-radius" => {
      let (h, v) = match value.split_once('/') {
        Some((h, v)) => (h, Some(v)),
        None => (value, None),
      };
      let hs = sides(h, |t| length(t, u))?;
      let vs = match v {
        Some(v) => sides(v, |t| length(t, u))?,
        None => hs,
      };
      for i in 0..4 {
        s.radius[i] = (hs[i], vs[i]);
      }
    }
    "border-top-left-radius" => s.radius[0] = radius_pair(value, u)?,
    "border-top-right-radius" => s.radius[1] = radius_pair(value, u)?,
    "border-bottom-right-radius" => s.radius[2] = radius_pair(value, u)?,
    "border-bottom-left-radius" => s.radius[3] = radius_pair(value, u)?,
    _ if name.starts_with("border-") => {
      let rest = &name[7..];
      let (which, prop) = rest.split_once('-').ok_or_else(|| Bad::Unsupported(format!("`{name}` is not supported")))?;
      let i = side(which).ok_or_else(|| Bad::Unsupported(format!("`{name}` is not supported")))?;
      match prop {
        "width" => s.border_width[i] = border_width(value, u)?,
        "style" => s.border_solid[i] = border_style(value)?,
        "color" => s.border_color[i] = color(value)?,
        _ => return unsupported(format!("`{name}` is not supported")),
      }
    }
    "background-color" => s.background_color = color(value)?,
    "background-image" => {
      s.background_images = if value.eq_ignore_ascii_case("none") { Vec::new() } else { list(value, |t| image(t, u))? };
    }
    "background" => {
      if value.eq_ignore_ascii_case("none") {
        s.background_color = Paint::Color(Color::TRANSPARENT);
        s.background_images.clear();
      } else if let Ok(c) = color(value) {
        s.background_color = c;
        s.background_images.clear();
      } else {
        let layers = split_top(value, ',');
        let mut images = Vec::new();
        for (i, layer) in layers.iter().enumerate() {
          let words = split_top(layer, ' ');
          let (img, rest): (Vec<&str>, Vec<&str>) = words.into_iter().partition(|w| function(w).is_some_and(|(n, _)| n != "rgb" && n != "rgba" && n != "hsl" && n != "hsla"));
          match (img.as_slice(), rest.as_slice()) {
            ([one], []) => images.push(image(one, u)?),
            ([], [c]) if i == layers.len() - 1 => s.background_color = color(c)?,
            ([one], [c]) if i == layers.len() - 1 && color(c).is_ok() => {
              images.push(image(one, u)?);
              s.background_color = color(c)?;
            }
            _ => return unsupported(format!("`background: {layer}` sets more than an image and a color; use the longhand properties")),
          }
        }
        s.background_images = images;
      }
    }
    "background-size" => s.background_size = list(value, |t| bg_size(t, u))?,
    "background-position" => s.background_position = list(value, |t| position(t, u))?,
    "background-repeat" => s.background_repeat = list(value, bg_repeat)?,
    "box-shadow" => s.box_shadow = shadows(value, u, false)?,
    "text-shadow" => s.text_shadow = shadows(value, u, true)?,
    "opacity" => {
      s.opacity = if let Some(p) = value.strip_suffix('%') { number(p).map(|n| n / 100.0) } else { number(value) }.ok_or_else(|| Bad::Invalid(format!("`{value}` is not an opacity")))?.clamp(0.0, 1.0);
    }
    "transform" => s.transform = transforms(value, u)?,
    "transform-origin" => s.transform_origin = position(value, u)?,
    "object-fit" => s.object_fit = keyword(value, &[("fill", Fit::Fill), ("contain", Fit::Contain), ("cover", Fit::Cover), ("none", Fit::None), ("scale-down", Fit::ScaleDown)], &[])?,
    "object-position" => s.object_position = position(value, u)?,
    "color" => s.color = color(value)?.resolve(parent.color),
    "font-family" => s.font_family = font_family(value),
    "font-weight" => s.font_weight = font_weight(value, parent.font_weight)?,
    "font-style" => {
      let lower = value.to_ascii_lowercase();
      s.italic = if lower == "normal" { false } else if lower == "italic" || lower.starts_with("oblique") { true } else { return invalid(format!("`{value}` is not a font style")) };
    }
    "line-height" => s.line_height = line_height(value, u)?,
    "letter-spacing" => s.letter_spacing = if value.eq_ignore_ascii_case("normal") { 0.0 } else { length(value, u)?.px },
    "text-align" => s.text_align = keyword(value, &[("left", TextAlign::Left), ("right", TextAlign::Right), ("center", TextAlign::Center), ("justify", TextAlign::Justify), ("start", TextAlign::Start), ("end", TextAlign::End)], &["match-parent", "justify-all"])?,
    "text-transform" => s.text_transform = keyword(value, &[("none", Transform::None), ("uppercase", Transform::Upper), ("lowercase", Transform::Lower), ("capitalize", Transform::Capitalize)], &["full-width", "full-size-kana"])?,
    "white-space" => s.white_space = keyword(value, &[("normal", WhiteSpace::Normal), ("nowrap", WhiteSpace::NoWrap), ("pre", WhiteSpace::Pre), ("pre-wrap", WhiteSpace::PreWrap), ("pre-line", WhiteSpace::PreWrap), ("break-spaces", WhiteSpace::PreWrap)], &[])?,
    "word-break" => s.word_break = keyword(value, &[("normal", WordBreak::Normal), ("break-all", WordBreak::BreakAll), ("break-word", WordBreak::BreakWord), ("keep-all", WordBreak::Normal)], &[])?,
    "overflow-wrap" | "word-wrap" => {
      let wrap = keyword(value, &[("normal", false), ("break-word", true), ("anywhere", true)], &[])?;
      if wrap {
        s.word_break = WordBreak::BreakWord;
      } else if s.word_break == WordBreak::BreakWord {
        s.word_break = WordBreak::Normal;
      }
    }
    "text-overflow" => s.ellipsis = keyword(value, &[("clip", false), ("ellipsis", true)], &[])?,
    "line-clamp" | "-webkit-line-clamp" => {
      s.line_clamp = if value.eq_ignore_ascii_case("none") { None } else { Some(number(value).filter(|n| *n >= 1.0).ok_or_else(|| Bad::Invalid(format!("`{value}` is not a line count")))? as u32) };
    }
    "-webkit-box-orient" => {}
    "text-decoration" | "text-decoration-line" => {
      let mut d = Decoration { color: if name == "text-decoration" { None } else { s.text_decoration.color }, ..Decoration::default() };
      for w in split_top(value, ' ') {
        match w.to_ascii_lowercase().as_str() {
          "none" => {}
          "underline" => d.underline = true,
          "line-through" => d.line_through = true,
          "overline" | "blink" => return unsupported(format!("`text-decoration: {w}` is not supported")),
          "solid" => {}
          "double" | "dotted" | "dashed" | "wavy" => return unsupported(format!("`text-decoration-style: {w}` is not supported")),
          _ if name == "text-decoration" => d.color = Some(color(w)?),
          _ => return invalid(format!("`{w}` is not a text decoration")),
        }
      }
      s.text_decoration = d;
    }
    "text-decoration-color" => s.text_decoration.color = Some(color(value)?),
    _ => return unsupported(format!("`{name}` is not supported")),
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const U: Units = Units { em: 16.0, rem: 16.0, width: 1200.0, height: 630.0 };

  fn computed(decls: &[(&str, &str)]) -> (Style, Vec<(String, Bad)>) {
    let decls: Vec<(String, String)> = decls.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect();
    let mut bad = Vec::new();
    let style = compute(&decls, &Style::root(), 16.0, (1200.0, 630.0), &mut |n, _, b| bad.push((n.to_owned(), b)));
    (style, bad)
  }

  #[test]
  fn named_colors_are_sorted_for_the_binary_search() {
    assert!(NAMED.windows(2).all(|w| w[0].0 < w[1].0));
  }

  #[test]
  fn colors_parse_in_each_syntax() {
    let rgb = |s: &str| match color(s).unwrap() {
      Paint::Color(c) => ((c.r * 255.0).round() as u8, (c.g * 255.0).round() as u8, (c.b * 255.0).round() as u8, (c.a * 100.0).round() as u8),
      Paint::Current => panic!("current"),
    };
    assert_eq!(rgb("#f97316"), (249, 115, 22, 100));
    assert_eq!(rgb("#fff8"), (255, 255, 255, 53));
    assert_eq!(rgb("rgb(1, 2, 3)"), (1, 2, 3, 100));
    assert_eq!(rgb("rgba(0,0,0,0.5)"), (0, 0, 0, 50));
    assert_eq!(rgb("rgb(0 0 0 / 25%)"), (0, 0, 0, 25));
    assert_eq!(rgb("hsl(0, 100%, 50%)"), (255, 0, 0, 100));
    assert_eq!(rgb("RebeccaPurple"), (102, 51, 153, 100));
    assert!(matches!(color("notacolor"), Err(Bad::Invalid(_))));
    assert!(matches!(color("oklch(0.7 0.1 200)"), Err(Bad::Unsupported(_))));
  }

  #[test]
  fn lengths_resolve_their_units() {
    assert_eq!(length("12px", &U).unwrap(), Len::px(12.0));
    assert_eq!(length("2em", &U).unwrap(), Len::px(32.0));
    assert_eq!(length("10vw", &U).unwrap(), Len::px(120.0));
    assert_eq!(length("50%", &U).unwrap(), Len::pct(50.0));
    assert_eq!(length("calc(100% - 2 * 16px)", &U).unwrap(), Len { px: -32.0, pct: 100.0 });
    assert_eq!(length("calc((1rem + 4px) / 2)", &U).unwrap(), Len::px(10.0));
    assert!(matches!(length("12", &U), Err(Bad::Invalid(_))));
    assert!(matches!(length("3ch", &U), Err(Bad::Unsupported(_))));
  }

  #[test]
  fn em_reads_the_elements_own_font_size_wherever_it_is_declared() {
    let (style, bad) = computed(&[("padding", "1em"), ("font-size", "20px")]);
    assert!(bad.is_empty());
    assert_eq!(style.padding[0], Len::px(20.0));
  }

  #[test]
  fn an_unknown_property_and_a_left_out_value_warn_and_a_bad_value_fails() {
    let (_, bad) = computed(&[("foo-bar", "1px"), ("display", "grid"), ("color", "notacolor")]);
    assert!(matches!(&bad[0], (n, Bad::Unsupported(_)) if n == "foo-bar"));
    assert!(matches!(&bad[1], (n, Bad::Unsupported(_)) if n == "display"));
    assert!(matches!(&bad[2], (n, Bad::Invalid(_)) if n == "color"));
  }

  #[test]
  fn shorthands_expand_in_css_order() {
    let (s, bad) = computed(&[("margin", "1px 2px 3px"), ("border", "2px solid red"), ("border-radius", "4px 8px / 2px"), ("flex", "2 1 10px")]);
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!(s.margin, [LenAuto::Len(Len::px(1.0)), LenAuto::Len(Len::px(2.0)), LenAuto::Len(Len::px(3.0)), LenAuto::Len(Len::px(2.0))]);
    assert_eq!(s.border(3), 2.0);
    assert_eq!(s.radius[1], (Len::px(8.0), Len::px(2.0)));
    assert_eq!((s.grow, s.shrink, s.basis), (2.0, 1.0, LenAuto::Len(Len::px(10.0))));
  }

  #[test]
  fn gradients_and_shadows_parse() {
    let (s, bad) = computed(&[("background-image", "linear-gradient(to right, #000 0%, rgba(0,0,0,0) 100%), radial-gradient(circle at top left, red, blue)"), ("box-shadow", "0 10px 30px rgba(0,0,0,0.2), inset 0 0 0 2px red")]);
    assert!(bad.is_empty(), "{bad:?}");
    assert_eq!(s.background_images.len(), 2);
    assert!(matches!(s.background_images[0], Image::Linear { dir: LinearDir::Angle(a), .. } if a == 90.0));
    assert!(matches!(s.background_images[1], Image::Radial { circle: true, at: (x, y), .. } if x == Len::pct(0.0) && y == Len::pct(0.0)));
    assert_eq!(s.box_shadow.len(), 2);
    assert!(s.box_shadow[1].inset);
  }

  #[test]
  fn inherited_properties_reach_the_child_and_others_reset() {
    let (parent, _) = computed(&[("color", "red"), ("font-weight", "700"), ("padding", "4px")]);
    let decls = vec![("border-color".to_owned(), "currentColor".to_owned())];
    let child = compute(&decls, &parent, 16.0, (1200.0, 630.0), &mut |_, _, _| {});
    assert_eq!(child.color, parent.color);
    assert_eq!(child.font_weight, 700);
    assert_eq!(child.padding[0], Len::ZERO);
    assert_eq!(child.border_color[0].resolve(child.color), parent.color);
  }
}
