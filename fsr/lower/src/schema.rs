//! Reads the TypeScript interface subset the contract accepts, so a session
//! schema or an action input declared in TypeScript becomes contract types.

use oxc_ast::ast as js;
use oxc_span::GetSpan;
use snapfire_fsr_service::{Field, Type, TypeDef, Variant};

use crate::{parse, LowerError, Lowerer, Parsed, Residue, SessionDefaults};

/// A named type read from a schema module.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct SchemaType {
  pub name: String,
  pub def: TypeDef,
}

/// The name a schema gives a file part posted to an action. The host builds
/// one per part, so the contract always defines it and an application declaring
/// its own is refused rather than silently disagreeing with the host.
pub const UPLOAD: &str = "Upload";

/// The types every contract has without an application declaring them.
pub fn builtin_types() -> Vec<SchemaType> {
  vec![SchemaType {
    name: UPLOAD.to_owned(),
    def: TypeDef::record(vec![
      Field::new("filename".to_owned(), Type::Str),
      Field::new("content_type".to_owned(), Type::Str),
      Field::new("size".to_owned(), Type::I64),
      Field::new("bytes".to_owned(), Type::Bytes),
    ]),
  }]
}

/// Every exported `interface` and string-literal-union `type` in the module.
pub fn read_schema(file: &str, source: &str) -> Result<Vec<SchemaType>, LowerError> {
  let arena = oxc_allocator::Allocator::default();
  let parsed = parse(&arena, file, source)?;
  let mut out = Vec::new();
  for stmt in &parsed.module().body {
    let js::Statement::ExportDeclaration(export) = stmt else {
      continue;
    };
    match &export.declaration {
      js::Declaration::TSInterfaceDeclaration(interface) => {
        if !interface.extends.is_empty() {
          return Err(parsed.residue(interface.span, "`extends` on a schema interface").into());
        }
        if interface.type_parameters.is_some() {
          return Err(parsed.residue(interface.span, "a generic schema interface").into());
        }
        let mut fields = Vec::new();
        for member in &interface.body.body {
          let js::TSSignature::TSPropertySignature(prop) = member else {
            return Err(parsed.residue(member.span(), "a method or index signature in a schema").into());
          };
          let (js::PropertyKey::StaticIdentifier(key), false) = (&prop.key, prop.computed) else {
            return Err(parsed.residue(prop.key.span(), "a computed property name in a schema").into());
          };
          let ann = prop.type_annotation.as_ref().ok_or_else(|| parsed.residue(prop.span, "a schema field without a type"))?;
          let mut ty = ts_type(&parsed, &ann.type_annotation)?;
          if prop.optional && !matches!(ty, Type::Optional(_)) {
            ty = Type::optional(ty);
          }
          fields.push(Field::new(key.name.to_string(), ty));
        }
        out.push(SchemaType { name: interface.id.name.to_string(), def: TypeDef::record(fields) });
      }
      js::Declaration::TSTypeAliasDeclaration(alias) => {
        let js::TSType::TSUnionType(union) = &alias.type_annotation else {
          return Err(parsed.residue(alias.span, "a type alias that is not a union of string literals; use an interface").into());
        };
        let mut variants = Vec::new();
        for arm in &union.types {
          match arm {
            js::TSType::TSLiteralType(lit) => match &lit.literal {
              js::TSLiteral::StringLiteral(s) => variants.push(Variant::unit(crate::lossy(s.value))),
              _ => return Err(parsed.residue(lit.span, "a union arm that is not a string literal").into()),
            },
            other => return Err(parsed.residue(other.span(), "a union arm that is not a string literal").into()),
          }
        }
        out.push(SchemaType { name: alias.id.name.to_string(), def: TypeDef::Union { variants } });
      }
      _ => {}
    }
  }
  Ok(out)
}

/// Whether `t` is `null` or `undefined`.
fn nullish(t: &js::TSType) -> bool {
  matches!(t, js::TSType::TSNullKeyword(_) | js::TSType::TSUndefinedKeyword(_))
}

fn ts_type(parsed: &Parsed, ty: &js::TSType) -> Result<Type, Residue> {
  Ok(match ty {
    js::TSType::TSParenthesizedType(p) => ts_type(parsed, &p.type_annotation)?,
    js::TSType::TSStringKeyword(_) => Type::Str,
    js::TSType::TSNumberKeyword(_) => Type::F64,
    js::TSType::TSBigIntKeyword(_) => Type::I64,
    js::TSType::TSBooleanKeyword(_) => Type::Bool,
    t if nullish(t) => Type::Null,
    js::TSType::TSAnyKeyword(_) | js::TSType::TSUnknownKeyword(_) | js::TSType::TSNeverKeyword(_) | js::TSType::TSObjectKeyword(_) | js::TSType::TSSymbolKeyword(_) | js::TSType::TSVoidKeyword(_) | js::TSType::TSIntrinsicKeyword(_) => {
      return Err(parsed.residue(ty.span(), "a keyword type outside the contract; use string, number, bigint, boolean or null"));
    }
    js::TSType::TSArrayType(a) => Type::list(ts_type(parsed, &a.element_type)?),
    js::TSType::TSTypeReference(r) => {
      let js::TSTypeName::IdentifierReference(id) = &r.type_name else {
        return Err(parsed.residue(r.span, "a qualified type name"));
      };
      let args: Vec<&js::TSType> = r.type_arguments.as_ref().map(|p| p.params.iter().collect()).unwrap_or_default();
      match (id.name.as_str(), args.as_slice()) {
        ("Array", [inner]) => Type::list(ts_type(parsed, inner)?),
        ("Record", [key, value]) => {
          if !matches!(key, js::TSType::TSStringKeyword(_)) {
            return Err(parsed.residue(key.span(), "a `Record` key that is not `string`"));
          }
          Type::map(ts_type(parsed, value)?)
        }
        ("Uint8Array", []) => Type::Bytes,
        ("Int8Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::I8),
        ("Int16Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::I16),
        ("Uint16Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::U16),
        ("Int32Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::I32),
        ("Uint32Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::U32),
        ("BigInt64Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::I64),
        ("BigUint64Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::U64),
        ("Float32Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::F32),
        ("Float64Array", []) => Type::Array(snapfire_fsr_service::ScalarKind::F64),
        (name, []) => Type::named(name),
        (name, _) => return Err(parsed.residue(r.span, format!("a generic reference to `{name}`"))),
      }
    }
    js::TSType::TSUnionType(union) => {
      let mut inner = None;
      let mut nullable = false;
      for arm in &union.types {
        if nullish(arm) {
          nullable = true;
          continue;
        }
        if inner.is_some() {
          return Err(parsed.residue(union.span, "a union of two types; declare a named union of string literals instead"));
        }
        inner = Some(ts_type(parsed, arm)?);
      }
      match (inner, nullable) {
        (Some(t), true) => Type::optional(t),
        (Some(t), false) => t,
        (None, _) => Type::Null,
      }
    }
    js::TSType::TSTypeLiteral(lit) => return Err(parsed.residue(lit.span, "an inline object type; declare an interface and name it")),
    js::TSType::TSLiteralType(lit) => return Err(parsed.residue(lit.span, "a literal type outside a named union")),
    other => return Err(parsed.residue(other.span(), "a type outside the contract")),
  })
}

/// The name an export specifier exports under.
pub(crate) fn export_name(name: &js::ModuleExportName) -> String {
  match name {
    js::ModuleExportName::IdentifierName(id) => id.name.to_string(),
    js::ModuleExportName::IdentifierReference(id) => id.name.to_string(),
    js::ModuleExportName::StringLiteral(s) => crate::lossy(s.value),
  }
}

/// `export const defaults: Session = { ... }` in a schema module: one literal
/// per session key. Absent when the module declares none.
pub fn read_session_defaults(file: &str, source: &str) -> Result<SessionDefaults, LowerError> {
  let arena = oxc_allocator::Allocator::default();
  let parsed = parse(&arena, file, source)?;
  let empty = SessionDefaults::new();
  for stmt in &parsed.module().body {
    let specifiers = match stmt {
      js::Statement::ExportNamedDeclaration(named) => Some(&named.specifiers),
      js::Statement::ExportFromDeclaration(from) => Some(&from.specifiers),
      _ => None,
    };
    if let Some(specifiers) = specifiers {
      for spec in specifiers {
        if export_name(&spec.exported) == "defaults" {
          return Err(parsed.residue(spec.span, "`defaults` exported apart from its declaration; the build reads `export const defaults = { … }`").into());
        }
      }
      continue;
    }
    let js::Statement::ExportDeclaration(export) = stmt else {
      continue;
    };
    let js::Declaration::VariableDeclaration(var) = &export.declaration else { continue };
    for decl in &var.declarations {
      let js::BindingPattern::BindingIdentifier(name) = &decl.id else {
        return Err(parsed.residue(decl.span, "a destructuring export in a session module; `defaults` is an object literal bound to its own name").into());
      };
      if name.name.as_str() != "defaults" {
        continue;
      }
      let Some(init) = &decl.init else {
        return Err(parsed.residue(decl.span, "`defaults` without a value").into());
      };
      let js::Expression::ObjectExpression(obj) = init else {
        return Err(parsed.residue(init.span(), "`defaults` must be an object literal").into());
      };
      let mut out = SessionDefaults::new();
      let mut lowerer = Lowerer::new(&parsed, &empty);
      for prop in &obj.properties {
        let js::ObjectPropertyKind::ObjectProperty(p) = prop else {
          return Err(parsed.residue(prop.span(), "a spread in `defaults`").into());
        };
        if p.kind != js::PropertyKind::Init || p.shorthand || p.method {
          return Err(parsed.residue(p.span, "`defaults` entries are `key: literal`").into());
        }
        let key = match &p.key {
          js::PropertyKey::StaticIdentifier(id) if !p.computed => id.name.to_string(),
          js::PropertyKey::StringLiteral(s) if !p.computed => crate::lossy(s.value),
          other => return Err(parsed.residue(other.span(), "a computed key in `defaults`").into()),
        };
        out.push((key, lowerer.literal(&p.value)?));
      }
      return Ok(out);
    }
  }
  Ok(empty)
}
