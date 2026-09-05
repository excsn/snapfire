# snapfire_fsr_service

MPL-2.0. Version 0.1.0, pre-release, not published to crates.io.

The typed service boundary for SnapFire FSR. Application code asks for a capability by naming a service, a method and its arguments; it never names a host, a header or a token. A *contract* declares what exists, a *registry* binds that contract to one request, *interceptors* attach identity and credentials on the way out and a *transport* carries the call to wherever the implementation lives. Start with the [usage guide](README.USAGE.md); look calls up in the [API reference](API_REFERENCE.md).

The contract is a neutral data artifact speaking the FSR value model. Neither Rust nor TypeScript is its source of truth. Two of its three front ends are built: `openapi::import` reads an OpenAPI document and, behind the `grpc` feature, `import_proto` reads a `.proto` file, each into the same artifact; the TypeScript schema reader lives in `snapfire_fsr_lower` and the Rust derive export is not built. A contract is also constructed in Rust with the builder methods on `Contract`, `Service` and `Method`, then serialised with `Contract::to_json`. Contracts from several sources merge with `Contract::merge`, which refuses a name two of them define.

## Install

```toml
[dependencies]
snapfire_fsr_service = { path = "../service" }
```

One cargo feature, `grpc`, adds `import_proto` and `GrpcTransport` with protox, prost-reflect and tonic behind them; everything else is always compiled. The crate depends on `snapfire_fsr_core` for the value model, `snapfire_fsr_payload` for the JSON pair, `snapfire_fsr_runtime` for the `ServiceHandle` seam it fills and `snapfire_fsr_session` for `TokenCell`.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Declare the types a backend speaks | `Contract::record`, `Contract::union`, `Type`, `Field`, `Variant` |
| Declare the methods a backend exposes | `Service::new().method(..)`, `Method::new` |
| Store or load the artifact | `Contract::to_json`, `Contract::from_json` |
| Prove every named type resolves | `Contract::validate` |
| Check one call, one response or one value | `Contract::check_call`, `Contract::check_return`, `Contract::check_value` |
| Assemble the layer once per process | `Services::builder()` |
| Give application code a caller for one request | `Services::bind`, `Services::bind_anonymous` |
| Attach the subject, a bearer token or a request id | `IdentityInterceptor`, `CredentialInterceptor`, `TraceInterceptor` |
| Stop a call before it leaves | an `Interceptor` that does not call `Next::run` |
| Reuse a backend's answer for a while | `Method::cached(Freshness)`, `Method::writes`, `ServicesBuilder::data_cache`, `Services::invalidate_tags` |
| Implement a method in the same process | `LocalTransport` |
| Reach a backend over HTTP | `HttpTransport`, `Route` |
| Import an OpenAPI document | `import` |
| Import a `.proto` and reach its server over gRPC | `import_proto`, `GrpcTransport`, feature `grpc` |
| Merge contracts from several sources | `Contract::merge` |
| Run the whole application with no backend | `MockTransport` |
| Hold a token application code cannot read | `Credentials`, `TokenCell` |
| Print the contract as server-side TypeScript declarations | `typescript::declarations` |

## Status

Pre-release and unpublished, with no stability guarantee on any name here. The layer is exercised end to end by the `advanced_tera_app` example under `fsr/examples/`, which declares a `fleet` contract, serves it from a `LocalTransport` and calls it from both a loader and an action. `shopping_react_ts` imports an OpenAPI document and a `.proto` and reaches an HTTP service and a gRPC service through the same registry. The crate carries 29 integration tests, 3 of them over the proto importer and the message conversions behind `grpc`, plus: 10 over the contract and its checking; 11 over the registry with its interceptor chain, its local transport and its mock transport; 5 driving `HttpTransport` against a real socket.
