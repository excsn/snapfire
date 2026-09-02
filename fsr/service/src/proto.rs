//! A `.proto` file as a contract: messages become records, services become
//! services with unary methods, and the descriptors are kept so `GrpcTransport`
//! can encode a call without generated code. Compiled by protox, so no protoc.

use std::path::Path;

use indexmap::IndexMap;
use prost_reflect::{DescriptorPool, FieldDescriptor, Kind, MessageDescriptor, MethodDescriptor};
use protox::file::{ChainFileResolver, File, FileResolver, GoogleFileResolver, IncludeFileResolver};

use crate::contract::{Contract, Field, Method, Service, Type, TypeDef};
use crate::openapi::ImportError;

/// How one contract method reaches a gRPC server: the request path and the
/// full names of its request and response messages in the pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcMethod {
  pub path: String,
  pub input: String,
  pub output: String,
}

/// What one `.proto` lowers to: the neutral contract, the descriptor pool the
/// transport encodes with and one `GrpcMethod` per `service.method` key.
#[derive(Debug, Clone)]
pub struct ImportedProto {
  pub contract: Contract,
  pub pool: DescriptorPool,
  pub methods: Vec<(String, GrpcMethod)>,
}

fn malformed(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Malformed { at: at.into(), what: what.into() }
}

fn unsupported(at: impl Into<String>, what: impl Into<String>) -> ImportError {
  ImportError::Unsupported { at: at.into(), what: what.into() }
}

struct SourceResolver {
  name: String,
  source: String,
}

impl FileResolver for SourceResolver {
  fn open_file(&self, name: &str) -> Result<File, protox::Error> {
    if name == self.name {
      File::from_source(name, &self.source)
    } else {
      Err(protox::Error::file_not_found(name))
    }
  }
}

/// Imports the file at `path`, resolving its imports against its directory and
/// the well-known Google types.
pub fn import_proto(path: &Path, default_service: &str) -> Result<ImportedProto, ImportError> {
  let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
  let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
  let mut resolver = ChainFileResolver::new();
  resolver.add(IncludeFileResolver::new(dir));
  resolver.add(GoogleFileResolver::new());
  compile(resolver, &name, default_service)
}

/// Imports proto source held in memory as `name`; only the well-known Google
/// types can be imported from it.
pub fn import_proto_source(name: &str, source: &str, default_service: &str) -> Result<ImportedProto, ImportError> {
  let mut resolver = ChainFileResolver::new();
  resolver.add(SourceResolver { name: name.to_owned(), source: source.to_owned() });
  resolver.add(GoogleFileResolver::new());
  compile(resolver, name, default_service)
}

fn compile<R: FileResolver + 'static>(resolver: R, name: &str, default_service: &str) -> Result<ImportedProto, ImportError> {
  let mut compiler = protox::Compiler::with_file_resolver(resolver);
  compiler.include_imports(true);
  compiler.open_file(name).map_err(|e| malformed(name, e.to_string()))?;
  let set = compiler.file_descriptor_set();
  let pool = DescriptorPool::from_file_descriptor_set(set).map_err(|e| malformed(name, e.to_string()))?;
  let file = pool.get_file_by_name(name).ok_or_else(|| malformed(name, "the compiler did not keep the file"))?;

  let mut lower = Lower { pool: pool.clone(), types: IndexMap::new(), package: file.package_name().to_owned() };
  let services: Vec<_> = file.services().collect();
  let mut contract = Contract::new();
  let mut methods = Vec::new();
  for service in &services {
    let service_name = if services.len() == 1 { default_service.to_owned() } else { service.name().to_owned() };
    let mut def = Service::new();
    for method in service.methods() {
      let at = format!("{}.{}", service.full_name(), method.name());
      if method.is_client_streaming() || method.is_server_streaming() {
        return Err(unsupported(at, "a streaming method"));
      }
      let method_name = lower_camel(method.name());
      let params = lower.params(&method, &at)?;
      let returns = lower.returns(&method, &at)?;
      def = def.method(method_name.clone(), Method::new(params, returns));
      methods.push((
        format!("{service_name}.{method_name}"),
        GrpcMethod { path: format!("/{}/{}", service.full_name(), method.name()), input: method.input().full_name().to_owned(), output: method.output().full_name().to_owned() },
      ));
    }
    contract = contract.service(service_name, def);
  }
  for (name, def) in lower.types {
    contract = contract.define(name, def);
  }
  contract.validate()?;
  Ok(ImportedProto { contract, pool, methods })
}

fn lower_camel(name: &str) -> String {
  let mut chars = name.chars();
  match chars.next() {
    Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
    None => String::new(),
  }
}

struct Lower {
  pool: DescriptorPool,
  types: IndexMap<String, TypeDef>,
  package: String,
}

impl Lower {
  /// `pkg.Outer.Inner` is `OuterInner`: the package drops, nesting joins.
  fn type_name(&self, message: &MessageDescriptor) -> String {
    let full = message.full_name();
    let local = full.strip_prefix(&self.package).map(|s| s.trim_start_matches('.')).unwrap_or(full);
    local.replace('.', "")
  }

  fn params(&mut self, method: &MethodDescriptor, at: &str) -> Result<Vec<Field>, ImportError> {
    let input = method.input();
    if input.full_name() == "google.protobuf.Empty" {
      return Ok(Vec::new());
    }
    let mut params = Vec::new();
    for field in input.fields() {
      params.push(Field::new(field.name(), self.field_type(&field, &format!("{at}({})", field.name()))?));
    }
    Ok(params)
  }

  fn returns(&mut self, method: &MethodDescriptor, at: &str) -> Result<Type, ImportError> {
    let output = method.output();
    if output.full_name() == "google.protobuf.Empty" {
      return Ok(Type::Null);
    }
    self.message(&output, at)
  }

  fn message(&mut self, message: &MessageDescriptor, at: &str) -> Result<Type, ImportError> {
    if let Some(ty) = well_known(message.full_name()) {
      return ty.map_err(|what| unsupported(at, what));
    }
    let name = self.type_name(message);
    if !self.types.contains_key(&name) {
      self.types.insert(name.clone(), TypeDef::Record { fields: Vec::new() });
      let mut fields = Vec::new();
      for field in message.fields() {
        fields.push(Field::new(field.name(), self.field_type(&field, &format!("{}.{}", message.full_name(), field.name()))?));
      }
      self.types.insert(name.clone(), TypeDef::Record { fields });
    }
    Ok(Type::named(name))
  }

  fn field_type(&mut self, field: &FieldDescriptor, at: &str) -> Result<Type, ImportError> {
    if field.is_map() {
      let Kind::Message(entry) = field.kind() else { return Err(malformed(at, "a map field without an entry message")) };
      let key = entry.map_entry_key_field();
      if !matches!(key.kind(), Kind::String) {
        return Err(unsupported(at, format!("a map keyed by {}", describe(&key.kind()))));
      }
      let value = entry.map_entry_value_field();
      return Ok(Type::map(self.scalar_or_message(&value.kind(), at)?));
    }
    let inner = self.scalar_or_message(&field.kind(), at)?;
    if field.is_list() {
      return Ok(Type::list(inner));
    }
    let nullable = matches!(field.kind(), Kind::Message(_)) || field.supports_presence() || field.containing_oneof().is_some();
    Ok(if nullable && !matches!(inner, Type::Optional(_)) { Type::optional(inner) } else { inner })
  }

  fn scalar_or_message(&mut self, kind: &Kind, at: &str) -> Result<Type, ImportError> {
    Ok(match kind {
      Kind::Double => Type::F64,
      Kind::Float => Type::F32,
      Kind::Int32 | Kind::Sint32 | Kind::Sfixed32 => Type::I32,
      Kind::Int64 | Kind::Sint64 | Kind::Sfixed64 => Type::I64,
      Kind::Uint32 | Kind::Fixed32 => Type::U32,
      Kind::Uint64 | Kind::Fixed64 => Type::U64,
      Kind::Bool => Type::Bool,
      Kind::String => Type::Str,
      Kind::Bytes => Type::Bytes,
      Kind::Enum(_) => Type::Str,
      Kind::Message(message) => {
        let message = self.pool.get_message_by_name(message.full_name()).unwrap_or_else(|| message.clone());
        self.message(&message, at)?
      }
    })
  }
}

fn describe(kind: &Kind) -> String {
  match kind {
    Kind::Message(m) => m.full_name().to_owned(),
    Kind::Enum(e) => e.full_name().to_owned(),
    other => format!("{other:?}").to_lowercase(),
  }
}

/// The Google well-known types the JSON mapping renders as scalars, and the
/// ones the value model has no shape for.
fn well_known(full_name: &str) -> Option<Result<Type, String>> {
  Some(match full_name {
    "google.protobuf.Timestamp" | "google.protobuf.Duration" => Ok(Type::Str),
    "google.protobuf.StringValue" => Ok(Type::optional(Type::Str)),
    "google.protobuf.BytesValue" => Ok(Type::optional(Type::Bytes)),
    "google.protobuf.BoolValue" => Ok(Type::optional(Type::Bool)),
    "google.protobuf.Int32Value" => Ok(Type::optional(Type::I32)),
    "google.protobuf.Int64Value" => Ok(Type::optional(Type::I64)),
    "google.protobuf.UInt32Value" => Ok(Type::optional(Type::U32)),
    "google.protobuf.UInt64Value" => Ok(Type::optional(Type::U64)),
    "google.protobuf.FloatValue" => Ok(Type::optional(Type::F32)),
    "google.protobuf.DoubleValue" => Ok(Type::optional(Type::F64)),
    "google.protobuf.Empty" => Ok(Type::Null),
    "google.protobuf.Any" | "google.protobuf.Struct" | "google.protobuf.Value" | "google.protobuf.ListValue" | "google.protobuf.FieldMask" => Err(format!("{full_name}, which the value model has no shape for")),
    _ => return None,
  })
}
