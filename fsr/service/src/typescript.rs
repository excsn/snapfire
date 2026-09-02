//! The contract as TypeScript declarations for the server side: every named
//! type, plus a `Services` interface with one method per contract method.
//! Integers of every width are `bigint`, because a body runs over the value
//! model where an integer is always `Value::Int`.

use std::fmt::Write;

use crate::contract::{Contract, Field, ScalarKind, Type, TypeDef};

/// Which side of the wire a declaration is for. On the server an integer is
/// always `Value::Int`, so `bigint`; the browser codec hands back a `number`
/// when the value is safe and a `bigint` otherwise, so `bigint | number`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavour {
  Server,
  Client,
}

pub fn declarations(contract: &Contract) -> String {
  let mut out = String::from("// Generated from the contract by fsr build. Do not edit.\n\n");
  out.push_str(&type_declarations(contract, Flavour::Server));

  let _ = writeln!(out, "export interface Services {{");
  for (service, def) in &contract.services {
    let _ = writeln!(out, "  {}: {{", property(service));
    for (method, m) in &def.methods {
      let all_optional = m.params.iter().all(|p| matches!(p.ty, Type::Optional(_)));
      let args = if m.params.is_empty() {
        String::new()
      } else {
        let fields: Vec<String> = m.params.iter().map(field_line).collect();
        format!("args{}: {{ {} }}", if all_optional { "?" } else { "" }, fields.join(" "))
      };
      let _ = writeln!(out, "    {}({args}): Promise<{}>;", property(method), type_name(&m.returns));
    }
    let _ = writeln!(out, "  }};");
  }
  let _ = writeln!(out, "}}");
  out
}

/// Every named type of the contract, without `Services`. The client flavour is
/// what a page imports.
pub fn type_declarations(contract: &Contract, flavour: Flavour) -> String {
  let mut out = String::new();
  for (name, def) in &contract.types {
    match def {
      TypeDef::Record { fields } => {
        let _ = writeln!(out, "export interface {name} {{");
        for field in fields {
          let _ = writeln!(out, "  {}", field_line_for(field, flavour));
        }
        let _ = writeln!(out, "}}\n");
      }
      TypeDef::Union { variants } => {
        let arms: Vec<String> = variants
          .iter()
          .map(|v| match &v.payload {
            None => format!("{{ tag: \"{}\" }}", v.tag),
            Some(payload) => format!("{{ tag: \"{}\"; payload: {} }}", v.tag, type_name_for(payload, flavour)),
          })
          .collect();
        let _ = writeln!(out, "export type {name} =\n  | {};\n", arms.join("\n  | "));
      }
    }
  }
  out
}

fn field_line(field: &Field) -> String {
  field_line_for(field, Flavour::Server)
}

fn field_line_for(field: &Field, flavour: Flavour) -> String {
  match &field.ty {
    Type::Optional(inner) => format!("{}?: {} | null;", property(&field.name), type_name_for(inner, flavour)),
    ty => format!("{}: {};", property(&field.name), type_name_for(ty, flavour)),
  }
}

pub fn type_name(ty: &Type) -> String {
  type_name_for(ty, Flavour::Server)
}

pub fn type_name_for(ty: &Type, flavour: Flavour) -> String {
  match ty {
    Type::Null => "null".to_owned(),
    Type::Bool => "boolean".to_owned(),
    Type::I32 | Type::I64 | Type::I128 | Type::U32 | Type::U64 | Type::U128 => match flavour {
      Flavour::Server => "bigint".to_owned(),
      Flavour::Client => "bigint | number".to_owned(),
    },
    Type::F32 | Type::F64 => "number".to_owned(),
    Type::Str => "string".to_owned(),
    Type::Bytes => "Uint8Array".to_owned(),
    Type::Array(kind) => match kind {
      ScalarKind::I8 => "Int8Array",
      ScalarKind::U8 => "Uint8Array",
      ScalarKind::I16 => "Int16Array",
      ScalarKind::U16 => "Uint16Array",
      ScalarKind::I32 => "Int32Array",
      ScalarKind::U32 => "Uint32Array",
      ScalarKind::I64 => "BigInt64Array",
      ScalarKind::U64 => "BigUint64Array",
      ScalarKind::F32 => "Float32Array",
      ScalarKind::F64 => "Float64Array",
    }
    .to_owned(),
    Type::Optional(inner) => format!("{} | null", type_name_for(inner, flavour)),
    Type::List(inner) => match (&**inner, flavour) {
      (Type::Optional(_), _) => format!("({})[]", type_name_for(inner, flavour)),
      (Type::I32 | Type::I64 | Type::I128 | Type::U32 | Type::U64 | Type::U128, Flavour::Client) => format!("({})[]", type_name_for(inner, flavour)),
      _ => format!("{}[]", type_name_for(inner, flavour)),
    },
    Type::Map(values) => format!("Record<string, {}>", type_name_for(values, flavour)),
    Type::Named(name) => name.clone(),
  }
}

fn property(name: &str) -> String {
  let plain = !name.is_empty()
    && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
    && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$');
  if plain {
    name.to_owned()
  } else {
    format!("{name:?}")
  }
}
