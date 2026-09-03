# 202. Services and transports

The question this chapter answers: what happens between a body saying `services.shopping.listProducts({})` and bytes leaving the process, who checks what and why application code can never see a token?

**For:** platform developers.

## One registry, many transports

Every call a body makes goes through one registry, `Services`. It holds the contract and, per service, a chain: an ordered list of interceptors ending in a transport. The registry checks the call's arguments against the contract before the chain sees them and the response after it comes back, then hands the value to the body. That is the whole request path for data; it is the same path whether the body is TypeScript the interpreter runs or Rust that took the name over.

The host builds the registry from configuration. Each `[clients.<name>]` names a document and a base URL; the document's extension picks the transport: an OpenAPI document becomes `HttpTransport`, with one route per operation mapping the method's arguments into the path, the query and the body the document describes; a `.proto` becomes `GrpcTransport`, which compiles the file with no protoc, keeps the descriptors and encodes each unary call from the value model directly, so a 64-bit integer keeps its width and bytes stay bytes. The report lists each service with its kind and base URL.

Two more transports exist for the cases where the network is not there. `LocalTransport` answers a service in process, method by method, which is how a Rust function can stand behind a name a body calls. `MockTransport` answers with canned values and records every call, which is what the storefront's route tests run against and what makes an application testable with no backend at all. The registry does not know the difference; a transport is a block.

## The chain

An interceptor sees the outbound call and decides to continue it, alter it or stop it. Three ship:

- **`TraceInterceptor`** mints one request id per request and attaches it to every call that fans out from it, so a log line in a backend can be found from the page that caused it.
- **`IdentityInterceptor`** propagates who the request is onto every call, so a backend that cares about the caller learns it from the platform rather than from a header a body remembered to set.
- **`CredentialInterceptor`** reads one credential out of custody and attaches it. This is the step that makes "application code never sees a token" structural rather than a convention.

A chain is tower-style: a list of functions, not a workflow engine. A cache or a circuit breaker sits here by not calling the rest of the chain.

## Custody

A session carries two things: the cell a body reads and writes plus the tokens the platform holds for it. The cell flows into the request context. The tokens never do. A body has no field through which to reach a bearer token, a refresh token or an API key; no service call it can make takes one as an argument. The credential interceptor reads from custody at the moment of the call and writes a refreshed token back the same way; neither end of that is application code.

**The boundary is not that tokens are hidden. It is that there is no path.** That is the property an auditor wants and it is the reason the registry, the interceptors and the session are one design rather than three.

## Failure has seven names

A call fails with a kind from a fixed list: `unauthorized`, `not_found`, `invalid`, `conflict`, `timeout`, `unavailable`, `internal`. The HTTP transport maps status codes onto them, the gRPC transport maps status codes onto them, a guard in a body names one directly. A loader that fails renders the route's error component with the message; an action that fails returns the kind to the browser, which is what the toast shows. There is one vocabulary for "this did not work" from the backend to the pixel; the contract check's own failures use it too, as `invalid` with the field's path.

## The lab

Point the shopping client somewhere empty: in `config/app.toml` change `clients.shopping.base_url` to a port nothing listens on and start the host. The report still lists the service, since binding a transport does not open a socket. Load the catalog: the error page renders; its message names `shopping.listProducts` as `unavailable`. The body never saw a connection error, because a body never sees a transport.

Put it back, then read [`storefront.rs`](../../examples/shopping_react_ts/tests/storefront.rs) and find `services_over`. Every test in the file runs the whole host over a mock transport with no backend running; the assertions on the transport's recorded calls are how a test says what the page asked for.
