use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// The element kind of a `Value::TypedArray`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScalarKind {
  I8,
  U8,
  I16,
  U16,
  I32,
  U32,
  I64,
  U64,
  F32,
  F64,
}

impl ScalarKind {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::I8 => "i8",
      Self::U8 => "u8",
      Self::I16 => "i16",
      Self::U16 => "u16",
      Self::I32 => "i32",
      Self::U32 => "u32",
      Self::I64 => "i64",
      Self::U64 => "u64",
      Self::F32 => "f32",
      Self::F64 => "f64",
    }
  }
}

/// The contract type vocabulary. Every variant projects onto exactly one shape
/// of the value model, and the integer widths are the reason a `u64` field
/// cannot be silently truncated at 2^53 on the way to TypeScript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Type {
  Null,
  Bool,
  I32,
  I64,
  I128,
  U32,
  U64,
  U128,
  F32,
  F64,
  Str,
  Bytes,
  Array(ScalarKind),
  Optional(Box<Type>),
  List(Box<Type>),
  Map(Box<Type>),
  Named(String),
}

impl Type {
  pub fn optional(inner: Type) -> Self {
    Self::Optional(Box::new(inner))
  }

  pub fn list(inner: Type) -> Self {
    Self::List(Box::new(inner))
  }

  pub fn map(values: Type) -> Self {
    Self::Map(Box::new(values))
  }

  pub fn named(name: impl Into<String>) -> Self {
    Self::Named(name.into())
  }

  pub fn describe(&self) -> String {
    match self {
      Self::Null => "null".to_owned(),
      Self::Bool => "bool".to_owned(),
      Self::I32 => "i32".to_owned(),
      Self::I64 => "i64".to_owned(),
      Self::I128 => "i128".to_owned(),
      Self::U32 => "u32".to_owned(),
      Self::U64 => "u64".to_owned(),
      Self::U128 => "u128".to_owned(),
      Self::F32 => "f32".to_owned(),
      Self::F64 => "f64".to_owned(),
      Self::Str => "str".to_owned(),
      Self::Bytes => "bytes".to_owned(),
      Self::Array(kind) => format!("array<{}>", kind.as_str()),
      Self::Optional(inner) => format!("optional<{}>", inner.describe()),
      Self::List(inner) => format!("list<{}>", inner.describe()),
      Self::Map(values) => format!("map<str, {}>", values.describe()),
      Self::Named(name) => name.clone(),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
  pub name: String,
  #[serde(rename = "type")]
  pub ty: Type,
}

impl Field {
  pub fn new(name: impl Into<String>, ty: Type) -> Self {
    Self { name: name.into(), ty }
  }
}

/// A union arm. A payloadless arm is how a proto3 or OpenAPI enum lands here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variant {
  pub tag: String,
  #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
  pub payload: Option<Type>,
}

impl Variant {
  pub fn unit(tag: impl Into<String>) -> Self {
    Self { tag: tag.into(), payload: None }
  }

  pub fn with(tag: impl Into<String>, payload: Type) -> Self {
    Self { tag: tag.into(), payload: Some(payload) }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeDef {
  Record { fields: Vec<Field> },
  Union { variants: Vec<Variant> },
}

/// Who a cached answer may be served to. `Private` bypasses the cache for
/// any identified call; `Shared` answers everyone alike; `Subject` keeps one
/// entry per subject.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
  #[default]
  Private,
  Shared,
  Subject,
}

impl Scope {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::Private => "private",
      Self::Shared => "shared",
      Self::Subject => "subject",
    }
  }
}

/// How long a method's answer may be reused, declared by the data owner on
/// the contract: `ttl` and `stale` in the duration spelling (`30s`, `5m`),
/// `tags` the writes that drop it, `scope` who may share it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Freshness {
  pub ttl: String,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub tags: Vec<String>,
  #[serde(default)]
  pub scope: Scope,
  /// A window after `ttl` in which the last answer is served while a refresh
  /// runs behind it; `shared` scope only.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub stale: Option<String>,
}

impl Freshness {
  pub fn ttl(ttl: impl Into<String>) -> Self {
    Self { ttl: ttl.into(), ..Default::default() }
  }

  pub fn tags<I, S>(mut self, tags: I) -> Self
  where
    I: IntoIterator<Item = S>,
    S: Into<String>,
  {
    self.tags = tags.into_iter().map(Into::into).collect();
    self
  }

  pub fn shared(mut self) -> Self {
    self.scope = Scope::Shared;
    self
  }

  pub fn per_subject(mut self) -> Self {
    self.scope = Scope::Subject;
    self
  }

  pub fn stale(mut self, window: impl Into<String>) -> Self {
    self.stale = Some(window.into());
    self
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
  #[serde(default)]
  pub params: Vec<Field>,
  pub returns: Type,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub cache: Option<Freshness>,
  /// The tags a successful call drops.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub writes: Vec<String>,
}

impl Method {
  pub fn new(params: Vec<Field>, returns: Type) -> Self {
    Self { params, returns, cache: None, writes: Vec::new() }
  }

  pub fn cached(mut self, freshness: Freshness) -> Self {
    self.cache = Some(freshness);
    self
  }

  pub fn writes<I, S>(mut self, tags: I) -> Self
  where
    I: IntoIterator<Item = S>,
    S: Into<String>,
  {
    self.writes = tags.into_iter().map(Into::into).collect();
    self
  }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Service {
  #[serde(default)]
  pub methods: IndexMap<String, Method>,
}

impl Service {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn method(mut self, name: impl Into<String>, method: Method) -> Self {
    self.methods.insert(name.into(), method);
    self
  }
}

/// The neutral artifact: what a TS subset extraction, a Rust derive export and
/// a proto or OpenAPI import all produce, and what the TS stubs, the optional
/// Rust traits, the plan-file validation and the runtime marshalling all read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
  #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
  pub types: IndexMap<String, TypeDef>,
  #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
  pub services: IndexMap<String, Service>,
}

impl Contract {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn define(mut self, name: impl Into<String>, def: TypeDef) -> Self {
    self.types.insert(name.into(), def);
    self
  }

  pub fn record(self, name: impl Into<String>, fields: Vec<Field>) -> Self {
    self.define(name, TypeDef::Record { fields })
  }

  pub fn union(self, name: impl Into<String>, variants: Vec<Variant>) -> Self {
    self.define(name, TypeDef::Union { variants })
  }

  pub fn service(mut self, name: impl Into<String>, service: Service) -> Self {
    self.services.insert(name.into(), service);
    self
  }

  pub fn to_json(&self) -> String {
    serde_json::to_string_pretty(self).expect("contract serializes")
  }

  pub fn from_json(source: &str) -> Result<Self, serde_json::Error> {
    serde_json::from_str(source)
  }

  pub fn method(&self, service: &str, method: &str) -> Option<&Method> {
    self.services.get(service)?.methods.get(method)
  }

  /// Takes every type and service of `other`, refusing a name this contract
  /// already defines; `file` names `other` in the error.
  pub fn merge(&mut self, other: Contract, file: &str) -> Result<(), crate::ContractError> {
    for (name, def) in other.types {
      if self.types.contains_key(&name) {
        return Err(crate::ContractError::DuplicateType { name, file: file.to_owned() });
      }
      self.types.insert(name, def);
    }
    for (name, service) in other.services {
      if self.services.contains_key(&name) {
        return Err(crate::ContractError::DuplicateService { name, file: file.to_owned() });
      }
      self.services.insert(name, service);
    }
    Ok(())
  }
}

fn namespace_json(value: &mut serde_json::Value, prefix: &str) {
  match value {
    serde_json::Value::Object(map) => {
      if let Some(serde_json::Value::String(named)) = map.get_mut("named") {
        if !named.starts_with(prefix) {
          *named = format!("{prefix}{named}");
        }
      }
      for child in map.values_mut() {
        namespace_json(child, prefix);
      }
    }
    serde_json::Value::Array(items) => items.iter_mut().for_each(|i| namespace_json(i, prefix)),
    _ => {}
  }
}

impl Contract {
  /// The contract as a site's: every type and service named `<name>:<its
  /// name>`, every named reference following, every cache tag prefixed the
  /// same way unless it starts with `@`, which marks a tag shared with the
  /// shell on purpose.
  pub fn namespaced(&self, name: &str) -> Contract {
    let prefix = format!("{name}:");
    let mut json = serde_json::to_value(self).expect("a contract serializes");
    for table in ["types", "services"] {
      if let Some(serde_json::Value::Object(map)) = json.get_mut(table) {
        let entries: Vec<(String, serde_json::Value)> = std::mem::take(map).into_iter().collect();
        for (key, value) in entries {
          let key = if key.starts_with(&prefix) { key } else { format!("{prefix}{key}") };
          map.insert(key, value);
        }
      }
    }
    namespace_json(&mut json, &prefix);
    if let Some(serde_json::Value::Object(services)) = json.get_mut("services") {
      for service in services.values_mut() {
        for method in service.get_mut("methods").and_then(|m| m.as_object_mut()).into_iter().flat_map(|m| m.values_mut()) {
          for key in ["cache", "writes"] {
            let tags = match key {
              "cache" => method.get_mut("cache").and_then(|c| c.get_mut("tags")),
              _ => method.get_mut("writes"),
            };
            for tag in tags.and_then(|t| t.as_array_mut()).into_iter().flatten() {
              if let serde_json::Value::String(tag) = tag {
                if !tag.starts_with('@') && !tag.starts_with(&prefix) {
                  *tag = format!("{prefix}{tag}");
                }
              }
            }
          }
        }
      }
    }
    serde_json::from_value(json).expect("a namespaced contract deserializes")
  }
}

#[cfg(test)]
mod namespace_tests {
  use super::*;

  #[test]
  fn types_services_references_and_tags_carry_the_name() {
    let contract = Contract::from_json(r#"{
      "types": { "Invoice": { "record": { "fields": [ { "name": "id", "type": "i64" }, { "name": "lines", "type": { "list": { "named": "Line" } } } ] } }, "Line": { "record": { "fields": [] } } },
      "services": { "ledger": { "methods": {
        "list": { "params": [], "returns": { "list": { "named": "Invoice" } }, "cache": { "ttl": "15s", "tags": ["invoices", "@catalog"], "scope": "shared" } },
        "pay": { "params": [ { "name": "invoice", "type": { "named": "Invoice" } } ], "returns": "bool", "writes": ["invoices"] }
      } } }
    }"#).unwrap();
    let json = contract.namespaced("billing").to_json();
    for expected in ["\"billing:Invoice\"", "\"billing:Line\"", "\"billing:ledger\"", "\"named\": \"billing:Line\"", "\"billing:invoices\"", "\"@catalog\""] {
      assert!(json.contains(expected), "missing {expected} in {json}");
    }
    assert!(!json.contains("\"named\": \"Invoice\""), "{json}");
    assert!(!json.contains("\"invoices\""), "{json}");
    let back = Contract::from_json(&json).unwrap();
    assert!(back.services.contains_key("billing:ledger") && back.types.contains_key("billing:Line"));
  }
}
