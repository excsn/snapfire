//! The application's own Rust, read the way a body is read: syntactically,
//! without compiling it. A `#[native]` `impl` block names the methods a body
//! may reach as `ctx.native.<module>.<method>()`; a `#[service]` `impl` block
//! declares a service a body calls as `ctx.services.<name>.<method>()`, its
//! contract read off the signatures. The types either mentions come from
//! the structs beside them.
//!
//! Reading rather than expanding is what keeps the ordering honest. `build.rs`
//! runs before the crate compiles, so a macro's output does not exist yet;
//! the source does.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use snapfire_fsr_service::{Contract, Field, Freshness, Method, Scope, Service, Type, TypeDef};

use crate::BuildError;

/// One method a body may call.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeMethod {
  /// The name a body calls it by: `list_rooms` reads as `listRooms`.
  pub name: String,
  pub args: Vec<(String, String)>,
  pub returns: String,
  /// `fn` rather than `async fn`, so it can answer without suspending.
  pub is_sync: bool,
}

/// One module, under the name the host registers it with.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeModule {
  pub name: String,
  pub rust_type: String,
  pub methods: Vec<NativeMethod>,
}

/// A struct a method mentions, so the declaration can name its fields.
#[derive(Debug, Clone, PartialEq)]
pub struct NativeType {
  pub name: String,
  pub fields: Vec<(String, String)>,
}

/// One `#[service]` block: the contract it declares, under the name a body
/// calls it by and the file it was read from, relative to the project.
#[derive(Debug, Clone, PartialEq)]
pub struct RustService {
  pub name: String,
  pub rust_type: String,
  pub file: String,
  pub contract: Contract,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Natives {
  pub modules: Vec<NativeModule>,
  pub types: Vec<NativeType>,
  pub services: Vec<RustService>,
}

impl Natives {
  pub fn is_empty(&self) -> bool {
    self.modules.is_empty()
  }
}

/// Every `.rs` file under `dir`, read for `#[native]` and `#[service]` impls
/// and the types they name. A directory that does not exist is not an error:
/// an application with no Rust has no natives.
pub fn read(dir: &Path) -> Result<Natives, BuildError> {
  let mut sources = Vec::new();
  collect(dir, &mut sources)?;
  let mut files = Vec::new();
  for (path, text) in &sources {
    let file = syn::parse_file(text).map_err(|e| BuildError::Manifest(path.clone(), format!("native read: {e}")))?;
    files.push((path.clone(), file));
  }
  let mut structs: HashMap<String, syn::ItemStruct> = HashMap::new();
  for (_, file) in &files {
    for item in &file.items {
      if let syn::Item::Struct(s) = item {
        structs.entry(s.ident.to_string()).or_insert_with(|| s.clone());
      }
    }
  }
  let mut found = Natives::default();
  for (path, file) in &files {
    read_file(path, dir, file, &structs, &mut found)?;
  }
  found.modules.sort_by(|a, b| a.name.cmp(&b.name));
  found.types.sort_by(|a, b| a.name.cmp(&b.name));
  found.types.dedup();
  found.services.sort_by(|a, b| a.name.cmp(&b.name));
  Ok(found)
}

fn collect(dir: &Path, out: &mut Vec<(PathBuf, String)>) -> Result<(), BuildError> {
  let Ok(entries) = std::fs::read_dir(dir) else {
    return Ok(());
  };
  let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path()).collect();
  paths.sort();
  for path in paths {
    if path.is_dir() {
      collect(&path, out)?;
    } else if path.extension().is_some_and(|e| e == "rs") {
      let text = std::fs::read_to_string(&path).map_err(|e| BuildError::Io(path.clone(), e))?;
      out.push((path, text));
    }
  }
  Ok(())
}

fn read_file(
  path: &Path,
  dir: &Path,
  file: &syn::File,
  structs: &HashMap<String, syn::ItemStruct>,
  out: &mut Natives,
) -> Result<(), BuildError> {
  let mut wanted: Vec<String> = Vec::new();
  for item in &file.items {
    let syn::Item::Impl(block) = item else { continue };
    let Some(rust_type) = type_name(&block.self_ty) else { continue };
    if block.attrs.iter().any(|a| is_attr(a, "service")) {
      let contract = read_service(path, &rust_type, block, structs)?;
      let file = match path.strip_prefix(dir.parent().unwrap_or(dir)) {
        Ok(rel) => rel.display().to_string(),
        Err(_) => path.display().to_string(),
      };
      out.services.push(RustService { name: snake(&rust_type), rust_type, file, contract });
      continue;
    }
    if !block.attrs.iter().any(|a| is_attr(a, "native")) {
      continue;
    }
    let mut methods = Vec::new();
    for item in &block.items {
      let syn::ImplItem::Fn(f) = item else { continue };
      if !matches!(f.vis, syn::Visibility::Public(_)) {
        continue;
      }
      let mut args = Vec::new();
      for arg in &f.sig.inputs {
        let syn::FnArg::Typed(typed) = arg else { continue };
        let syn::Pat::Ident(name) = &*typed.pat else { continue };
        let ts = ts_type(&typed.ty, &mut wanted);
        args.push((camel(&name.ident.to_string()), ts));
      }
      let returns = match &f.sig.output {
        syn::ReturnType::Default => "void".to_owned(),
        syn::ReturnType::Type(_, ty) => ts_type(ty, &mut wanted),
      };
      methods.push(NativeMethod {
        name: camel(&f.sig.ident.to_string()),
        args,
        returns,
        is_sync: f.sig.asyncness.is_none(),
      });
    }
    out.modules.push(NativeModule { name: snake(&rust_type), rust_type, methods });
  }

  let mut seen = Vec::new();
  while let Some(name) = wanted.pop() {
    if seen.contains(&name) {
      continue;
    }
    seen.push(name.clone());
    let Some(s) = structs.get(&name) else { continue };
    let mut fields = Vec::new();
    for field in &s.fields {
      let Some(ident) = &field.ident else { continue };
      if !matches!(field.vis, syn::Visibility::Public(_)) {
        continue;
      }
      fields.push((camel(&ident.to_string()), ts_type(&field.ty, &mut wanted)));
    }
    out.types.push(NativeType { name, fields });
  }
  Ok(())
}

/// The contract one `#[service]` block declares: a method per `pub` fn, a
/// record per struct the signatures name and the policy `#[cache]` and
/// `#[writes]` carry.
fn read_service(
  path: &Path,
  rust_type: &str,
  block: &syn::ItemImpl,
  structs: &HashMap<String, syn::ItemStruct>,
) -> Result<Contract, BuildError> {
  let name = snake(rust_type);
  let refused = |what: String| BuildError::Manifest(path.to_path_buf(), format!("service `{name}`: {what}"));
  let mut wanted: Vec<String> = Vec::new();
  let mut service = Service::new();
  for item in &block.items {
    let syn::ImplItem::Fn(f) = item else { continue };
    if !matches!(f.vis, syn::Visibility::Public(_)) {
      continue;
    }
    let method_name = camel(&f.sig.ident.to_string());
    let mut params = Vec::new();
    for arg in &f.sig.inputs {
      let syn::FnArg::Typed(typed) = arg else { continue };
      let syn::Pat::Ident(pat) = &*typed.pat else { continue };
      if type_name(&typed.ty).is_some_and(|t| t == "Caller") {
        continue;
      }
      let key = camel(&pat.ident.to_string());
      let ty = contract_type(&typed.ty, &mut wanted).map_err(|t| refused(format!("`{method_name}` takes `{key}: {t}`, which is outside the value model")))?;
      params.push(Field::new(key, ty));
    }
    let returns = match &f.sig.output {
      syn::ReturnType::Default => Type::Null,
      syn::ReturnType::Type(_, ty) => contract_type(ty, &mut wanted).map_err(|t| refused(format!("`{method_name}` returns `{t}`, which is outside the value model")))?,
    };
    let mut method = Method::new(params, returns);
    for attr in &f.attrs {
      if is_attr(attr, "cache") {
        method = method.cached(read_cache(attr).map_err(|e| refused(format!("`{method_name}`: {e}")))?);
      } else if is_attr(attr, "writes") {
        let tags = attr
          .parse_args_with(syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated)
          .map_err(|e| refused(format!("`{method_name}`: writes: {e}")))?;
        method = method.writes(tags.iter().map(syn::LitStr::value));
      }
    }
    service = service.method(method_name, method);
  }
  let mut contract = Contract::new().service(name.clone(), service);
  let mut seen = Vec::new();
  while let Some(record) = wanted.pop() {
    if seen.contains(&record) {
      continue;
    }
    seen.push(record.clone());
    let Some(s) = structs.get(&record) else {
      return Err(refused(format!("names `{record}`, which is not a struct under `src/`")));
    };
    let mut fields = Vec::new();
    for field in &s.fields {
      let Some(ident) = &field.ident else {
        return Err(refused(format!("`{record}` is a record, so its fields are named")));
      };
      if !matches!(field.vis, syn::Visibility::Public(_)) {
        continue;
      }
      let key = camel(&ident.to_string());
      let ty = contract_type(&field.ty, &mut wanted).map_err(|t| refused(format!("`{record}.{key}` is `{t}`, which is outside the value model")))?;
      fields.push(Field::new(key, ty));
    }
    contract.types.insert(record, TypeDef::Record { fields });
  }
  Ok(contract)
}

/// `#[cache(ttl = "30s", tags = ["catalog"], scope = "shared", stale = "2m")]`,
/// the spelling the proto option and `x-sf-cache` use.
fn read_cache(attr: &syn::Attribute) -> Result<Freshness, String> {
  let mut ttl = None;
  let mut tags = Vec::new();
  let mut scope = Scope::Private;
  let mut stale = None;
  attr
    .parse_nested_meta(|meta| {
      if meta.path.is_ident("ttl") {
        ttl = Some(meta.value()?.parse::<syn::LitStr>()?.value());
      } else if meta.path.is_ident("stale") {
        stale = Some(meta.value()?.parse::<syn::LitStr>()?.value());
      } else if meta.path.is_ident("scope") {
        let value = meta.value()?.parse::<syn::LitStr>()?;
        scope = match value.value().as_str() {
          "private" => Scope::Private,
          "shared" => Scope::Shared,
          "subject" => Scope::Subject,
          other => return Err(meta.error(format!("a scope of `{other}`; it is `private`, `shared` or `subject`"))),
        };
      } else if meta.path.is_ident("tags") {
        let array = meta.value()?.parse::<syn::ExprArray>()?;
        for element in array.elems {
          match element {
            syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(s), .. }) => tags.push(s.value()),
            other => return Err(syn::Error::new_spanned(other, "a tag is a string literal")),
          }
        }
      } else {
        return Err(meta.error("`cache` takes `ttl`, `tags`, `scope` and `stale`"));
      }
      Ok(())
    })
    .map_err(|e| format!("cache: {e}"))?;
  let ttl = ttl.ok_or_else(|| "a cache without a ttl".to_owned())?;
  Ok(Freshness { ttl, tags, scope, stale })
}

fn is_attr(attr: &syn::Attribute, name: &str) -> bool {
  attr.path().segments.last().is_some_and(|s| s.ident == name)
}

fn type_name(ty: &syn::Type) -> Option<String> {
  match ty {
    syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
    _ => None,
  }
}

fn generic_args(segment: &syn::PathSegment) -> Vec<&syn::Type> {
  let syn::PathArguments::AngleBracketed(a) = &segment.arguments else { return Vec::new() };
  a.args
    .iter()
    .filter_map(|arg| match arg {
      syn::GenericArgument::Type(t) => Some(t),
      _ => None,
    })
    .collect()
}

/// The contract type a Rust type declares as. `Err` carries the spelling of
/// a type outside the value model, since a contract is checked at the wire
/// and cannot hold an `unknown`.
fn contract_type(ty: &syn::Type, wanted: &mut Vec<String>) -> Result<Type, String> {
  let spelled = || quote::ToTokens::to_token_stream(ty).to_string().replace(' ', "");
  match ty {
    syn::Type::Reference(r) => contract_type(&r.elem, wanted),
    syn::Type::Tuple(t) if t.elems.is_empty() => Ok(Type::Null),
    syn::Type::Path(p) => {
      let Some(last) = p.path.segments.last() else { return Err(spelled()) };
      let inner = generic_args(last);
      match last.ident.to_string().as_str() {
        "String" | "str" => Ok(Type::Str),
        "bool" => Ok(Type::Bool),
        "i8" | "i16" | "i32" => Ok(Type::I32),
        "u8" | "u16" | "u32" => Ok(Type::U32),
        "i64" | "isize" => Ok(Type::I64),
        "u64" | "usize" => Ok(Type::U64),
        "i128" => Ok(Type::I128),
        "u128" => Ok(Type::U128),
        "f32" => Ok(Type::F32),
        "f64" => Ok(Type::F64),
        "Option" => match inner.first() {
          Some(t) => Ok(Type::optional(contract_type(t, wanted)?)),
          None => Err(spelled()),
        },
        "Vec" => match inner.first() {
          Some(t) => Ok(Type::list(contract_type(t, wanted)?)),
          None => Err(spelled()),
        },
        "HashMap" | "BTreeMap" | "IndexMap" => match inner.as_slice() {
          [keys, values] if matches!(contract_type(keys, &mut Vec::new()), Ok(Type::Str)) => Ok(Type::map(contract_type(values, wanted)?)),
          _ => Err(spelled()),
        },
        "Result" => match inner.first() {
          Some(t) => contract_type(t, wanted),
          None => Err(spelled()),
        },
        other if inner.is_empty() => {
          wanted.push(other.to_owned());
          Ok(Type::named(other))
        }
        _ => Err(spelled()),
      }
    }
    _ => Err(spelled()),
  }
}

/// The TypeScript a Rust type reads as. A type outside the value model is
/// `unknown` rather than a guess and a named struct is recorded so its own
/// declaration is written beside it.
fn ts_type(ty: &syn::Type, wanted: &mut Vec<String>) -> String {
  match ty {
    syn::Type::Reference(r) => ts_type(&r.elem, wanted),
    syn::Type::Path(p) => {
      let Some(last) = p.path.segments.last() else { return "unknown".to_owned() };
      let name = last.ident.to_string();
      let inner = |wanted: &mut Vec<String>| -> Option<String> { generic_args(last).first().map(|t| ts_type(t, wanted)) };
      match name.as_str() {
        "String" | "str" => "string".to_owned(),
        "bool" => "boolean".to_owned(),
        "f32" | "f64" | "i8" | "i16" | "i32" | "u8" | "u16" | "u32" => "number".to_owned(),
        "i64" | "u64" | "i128" | "u128" | "usize" | "isize" => "bigint".to_owned(),
        "Option" => match inner(wanted) {
          Some(t) => format!("{t} | null"),
          None => "unknown".to_owned(),
        },
        "Vec" => match inner(wanted) {
          Some(t) => format!("{t}[]"),
          None => "unknown[]".to_owned(),
        },
        "Result" => inner(wanted).unwrap_or_else(|| "unknown".to_owned()),
        "Value" => "unknown".to_owned(),
        other => {
          wanted.push(other.to_owned());
          other.to_owned()
        }
      }
    }
    _ => "unknown".to_owned(),
  }
}

/// `generated/native.d.ts`: the modules as `ctx.native` sees them.
pub fn declarations(found: &Natives) -> String {
  let mut out = String::from("// Generated by fsr build from the `#[native]` impls. Do not edit.\n\n");
  for ty in &found.types {
    let _ = writeln!(out, "export interface {} {{", ty.name);
    for (name, ts) in &ty.fields {
      let _ = writeln!(out, "  {name}: {ts};");
    }
    out.push_str("}\n\n");
  }
  out.push_str("export interface Natives {\n");
  for module in &found.modules {
    let _ = writeln!(out, "  {}: {{", module.name);
    for m in &module.methods {
      let args: Vec<String> = m.args.iter().map(|(n, t)| format!("{n}: {t}")).collect();
      let input = match args.is_empty() {
        true => String::new(),
        false => format!("input: {{ {} }}", args.join("; ")),
      };
      let returns = match m.is_sync {
        true => m.returns.clone(),
        false => format!("Promise<{}>", m.returns),
      };
      let _ = writeln!(out, "    {}({input}): {returns};", m.name);
    }
    out.push_str("  };\n");
  }
  out.push_str("}\n");
  out
}

fn camel(name: &str) -> String {
  let mut out = String::with_capacity(name.len());
  let mut upper = false;
  for c in name.chars() {
    match c {
      '_' => upper = true,
      c if upper => {
        out.extend(c.to_uppercase());
        upper = false;
      }
      c => out.push(c),
    }
  }
  out
}

/// `Rooms` registers as `rooms`, which is the name a body calls it by unless
/// the host says otherwise.
fn snake(name: &str) -> String {
  let mut out = String::with_capacity(name.len() + 4);
  for (i, c) in name.chars().enumerate() {
    if c.is_ascii_uppercase() {
      if i > 0 {
        out.push('_');
      }
      out.extend(c.to_lowercase());
    } else {
      out.push(c);
    }
  }
  out
}
