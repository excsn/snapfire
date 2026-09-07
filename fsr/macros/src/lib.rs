//! `#[native]` on an `impl` block: the methods a body may reach as
//! `ctx.native.<name>.<method>()`.
//!
//! The attribute writes the dispatcher and nothing else. The `impl` block is
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
