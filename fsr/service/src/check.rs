use snapfire_fsr_core::{TypedArray, Value, ValueMap};

use crate::contract::{Contract, ScalarKind, Type, TypeDef};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ContractError {
  #[error("no service `{0}` in the contract")]
  UnknownService(String),
  #[error("service `{service}` has no method `{method}`")]
  UnknownMethod { service: String, method: String },
  #[error("contract names type `{name}` at {path}, which it does not define")]
  UnknownType { path: String, name: String },
  #[error("{path}: unknown field `{field}`")]
  UnknownField { path: String, field: String },
  #[error("{path}: missing field `{field}`")]
  MissingField { path: String, field: String },
  #[error("{path}: unknown variant `{tag}`, expected one of {expected}")]
  UnknownVariant { path: String, tag: String, expected: String },
  #[error("{path}: expected {expected}, found {found}")]
  Mismatch { path: String, expected: String, found: String },
}

fn kind_of(value: &Value) -> String {
  match value {
    Value::Null => "null".to_owned(),
    Value::Bool(_) => "bool".to_owned(),
    Value::Int(v) => format!("int {v}"),
    Value::UInt(v) => format!("uint {v}"),
    Value::F32(_) => "f32".to_owned(),
    Value::F64(_) => "f64".to_owned(),
    Value::Str(_) => "str".to_owned(),
    Value::Bytes(_) => "bytes".to_owned(),
    Value::TypedArray(a) => format!("array<{}>", array_kind(a).as_str()),
    Value::Seq(_) => "list".to_owned(),
    Value::Map(_) => "map".to_owned(),
    Value::Variant { tag, .. } => format!("variant `{tag}`"),
    Value::Ref { .. } => "ref".to_owned(),
  }
}

fn array_kind(array: &TypedArray) -> ScalarKind {
  match array {
    TypedArray::I8(_) => ScalarKind::I8,
    TypedArray::U8(_) => ScalarKind::U8,
    TypedArray::I16(_) => ScalarKind::I16,
    TypedArray::U16(_) => ScalarKind::U16,
    TypedArray::I32(_) => ScalarKind::I32,
    TypedArray::U32(_) => ScalarKind::U32,
    TypedArray::I64(_) => ScalarKind::I64,
    TypedArray::U64(_) => ScalarKind::U64,
    TypedArray::F32(_) => ScalarKind::F32,
    TypedArray::F64(_) => ScalarKind::F64,
  }
}

fn field_path(path: &str, field: &str) -> String {
  if path.is_empty() {
    field.to_owned()
  } else {
    format!("{path}.{field}")
  }
}

fn int_in_range(value: &Value, low: i128, high: i128) -> bool {
  matches!(value, Value::Int(v) if *v >= low && *v <= high)
}

impl Contract {
  /// Every `Named` reference resolves. What the build runs once before writing
  /// the artifact, so a runtime lookup can trust the graph.
  pub fn validate(&self) -> Result<(), ContractError> {
    for (name, def) in &self.types {
      match def {
        TypeDef::Record { fields } => {
          for field in fields {
            self.check_type(&field.ty, &field_path(name, &field.name))?;
          }
        }
        TypeDef::Union { variants } => {
          for variant in variants {
            if let Some(payload) = &variant.payload {
              self.check_type(payload, &field_path(name, &variant.tag))?;
            }
          }
        }
      }
    }
    for (service_name, service) in &self.services {
      for (method_name, method) in &service.methods {
        let path = format!("{service_name}.{method_name}");
        for param in &method.params {
          self.check_type(&param.ty, &field_path(&path, &param.name))?;
        }
        self.check_type(&method.returns, &format!("{path}()"))?;
      }
    }
    Ok(())
  }

  fn check_type(&self, ty: &Type, path: &str) -> Result<(), ContractError> {
    match ty {
      Type::Named(name) => {
        if self.types.contains_key(name) {
          Ok(())
        } else {
          Err(ContractError::UnknownType { path: path.to_owned(), name: name.clone() })
        }
      }
      Type::Optional(inner) | Type::List(inner) | Type::Map(inner) => self.check_type(inner, path),
      _ => Ok(()),
    }
  }

  pub fn check_value(&self, ty: &Type, value: &Value, path: &str) -> Result<(), ContractError> {
    let mismatch = |expected: String| ContractError::Mismatch {
      path: path.to_owned(),
      expected,
      found: kind_of(value),
    };

    let ok = match ty {
      Type::Null => matches!(value, Value::Null),
      Type::Bool => matches!(value, Value::Bool(_)),
      Type::I32 => int_in_range(value, i32::MIN as i128, i32::MAX as i128),
      Type::I64 => int_in_range(value, i64::MIN as i128, i64::MAX as i128),
      Type::I128 => matches!(value, Value::Int(_)),
      Type::U32 => int_in_range(value, 0, u32::MAX as i128),
      Type::U64 => int_in_range(value, 0, u64::MAX as i128),
      Type::U128 => matches!(value, Value::UInt(_)) || int_in_range(value, 0, i128::MAX),
      Type::F32 => matches!(value, Value::F32(_)),
      Type::F64 => matches!(value, Value::F64(_)),
      Type::Str => matches!(value, Value::Str(_)),
      Type::Bytes => matches!(value, Value::Bytes(_)),
      Type::Array(kind) => matches!(value, Value::TypedArray(a) if array_kind(a) == *kind),
      Type::Optional(inner) => {
        return match value {
          Value::Null => Ok(()),
          _ => self.check_value(inner, value, path),
        }
      }
      Type::List(inner) => {
        let Value::Seq(items) = value else { return Err(mismatch(ty.describe())) };
        for (i, item) in items.iter().enumerate() {
          self.check_value(inner, item, &format!("{path}[{i}]"))?;
        }
        return Ok(());
      }
      Type::Map(values) => {
        let Value::Map(map) = value else { return Err(mismatch(ty.describe())) };
        for (key, item) in map {
          self.check_value(values, item, &field_path(path, key))?;
        }
        return Ok(());
      }
      Type::Named(name) => {
        let Some(def) = self.types.get(name) else {
          return Err(ContractError::UnknownType { path: path.to_owned(), name: name.clone() });
        };
        return match def {
          TypeDef::Record { fields } => {
            let Value::Map(map) = value else { return Err(mismatch(name.clone())) };
            for field in fields {
              match map.get(&field.name) {
                Some(item) => self.check_value(&field.ty, item, &field_path(path, &field.name))?,
                None if matches!(field.ty, Type::Optional(_)) => {}
                None => {
                  return Err(ContractError::MissingField {
                    path: path.to_owned(),
                    field: field.name.clone(),
                  })
                }
              }
            }
            for key in map.keys() {
              if !fields.iter().any(|f| &f.name == key) {
                return Err(ContractError::UnknownField {
                  path: path.to_owned(),
                  field: key.clone(),
                });
              }
            }
            Ok(())
          }
          TypeDef::Union { variants } => {
            let Value::Variant { tag, payload } = value else { return Err(mismatch(name.clone())) };
            let Some(variant) = variants.iter().find(|v| &v.tag == tag) else {
              return Err(ContractError::UnknownVariant {
                path: path.to_owned(),
                tag: tag.clone(),
                expected: variants.iter().map(|v| v.tag.as_str()).collect::<Vec<_>>().join(", "),
              });
            };
            match (&variant.payload, payload) {
              (None, None) => Ok(()),
              (Some(ty), Some(value)) => self.check_value(ty, value, &field_path(path, tag)),
              (Some(ty), None) => Err(ContractError::Mismatch {
                path: field_path(path, tag),
                expected: ty.describe(),
                found: "no payload".to_owned(),
              }),
              (None, Some(value)) => Err(ContractError::Mismatch {
                path: field_path(path, tag),
                expected: "no payload".to_owned(),
                found: kind_of(value),
              }),
            }
          }
        };
      }
    };

    if ok {
      Ok(())
    } else {
      Err(mismatch(ty.describe()))
    }
  }

  /// The one call the plan-file validator, the registry and any generated stub
  /// all go through. An optional parameter may be omitted.
  pub fn check_call(&self, service: &str, method: &str, args: &ValueMap) -> Result<(), ContractError> {
    let Some(entry) = self.services.get(service) else {
      return Err(ContractError::UnknownService(service.to_owned()));
    };
    let Some(signature) = entry.methods.get(method) else {
      return Err(ContractError::UnknownMethod {
        service: service.to_owned(),
        method: method.to_owned(),
      });
    };

    let path = format!("{service}.{method}");
    for param in &signature.params {
      match args.get(&param.name) {
        Some(value) => self.check_value(&param.ty, value, &field_path(&path, &param.name))?,
        None if matches!(param.ty, Type::Optional(_)) => {}
        None => {
          return Err(ContractError::MissingField { path: path.clone(), field: param.name.clone() })
        }
      }
    }
    for key in args.keys() {
      if !signature.params.iter().any(|p| &p.name == key) {
        return Err(ContractError::UnknownField { path: path.clone(), field: key.clone() });
      }
    }
    Ok(())
  }

  pub fn check_return(&self, service: &str, method: &str, value: &Value) -> Result<(), ContractError> {
    let Some(signature) = self.method(service, method) else {
      return Err(ContractError::UnknownMethod {
        service: service.to_owned(),
        method: method.to_owned(),
      });
    };
    self.check_value(&signature.returns, value, &format!("{service}.{method}()"))
  }
}
