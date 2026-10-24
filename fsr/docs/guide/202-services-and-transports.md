# 202. Services and transports

The question this chapter answers: what happens between a body saying `services.shopping.listProducts({})` and bytes leaving the process, who checks what and why application code can never see a token?

**For:** platform developers.

## The registry

Every call a body makes goes through one registry, `Services`. It holds the contract and, per service, a chain: an ordered list of interceptors ending in a transport. The registry checks the call's arguments against the contract before the chain sees them and the response after it comes back, then hands the value to the body. Every data request takes this path, whether the body is TypeScript the interpreter runs or Rust that took the name over.

The host builds the registry from configuration. Each `[clients.<name>]` names a document and a base URL; the document's extension picks the transport: an OpenAPI document becomes `HttpTransport`, with one route per operation mapping the method's arguments into the path, the query and the body the document describes; a `.proto` becomes `GrpcTransport`, which compiles the file with no protoc, keeps the descriptors and encodes each unary call from the value model directly, so a 64-bit integer keeps its width and bytes stay bytes; a `.graphql` becomes `GraphqlTransport`, which posts the method's operation with the arguments as its variables to the base URL and answers the body's `data`. The report lists each service with its kind and base URL.

Two more transports exist for the cases where the network is not there. `LocalTransport` answers a service in process, method by method, which is how a Rust function can stand behind a name a body calls; an `impl` block marked `#[service]` is the same thing written once, the attribute deriving both the transport and the contract from the signatures. `MockTransport` answers with canned values and records every call, which the storefront's route tests run against, so an application can be tested with no backend at all. The registry treats all four transports the same way.

## The chain

An interceptor sees the outbound call and decides to continue it, alter it or stop it. fsr ships three interceptors:

- **`TraceInterceptor`** mints one request id per request and attaches it to every call that fans out from it, so a log line in a backend can be found from the page that caused it.
- **`IdentityInterceptor`** propagates who the request is onto every call, so a backend that cares about the caller learns it from the platform rather than from a header a body remembered to set.
- **`CredentialInterceptor`** reads one credential out of custody and attaches it. This step keeps tokens out of application code by construction, so nothing depends on a convention.

A chain is tower-style: a list of functions. A cache or a circuit breaker sits here by not calling the rest of the chain.

## Custody

A session carries two things: the cell a body reads and writes plus the tokens the platform holds for it. The cell goes into the request context and the tokens do not. A body has no field through which to reach a bearer token, a refresh token or an API key; no service call it can make takes one as an argument. The credential interceptor reads from custody at the moment of the call and writes a refreshed token back the same way; neither end of that is application code.

Application code has no way to reach a token. The registry, the interceptors and the session are designed together to keep it that way.

## An application credential

Some backends authenticate the application rather than the visitor: a CMS token, a payments key, an API key a SaaS backend issues once. That value is the deployment's, not any session's, so it lives in configuration. Declare it under `[secrets]` and name it on the client that sends it:

```toml
[secrets.cms]
description = "The CMS's read-only API token"

[clients.cms]
base_url = "https://graphql.example-cms.com"
credential = { secret = "cms" }
```

The client sends it as `Authorization: Bearer <value>` on every call. `credential = { secret = "maps", header = "x-api-key" }` sends it as that header instead, `credential = { secret = "maps", query = "key" }` as that query parameter. A gRPC client sends a header as call metadata and has no query string.

The value comes from the deployment's environment (`C5_SECRETS__CMS__VALUE`) or is written encrypted in an overlay as a `.c5encval` value. Outside development a value written as plain text in a file stops the host at boot, since the file is in the repository. `development.toml` may hold one for a backend on your own machine. A client on the `mock` transport sends nothing, so tests and a project without the backend run with no secret at all.

The value sits on the client's transport, not on any call, so no interceptor, trace or body sees it. It never prints. A secret a client sends with no value stops the host at boot, naming the variable. `fsr doctor` checks the declarations (chapter 304): it lists each value as one the deployment must set, reports a secret no client sends and one named like a `[public]` key. A client may carry both: `bearer = true` sends the visitor's token while `credential` with a `header` other than `authorization` sends the application's beside it.

## Failure kinds

A call fails with a kind, one per HTTP error status: `unauthorized`, `forbidden`, `not_found`, `conflict`, `too_many_requests`, `bad_gateway`, `invalid` for 400 and the rest. The HTTP transport reads a backend's status as its own kind, so a 403 stays `forbidden` and a 429 stays `too_many_requests`; the gRPC transport maps `PermissionDenied` to `forbidden` and `ResourceExhausted` to `too_many_requests`; the GraphQL transport reads the first error's `extensions.code`, so `NOT_FOUND` is `not_found` and an error without a code is `internal`; a guard in a body names one directly. A loader that fails renders the route's error component with the message; an action that fails returns the kind to the browser, which is what the toast shows. The same kinds describe every failure from the backend to the rendered page; the contract check's own failures use them too, as `invalid` with the field's path.

## The lab

Point the shopping client somewhere empty: in `config/app.toml` change `clients.shopping.base_url` to a port nothing listens on and start the host. The report still lists the service, since binding a transport does not open a socket. Load the catalog: the error page renders with `data source $root: shopping.listProducts failed (unavailable): the backend did not answer`, which the log's `segment loader failed` line carries too. Under `fsr dev` the shopping binary starts its own backend on 8081, so point the client elsewhere with `C5_CLIENTS__SHOPPING__BASE_URL=http://127.0.0.1:9`. The body never saw a connection error, because transport errors become failure kinds before a body sees them.

Put it back, then read [`storefront.rs`](../../examples/shopping_react_ts/tests/storefront.rs) and find `services_over`. Every test in the file runs the whole host over a mock transport with no backend running; the assertions on the transport's recorded calls are how a test says what the page asked for.
