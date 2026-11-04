//! CLDR number patterns, read for the text around the number: `¤#,##0.00`, `#,##0 %`, `#,##0.00 ¤;(#,##0.00 ¤)`. The digits themselves are the caller's.

/// One piece of a prefix or a suffix.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
  Text(String),
  Currency,
  Percent,
  Minus,
  Plus,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Affixes {
  prefix: Vec<Piece>,
  suffix: Vec<Piece>,
}

/// A pattern's positive affixes and its negative ones when it writes a negative subpattern after `;`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pattern {
  positive: Affixes,
  negative: Option<Affixes>,
}

/// Which sign the number is written with, if any.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Sign {
  /// No sign: a positive number or zero.
  #[default]
  None,
  /// The locale's minus sign.
  Minus,
  /// The locale's plus sign, as `signDisplay: "always"` writes one.
  Plus,
}

/// The symbols a pattern's placeholders stand for.
pub(crate) struct Symbols<'a> {
  pub currency: &'a str,
  pub percent: &'a str,
  pub minus: &'a str,
  pub plus: &'a str,
}

fn is_number(c: char) -> bool {
  matches!(c, '#' | '0'..='9' | '@' | ',' | '.')
}

fn push_text(pieces: &mut Vec<Piece>, c: char) {
  match pieces.last_mut() {
    Some(Piece::Text(text)) => text.push(c),
    _ => pieces.push(Piece::Text(c.to_string())),
  }
}

fn subpattern(text: &str) -> Affixes {
  let mut affixes = Affixes::default();
  let mut in_number = false;
  let mut after_number = false;
  let mut quoted = false;
  let mut chars = text.chars().peekable();
  while let Some(c) = chars.next() {
    let side = match after_number {
      true => &mut affixes.suffix,
      false => &mut affixes.prefix,
    };
    if quoted {
      match (c, chars.peek()) {
        ('\'', Some('\'')) => {
          chars.next();
          push_text(side, '\'');
        }
        ('\'', _) => quoted = false,
        _ => push_text(side, c),
      }
      continue;
    }
    if is_number(c) && !after_number {
      in_number = true;
      continue;
    }
    if in_number {
      in_number = false;
      after_number = true;
    }
    let side = match after_number {
      true => &mut affixes.suffix,
      false => &mut affixes.prefix,
    };
    match c {
      '\'' if chars.peek() == Some(&'\'') => {
        chars.next();
        push_text(side, '\'');
      }
      '\'' => quoted = true,
      '¤' => {
        while chars.peek() == Some(&'¤') {
          chars.next();
        }
        side.push(Piece::Currency);
      }
      '%' => side.push(Piece::Percent),
      '-' => side.push(Piece::Minus),
      '+' => side.push(Piece::Plus),
      other => push_text(side, other),
    }
  }
  affixes
}

impl Pattern {
  pub(crate) fn parse(text: &str) -> Pattern {
    let (positive, negative) = match text.split_once(';') {
      Some((positive, negative)) => (positive, Some(negative)),
      None => (text, None),
    };
    Pattern { positive: subpattern(positive), negative: negative.map(subpattern) }
  }

  /// Whether the currency sits against the number with nothing between them: `Some(true)` when it comes before the number, `Some(false)` after, `None` when text separates them or there is no currency.
  pub(crate) fn currency_touches_number(&self) -> Option<bool> {
    if self.positive.prefix.last() == Some(&Piece::Currency) {
      return Some(true);
    }
    if self.positive.suffix.first() == Some(&Piece::Currency) {
      return Some(false);
    }
    None
  }

  /// `digits` with the affixes for `sign`. A pattern with no negative subpattern writes a negative number as the minus sign before the positive one. A plus sign goes in the same place.
  pub(crate) fn write(&self, digits: &str, sign: Sign, symbols: &Symbols) -> String {
    let (affixes, lead) = match (sign, &self.negative) {
      (Sign::None, _) => (&self.positive, None),
      (Sign::Minus, Some(negative)) => (negative, None),
      (Sign::Plus, Some(negative)) => (negative, None),
      (Sign::Minus, None) => (&self.positive, Some(symbols.minus)),
      (Sign::Plus, None) => (&self.positive, Some(symbols.plus)),
    };
    let piece = |out: &mut String, piece: &Piece| match piece {
      Piece::Text(text) => out.push_str(text),
      Piece::Currency => out.push_str(symbols.currency),
      Piece::Percent => out.push_str(symbols.percent),
      Piece::Minus if sign == Sign::Plus => out.push_str(symbols.plus),
      Piece::Minus => out.push_str(symbols.minus),
      Piece::Plus => out.push_str(symbols.plus),
    };
    let mut out = String::with_capacity(digits.len() + 16);
    if let Some(lead) = lead {
      out.push_str(lead);
    }
    affixes.prefix.iter().for_each(|p| piece(&mut out, p));
    out.push_str(digits);
    affixes.suffix.iter().for_each(|p| piece(&mut out, p));
    out
  }
}

#[cfg(test)]
mod tests {
  use super::{Pattern, Sign, Symbols};

  fn symbols() -> Symbols<'static> {
    Symbols { currency: "$", percent: "%", minus: "-", plus: "+" }
  }

  #[test]
  fn affixes_take_the_symbols_and_the_number_keeps_its_place() {
    assert_eq!(Pattern::parse("¤#,##0.00").write("1,234.50", Sign::None, &symbols()), "$1,234.50");
    assert_eq!(Pattern::parse("#,##0.00\u{a0}¤").write("1.234,50", Sign::Minus, &symbols()), "-1.234,50\u{a0}$");
    assert_eq!(Pattern::parse("¤#,##0.00;(¤#,##0.00)").write("5.00", Sign::Minus, &symbols()), "($5.00)");
    assert_eq!(Pattern::parse("%#,##0").write("12", Sign::Minus, &symbols()), "-%12");
    assert_eq!(Pattern::parse("#,##0'x'%").write("3", Sign::Plus, &symbols()), "+3x%");
  }

  #[test]
  fn the_currency_touches_the_number_only_with_nothing_between() {
    assert_eq!(Pattern::parse("¤#,##0.00").currency_touches_number(), Some(true));
    assert_eq!(Pattern::parse("#,##0.00¤").currency_touches_number(), Some(false));
    assert_eq!(Pattern::parse("¤\u{a0}#,##0.00").currency_touches_number(), None);
  }
}
