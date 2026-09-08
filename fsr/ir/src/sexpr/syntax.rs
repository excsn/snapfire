//! The syntax tree the text parses to, and how a term is spelled.

use super::{Res, SexprError};

/// One node of the syntax. A symbol and a string are distinct terms: `nil` is
/// the null literal and `"nil"` is the string, so the two never collide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sx {
  Sym(String),
  Str(String),
  List(Vec<Sx>),
  /// `{expr}`, an expression interpolated where a template child is expected.
  Interp(Box<Sx>),
}

impl Sx {
  pub(super) fn sym(s: impl Into<String>) -> Self {
    Sx::Sym(s.into())
  }

  pub(super) fn list(items: Vec<Sx>) -> Self {
    Sx::List(items)
  }

  pub(super) fn as_sym(&self) -> Res<&str> {
    match self {
      Sx::Sym(s) => Ok(s),
      other => Err(SexprError::shape(format!("expected a symbol, found {}", other.kind()))),
    }
  }

  pub(super) fn as_list(&self) -> Res<&[Sx]> {
    match self {
      Sx::List(items) => Ok(items),
      other => Err(SexprError::shape(format!("expected a list, found {}", other.kind()))),
    }
  }

  pub(super) fn kind(&self) -> &'static str {
    match self {
      Sx::Sym(_) => "a symbol",
      Sx::Str(_) => "a string",
      Sx::List(_) => "a list",
      Sx::Interp(_) => "an interpolation",
    }
  }

  /// The head symbol of a list, for dispatching on a form.
  pub(super) fn head(&self) -> Option<&str> {
    match self {
      Sx::List(items) => match items.first() {
        Some(Sx::Sym(s)) => Some(s),
        _ => None,
      },
      _ => None,
    }
  }
}

pub(super) const DELIMS: &[u8] = b"()\"|;{}";

pub(super) fn plain_symbol(s: &str) -> bool {
  !s.is_empty() && !s.bytes().any(|b| b.is_ascii_whitespace() || DELIMS.contains(&b) || b == b'\\')
}

pub(super) fn write_escaped(out: &mut String, s: &str, quote: char) {
  out.push(quote);
  for c in s.chars() {
    match c {
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      c if c == quote => {
        out.push('\\');
        out.push(c);
      }
      c => out.push(c),
    }
  }
  out.push(quote);
}
