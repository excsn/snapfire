//! Ink's `Box` and `Text` laid out with flexbox and drawn to a terminal as cells.
//!
//! [`render`] lays a [`Node`] tree out across a number of columns the way Ink
//! lays its root out at the terminal's width. It returns a [`Frame`]: a grid
//! of styled cells that reads back as plain lines, as ANSI text or as the
//! escape sequences that turn one frame into the next.

mod frame;
mod layout;
mod markup;
mod style;

pub use frame::{Cell, Frame};
pub use layout::render;
pub use markup::from_markup;
pub use style::{Color, Style};

#[derive(Debug, Clone)]
pub enum Node {
  Box(Boxed),
  Text(Text),
}

impl From<Boxed> for Node {
  fn from(boxed: Boxed) -> Self {
    Node::Box(boxed)
  }
}

impl From<Text> for Node {
  fn from(text: Text) -> Self {
    Node::Text(text)
  }
}

/// Ink's `flexDirection`. A `Box` is a row unless it says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
  #[default]
  Row,
  Column,
}

/// Ink's `justifyContent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justify {
  Start,
  Center,
  End,
  SpaceBetween,
  SpaceAround,
}

/// Ink's `alignItems`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
  Start,
  Center,
  End,
  Stretch,
}

/// Ink's `borderStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Border {
  Single,
  Double,
  Round,
  Bold,
  Classic,
}

impl Border {
  /// Top left, top right, bottom left, bottom right, horizontal, vertical.
  pub(crate) fn glyphs(self) -> [char; 6] {
    match self {
      Border::Single => ['┌', '┐', '└', '┘', '─', '│'],
      Border::Double => ['╔', '╗', '╚', '╝', '═', '║'],
      Border::Round => ['╭', '╮', '╰', '╯', '─', '│'],
      Border::Bold => ['┏', '┓', '┗', '┛', '━', '┃'],
      Border::Classic => ['+', '+', '+', '+', '-', '|'],
    }
  }
}

/// A `Box`'s width or height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Extent {
  Cells(u16),
  /// A share of the parent's size, `50%` as 50.0. A parent with no definite size gives it nothing to share, so it sizes to its content, as Yoga does.
  Percent(f32),
}

/// Ink's `Box` props, in cells.
#[derive(Debug, Clone, Default)]
pub struct BoxStyle {
  pub direction: Direction,
  pub gap: u16,
  /// Top, right, bottom, left.
  pub padding: [u16; 4],
  /// Top, right, bottom, left.
  pub margin: [u16; 4],
  pub width: Option<Extent>,
  pub height: Option<Extent>,
  pub grow: f32,
  pub shrink: Option<f32>,
  pub justify: Option<Justify>,
  pub align: Option<Align>,
  pub border: Option<Border>,
  pub border_color: Option<Color>,
}

/// Ink's `Box`.
#[derive(Debug, Clone, Default)]
pub struct Boxed {
  pub style: BoxStyle,
  pub children: Vec<Node>,
}

impl Boxed {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn child(mut self, child: impl Into<Node>) -> Self {
    self.children.push(child.into());
    self
  }

  pub fn column(mut self) -> Self {
    self.style.direction = Direction::Column;
    self
  }

  pub fn gap(mut self, cells: u16) -> Self {
    self.style.gap = cells;
    self
  }

  pub fn padding(mut self, cells: u16) -> Self {
    self.style.padding = [cells; 4];
    self
  }

  pub fn padding_x(mut self, cells: u16) -> Self {
    self.style.padding[1] = cells;
    self.style.padding[3] = cells;
    self
  }

  pub fn padding_y(mut self, cells: u16) -> Self {
    self.style.padding[0] = cells;
    self.style.padding[2] = cells;
    self
  }

  pub fn margin(mut self, cells: u16) -> Self {
    self.style.margin = [cells; 4];
    self
  }

  pub fn width(mut self, cells: u16) -> Self {
    self.style.width = Some(Extent::Cells(cells));
    self
  }

  pub fn width_percent(mut self, percent: f32) -> Self {
    self.style.width = Some(Extent::Percent(percent));
    self
  }

  pub fn height(mut self, cells: u16) -> Self {
    self.style.height = Some(Extent::Cells(cells));
    self
  }

  pub fn height_percent(mut self, percent: f32) -> Self {
    self.style.height = Some(Extent::Percent(percent));
    self
  }

  pub fn grow(mut self, grow: f32) -> Self {
    self.style.grow = grow;
    self
  }

  pub fn justify(mut self, justify: Justify) -> Self {
    self.style.justify = Some(justify);
    self
  }

  pub fn align(mut self, align: Align) -> Self {
    self.style.align = Some(align);
    self
  }

  pub fn border(mut self, border: Border) -> Self {
    self.style.border = Some(border);
    self
  }

  pub fn border_color(mut self, color: Color) -> Self {
    self.style.border_color = Some(color);
    self
  }
}

/// Ink's `wrap` on a `Text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wrap {
  #[default]
  Wrap,
  Truncate,
}

/// Ink's `Text`: strings and nested `Text`, each nested one adding its style
/// to the one around it.
#[derive(Debug, Clone, Default)]
pub struct Text {
  pub style: Style,
  pub wrap: Wrap,
  pub children: Vec<Inline>,
}

#[derive(Debug, Clone)]
pub enum Inline {
  Str(String),
  Text(Text),
}

impl Text {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn plain(text: &str) -> Self {
    Self::new().str(text)
  }

  pub fn str(mut self, text: &str) -> Self {
    self.children.push(Inline::Str(text.to_owned()));
    self
  }

  pub fn child(mut self, text: Text) -> Self {
    self.children.push(Inline::Text(text));
    self
  }

  pub fn color(mut self, color: Color) -> Self {
    self.style.color = Some(color);
    self
  }

  pub fn background(mut self, color: Color) -> Self {
    self.style.background = Some(color);
    self
  }

  pub fn bold(mut self) -> Self {
    self.style.bold = true;
    self
  }

  pub fn dim(mut self) -> Self {
    self.style.dim = true;
    self
  }

  pub fn italic(mut self) -> Self {
    self.style.italic = true;
    self
  }

  pub fn underline(mut self) -> Self {
    self.style.underline = true;
    self
  }

  pub fn strikethrough(mut self) -> Self {
    self.style.strikethrough = true;
    self
  }

  pub fn inverse(mut self) -> Self {
    self.style.inverse = true;
    self
  }

  pub fn truncate(mut self) -> Self {
    self.wrap = Wrap::Truncate;
    self
  }

  /// Every character with the style it is drawn in, nested styles added on.
  pub(crate) fn styled(&self, around: &Style) -> Vec<(char, Style)> {
    let style = self.style.over(around);
    let mut out = Vec::new();
    for child in &self.children {
      match child {
        Inline::Str(text) => out.extend(text.chars().map(|c| (c, style))),
        Inline::Text(text) => out.extend(text.styled(&style)),
      }
    }
    out
  }
}
