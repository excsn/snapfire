//! Reads the TypeScript interface subset the contract accepts, so a session
//! schema or an action input declared in TypeScript becomes contract types.

use snapfire_fsr_service::{Field, Type, TypeDef, Variant};
use swc_core::common::Spanned;
use swc_core::ecma::ast as js;

use crate::{parse, LowerError, Lowerer, Parsed, Residue, SessionDefaults};

/// A named type read from a schema module.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaType {
  pub name: String,
  pub def: TypeDef,
}

/// Every exported `interface` and string-literal-union `type` in the module.
pub fn read_schema(file: &str, source: &str) -> Result<Vec<SchemaType>, LowerError> {
  let parsed = parse(file, source)?;
  let mut out = Vec::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else {
      continue;
    };
    match &export.decl {
      js::Decl::TsInterface(interface) => {
        if !interface.extends.is_empty() {
          return Err(parsed.residue(interface.span, "`extends` on a schema interface").into());
        }
        if interface.type_params.is_some() {
          return Err(parsed.residue(interface.span, "a generic schema interface").into());
        }
        let mut fields = Vec::new();
        for member in &interface.body.body {
          let js::TsTypeElement::TsPropertySignature(prop) = member else {
            return Err(parsed.residue(member.span(), "a method or index signature in a schema").into());
          };
          let js::Expr::Ident(key) = &*prop.key else {
            return Err(parsed.residue(prop.key.span(), "a computed property name in a schema").into());
          };
          let ann = prop
            .type_ann
            .as_ref()
            .ok_or_else(|| parsed.residue(prop.span, "a schema field without a type"))?;
          let mut ty = ts_type(&parsed, &ann.type_ann)?;
          if prop.optional && !matches!(ty, Type::Optional(_)) {
            ty = Type::optional(ty);
          }
          fields.push(Field::new(key.sym.to_string(), ty));
        }
        out.push(SchemaType { name: interface.id.sym.to_string(), def: TypeDef::Record { fields } });
      }
      js::Decl::TsTypeAlias(alias) => {
        let js::TsType::TsUnionOrIntersectionType(js::TsUnionOrIntersectionType::TsUnionType(union)) = &*alias.type_ann else {
          return Err(parsed.residue(alias.span, "a type alias that is not a union of string literals; use an interface").into());
        };
        let mut variants = Vec::new();
        for arm in &union.types {
          match &**arm {
            js::TsType::TsLitType(js::TsLitType { lit: js::TsLit::Str(s), .. }) => {
              variants.push(Variant::unit(s.value.to_atom_lossy().to_string()));
            }
            other => return Err(parsed.residue(other.span(), "a union arm that is not a string literal").into()),
          }
        }
        out.push(SchemaType { name: alias.id.sym.to_string(), def: TypeDef::Union { variants } });
      }
      _ => {}
    }
  }
  Ok(out)
}

fn ts_type(parsed: &Parsed, ty: &js::TsType) -> Result<Type, Residue> {
  Ok(match ty {
    js::TsType::TsParenthesizedType(p) => ts_type(parsed, &p.type_ann)?,
    js::TsType::TsKeywordType(k) => match k.kind {
      js::TsKeywordTypeKind::TsStringKeyword => Type::Str,
      js::TsKeywordTypeKind::TsNumberKeyword => Type::F64,
      js::TsKeywordTypeKind::TsBigIntKeyword => Type::I64,
      js::TsKeywordTypeKind::TsBooleanKeyword => Type::Bool,
      js::TsKeywordTypeKind::TsNullKeyword | js::TsKeywordTypeKind::TsUndefinedKeyword => Type::Null,
      _ => return Err(parsed.residue(k.span, "a keyword type outside the contract; use string, number, bigint, boolean or null")),
    },
    js::TsType::TsArrayType(a) => Type::list(ts_type(parsed, &a.elem_type)?),
    js::TsType::TsTypeRef(r) => {
      let js::TsEntityName::Ident(id) = &r.type_name else {
        return Err(parsed.residue(r.span, "a qualified type name"));
      };
      let args: Vec<&js::TsType> = r.type_params.as_ref().map(|p| p.params.iter().map(|t| &**t).collect()).unwrap_or_default();
      match (id.sym.as_ref(), args.as_slice()) {
        ("Array", [inner]) => Type::list(ts_type(parsed, inner)?),
        ("Record", [key, value]) => {
          if !matches!(key, js::TsType::TsKeywordType(k) if k.kind == js::TsKeywordTypeKind::TsStringKeyword) {
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
    js::TsType::TsUnionOrIntersectionType(js::TsUnionOrIntersectionType::TsUnionType(union)) => {
      let mut inner = None;
      let mut nullable = false;
      for arm in &union.types {
        match &**arm {
          js::TsType::TsKeywordType(k) if matches!(k.kind, js::TsKeywordTypeKind::TsNullKeyword | js::TsKeywordTypeKind::TsUndefinedKeyword) => nullable = true,
          other => {
            if inner.is_some() {
              return Err(parsed.residue(union.span, "a union of two types; declare a named union of string literals instead"));
            }
            inner = Some(ts_type(parsed, other)?);
          }
        }
      }
      match (inner, nullable) {
        (Some(t), true) => Type::optional(t),
        (Some(t), false) => t,
        (None, _) => Type::Null,
      }
    }
    js::TsType::TsTypeLit(lit) => return Err(parsed.residue(lit.span, "an inline object type; declare an interface and name it")),
    js::TsType::TsLitType(lit) => return Err(parsed.residue(lit.span, "a literal type outside a named union")),
    other => return Err(parsed.residue(other.span(), "a type outside the contract")),
  })
}

/// `export const defaults: Session = { ... }` in a schema module: one literal
/// per session key. Absent when the module declares none.
pub fn read_session_defaults(file: &str, source: &str) -> Result<SessionDefaults, LowerError> {
  let parsed = parse(file, source)?;
  let empty = SessionDefaults::new();
  for item in &parsed.module.body {
    let js::ModuleItem::ModuleDecl(js::ModuleDecl::ExportDecl(export)) = item else {
      continue;
    };
    let js::Decl::Var(var) = &export.decl else { continue };
    for decl in &var.decls {
      let js::Pat::Ident(name) = &decl.name else { continue };
      if name.id.sym.as_ref() != "defaults" {
        continue;
      }
      let Some(init) = &decl.init else {
        return Err(parsed.residue(decl.span, "`defaults` without a value").into());
      };
      let js::Expr::Object(obj) = &**init else {
        return Err(parsed.residue(init.span(), "`defaults` must be an object literal").into());
      };
      let mut out = SessionDefaults::new();
      let mut lowerer = Lowerer::new(&parsed, &empty);
      for prop in &obj.props {
        let js::PropOrSpread::Prop(p) = prop else {
          return Err(parsed.residue(prop.span(), "a spread in `defaults`").into());
        };
        let js::Prop::KeyValue(kv) = &**p else {
          return Err(parsed.residue(p.span(), "`defaults` entries are `key: literal`").into());
        };
        let key = match &kv.key {
          js::PropName::Ident(id) => id.sym.to_string(),
          js::PropName::Str(s) => s.value.to_atom_lossy().to_string(),
          other => return Err(parsed.residue(other.span(), "a computed key in `defaults`").into()),
        };
        out.push((key, lowerer.literal(&kv.value)?));
      }
      return Ok(out);
    }
  }
  Ok(empty)
}
