//! The contract an extension is written against: where it may run, what a
//! call runs under, how it fails and the helpers that read its arguments.
//! The registry and the standard library live in `snapfire_fsr_ir`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Value;

/// The failure shapes a UI has to render, so no application re-invents the
/// mapping. Kinds correspond to HTTP statuses at the transport edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
  Unauthorized,
  NotFound,
  Invalid,
  Conflict,
  Timeout,
  Unavailable,
  Internal,
}

impl FailureKind {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Unauthorized => "unauthorized",
      Self::NotFound => "not_found",
      Self::Invalid => "invalid",
      Self::Conflict => "conflict",
      Self::Timeout => "timeout",
      Self::Unavailable => "unavailable",
      Self::Internal => "internal",
    }
  }

  pub fn http_status(&self) -> u16 {
    match self {
      Self::Unauthorized => 401,
      Self::NotFound => 404,
      Self::Invalid => 400,
      Self::Conflict => 409,
      Self::Timeout => 504,
      Self::Unavailable => 503,
      Self::Internal => 500,
    }
  }
}

/// A failed call. Displays as `kind: message`.
#[derive(Debug, Clone, PartialEq)]
pub struct Fail {
  pub kind: FailureKind,
  pub message: String,
}

impl Fail {
  pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
    Self { kind, message: message.into() }
  }

  /// A failure of kind [`FailureKind::Internal`].
  pub fn internal(message: impl Into<String>) -> Self {
    Self::new(FailureKind::Internal, message)
  }
}

impl std::fmt::Display for Fail {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{}: {}", self.kind.as_str(), self.message)
  }
}

impl std::error::Error for Fail {}

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
/// spelling, empty when nothing set one and the clock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ambient {
  pub locale: String,
  pub now: i128,
  /// The message catalogs the host loaded, which `i18n.t` reads; `None`
  /// when the application has none.
  pub catalogs: Option<Arc<Catalogs>>,
}

impl Ambient {
  /// The locale as BCP 47, `fr-FR` for `fr_FR`; `en` when none is set. The
  /// browser half converts the same way.
  pub fn bcp47(&self) -> String {
    if self.locale.is_empty() { "en".to_owned() } else { self.locale.replace('_', "-") }
  }
}

/// One locale's messages, by dotted key.
pub type Table = BTreeMap<String, String>;

/// Message catalogs: one table of dotted keys to strings per locale, which
/// `i18n.t` reads under the ambient locale. Every locale's table is held
/// merged over the default locale's, so a key the locale lacks reads as the
/// default's and the table the browser receives answers exactly as the
/// server does.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Catalogs {
  default: String,
  /// Per locale, the default's table with the locale's own laid over it.
  merged: BTreeMap<String, Arc<Table>>,
  /// `merged` as JSON, the text a document or a payload carries.
  json: BTreeMap<String, Arc<str>>,
  /// Per locale, how many keys its own file held.
  own: BTreeMap<String, usize>,
}

impl Catalogs {
  /// `tables` by locale tag as the application spells it; `default` is the
  /// locale whose table fills in for every other.
  pub fn from_tables(default: impl Into<String>, tables: BTreeMap<String, Table>) -> Self {
    let default = default.into();
    let base = tables.get(&default).cloned().unwrap_or_default();
    let mut merged = BTreeMap::new();
    let mut json = BTreeMap::new();
    let mut own = BTreeMap::new();
    for (tag, table) in tables {
      own.insert(tag.clone(), table.len());
      let mut whole = base.clone();
      whole.extend(table);
      json.insert(tag.clone(), Arc::from(serde_json::to_string(&whole).expect("a string table serialises").as_str()));
      merged.insert(tag, Arc::new(whole));
    }
    Self { default, merged, json, own }
  }

  pub fn is_empty(&self) -> bool {
    self.merged.is_empty()
  }

  pub fn default_tag(&self) -> &str {
    &self.default
  }

  /// Every locale with a table and how many keys its own file held, by tag.
  pub fn rows(&self) -> Vec<(String, usize)> {
    self.own.iter().map(|(tag, n)| (tag.clone(), *n)).collect()
  }

  /// The merged table for `tag` or the default locale's when `tag` has none.
  pub fn table(&self, tag: &str) -> Option<&Arc<Table>> {
    self.merged.get(tag).or_else(|| self.merged.get(&self.default))
  }

  /// `table` as JSON.
  pub fn json(&self, tag: &str) -> Option<Arc<str>> {
    self.json.get(tag).or_else(|| self.json.get(&self.default)).cloned()
  }

  pub fn lookup(&self, tag: &str, key: &str) -> Option<&str> {
    self.table(tag)?.get(key).map(String::as_str)
  }
}

/// The name a failure gives a value's kind.
pub fn kind_name(value: &Value) -> &'static str {
  match value {
    Value::Null => "null",
    Value::Bool(_) => "bool",
    Value::Int(_) => "int",
    Value::UInt(_) => "uint",
    Value::F32(_) | Value::F64(_) => "float",
    Value::Str(_) => "string",
    Value::Bytes(_) => "bytes",
    Value::TypedArray(_) => "typed array",
    Value::Seq(_) => "array",
    Value::Map(_) => "object",
    Value::Variant { .. } => "variant",
    Value::Ref { .. } => "ref",
  }
}

/// An internal failure: `what wants wanted, got kind`.
pub fn type_error(what: &str, wanted: &str, got: &Value) -> Fail {
  Fail::internal(format!("{what} wants {wanted}, got {}", kind_name(got)))
}

/// A number argument as `f64`; `Int` and `UInt` included, since a BigInt
/// reaches an extension the way it reaches a builtin.
pub fn number(what: &str, args: &[Value], i: usize) -> Result<f64, Fail> {
  match args.get(i) {
    Some(Value::Int(n)) => Ok(*n as f64),
    Some(Value::UInt(n)) => Ok(*n as f64),
    Some(Value::F32(f)) => Ok(*f as f64),
    Some(Value::F64(f)) => Ok(*f),
    Some(other) => Err(type_error(what, "a number", other)),
    None => Err(Fail::internal(format!("{what} takes a number as argument {}", i + 1))),
  }
}

/// A string argument.
pub fn text<'a>(what: &str, args: &'a [Value], i: usize) -> Result<&'a str, Fail> {
  match args.get(i) {
    Some(Value::Str(s)) => Ok(s),
    Some(other) => Err(type_error(what, "a string", other)),
    None => Err(Fail::internal(format!("{what} takes a string as argument {}", i + 1))),
  }
}

/// An optional string argument: absent or `null` is `None`.
pub fn text_opt<'a>(what: &str, args: &'a [Value], i: usize) -> Result<Option<&'a str>, Fail> {
  match args.get(i) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Str(s)) => Ok(Some(s)),
    Some(other) => Err(type_error(what, "a string", other)),
  }
}

/// A field of an optional options object: `None` when the object or the
/// field is absent.
pub fn option<'a>(what: &str, args: &'a [Value], i: usize, field: &str) -> Result<Option<&'a Value>, Fail> {
  match args.get(i) {
    None | Some(Value::Null) => Ok(None),
    Some(Value::Map(map)) => Ok(map.get(field).filter(|v| !matches!(v, Value::Null))),
    Some(other) => Err(type_error(what, "an options object", other)),
  }
}
