//! Extensions: the named synchronous functions an `Expr::Ext` calls, the
//! standard library and whatever a host registers beside it. Each carries a
//! reach: `render` runs on both sides of a render and must agree byte for
//! byte with its browser half; `body` runs on the server only.

use std::collections::BTreeMap;
use std::sync::Arc;

use snapfire_fsr_core::Value;

use crate::interp::Fail;

/// Where an extension may run. `Render`: pure, both sides, callable from
/// every site. `Body`: server only, callable from a loader, an action, a
/// handler or middleware, refused on a component's render path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
  Render,
  Body,
}

impl Reach {
  pub fn as_str(&self) -> &'static str {
    match self {
      Reach::Render => "render",
      Reach::Body => "body",
    }
  }
}

/// What a call runs under: the request's locale in the application's
/// spelling, empty when nothing set one, and the clock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ambient {
  pub locale: String,
  pub now: i128,
  /// The message catalogs the host loaded, which `i18n.t` reads; `None`
  /// when the application has none.
  pub catalogs: Option<Arc<crate::catalog::Catalogs>>,
}

impl Ambient {
  /// The locale as BCP 47, `fr-FR` for `fr_FR`; `en` when none is set. The
  /// browser half converts the same way.
  pub fn bcp47(&self) -> String {
    if self.locale.is_empty() { "en".to_owned() } else { self.locale.replace('_', "-") }
  }
}

pub type ExtFn = dyn Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync;

#[derive(Clone)]
pub struct Extension {
  pub reach: Reach,
  f: Arc<ExtFn>,
}

impl Extension {
  pub fn call(&self, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
    (self.f)(ambient, args)
  }
}

/// The standard library by module, member and reach: the table the lowerer
/// checks a call against and the registry fills.
pub const STANDARD: &[(&str, &str, Reach)] = &[
  ("intl", "number", Reach::Render),
  ("intl", "currency", Reach::Render),
  ("intl", "date", Reach::Render),
  ("intl", "plural", Reach::Render),
  ("text", "slug", Reach::Render),
  ("text", "truncate", Reach::Render),
  ("time", "format", Reach::Render),
  ("time", "add", Reach::Render),
  ("time", "diff", Reach::Render),
  ("time", "parse", Reach::Render),
  ("time", "now", Reach::Body),
  ("crypto", "hash", Reach::Render),
  ("crypto", "verify", Reach::Render),
  ("crypto", "random", Reach::Body),
  ("id", "new", Reach::Body),
  ("i18n", "t", Reach::Render),
];

/// The reach of a standard member, or `None` when no such member exists.
pub fn standard_reach(module: &str, name: &str) -> Option<Reach> {
  STANDARD.iter().find(|(m, n, _)| *m == module && *n == name).map(|(_, _, reach)| *reach)
}

/// The extensions an interpreter answers, by `module.name`.
#[derive(Clone, Default)]
pub struct Extensions {
  map: BTreeMap<String, Extension>,
}

impl Extensions {
  /// No extensions at all, not even the standard library.
  pub fn empty() -> Self {
    Self::default()
  }

  /// The standard library.
  pub fn standard() -> Self {
    let mut extensions = Self::empty();
    crate::std::register(&mut extensions);
    extensions
  }

  /// Registers `f` under `name`, `module.member`, replacing what the name held.
  pub fn register<F>(&mut self, name: impl Into<String>, reach: Reach, f: F)
  where
    F: Fn(&Ambient, &[Value]) -> Result<Value, Fail> + Send + Sync + 'static,
  {
    self.map.insert(name.into(), Extension { reach, f: Arc::new(f) });
  }

  pub fn get(&self, name: &str) -> Option<&Extension> {
    self.map.get(name)
  }

  pub fn contains(&self, name: &str) -> bool {
    self.map.contains_key(name)
  }

  /// Every registered name, sorted.
  pub fn names(&self) -> Vec<String> {
    self.map.keys().cloned().collect()
  }

  pub fn call(&self, name: &str, ambient: &Ambient, args: &[Value]) -> Result<Value, Fail> {
    let extension = self.map.get(name).ok_or_else(|| Fail::internal(format!("extension `{name}` is not registered")))?;
    extension.call(ambient, args)
  }
}

impl std::fmt::Debug for Extensions {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_list().entries(self.map.iter().map(|(name, e)| format!("{name} ({})", e.reach.as_str()))).finish()
  }
}

/// A number argument as `f64`; `Int` and `UInt` included, since a BigInt
/// reaches an extension the way it reaches a builtin.
pub fn number(what: &str, args: &[Value], i: usize) -> Result<f64, Fail> {
  match args.get(i) {
    Some(Value::Int(n)) => Ok(*n as f64),
    Some(Value::UInt(n)) => Ok(*n as f64),
    Some(Value::F32(f)) => Ok(*f as f64),
    Some(Value::F64(f)) => Ok(*f),
    Some(other) => Err(crate::interp::type_error(what, "a number", other)),
    None => Err(Fail::internal(format!("{what} takes a number as argument {}", i + 1))),
  }
}

/// A string argument.
pub fn text<'a>(what: &str, args: &'a [Value], i: usize) -> Result<&'a str, Fail> {
  match args.get(i) {
    Some(Value::Str(s)) => Ok(s),
    Some(other) => Err(crate::interp::type_error(what, "a string", other)),
    None => Err(Fail::internal(format!("{what} takes a string as argument {}", i + 1))),
  }
}

/// An optional string argument: absent or `null` is `None`.
pub fn text_opt<'a>(what: &str, args: &'a [Value], i: usize) -> Result<Option<&'a str>, Fail> {
  match args.get(i) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Str(s)) => Ok(Some(s)),
    Some(other) => Err(crate::interp::type_error(what, "a string", other)),
  }
}

/// A field of an optional options object: `None` when the object or the
/// field is absent.
pub fn option<'a>(what: &str, args: &'a [Value], i: usize, field: &str) -> Result<Option<&'a Value>, Fail> {
  match args.get(i) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Map(map)) => Ok(map.get(field).filter(|v| !matches!(v, Value::Null))),
    Some(other) => Err(crate::interp::type_error(what, "an options object", other)),
  }
}
