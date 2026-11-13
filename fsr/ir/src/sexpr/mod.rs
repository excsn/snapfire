//! The s-expression form of a plan artifact: the text `plan.sexp` carries.
//!
//! [`Sx`] is what the text parses to. The typed layer turns an [`Sx`] into the
//! IR and back, collapsing the shapes a plan is mostly made of, so a literal
//! attribute is `(class "wide")` rather than a field wrapping a literal
//! wrapping a string.

mod atoms;
pub(crate) use atoms::{builtin_of, builtin_sym};
mod body;
mod expr;
mod parse;
mod print;
mod syntax;
mod tmpl;

pub use body::{component_from_sections, component_from_sx, component_sections, component_to_sx, stmt_from_sx, stmt_to_sx};
pub use expr::{expr_from_sx, expr_to_sx};
pub use parse::{forms, parse, Forms};
pub use print::print;
pub use syntax::Sx;
pub use tmpl::{tmpl_from_sx, tmpl_to_sx};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct SexprError(String);

impl SexprError {
  pub fn new(msg: impl std::fmt::Display) -> Self {
    Self(msg.to_string())
  }

  fn at(line: usize, msg: impl std::fmt::Display) -> Self {
    Self(format!("line {line}: {msg}"))
  }

  fn shape(msg: impl std::fmt::Display) -> Self {
    Self(msg.to_string())
  }
}

type Res<T> = Result<T, SexprError>;

/// `read` over every item, stopping at the first error, into a vector exactly
/// `items.len()` long. Collecting into a `Result` loses the length, so that
/// vector starts at four and doubles, which for a plan's one-child elements
/// is most of the memory its tree holds.
pub fn read_all<T>(items: &[Sx], mut read: impl FnMut(&Sx) -> Result<T, SexprError>) -> Result<Vec<T>, SexprError> {
  let mut out = Vec::with_capacity(items.len());
  for item in items {
    out.push(read(item)?);
  }
  Ok(out)
}
