//! The terms every row of a plan file is built from: a name, a tagged
//! section and the owner of a row.

use snapfire_fsr_ir::sexpr::{SexprError, Sx};
use snapfire_fsr_ir::Body;
use snapfire_fsr_ir::sexpr::{stmt_from_sx, stmt_to_sx};

use crate::RowOwner;

pub(crate) type Res<T> = Result<T, SexprError>;

pub(super) fn err(msg: impl std::fmt::Display) -> SexprError {
  SexprError::new(msg)
}

pub(super) fn sym(s: impl Into<String>) -> Sx {
  Sx::Sym(s.into())
}

pub(super) fn form(head: &str, mut rest: Vec<Sx>) -> Sx {
  let mut items = vec![sym(head)];
  items.append(&mut rest);
  Sx::List(items)
}

/// `(name value)` when the value is there, nothing when it is not.
pub(super) fn opt_form(head: &str, value: &Option<String>) -> Option<Sx> {
  value.as_ref().map(|v| form(head, vec![sym(v.clone())]))
}

pub(super) fn body_form(head: &str, body: &Option<Body>) -> Option<Sx> {
  body.as_ref().map(|b| form(head, b.iter().map(stmt_to_sx).collect()))
}

pub(super) fn owner_sym(owner: RowOwner) -> Sx {
  sym(owner.as_str())
}

pub(super) fn owner_of(sx: &Sx) -> Res<RowOwner> {
  match as_sym(sx)?.as_str() {
    "lowered" => Ok(RowOwner::Lowered),
    "engine" => Ok(RowOwner::Engine),
    "rust" => Ok(RowOwner::Rust),
    other => Err(err(format!("`{other}` is not a row owner"))),
  }
}

pub(super) fn as_sym(sx: &Sx) -> Res<String> {
  match sx {
    Sx::Sym(s) => Ok(s.clone()),
    Sx::Str(s) => Ok(s.clone()),
    _ => Err(err("expected a name")),
  }
}

pub(super) fn as_list(sx: &Sx) -> Res<&[Sx]> {
  match sx {
    Sx::List(items) => Ok(items),
    _ => Err(err("expected a list")),
  }
}

pub(super) fn head_of(sx: &Sx) -> Res<String> {
  match sx {
    Sx::List(items) => match items.first() {
      Some(Sx::Sym(s)) => Ok(s.clone()),
      _ => Err(err("a form starts with a symbol")),
    },
    _ => Err(err("expected a form")),
  }
}

pub(super) fn sections(items: &[Sx], from: usize) -> Res<Vec<(String, Vec<Sx>)>> {
  items[from..]
    .iter()
    .map(|section| {
      let inner = as_list(section)?;
      Ok((head_of(section)?, inner[1..].to_vec()))
    })
    .collect()
}

pub(super) fn one_of(values: &[Sx], head: &str) -> Res<String> {
  match values.first() {
    Some(value) => as_sym(value),
    None => Err(err(format!("`{head}` needs a value"))),
  }
}

pub(super) fn body_of(values: &[Sx]) -> Res<Body> {
  values.iter().map(stmt_from_sx).collect()
}
