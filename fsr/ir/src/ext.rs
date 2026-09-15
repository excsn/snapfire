//! Extensions: the named synchronous functions an `Expr::Ext` calls, the
//! standard library and whatever a host registers beside it. Each carries a
//! reach: `render` runs on both sides of a render and must agree byte for
//! byte with its browser half; `body` runs on the server only.

use std::collections::BTreeMap;
use std::sync::Arc;

use snapfire_fsr_core::Value;
pub use snapfire_fsr_core::ext::{number, option, text, text_opt, Ambient, Reach};

use crate::interp::Fail;

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

/// The reach of a standard member or `None` when no such member exists.
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
