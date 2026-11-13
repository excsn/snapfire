//! The tables and shape checks the typed layers share: operator spellings, literal atoms and the arity helpers.

use crate::ast::{ArithOp, Builtin, CompareOp, Lit};

use super::syntax::Sx;
use super::{Res, SexprError};

/// Heads that mean something other than a field name in an entry position.
pub(super) const ENTRY_MARKERS: [&str; 5] = ["=", ":", "...", "@", "!"];

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
    Lit::Undefined => Sx::sym("undefined"),
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
    ArithOp::Quot => "quot",
    ArithOp::BitAnd => "bit-and",
    ArithOp::BitOr => "bit-or",
    ArithOp::BitXor => "bit-xor",
    ArithOp::Shl => "<<",
    ArithOp::Shr => ">>",
    ArithOp::UShr => ">>>",
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
    "quot" => ArithOp::Quot,
    "bit-and" => ArithOp::BitAnd,
    "bit-or" => ArithOp::BitOr,
    "bit-xor" => ArithOp::BitXor,
    "<<" => ArithOp::Shl,
    ">>" => ArithOp::Shr,
    ">>>" => ArithOp::UShr,
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

pub(crate) fn builtin_sym(b: Builtin) -> &'static str {
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
    Builtin::DecodeUriComponent => "decode-uri",
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
    Builtin::JsonParse => "json-parse",
    Builtin::Pow => "pow",
    Builtin::Sqrt => "sqrt",
    Builtin::Math => "math",
    Builtin::Trunc => "trunc",
    Builtin::Sign => "sign",
    Builtin::MinOf => "min-of",
    Builtin::MaxOf => "max-of",
    Builtin::ClassNames => "class-names",
    Builtin::TypeOf => "type-of",
    Builtin::IsArray => "is-array",
    Builtin::IsError => "is-error",
    Builtin::WriteBack => "write-back",
    Builtin::Throw => "throw",
    Builtin::Log => "log",
    Builtin::LogOf => "log-of",
    Builtin::ComponentRef => "component-ref",
    Builtin::IsElement => "is-element",
    Builtin::CloneElement => "clone-element",
    Builtin::DateMs => "date-ms",
    Builtin::DatePart => "date-part",
    Builtin::IsoString => "iso-string",
    Builtin::FromEntries => "from-entries",
    Builtin::Unique => "unique",
    Builtin::HasKey => "has-key",
    Builtin::FormEncode => "form-encode",
    Builtin::FormGet => "form-get",
    Builtin::FormHas => "form-has",
    Builtin::ClassOf => "class-of",
    Builtin::CallMethod => "call-method",
    Builtin::GetMember => "get-member",
    Builtin::FormSet => "form-set",
    Builtin::FormAppend => "form-append",
    Builtin::FormDelete => "form-delete",
    Builtin::UrlParse => "url-parse",
    Builtin::Response => "response",
    Builtin::ResponseJson => "response-json",
    Builtin::ResponseRedirect => "response-redirect",
    Builtin::RedirectTo => "redirect-to",
    Builtin::Uint8Array => "uint8-array",
    Builtin::RequestNew => "request",
    Builtin::ResponseMember => "response-member",
    Builtin::ResponseHeaderGet => "response-header",
    Builtin::ResponseHeaderSet => "response-header-set",
    Builtin::UrlPart => "url-part",
    Builtin::UrlSet => "url-set",
    Builtin::FormGetAll => "form-get-all",
    Builtin::FormEntries => "form-entries",
    Builtin::RegexTest => "regex-test",
    Builtin::Match => "match",
    Builtin::MatchAll => "match-all",
    Builtin::Search => "search",
    Builtin::ReplaceAll => "replace-all",
    Builtin::Checked => "checked",
    Builtin::LooseMatch => "loose-match",
    Builtin::IsNaN => "is-nan",
    Builtin::IsNullish => "nullish?",
    Builtin::ParseInt => "parse-int",
    Builtin::ParseFloat => "parse-float",
    Builtin::Substr => "substr",
    Builtin::IsWellFormed => "is-well-formed",
    Builtin::ToWellFormed => "to-well-formed",
    Builtin::Exec => "exec",
    Builtin::UtcString => "utc-string",
    Builtin::EncodeUri => "encode-full-uri",
    Builtin::DecodeUri => "decode-full-uri",
    Builtin::SameValue => "same-value",
    Builtin::FromCharCode => "from-char-code",
    Builtin::FromCodePoint => "from-code-point",
    Builtin::DateUtc => "date-utc",
    Builtin::ToPrecision => "to-precision",
    Builtin::ToExponential => "to-exponential",
    Builtin::Normalize => "normalize",
    Builtin::ToSpliced => "to-spliced",
    Builtin::ArrayWith => "array-with",
    Builtin::Fill => "fill",
    Builtin::CopyWithin => "copy-within",
    Builtin::Iterate => "iterate",
    Builtin::Spread => "spread",
    Builtin::Splice => "splice",
    Builtin::ExecFrom => "exec-from",
    Builtin::TestFrom => "test-from",
    Builtin::Tagged => "tagged",
    Builtin::TaggedPayload => "tagged-payload",
    Builtin::IsTagged => "is-tagged",
    Builtin::Settle => "settle",
    Builtin::Segments => "segments",
    Builtin::ElementOf => "element-of",
    Builtin::ViewGet => "view-get",
    Builtin::ViewSet => "view-set",
    Builtin::LastIndexOf => "last-index-of",
    Builtin::CharAt => "char-at",
    Builtin::CharCodeAt => "char-code-at",
    Builtin::CodePointAt => "code-point-at",
    Builtin::Radix => "radix",
    Builtin::Graphemes => "graphemes",
    Builtin::SetAt => "set-at",
    Builtin::LooseEq => "loose-eq",
    Builtin::HeapOn => "heap-on",
    Builtin::Overwrite => "overwrite",
    Builtin::CopyDeep => "copy-deep",
    Builtin::MapKey => "map-key",
    Builtin::MapOf => "map-of",
    Builtin::AsMap => "as-map",
    Builtin::AsError => "as-error",
    Builtin::TypedNew => "typed-new",
    Builtin::Is => "is",
    Builtin::UrlTry => "url-try",
    Builtin::SymbolKeys => "symbol-keys",
    Builtin::SymbolNew => "symbol-new",
    Builtin::SymbolDescription => "symbol-description",
    Builtin::SymbolKeyFor => "symbol-key-for",
    Builtin::RegExpEscape => "regexp-escape",
    Builtin::TypedLike => "typed-like",
    Builtin::TypedField => "typed-field",
    Builtin::AsOpaque => "as-opaque",
    Builtin::AsSet => "as-set",
    Builtin::Emit => "emit",
  }
}

pub(crate) fn builtin_of(s: &str) -> Option<Builtin> {
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
    "decode-uri" => Builtin::DecodeUriComponent,
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
    "json-parse" => Builtin::JsonParse,
    "pow" => Builtin::Pow,
    "sqrt" => Builtin::Sqrt,
    "math" => Builtin::Math,
    "trunc" => Builtin::Trunc,
    "sign" => Builtin::Sign,
    "min-of" => Builtin::MinOf,
    "max-of" => Builtin::MaxOf,
    "class-names" => Builtin::ClassNames,
    "type-of" => Builtin::TypeOf,
    "is-array" => Builtin::IsArray,
    "is-error" => Builtin::IsError,
    "write-back" => Builtin::WriteBack,
    "throw" => Builtin::Throw,
    "log" => Builtin::Log,
    "log-of" => Builtin::LogOf,
    "component-ref" => Builtin::ComponentRef,
    "is-element" => Builtin::IsElement,
    "clone-element" => Builtin::CloneElement,
    "date-ms" => Builtin::DateMs,
    "date-part" => Builtin::DatePart,
    "iso-string" => Builtin::IsoString,
    "from-entries" => Builtin::FromEntries,
    "unique" => Builtin::Unique,
    "has-key" => Builtin::HasKey,
    "form-encode" => Builtin::FormEncode,
    "form-get" => Builtin::FormGet,
    "form-has" => Builtin::FormHas,
    "class-of" => Builtin::ClassOf,
    "call-method" => Builtin::CallMethod,
    "get-member" => Builtin::GetMember,
    "form-set" => Builtin::FormSet,
    "form-append" => Builtin::FormAppend,
    "form-delete" => Builtin::FormDelete,
    "url-parse" => Builtin::UrlParse,
    "response" => Builtin::Response,
    "response-json" => Builtin::ResponseJson,
    "response-redirect" => Builtin::ResponseRedirect,
    "redirect-to" => Builtin::RedirectTo,
    "uint8-array" => Builtin::Uint8Array,
    "request" => Builtin::RequestNew,
    "response-member" => Builtin::ResponseMember,
    "response-header" => Builtin::ResponseHeaderGet,
    "response-header-set" => Builtin::ResponseHeaderSet,
    "url-part" => Builtin::UrlPart,
    "url-set" => Builtin::UrlSet,
    "form-get-all" => Builtin::FormGetAll,
    "form-entries" => Builtin::FormEntries,
    "regex-test" => Builtin::RegexTest,
    "match" => Builtin::Match,
    "match-all" => Builtin::MatchAll,
    "search" => Builtin::Search,
    "replace-all" => Builtin::ReplaceAll,
    "checked" => Builtin::Checked,
    "loose-match" => Builtin::LooseMatch,
    "is-nan" => Builtin::IsNaN,
    "nullish?" => Builtin::IsNullish,
    "parse-int" => Builtin::ParseInt,
    "parse-float" => Builtin::ParseFloat,
    "substr" => Builtin::Substr,
    "is-well-formed" => Builtin::IsWellFormed,
    "to-well-formed" => Builtin::ToWellFormed,
    "exec" => Builtin::Exec,
    "utc-string" => Builtin::UtcString,
    "encode-full-uri" => Builtin::EncodeUri,
    "decode-full-uri" => Builtin::DecodeUri,
    "same-value" => Builtin::SameValue,
    "from-char-code" => Builtin::FromCharCode,
    "from-code-point" => Builtin::FromCodePoint,
    "date-utc" => Builtin::DateUtc,
    "to-precision" => Builtin::ToPrecision,
    "to-exponential" => Builtin::ToExponential,
    "normalize" => Builtin::Normalize,
    "to-spliced" => Builtin::ToSpliced,
    "array-with" => Builtin::ArrayWith,
    "fill" => Builtin::Fill,
    "copy-within" => Builtin::CopyWithin,
    "iterate" => Builtin::Iterate,
    "spread" => Builtin::Spread,
    "splice" => Builtin::Splice,
    "tagged" => Builtin::Tagged,
    "tagged-payload" => Builtin::TaggedPayload,
    "is-tagged" => Builtin::IsTagged,
    "settle" => Builtin::Settle,
    "segments" => Builtin::Segments,
    "element-of" => Builtin::ElementOf,
    "view-get" => Builtin::ViewGet,
    "view-set" => Builtin::ViewSet,
    "exec-from" => Builtin::ExecFrom,
    "test-from" => Builtin::TestFrom,
    "last-index-of" => Builtin::LastIndexOf,
    "char-at" => Builtin::CharAt,
    "char-code-at" => Builtin::CharCodeAt,
    "code-point-at" => Builtin::CodePointAt,
    "radix" => Builtin::Radix,
    "graphemes" => Builtin::Graphemes,
    "set-at" => Builtin::SetAt,
    "loose-eq" => Builtin::LooseEq,
    "heap-on" => Builtin::HeapOn,
    "overwrite" => Builtin::Overwrite,
    "copy-deep" => Builtin::CopyDeep,
    "map-key" => Builtin::MapKey,
    "map-of" => Builtin::MapOf,
    "as-map" => Builtin::AsMap,
    "as-error" => Builtin::AsError,
    "typed-new" => Builtin::TypedNew,
    "is" => Builtin::Is,
    "url-try" => Builtin::UrlTry,
    "symbol-keys" => Builtin::SymbolKeys,
    "symbol-new" => Builtin::SymbolNew,
    "symbol-description" => Builtin::SymbolDescription,
    "symbol-key-for" => Builtin::SymbolKeyFor,
    "regexp-escape" => Builtin::RegExpEscape,
    "typed-like" => Builtin::TypedLike,
    "typed-field" => Builtin::TypedField,
    "as-opaque" => Builtin::AsOpaque,
    "as-set" => Builtin::AsSet,
    "emit" => Builtin::Emit,
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

pub(super) fn u32_of(sx: &Sx) -> Res<u32> {
  sx.as_sym()?.parse().map_err(|_| SexprError::shape("expected a number"))
}

/// A form's terms after its head symbol, taken by value, with the head.
pub(super) fn into_form(items: Vec<Sx>, headless: &str) -> Res<(String, std::vec::IntoIter<Sx>)> {
  let mut rest = items.into_iter();
  match rest.next() {
    Some(Sx::Sym(head)) => Ok((head, rest)),
    _ => Err(SexprError::shape(headless)),
  }
}

pub(super) fn exactly(rest: &std::vec::IntoIter<Sx>, head: &str, n: usize) -> Res<()> {
  if rest.len() != n {
    return Err(SexprError::shape(format!("`{head}` takes {n} terms, found {}", rest.len())));
  }
  Ok(())
}

pub(super) fn no_fewer(rest: &std::vec::IntoIter<Sx>, head: &str, n: usize) -> Res<()> {
  if rest.len() < n {
    return Err(SexprError::shape(format!("`{head}` takes at least {n} terms, found {}", rest.len())));
  }
  Ok(())
}

/// The next term of a form whose length was already checked.
pub(super) fn next(rest: &mut std::vec::IntoIter<Sx>) -> Sx {
  rest.next().expect("the form's length was checked")
}

pub(super) fn into_sym(sx: Sx) -> Res<String> {
  match sx {
    Sx::Sym(s) => Ok(s),
    other => Err(SexprError::shape(format!("expected a symbol, found {}", other.kind()))),
  }
}

pub(super) fn into_str(sx: Sx) -> Res<String> {
  match sx {
    Sx::Str(s) => Ok(s),
    other => Err(SexprError::shape(format!("expected a string, found {}", other.kind()))),
  }
}

pub(super) fn into_opt(sx: Sx) -> Res<Option<String>> {
  match sx {
    Sx::Sym(s) if s == "nil" => Ok(None),
    Sx::Str(s) => Ok(Some(s)),
    other => Err(SexprError::shape(format!("expected a string or `nil`, found {}", other.kind()))),
  }
}

/// [`super::read_all`] over terms taken by value, each dropped once read.
pub(super) fn read_each<T>(items: impl ExactSizeIterator<Item = Sx>, mut read: impl FnMut(Sx) -> Res<T>) -> Res<Vec<T>> {
  let mut out = Vec::with_capacity(items.len());
  for item in items {
    out.push(read(item)?);
  }
  Ok(out)
}

