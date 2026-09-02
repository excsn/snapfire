//! A `Transport` over gRPC: the arguments become the request message through
//! the descriptors `import_proto` kept, the response message becomes a value,
//! both by protobuf's JSON mapping, so no generated code is needed on this
//! side of the wire.

use std::collections::HashMap;
use std::str::FromStr;

use bytes::{Buf, BufMut};
use futures_util::future::BoxFuture;
use parking_lot::Mutex;
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, FieldDescriptor, Kind, MapKey, MessageDescriptor, ReflectMessage, Value as Reflect};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use tonic::codec::{Codec, DecodeBuf, Decoder, EncodeBuf, Encoder};
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Status};

use crate::call::Call;
use crate::proto::{GrpcMethod, ImportedProto};
use crate::transport::Transport;

pub struct GrpcTransport {
  endpoint: Option<Endpoint>,
  /// Opened on the first call, since a channel needs the runtime it will run on.
  channel: Mutex<Option<Channel>>,
  pool: DescriptorPool,
  methods: HashMap<String, GrpcMethod>,
}

impl GrpcTransport {
  /// `base` is the server's URL, `http://host:port` or `https://`; nothing
  /// connects until the first call.
  pub fn new(base: &str, imported: &ImportedProto) -> Result<Self, String> {
    let endpoint = Endpoint::from_shared(base.to_owned()).map_err(|e| e.to_string())?;
    Ok(Self { endpoint: Some(endpoint), channel: Mutex::new(None), pool: imported.pool.clone(), methods: imported.methods.iter().cloned().collect() })
  }

  pub fn with_channel(channel: Channel, imported: &ImportedProto) -> Self {
    Self { endpoint: None, channel: Mutex::new(Some(channel)), pool: imported.pool.clone(), methods: imported.methods.iter().cloned().collect() }
  }

  fn channel(&self) -> Result<Channel, String> {
    let mut slot = self.channel.lock();
    if let Some(channel) = slot.as_ref() {
      return Ok(channel.clone());
    }
    let endpoint = self.endpoint.as_ref().ok_or_else(|| "no endpoint and no channel".to_owned())?;
    let channel = endpoint.connect_lazy();
    *slot = Some(channel.clone());
    Ok(channel)
  }
}

/// Bytes in, bytes out; the messages are encoded before and decoded after.
#[derive(Default)]
struct Raw;

struct RawEncoder;
struct RawDecoder;

impl Encoder for RawEncoder {
  type Item = Vec<u8>;
  type Error = Status;

  fn encode(&mut self, item: Vec<u8>, dst: &mut EncodeBuf<'_>) -> Result<(), Status> {
    dst.put_slice(&item);
    Ok(())
  }
}

impl Decoder for RawDecoder {
  type Item = Vec<u8>;
  type Error = Status;

  fn decode(&mut self, src: &mut DecodeBuf<'_>) -> Result<Option<Vec<u8>>, Status> {
    let mut out = vec![0; src.remaining()];
    src.copy_to_slice(&mut out);
    Ok(Some(out))
  }
}

impl Codec for Raw {
  type Encode = Vec<u8>;
  type Decode = Vec<u8>;
  type Encoder = RawEncoder;
  type Decoder = RawDecoder;

  fn encoder(&mut self) -> RawEncoder {
    RawEncoder
  }

  fn decoder(&mut self) -> RawDecoder {
    RawDecoder
  }
}

pub fn kind_for_code(code: Code) -> FailureKind {
  match code {
    Code::NotFound => FailureKind::NotFound,
    Code::InvalidArgument | Code::OutOfRange | Code::FailedPrecondition => FailureKind::Invalid,
    Code::Unauthenticated | Code::PermissionDenied => FailureKind::Unauthorized,
    Code::AlreadyExists | Code::Aborted => FailureKind::Conflict,
    Code::DeadlineExceeded => FailureKind::Timeout,
    Code::Unavailable => FailureKind::Unavailable,
    _ => FailureKind::Internal,
  }
}

const WRAPPERS: &[&str] = &[
  "google.protobuf.StringValue",
  "google.protobuf.BytesValue",
  "google.protobuf.BoolValue",
  "google.protobuf.Int32Value",
  "google.protobuf.Int64Value",
  "google.protobuf.UInt32Value",
  "google.protobuf.UInt64Value",
  "google.protobuf.FloatValue",
  "google.protobuf.DoubleValue",
];

fn int_of(value: &Value, at: &str) -> Result<i128, String> {
  match value {
    Value::Int(v) => Ok(*v),
    Value::F64(f) if f.fract() == 0.0 => Ok(*f as i128),
    Value::F32(f) if f.fract() == 0.0 => Ok(*f as i128),
    other => Err(format!("{at}: expected an integer, found {other:?}")),
  }
}

fn fit<T: TryFrom<i128>>(value: &Value, at: &str, width: &str) -> Result<T, String> {
  let int = int_of(value, at)?;
  T::try_from(int).map_err(|_| format!("{at}: {int} does not fit {width}"))
}

/// A field's value in prost-reflect's model, by the field's kind.
fn to_reflect(field: &FieldDescriptor, value: &Value, at: &str) -> Result<Option<Reflect>, String> {
  if field.is_map() {
    let Value::Map(entries) = value else { return Err(format!("{at}: expected a map, found {value:?}")) };
    let Kind::Message(entry) = field.kind() else { return Err(format!("{at}: a map field without an entry message")) };
    let value_field = entry.map_entry_value_field();
    let mut out = HashMap::new();
    for (k, v) in entries {
      if let Some(v) = scalar_to_reflect(&value_field.kind(), v, &format!("{at}.{k}"))? {
        out.insert(MapKey::String(k.clone()), v);
      }
    }
    return Ok(Some(Reflect::Map(out)));
  }
  if field.is_list() {
    let Value::Seq(items) = value else { return Err(format!("{at}: expected a list, found {value:?}")) };
    let mut out = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
      if let Some(v) = scalar_to_reflect(&field.kind(), item, &format!("{at}[{i}]"))? {
        out.push(v);
      }
    }
    return Ok(Some(Reflect::List(out)));
  }
  scalar_to_reflect(&field.kind(), value, at)
}

fn scalar_to_reflect(kind: &Kind, value: &Value, at: &str) -> Result<Option<Reflect>, String> {
  if matches!(value, Value::Null) {
    return Ok(None);
  }
  Ok(Some(match kind {
    Kind::Bool => match value {
      Value::Bool(b) => Reflect::Bool(*b),
      other => return Err(format!("{at}: expected a bool, found {other:?}")),
    },
    Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Reflect::I32(fit(value, at, "int32")?),
    Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Reflect::I64(fit(value, at, "int64")?),
    Kind::Uint32 | Kind::Fixed32 => Reflect::U32(fit(value, at, "uint32")?),
    Kind::Uint64 | Kind::Fixed64 => Reflect::U64(fit(value, at, "uint64")?),
    Kind::Float => match value {
      Value::F32(f) => Reflect::F32(*f),
      Value::F64(f) => Reflect::F32(*f as f32),
      Value::Int(i) => Reflect::F32(*i as f32),
      other => return Err(format!("{at}: expected a number, found {other:?}")),
    },
    Kind::Double => match value {
      Value::F64(f) => Reflect::F64(*f),
      Value::F32(f) => Reflect::F64(f64::from(*f)),
      Value::Int(i) => Reflect::F64(*i as f64),
      other => return Err(format!("{at}: expected a number, found {other:?}")),
    },
    Kind::String => match value {
      Value::Str(s) => Reflect::String(s.clone()),
      other => return Err(format!("{at}: expected a string, found {other:?}")),
    },
    Kind::Bytes => match value {
      Value::Bytes(b) => Reflect::Bytes(bytes::Bytes::from(b.clone())),
      other => return Err(format!("{at}: expected bytes, found {other:?}")),
    },
    Kind::Enum(e) => match value {
      Value::Str(name) => {
        let v = e.get_value_by_name(name).ok_or_else(|| format!("{at}: `{name}` is not a value of {}", e.full_name()))?;
        Reflect::EnumNumber(v.number())
      }
      Value::Int(n) => Reflect::EnumNumber(i32::try_from(*n).map_err(|_| format!("{at}: {n} is not an enum number"))?),
      other => return Err(format!("{at}: expected an enum name, found {other:?}")),
    },
    Kind::Message(desc) => match desc.full_name() {
      "google.protobuf.Timestamp" => {
        let Value::Str(text) = value else { return Err(format!("{at}: expected an RFC 3339 timestamp string")) };
        let ts: prost_types::Timestamp = text.parse().map_err(|e| format!("{at}: {e}"))?;
        let mut message = DynamicMessage::new(desc.clone());
        message.transcode_from(&ts).map_err(|e| format!("{at}: {e}"))?;
        Reflect::Message(message)
      }
      "google.protobuf.Duration" => {
        let Value::Str(text) = value else { return Err(format!("{at}: expected a duration string")) };
        let d: prost_types::Duration = text.parse().map_err(|e| format!("{at}: {e}"))?;
        let mut message = DynamicMessage::new(desc.clone());
        message.transcode_from(&d).map_err(|e| format!("{at}: {e}"))?;
        Reflect::Message(message)
      }
      name if WRAPPERS.contains(&name) => {
        let mut message = DynamicMessage::new(desc.clone());
        let inner = desc.get_field_by_name("value").ok_or_else(|| format!("{at}: {name} has no value field"))?;
        if let Some(v) = scalar_to_reflect(&inner.kind(), value, at)? {
          message.set_field(&inner, v);
        }
        Reflect::Message(message)
      }
      _ => {
        let Value::Map(fields) = value else { return Err(format!("{at}: expected a record, found {value:?}")) };
        Reflect::Message(to_message(desc, fields, at)?)
      }
    },
  }))
}

fn to_message(desc: &MessageDescriptor, fields: &ValueMap, at: &str) -> Result<DynamicMessage, String> {
  let mut message = DynamicMessage::new(desc.clone());
  for (name, value) in fields {
    let field = desc.get_field_by_name(name).ok_or_else(|| format!("{at}: {} has no field `{name}`", desc.full_name()))?;
    if let Some(v) = to_reflect(&field, value, &format!("{at}.{name}"))? {
      message.set_field(&field, v);
    }
  }
  Ok(message)
}

fn from_reflect(kind: &Kind, value: &Reflect) -> Result<Value, String> {
  Ok(match value {
    Reflect::Bool(b) => Value::Bool(*b),
    Reflect::I32(v) => Value::int(*v),
    Reflect::I64(v) => Value::int(*v),
    Reflect::U32(v) => Value::int(*v),
    Reflect::U64(v) => Value::int(*v),
    Reflect::F32(v) => Value::F32(*v),
    Reflect::F64(v) => Value::F64(*v),
    Reflect::String(s) => Value::Str(s.clone()),
    Reflect::Bytes(b) => Value::Bytes(b.to_vec()),
    Reflect::EnumNumber(n) => match kind {
      Kind::Enum(e) => match e.get_value(*n) {
        Some(v) => Value::Str(v.name().to_owned()),
        None => Value::int(*n),
      },
      _ => Value::int(*n),
    },
    Reflect::Message(m) => from_message(m)?,
    Reflect::List(items) => Value::Seq(items.iter().map(|i| from_reflect(kind, i)).collect::<Result<_, _>>()?),
    Reflect::Map(entries) => {
      let Kind::Message(entry) = kind else { return Err("a map value without an entry message".to_owned()) };
      let value_kind = entry.map_entry_value_field().kind();
      let mut out = ValueMap::new();
      let mut pairs: Vec<(String, &Reflect)> = entries.iter().map(|(k, v)| (map_key(k), v)).collect();
      pairs.sort_by(|a, b| a.0.cmp(&b.0));
      for (k, v) in pairs {
        out.insert(k, from_reflect(&value_kind, v)?);
      }
      Value::Map(out)
    }
  })
}

fn map_key(key: &MapKey) -> String {
  match key {
    MapKey::String(s) => s.clone(),
    MapKey::Bool(b) => b.to_string(),
    MapKey::I32(v) => v.to_string(),
    MapKey::I64(v) => v.to_string(),
    MapKey::U32(v) => v.to_string(),
    MapKey::U64(v) => v.to_string(),
  }
}

/// A message as a record: every field present, an unset message or
/// presence-tracking field as null, a Timestamp or Duration as its string.
pub fn from_message(message: &DynamicMessage) -> Result<Value, String> {
  let desc = message.descriptor();
  match desc.full_name() {
    "google.protobuf.Timestamp" => {
      let ts: prost_types::Timestamp = message.transcode_to().map_err(|e| e.to_string())?;
      return Ok(Value::Str(ts.to_string()));
    }
    "google.protobuf.Duration" => {
      let d: prost_types::Duration = message.transcode_to().map_err(|e| e.to_string())?;
      return Ok(Value::Str(d.to_string()));
    }
    name if WRAPPERS.contains(&name) => {
      let inner = desc.get_field_by_name("value").ok_or_else(|| format!("{name} has no value field"))?;
      return from_reflect(&inner.kind(), &message.get_field(&inner));
    }
    _ => {}
  }
  let mut out = ValueMap::new();
  for field in desc.fields() {
    let unset = !message.has_field(&field);
    let nullable = matches!(field.kind(), Kind::Message(_)) || field.supports_presence() || field.containing_oneof().is_some();
    let value = if unset && nullable && !field.is_list() && !field.is_map() { Value::Null } else { from_reflect(&field.kind(), &message.get_field(&field))? };
    out.insert(field.name().to_owned(), value);
  }
  Ok(Value::Map(out))
}

/// The arguments as the request message.
pub fn encode_request(descriptor: &MessageDescriptor, args: &ValueMap) -> Result<Vec<u8>, String> {
  Ok(to_message(descriptor, args, descriptor.full_name())?.encode_to_vec())
}

/// The response message as a value.
pub fn decode_response(descriptor: &MessageDescriptor, bytes: &[u8]) -> Result<Value, String> {
  let message = DynamicMessage::decode(descriptor.clone(), bytes).map_err(|e| e.to_string())?;
  from_message(&message)
}

impl Transport for GrpcTransport {
  fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>> {
    let key = format!("{}.{}", call.service, call.method);
    let service = call.service.clone();
    let name = call.method.clone();
    let fail = move |kind: FailureKind, message: String| ServiceError::new(kind, service.clone(), name.clone(), message);

    let Some(method) = self.methods.get(&key).cloned() else {
      return Box::pin(async move { Err(fail(FailureKind::NotFound, format!("no gRPC method for `{key}`"))) });
    };
    let (Some(input), Some(output)) = (self.pool.get_message_by_name(&method.input), self.pool.get_message_by_name(&method.output)) else {
      return Box::pin(async move { Err(fail(FailureKind::Internal, format!("`{key}` names messages the pool does not hold"))) });
    };
    let body = match encode_request(&input, &call.args) {
      Ok(body) => body,
      Err(e) => return Box::pin(async move { Err(fail(FailureKind::Invalid, e)) }),
    };
    let mut request = Request::new(body);
    for (k, v) in &call.metadata {
      if let Value::Str(v) = v {
        if let (Ok(key), Ok(value)) = (tonic::metadata::MetadataKey::from_bytes(k.as_bytes()), v.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>()) {
          request.metadata_mut().insert(key, value);
        }
      }
    }
    let channel = match self.channel() {
      Ok(channel) => channel,
      Err(e) => return Box::pin(async move { Err(fail(FailureKind::Unavailable, e)) }),
    };
    Box::pin(async move {
      let path = http::uri::PathAndQuery::from_str(&method.path).map_err(|e| fail(FailureKind::Internal, e.to_string()))?;
      let mut grpc = tonic::client::Grpc::new(channel);
      grpc.ready().await.map_err(|e| fail(FailureKind::Unavailable, e.to_string()))?;
      let response = grpc.unary(request, path, Raw).await.map_err(|status| fail(kind_for_code(status.code()), status.message().to_owned()))?;
      decode_response(&output, &response.into_inner()).map_err(|e| fail(FailureKind::Internal, e))
    })
  }
}
