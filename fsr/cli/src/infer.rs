//! The TypeScript type of what a lowered body returns, for the props a page
//! receives and the value an action resolves to. Every read is typed by its
//! root, so this is a walk over the IR with the contract at hand; anything it
//! cannot settle is `unknown`, never a guess.

use snapfire_fsr_ir::ast::{ArithOp, Body, Builtin, Consts, Entry, Expr, Lit, Stmt};
use snapfire_fsr_service::typescript::{type_name_for, Flavour};
use snapfire_fsr_service::{Contract, Type, TypeDef};

#[derive(Debug, Clone, PartialEq)]
pub enum Ts {
  Str,
  Num,
  Big,
  Bool,
  Null,
  Unknown,
  Named(String),
  /// A TypeScript type expression written as-is, which indexing and field
  /// access extend rather than resolve: the app's own declared type, kept
  /// instead of re-derived from the values a constant happens to hold.
  TsExpr(String),
  List(Box<Ts>),
  Map(Box<Ts>),
  Tuple(Vec<Ts>),
  Record(Vec<(String, Ts)>),
  Union(Vec<Ts>),
  Inter(Vec<Ts>),
}

impl Ts {
  pub fn print(&self, flavour: Flavour) -> String {
    match self {
      Ts::Str => "string".into(),
      Ts::Num => "number".into(),
      Ts::Big => match flavour {
        Flavour::Server => "bigint".into(),
        Flavour::Client => "bigint | number".into(),
      },
      Ts::Bool => "boolean".into(),
      Ts::Null => "null".into(),
      Ts::Unknown => "unknown".into(),
      Ts::Named(n) => n.clone(),
      Ts::TsExpr(t) => t.clone(),
      Ts::List(inner) => {
        let s = inner.print(flavour);
        if s.contains('|') || s.contains('&') { format!("({s})[]") } else { format!("{s}[]") }
      }
      Ts::Map(v) => format!("Record<string, {}>", v.print(flavour)),
      Ts::Tuple(items) => format!("[{}]", items.iter().map(|t| t.print(flavour)).collect::<Vec<_>>().join(", ")),
      Ts::Record(fields) => {
        if fields.is_empty() {
          return "{}".into();
        }
        let inner: Vec<String> = fields.iter().map(|(k, t)| format!("{k}: {}", t.print(flavour))).collect();
        format!("{{ {} }}", inner.join("; "))
      }
      Ts::Union(arms) => arms.iter().map(|t| t.print(flavour)).collect::<Vec<_>>().join(" | "),
      Ts::Inter(parts) => parts
        .iter()
        .map(|t| match t {
          Ts::Union(_) | Ts::Big => format!("({})", t.print(flavour)),
          _ => t.print(flavour),
        })
        .collect::<Vec<_>>()
        .join(" & "),
    }
  }

  fn from_contract(ty: &Type) -> Ts {
    match ty {
      Type::Null => Ts::Null,
      Type::Bool => Ts::Bool,
      Type::I32 | Type::I64 | Type::I128 | Type::U32 | Type::U64 | Type::U128 => Ts::Big,
      Type::F32 | Type::F64 => Ts::Num,
      Type::Str => Ts::Str,
      Type::Optional(inner) => union(vec![Ts::from_contract(inner), Ts::Null]),
      Type::List(inner) => Ts::List(Box::new(Ts::from_contract(inner))),
      Type::Map(v) => Ts::Map(Box::new(Ts::from_contract(v))),
      Type::Named(n) => Ts::Named(n.clone()),
      other => Ts::Named(type_name_for(other, Flavour::Client)),
    }
  }
}

fn union(arms: Vec<Ts>) -> Ts {
  let mut out: Vec<Ts> = Vec::new();
  for arm in arms {
    match arm {
      Ts::Union(inner) => {
        for t in inner {
          if !out.contains(&t) {
            out.push(t);
          }
        }
      }
      t => {
        if !out.contains(&t) {
          out.push(t);
        }
      }
    }
  }
  if out.iter().any(|t| *t == Ts::Unknown) {
    return Ts::Unknown;
  }
  match out.len() {
    0 => Ts::Unknown,
    1 => out.pop().unwrap(),
    _ => Ts::Union(out),
  }
}

fn non_null(t: Ts) -> Ts {
  match t {
    Ts::Union(arms) => union(arms.into_iter().filter(|a| *a != Ts::Null).collect()),
    Ts::Null => Ts::Unknown,
    other => other,
  }
}

pub struct Inferer<'a> {
  pub contract: &'a Contract,
  /// The contract type of `ctx.session`, when a schema declared one.
  pub session: Option<&'a str>,
  /// The contract type of `ctx.input`, for an action.
  pub input: Option<&'a str>,
  /// The type of `input` when it is not a contract type: a store body's
  /// `data`, which is what its loader returned.
  pub input_type: Option<Ts>,
  /// The plan's named constants, so `Expr::Const` types as what it holds
  /// rather than as unknown.
  pub consts: &'a Consts,
}

impl<'a> Inferer<'a> {
  /// The type a body returns: the union of every `return`, or `null` when it
  /// never returns a value.
  pub fn returns(&self, body: &Body) -> Ts {
    let mut env: Vec<(String, Ts)> = Vec::new();
    let mut returns = Vec::new();
    self.block(body, &mut env, &mut returns);
    if returns.is_empty() { Ts::Null } else { union(returns) }
  }

  fn block(&self, body: &Body, env: &mut Vec<(String, Ts)>, returns: &mut Vec<Ts>) {
    let depth = env.len();
    for stmt in body {
      match stmt {
        Stmt::Let { name, expr } => {
          let t = self.expr(expr, env);
          env.push((name.clone(), t));
        }
        Stmt::If { then, r#else, .. } => {
          self.block(then, env, returns);
          self.block(r#else, env, returns);
        }
        Stmt::ForOf { name, over, body } => {
          let elem = element(self.expr(over, env));
          env.push((name.clone(), elem));
          self.block(body, env, returns);
          env.pop();
        }
        Stmt::Return(e) => returns.push(self.expr(e, env)),
        Stmt::Guard { .. } | Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::Expr(_) => {}
      }
    }
    env.truncate(depth);
  }

  fn field_of(&self, target: &Ts, name: &str) -> Ts {
    match target {
      Ts::Named(n) => match self.contract.types.get(n) {
        Some(TypeDef::Record { fields }) => fields
          .iter()
          .find(|f| f.name == name)
          .map(|f| Ts::from_contract(&f.ty))
          .unwrap_or(Ts::Unknown),
        _ => Ts::Unknown,
      },
      Ts::TsExpr(t) => Ts::TsExpr(format!("{t}[\"{name}\"]")),
      Ts::Record(fields) => fields.iter().find(|(k, _)| k == name).map(|(_, t)| t.clone()).unwrap_or(Ts::Unknown),
      Ts::Inter(parts) => parts.iter().map(|p| self.field_of(p, name)).find(|t| *t != Ts::Unknown).unwrap_or(Ts::Unknown),
      Ts::Union(arms) => union(arms.iter().map(|a| if *a == Ts::Null { Ts::Null } else { self.field_of(a, name) }).collect()),
      _ => Ts::Unknown,
    }
  }

  pub fn expr(&self, expr: &Expr, env: &[(String, Ts)]) -> Ts {
    match expr {
      Expr::Const(key) => match const_binding(key) {
        Some(binding) => Ts::TsExpr(binding),
        None => self.consts.get(key).map(|held| self.expr(held, env)).unwrap_or(Ts::Unknown),
      },
      Expr::Param(_) | Expr::Query(_) => Ts::Str,
      Expr::Session(key) => match self.session {
        Some(name) => self.field_of(&Ts::Named(name.to_owned()), key),
        None => Ts::Unknown,
      },
      Expr::Store(_) => Ts::Unknown,
      Expr::Locale => Ts::Str,
      Expr::Identity(path) => match path.first().map(String::as_str) {
        Some("subject") => Ts::Str,
        _ => Ts::Unknown,
      },
      Expr::Input => self.input_type.clone().or_else(|| self.input.map(|n| Ts::Named(n.to_owned()))).unwrap_or(Ts::Unknown),
      Expr::Now => Ts::Big,
      Expr::Var(name) => env.iter().rev().find(|(n, _)| n == name).map(|(_, t)| t.clone()).unwrap_or(Ts::Unknown),
      Expr::Lit(lit) => match lit {
        Lit::Null => Ts::Null,
        Lit::Bool(_) => Ts::Bool,
        Lit::Int(_) => Ts::Big,
        Lit::Float(_) => Ts::Num,
        Lit::Str(_) => Ts::Str,
      },
      Expr::Object(entries) => {
        let mut fields = Vec::new();
        let mut parts = Vec::new();
        let mut computed = None;
        for entry in entries {
          match entry {
            Entry::Field(k, e) => fields.push((k.clone(), self.expr(e, env))),
            Entry::Computed(_, e) => computed = Some(self.expr(e, env)),
            Entry::Spread(e) => parts.push(self.expr(e, env)),
            Entry::Item(_) => {}
          }
        }
        if let Some(v) = computed {
          parts.push(Ts::Map(Box::new(v)));
        }
        if !fields.is_empty() || parts.is_empty() {
          parts.push(Ts::Record(fields));
        }
        if parts.len() == 1 { parts.pop().unwrap() } else { Ts::Inter(parts) }
      }
      Expr::Array(entries) => {
        let items: Vec<Ts> = entries
          .iter()
          .map(|entry| match entry {
            Entry::Item(e) => self.expr(e, env),
            Entry::Spread(e) => element(self.expr(e, env)),
            _ => Ts::Unknown,
          })
          .collect();
        if items.is_empty() { Ts::List(Box::new(Ts::Unknown)) } else { Ts::List(Box::new(union(items))) }
      }
      Expr::Field(target, name) => self.field_of(&self.expr(target, env), name),
      Expr::Index(target, _) => indexed(non_null(self.expr(target, env))),
      Expr::Arith(op, l, r) => {
        let (l, r) = (self.expr(l, env), self.expr(r, env));
        match (op, &l, &r) {
          (ArithOp::Add, Ts::Str, _) | (ArithOp::Add, _, Ts::Str) => Ts::Str,
          _ => non_null(l),
        }
      }
      Expr::Compare(..) | Expr::Not(_) | Expr::Some(..) | Expr::Every(..) => Ts::Bool,
      Expr::Logic(_, l, r) => union(vec![self.expr(l, env), self.expr(r, env)]),
      Expr::Coalesce(l, r) => {
        let left = non_null(self.expr(l, env));
        let right = self.expr(r, env);
        match (&left, &right) {
          (Ts::Map(_) | Ts::Record(_) | Ts::Named(_) | Ts::List(_), Ts::Record(fields)) if fields.is_empty() => left,
          (Ts::List(_), Ts::List(inner)) if **inner == Ts::Unknown => left,
          _ => union(vec![left, right]),
        }
      }
      Expr::Ternary(_, t, e) => union(vec![self.expr(t, env), self.expr(e, env)]),
      Expr::Template(_) | Expr::Str(_) => Ts::Str,
      Expr::Call { service, method, .. } => self
        .contract
        .method(service, method)
        .map(|m| Ts::from_contract(&m.returns))
        .unwrap_or(Ts::Unknown),
      Expr::Lambda { .. } => Ts::Unknown,
      Expr::Hoist { expr, .. } => self.expr(expr, env),
      Expr::Map(over, f) => {
        let elem = element(self.expr(over, env));
        Ts::List(Box::new(self.apply(f, vec![elem], env)))
      }
      Expr::Filter(over, _) => non_null(self.expr(over, env)),
      Expr::Reduce(over, init, f) => {
        let elem = element(self.expr(over, env));
        let init = self.expr(init, env);
        union(vec![init.clone(), self.apply(f, vec![init, elem], env)])
      }
      Expr::Find(over, _) => union(vec![element(self.expr(over, env)), Ts::Null]),
      Expr::Entries(e) => match non_null(self.expr(e, env)) {
        Ts::Map(v) => Ts::List(Box::new(Ts::Tuple(vec![Ts::Str, *v]))),
        _ => Ts::List(Box::new(Ts::Tuple(vec![Ts::Str, Ts::Unknown]))),
      },
      Expr::Keys(_) => Ts::List(Box::new(Ts::Str)),
      Expr::Values(e) => match non_null(self.expr(e, env)) {
        Ts::Map(v) => Ts::List(v),
        _ => Ts::List(Box::new(Ts::Unknown)),
      },
      Expr::Length(_) | Expr::Num(_) | Expr::FindIndex(..) => Ts::Num,
      Expr::BigInt(_) => Ts::Big,
      Expr::Apply { f, args } => {
        let args = args.iter().map(|a| self.expr(a, env)).collect();
        self.apply(f, args, env)
      }
      Expr::Builtin { name, .. } => match name {
        Builtin::Round | Builtin::Floor | Builtin::Ceil | Builtin::Abs | Builtin::Min | Builtin::Max => Ts::Num,
        Builtin::Includes => Ts::Bool,
        Builtin::Range => Ts::List(Box::new(Ts::Big)),
        _ => Ts::Str,
      },
      Expr::Ext { module, name, .. } => match (module.as_str(), name.as_str()) {
        ("time", "add" | "diff" | "parse") => Ts::Num,
        ("time", "now") => Ts::Big,
        ("crypto", "verify") => Ts::Bool,
        _ => Ts::Str,
      },
    }
  }

  fn apply(&self, f: &Expr, args: Vec<Ts>, env: &[(String, Ts)]) -> Ts {
    let Expr::Lambda { params, body } = f else { return Ts::Unknown };
    let mut inner: Vec<(String, Ts)> = env.to_vec();
    for (p, a) in params.iter().zip(args) {
      inner.push((p.clone(), a));
    }
    self.expr(body, &inner)
  }
}

/// `src/docs/guide.ts#CHAPTERS` as the type of that export, written against
/// `generated/`, where the module this describes is emitted. A key naming
/// anything but a file and an export types structurally instead.
fn const_binding(key: &str) -> Option<String> {
  let (file, name) = key.rsplit_once('#')?;
  if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
    return None;
  }
  let module = file.strip_suffix(".tsx").or_else(|| file.strip_suffix(".ts")).unwrap_or(file);
  if module.is_empty() || module.contains('"') {
    return None;
  }
  Some(format!("(typeof import(\"../{module}\"))[\"{name}\"]"))
}

fn indexed(t: Ts) -> Ts {
  match t {
    Ts::Map(v) => *v,
    Ts::List(e) => *e,
    Ts::TsExpr(t) => Ts::TsExpr(format!("{t}[number]")),
    Ts::Union(arms) => union(arms.into_iter().map(indexed).collect()),
    _ => Ts::Unknown,
  }
}

fn element(t: Ts) -> Ts {
  match non_null(t) {
    Ts::List(e) => *e,
    Ts::TsExpr(t) => Ts::TsExpr(format!("{t}[number]")),
    _ => Ts::Unknown,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use snapfire_fsr_service::{Field, Method, Service};

  fn contract() -> Contract {
    Contract::new()
      .record("Product", vec![Field::new("id", Type::I64), Field::new("name", Type::Str)])
      .record("Session", vec![Field::new("cart", Type::map(Type::I64))])
      .service("shop", Service::new().method("list", Method::new(vec![], Type::list(Type::named("Product")))))
  }

  #[test]
  fn a_join_over_the_session_types_its_lines() {
    let c = contract();
    let inferer = Inferer { contract: &c, session: Some("Session"), input: None, input_type: None, consts: &Consts::new() };
    let held = || Expr::Session("cart".into()).index(Expr::Str(Box::new(Expr::var("p").field("id"))));
    let body = vec![
      Stmt::Let { name: "catalog".into(), expr: Expr::call("shop", "list", vec![]) },
      Stmt::Let {
        name: "lines".into(),
        expr: Expr::Map(
          Box::new(Expr::Filter(Box::new(Expr::var("catalog")), Box::new(Expr::lambda(&["p"], held())))),
          Box::new(Expr::lambda(&["p"], Expr::Object(vec![Entry::Spread(Expr::var("p")), Entry::Field("quantity".into(), held())]))),
        ),
      },
      Stmt::Return(Expr::object(vec![("lines", Expr::var("lines"))])),
    ];
    assert_eq!(inferer.returns(&body).print(Flavour::Client), "{ lines: (Product & { quantity: bigint | number })[] }");
  }

  #[test]
  fn what_cannot_be_settled_is_unknown() {
    let c = contract();
    let inferer = Inferer { contract: &c, session: None, input: None, input_type: None, consts: &Consts::new() };
    let body = vec![Stmt::Return(Expr::object(vec![("a", Expr::Session("cart".into())), ("b", Expr::call("nope", "x", vec![]))]))];
    assert_eq!(inferer.returns(&body).print(Flavour::Server), "{ a: unknown; b: unknown }");
  }
}
