use indexmap::IndexMap;

pub type ValueMap = IndexMap<String, Value>;
pub type Props = ValueMap;

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
