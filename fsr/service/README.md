# snapfire_fsr_service

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_service.svg)](https://crates.io/crates/snapfire_fsr_service)
[![Docs.rs](https://docs.rs/snapfire_fsr_service/badge.svg)](https://docs.rs/snapfire_fsr_service)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Typed calls from an FSR application to its backends, with the tokens kept out of application code.

Application code calls a backend by service, method and arguments. It never names a host, a header or a token.

* A **contract** declares the services, their methods and the types they take and return. Every call and every answer is checked against it.
* A **registry** binds the contract to one request's identity and tokens.
* **Interceptors** add the signed-in subject, a bearer token or a request id to each call on its way out.
* A **transport** carries the call: in-process, over HTTP, over gRPC, to GraphQL or to canned mock answers.

A contract is plain data. You can import one from an OpenAPI document, a `.proto` file or a GraphQL schema, build one in Rust or get one from `fsr build`, which reads the TypeScript schemas under `schemas/`. Contracts from several sources merge into one.

Most applications configure their backends in `fsr`'s configuration and never touch this crate. Depend on it directly to write a backend in Rust, add an interceptor or wire services into your own host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_service = "0"
```

| Feature | Adds |
| --- | --- |
| `http` | `HttpTransport`. `Route` and the OpenAPI `import` need no feature |
| `grpc` | `import_proto` and `GrpcTransport` |
| `graphql` | `import_graphql` and `GraphqlTransport` |

Without `grpc` or `graphql` the importers still compile and return an error naming the feature to turn on.

## Quick start

A contract, an in-process implementation and a call:

```rust
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::{Contract, Field, LocalTransport, Method, Service, Services, Type};

#[tokio::main]
async fn main() {
  let contract = Contract::new()
    .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
    .service("fleet", Service::new().method("get", Method::new(vec![Field::new("name", Type::Str)], Type::named("Server"))));

  let transport = LocalTransport::new().method("fleet.get", |call| async move {
    let mut server = ValueMap::default();
    server.insert("name".to_owned(), call.args.get("name").cloned().unwrap_or(Value::Null));
    server.insert("load".to_owned(), Value::F64(0.25));
    Ok(Value::Map(server))
  });

  let services = Services::builder().contract(contract).default_transport(Arc::new(transport)).build();

  let mut args = ValueMap::default();
  args.insert("name".to_owned(), Value::str("db-1"));
  let server = services.bind_anonymous().call("fleet", "get", args).await.unwrap();
  println!("{server:?}");
}
```

A call with a missing or mistyped argument fails before it reaches the transport. An answer that breaks the contract fails as an internal error.

## What to reach for

| You want to | Use |
| --- | --- |
| Declare the types a backend speaks | `Contract::record`, `Contract::union`, `Type`, `Field`, `Variant` |
| Declare the methods a backend exposes | `Service::new().method(..)`, `Method::new` |
| Store or load a contract | `Contract::to_json`, `Contract::from_json` |
| Check every named type resolves | `Contract::validate` |
| Check one call, one answer or one value | `Contract::check_call`, `Contract::check_return`, `Contract::check_value` |
| Build the service layer once per process | `Services::builder()` |
| Give application code a caller for one request | `Services::bind`, `Services::bind_anonymous` |
| Send the subject, a bearer token or a request id | `IdentityInterceptor`, `CredentialInterceptor`, `TraceInterceptor` |
| Stop a call before it leaves | an `Interceptor` that does not call `Next::run` |
| Reuse a backend's answer for a while | `Method::cached(Freshness)`, `Method::writes`, `ServicesBuilder::data_cache`, `Services::invalidate_tags` |
| Implement a method in the same process | `LocalTransport` or `#[service]` from `snapfire_fsr_macros` |
| Reach a backend over HTTP | `HttpTransport`, `Route`, feature `http` |
| Import an OpenAPI document | `import` |
| Import a `.proto` and call its server over gRPC | `import_proto`, `GrpcTransport`, feature `grpc` |
| Import a GraphQL schema and its operations | `import_graphql`, `GraphqlTransport`, feature `graphql` |
| Merge contracts from several sources | `Contract::merge` |
| Run the whole application with no backend | `MockTransport` |
| Hold a token application code cannot read | `Credentials`, `TokenCell` |
| Print the contract as TypeScript declarations | `typescript::declarations` |
