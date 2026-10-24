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

/// `text`'s characters with the style each is drawn in, starting from `base`: an SGR sequence, `ESC [ … m`, sets the style of what follows as a terminal would, a reset or a closing code going back to `base`, and draws as nothing; any other escape sequence draws as nothing too.
pub(crate) fn ansi_styled(text: &str, base: Style) -> Vec<(char, Style)> {
  let mut out = Vec::new();
  let mut style = base;
  let mut chars = text.chars().peekable();
  while let Some(ch) = chars.next() {
    if ch != '\u{1b}' {
      out.push((ch, style));
      continue;
    }
    match chars.next() {
      Some('[') => {
        let mut params = String::new();
        let mut last = None;
        for next in chars.by_ref() {
          if ('\u{40}'..='\u{7e}').contains(&next) {
            last = Some(next);
            break;
          }
          params.push(next);
        }
        if last == Some('m') {
          apply_sgr(&params, base, &mut style);
        }
      }
      Some(']') => {
        while let Some(next) = chars.next() {
          if next == '\u{7}' || (next == '\u{1b}' && chars.next_if_eq(&'\\').is_some()) {
            break;
          }
        }
      }
      _ => {}
    }
  }
  out
}

/// Applies the codes of one SGR sequence to `style`, `base` being what a reset or a closing code goes back to.
fn apply_sgr(params: &str, base: Style, style: &mut Style) {
  let codes: Vec<u16> = if params.is_empty() { vec![0] } else { params.split(';').map(|code| code.parse().unwrap_or(0)).collect() };
  let mut at = 0;
  while at < codes.len() {
    let code = codes[at];
    let extended = |at: &mut usize| -> Option<Color> {
      match codes.get(*at + 1) {
        Some(5) => {
          *at += 2;
          codes.get(*at).map(|n| Color::Ansi(*n as u8))
        }
        Some(2) => {
          *at += 4;
          Some(Color::Rgb(*codes.get(*at - 2)? as u8, *codes.get(*at - 1)? as u8, *codes.get(*at)? as u8))
        }
        _ => None,
      }
    };
    match code {
      0 => *style = base,
      1 => style.bold = true,
      2 => style.dim = true,
      3 => style.italic = true,
      4 => style.underline = true,
      7 => style.inverse = true,
      9 => style.strikethrough = true,
      22 => (style.bold, style.dim) = (base.bold, base.dim),
      23 => style.italic = base.italic,
      24 => style.underline = base.underline,
      27 => style.inverse = base.inverse,
      29 => style.strikethrough = base.strikethrough,
      30..=37 => style.color = Some(Color::Ansi((code - 30) as u8)),
      90..=97 => style.color = Some(Color::Ansi((code - 90 + 8) as u8)),
      39 => style.color = base.color,
      40..=47 => style.background = Some(Color::Ansi((code - 40) as u8)),
      100..=107 => style.background = Some(Color::Ansi((code - 100 + 8) as u8)),
      49 => style.background = base.background,
      38 => style.color = extended(&mut at).or(style.color),
      48 => style.background = extended(&mut at).or(style.background),
      _ => {}
    }
    at += 1;
  }
}
