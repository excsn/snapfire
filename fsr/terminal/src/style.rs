/// A terminal color: one of the 256 palette entries or a 24-bit value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Color {
  Ansi(u8),
  Rgb(u8, u8, u8),
}

impl Color {
  /// A color as Ink takes one: a name such as `green` or `redBright`, `#rrggbb` or `rgb(r, g, b)`.
  pub fn parse(text: &str) -> Option<Self> {
    const NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
      if hex.len() != 6 {
        return None;
      }
      let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
      return Some(Color::Rgb(byte(0)?, byte(2)?, byte(4)?));
    }
    if let Some(inner) = text.strip_prefix("rgb(").and_then(|rest| rest.strip_suffix(')')) {
      let parts: Vec<u8> = inner.split(',').map(|part| part.trim().parse().ok()).collect::<Option<_>>()?;
      return match parts[..] {
        [r, g, b] => Some(Color::Rgb(r, g, b)),
        _ => None,
      };
    }
    if text == "gray" || text == "grey" {
      return Some(Color::Ansi(8));
    }
    let (name, bright) = match text.strip_suffix("Bright") {
      Some(name) => (name, 8),
      None => (text, 0),
    };
    NAMES.iter().position(|known| *known == name).map(|index| Color::Ansi(index as u8 + bright))
  }

  fn codes(self, background: bool) -> String {
    match self {
      Color::Ansi(n) if n < 8 => format!("{}", if background { 40 } else { 30 } + n as u16),
      Color::Ansi(n) if n < 16 => format!("{}", if background { 100 } else { 90 } + (n - 8) as u16),
      Color::Ansi(n) => format!("{};5;{n}", if background { 48 } else { 38 }),
      Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", if background { 48 } else { 38 }),
    }
  }
}

/// How a cell is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
  pub color: Option<Color>,
  pub background: Option<Color>,
  pub bold: bool,
  pub dim: bool,
  pub italic: bool,
  pub underline: bool,
  pub strikethrough: bool,
  pub inverse: bool,
}

impl Style {
  /// This style laid over `around`: a color it sets wins, a flag either sets is on.
  pub fn over(&self, around: &Style) -> Style {
    Style {
      color: self.color.or(around.color),
      background: self.background.or(around.background),
      bold: self.bold || around.bold,
      dim: self.dim || around.dim,
      italic: self.italic || around.italic,
      underline: self.underline || around.underline,
      strikethrough: self.strikethrough || around.strikethrough,
      inverse: self.inverse || around.inverse,
    }
  }

  pub fn is_plain(&self) -> bool {
    *self == Style::default()
  }

  /// The SGR sequence that turns this style on.
  pub fn sgr(&self) -> String {
    let mut codes: Vec<String> = Vec::new();
    for (on, code) in [(self.bold, "1"), (self.dim, "2"), (self.italic, "3"), (self.underline, "4"), (self.inverse, "7"), (self.strikethrough, "9")] {
      if on {
        codes.push(code.to_owned());
      }
    }
    if let Some(color) = self.color {
      codes.push(color.codes(false));
    }
    if let Some(color) = self.background {
      codes.push(color.codes(true));
    }
    format!("\x1b[{}m", codes.join(";"))
  }
}
