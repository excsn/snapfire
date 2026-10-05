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
    ArithOp::Pow => "**",
  }
}

pub(super) fn arith_of(s: &str) -> Option<ArithOp> {
  Some(match s {
    "+" => ArithOp::Add,
    "-" => ArithOp::Sub,
    "*" => ArithOp::Mul,
    "/" => ArithOp::Div,
    "%" => ArithOp::Rem,
    "**" => ArithOp::Pow,
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
    Builtin::StartsWith => "starts-with",
    Builtin::EndsWith => "ends-with",
    Builtin::Split => "split",
    Builtin::Replace => "replace",
    Builtin::EncodeUriComponent => "encode-uri",
    Builtin::LocaleNumber => "locale-number",
    Builtin::Range => "range",
    Builtin::Omit => "omit",
    Builtin::Slice => "slice",
    Builtin::At => "at",
    Builtin::IndexOf => "index-of",
    Builtin::Concat => "concat-items",
    Builtin::Reverse => "reverse",
    Builtin::PadStart => "pad-start",
    Builtin::PadEnd => "pad-end",
    Builtin::Substring => "substring",
    Builtin::Json => "json",
    Builtin::Pow => "pow",
    Builtin::Sqrt => "sqrt",
    Builtin::Trunc => "trunc",
    Builtin::Sign => "sign",
    Builtin::MinOf => "min-of",
    Builtin::MaxOf => "max-of",
    Builtin::DateMs => "date-ms",
    Builtin::DatePart => "date-part",
    Builtin::IsoString => "iso-string",
    Builtin::FromEntries => "from-entries",
    Builtin::Unique => "unique",
    Builtin::HasKey => "has-key",
    Builtin::FormEncode => "form-encode",
    Builtin::RegexTest => "regex-test",
    Builtin::Match => "match",
    Builtin::MatchAll => "match-all",
    Builtin::Search => "search",
    Builtin::ReplaceAll => "replace-all",
    Builtin::Checked => "checked",
    Builtin::LooseMatch => "loose-match",
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
    "starts-with" => Builtin::StartsWith,
    "ends-with" => Builtin::EndsWith,
    "split" => Builtin::Split,
    "replace" => Builtin::Replace,
    "encode-uri" => Builtin::EncodeUriComponent,
    "locale-number" => Builtin::LocaleNumber,
    "range" => Builtin::Range,
    "omit" => Builtin::Omit,
    "slice" => Builtin::Slice,
    "at" => Builtin::At,
    "index-of" => Builtin::IndexOf,
    "concat-items" => Builtin::Concat,
    "reverse" => Builtin::Reverse,
    "pad-start" => Builtin::PadStart,
    "pad-end" => Builtin::PadEnd,
    "substring" => Builtin::Substring,
    "json" => Builtin::Json,
    "pow" => Builtin::Pow,
    "sqrt" => Builtin::Sqrt,
    "trunc" => Builtin::Trunc,
    "sign" => Builtin::Sign,
    "min-of" => Builtin::MinOf,
    "max-of" => Builtin::MaxOf,
    "date-ms" => Builtin::DateMs,
    "date-part" => Builtin::DatePart,
    "iso-string" => Builtin::IsoString,
    "from-entries" => Builtin::FromEntries,
    "unique" => Builtin::Unique,
    "has-key" => Builtin::HasKey,
    "form-encode" => Builtin::FormEncode,
    "regex-test" => Builtin::RegexTest,
    "match" => Builtin::Match,
    "match-all" => Builtin::MatchAll,
    "search" => Builtin::Search,
    "replace-all" => Builtin::ReplaceAll,
    "checked" => Builtin::Checked,
    "loose-match" => Builtin::LooseMatch,
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

