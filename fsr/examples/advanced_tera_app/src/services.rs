use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{
  Contract, CredentialInterceptor, Field, IdentityInterceptor, LocalTransport, Method, Service,
  Services, TraceInterceptor, Type,
};

use crate::state::Fleet;

pub const FLEET: &str = "fleet";

pub fn contract() -> Contract {
  Contract::new()
    .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
    .record("Added", vec![Field::new("count", Type::U32)])
    .service(
      FLEET,
      Service::new()
        .method(
          "list",
          Method::new(vec![Field::new("section", Type::Str)], Type::list(Type::named("Server"))),
        )
        .method("count", Method::new(vec![], Type::U32))
        .method(
          "add",
          Method::new(
            vec![Field::new("name", Type::Str), Field::new("load", Type::F64)],
            Type::named("Added"),
          ),
        ),
    )
}

fn server(name: &str, load: f64) -> Value {
  let mut map = ValueMap::new();
  map.insert("name".to_owned(), Value::str(name));
  map.insert("load".to_owned(), Value::F64(load));
  Value::Map(map)
}

fn arg_str(args: &ValueMap, key: &str) -> String {
  match args.get(key) {
    Some(Value::Str(v)) => v.clone(),
    _ => String::new(),
  }
}

pub fn build(fleet: Fleet) -> Arc<Services> {
  let listing = fleet.clone();
  let counting = fleet.clone();
  let adding = fleet;
  let transport = LocalTransport::new()
    .method("fleet.list", move |call| {
      let fleet = listing.clone();
      async move {
        if arg_str(&call.args, "section") == "down" {
          return Err(ServiceError::new(
            FailureKind::Unavailable,
            FLEET,
            "list",
            "the servers backend is unreachable",
          ));
        }
        Ok(Value::Seq(fleet.list().iter().map(|(name, load)| server(name, *load)).collect()))
      }
    })
    .method("fleet.count", move |_call| {
      let fleet = counting.clone();
      async move { Ok(Value::int(fleet.list().len() as i64)) }
    })
    .method("fleet.add", move |call| {
      let fleet = adding.clone();
      async move {
        let name = arg_str(&call.args, "name");
        let load = match call.args.get("load") {
          Some(Value::F64(load)) => *load,
          _ => 0.0,
        };
        let count = fleet.add(name.clone(), load).map_err(|_| {
          ServiceError::new(FailureKind::Conflict, FLEET, "add", format!("server `{name}` already exists"))
        })?;
        let mut out = ValueMap::new();
        out.insert("count".to_owned(), Value::int(count as i64));
        Ok(Value::Map(out))
      }
    });

  Services::builder()
    .contract(contract())
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(Arc::new(transport))
    .build()
}
