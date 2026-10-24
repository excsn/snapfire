//! `terminal.bigText`: text drawn in a font file of cfonts' format, laid out as cfonts 3.3 lays it out. FSR ships no font; the application supplies the file it draws with, under that font's own licence.

use crate::ext::{number, option, text, Ambient, Reach};
use crate::interp::Fail;
use snapfire_fsr_core::Value;

const WHAT: &str = "terminal.bigText";

pub fn register(extensions: &mut crate::ext::Extensions) {
  extensions.register(WHAT, Reach::Terminal, big_text);
}

/// A font as cfonts' JSON holds it: how many colours its `<cN>` tags name, how many lines a letter takes, the lines that start a row, the lines between letters and each letter's lines.
struct Font {
  colors: usize,
  lines: usize,
  buffer: Vec<String>,
  letterspace: Vec<String>,
  chars: Vec<(String, Vec<String>)>,
}

impl Font {
  fn read(value: &Value) -> Result<Self, Fail> {
    let Value::Map(map) = value else { return Err(Fail::internal(format!("{WHAT}: the font is not an object; pass the font file's JSON"))) };
    let count = |key: &str| match map.get(key) {
      Some(Value::Int(n)) => Ok((*n).max(0) as usize),
      Some(Value::UInt(n)) => Ok(*n as usize),
      Some(Value::F64(f)) => Ok(f.max(0.0) as usize),
      _ => Err(Fail::internal(format!("{WHAT}: the font has no number `{key}`"))),
    };
    let lines_of = |what: &str, value: Option<&Value>| -> Result<Vec<String>, Fail> {
      match value {
        Some(Value::Seq(items)) => items.iter().map(|item| match item {
          Value::Str(s) => Ok(s.to_string()),
          _ => Err(Fail::internal(format!("{WHAT}: the font's `{what}` holds a line that is not a string"))),
        }).collect(),
        _ => Err(Fail::internal(format!("{WHAT}: the font has no list `{what}`"))),
      }
    };
    let Some(Value::Map(chars)) = map.get("chars") else { return Err(Fail::internal(format!("{WHAT}: the font has no object `chars`"))) };
    let font = Font {
      colors: count("colors")?,
      lines: count("lines")?,
      buffer: lines_of("buffer", map.get("buffer"))?,
      letterspace: lines_of("letterspace", map.get("letterspace"))?,
      chars: chars.iter().map(|(name, lines)| Ok((name.clone(), lines_of(&format!("chars.{name}"), Some(lines))?))).collect::<Result<_, Fail>>()?,
    };
    for (what, lines) in [("buffer", &font.buffer), ("letterspace", &font.letterspace)].into_iter().chain(font.chars.iter().map(|(name, lines)| (name.as_str(), lines))) {
      if lines.len() < font.lines {
        return Err(Fail::internal(format!("{WHAT}: the font's `{what}` has {} lines of the {} it says a letter takes", lines.len(), font.lines)));
      }
    }
    Ok(font)
  }

  fn char(&self, ch: &str) -> Option<&[String]> {
    self.chars.iter().find(|(name, _)| name == ch).map(|(_, lines)| lines.as_slice())
  }
}

#[derive(PartialEq)]
enum Align {
  Left,
  Center,
  Right,
}

/// `bigText(font, text, { align, colors, background, letterSpacing, lineHeight, space, maxLength, width })`.
fn big_text(_: &Ambient, args: &[Value]) -> Result<Value, Fail> {
  let font = Font::read(args.first().unwrap_or(&Value::Undefined))?;
  let input = text(WHAT, args, 1)?;
  let count = |key: &str, default: usize| -> Result<usize, Fail> {
    match option(WHAT, args, 2, key)? {
      None => Ok(default),
      Some(value) => Ok(number(&format!("{WHAT} `{key}`"), std::slice::from_ref(value), 0)?.max(0.0) as usize),
    }
  };
  let word = |key: &str| -> Result<Option<String>, Fail> {
    match option(WHAT, args, 2, key)? {
      None => Ok(None),
      Some(Value::Str(s)) => Ok(Some(s.to_string())),
      Some(_) => Err(Fail::internal(format!("{WHAT}: `{key}` is not a string"))),
    }
  };
  let align = match word("align")?.as_deref() {
    None | Some("left" | "top" | "bottom") => Align::Left,
    Some("center") => Align::Center,
    Some("right") => Align::Right,
    Some(other) => return Err(Fail::internal(format!("{WHAT}: an align of `{other}`; `left`, `center` and `right` are read"))),
  };
  let colors: Vec<String> = match option(WHAT, args, 2, "colors")? {
    None => Vec::new(),
    Some(Value::Seq(items)) => items.iter().map(|item| match item {
      Value::Str(s) => Ok(s.to_string()),
      _ => Err(Fail::internal(format!("{WHAT}: `colors` holds a colour that is not a string"))),
    }).collect::<Result<_, Fail>>()?,
    Some(_) => return Err(Fail::internal(format!("{WHAT}: `colors` is not a list"))),
  };
  let letter_spacing = count("letterSpacing", font.letterspace.iter().map(|line| width(line)).max().unwrap_or(0))?;
  let line_height = count("lineHeight", 1)?;
  let max_length = count("maxLength", 0)?;
  let columns = count("width", 80)?;
  let space = option(WHAT, args, 2, "space")?.map_or(true, crate::interp::truthy);
  let background = word("background")?.unwrap_or_else(|| "transparent".to_owned());

  let colorize = |line: &str| colorize(line, font.colors, &colors);
  let spacing = font.letterspace.iter().take(font.lines).map(|line| width(line)).max().unwrap_or(0);
  let buffer_width = font.buffer.iter().take(font.lines).map(|line| width(line)).max().unwrap_or(0);
  let mut output: Vec<String> = Vec::new();
  add_line(&mut output, &font, line_height);
  let mut line_length = buffer_width;
  let mut printed = 0;
  for ch in input.replace("\r\n", "|").replace(['\r', '\n'], "|").chars() {
    let upper: String = ch.to_uppercase().collect();
    let glyph = font.char(&upper);
    if upper != "|" && glyph.is_none() {
      continue;
    }
    let last_length = line_length;
    if let Some(glyph) = glyph {
      line_length += glyph.iter().take(font.lines).map(|line| width(line)).max().unwrap_or(0) + spacing * letter_spacing;
    }
    if (max_length != 0 && printed >= max_length) || upper == "|" || line_length > columns {
      align_text(&mut output, last_length, font.lines, &align, columns);
      line_length = buffer_width;
      if let Some(glyph) = glyph {
        line_length += spacing * letter_spacing + glyph.iter().take(font.lines).map(|line| width(line)).max().unwrap_or(0);
      }
      printed = 0;
      add_line(&mut output, &font, line_height);
    }
    if let Some(glyph) = glyph {
      printed += 1;
      let top = output.len() - font.lines;
      for (index, row) in output[top..].iter_mut().enumerate() {
        let mut gap = colorize(&font.letterspace[index]);
        if gap.is_empty() && letter_spacing > 0 {
          gap = " ".to_owned();
        }
        row.push_str(&gap.repeat(letter_spacing));
        row.push_str(&colorize(&glyph[index]));
      }
    }
  }
  align_text(&mut output, line_length, font.lines, &align, columns);
  if space && !output.is_empty() {
    output[0].insert_str(0, "\n\n");
    output.last_mut().expect("a row").push_str("\n\n");
  }
  if background != "transparent" && !output.is_empty() {
    let (open, close) = color(&background, true);
    output[0].insert_str(0, &format!("{open}\n"));
    output.last_mut().expect("a row").push_str(close);
  }
  Ok(Value::str(output.join("\n")))
}

/// Starts a row of letters: `line_height` empty lines, none before the first row, then the font's buffer.
fn add_line(output: &mut Vec<String>, font: &Font, line_height: usize) {
  let line_height = if output.is_empty() { 0 } else { line_height };
  for index in 0..font.lines + line_height {
    output.push(if index > line_height { font.buffer[index - line_height].clone() } else { String::new() });
  }
}

fn align_text(output: &mut [String], line_length: usize, lines: usize, align: &Align, columns: usize) {
  let space = match align {
    Align::Left => 0,
    Align::Center => columns.saturating_sub(line_length).div_ceil(2),
    Align::Right => columns.saturating_sub(line_length),
  };
  if space > 0 {
    let top = output.len() - lines;
    let pad = " ".repeat(space);
    for row in &mut output[top..] {
      row.insert_str(0, &pad);
    }
  }
}

/// The columns a line of the font takes, its `<…>` tags left out, counted in UTF-16 units as cfonts counts them.
fn width(line: &str) -> usize {
  let mut count = 0;
  let mut rest = line;
  while let Some(open) = rest.find('<') {
    match rest[open + 1..].find('>') {
      Some(len) if len > 0 => {
        count += rest[..open].encode_utf16().count();
        rest = &rest[open + 1 + len + 1..];
      }
      _ => {
        count += rest[..=open].encode_utf16().count();
        rest = &rest[open + 1..];
      }
    }
  }
  count + rest.encode_utf16().count()
}

/// `line` with each `<cN>` and `</cN>` replaced by the codes of the Nth colour; a one-colour font's line wrapped in the first.
fn colorize(line: &str, font_colors: usize, colors: &[String]) -> String {
  let nth = |i: usize| colors.get(i).map_or("system", String::as_str);
  match font_colors {
    1 => {
      let (open, close) = color(nth(0), false);
      format!("{open}{line}{close}")
    }
    n if n > 1 => {
      let mut out = line.to_owned();
      for i in 0..n {
        let (open, close) = color(nth(i), false);
        out = out.replace(&format!("<c{}>", i + 1), &open).replace(&format!("</c{}>", i + 1), close);
      }
      out
    }
    _ => line.to_owned(),
  }
}

/// The codes that open and close a cfonts colour: a name, `candy` for one picked at random, or `#rgb` and `#rrggbb` in 24-bit colour. `system` and a name cfonts does not know add none.
fn color(name: &str, background: bool) -> (String, &'static str) {
  let close = if background { "\u{1b}[49m" } else { "\u{1b}[39m" };
  let code = |fg: u8, bg: u8| Some(format!("\u{1b}[{}m", if background { bg } else { fg }));
  let open = match name.to_ascii_lowercase().as_str() {
    "system" => None,
    "transparent" => Some("\u{1b}[49m".to_owned()),
    "black" => code(30, 40),
    "red" => code(31, 41),
    "green" => code(32, 42),
    "yellow" => code(33, 43),
    "blue" => code(34, 44),
    "magenta" => code(35, 45),
    "cyan" => code(36, 46),
    "white" => code(37, 47),
    "gray" => code(90, 100),
    "redbright" => code(91, 101),
    "greenbright" => code(92, 102),
    "yellowbright" => code(93, 103),
    "bluebright" => code(94, 104),
    "magentabright" => code(95, 105),
    "cyanbright" => code(96, 106),
    "whitebright" => code(97, 107),
    "candy" => {
      const CANDY: [u8; 11] = [31, 32, 33, 35, 36, 91, 92, 93, 94, 95, 96];
      let mut byte = [0u8];
      let _ = getrandom::fill(&mut byte);
      Some(format!("\u{1b}[{}m", CANDY[byte[0] as usize % CANDY.len()]))
    }
    _ => hex(name).map(|(r, g, b)| format!("\u{1b}[{};2;{r};{g};{b}m", if background { 48 } else { 38 })),
  };
  match open {
    Some(open) => (open, close),
    None => (String::new(), ""),
  }
}

fn hex(name: &str) -> Option<(u8, u8, u8)> {
  let digits = name.strip_prefix('#')?;
  if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
    return None;
  }
  let full = match digits.len() {
    3 => digits.chars().flat_map(|c| [c, c]).collect(),
    6 => digits.to_owned(),
    _ => return None,
  };
  let at = |i: usize| u8::from_str_radix(&full[i..i + 2], 16).ok();
  Some((at(0)?, at(2)?, at(4)?))
}
