use serde::{Deserialize, Serialize};

/// A body is a statement list. A loader body ends in a `return` of an object;
/// an action body may return anything or nothing.
pub type Body = Vec<Stmt>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stmt {
  Let { name: String, expr: Expr },
  If { cond: Expr, then: Body, #[serde(default, skip_serializing_if = "Vec::is_empty")] r#else: Body },
  ForOf { name: String, over: Expr, body: Body },
  Return(Expr),
  /// `if (cond) fail(kind, message)`. The kind is a `FailureKind` name.
  Guard { cond: Expr, kind: String, message: String },
  SessionSet { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr>, value: Expr },
  SessionDelete { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr> },
  Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
  Param(String),
  Query(String),
  Session(String),
  /// A store key, read from the seed the route's `store` exports settled on.
  /// Ambient in a render: a nested component reads it without a prop.
  Store(String),
  Identity(Vec<String>),
  Input,
  Now,
  Var(String),
  Lit(Lit),
  Object(Vec<Entry>),
  Array(Vec<Entry>),
  Field(Box<Expr>, String),
  Index(Box<Expr>, Box<Expr>),
  Arith(ArithOp, Box<Expr>, Box<Expr>),
  Compare(CompareOp, Box<Expr>, Box<Expr>),
  Logic(LogicOp, Box<Expr>, Box<Expr>),
  Not(Box<Expr>),
  Coalesce(Box<Expr>, Box<Expr>),
  Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
  Template(Vec<Expr>),
  /// `await services.<service>.<method>(args)`. An argument whose value is
  /// `null` is omitted, the way an absent optional argument is in TypeScript.
  Call { service: String, method: String, #[serde(default)] args: Vec<(String, Expr)> },
  Lambda { params: Vec<String>, body: Box<Expr> },
  /// A lambda applied to arguments: a module-level helper a component calls.
  Apply { f: Box<Expr>, args: Vec<Expr> },
  /// A fixed pure function; the list is `Builtin`.
  Builtin { name: Builtin, args: Vec<Expr> },
  Map(Box<Expr>, Box<Expr>),
  Filter(Box<Expr>, Box<Expr>),
  Reduce(Box<Expr>, Box<Expr>, Box<Expr>),
  Find(Box<Expr>, Box<Expr>),
  Some(Box<Expr>, Box<Expr>),
  Every(Box<Expr>, Box<Expr>),
  Entries(Box<Expr>),
  Keys(Box<Expr>),
  Values(Box<Expr>),
  Length(Box<Expr>),
  Str(Box<Expr>),
  Num(Box<Expr>),
  BigInt(Box<Expr>),
}

/// The pure functions a component may call by name. Each is one JavaScript
/// method or global with the same semantics for the value model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Builtin {
  Round,
  Floor,
  Ceil,
  Abs,
  Min,
  Max,
  ToFixed,
  Repeat,
  Join,
  Trim,
  Upper,
  Lower,
  Includes,
  EncodeUriComponent,
  LocaleNumber,
  /// `Array.from({ length: n })`: the integers `0..n`.
  Range,
  /// An object without the named keys: the rest of a destructuring.
  Omit,
}

/// A component's render tree. Elements and text are literal; `Expr` is
/// interpolated as text; `If`, `For` and `Let` are the JSX idioms for a
/// ternary, `.map` and a block body; `Component` is another lowered component
/// applied to props.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tmpl {
  Text(String),
  Expr(Expr),
  /// Attributes are `Entry::Field` in HTML spelling or `Entry::Spread` of a map whose keys are React's spelling, later entries winning.
  Element { tag: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] attrs: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl> },
  Fragment(Vec<Tmpl>),
  If { cond: Expr, then: Box<Tmpl>, #[serde(default, skip_serializing_if = "Option::is_none")] r#else: Option<Box<Tmpl>> },
  For { over: Expr, params: Vec<String>, body: Box<Tmpl> },
  Let { name: String, expr: Expr, then: Box<Tmpl> },
  /// Props are `Entry::Field` or `Entry::Spread`; `children` render in the caller's scope wherever the callee places its `Slot`.
  Component { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl> },
  /// A component placed as its own island: rendered like `Component`, then
  /// wrapped as a nested client node the browser mounts in its own root,
  /// `when` its hydration timing.
  Island { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl>, #[serde(default, skip_serializing_if = "Option::is_none")] when: Option<String> },
  /// The caller's children where the callee places `{children}`, named
  /// `content`; at a layout's root, the plan child of that name, so a
  /// `<Slot name="modal" />` names a second segment beside the page.
  Slot(String),
}

/// A lowered component: `let`s run once with `$props` bound, then the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub body: Body,
  pub render: Tmpl,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Entry {
  Field(String, Expr),
  /// `{ [key]: value }`; the key must evaluate to a string.
  Computed(Expr, Expr),
  Item(Expr),
  Spread(Expr),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Lit {
  Null,
  Bool(bool),
  Int(i128),
  Float(f64),
  Str(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArithOp {
  Add,
  Sub,
  Mul,
  Div,
  Rem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareOp {
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogicOp {
  And,
  Or,
}

#[derive(Debug, thiserror::Error)]
#[error("malformed IR: {0}")]
pub struct ParseError(#[from] serde_json::Error);

pub fn from_json(text: &str) -> Result<Body, ParseError> {
  Ok(serde_json::from_str(text)?)
}

pub fn to_json(body: &Body) -> String {
  serde_json::to_string_pretty(body).expect("the IR serialises")
}

impl Expr {
  pub fn lit_str(s: impl Into<String>) -> Self {
    Expr::Lit(Lit::Str(s.into()))
  }

  pub fn lit_int(n: impl Into<i128>) -> Self {
    Expr::Lit(Lit::Int(n.into()))
  }

  pub fn var(name: impl Into<String>) -> Self {
    Expr::Var(name.into())
  }

  pub fn field(self, name: impl Into<String>) -> Self {
    Expr::Field(Box::new(self), name.into())
  }

  pub fn index(self, key: Expr) -> Self {
    Expr::Index(Box::new(self), Box::new(key))
  }

  pub fn call(service: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>) -> Self {
    Expr::Call {
      service: service.into(),
      method: method.into(),
      args: args.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
    }
  }

  pub fn lambda(params: &[&str], body: Expr) -> Self {
    Expr::Lambda { params: params.iter().map(|p| (*p).to_owned()).collect(), body: Box::new(body) }
  }

  pub fn object(entries: Vec<(&str, Expr)>) -> Self {
    Expr::Object(entries.into_iter().map(|(k, v)| Entry::Field(k.to_owned(), v)).collect())
  }

  /// Every `Var` name the expression reads that it does not bind itself.
  pub fn free_vars(&self, out: &mut Vec<String>) {
    match self {
      Expr::Var(name) => {
        if !out.contains(name) {
          out.push(name.clone());
        }
      }
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Input | Expr::Now | Expr::Lit(_) => {}
      Expr::Object(entries) | Expr::Array(entries) => {
        for entry in entries {
          match entry {
            Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.free_vars(out),
            Entry::Computed(k, v) => {
              k.free_vars(out);
              v.free_vars(out);
            }
          }
        }
      }
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.free_vars(out),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => {
        a.free_vars(out);
        b.free_vars(out);
      }
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => {
        a.free_vars(out);
        b.free_vars(out);
        c.free_vars(out);
      }
      Expr::Template(parts) => parts.iter().for_each(|p| p.free_vars(out)),
      Expr::Call { args, .. } => args.iter().for_each(|(_, e)| e.free_vars(out)),
      Expr::Builtin { args, .. } => args.iter().for_each(|e| e.free_vars(out)),
      Expr::Apply { f, args } => {
        f.free_vars(out);
        args.iter().for_each(|e| e.free_vars(out));
      }
      Expr::Lambda { params, body } => {
        let mut inner = Vec::new();
        body.free_vars(&mut inner);
        for name in inner {
          if !params.contains(&name) && !out.contains(&name) {
            out.push(name);
          }
        }
      }
    }
  }

  /// Calls `f` on this expression and every expression beneath it, in tree order.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    f(self);
    match self {
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Lit(_) => {}
      Expr::Call { args, .. } => args.iter().for_each(|(_, e)| e.visit(f)),
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().for_each(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.visit(f),
        Entry::Computed(k, v) => {
          k.visit(f);
          v.visit(f);
        }
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.visit(f),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => {
        a.visit(f);
        b.visit(f);
      }
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => {
        a.visit(f);
        b.visit(f);
        c.visit(f);
      }
      Expr::Template(parts) => parts.iter().for_each(|e| e.visit(f)),
      Expr::Builtin { args, .. } => args.iter().for_each(|e| e.visit(f)),
      Expr::Apply { f: callee, args } => {
        callee.visit(f);
        args.iter().for_each(|e| e.visit(f));
      }
      Expr::Lambda { body, .. } => body.visit(f),
    }
  }

  /// True when the expression reads anything that differs between requests:
  /// a parameter, the query, the session, the identity, the input or the clock.
  pub fn reads_request(&self) -> bool {
    match self {
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Input | Expr::Now => true,
      Expr::Call { args, .. } => args.iter().any(|(_, e)| e.reads_request()),
      Expr::Var(_) | Expr::Lit(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.reads_request(),
        Entry::Computed(k, v) => k.reads_request() || v.reads_request(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.reads_request(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => a.reads_request() || b.reads_request(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => a.reads_request() || b.reads_request() || c.reads_request(),
      Expr::Template(parts) => parts.iter().any(Expr::reads_request),
      Expr::Builtin { args, .. } => args.iter().any(Expr::reads_request),
      Expr::Apply { f, args } => f.reads_request() || args.iter().any(Expr::reads_request),
      Expr::Lambda { body, .. } => body.reads_request(),
    }
  }

  pub fn has_call(&self) -> bool {
    match self {
      Expr::Call { .. } => true,
      Expr::Var(_) | Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Input | Expr::Now | Expr::Lit(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.has_call(),
        Entry::Computed(k, v) => k.has_call() || v.has_call(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.has_call(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => a.has_call() || b.has_call(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => a.has_call() || b.has_call() || c.has_call(),
      Expr::Template(parts) => parts.iter().any(Expr::has_call),
      Expr::Builtin { args, .. } => args.iter().any(Expr::has_call),
      Expr::Apply { f, args } => f.has_call() || args.iter().any(Expr::has_call),
      Expr::Lambda { body, .. } => body.has_call(),
    }
  }
}

/// True when any statement of the body reads the request or writes the
/// session, so the body's result is not the same for every request.
pub fn body_reads_request(body: &Body) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.reads_request(),
    Stmt::If { cond, then, r#else } => cond.reads_request() || body_reads_request(then) || body_reads_request(r#else),
    Stmt::ForOf { over, body, .. } => over.reads_request() || body_reads_request(body),
    Stmt::Guard { cond, .. } => cond.reads_request(),
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } => true,
  })
}

/// True when a body reads anything of the request other than its input: a
/// parameter, the query, the session, the identity or the clock. A `meta`
/// body's input is its loader's data, which is not the request.
pub fn body_reads_ambient(body: &Body) -> bool {
  let mut found = false;
  for expr in body_exprs(body) {
    expr.visit(&mut |e| {
      if matches!(e, Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Now) {
        found = true;
      }
    });
  }
  found
}

/// Every expression a body holds, in order, branches and loops included.
fn body_exprs(body: &Body) -> Vec<&Expr> {
  fn exprs<'a>(body: &'a Body, into: &mut Vec<&'a Expr>) {
    for stmt in body {
      match stmt {
        Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => into.push(expr),
        Stmt::If { cond, then, r#else } => {
          into.push(cond);
          exprs(then, into);
          exprs(r#else, into);
        }
        Stmt::ForOf { over, body, .. } => {
          into.push(over);
          exprs(body, into);
        }
        Stmt::Guard { cond, .. } => into.push(cond),
        Stmt::SessionSet { path, value, .. } => {
          into.extend(path.iter());
          into.push(value);
        }
        Stmt::SessionDelete { path, .. } => into.extend(path.iter()),
      }
    }
  }
  let mut all = Vec::new();
  exprs(body, &mut all);
  all
}

/// The route parameters a body reads, by name, without duplicates.
pub fn body_params_read(body: &Body) -> Vec<String> {
  let mut out = Vec::new();
  for expr in body_exprs(body) {
    expr.visit(&mut |e| {
      if let Expr::Param(name) = e {
        if !out.contains(name) {
          out.push(name.clone());
        }
      }
    });
  }
  out
}
