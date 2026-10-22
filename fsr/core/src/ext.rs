//! The contract an extension is written against: where it may run, what a
//! call runs under, how it fails and the helpers that read its arguments.
//! The registry and the standard library live in `snapfire_fsr_ir`.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Value;

/// Defines `FailureKind` from one list, every lookup a `match` the compiler
/// turns into a jump table or a short run of comparisons.
macro_rules! failure_kinds {
  ($($kind:ident = $name:literal, $status:literal;)*) => {
    /// The failure shapes a UI has to render, so no application re-invents the
    /// mapping: one kind per HTTP error status, 4xx for the request's own fault and
    /// 5xx for the server's. `Invalid` is 400, `Unavailable` 503 and `Timeout` 504.
    /// The statuses are the 4xx and 5xx ones the IANA HTTP status code registry
    /// assigns; 418 is reserved there and has no kind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[non_exhaustive]
    pub enum FailureKind {
      $(#[doc = concat!(stringify!($status), ", `", $name, "`.")] $kind,)*
    }

    impl FailureKind {
      /// Every kind, in status order.
      pub const ALL: &'static [FailureKind] = &[$(FailureKind::$kind),*];

      /// The name a body, a payload and `fail(kind, message)` spell it with.
      pub fn as_str(&self) -> &'static str {
        match self { $(FailureKind::$kind => $name,)* }
      }

      pub fn http_status(&self) -> u16 {
        match self { $(FailureKind::$kind => $status,)* }
      }

      /// The kind `name` spells, `None` for a name that is not one.
      pub fn from_name(name: &str) -> Option<FailureKind> {
        match name {
          $($name => Some(FailureKind::$kind),)*
          _ => None,
        }
      }

      /// The kind for an HTTP status: its own, else `Invalid` for a 4xx no kind
      /// names and `Internal` for anything else.
      pub fn from_status(status: u16) -> FailureKind {
        match status {
          $($status => FailureKind::$kind,)*
          400..=499 => FailureKind::Invalid,
          _ => FailureKind::Internal,
        }
      }
    }
  };
}

failure_kinds! {
  Invalid = "invalid", 400;
  Unauthorized = "unauthorized", 401;
  PaymentRequired = "payment_required", 402;
  Forbidden = "forbidden", 403;
  NotFound = "not_found", 404;
  MethodNotAllowed = "method_not_allowed", 405;
  NotAcceptable = "not_acceptable", 406;
  ProxyAuthenticationRequired = "proxy_authentication_required", 407;
  RequestTimeout = "request_timeout", 408;
  Conflict = "conflict", 409;
  Gone = "gone", 410;
  LengthRequired = "length_required", 411;
  PreconditionFailed = "precondition_failed", 412;
  ContentTooLarge = "content_too_large", 413;
  UriTooLong = "uri_too_long", 414;
  UnsupportedMediaType = "unsupported_media_type", 415;
  RangeNotSatisfiable = "range_not_satisfiable", 416;
  ExpectationFailed = "expectation_failed", 417;
  MisdirectedRequest = "misdirected_request", 421;
  UnprocessableContent = "unprocessable_content", 422;
  Locked = "locked", 423;
  FailedDependency = "failed_dependency", 424;
  TooEarly = "too_early", 425;
  UpgradeRequired = "upgrade_required", 426;
  PreconditionRequired = "precondition_required", 428;
  TooManyRequests = "too_many_requests", 429;
  RequestHeaderFieldsTooLarge = "request_header_fields_too_large", 431;
  UnavailableForLegalReasons = "unavailable_for_legal_reasons", 451;
  Internal = "internal", 500;
  NotImplemented = "not_implemented", 501;
  BadGateway = "bad_gateway", 502;
  Unavailable = "unavailable", 503;
  Timeout = "timeout", 504;
  HttpVersionNotSupported = "http_version_not_supported", 505;
  VariantAlsoNegotiates = "variant_also_negotiates", 506;
  InsufficientStorage = "insufficient_storage", 507;
  LoopDetected = "loop_detected", 508;
  NotExtended = "not_extended", 510;
  NetworkAuthenticationRequired = "network_authentication_required", 511;
}

impl FailureKind {
  /// A 4xx: the request's own fault.
  pub fn is_client_error(&self) -> bool {
    (400..500).contains(&self.http_status())
  }

  /// A 5xx: the server's fault.
  pub fn is_server_error(&self) -> bool {
    self.http_status() >= 500
  }
}

/// A failed call. Displays as `kind: message`.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct Fail {
  pub kind: FailureKind,
  pub message: String,
  /// What a `throw` threw, which a `catch` binds; `None` for a failure the interpreter or a call raised.
  pub thrown: Option<Box<Value>>,
}

impl Fail {
  pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
    Self { kind, message: message.into(), thrown: None }
  }

  /// This failure as the throw of `value`.
  pub fn with_thrown(mut self, value: Value) -> Self {
    self.thrown = Some(Box::new(value));
    self
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

/// Where a call may run. `Render`: on the server and in the browser, so it
/// must give the same answer in both. `Body`: on the server only, in a
/// loader, an action, a handler or middleware; refused on a component's
/// render path. `Terminal`: in a program that draws where it is viewed, with
/// no browser half, so a render may read the clock. `Later`: in a program
/// that keeps running after a handler returns, so a timer, an `await` or a
/// listener can run the rest of the handler later. `Process`: in a program
/// that starts processes, reads its environment and owns the screen. A
/// terminal page may use all five. A web page may use `Render` and `Body`
/// only; the build refuses the other three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
  Render,
  Body,
  Terminal,
  Later,
  Process,
}

impl Reach {
  pub fn as_str(&self) -> &'static str {
    match self {
      Reach::Render => "render",
      Reach::Body => "body",
      Reach::Terminal => "terminal",
      Reach::Later => "later",
      Reach::Process => "process",
    }
  }

  /// Why a web page cannot use this reach, finishing "`x`, which ...". `None` for `Render` and `Body`.
  pub fn needs(&self) -> Option<&'static str> {
    match self {
      Reach::Render | Reach::Body => None,
      Reach::Terminal => Some("runs in a terminal page only"),
      Reach::Later => Some("needs a host that resumes a handler after its step"),
      Reach::Process => Some("needs a host with processes of its own"),
    }
  }
}

/// What a call runs under: the request's locale in the application's
/// spelling, empty when nothing set one and the clock.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct Ambient {
  pub locale: String,
  pub now: i128,
  /// The message catalogs the host loaded, which `i18n.t` reads; `None`
  /// when the application has none.
  pub catalogs: Option<Arc<Catalogs>>,
}

impl Ambient {
  pub fn locale(mut self, locale: String) -> Self {
    self.locale = locale;
    self
  }

  pub fn now(mut self, now: i128) -> Self {
    self.now = now;
    self
  }

  pub fn catalogs(mut self, catalogs: Option<Arc<Catalogs>>) -> Self {
    self.catalogs = catalogs;
    self
  }
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
    Value::Undefined => "undefined",
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
    None | Some(Value::Null | Value::Undefined) => Ok(None),
    Some(Value::Str(s)) => Ok(Some(s)),
    Some(other) => Err(type_error(what, "a string", other)),
  }
}

/// A field of an optional options object: `None` when the object or the
/// field is absent.
pub fn option<'a>(what: &str, args: &'a [Value], i: usize, field: &str) -> Result<Option<&'a Value>, Fail> {
  match args.get(i) {
    None | Some(Value::Null | Value::Undefined) => Ok(None),
    Some(Value::Map(map)) => Ok(map.get(field).filter(|v| !matches!(v, Value::Null | Value::Undefined))),
    Some(other) => Err(type_error(what, "an options object", other)),
  }
}
