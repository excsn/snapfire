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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Method {
  #[serde(default)]
  pub params: Vec<Field>,
  pub returns: Type,
}

impl Method {
  pub fn new(params: Vec<Field>, returns: Type) -> Self {
    Self { params, returns }
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
