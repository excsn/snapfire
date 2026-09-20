//! `#[native]` on an `impl` block: the methods a body may reach as
//! `ctx.native.<name>.<method>()`. `#[service]` on an `impl` block: a service
//! a body calls as `ctx.services.<name>.<method>()`, served in process, its
//! contract written from the signatures. `#[derive(Record)]` on a struct: a
//! record either may name.
//!
//! Each attribute writes its dispatcher and nothing else. The `impl` block is
//! emitted unchanged, so the methods stay ordinary Rust and a module composes
//! with another by holding it and calling it directly. Only what the block
//! declares `pub` crosses; everything else stays private.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{FnArg, ImplItem, ItemImpl, Pat, ReturnType, parse_macro_input};

/// A method the dispatcher answers: its Rust name, the name a body calls it
/// by, its arguments and whether it suspends.
struct Method {
  ident: syn::Ident,
  wire: String,
  args: Vec<(syn::Ident, String)>,
  is_async: bool,
  returns_unit: bool,
}

#[proc_macro_attribute]
pub fn native(_attr: TokenStream, item: TokenStream) -> TokenStream {
  let block = parse_macro_input!(item as ItemImpl);
  let ty = &block.self_ty;

  let mut methods = Vec::new();
  for item in &block.items {
    let ImplItem::Fn(f) = item else { continue };
    if !matches!(f.vis, syn::Visibility::Public(_)) {
      continue;
    }
    let mut args = Vec::new();
    let mut takes_self = false;
    for arg in &f.sig.inputs {
      match arg {
        FnArg::Receiver(_) => takes_self = true,
        FnArg::Typed(typed) => {
          let Pat::Ident(name) = &*typed.pat else {
            return err(typed, "a native method's argument must be a plain name");
          };
          args.push((name.ident.clone(), camel(&name.ident.to_string())));
        }
      }
    }
    if !takes_self {
      return err(&f.sig, "a native method takes `&self`");
    }
    methods.push(Method {
      ident: f.sig.ident.clone(),
      wire: camel(&f.sig.ident.to_string()),
      args,
      is_async: f.sig.asyncness.is_some(),
      returns_unit: matches!(f.sig.output, ReturnType::Default),
    });
  }

  let mut async_arms = Vec::new();
  let mut sync_arms = Vec::new();
  for m in &methods {
    let name = &m.ident;
    let wire = &m.wire;
    let arg_idents: Vec<_> = m.args.iter().map(|(ident, _)| format_ident!("arg_{}", ident)).collect();
    let arg_keys: Vec<_> = m.args.iter().map(|(_, key)| key.clone()).collect();
    let into_value = match m.returns_unit {
      true => quote! { { answered; ::snapfire_fsr_core::Value::Null } },
      false => quote! { ::snapfire_fsr_runtime::IntoNativeValue::into_native_value(answered) },
    };
    if m.is_async {
      async_arms.push(quote! {
        #wire => {
          let this = ::std::sync::Arc::clone(&held);
          #( let #arg_idents = match ::snapfire_fsr_runtime::native_arg(&args, #arg_keys, #wire) {
              Ok(v) => v,
              Err(e) => return ::std::boxed::Box::pin(async move { Err(e) }),
            }; )*
          ::std::boxed::Box::pin(async move {
            let answered = this.#name(#(#arg_idents),*).await;
            Ok(#into_value)
          })
        }
      });
    } else {
      sync_arms.push(quote! {
        #wire => {
          #( let #arg_idents = match ::snapfire_fsr_runtime::native_arg(&args, #arg_keys, #wire) {
              Ok(v) => v,
              Err(e) => return Some(Err(e)),
            }; )*
          let answered = self.#name(#(#arg_idents),*);
          Some(Ok(#into_value))
        }
      });
    }
  }

  let expanded = quote! {
    #block

    impl ::snapfire_fsr_runtime::Native for #ty {
      fn call(
        &self,
        method: &str,
        args: ::snapfire_fsr_core::ValueMap,
      ) -> ::futures_util::future::BoxFuture<'static, ::std::result::Result<::snapfire_fsr_core::Value, ::snapfire_fsr_runtime::ServiceError>> {
        let held = ::std::sync::Arc::new(self.clone());
        match method {
            #(#async_arms)*
            other => {
              let error = ::snapfire_fsr_runtime::ServiceError::new(
                ::snapfire_fsr_runtime::FailureKind::NotFound,
                "native",
                other,
                ::std::format!("no native method `{}`", other),
              );
              ::std::boxed::Box::pin(async move { Err(error) })
            }
        }
      }

      fn call_sync(
        &self,
        method: &str,
        args: ::snapfire_fsr_core::ValueMap,
      ) -> ::std::option::Option<::std::result::Result<::snapfire_fsr_core::Value, ::snapfire_fsr_runtime::ServiceError>> {
        match method {
          #(#sync_arms)*
          _ => None,
        }
      }
    }
  };
  expanded.into()
}

/// A service method: its Rust name, the name the contract declares it under,
/// its typed arguments, its return type and the policy its attributes carry.
struct ServiceMethod {
  ident: syn::Ident,
  wire: String,
  args: Vec<(syn::Ident, String, syn::Type)>,
  /// The parameter typed `Caller`, filled from the call rather than the arguments.
  caller: Option<syn::Ident>,
  /// Every parameter in signature order, the caller included.
  order: Vec<syn::Ident>,
  returns: syn::Type,
  returns_result: bool,
  is_async: bool,
  cache: Option<Cache>,
  writes: Vec<String>,
}

struct Cache {
  ttl: String,
  tags: Vec<String>,
  scope: String,
  stale: Option<String>,
}

#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
  let mut block = parse_macro_input!(item as ItemImpl);
  let ty = block.self_ty.clone();
  let syn::Type::Path(path) = &*ty else {
    return err(&ty, "a service is an `impl` of a named type");
  };
  let Some(type_name) = path.path.segments.last().map(|s| s.ident.to_string()) else {
    return err(&ty, "a service is an `impl` of a named type");
  };
  let name = snake(&type_name);

  let mut methods = Vec::new();
  for item in &mut block.items {
    let ImplItem::Fn(f) = item else { continue };
    let (cache, writes) = match take_policy(&mut f.attrs) {
      Ok(policy) => policy,
      Err(e) => return e.to_compile_error().into(),
    };
    if !matches!(f.vis, syn::Visibility::Public(_)) {
      if cache.is_some() || !writes.is_empty() {
        return err(&f.sig, "a cache or writes policy sits on a `pub` method; a private one is not in the contract");
      }
      continue;
    }
    let mut args = Vec::new();
    let mut caller = None;
    let mut order = Vec::new();
    let mut takes_self = false;
    for arg in &f.sig.inputs {
      match arg {
        FnArg::Receiver(_) => takes_self = true,
        FnArg::Typed(typed) => {
          let Pat::Ident(pat) = &*typed.pat else {
            return err(typed, "a service method's argument must be a plain name");
          };
          order.push(pat.ident.clone());
          if is_named(&typed.ty, "Caller") {
            if caller.is_some() {
              return err(typed, "a service method takes `Caller` once");
            }
            caller = Some(pat.ident.clone());
            continue;
          }
          args.push((pat.ident.clone(), camel(&pat.ident.to_string()), (*typed.ty).clone()));
        }
      }
    }
    if !takes_self {
      return err(&f.sig, "a service method takes `&self`");
    }
    let returns = match &f.sig.output {
      ReturnType::Default => syn::parse_quote!(()),
      ReturnType::Type(_, ty) => (**ty).clone(),
    };
    methods.push(ServiceMethod {
      ident: f.sig.ident.clone(),
      wire: camel(&f.sig.ident.to_string()),
      args,
      caller,
      order,
      returns_result: is_result(&returns),
      returns,
      is_async: f.sig.asyncness.is_some(),
      cache,
      writes,
    });
  }

  let mut arms = Vec::new();
  let mut declared = Vec::new();
  let mut defined = Vec::new();
  for m in &methods {
    let ident = &m.ident;
    let wire = &m.wire;
    let arg_idents: Vec<_> = m.args.iter().map(|(ident, _, _)| format_ident!("arg_{}", ident)).collect();
    let arg_keys: Vec<_> = m.args.iter().map(|(_, key, _)| key.clone()).collect();
    let arg_types: Vec<_> = m.args.iter().map(|(_, _, ty)| ty.clone()).collect();
    let returns = &m.returns;
    let params: Vec<proc_macro2::TokenStream> = m
      .order
      .iter()
      .map(|ident| match m.caller.as_ref() == Some(ident) {
        true => quote! { caller },
        false => {
          let ident = format_ident!("arg_{}", ident);
          quote! { #ident }
        }
      })
      .collect();
    let awaited = match m.is_async {
      true => quote! { this.#ident(#(#params),*).await },
      false => quote! { this.#ident(#(#params),*) },
    };
    let answer = match m.returns_result {
      true => quote! { answered.map(::snapfire_fsr_runtime::IntoNativeValue::into_native_value) },
      false => quote! { Ok(::snapfire_fsr_runtime::IntoNativeValue::into_native_value(answered)) },
    };
    arms.push(quote! {
      #wire => {
        #( let #arg_idents: #arg_types = match ::snapfire_fsr_runtime::native_arg(&args, #arg_keys, #wire) {
            Ok(v) => v,
            Err(e) => {
              let error = ::snapfire_fsr_runtime::ServiceError::new(e.kind, #name, #wire, e.message);
              return ::std::boxed::Box::pin(async move { Err(error) });
            }
          }; )*
        ::std::boxed::Box::pin(async move {
          let answered = #awaited;
          #answer
        })
      }
    });
    let cached = match &m.cache {
      Some(cache) => {
        let ttl = &cache.ttl;
        let tags = &cache.tags;
        let scope = match cache.scope.as_str() {
          "shared" => quote! { ::snapfire_fsr_service::Scope::Shared },
          "subject" => quote! { ::snapfire_fsr_service::Scope::Subject },
          _ => quote! { ::snapfire_fsr_service::Scope::Private },
        };
        let stale = match &cache.stale {
          Some(stale) => quote! { ::std::option::Option::Some(::std::string::String::from(#stale)) },
          None => quote! { ::std::option::Option::None },
        };
        quote! {
          let method = method.cached(::snapfire_fsr_service::Freshness {
            ttl: ::std::string::String::from(#ttl),
            tags: ::std::vec![#(::std::string::String::from(#tags)),*],
            scope: #scope,
            stale: #stale,
          });
        }
      }
      None => quote! {},
    };
    let writes = match m.writes.is_empty() {
      true => quote! {},
      false => {
        let tags = &m.writes;
        quote! { let method = method.writes([#(#tags),*]); }
      }
    };
    declared.push(quote! {
      .method(#wire, {
        let method = ::snapfire_fsr_service::Method::new(
          ::std::vec![#(::snapfire_fsr_service::Field::new(#arg_keys, <#arg_types as ::snapfire_fsr_service::ContractType>::contract_type())),*],
          <#returns as ::snapfire_fsr_service::ContractType>::contract_type(),
        );
        #cached
        #writes
        method
      })
    });
    defined.push(quote! {
      #( <#arg_types as ::snapfire_fsr_service::ContractType>::define(&mut contract); )*
      <#returns as ::snapfire_fsr_service::ContractType>::define(&mut contract);
    });
  }

  let expanded = quote! {
    #block

    impl ::snapfire_fsr_service::Transport for #ty {
      fn call(
        &self,
        call: ::snapfire_fsr_service::Call,
      ) -> ::futures_util::future::BoxFuture<'static, ::std::result::Result<::snapfire_fsr_core::Value, ::snapfire_fsr_runtime::ServiceError>> {
        let this = ::std::sync::Arc::new(self.clone());
        let caller = ::snapfire_fsr_service::Caller::of(&call);
        let args = call.args;
        match call.method.as_str() {
          #(#arms)*
          other => {
            let error = ::snapfire_fsr_runtime::ServiceError::new(
              ::snapfire_fsr_runtime::FailureKind::NotFound,
              #name,
              other,
              ::std::format!("no method `{}` on `{}`", other, #name),
            );
            ::std::boxed::Box::pin(async move { Err(error) })
          }
        }
      }
    }

    impl ::snapfire_fsr_service::DeclaredService for #ty {
      const NAME: &'static str = #name;

      fn contract() -> ::snapfire_fsr_service::Contract {
        let mut contract = ::snapfire_fsr_service::Contract::new();
        #(#defined)*
        let service = ::snapfire_fsr_service::Service::new() #(#declared)*;
        contract.service(#name, service)
      }
    }
  };
  expanded.into()
}

/// Takes `#[cache(..)]` and `#[writes(..)]` off a method, since nothing
/// downstream knows them. Returns what they said.
fn take_policy(attrs: &mut Vec<syn::Attribute>) -> syn::Result<(Option<Cache>, Vec<String>)> {
  let mut cache = None;
  let mut writes = Vec::new();
  let mut kept = Vec::new();
  for attr in attrs.drain(..) {
    if attr.path().is_ident("cache") {
      let mut ttl = None;
      let mut tags = Vec::new();
      let mut scope = "private".to_owned();
      let mut stale = None;
      attr.parse_nested_meta(|meta| {
        if meta.path.is_ident("ttl") {
          ttl = Some(meta.value()?.parse::<syn::LitStr>()?.value());
        } else if meta.path.is_ident("stale") {
          stale = Some(meta.value()?.parse::<syn::LitStr>()?.value());
        } else if meta.path.is_ident("scope") {
          let value = meta.value()?.parse::<syn::LitStr>()?;
          match value.value().as_str() {
            "private" | "shared" | "subject" => scope = value.value(),
            other => return Err(meta.error(format!("a scope of `{other}`; it is `private`, `shared` or `subject`"))),
          }
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
      })?;
      let Some(ttl) = ttl else {
        return Err(syn::Error::new_spanned(attr, "a cache without a ttl"));
      };
      cache = Some(Cache { ttl, tags, scope, stale });
    } else if attr.path().is_ident("writes") {
      let tags = attr.parse_args_with(syn::punctuated::Punctuated::<syn::LitStr, syn::Token![,]>::parse_terminated)?;
      writes.extend(tags.iter().map(syn::LitStr::value));
    } else {
      kept.push(attr);
    }
  }
  *attrs = kept;
  Ok((cache, writes))
}

fn is_result(ty: &syn::Type) -> bool {
  is_named(ty, "Result")
}

fn is_named(ty: &syn::Type, name: &str) -> bool {
  match ty {
    syn::Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == name),
    _ => false,
  }
}

/// `IntoNativeValue`, `FromNativeValue` and `ContractType` for a struct with
/// named `pub` fields, keyed the way the build declares them, camelCased.
#[proc_macro_derive(Record)]
pub fn record(item: TokenStream) -> TokenStream {
  let input = parse_macro_input!(item as syn::DeriveInput);
  let ident = &input.ident;
  let name = ident.to_string();
  if !input.generics.params.is_empty() {
    return err(&input.generics, "a record has no type parameters");
  }
  let syn::Data::Struct(data) = &input.data else {
    return err(&input, "a record is a struct");
  };
  let syn::Fields::Named(fields) = &data.fields else {
    return err(&data.fields, "a record's fields are named");
  };
  let mut idents = Vec::new();
  let mut keys = Vec::new();
  let mut types = Vec::new();
  for field in &fields.named {
    if !matches!(field.vis, syn::Visibility::Public(_)) {
      return err(field, "a record's fields are all `pub`; a private field cannot cross");
    }
    let ident = field.ident.clone().expect("named");
    keys.push(camel(&ident.to_string()));
    idents.push(ident);
    types.push(field.ty.clone());
  }

  let expanded = quote! {
    impl ::snapfire_fsr_runtime::IntoNativeValue for #ident {
      fn into_native_value(self) -> ::snapfire_fsr_core::Value {
        let mut map = ::snapfire_fsr_core::ValueMap::default();
        #( map.insert(::std::string::String::from(#keys), ::snapfire_fsr_runtime::IntoNativeValue::into_native_value(self.#idents)); )*
        ::snapfire_fsr_core::Value::Map(map)
      }
    }

    impl ::snapfire_fsr_runtime::FromNativeValue for #ident {
      fn from_native_value(value: ::snapfire_fsr_core::Value) -> ::std::option::Option<Self> {
        let ::snapfire_fsr_core::Value::Map(map) = value else { return ::std::option::Option::None };
        ::std::option::Option::Some(Self {
          #( #idents: ::snapfire_fsr_runtime::FromNativeValue::from_native_value(
            map.get(#keys).cloned().unwrap_or(::snapfire_fsr_core::Value::Null),
          )?, )*
        })
      }
    }

    impl ::snapfire_fsr_service::ContractType for #ident {
      fn contract_type() -> ::snapfire_fsr_service::Type {
        ::snapfire_fsr_service::Type::named(#name)
      }

      fn define(contract: &mut ::snapfire_fsr_service::Contract) {
        if contract.types.contains_key(#name) {
          return;
        }
        contract.types.insert(
          ::std::string::String::from(#name),
          ::snapfire_fsr_service::TypeDef::Record {
            fields: ::std::vec![#(::snapfire_fsr_service::Field::new(#keys, <#types as ::snapfire_fsr_service::ContractType>::contract_type())),*],
          },
        );
        #( <#types as ::snapfire_fsr_service::ContractType>::define(contract); )*
      }
    }
  };
  expanded.into()
}

fn err(spanned: &impl syn::spanned::Spanned, message: &str) -> TokenStream {
  syn::Error::new(spanned.span(), message).to_compile_error().into()
}

/// `list_rooms` reads as `listRooms`, the way the contract already renames a
/// method for TypeScript.
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

/// `Fleet` declares as `fleet`, the name a body calls the service by.
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
