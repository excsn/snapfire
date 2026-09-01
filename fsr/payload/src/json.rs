use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{json, Map as JsonMap, Value as Json};
use snapfire_fsr_core::{RefKind, TypedArray, Value, ValueMap};
use std::fmt;

/// Largest integer JSON carries without loss, 2^53 - 1.
const JSON_SAFE_INT: i128 = 9_007_199_254_740_991;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(pub String);

impl fmt::Display for DecodeError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "payload json decode: {}", self.0)
  }
}

impl std::error::Error for DecodeError {}

fn err(msg: impl Into<String>) -> DecodeError {
  DecodeError(msg.into())
}

fn tag(name: &str, fields: Vec<(&str, Json)>) -> Json {
  let mut map = JsonMap::new();
  map.insert("$".to_owned(), Json::String(name.to_owned()));
  for (k, v) in fields {
    map.insert(k.to_owned(), v);
  }
  Json::Object(map)
}

fn f64_to_json(v: f64) -> Json {
  if v.is_nan() {
    tag("f", vec![("v", json!("nan"))])
  } else if v == f64::INFINITY {
    tag("f", vec![("v", json!("inf"))])
  } else if v == f64::NEG_INFINITY {
    tag("f", vec![("v", json!("-inf"))])
  } else if v.fract() == 0.0 {
    tag("f", vec![("v", json!(v))])
  } else {
    json!(v)
  }
}

fn typed_array_kind(a: &TypedArray) -> &'static str {
  match a {
    TypedArray::I8(_) => "i8",
    TypedArray::U8(_) => "u8",
    TypedArray::I16(_) => "i16",
    TypedArray::U16(_) => "u16",
    TypedArray::I32(_) => "i32",
    TypedArray::U32(_) => "u32",
    TypedArray::I64(_) => "i64",
    TypedArray::U64(_) => "u64",
    TypedArray::F32(_) => "f32",
    TypedArray::F64(_) => "f64",
  }
}

fn typed_array_le_bytes(a: &TypedArray) -> Vec<u8> {
  macro_rules! bytes {
    ($items:expr) => {
      $items.iter().flat_map(|v| v.to_le_bytes()).collect()
    };
  }
  match a {
    TypedArray::I8(v) => bytes!(v),
    TypedArray::U8(v) => v.clone(),
    TypedArray::I16(v) => bytes!(v),
    TypedArray::U16(v) => bytes!(v),
    TypedArray::I32(v) => bytes!(v),
    TypedArray::U32(v) => bytes!(v),
    TypedArray::I64(v) => bytes!(v),
    TypedArray::U64(v) => bytes!(v),
    TypedArray::F32(v) => bytes!(v),
    TypedArray::F64(v) => bytes!(v),
  }
}

fn typed_array_from_le_bytes(kind: &str, bytes: &[u8]) -> Result<TypedArray, DecodeError> {
  macro_rules! decode {
    ($variant:ident, $ty:ty) => {{
      let width = std::mem::size_of::<$ty>();
      if bytes.len() % width != 0 {
        return Err(err(format!("typed array `{kind}` byte length {} not a multiple of {width}", bytes.len())));
      }
      Ok(TypedArray::$variant(
        bytes.chunks_exact(width).map(|c| <$ty>::from_le_bytes(c.try_into().unwrap())).collect(),
      ))
    }};
  }
  match kind {
    "i8" => decode!(I8, i8),
    "u8" => Ok(TypedArray::U8(bytes.to_vec())),
    "i16" => decode!(I16, i16),
    "u16" => decode!(U16, u16),
    "i32" => decode!(I32, i32),
    "u32" => decode!(U32, u32),
    "i64" => decode!(I64, i64),
    "u64" => decode!(U64, u64),
    "f32" => decode!(F32, f32),
    "f64" => decode!(F64, f64),
    other => Err(err(format!("unknown typed array kind `{other}`"))),
  }
}

pub fn value_to_json(value: &Value) -> Json {
  match value {
    Value::Null => Json::Null,
    Value::Bool(v) => json!(v),
    Value::Int(v) => {
      if (-JSON_SAFE_INT..=JSON_SAFE_INT).contains(v) {
        json!(*v as i64)
      } else {
        tag("i", vec![("v", json!(v.to_string()))])
      }
    }
    Value::UInt(v) => match i128::try_from(*v) {
      Ok(i) => value_to_json(&Value::Int(i)),
      Err(_) => tag("u", vec![("v", json!(v.to_string()))]),
    },
    Value::F32(v) => {
      if v.is_nan() {
        tag("f32", vec![("v", json!("nan"))])
      } else if *v == f32::INFINITY {
        tag("f32", vec![("v", json!("inf"))])
      } else if *v == f32::NEG_INFINITY {
        tag("f32", vec![("v", json!("-inf"))])
      } else {
        tag("f32", vec![("v", json!(*v as f64))])
      }
    }
    Value::F64(v) => f64_to_json(*v),
    Value::Str(v) => json!(v),
    Value::Bytes(v) => tag("b", vec![("v", json!(B64.encode(v)))]),
    Value::TypedArray(a) => tag(
      "ta",
      vec![("k", json!(typed_array_kind(a))), ("v", json!(B64.encode(typed_array_le_bytes(a))))],
    ),
    Value::Seq(items) => Json::Array(items.iter().map(value_to_json).collect()),
    Value::Map(map) => {
      if map.contains_key("$") {
        let entries: Vec<Json> = map.iter().map(|(k, v)| json!([k, value_to_json(v)])).collect();
        tag("m", vec![("v", Json::Array(entries))])
      } else {
        let mut out = JsonMap::new();
        for (k, v) in map {
          out.insert(k.clone(), value_to_json(v));
        }
        Json::Object(out)
      }
    }
    Value::Variant { tag: t, payload } => match payload {
      None => tag("var", vec![("t", json!(t))]),
      Some(p) => tag("var", vec![("t", json!(t)), ("p", value_to_json(p))]),
    },
    Value::Ref { kind, id } => {
      let k = match kind {
        RefKind::Action => "action",
        RefKind::Module => "module",
      };
      tag("ref", vec![("k", json!(k)), ("id", json!(id))])
    }
  }
}

fn field<'a>(map: &'a JsonMap<String, Json>, name: &str, tag: &str) -> Result<&'a Json, DecodeError> {
  map.get(name).ok_or_else(|| err(format!("tag `{tag}` missing field `{name}`")))
}

fn str_field<'a>(map: &'a JsonMap<String, Json>, name: &str, tag: &str) -> Result<&'a str, DecodeError> {
  field(map, name, tag)?.as_str().ok_or_else(|| err(format!("tag `{tag}` field `{name}` must be a string")))
}

fn decode_tagged(name: &str, map: &JsonMap<String, Json>) -> Result<Value, DecodeError> {
  match name {
    "i" => {
      let v: i128 = str_field(map, "v", name)?.parse().map_err(|_| err("tag `i` holds a non-integer"))?;
      Ok(Value::Int(v))
    }
    "u" => {
      let v: u128 = str_field(map, "v", name)?.parse().map_err(|_| err("tag `u` holds a non-integer"))?;
      Ok(Value::uint(v))
    }
    "f" => match field(map, "v", name)? {
      Json::String(s) => match s.as_str() {
        "nan" => Ok(Value::F64(f64::NAN)),
        "inf" => Ok(Value::F64(f64::INFINITY)),
        "-inf" => Ok(Value::F64(f64::NEG_INFINITY)),
        other => Err(err(format!("tag `f` unknown symbol `{other}`"))),
      },
      v => v.as_f64().map(Value::F64).ok_or_else(|| err("tag `f` field `v` must be a number")),
    },
    "f32" => match field(map, "v", name)? {
      Json::String(s) => match s.as_str() {
        "nan" => Ok(Value::F32(f32::NAN)),
        "inf" => Ok(Value::F32(f32::INFINITY)),
        "-inf" => Ok(Value::F32(f32::NEG_INFINITY)),
        other => Err(err(format!("tag `f32` unknown symbol `{other}`"))),
      },
      v => v.as_f64().map(|f| Value::F32(f as f32)).ok_or_else(|| err("tag `f32` field `v` must be a number")),
    },
    "b" => {
      let bytes = B64.decode(str_field(map, "v", name)?).map_err(|_| err("tag `b` holds invalid base64"))?;
      Ok(Value::Bytes(bytes))
    }
    "ta" => {
      let kind = str_field(map, "k", name)?;
      let bytes = B64.decode(str_field(map, "v", name)?).map_err(|_| err("tag `ta` holds invalid base64"))?;
      Ok(Value::TypedArray(typed_array_from_le_bytes(kind, &bytes)?))
    }
    "m" => {
      let entries = field(map, "v", name)?.as_array().ok_or_else(|| err("tag `m` field `v` must be an array"))?;
      let mut out = ValueMap::new();
      for entry in entries {
        let pair = entry.as_array().filter(|p| p.len() == 2).ok_or_else(|| err("tag `m` entries must be pairs"))?;
        let key = pair[0].as_str().ok_or_else(|| err("tag `m` keys must be strings"))?;
        out.insert(key.to_owned(), json_to_value(&pair[1])?);
      }
      Ok(Value::Map(out))
    }
    "var" => {
      let t = str_field(map, "t", name)?.to_owned();
      let payload = match map.get("p") {
        None => None,
        Some(p) => Some(Box::new(json_to_value(p)?)),
      };
      Ok(Value::Variant { tag: t, payload })
    }
    "ref" => {
      let kind = match str_field(map, "k", name)? {
        "action" => RefKind::Action,
        "module" => RefKind::Module,
        other => return Err(err(format!("unknown ref kind `{other}`"))),
      };
      Ok(Value::Ref { kind, id: str_field(map, "id", name)?.to_owned() })
    }
    other => Err(err(format!("unknown tag `{other}`"))),
  }
}

pub fn json_to_value(json: &Json) -> Result<Value, DecodeError> {
  match json {
    Json::Null => Ok(Value::Null),
    Json::Bool(v) => Ok(Value::Bool(*v)),
    Json::Number(n) => {
      if let Some(v) = n.as_i64() {
        Ok(Value::Int(v.into()))
      } else if let Some(v) = n.as_u64() {
        Ok(Value::Int(v.into()))
      } else {
        Ok(Value::F64(n.as_f64().ok_or_else(|| err("unrepresentable number"))?))
      }
    }
    Json::String(v) => Ok(Value::Str(v.clone())),
    Json::Array(items) => {
      let mut out = Vec::with_capacity(items.len());
      for item in items {
        out.push(json_to_value(item)?);
      }
      Ok(Value::Seq(out))
    }
    Json::Object(map) => {
      if let Some(Json::String(name)) = map.get("$") {
        decode_tagged(name.clone().as_str(), map)
      } else {
        let mut out = ValueMap::new();
        for (k, v) in map {
          out.insert(k.clone(), json_to_value(v)?);
        }
        Ok(Value::Map(out))
      }
    }
  }
}
