# Usage Guide: snapfire_fsr_service

How to declare a contract, build the service registry, bind it to a request and call through it, with the transports and interceptors that sit underneath.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [An In-Process Service](#an-in-process-service)
  * [An HTTP Backend with Identity](#an-http-backend-with-identity)
* [Describing Types](#describing-types)
  * [Records](#records)
  * [Unions](#unions)
  * [Integer Widths](#integer-widths)
* [Declaring Services and Methods](#declaring-services-and-methods)
* [Storing the Contract as JSON](#storing-the-contract-as-json)
* [Validating the Reference Graph](#validating-the-reference-graph)
* [Checking a Call](#checking-a-call)
* [Checking a Response](#checking-a-response)
* [Building the Registry](#building-the-registry)
  * [Per-Service Transports](#per-service-transports)
  * [Turning Response Checking Off](#turning-response-checking-off)
* [Binding a Request](#binding-a-request)
* [Implementing a Method in Process](#implementing-a-method-in-process)
* [Calling a Backend over HTTP](#calling-a-backend-over-http)
  * [Shaping the Route](#shaping-the-route)
  * [Metadata Becomes Headers](#metadata-becomes-headers)
  * [Statuses Become Failure Kinds](#statuses-become-failure-kinds)
* [Importing a Proto and Calling over gRPC](#importing-a-proto-and-calling-over-grpc)
  * [What a Proto Becomes](#what-a-proto-becomes)
  * [Codes Become Failure Kinds](#codes-become-failure-kinds)
* [Adding an Interceptor](#adding-an-interceptor)
  * [The Built-In Three](#the-built-in-three)
  * [Writing Your Own](#writing-your-own)
  * [Short-Circuiting the Chain](#short-circuiting-the-chain)
* [Caching a Method's Answers](#caching-a-methods-answers)
* [Holding Credentials](#holding-credentials)
* [Testing Against a Mock](#testing-against-a-mock)
* [Wiring the Layer into an Application](#wiring-the-layer-into-an-application)
* [Why the Contract Is Neutral Data](#why-the-contract-is-neutral-data)
* [Error Handling](#error-handling)

## Core Concepts

* **Contract** is the neutral artifact: a set of named type definitions plus a set of named services, each with named methods. It is data rather than code and it serialises to JSON.
* **Value model** is `snapfire_fsr_core::Value`, the vocabulary every contract type projects onto. The contract never mentions a Rust type or a TypeScript type.
* **Type** is one entry in the contract's type vocabulary: a scalar, a container or a `Named` reference to a definition elsewhere in the same contract.
* **TypeDef** is a definition a `Named` reference resolves to, either a record with fields or a union with variants.
* **Checking** is the strict comparison of a `Value` against a `Type` at a named position. Unknown record fields are errors, missing non-optional fields are errors and the error carries the path that failed.
* **Services** is the per-process registry: one contract, one interceptor list, a transport per service and an optional default transport.
* **ServiceHandle** is what application code holds. It exposes exactly one operation, `call(service, method, args)`. It comes from `snapfire_fsr_runtime`.
* **Bind** turns the registry into a handle for one request by fixing that request's identity and credentials into it.
* **Call** is one outbound invocation as it travels the chain: service, method, arguments, identity, metadata and credentials.
* **Metadata** is the string-keyed side channel interceptors write and a transport interprets. The HTTP transport turns each string entry into a request header.
* **Interceptor** is one step of the outbound path, an ordered list rather than a workflow engine. It receives the `Call` and a `Next`.
* **Next** is the rest of the chain. Calling `run` continues; returning without calling it short-circuits.
* **Transport** is the last step, the thing that actually produces a `Value`: in-process, mocked or over HTTP.
* **Credentials** is the custody trait. Interceptors read a token through it; application code has no path to it.
* **FailureKind** is the runtime's failure taxonomy (`Unauthorized`, `NotFound`, `Invalid`, `Conflict`, `Timeout`, `Unavailable`, `Internal`) that every error at this boundary carries.

## Quick Start

### An In-Process Service

A contract, an implementation and a call, with no network anywhere.

```rust
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::{Contract, Field, LocalTransport, Method, Service, Services, Type};

#[tokio::main]
async fn main() {
  let contract = Contract::new()
    .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
    .service(
      "fleet",
      Service::new()
        .method("get", Method::new(vec![Field::new("name", Type::Str)], Type::named("Server")))
        .method("count", Method::new(vec![], Type::U32)),
    );
  contract.validate().expect("every named type resolves");

  let transport = LocalTransport::new().method("fleet.get", |call| async move {
    let name = match call.args.get("name") {
      Some(Value::Str(name)) => name.clone(),
      _ => unreachable!("the contract checked this"),
    };
    let mut server = ValueMap::new();
    server.insert("name".to_owned(), Value::str(name));
    server.insert("load".to_owned(), Value::F64(0.25));
    Ok(Value::Map(server))
  });

  let services = Services::builder()
    .contract(contract)
    .default_transport(Arc::new(transport))
    .build();

  let mut args = ValueMap::new();
  args.insert("name".to_owned(), Value::str("db-1"));
  let server = services.bind_anonymous().call("fleet", "get", args).await.unwrap();
  println!("{server:?}");
}
```

### An HTTP Backend with Identity

The same contract against a gateway, with the subject and a bearer token attached by the chain.

```rust
use std::sync::Arc;

use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::Identity;
use snapfire_fsr_service::{
  CredentialInterceptor, HttpTransport, IdentityInterceptor, Services, TraceInterceptor,
};
use snapfire_fsr_session::TokenCell;

#[tokio::main]
async fn main() {
  let services = Services::builder()
    .contract(fleet_contract())
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(Arc::new(HttpTransport::new("https://api.internal")))
    .build();

  let tokens = TokenCell::default();
  tokens.set("access_token", Value::str("secret-abc"));
  let identity = Identity { subject: "alice".into(), claims: Default::default() };

  let mut args = ValueMap::new();
  args.insert("name".to_owned(), Value::str("web-1"));
  let server = services
    .bind(Some(identity), Arc::new(tokens))
    .call("fleet", "get", args)
    .await
    .unwrap();
  println!("{server:?}");
}
```

That call goes out as `POST https://api.internal/fleet/get` with a body of `{"name":"web-1"}`, an `authorization: Bearer secret-abc` header, an `x-sf-subject: alice` header and an `x-sf-request-id` header.

## Describing Types

`Type` is the whole vocabulary. Scalars are named by width, containers wrap another `Type` and `Type::named` points at a definition in the same contract.

```rust
use snapfire_fsr_service::{ScalarKind, Type};

let a = Type::Str;
let b = Type::optional(Type::Str);
let c = Type::list(Type::named("Server"));
let d = Type::map(Type::I64);
let e = Type::Array(ScalarKind::F64);
let f = Type::Bytes;

assert_eq!(c.describe(), "list<Server>");
assert_eq!(d.describe(), "map<str, i64>");
assert_eq!(e.describe(), "array<f64>");
```

`describe` is what error messages print, so it is the shortest way to see what a type is.

### Records

A record is an ordered list of named fields. Fields whose type is `Optional` may be absent from a value; every other field must be present. A field the record does not declare is an error.

```rust
use snapfire_fsr_service::{Contract, Field, Type};

let contract = Contract::new().record(
  "User",
  vec![
    Field::new("id", Type::U64),
    Field::new("name", Type::Str),
    Field::new("nickname", Type::optional(Type::Str)),
  ],
);
```

A record projects onto `Value::Map`.

### Unions

A union is a list of tagged variants. `Variant::unit` is an arm with no payload, which is how a proto3 or OpenAPI enum lands here; `Variant::with` carries one.

```rust
use snapfire_fsr_service::{Contract, Type, Variant};

let contract = Contract::new()
  .union("Tier", vec![Variant::unit("free"), Variant::unit("paid"), Variant::with("trial", Type::U32)])
  .union("Pet", vec![Variant::with("dog", Type::named("Dog")), Variant::with("cat", Type::named("Cat"))]);
```

A union projects onto `Value::Variant { tag, payload }`. A unit arm with a payload attached fails and so does a payload arm with none.

### Integer Widths

The vocabulary names the width, so the boundary can reject what a JSON number would silently mangle. `Type::U64` accepts the whole unsigned 64-bit range, which is why a `u64` identifier cannot be truncated at 2^53 on the way to TypeScript: the width travels in the artifact and the check runs against the value model rather than against a double.

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_service::{Contract, Type};

let contract = Contract::new();

assert!(contract.check_value(&Type::U64, &Value::Int(i128::from(u64::MAX)), "id").is_ok());
assert!(contract.check_value(&Type::U64, &Value::Int(-1), "id").is_err());
assert!(contract.check_value(&Type::I32, &Value::Int(i128::from(i32::MAX) + 1), "n").is_err());
assert!(contract.check_value(&Type::U128, &Value::uint(u128::MAX), "big").is_ok());
assert!(contract.check_value(&Type::F64, &Value::Int(1), "ratio").is_err());
```

The last line is the rule worth remembering: there is no silent numeric coercion. An integer offered where an `f64` is declared is a mismatch, not a promotion.

## Declaring Services and Methods

A service is a named bag of methods; a method is a parameter list plus a return type. Both builders take ownership and return themselves, so a contract reads as one expression.

```rust
use snapfire_fsr_service::{Contract, Field, Method, Service, Type};

let contract = Contract::new()
  .record("Server", vec![Field::new("name", Type::Str), Field::new("load", Type::F64)])
  .service(
    "fleet",
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
  );

let signature = contract.method("fleet", "count").unwrap();
assert_eq!(signature.returns, Type::U32);
```

Both `types` and `services` are `IndexMap`s, so declaration order survives into the JSON artifact.

## Storing the Contract as JSON

`to_json` writes pretty JSON and `from_json` reads it back to an equal value.

```rust
use snapfire_fsr_service::Contract;

let json = contract.to_json();
let back = Contract::from_json(&json).unwrap();
assert_eq!(back, contract);
```

The encoding is externally tagged, so a scalar is a bare string and a container is a single-key object.

```json
{
  "types": {
    "User": {
      "record": {
        "fields": [
          { "name": "id", "type": "u64" },
          { "name": "name", "type": "str" },
          { "name": "nickname", "type": { "optional": "str" } }
        ]
      }
    },
    "Tier": {
      "union": {
        "variants": [
          { "tag": "free" },
          { "tag": "trial", "type": "u32" }
        ]
      }
    }
  },
  "services": {
    "users": {
      "methods": {
        "get": {
          "params": [{ "name": "id", "type": "u64" }],
          "returns": { "named": "User" }
        }
      }
    }
  }
}
```

`types`, `services`, `methods` and `params` all default to empty when absent; a unit variant omits its `type` key entirely.

## Validating the Reference Graph

`validate` walks every record field, every variant payload, every parameter and every return type, then fails on the first `Named` that has no definition. Run it once where the contract is built, so a lookup at request time can trust the graph.

```rust
use snapfire_fsr_service::{Contract, ContractError, Method, Service, Type};

let broken = Contract::new()
  .service("users", Service::new().method("get", Method::new(vec![], Type::named("Ghost"))));

assert!(matches!(
  broken.validate().unwrap_err(),
  ContractError::UnknownType { name, .. } if name == "Ghost"
));
```

## Checking a Call

`check_call` is the gate every call passes before a transport sees it. Arguments arrive as a `ValueMap` keyed by parameter name; order does not matter, presence and type do.

```rust
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_service::ContractError;

let mut args = ValueMap::new();
args.insert("id".to_owned(), Value::Int(7));
assert!(contract.check_call("users", "get", &args).is_ok());

assert!(contract.check_call("users", "list", &ValueMap::new()).is_ok());

assert!(matches!(
  contract.check_call("users", "get", &ValueMap::new()).unwrap_err(),
  ContractError::MissingField { field, .. } if field == "id"
));

let mut wrong = ValueMap::new();
wrong.insert("id".to_owned(), Value::str("7"));
assert_eq!(
  contract.check_call("users", "get", &wrong).unwrap_err().to_string(),
  "users.get.id: expected u64, found str",
);
```

The second call passes because `list` declares `limit` as `Optional`; an optional parameter may be omitted. An argument the method does not declare is a `ContractError::UnknownField`.

## Checking a Response

`check_return` runs the same machinery against the declared return type and names the failing position rather than just the failing type.

```rust
use snapfire_fsr_core::Value;

let bad = Value::Seq(vec![user_value(1, "alice"), Value::str("nope")]);
let err = contract.check_return("users", "list", &bad).unwrap_err();
assert!(err.to_string().starts_with("users.list()[1]:"));
```

The path grows as the checker descends: `[i]` for a list index, `.tag` for a variant payload and `.field` for a record field or a map key. The registry runs this on every response by default, so a backend that breaks its own contract surfaces as a failure here rather than as a confusing shape three layers up.

`check_value` is the same check against an arbitrary type at an arbitrary path, for a validator that is not going through a method.

```rust
use snapfire_fsr_service::Type;

contract.check_value(&Type::map(Type::I64), &some_map, "config").unwrap();
```

## Building the Registry

`Services::builder()` collects the contract, the interceptors and the transports, then `build` freezes them into an `Arc<Services>` for the life of the process.

```rust
use std::sync::Arc;

use snapfire_fsr_service::{HttpTransport, IdentityInterceptor, Services};

let services = Services::builder()
  .contract(contract())
  .intercept(Arc::new(IdentityInterceptor::new()))
  .default_transport(Arc::new(HttpTransport::new("https://api.internal")))
  .build();
```

`intercept` appends, so the chain runs in the order the calls were made. Every transport gets its own chain over the same interceptor list.

### Per-Service Transports

`transport` binds one named service; `default_transport` catches everything else. A service with its own transport never falls through.

```rust
let services = Services::builder()
  .contract(contract())
  .transport("fleet", Arc::new(HttpTransport::new("https://fleet.internal")))
  .transport("billing", Arc::new(HttpTransport::new("https://billing.internal")))
  .default_transport(Arc::new(MockTransport::new()))
  .build();
```

With neither a named transport nor a default, a call to that service fails with `FailureKind::Unavailable` and a message naming the service that has no transport bound.

### Turning Response Checking Off

`Services::builder()` starts with response checking on. Turn it off only when a backend is trusted to honour its own contract and the check is measurably in the way.

```rust
let services = Services::builder()
  .contract(contract())
  .check_responses(false)
  .default_transport(Arc::new(transport))
  .build();
```

Arguments are checked either way. `check_responses` governs only `check_return`.

## Binding a Request

`bind` fixes one request's identity and credential custody into a `ServiceHandle`. Handing that handle to application code is safe because it can only call.

```rust
use std::sync::Arc;

use snapfire_fsr_runtime::{Identity, RequestCtx};
use snapfire_fsr_session::TokenCell;

let ctx = RequestCtx {
  params: matched.params,
  session: incoming.session,
  csrf: incoming.csrf,
  services: services.bind(incoming.session.identity(), incoming.credentials),
};
```

For a request with no signed-in user, `bind_anonymous` is the same call with no identity and `NoCredentials`.

```rust
let handle = services.bind_anonymous();
```

A `ServiceHandle` clones freely and a default-constructed one is unbound: `is_bound` returns false and every call fails with `FailureKind::Unavailable` rather than pretending to succeed.

## Implementing a Method in Process

`LocalTransport` keys closures by `service.method`. It is the same machinery as a remote call with the network removed, so the contract still checks both directions.

```rust
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::LocalTransport;

let transport = LocalTransport::new()
  .method("fleet.count", move |_call| {
    let fleet = counting.clone();
    async move { Ok(Value::int(fleet.list().len() as i64)) }
  })
  .method("fleet.add", move |call| {
    let fleet = adding.clone();
    async move {
      let name = match call.args.get("name") {
        Some(Value::Str(name)) => name.clone(),
        _ => String::new(),
      };
      let count = fleet.add(name.clone(), 0.0).map_err(|_| {
        ServiceError::new(FailureKind::Conflict, "fleet", "add", format!("server `{name}` already exists"))
      })?;
      let mut out = ValueMap::new();
      out.insert("count".to_owned(), Value::int(count as i64));
      Ok(Value::Map(out))
    }
  });
```

A path with no closure registered fails with `FailureKind::NotFound`.

## Calling a Backend over HTTP

`HttpTransport::new` takes the base URL, trims any trailing slash and posts each method to `{base}/{service}/{method}` with the arguments as a JSON body. That is the whole default; nothing else has to be configured for a plain gateway.

```rust
use std::time::Duration;

use snapfire_fsr_service::HttpTransport;

let plain = HttpTransport::new("https://api.internal");
let timed = HttpTransport::with_timeout("https://api.internal", Duration::from_secs(5));
let custom = HttpTransport::with_client("https://api.internal", my_reqwest_client);
```

Use `with_timeout` when the deadline is all you want to change and `with_client` when you need connection pooling, proxies or TLS settings of your own.

A 2xx response with a body is parsed as JSON and converted through `snapfire_fsr_payload::json_to_value`; a 2xx response with an empty body becomes `Value::Null`.

### Shaping the Route

`route` overrides one `service.method` with a verb and a path template. A `{name}` segment takes the argument of that name; that argument is then not repeated in the body.

```rust
use snapfire_fsr_service::{HttpTransport, Route};

let transport = HttpTransport::new("https://api.internal")
  .route("fleet.get", Route::get("/servers/{name}"))
  .route("fleet.add", Route::post("/servers"))
  .route("fleet.drop", Route::new("DELETE", "/servers/{name}"));
```

With that first route, `fleet.get` with `name = "web-1"` goes out as `GET /servers/web-1` and an empty body. For `GET` and `DELETE` the arguments left after the template has taken its share become query parameters; for every other verb they become the JSON body.

A `{name}` naming an argument that is not present renders as an empty segment and consumes nothing, so keep template names to parameters the contract declares as non-optional.

### Metadata Becomes Headers

Every metadata entry whose value is a `Value::Str` becomes a request header with the same key. Entries of any other shape are skipped. That is the whole mechanism by which identity and the bearer token reach the backend without application code naming either.

```rust
let services = Services::builder()
  .contract(contract())
  .intercept(Arc::new(IdentityInterceptor::new()))
  .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
  .default_transport(Arc::new(HttpTransport::new(&base)))
  .build();
```

### Statuses Become Failure Kinds

A non-2xx response becomes a `ServiceError` whose message is `{status}: {body}` and whose kind comes from `kind_for_status`.

| Status | Kind |
| --- | --- |
| 401, 403 | `Unauthorized` |
| 404 | `NotFound` |
| 409 | `Conflict` |
| 408, 504 | `Timeout` |
| 502, 503 | `Unavailable` |
| any other 4xx | `Invalid` |
| anything else | `Internal` |

The named statuses are matched before the 4xx range, so 401, 404, 408 and 409 never fall through to `Invalid`. A transport-level failure never panics: a reqwest timeout is `Timeout` and any other send failure, an unreachable host included, is `Unavailable`. A body that will not parse as JSON is `Internal`.

```rust
use snapfire_fsr_service::kind_for_status;
use snapfire_fsr_runtime::FailureKind;

assert_eq!(kind_for_status(409), FailureKind::Conflict);
assert_eq!(kind_for_status(422), FailureKind::Invalid);
assert_eq!(kind_for_status(500), FailureKind::Internal);
```

## Importing a Proto and Calling over gRPC

With the `grpc` feature, `import_proto` compiles a `.proto` file with protox rather than protoc and returns the contract plus the descriptors the transport encodes with. `GrpcTransport::new` takes the server's URL and that import; nothing connects until the first call and no client code is generated on this side.

```rust
use std::path::Path;
use std::sync::Arc;

use snapfire_fsr_service::{import_proto, GrpcTransport, Services};

let imported = import_proto(Path::new("clients/inventory.proto"), "inventory")?;
let services = Services::builder()
  .contract(imported.contract.clone())
  .transport("inventory", Arc::new(GrpcTransport::new("http://127.0.0.1:8082", &imported)?))
  .build();
```

`import_proto_source` takes the source text instead of a path, for a proto held in memory; it can import only the Google well-known types.

### What a Proto Becomes

A file with one service takes the name you pass; with several, each keeps its own. Methods go lowerCamel, so `GetStock` is `getStock`, while the request path keeps the proto name. A method's parameters are its request message's fields, the way an OpenAPI body spreads; `google.protobuf.Empty` in is no parameters and out is `null`. Streaming methods are refused.

| Proto | Contract |
| --- | --- |
| `int32`, `sint32`, `sfixed32` | `I32` |
| `int64`, `sint64`, `sfixed64` | `I64`, a `bigint` in TypeScript |
| `uint32`, `fixed32`, `uint64`, `fixed64` | `U32`, `U64` |
| `float`, `double`, `bool`, `string`, `bytes` | `F32`, `F64`, `Bool`, `Str`, `Bytes` |
| `repeated T` | `List<T>` |
| `map<string, T>` | `Map<T>`; any other key is refused |
| an enum | `Str`, the value's name |
| a message field, a proto3 `optional` or a `oneof` member | `Optional<..>`, since each may be absent |
| a nested message `Outer.Inner` | the record `OuterInner`; the package drops |
| `Timestamp`, `Duration` | `Str`, RFC 3339 or `1.5s` |
| the wrapper types | `Optional` of the scalar |
| `Any`, `Struct`, `Value`, `ListValue`, `FieldMask` | refused |

On the wire the arguments become the request message field by field and the response comes back with every field present, an unset message or `optional` as `null` and an unset scalar at its default. Integers are checked against their width before they leave.

### Codes Become Failure Kinds

| Code | Kind |
| --- | --- |
| `NotFound` | `NotFound` |
| `InvalidArgument`, `OutOfRange`, `FailedPrecondition` | `Invalid` |
| `Unauthenticated`, `PermissionDenied` | `Unauthorized` |
| `AlreadyExists`, `Aborted` | `Conflict` |
| `DeadlineExceeded` | `Timeout` |
| `Unavailable` | `Unavailable` |
| anything else | `Internal` |

An argument the message has no field for or one outside its width is `Invalid` before the call leaves; a method the import did not see is `NotFound`. Metadata entries with string values become request metadata, the same rule as headers over HTTP.

## Adding an Interceptor

An interceptor takes the `Call` and the rest of the chain. This is an ordered list of functions, not a workflow engine: there is no branching, no retry policy language and no declarative composition, only the order you registered them in.

### The Built-In Three

Three ship with the crate. Each writes one metadata key.

| Interceptor | Metadata key | Value |
| --- | --- | --- |
| `IdentityInterceptor` | `x-sf-subject` | the identity's `subject`, only when the request has one |
| `CredentialInterceptor` | `authorization` | the named credential prefixed with `Bearer `, only when custody holds it as a string |
| `TraceInterceptor` | `x-sf-request-id` | a 16-digit hex counter, only when the key is not already set |

Each key is overridable and `CredentialInterceptor` also takes a different scheme.

```rust
use snapfire_fsr_service::{CredentialInterceptor, IdentityInterceptor, TraceInterceptor};

let identity = IdentityInterceptor::new().key("x-user");
let trace = TraceInterceptor::new().key("x-correlation-id");
let credential = CredentialInterceptor::bearer("access_token")
  .header("x-api-key")
  .scheme("");
```

`TraceInterceptor` leaves an existing id alone, so a request id minted at the edge survives the whole fanout instead of being replaced per call. It also emits a `tracing` debug event on target `fsr::service` carrying the service, the method and the request id.

An anonymous request attaches neither the subject nor the token: both interceptors write nothing when there is nothing to write.

### Writing Your Own

Implement `Interceptor` and call `next.run(call)` to continue.

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::ServiceError;
use snapfire_fsr_service::{Call, Interceptor, Next};

struct Tenant(String);

impl Interceptor for Tenant {
  fn call(&self, mut call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    call.set_metadata("x-tenant", self.0.clone());
    next.run(call)
  }
}
```

`set_metadata` writes a string entry and `metadata_str` reads one back, which is how an interceptor sees what an earlier one already wrote.

### Short-Circuiting the Chain

Not calling `next.run` ends the call there. Nothing further in the chain runs and the transport is never reached, which is where a cache, a circuit breaker or a policy gate belongs.

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::{FailureKind, ServiceError};
use snapfire_fsr_service::{Call, Interceptor, Next};

struct RequireIdentity;

impl Interceptor for RequireIdentity {
  fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>> {
    if call.identity.is_none() {
      let error = ServiceError::new(
        FailureKind::Unauthorized,
        &call.service,
        &call.method,
        "no identity on the request",
      );
      return Box::pin(async move { Err(error) });
    }
    next.run(call)
  }
}
```

## Caching a Method's Answers

Freshness is the data owner's knowledge, so it is declared on the contract, per method, and the registry does the rest. `ttl` says how long an answer holds, `tags` name what a write drops it under, `scope` says who may share it, and `stale` opens a window after `ttl` in which the last answer is served while a refresh runs behind it.

```rust
use snapfire_fsr_service::{Freshness, Method, Service, Type};

let catalog = Service::new()
  .method("list", Method::new(vec![], Type::list(Type::named("Product"))).cached(Freshness::ttl("30s").tags(["catalog"]).shared().stale("2m")))
  .method("mine", Method::new(vec![], Type::list(Type::named("Order"))).cached(Freshness::ttl("1m").per_subject()))
  .method("add", Method::new(vec![Field::new("name", Type::Str)], Type::Null).writes(["catalog"]));
```

An OpenAPI operation says the same with `x-sf-cache` and `x-sf-writes`; a `.proto` carries no annotation yet.

```json
{ "get": { "operationId": "list", "x-sf-cache": { "ttl": "30s", "tags": ["catalog"], "scope": "shared", "stale": "2m" }, "responses": { "200": { "..." : "..." } } } }
```

The builder turns it on with a capacity per policy:

```rust
let services = Services::builder().contract(contract).default_transport(transport).data_cache(500).build();
let anon = services.bind_anonymous();
anon.call("catalog", "list", ValueMap::new()).await?;
anon.call("catalog", "list", ValueMap::new()).await?;
assert_eq!(services.data_cache().unwrap().hits(), 1);
anon.call("catalog", "add", args).await?;
anon.call("catalog", "list", ValueMap::new()).await?;
assert_eq!(services.data_cache().unwrap().misses(), 2, "the write dropped the tag");
services.invalidate_tags(["catalog"]);
```

The scope is the safety rule. `private`, the default, means an identified call never reads or writes the cache, so a bearer-carrying answer cannot be served to someone else by accident; anonymous calls share one entry. `shared` serves everyone the same entry. `subject` keeps one entry per subject. A miss always runs the caller's own call with its own credentials; only the refresh a `stale` window starts runs anonymously, which is why `stale` is refused off `shared` scope by `validate` and by `try_build`.

A failure is never stored: the next call asks again, and a refresh that fails keeps the last answer. Two calls with the same arguments in another order share an entry, since the key renders maps by sorted key.

## Holding Credentials

`Credentials` is a two-method trait: `get` reads a credential by name, `set` writes one back so a refresh lands where the session will persist it. `TokenCell` from `snapfire_fsr_session` is the production implementation and `NoCredentials` is the empty one.

```rust
use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_session::TokenCell;

let tokens = TokenCell::default();
tokens.set("access_token", Value::str("secret-abc"));

let handle = services.bind(Some(identity), Arc::new(tokens));
```

The `Arc<dyn Credentials>` lives on the `Call`, which only interceptors and transports ever see. What comes back from `bind` is a `ServiceHandle`; a `ServiceHandle` has no accessor for it. That is the whole custody claim: application code never sees a token because there is no method that would return one.

## Testing Against a Mock

A transport is a block, so `MockTransport` swaps in for the real one with no change anywhere else. It answers from canned responses and records what the chain produced.

```rust
use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_runtime::FailureKind;
use snapfire_fsr_service::{MockTransport, Services};

let transport = Arc::new(
  MockTransport::new()
    .returns("fleet.count", Value::Int(3))
    .fails("fleet.add", FailureKind::Conflict, "already exists"),
);

let services = Services::builder()
  .contract(contract())
  .intercept(Arc::new(IdentityInterceptor::new()))
  .default_transport(transport.clone())
  .build();

let handle = services.bind(Some(identity), Arc::new(tokens));
assert_eq!(handle.call("fleet", "count", ValueMap::new()).await.unwrap(), Value::Int(3));

assert_eq!(transport.last_metadata("x-sf-subject").as_deref(), Some("alice"));
assert!(transport.calls().iter().any(|(path, _, _)| path == "fleet.count"));
```

`calls` returns one `(path, args, metadata)` triple per call in order, which is how an interceptor gets tested without a backend. A path with no canned response fails with `FailureKind::NotFound`. Asserting `calls().is_empty()` is how you prove a rejected call or a short circuit never reached the wire.

## Wiring the Layer into an Application

Build the registry once, at startup, next to the state its implementations need.

```rust
pub fn build(fleet: Fleet) -> Arc<Services> {
  let transport = LocalTransport::new()
    .method("fleet.count", move |_call| {
      let fleet = fleet.clone();
      async move { Ok(Value::int(fleet.list().len() as i64)) }
    });

  Services::builder()
    .contract(contract())
    .intercept(Arc::new(TraceInterceptor::new()))
    .intercept(Arc::new(IdentityInterceptor::new()))
    .intercept(Arc::new(CredentialInterceptor::bearer("access_token")))
    .default_transport(Arc::new(transport))
    .build()
}
```

Bind per request where the `RequestCtx` is assembled. Application code reads it off the context.

```rust
let ctx = RequestCtx {
  params: matched.params,
  session: opened.cell.clone(),
  csrf: None,
  services: app.services.bind(opened.cell.identity(), Arc::new(opened.tokens.clone())),
};
```

A loader asks for the capability and nothing else.

```rust
sources.insert_fn("servers_loader", move |ctx| async move {
  let mut args = ValueMap::new();
  args.insert("section".to_owned(), Value::Str(ctx.params.get("section").cloned().unwrap_or_default()));
  let servers = ctx.services.call("fleet", "list", args).await.map_err(|e| LoadError {
    source_id: "servers_loader".into(),
    message: e.message,
  })?;
  let mut data = ValueMap::new();
  data.insert("servers".to_owned(), servers);
  Ok(data)
});
```

So does an action, which maps the failure kind straight through.

```rust
actions.insert_fn("add_server", move |ctx, input| async move {
  let mut args = ValueMap::new();
  args.insert("name".to_owned(), Value::Str(name));
  args.insert("load".to_owned(), Value::F64(load));
  ctx
    .services
    .call("fleet", "add", args)
    .await
    .map_err(|e| ActionError::new(e.kind, e.message))
});
```

Naming the service and method through constants keeps the three-way agreement between the contract that declares a method, the transport that implements it and the caller honest.

```rust
pub mod fleet {
  pub const NAME: &str = "fleet";
  pub const LIST: &str = "list";
  pub const COUNT: &str = "count";
  pub const ADD: &str = "add";
}
```

## Why the Contract Is Neutral Data

The artifact speaks the value model, so neither language at either end owns it. A Rust type would make Rust the source of truth and force the TypeScript half to follow; a TypeScript interface would do the reverse. Neither can express a `u64` that a JSON number cannot hold. Keeping the artifact in its own vocabulary lets three front ends produce the same file: a TypeScript subset extracted by the compiler, a Rust derive export and a proto or OpenAPI import. None of those three is built. Today a contract is written in Rust with the builder methods, which is why the guide shows nothing else.

What exists and is worth using now is the artifact, its JSON serialisation and the checking over it. The type vocabulary was designed to receive a proto3 message or an OpenAPI `oneOf` without a redesign, since a brownfield shop imports contracts it already maintains: an enum lands as a union of unit variants, a `oneof` lands as a union of record payloads, a `map<string, T>` lands as `Type::map` and a `bytes` field lands as `Type::Bytes`. Both shapes are pinned by tests.

The checking is strict in both directions on purpose. Accepting an unknown field would let a backend's new field arrive silently and be dropped somewhere downstream; accepting a widened integer would let a truncation happen off-boundary, where no error names it. Failing at the boundary, with the path in the message, is the cheaper failure.

## Error Handling

Two error types meet here. `ContractError` comes from checking and never leaves the crate unwrapped; `ServiceError` is what a caller sees and it carries a `FailureKind` the UI can render.

`ContractError`, from `validate`, `check_call`, `check_return` and `check_value`:

```rust
use snapfire_fsr_service::ContractError;

match contract.check_call("users", "get", &args) {
  Err(ContractError::UnknownService(name)) => { /* no such service in the contract */ }
  Err(ContractError::UnknownMethod { service, method }) => { /* the service has no such method */ }
  Err(ContractError::UnknownType { path, name }) => { /* a Named reference with no definition */ }
  Err(ContractError::UnknownField { path, field }) => { /* an argument or field not declared */ }
  Err(ContractError::MissingField { path, field }) => { /* a non-optional field absent */ }
  Err(ContractError::UnknownVariant { path, tag, expected }) => { /* a tag outside the union */ }
  Err(ContractError::Mismatch { path, expected, found }) => { /* wrong shape or width at path */ }
  Ok(()) => {}
}
```

The registry converts them: `UnknownService` and `UnknownMethod` on the way out become `FailureKind::NotFound`, every other argument failure becomes `FailureKind::Invalid` and any failure checking a response becomes `FailureKind::Internal`, because a contract-breaking backend is not the caller's fault.

`ServiceError` is what `ServiceHandle::call` returns:

```rust
use snapfire_fsr_runtime::FailureKind;

match ctx.services.call("fleet", "add", args).await {
  Ok(value) => value,
  Err(e) => match e.kind {
    FailureKind::Unauthorized => return sign_in(),
    FailureKind::Conflict => return already_exists(&e.message),
    FailureKind::Unavailable | FailureKind::Timeout => return retry_later(),
    FailureKind::NotFound | FailureKind::Invalid | FailureKind::Internal => {
      return problem(e.kind.http_status(), &e.message)
    }
  },
}
```

`e.service` and `e.method` are always populated, `e.kind.http_status()` gives the status a UI or a JSON endpoint should return and the `Display` form reads `fleet.add failed (conflict): already exists`.
