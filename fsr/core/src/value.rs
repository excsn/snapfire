use indexmap::IndexMap;

/// Keys reach a `ValueMap` from a request body through
/// `snapfire_fsr_payload::json_to_value`, so the hasher is seeded per instance
/// and a caller must not swap in a fixed-state one.
pub type ValueHasher = foldhash::fast::RandomState;

pub type Fields = IndexMap<String, Value, ValueHasher>;

/// Copy on write: cloning shares, and the first `&mut` after a share copies.
/// A render clones a props map at every component and every loop iteration
/// without reading it back, which is what the sharing is for.
#[derive(Clone, Default)]
pub struct ValueMap(std::sync::Arc<Fields>);

pub type Props = ValueMap;

impl ValueMap {
  pub fn into_fields(self) -> Fields {
    std::sync::Arc::try_unwrap(self.0).unwrap_or_else(|held| (*held).clone())
  }
}

impl std::ops::Deref for ValueMap {
  type Target = Fields;
  fn deref(&self) -> &Fields {
    &self.0
  }
}

impl std::ops::DerefMut for ValueMap {
  fn deref_mut(&mut self) -> &mut Fields {
    std::sync::Arc::make_mut(&mut self.0)
  }
}

impl std::fmt::Debug for ValueMap {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    self.0.fmt(f)
  }
}

impl PartialEq for ValueMap {
  fn eq(&self, other: &Self) -> bool {
    std::sync::Arc::ptr_eq(&self.0, &other.0) || *self.0 == *other.0
  }
}

impl From<Fields> for ValueMap {
  fn from(fields: Fields) -> Self {
    ValueMap(std::sync::Arc::new(fields))
  }
}

impl FromIterator<(String, Value)> for ValueMap {
  fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
    ValueMap(std::sync::Arc::new(Fields::from_iter(iter)))
  }
}

impl IntoIterator for ValueMap {
  type Item = (String, Value);
  type IntoIter = indexmap::map::IntoIter<String, Value>;
  fn into_iter(self) -> Self::IntoIter {
    self.into_fields().into_iter()
  }
}

impl<'a> IntoIterator for &'a ValueMap {
  type Item = (&'a String, &'a Value);
  type IntoIter = indexmap::map::Iter<'a, String, Value>;
  fn into_iter(self) -> Self::IntoIter {
    self.0.iter()
  }
}

impl Extend<(String, Value)> for ValueMap {
  fn extend<T: IntoIterator<Item = (String, Value)>>(&mut self, iter: T) {
    std::sync::Arc::make_mut(&mut self.0).extend(iter);
  }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
  Null,
  Bool(bool),
  /// Integers up to i128; unsigned values that fit are normalized here.
  Int(i128),
  /// Only for magnitudes above i128::MAX. Constructors enforce the normalization.
  UInt(u128),
  F32(f32),
  F64(f64),
  Str(String),
  Bytes(Vec<u8>),
  TypedArray(TypedArray),
  Seq(Vec<Value>),
  Map(ValueMap),
  Variant { tag: String, payload: Option<Box<Value>> },
  Ref { kind: RefKind, id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefKind {
  Action,
  Module,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypedArray {
  I8(Vec<i8>),
  U8(Vec<u8>),
  I16(Vec<i16>),
  U16(Vec<u16>),
  I32(Vec<i32>),
  U32(Vec<u32>),
  I64(Vec<i64>),
  U64(Vec<u64>),
  F32(Vec<f32>),
  F64(Vec<f64>),
}

impl Value {
  pub fn int(v: impl Into<i128>) -> Self {
    Value::Int(v.into())
  }

  pub fn uint(v: u128) -> Self {
    match i128::try_from(v) {
      Ok(i) => Value::Int(i),
      Err(_) => Value::UInt(v),
    }
  }

  pub fn str(v: impl Into<String>) -> Self {
    Value::Str(v.into())
  }

  pub fn action_ref(id: impl Into<String>) -> Self {
    Value::Ref { kind: RefKind::Action, id: id.into() }
  }
}

impl From<bool> for Value {
  fn from(v: bool) -> Self {
    Value::Bool(v)
  }
}

impl From<&str> for Value {
  fn from(v: &str) -> Self {
    Value::Str(v.to_owned())
  }
}

impl From<String> for Value {
  fn from(v: String) -> Self {
    Value::Str(v)
  }
}

impl From<i64> for Value {
  fn from(v: i64) -> Self {
    Value::Int(v.into())
  }
}

impl From<u64> for Value {
  fn from(v: u64) -> Self {
    Value::Int(v.into())
  }
}

impl From<f64> for Value {
  fn from(v: f64) -> Self {
    Value::F64(v)
  }
}
