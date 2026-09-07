//! The application's own Rust, read the way a body is read: syntactically,
//! without compiling it. A `#[native]` `impl` block names the methods a body
//! may reach as `ctx.native.<module>.<method>()`, and the types they mention
//! come from the structs beside them.
//!
//! Reading rather than expanding is what keeps the ordering honest. `build.rs`
//! runs before the crate compiles, so a macro's output does not exist yet;
//! the source does.

use std::fmt::Write as _;
use std::path::Path;

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

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Natives {
  pub modules: Vec<NativeModule>,
  pub types: Vec<NativeType>,
}

impl Natives {
  pub fn is_empty(&self) -> bool {
    self.modules.is_empty()
  }
}

/// Every `.rs` file under `dir`, read for `#[native]` impls and the types they
/// name. A directory that does not exist is not an error: an application with
/// no Rust has no natives.
pub fn read(dir: &Path) -> Result<Natives, BuildError> {
  let mut sources = Vec::new();
  collect(dir, &mut sources)?;
  let mut found = Natives::default();
  for (path, text) in &sources {
    let file = syn::parse_file(text).map_err(|e| BuildError::Manifest(path.clone(), format!("native read: {e}")))?;
    read_file(&file, &mut found);
  }
  found.modules.sort_by(|a, b| a.name.cmp(&b.name));
  found.types.sort_by(|a, b| a.name.cmp(&b.name));
  found.types.dedup();
  Ok(found)
}

fn collect(dir: &Path, out: &mut Vec<(std::path::PathBuf, String)>) -> Result<(), BuildError> {
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

fn read_file(file: &syn::File, out: &mut Natives) {
  let mut wanted: Vec<String> = Vec::new();
  for item in &file.items {
    let syn::Item::Impl(block) = item else { continue };
    if !block.attrs.iter().any(is_native_attr) {
      continue;
    }
    let Some(rust_type) = type_name(&block.self_ty) else { continue };
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

  for item in &file.items {
    let syn::Item::Struct(s) = item else { continue };
    let name = s.ident.to_string();
    if !wanted.contains(&name) {
      continue;
    }
    let mut fields = Vec::new();
    for field in &s.fields {
      let Some(ident) = &field.ident else { continue };
      if !matches!(field.vis, syn::Visibility::Public(_)) {
        continue;
      }
      let mut more = Vec::new();
      fields.push((camel(&ident.to_string()), ts_type(&field.ty, &mut more)));
    }
    out.types.push(NativeType { name, fields });
  }
}

fn is_native_attr(attr: &syn::Attribute) -> bool {
  attr.path().segments.last().is_some_and(|s| s.ident == "native")
}

fn type_name(ty: &syn::Type) -> Option<String> {
  match ty {
    syn::Type::Path(p) => p.path.segments.last().map(|s| s.ident.to_string()),
    _ => None,
  }
}

/// The TypeScript a Rust type reads as. A type outside the value model is
/// `unknown` rather than a guess, and a named struct is recorded so its own
/// declaration is written beside it.
fn ts_type(ty: &syn::Type, wanted: &mut Vec<String>) -> String {
  match ty {
    syn::Type::Reference(r) => ts_type(&r.elem, wanted),
    syn::Type::Path(p) => {
      let Some(last) = p.path.segments.last() else { return "unknown".to_owned() };
      let name = last.ident.to_string();
      let inner = |wanted: &mut Vec<String>| -> Option<String> {
        let syn::PathArguments::AngleBracketed(a) = &last.arguments else { return None };
        a.args.iter().find_map(|arg| match arg {
          syn::GenericArgument::Type(t) => Some(ts_type(t, wanted)),
          _ => None,
        })
      };
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
