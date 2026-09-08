//! Expressions and object entries, both directions.

use crate::ast::{Entry, Expr, Lit, LogicOp};

use super::atoms::*;
use super::syntax::Sx;
use super::{Res, SexprError};

pub fn expr_to_sx(expr: &Expr) -> Sx {
  let one = |head: &str, e: &Expr| form(head, vec![expr_to_sx(e)]);
  let two = |head: &str, a: &Expr, b: &Expr| form(head, vec![expr_to_sx(a), expr_to_sx(b)]);
  match expr {
    Expr::Lit(lit) => lit_to_sx(lit),
    Expr::Var(name) => {
      if reserved_atom(name) {
        form("var", vec![Sx::Sym(name.clone())])
      } else {
        Sx::Sym(name.clone())
      }
    }
    Expr::Param(n) => form("param", vec![Sx::Sym(n.clone())]),
    Expr::Query(n) => form("query", vec![Sx::Sym(n.clone())]),
    Expr::Session(n) => form("session", vec![Sx::Sym(n.clone())]),
    Expr::Store(n) => form("store", vec![Sx::Sym(n.clone())]),
    Expr::Const(n) => form("const", vec![Sx::Sym(n.clone())]),
    Expr::Identity(path) => form("identity", path.iter().map(|p| Sx::Sym(p.clone())).collect()),
    Expr::Locale => form("locale", vec![]),
    Expr::Path => form("path", vec![]),
    Expr::Input => form("input", vec![]),
    Expr::Now => form("now", vec![]),
    Expr::Object(entries) => form("obj", entries.iter().map(entry_to_sx).collect()),
    Expr::Array(entries) => form("arr", entries.iter().map(entry_to_sx).collect()),
    Expr::Field(base, name) => form(".", vec![expr_to_sx(base), Sx::Sym(name.clone())]),
    Expr::Index(base, index) => two("idx", base, index),
    Expr::Arith(op, a, b) => two(arith_sym(*op), a, b),
    Expr::Compare(op, a, b) => two(compare_sym(*op), a, b),
    Expr::Logic(LogicOp::And, a, b) => two("and", a, b),
    Expr::Logic(LogicOp::Or, a, b) => two("or", a, b),
    Expr::Not(e) => one("!", e),
    Expr::Coalesce(a, b) => two("??", a, b),
    Expr::Ternary(c, a, b) => form("if", vec![expr_to_sx(c), expr_to_sx(a), expr_to_sx(b)]),
    Expr::Template(parts) => form("concat", parts.iter().map(expr_to_sx).collect()),
    Expr::Call { service, method, args } => {
      let mut rest = vec![Sx::Sym(service.clone()), Sx::Sym(method.clone())];
      rest.extend(named_args(args));
      form("call", rest)
    }
    Expr::NativeCall { module, method, args, sync } => {
      let mut rest = vec![Sx::Sym(module.clone()), Sx::Sym(method.clone())];
      rest.extend(named_args(args));
      form(if *sync { "native-sync" } else { "native" }, rest)
    }
    Expr::Lambda { params, body } => form(
      "fn",
      vec![Sx::list(params.iter().map(|p| Sx::Sym(p.clone())).collect()), expr_to_sx(body)],
    ),
    Expr::Apply { f, args } => {
      let mut rest = vec![expr_to_sx(f)];
      rest.extend(args.iter().map(expr_to_sx));
      form("apply", rest)
    }
    Expr::Builtin { name, args } => form(builtin_sym(*name), args.iter().map(expr_to_sx).collect()),
    Expr::Ext { module, name, args } => {
      let mut rest = vec![Sx::Sym(module.clone()), Sx::Sym(name.clone())];
      rest.extend(args.iter().map(expr_to_sx));
      form("ext", rest)
    }
    Expr::Map(a, b) => two("map", a, b),
    Expr::Filter(a, b) => two("filter", a, b),
    Expr::Reduce(a, b, c) => form("reduce", vec![expr_to_sx(a), expr_to_sx(b), expr_to_sx(c)]),
    Expr::Find(a, b) => two("find", a, b),
    Expr::FindIndex(a, b) => two("find-index", a, b),
    Expr::Some(a, b) => two("some", a, b),
    Expr::Every(a, b) => two("every", a, b),
    Expr::Entries(e) => one("entries", e),
    Expr::Keys(e) => one("keys", e),
    Expr::Values(e) => one("values", e),
    Expr::Length(e) => one("length", e),
    Expr::Str(e) => one("string", e),
    Expr::Num(e) => one("number", e),
    Expr::BigInt(e) => one("bigint", e),
    Expr::Hoist { id, expr } => form("hoist", vec![Sx::Sym(id.to_string()), expr_to_sx(expr)]),
  }
}

pub(super) fn entry_to_sx(entry: &Entry) -> Sx {
  match entry {
    Entry::Field(name, value) => {
      if ENTRY_MARKERS.contains(&name.as_str()) {
        form("=", vec![Sx::Sym(name.clone()), expr_to_sx(value)])
      } else {
        Sx::list(vec![Sx::Sym(name.clone()), expr_to_sx(value)])
      }
    }
    Entry::Item(value) => form(":", vec![expr_to_sx(value)]),
    Entry::Spread(value) => form("...", vec![expr_to_sx(value)]),
    Entry::Computed(key, value) => form("@", vec![expr_to_sx(key), expr_to_sx(value)]),
  }
}
pub fn expr_from_sx(sx: &Sx) -> Res<Expr> {
  let items = match sx {
    Sx::Str(s) => return Ok(Expr::Lit(Lit::Str(s.clone()))),
    Sx::Sym(s) => return Ok(atom_to_expr(s)),
    Sx::Interp(_) => {
      return Err(SexprError::shape("`{...}` belongs in a template, not an expression"))
    }
    Sx::List(items) => items,
  };
  let head = sx
    .head()
    .ok_or_else(|| SexprError::shape("an expression form starts with a symbol"))?;
  if let Some(op) = arith_of(head) {
    let a = args(items, head, 2)?;
    return Ok(Expr::Arith(op, boxed(&a[0])?, boxed(&a[1])?));
  }
  if let Some(op) = compare_of(head) {
    let a = args(items, head, 2)?;
    return Ok(Expr::Compare(op, boxed(&a[0])?, boxed(&a[1])?));
  }
  let two = |head: &str| -> Res<(Box<Expr>, Box<Expr>)> {
    let a = args(items, head, 2)?;
    Ok((boxed(&a[0])?, boxed(&a[1])?))
  };
  let one = |head: &str| -> Res<Box<Expr>> { boxed(&args(items, head, 1)?[0]) };
  Ok(match head {
    "var" => Expr::Var(sym_of(&args(items, head, 1)?[0])?),
    "param" => Expr::Param(sym_of(&args(items, head, 1)?[0])?),
    "query" => Expr::Query(sym_of(&args(items, head, 1)?[0])?),
    "session" => Expr::Session(sym_of(&args(items, head, 1)?[0])?),
    "store" => Expr::Store(sym_of(&args(items, head, 1)?[0])?),
    "const" => Expr::Const(sym_of(&args(items, head, 1)?[0])?),
    "identity" => Expr::Identity(items[1..].iter().map(sym_of).collect::<Res<_>>()?),
    "locale" => {
      args(items, head, 0)?;
      Expr::Locale
    }
    "path" => {
      args(items, head, 0)?;
      Expr::Path
    }
    "input" => {
      args(items, head, 0)?;
      Expr::Input
    }
    "now" => {
      args(items, head, 0)?;
      Expr::Now
    }
    "obj" => Expr::Object(items[1..].iter().map(entry_from_sx).collect::<Res<_>>()?),
    "arr" => Expr::Array(items[1..].iter().map(entry_from_sx).collect::<Res<_>>()?),
    "." => {
      let a = args(items, head, 2)?;
      Expr::Field(boxed(&a[0])?, sym_of(&a[1])?)
    }
    "idx" => {
      let (a, b) = two(head)?;
      Expr::Index(a, b)
    }
    "and" => {
      let (a, b) = two(head)?;
      Expr::Logic(LogicOp::And, a, b)
    }
    "or" => {
      let (a, b) = two(head)?;
      Expr::Logic(LogicOp::Or, a, b)
    }
    "!" => Expr::Not(one(head)?),
    "??" => {
      let (a, b) = two(head)?;
      Expr::Coalesce(a, b)
    }
    "if" => {
      let a = args(items, head, 3)?;
      Expr::Ternary(boxed(&a[0])?, boxed(&a[1])?, boxed(&a[2])?)
    }
    "concat" => Expr::Template(items[1..].iter().map(expr_from_sx).collect::<Res<_>>()?),
    "call" => {
      let a = at_least(items, head, 2)?;
      Expr::Call { service: sym_of(&a[0])?, method: sym_of(&a[1])?, args: named_args_from(&a[2..])? }
    }
    "native" | "native-sync" => {
      let a = at_least(items, head, 2)?;
      Expr::NativeCall {
        module: sym_of(&a[0])?,
        method: sym_of(&a[1])?,
        args: named_args_from(&a[2..])?,
        sync: head == "native-sync",
      }
    }
    "fn" => {
      let a = args(items, head, 2)?;
      Expr::Lambda {
        params: a[0].as_list()?.iter().map(sym_of).collect::<Res<_>>()?,
        body: boxed(&a[1])?,
      }
    }
    "apply" => {
      let a = at_least(items, head, 1)?;
      Expr::Apply { f: boxed(&a[0])?, args: a[1..].iter().map(expr_from_sx).collect::<Res<_>>()? }
    }
    "ext" => {
      let a = at_least(items, head, 2)?;
      Expr::Ext {
        module: sym_of(&a[0])?,
        name: sym_of(&a[1])?,
        args: a[2..].iter().map(expr_from_sx).collect::<Res<_>>()?,
      }
    }
    "map" => {
      let (a, b) = two(head)?;
      Expr::Map(a, b)
    }
    "filter" => {
      let (a, b) = two(head)?;
      Expr::Filter(a, b)
    }
    "reduce" => {
      let a = args(items, head, 3)?;
      Expr::Reduce(boxed(&a[0])?, boxed(&a[1])?, boxed(&a[2])?)
    }
    "find" => {
      let (a, b) = two(head)?;
      Expr::Find(a, b)
    }
    "find-index" => {
      let (a, b) = two(head)?;
      Expr::FindIndex(a, b)
    }
    "some" => {
      let (a, b) = two(head)?;
      Expr::Some(a, b)
    }
    "every" => {
      let (a, b) = two(head)?;
      Expr::Every(a, b)
    }
    "entries" => Expr::Entries(one(head)?),
    "keys" => Expr::Keys(one(head)?),
    "values" => Expr::Values(one(head)?),
    "length" => Expr::Length(one(head)?),
    "string" => Expr::Str(one(head)?),
    "number" => Expr::Num(one(head)?),
    "bigint" => Expr::BigInt(one(head)?),
    "hoist" => {
      let a = args(items, head, 2)?;
      Expr::Hoist { id: u32_of(&a[0])?, expr: boxed(&a[1])? }
    }
    other => match builtin_of(other) {
      Some(name) => Expr::Builtin {
        name,
        args: items[1..].iter().map(expr_from_sx).collect::<Res<_>>()?,
      },
      None => return Err(SexprError::shape(format!("`{other}` is not an expression form"))),
    },
  })
}

pub(super) fn entry_from_sx(sx: &Sx) -> Res<Entry> {
  let items = sx.as_list()?;
  let head = sx.head().ok_or_else(|| SexprError::shape("an entry starts with a symbol"))?;
  Ok(match head {
    "=" => {
      let a = args(items, head, 2)?;
      Entry::Field(sym_of(&a[0])?, expr_from_sx(&a[1])?)
    }
    ":" => Entry::Item(expr_from_sx(&args(items, head, 1)?[0])?),
    "..." => Entry::Spread(expr_from_sx(&args(items, head, 1)?[0])?),
    "@" => {
      let a = args(items, head, 2)?;
      Entry::Computed(expr_from_sx(&a[0])?, expr_from_sx(&a[1])?)
    }
    name => {
      let a = args(items, name, 1)?;
      Entry::Field(name.to_owned(), expr_from_sx(&a[0])?)
    }
  })
}

pub(super) fn named_args(args: &[(String, Expr)]) -> Vec<Sx> {
  args.iter().map(|(name, value)| Sx::list(vec![Sx::Sym(name.clone()), expr_to_sx(value)])).collect()
}

pub(super) fn named_args_from(items: &[Sx]) -> Res<Vec<(String, Expr)>> {
  items
    .iter()
    .map(|item| {
      let pair = item.as_list()?;
      if pair.len() != 2 {
        return Err(SexprError::shape("a named argument is `(name expr)`"));
      }
      Ok((sym_of(&pair[0])?, expr_from_sx(&pair[1])?))
    })
    .collect()
}

pub(super) fn boxed(sx: &Sx) -> Res<Box<Expr>> {
  expr_from_sx(sx).map(Box::new)
}

pub(super) fn atom_to_expr(s: &str) -> Expr {
  match s {
    "nil" => Expr::Lit(Lit::Null),
    "#t" => Expr::Lit(Lit::Bool(true)),
    "#f" => Expr::Lit(Lit::Bool(false)),
    _ => {
      if let Ok(n) = s.parse::<i128>() {
        Expr::Lit(Lit::Int(n))
      } else if let Ok(f) = s.parse::<f64>() {
        Expr::Lit(Lit::Float(f))
      } else {
        Expr::Var(s.to_owned())
      }
    }
  }
}
