//! The tables and shape checks the typed layers share: operator spellings, literal atoms and the arity helpers.

use crate::ast::{ArithOp, Builtin, CompareOp, Lit};

use super::syntax::Sx;
use super::{Res, SexprError};

/// Heads that mean something other than a field name in an entry position.
pub(super) const ENTRY_MARKERS: [&str; 4] = ["=", ":", "...", "@"];

pub(super) fn reserved_atom(s: &str) -> bool {
  s.is_empty()
    || s == "nil"
    || s == "#t"
    || s == "#f"
    || s.parse::<i128>().is_ok()
    || s.parse::<f64>().is_ok()
}

pub(super) fn lit_to_sx(lit: &Lit) -> Sx {
  match lit {
    Lit::Null => Sx::sym("nil"),
    Lit::Bool(true) => Sx::sym("#t"),
    Lit::Bool(false) => Sx::sym("#f"),
    Lit::Int(n) => Sx::Sym(n.to_string()),
    Lit::Float(f) => Sx::Sym(format!("{f:?}")),
    Lit::Str(s) => Sx::Str(s.clone()),
  }
}

pub(super) fn arith_sym(op: ArithOp) -> &'static str {
  match op {
    ArithOp::Add => "+",
    ArithOp::Sub => "-",
    ArithOp::Mul => "*",
    ArithOp::Div => "/",
    ArithOp::Rem => "%",
  }
}

pub(super) fn arith_of(s: &str) -> Option<ArithOp> {
  Some(match s {
    "+" => ArithOp::Add,
    "-" => ArithOp::Sub,
    "*" => ArithOp::Mul,
    "/" => ArithOp::Div,
    "%" => ArithOp::Rem,
    _ => return None,
  })
}

pub(super) fn compare_sym(op: CompareOp) -> &'static str {
  match op {
    CompareOp::Eq => "==",
    CompareOp::Ne => "!=",
    CompareOp::Lt => "<",
    CompareOp::Le => "<=",
    CompareOp::Gt => ">",
    CompareOp::Ge => ">=",
  }
}

pub(super) fn compare_of(s: &str) -> Option<CompareOp> {
  Some(match s {
    "==" => CompareOp::Eq,
    "!=" => CompareOp::Ne,
    "<" => CompareOp::Lt,
    "<=" => CompareOp::Le,
    ">" => CompareOp::Gt,
    ">=" => CompareOp::Ge,
    _ => return None,
  })
}

pub(super) fn builtin_sym(b: Builtin) -> &'static str {
  match b {
    Builtin::Round => "round",
    Builtin::Floor => "floor",
    Builtin::Ceil => "ceil",
    Builtin::Abs => "abs",
    Builtin::Min => "min",
    Builtin::Max => "max",
    Builtin::ToFixed => "to-fixed",
    Builtin::Repeat => "repeat",
    Builtin::Join => "join",
    Builtin::Trim => "trim",
    Builtin::Upper => "upper",
    Builtin::Lower => "lower",
    Builtin::Includes => "includes",
    Builtin::EncodeUriComponent => "encode-uri",
    Builtin::LocaleNumber => "locale-number",
    Builtin::Range => "range",
    Builtin::Omit => "omit",
  }
}

pub(super) fn builtin_of(s: &str) -> Option<Builtin> {
  Some(match s {
    "round" => Builtin::Round,
    "floor" => Builtin::Floor,
    "ceil" => Builtin::Ceil,
    "abs" => Builtin::Abs,
    "min" => Builtin::Min,
    "max" => Builtin::Max,
    "to-fixed" => Builtin::ToFixed,
    "repeat" => Builtin::Repeat,
    "join" => Builtin::Join,
    "trim" => Builtin::Trim,
    "upper" => Builtin::Upper,
    "lower" => Builtin::Lower,
    "includes" => Builtin::Includes,
    "encode-uri" => Builtin::EncodeUriComponent,
    "locale-number" => Builtin::LocaleNumber,
    "range" => Builtin::Range,
    "omit" => Builtin::Omit,
    _ => return None,
  })
}

pub(super) fn form(head: &str, mut rest: Vec<Sx>) -> Sx {
  let mut items = vec![Sx::sym(head)];
  items.append(&mut rest);
  Sx::List(items)
}

pub(super) fn args<'a>(items: &'a [Sx], head: &str, n: usize) -> Res<&'a [Sx]> {
  let rest = &items[1..];
  if rest.len() != n {
    return Err(SexprError::shape(format!("`{head}` takes {n} terms, found {}", rest.len())));
  }
  Ok(rest)
}

pub(super) fn at_least<'a>(items: &'a [Sx], head: &str, n: usize) -> Res<&'a [Sx]> {
  let rest = &items[1..];
  if rest.len() < n {
    return Err(SexprError::shape(format!("`{head}` takes at least {n} terms, found {}", rest.len())));
  }
  Ok(rest)
}

pub(super) fn sym_of(sx: &Sx) -> Res<String> {
  sx.as_sym().map(str::to_owned)
}

pub(super) fn str_of(sx: &Sx) -> Res<String> {
  match sx {
    Sx::Str(s) => Ok(s.clone()),
    other => Err(SexprError::shape(format!("expected a string, found {}", other.kind()))),
  }
}

pub(super) fn opt_of(sx: &Sx) -> Res<Option<String>> {
  match sx {
    Sx::Sym(s) if s == "nil" => Ok(None),
    Sx::Str(s) => Ok(Some(s.clone())),
    other => Err(SexprError::shape(format!("expected a string or `nil`, found {}", other.kind()))),
  }
}

pub(super) fn u32_of(sx: &Sx) -> Res<u32> {
  sx.as_sym()?.parse().map_err(|_| SexprError::shape("expected a number"))
}

