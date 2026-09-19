use serde::{Deserialize, Serialize};

/// A body is a statement list. A loader body ends in a `return` of an object;
/// an action body may return anything or nothing.
pub type Body = Vec<Stmt>;

/// The module-level constants a plan names, by `<file>#<name>`. A body reads
/// one with `Expr::Const` rather than carrying its own copy.
pub type Consts = std::collections::BTreeMap<String, Expr>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stmt {
  Let { name: String, expr: Expr },
  If { cond: Expr, then: Body, #[serde(default, skip_serializing_if = "Vec::is_empty")] r#else: Body },
  ForOf { name: String, over: Expr, body: Body },
  Return(Expr),
  /// `if (cond) fail(kind, message)`. The kind is a `FailureKind` name; the
  /// message is any expression, evaluated only when the guard fires.
  Guard { cond: Expr, kind: String, message: Expr },
  SessionSet { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr>, value: Expr },
  SessionDelete { key: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] path: Vec<Expr> },
  /// `void save(input)` in a handler, `save` an `action("id")`: the host
  /// dispatches the action when the handler runs in server mode. Only a
  /// handler holds one; a body cannot dispatch an action.
  Act { action: String, input: Expr },
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
  /// The request's locale as the application spells it. Ambient in a render,
  /// the way a store key is.
  Locale,
  /// The path the request matched, without its query. A route that is
  /// prerendered has one, so this is fixed rather than dynamic.
  Path,
  /// The host the request named, `null` unless `[server] hosts` lists it.
  /// Several configured hosts differ between requests, so this is dynamic
  /// where `Path` is not.
  Host,
  /// One `[public]` value, `ctx.config.<key>`. Fixed for the deployment, so
  /// it is constant the way `Path` is rather than per-request the way `Host` is.
  Config(String),
  Input,
  Now,
  Var(String),
  /// A module-level `const` the build named rather than inlined, `<file>#<name>`.
  /// The plan's `consts` table holds it once however many bodies read it.
  Const(String),
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
  /// `await native.<module>.<method>(args)`: the application's own Rust,
  /// in this process. Shaped like `Call` and resolved through a different
  /// handle, since nothing crosses a wire and there is no contract to check
  /// it against.
  NativeCall { module: String, method: String, #[serde(default)] args: Vec<(String, Expr)>, #[serde(default, skip_serializing_if = "std::ops::Not::not")] sync: bool },
  Lambda { params: Vec<String>, body: Box<Expr> },
  /// A lambda applied to arguments: a module-level helper a component calls.
  Apply { f: Box<Expr>, args: Vec<Expr> },
  /// A fixed pure function; the list is `Builtin`.
  Builtin { name: Builtin, args: Vec<Expr> },
  /// An extension call, `intl.number(n)`: a member of a module the
  /// interpreter's registry holds, the standard library or a native pair the
  /// host registered. The locale is ambient.
  Ext { module: String, name: String, args: Vec<Expr> },
  Map(Box<Expr>, Box<Expr>),
  Filter(Box<Expr>, Box<Expr>),
  Reduce(Box<Expr>, Box<Expr>, Box<Expr>),
  Find(Box<Expr>, Box<Expr>),
  FindIndex(Box<Expr>, Box<Expr>),
  Some(Box<Expr>, Box<Expr>),
  Every(Box<Expr>, Box<Expr>),
  Entries(Box<Expr>),
  Keys(Box<Expr>),
  Values(Box<Expr>),
  Length(Box<Expr>),
  Str(Box<Expr>),
  Num(Box<Expr>),
  BigInt(Box<Expr>),
  /// A render-path expression whose inputs are props only: the renderer
  /// records its value under `id` and the enclosing loop indices; the
  /// browser reads that record instead of computing it. `id` is unique
  /// within the component's module.
  Hoist { id: u32, expr: Box<Expr> },
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
  StartsWith,
  EndsWith,
  /// `s.split(sep)`. An empty separator is refused: JavaScript splits one into
  /// UTF-16 code units, which a Rust `String` cannot hold a half of.
  Split,
  /// `s.replace(from, to)`, the first occurrence only, with `$` in `to`
  /// carrying JavaScript's substitutions.
  Replace,
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
  /// `id` is the placement's index among the enclosing component's hoist
  /// candidates, kept so a placement that turns out to be an island carries it.
  /// `keyed` says the placed component keys a hoist or a region somewhere
  /// below it, so the placement extends the key path by `id` on both halves:
  /// two placements of one component then key what it holds apart.
  Component { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl>, #[serde(default, skip_serializing_if = "is_zero")] id: u32, #[serde(default, skip_serializing_if = "is_false")] keyed: bool },
  /// A component placed as its own island: rendered like `Component`, then
  /// wrapped as a nested client node the browser mounts in its own root,
  /// `when` its hydration timing. `id` is the placement's index among the
  /// enclosing component's hoist candidates, which with the module and the
  /// loop path names the region the browser mounts it in.
  /// `define` marks an island whose module is an element definition rather
  /// than a component: the server renders `children` inside the marker as
  /// ordinary markup, the browser imports the module at the island's timing
  /// and the element upgrades itself.
  Island { module: String, #[serde(default, skip_serializing_if = "Vec::is_empty")] props: Vec<Entry>, #[serde(default, skip_serializing_if = "Vec::is_empty")] children: Vec<Tmpl>, #[serde(default, skip_serializing_if = "Option::is_none")] when: Option<String>, #[serde(default, skip_serializing_if = "Option::is_none")] mode: Option<String>, #[serde(default, skip_serializing_if = "is_zero")] id: u32, #[serde(default, skip_serializing_if = "is_false")] define: bool },
  /// The caller's children where the callee places `{children}`, named
  /// `content`; at a layout's root, the plan child of that name, so a
  /// `<Slot name="modal" />` names a second segment beside the page.
  Slot(String),
  /// An element whose open tag is entirely literal, baked into one slice when
  /// the component was prepared. The plan never carries this: it is what
  /// `snapfire_fsr_ir::render::prepare` turns an `Element` into, so the
  /// renderer copies the tag instead of evaluating it. `tag` is `None` for a
  /// void element, whose `open` already closes itself and which has no
  /// children.
  Baked { open: String, tag: Option<String>, children: Vec<Tmpl> },
}

fn is_false(b: &bool) -> bool {
  !*b
}

fn is_zero(n: &u32) -> bool {
  *n == 0
}

/// A lowered component: `let`s run once with `$props` bound, then the tree.
/// `state` names the `let`s the browser can change, `useState` and `useStore`
/// bindings in order; `handlers` are its event handlers as bodies, for an
/// island in server mode, each returning the state it sets. `hydrated_by` is
/// what mounts the component over the server's markup in the browser: `None`
/// for a template with no state and no handlers, which then has no browser
/// twin and pulls no framework into the page. `shadow` is the shadow root an
/// element template declares with its root `<template shadowrootmode>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Component {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub body: Body,
  pub render: Tmpl,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub state: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub handlers: Vec<Handler>,
  #[serde(rename = "hydrate", default = "by_react", skip_serializing_if = "is_by_react", with = "hydrated_as_flag")]
  pub hydrated_by: Option<HydratedBy>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub shadow: Option<ShadowRoot>,
}

/// The declarative shadow root the server writes around an element template.
/// The default is an open root with no options.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowRoot {
  pub mode: ShadowMode,
  #[serde(default, skip_serializing_if = "is_false")]
  pub delegates_focus: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub clonable: bool,
  #[serde(default, skip_serializing_if = "is_false")]
  pub serializable: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShadowMode {
  #[default]
  Open,
  Closed,
}

impl ShadowMode {
  pub fn of(mode: &str) -> Option<Self> {
    match mode {
      "open" => Some(Self::Open),
      "closed" => Some(Self::Closed),
      _ => None,
    }
  }

  pub fn as_str(self) -> &'static str {
    match self {
      Self::Open => "open",
      Self::Closed => "closed",
    }
  }
}

/// What mounts a component over the server's markup. React is the only one:
/// a template is TSX, which runs as React when it needs the browser.
/// `ReactTree` is a layout the React adapter renders as one root with the
/// page inside it, declared as `export default tree(Layout)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HydratedBy {
  React,
  ReactTree,
}

impl HydratedBy {
  /// The word the plan's `(tree)` section and the JSON `hydrate` field use.
  pub const TREE: &'static str = "tree";
}

fn by_react() -> Option<HydratedBy> {
  Some(HydratedBy::React)
}

fn is_by_react(by: &Option<HydratedBy>) -> bool {
  *by == Some(HydratedBy::React)
}

/// `hydrated_by` in the JSON plan as the `hydrate` flag it replaced: `false`
/// for nothing, `true` for React and `"tree"` for a React tree root.
mod hydrated_as_flag {
  use serde::{Deserialize, Deserializer, Serializer};

  use super::HydratedBy;

  #[derive(Deserialize)]
  #[serde(untagged)]
  enum Flag {
    Bool(bool),
    Word(String),
  }

  pub fn serialize<S: Serializer>(by: &Option<HydratedBy>, s: S) -> Result<S::Ok, S::Error> {
    match by {
      Some(HydratedBy::ReactTree) => s.serialize_str(HydratedBy::TREE),
      other => s.serialize_bool(other.is_some()),
    }
  }

  pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<HydratedBy>, D::Error> {
    match Flag::deserialize(d)? {
      Flag::Bool(on) => Ok(on.then_some(HydratedBy::React)),
      Flag::Word(word) if word == HydratedBy::TREE => Ok(Some(HydratedBy::ReactTree)),
      Flag::Word(word) => Err(serde::de::Error::custom(format!("`hydrate` is a boolean or \"tree\", not \"{word}\""))),
    }
  }
}

/// One event handler of a component, lowered: runs with `$props`, `$state`
/// and `$event` bound and returns an object whose keys are the state names
/// it sets. `element` is the attribute marker's id, so the renderer can bind
/// it to its element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Handler {
  pub event: String,
  pub body: Body,
}

impl Component {
  pub fn new(body: Body, render: Tmpl) -> Self {
    Self { body, render, state: Vec::new(), handlers: Vec::new(), hydrated_by: Some(HydratedBy::React), shadow: None }
  }
}

impl Component {
  /// Calls `f` on every expression in the body and the tree.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    body_visit(&self.body, f);
    self.render.visit(f);
  }

  /// True when the component reads `$props.<name>` or uses `$props` whole,
  /// which is how a spread or a rest carries every prop along.
  pub fn reads_prop(&self, name: &str) -> bool {
    let mut aliases = vec!["$props".to_owned()];
    for stmt in &self.body {
      if let Stmt::Let { name, expr: Expr::Var(var) } = stmt {
        if var == "$props" {
          aliases.push(name.clone());
        }
      }
    }
    let mut found = false;
    let mut bare = 0usize;
    let mut wrapped = 0usize;
    self.visit(&mut |expr| match expr {
      Expr::Field(inner, field) if matches!(&**inner, Expr::Var(v) if aliases.contains(v)) => {
        wrapped += 1;
        if field == name {
          found = true;
        }
      }
      Expr::Var(v) if aliases.contains(v) => bare += 1,
      _ => {}
    });
    let defined = aliases.len() - 1;
    found || bare > wrapped + defined
  }
}

/// Calls `f` on every expression a body holds, in statement order.
pub fn body_visit(body: &Body, f: &mut dyn FnMut(&Expr)) {
  for stmt in body {
    match stmt {
      Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.visit(f),
      Stmt::If { cond, then, r#else } => {
        cond.visit(f);
        body_visit(then, f);
        body_visit(r#else, f);
      }
      Stmt::ForOf { over, body, .. } => {
        over.visit(f);
        body_visit(body, f);
      }
      Stmt::Guard { cond, message, .. } => {
        cond.visit(f);
        message.visit(f);
      }
      Stmt::SessionSet { path, value, .. } => {
        path.iter().for_each(|p| p.visit(f));
        value.visit(f);
      }
      Stmt::SessionDelete { path, .. } => path.iter().for_each(|p| p.visit(f)),
      Stmt::Act { input, .. } => input.visit(f),
    }
  }
}

impl Tmpl {
  /// Calls `f` on every expression in the tree, in tree order.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    let entries = |entries: &[Entry], f: &mut dyn FnMut(&Expr)| {
      for entry in entries {
        match entry {
          Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.visit(f),
          Entry::Computed(k, v) => {
            k.visit(f);
            v.visit(f);
          }
        }
      }
    };
    match self {
      Tmpl::Text(_) | Tmpl::Slot(_) => {}
      Tmpl::Expr(e) => e.visit(f),
      Tmpl::Element { attrs, children, .. } => {
        entries(attrs, f);
        children.iter().for_each(|c| c.visit(f));
      }
      Tmpl::Fragment(children) => children.iter().for_each(|c| c.visit(f)),
      Tmpl::If { cond, then, r#else } => {
        cond.visit(f);
        then.visit(f);
        if let Some(other) = r#else {
          other.visit(f);
        }
      }
      Tmpl::For { over, body, .. } => {
        over.visit(f);
        body.visit(f);
      }
      Tmpl::Let { expr, then, .. } => {
        expr.visit(f);
        then.visit(f);
      }
      Tmpl::Component { props, children, .. } | Tmpl::Island { props, children, .. } => {
        entries(props, f);
        children.iter().for_each(|c| c.visit(f));
      }
      Tmpl::Baked { children, .. } => children.iter().for_each(|c| c.visit(f)),
    }
  }
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

  pub fn native_call(module: impl Into<String>, method: impl Into<String>, args: Vec<(&str, Expr)>, sync: bool) -> Self {
    Expr::NativeCall {
      module: module.into(),
      method: method.into(),
      args: args.into_iter().map(|(k, v)| (k.to_owned(), v)).collect(),
      sync,
    }
  }

  pub fn ext(module: impl Into<String>, name: impl Into<String>, args: Vec<Expr>) -> Self {
    Expr::Ext { module: module.into(), name: name.into(), args }
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
      Expr::Const(_) => {}
      Expr::Var(name) => {
        if !out.contains(name) {
          out.push(name.clone());
        }
      }
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Path | Expr::Host | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Lit(_) => {}
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
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
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
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().for_each(|(_, e)| e.free_vars(out)),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().for_each(|e| e.free_vars(out)),
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
      Expr::Hoist { expr, .. } => expr.free_vars(out),
    }
  }

  /// Calls `f` on this expression and every expression beneath it, in tree order.
  pub fn visit(&self, f: &mut dyn FnMut(&Expr)) {
    f(self);
    match self {
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Path | Expr::Host | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) => {}
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().for_each(|(_, e)| e.visit(f)),
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
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
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
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().for_each(|e| e.visit(f)),
      Expr::Apply { f: callee, args } => {
        callee.visit(f);
        args.iter().for_each(|e| e.visit(f));
      }
      Expr::Lambda { body, .. } => body.visit(f),
      Expr::Hoist { expr, .. } => expr.visit(f),
    }
  }

  /// True when the expression reads anything that differs between requests:
  /// a parameter, the query, the session, the identity, the input or the clock.
  /// The locale is not counted: a route reading only it renders once per
  /// configured locale, which is what prerendering does with it. Neither is
  /// the path, which a prerendered route has one of.
  pub fn reads_request(&self) -> bool {
    match self {
      Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Input | Expr::Now | Expr::Host => true,
      Expr::Locale | Expr::Path | Expr::Config(_) => false,
      Expr::Call { args, .. } | Expr::NativeCall { args, .. } => args.iter().any(|(_, e)| e.reads_request()),
      Expr::Var(_) | Expr::Const(_) | Expr::Lit(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.reads_request(),
        Entry::Computed(k, v) => k.reads_request() || v.reads_request(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.reads_request(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => a.reads_request() || b.reads_request(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => a.reads_request() || b.reads_request() || c.reads_request(),
      Expr::Template(parts) => parts.iter().any(Expr::reads_request),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().any(Expr::reads_request),
      Expr::Apply { f, args } => f.reads_request() || args.iter().any(Expr::reads_request),
      Expr::Lambda { body, .. } => body.reads_request(),
      Expr::Hoist { expr, .. } => expr.reads_request(),
    }
  }

  pub fn has_call(&self) -> bool {
    match self {
      Expr::Call { .. } | Expr::NativeCall { .. } => true,
      Expr::Var(_) | Expr::Param(_) | Expr::Query(_) | Expr::Session(_) | Expr::Store(_) | Expr::Identity(_) | Expr::Locale | Expr::Path | Expr::Host | Expr::Config(_) | Expr::Input | Expr::Now | Expr::Const(_) | Expr::Lit(_) => false,
      Expr::Object(entries) | Expr::Array(entries) => entries.iter().any(|entry| match entry {
        Entry::Field(_, e) | Entry::Item(e) | Entry::Spread(e) => e.has_call(),
        Entry::Computed(k, v) => k.has_call() || v.has_call(),
      }),
      Expr::Field(e, _) | Expr::Not(e) | Expr::Entries(e) | Expr::Keys(e) | Expr::Values(e)
      | Expr::Length(e) | Expr::Str(e) | Expr::Num(e) | Expr::BigInt(e) => e.has_call(),
      Expr::Index(a, b) | Expr::Arith(_, a, b) | Expr::Compare(_, a, b) | Expr::Logic(_, a, b)
      | Expr::Coalesce(a, b) | Expr::Map(a, b) | Expr::Filter(a, b) | Expr::Find(a, b) | Expr::FindIndex(a, b)
      | Expr::Some(a, b) | Expr::Every(a, b) => a.has_call() || b.has_call(),
      Expr::Ternary(a, b, c) | Expr::Reduce(a, b, c) => a.has_call() || b.has_call() || c.has_call(),
      Expr::Template(parts) => parts.iter().any(Expr::has_call),
      Expr::Builtin { args, .. } | Expr::Ext { args, .. } => args.iter().any(Expr::has_call),
      Expr::Apply { f, args } => f.has_call() || args.iter().any(Expr::has_call),
      Expr::Lambda { body, .. } => body.has_call(),
      Expr::Hoist { expr, .. } => expr.has_call(),
    }
  }
}

/// The names a body reads that it does not bind itself, in first-read order.
/// A `let` binds for the statements after it and a `for-of` for its own body,
/// the way the interpreter scopes them.
pub fn body_free_vars(body: &Body) -> Vec<String> {
  fn walk(body: &Body, bound: &mut Vec<String>, out: &mut Vec<String>) {
    let depth = bound.len();
    for stmt in body {
      let mut exprs: Vec<&Expr> = Vec::new();
      match stmt {
        Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => exprs.push(expr),
        Stmt::Guard { cond, message, .. } => {
          exprs.push(cond);
          exprs.push(message);
        }
        Stmt::Act { input, .. } => exprs.push(input),
        Stmt::SessionSet { path, value, .. } => {
          exprs.extend(path.iter());
          exprs.push(value);
        }
        Stmt::SessionDelete { path, .. } => exprs.extend(path.iter()),
        Stmt::If { cond, .. } | Stmt::ForOf { over: cond, .. } => exprs.push(cond),
      }
      for expr in exprs {
        let mut free = Vec::new();
        expr.free_vars(&mut free);
        for name in free {
          if !bound.contains(&name) && !out.contains(&name) {
            out.push(name);
          }
        }
      }
      match stmt {
        Stmt::Let { name, .. } => bound.push(name.clone()),
        Stmt::If { then, r#else, .. } => {
          walk(then, bound, out);
          walk(r#else, bound, out);
        }
        Stmt::ForOf { name, body, .. } => {
          bound.push(name.clone());
          walk(body, bound, out);
          bound.truncate(bound.len() - 1);
        }
        _ => {}
      }
    }
    bound.truncate(depth);
  }
  let mut out = Vec::new();
  walk(body, &mut Vec::new(), &mut out);
  out
}

/// True when any statement of the body reads the request or writes the
/// session, so the body's result is not the same for every request.
pub fn body_reads_request(body: &Body) -> bool {
  body.iter().any(|stmt| match stmt {
    Stmt::Let { expr, .. } | Stmt::Return(expr) | Stmt::Expr(expr) => expr.reads_request(),
    Stmt::If { cond, then, r#else } => cond.reads_request() || body_reads_request(then) || body_reads_request(r#else),
    Stmt::ForOf { over, body, .. } => over.reads_request() || body_reads_request(body),
    Stmt::Guard { cond, message, .. } => cond.reads_request() || message.reads_request(),
    Stmt::SessionSet { .. } | Stmt::SessionDelete { .. } | Stmt::Act { .. } => true,
  })
}

/// True when a body reads anything of the request other than its input: a
/// parameter, the query, the session, the identity or the clock. A `meta`
/// body's input is its loader's data, which is not the request and the
/// locale is left out for the reason `reads_request` gives.
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
        Stmt::Guard { cond, message, .. } => {
          into.push(cond);
          into.push(message);
        }
        Stmt::SessionSet { path, value, .. } => {
          into.extend(path.iter());
          into.push(value);
        }
        Stmt::SessionDelete { path, .. } => into.extend(path.iter()),
        Stmt::Act { input, .. } => into.push(input),
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
