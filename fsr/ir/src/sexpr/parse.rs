//! Reading the text into [`Sx`].

use super::syntax::{Sx, DELIMS};
use super::{Res, SexprError};

pub(super) struct Parser<'a> {
  src: &'a [u8],
  pos: usize,
  line: usize,
  depth: usize,
}

/// How deep a file may nest. `form` recurses, so without a bound a file of
/// nothing but `(` overflows the stack, which aborts the process rather than
/// erroring. The deepest tree any real plan has reached is 23.
const MAX_DEPTH: usize = 256;

/// Every top-level form in `src`.
pub fn parse(src: &str) -> Res<Vec<Sx>> {
  let mut p = Parser { src: src.as_bytes(), pos: 0, line: 1, depth: 0 };
  let mut out = Vec::new();
  loop {
    p.skip_trivia();
    if p.pos >= p.src.len() {
      return Ok(out);
    }
    out.push(p.form()?);
  }
}

impl<'a> Parser<'a> {
  fn skip_trivia(&mut self) {
    while self.pos < self.src.len() {
      match self.src[self.pos] {
        b'\n' => {
          self.line += 1;
          self.pos += 1;
        }
        b if b.is_ascii_whitespace() => self.pos += 1,
        b';' => {
          while self.pos < self.src.len() && self.src[self.pos] != b'\n' {
            self.pos += 1;
          }
        }
        _ => return,
      }
    }
  }

  /// Every nested form goes through here, so one counter bounds the recursion
  /// however the file is shaped.
  fn form(&mut self) -> Res<Sx> {
    self.skip_trivia();
    if self.depth >= MAX_DEPTH {
      return Err(SexprError::at(self.line, format!("nested deeper than {MAX_DEPTH}")));
    }
    self.depth += 1;
    let out = self.form_inner();
    self.depth -= 1;
    out
  }

  fn form_inner(&mut self) -> Res<Sx> {
    self.skip_trivia();
    let line = self.line;
    match self.src.get(self.pos) {
      None => Err(SexprError::at(line, "the file ends inside a form")),
      Some(b'(') => {
        self.pos += 1;
        let mut items = Vec::new();
        loop {
          self.skip_trivia();
          match self.src.get(self.pos) {
            None => return Err(SexprError::at(line, "unclosed `(`")),
            Some(b')') => {
              self.pos += 1;
              return Ok(Sx::List(items));
            }
            _ => items.push(self.form()?),
          }
        }
      }
      Some(b'{') => {
        self.pos += 1;
        let inner = self.form()?;
        self.skip_trivia();
        match self.src.get(self.pos) {
          Some(b'}') => {
            self.pos += 1;
            Ok(Sx::Interp(Box::new(inner)))
          }
          _ => Err(SexprError::at(line, "unclosed `{`")),
        }
      }
      Some(b')') => Err(SexprError::at(line, "unbalanced `)`")),
      Some(b'}') => Err(SexprError::at(line, "unbalanced `}`")),
      Some(b'"') => self.quoted(b'"').map(Sx::Str),
      Some(b'|') => self.quoted(b'|').map(Sx::Sym),
      _ => {
        let start = self.pos;
        while self.pos < self.src.len() {
          let b = self.src[self.pos];
          if b.is_ascii_whitespace() || DELIMS.contains(&b) {
            break;
          }
          self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.src[start..self.pos])
          .map_err(|_| SexprError::at(line, "a symbol is not valid UTF-8"))?;
        Ok(Sx::Sym(raw.to_owned()))
      }
    }
  }

  fn quoted(&mut self, quote: u8) -> Res<String> {
    let line = self.line;
    self.pos += 1;
    let mut out = String::new();
    loop {
      match self.src.get(self.pos) {
        None => return Err(SexprError::at(line, "the file ends inside a quoted term")),
        Some(&b) if b == quote => {
          self.pos += 1;
          return Ok(out);
        }
        Some(b'\\') => {
          self.pos += 1;
          match self.src.get(self.pos) {
            None => return Err(SexprError::at(line, "the file ends after `\\`")),
            Some(b'n') => {
              out.push('\n');
              self.pos += 1;
            }
            Some(b'r') => {
              out.push('\r');
              self.pos += 1;
            }
            Some(b't') => {
              out.push('\t');
              self.pos += 1;
            }
            // Any other escape is the character itself, and a character is not
            // a byte: `\é` must carry all of `é` through, not its first byte.
            Some(&b) => {
              let end = (self.pos + utf8_len(b)).min(self.src.len());
              let text = std::str::from_utf8(&self.src[self.pos..end])
                .map_err(|_| SexprError::at(line, "a quoted term is not valid UTF-8"))?;
              if text == "\n" {
                self.line += 1;
              }
              out.push_str(text);
              self.pos = end;
            }
          }
        }
        Some(_) => {
          let start = self.pos;
          while self.pos < self.src.len() {
            let b = self.src[self.pos];
            if b == quote || b == b'\\' {
              break;
            }
            if b == b'\n' {
              self.line += 1;
            }
            self.pos += 1;
          }
          out.push_str(
            std::str::from_utf8(&self.src[start..self.pos])
              .map_err(|_| SexprError::at(line, "a quoted term is not valid UTF-8"))?,
          );
        }
      }
    }
  }
}

/// How many bytes the character starting with `b` occupies.
fn utf8_len(b: u8) -> usize {
  match b {
    0x00..=0x7f => 1,
    0xc0..=0xdf => 2,
    0xe0..=0xef => 3,
    0xf0..=0xf7 => 4,
    _ => 1,
  }
}
