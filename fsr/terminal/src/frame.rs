use unicode_width::UnicodeWidthChar;

use crate::style::Style;

const RESET: &str = "\x1b[0m";

/// One cell. A wide character's second cell is its `tail`, drawn by the first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
  pub ch: char,
  pub style: Style,
  pub tail: bool,
}

impl Default for Cell {
  fn default() -> Self {
    Cell { ch: ' ', style: Style::default(), tail: false }
  }
}

impl Cell {
  fn blank(&self) -> bool {
    self.ch == ' ' && self.style.is_plain() && !self.tail
  }
}

/// A drawn grid of cells, `width` by `height`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
  pub width: usize,
  pub height: usize,
  cells: Vec<Cell>,
}

impl Frame {
  pub fn new(width: usize, height: usize) -> Self {
    Frame { width, height, cells: vec![Cell::default(); width * height] }
  }

  pub fn cell(&self, x: usize, y: usize) -> Cell {
    match x < self.width && y < self.height {
      true => self.cells[y * self.width + x],
      false => Cell::default(),
    }
  }

  /// Draws `ch` at `x`, `y`, taking two cells for a wide character. One that
  /// would not fit is not drawn, nor is a zero-width one.
  pub fn put(&mut self, x: usize, y: usize, ch: char, style: Style) {
    let wide = ch.width().unwrap_or(0);
    if wide == 0 || y >= self.height || x + wide > self.width {
      return;
    }
    self.cells[y * self.width + x] = Cell { ch, style, tail: false };
    if wide == 2 {
      self.cells[y * self.width + x + 1] = Cell { ch: ' ', style, tail: true };
    }
  }

  /// Each row as plain text, trailing blanks dropped.
  pub fn lines(&self) -> Vec<String> {
    (0..self.height).map(|y| (0..self.width).map(|x| self.cell(x, y)).filter(|cell| !cell.tail).map(|cell| cell.ch).collect::<String>().trim_end().to_owned()).collect()
  }

  /// Each row with its styles as SGR sequences, rows joined by newlines,
  /// trailing blanks dropped: what Ink's `renderToString` returns.
  pub fn to_ansi(&self) -> String {
    let mut rows = Vec::new();
    for y in 0..self.height {
      let end = (0..self.width).rev().find(|&x| !self.cell(x, y).blank()).map_or(0, |x| x + 1);
      let mut row = String::new();
      let mut current = Style::default();
      for x in 0..end {
        let cell = self.cell(x, y);
        if cell.tail {
          continue;
        }
        write_style(&mut row, &mut current, cell.style);
        row.push(cell.ch);
      }
      write_style(&mut row, &mut current, Style::default());
      rows.push(row);
    }
    rows.join("\n")
  }

  /// What turns `previous` into this frame on a terminal showing `previous`
  /// at its top left: each run of changed cells after a cursor move, then a
  /// clear for each row this frame no longer has.
  pub fn diff(&self, previous: &Frame) -> String {
    let width = self.width.max(previous.width);
    let mut out = String::new();
    for y in 0..self.height {
      let mut x = 0;
      while x < width {
        if self.cell(x, y) == previous.cell(x, y) {
          x += 1;
          continue;
        }
        if self.cell(x, y).tail && x > 0 {
          x -= 1;
        }
        out.push_str(&format!("\x1b[{};{}H", y + 1, x + 1));
        let mut current = Style::default();
        while x < width && (self.cell(x, y) != previous.cell(x, y) || self.cell(x, y).tail) {
          let cell = self.cell(x, y);
          if !cell.tail {
            write_style(&mut out, &mut current, cell.style);
            out.push(cell.ch);
          }
          x += 1;
        }
        write_style(&mut out, &mut current, Style::default());
      }
    }
    for y in self.height..previous.height {
      out.push_str(&format!("\x1b[{};1H\x1b[2K", y + 1));
    }
    out
  }
}

fn write_style(out: &mut String, current: &mut Style, next: Style) {
  if *current == next {
    return;
  }
  if !current.is_plain() {
    out.push_str(RESET);
  }
  if !next.is_plain() {
    out.push_str(&next.sgr());
  }
  *current = next;
}
