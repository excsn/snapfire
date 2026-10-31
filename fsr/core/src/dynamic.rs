//! Data whose shape a contract does not declare, and the bridge between it and any
//! type with serde derives. The bridge works on `Value` directly, so no JSON is written on the way.

use serde::de::value::{MapDeserializer, SeqDeserializer};
use serde::de::{self, DeserializeOwned, DeserializeSeed, IntoDeserializer, Visitor};
use serde::ser::{self, Serialize};

use crate::Value;

/// In a contract this is `unknown` in TypeScript and `google.protobuf.Value` in gRPC.
#[derive(Debug, Clone, PartialEq)]
pub struct Dynamic(pub Value);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicError(pub String);

impl std::fmt::Display for DynamicError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.0)
  }
}

impl std::error::Error for DynamicError {}

impl ser::Error for DynamicError {
  fn custom<T: std::fmt::Display>(message: T) -> Self {
    DynamicError(message.to_string())
  }
}

impl de::Error for DynamicError {
  fn custom<T: std::fmt::Display>(message: T) -> Self {
    DynamicError(message.to_string())
  }
}

impl Dynamic {
  pub fn from_serde<T: Serialize + ?Sized>(value: &T) -> Result<Self, DynamicError> {
    value.serialize(ValueSerializer).map(Dynamic)
  }

  pub fn to_serde<T: DeserializeOwned>(&self) -> Result<T, DynamicError> {
    T::deserialize(ValueDeserializer(self.0.clone()))
  }

  pub fn from_json(json: &serde_json::Value) -> Self {
    Dynamic(crate::json::to_value(json))
  }

  pub fn to_json(&self) -> Result<serde_json::Value, DynamicError> {
    crate::json::to_json(&self.0).map_err(|error| DynamicError(error.to_string()))
  }
}

impl From<Value> for Dynamic {
  fn from(value: Value) -> Self {
    Dynamic(value)
  }
}

impl From<Dynamic> for Value {
  fn from(dynamic: Dynamic) -> Self {
    dynamic.0
  }
}

fn map_of(fields: Vec<(String, Value)>) -> Value {
  Value::Map(fields.into_iter().collect())
}

fn seq_of(items: Vec<Value>) -> Value {
  Value::Seq(items.into_iter().collect())
}

fn one_field(name: &str, value: Value) -> Value {
  map_of(vec![(name.to_string(), value)])
}

struct ValueSerializer;

struct SeqBuilder(Vec<Value>);

struct MapBuilder {
  fields: Vec<(String, Value)>,
  key: Option<String>,
}

struct VariantSeq {
  name: &'static str,
  items: Vec<Value>,
}

struct VariantMap {
  name: &'static str,
  fields: Vec<(String, Value)>,
}

fn int(value: i128) -> Result<Value, DynamicError> {
  Ok(Value::Int(value))
}

impl ser::Serializer for ValueSerializer {
  type Ok = Value;
  type Error = DynamicError;
  type SerializeSeq = SeqBuilder;
  type SerializeTuple = SeqBuilder;
  type SerializeTupleStruct = SeqBuilder;
  type SerializeTupleVariant = VariantSeq;
  type SerializeMap = MapBuilder;
  type SerializeStruct = MapBuilder;
  type SerializeStructVariant = VariantMap;

  fn serialize_bool(self, v: bool) -> Result<Value, DynamicError> {
    Ok(Value::Bool(v))
  }
  fn serialize_i8(self, v: i8) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_i16(self, v: i16) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_i32(self, v: i32) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_i64(self, v: i64) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_i128(self, v: i128) -> Result<Value, DynamicError> {
    int(v)
  }
  fn serialize_u8(self, v: u8) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_u16(self, v: u16) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_u32(self, v: u32) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_u64(self, v: u64) -> Result<Value, DynamicError> {
    int(v as i128)
  }
  fn serialize_u128(self, v: u128) -> Result<Value, DynamicError> {
    Ok(i128::try_from(v).map_or(Value::UInt(v), Value::Int))
  }
  fn serialize_f32(self, v: f32) -> Result<Value, DynamicError> {
    Ok(Value::F32(v))
  }
  fn serialize_f64(self, v: f64) -> Result<Value, DynamicError> {
    Ok(Value::F64(v))
  }
  fn serialize_char(self, v: char) -> Result<Value, DynamicError> {
    Ok(Value::str(v.to_string()))
  }
  fn serialize_str(self, v: &str) -> Result<Value, DynamicError> {
    Ok(Value::str(v))
  }
  fn serialize_bytes(self, v: &[u8]) -> Result<Value, DynamicError> {
    Ok(Value::Bytes(v.to_vec()))
  }
  fn serialize_none(self) -> Result<Value, DynamicError> {
    Ok(Value::Null)
  }
  fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Value, DynamicError> {
    value.serialize(self)
  }
  fn serialize_unit(self) -> Result<Value, DynamicError> {
    Ok(Value::Null)
  }
  fn serialize_unit_struct(self, _name: &'static str) -> Result<Value, DynamicError> {
    Ok(Value::Null)
  }
  fn serialize_unit_variant(self, _name: &'static str, _index: u32, variant: &'static str) -> Result<Value, DynamicError> {
    Ok(Value::str(variant))
  }
  fn serialize_newtype_struct<T: Serialize + ?Sized>(self, _name: &'static str, value: &T) -> Result<Value, DynamicError> {
    value.serialize(self)
  }
  fn serialize_newtype_variant<T: Serialize + ?Sized>(self, _name: &'static str, _index: u32, variant: &'static str, value: &T) -> Result<Value, DynamicError> {
    Ok(one_field(variant, value.serialize(ValueSerializer)?))
  }
  fn serialize_seq(self, len: Option<usize>) -> Result<SeqBuilder, DynamicError> {
    Ok(SeqBuilder(Vec::with_capacity(len.unwrap_or(0))))
  }
  fn serialize_tuple(self, len: usize) -> Result<SeqBuilder, DynamicError> {
    self.serialize_seq(Some(len))
  }
  fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<SeqBuilder, DynamicError> {
    self.serialize_seq(Some(len))
  }
  fn serialize_tuple_variant(self, _name: &'static str, _index: u32, variant: &'static str, len: usize) -> Result<VariantSeq, DynamicError> {
    Ok(VariantSeq { name: variant, items: Vec::with_capacity(len) })
  }
  fn serialize_map(self, len: Option<usize>) -> Result<MapBuilder, DynamicError> {
    Ok(MapBuilder { fields: Vec::with_capacity(len.unwrap_or(0)), key: None })
  }
  fn serialize_struct(self, _name: &'static str, len: usize) -> Result<MapBuilder, DynamicError> {
    self.serialize_map(Some(len))
  }
  fn serialize_struct_variant(self, _name: &'static str, _index: u32, variant: &'static str, len: usize) -> Result<VariantMap, DynamicError> {
    Ok(VariantMap { name: variant, fields: Vec::with_capacity(len) })
  }
}

impl ser::SerializeSeq for SeqBuilder {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DynamicError> {
    self.0.push(value.serialize(ValueSerializer)?);
    Ok(())
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(seq_of(self.0))
  }
}

impl ser::SerializeTuple for SeqBuilder {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DynamicError> {
    ser::SerializeSeq::serialize_element(self, value)
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(seq_of(self.0))
  }
}

impl ser::SerializeTupleStruct for SeqBuilder {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DynamicError> {
    ser::SerializeSeq::serialize_element(self, value)
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(seq_of(self.0))
  }
}

impl ser::SerializeTupleVariant for VariantSeq {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DynamicError> {
    self.items.push(value.serialize(ValueSerializer)?);
    Ok(())
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(one_field(self.name, seq_of(self.items)))
  }
}

/// A key is text in the value model. A number or a bool writes as its digits or its word.
fn key_text(value: Value) -> Result<String, DynamicError> {
  match value {
    Value::Str(text) => Ok(text.to_string()),
    Value::Int(number) => Ok(number.to_string()),
    Value::UInt(number) => Ok(number.to_string()),
    Value::Bool(flag) => Ok(flag.to_string()),
    other => Err(DynamicError(format!("a map key has to be text, not {other:?}"))),
  }
}

impl ser::SerializeMap for MapBuilder {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), DynamicError> {
    self.key = Some(key_text(key.serialize(ValueSerializer)?)?);
    Ok(())
  }
  fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), DynamicError> {
    let key = self.key.take().ok_or_else(|| DynamicError("a map value came before its key".to_string()))?;
    self.fields.push((key, value.serialize(ValueSerializer)?));
    Ok(())
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(map_of(self.fields))
  }
}

impl ser::SerializeStruct for MapBuilder {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), DynamicError> {
    self.fields.push((key.to_string(), value.serialize(ValueSerializer)?));
    Ok(())
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(map_of(self.fields))
  }
}

impl ser::SerializeStructVariant for VariantMap {
  type Ok = Value;
  type Error = DynamicError;
  fn serialize_field<T: Serialize + ?Sized>(&mut self, key: &'static str, value: &T) -> Result<(), DynamicError> {
    self.fields.push((key.to_string(), value.serialize(ValueSerializer)?));
    Ok(())
  }
  fn end(self) -> Result<Value, DynamicError> {
    Ok(one_field(self.name, map_of(self.fields)))
  }
}

struct ValueDeserializer(Value);

impl<'de> IntoDeserializer<'de, DynamicError> for ValueDeserializer {
  type Deserializer = Self;
  fn into_deserializer(self) -> Self {
    self
  }
}

/// The largest magnitude where every whole number is exactly one float.
const MAX_EXACT_FLOAT: f64 = 9_007_199_254_740_992.0;

impl<'de> de::Deserializer<'de> for ValueDeserializer {
  type Error = DynamicError;

  fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DynamicError> {
    match self.0 {
      Value::Null | Value::Undefined => visitor.visit_unit(),
      Value::Bool(flag) => visitor.visit_bool(flag),
      Value::Int(number) => {
        if let Ok(small) = i64::try_from(number) {
          visitor.visit_i64(small)
        } else if let Ok(large) = u64::try_from(number) {
          visitor.visit_u64(large)
        } else {
          visitor.visit_i128(number)
        }
      }
      Value::UInt(number) => visitor.visit_u128(number),
      // A script's number is a float even when it is whole, and an integer field has to take it.
      // A float field takes an integer too, so nothing is lost by reading a whole one this way.
      Value::F32(number) if number.fract() == 0.0 && (number.abs() as f64) <= MAX_EXACT_FLOAT => visitor.visit_i64(number as i64),
      Value::F64(number) if number.fract() == 0.0 && number.abs() <= MAX_EXACT_FLOAT => visitor.visit_i64(number as i64),
      Value::F32(number) => visitor.visit_f32(number),
      Value::F64(number) => visitor.visit_f64(number),
      Value::Str(text) => visitor.visit_string(text.to_string()),
      Value::Bytes(bytes) => visitor.visit_byte_buf(bytes),
      Value::Seq(items) => {
        let mut access = SeqDeserializer::new(items.iter().cloned().map(ValueDeserializer));
        let read = visitor.visit_seq(&mut access)?;
        access.end()?;
        Ok(read)
      }
      Value::Map(fields) => {
        let present = fields.iter().filter(|(_, value)| !matches!(value, Value::Undefined)).map(|(key, value)| (key.to_string(), ValueDeserializer(value.clone())));
        let mut access = MapDeserializer::new(present);
        let read = visitor.visit_map(&mut access)?;
        access.end()?;
        Ok(read)
      }
      Value::Variant { tag, payload } => visitor.visit_enum(VariantAccess { variant: tag, value: payload.map(|value| *value) }),
      other => Err(DynamicError(format!("{other:?} has no serde form"))),
    }
  }

  fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, DynamicError> {
    match self.0 {
      Value::Null | Value::Undefined => visitor.visit_none(),
      _ => visitor.visit_some(self),
    }
  }

  fn deserialize_newtype_struct<V: Visitor<'de>>(self, _name: &'static str, visitor: V) -> Result<V::Value, DynamicError> {
    visitor.visit_newtype_struct(self)
  }

  fn deserialize_enum<V: Visitor<'de>>(self, _name: &'static str, _variants: &'static [&'static str], visitor: V) -> Result<V::Value, DynamicError> {
    match self.0 {
      Value::Str(text) => visitor.visit_enum(VariantAccess { variant: text.to_string(), value: None }),
      Value::Variant { tag, payload } => visitor.visit_enum(VariantAccess { variant: tag, value: payload.map(|value| *value) }),
      Value::Map(fields) => {
        let mut present = fields.iter();
        match (present.next(), present.next()) {
          (Some((variant, value)), None) => visitor.visit_enum(VariantAccess { variant: variant.to_string(), value: Some(value.clone()) }),
          _ => Err(DynamicError("an enum written as a map has exactly one field".to_string())),
        }
      }
      other => Err(DynamicError(format!("{other:?} is not an enum"))),
    }
  }

  serde::forward_to_deserialize_any! {
    bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string bytes byte_buf unit
    unit_struct seq tuple tuple_struct map struct identifier ignored_any
  }
}

struct VariantAccess {
  variant: String,
  value: Option<Value>,
}

impl<'de> de::EnumAccess<'de> for VariantAccess {
  type Error = DynamicError;
  type Variant = VariantPayload;

  fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, VariantPayload), DynamicError> {
    let variant = seed.deserialize(self.variant.into_deserializer())?;
    Ok((variant, VariantPayload(self.value)))
  }
}

struct VariantPayload(Option<Value>);

impl VariantPayload {
  fn required(self) -> Result<ValueDeserializer, DynamicError> {
    self.0.map(ValueDeserializer).ok_or_else(|| DynamicError("this enum variant needs a value".to_string()))
  }
}

impl<'de> de::VariantAccess<'de> for VariantPayload {
  type Error = DynamicError;

  fn unit_variant(self) -> Result<(), DynamicError> {
    match self.0 {
      None | Some(Value::Null) | Some(Value::Undefined) => Ok(()),
      Some(other) => Err(DynamicError(format!("a unit variant has no value, got {other:?}"))),
    }
  }

  fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, seed: T) -> Result<T::Value, DynamicError> {
    seed.deserialize(self.required()?)
  }

  fn tuple_variant<V: Visitor<'de>>(self, _len: usize, visitor: V) -> Result<V::Value, DynamicError> {
    de::Deserializer::deserialize_any(self.required()?, visitor)
  }

  fn struct_variant<V: Visitor<'de>>(self, _fields: &'static [&'static str], visitor: V) -> Result<V::Value, DynamicError> {
    de::Deserializer::deserialize_any(self.required()?, visitor)
  }
}
