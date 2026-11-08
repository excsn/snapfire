# API Reference: snapfire_fsr_service

The typed service boundary for SnapFire FSR: the contract artifact, its checking, the registry, interceptors, transports and the OpenAPI, protobuf and GraphQL importers.

## Contents

* [1. The Contract Artifact](#1-the-contract-artifact)
  * [`ScalarKind`](#scalarkind)
  * [`Type`](#type)
  * [`Field`](#field)
  * [`Variant`](#variant)
  * [`TypeDef`](#typedef)
  * [`Method`](#method)
  * [`Freshness`](#freshness)
  * [`Scope`](#scope)
  * [`Service`](#service)
  * [`Contract`](#contract)
* [2. The Registry](#2-the-registry)
  * [`Services`](#services)
  * [`ServicesBuilder`](#servicesbuilder)
* [3. Calls and Custody](#3-calls-and-custody)
  * [`Call`](#call)
  * [`next_request`](#next_request)
  * [`Credentials`](#credentials)
  * [`NoCredentials`](#nocredentials)
* [4. Interceptors](#4-interceptors)
  * [`Interceptor`](#interceptor)
  * [`Next`](#next)
  * [`IdentityInterceptor`](#identityinterceptor)
  * [`CredentialInterceptor`](#credentialinterceptor)
  * [`Refresher`](#refresher)
  * [`TraceInterceptor`](#traceinterceptor)
  * [`DataCache`](#datacache)
  * [`cache::canonical`](#cachecanonical)
* [5. Transports](#5-transports)
  * [`Transport`](#transport)
  * [`LocalTransport`](#localtransport)
  * [`MockTransport`](#mocktransport)
  * [`transport::unavailable`](#transportunavailable)
* [6. HTTP and OpenAPI](#6-http-and-openapi)
  * [`Route`](#route)
  * [`HttpTransport`](#httptransport)
  * [`kind_for_status`](#kind_for_status)
  * [`import`](#import)
  * [`Imported`](#imported)
* [7. Protobuf and gRPC](#7-protobuf-and-grpc)
  * [`import_proto`](#import_proto)
  * [`FSR_PROTO`](#fsr_proto)
  * [`ImportedProto`](#importedproto)
  * [`GrpcMethod`](#grpcmethod)
  * [`GrpcTransport`](#grpctransport)
  * [`grpc::kind_for_code`](#grpckind_for_code)
* [8. GraphQL](#8-graphql)
  * [`import_graphql`](#import_graphql)
  * [`ImportedGraphql`](#importedgraphql)
  * [`GraphqlOperation`](#graphqloperation)
  * [`GraphqlTransport`](#graphqltransport)
* [9. Builds Without a Feature](#9-builds-without-a-feature)
  * [Stand-in types](#stand-in-types)
* [10. TypeScript Declarations](#10-typescript-declarations)
  * [`Flavour`](#flavour)
  * [`typescript::declarations`](#typescriptdeclarations)
  * [`typescript::type_name`](#typescripttype_name)
* [11. Services Written in Rust](#11-services-written-in-rust)
  * [`ContractType`](#contracttype)
  * [`DeclaredService`](#declaredservice)
  * [`Caller`](#caller)
* [12. Types From Other Crates](#12-types-from-other-crates)
  * [Runtime types](#runtime-types)
  * [`TokenCell`](#tokencell)
* [13. Error Handling](#13-error-handling)
  * [`ContractError`](#contracterror)
  * [`DataCacheError`](#datacacheerror)
  * [`ImportError`](#importerror)

## 1. The Contract Artifact

Module `contract`, re-exported at the crate root. Every type here is `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize` and `Deserialize`. Enums serialise externally tagged in `snake_case`: `Type::Str` is `"str"` and `Type::Optional(Type::Str)` is `{"optional":"str"}`.

### `ScalarKind`

The element kind of a `Value::TypedArray`. `Copy`. `#[non_exhaustive]`.

* `I8`, `U8`, `I16`, `U16`, `I32`, `U32`, `I64`, `U64`, `F32`, `F64`
* `pub fn as_str(&self) -> &'static str`: the lowercase name, `"f64"` for `F64`.

### `Type`

The contract type vocabulary.

* `Null`, `Bool`, `I32`, `I64`, `I128`, `U32`, `U64`, `U128`, `F32`, `F64`, `Str`, `Bytes`
* `Array(ScalarKind)`
* `Optional(Box<Type>)`
* `List(Box<Type>)`
* `Map(Box<Type>)`: keys are always strings.
* `Named(String)`: a `TypeDef` in the same contract.
* `Dynamic`: any value, unchecked. A `#[service]` signature spells it `snapfire_fsr_core::Dynamic`.
* `pub fn optional(inner: Type) -> Self`
* `pub fn list(inner: Type) -> Self`
* `pub fn map(values: Type) -> Self`
* `pub fn named(name: impl Into<String>) -> Self`
* `pub fn describe(&self) -> String`: the type as error messages print it, such as `u64`, `list<Server>`, `map<str, i64>`, `array<f64>` and `optional<str>`. `Named` prints the bare name.

What each variant accepts in `check_value`:

| Variant | Accepted value |
| --- | --- |
| `Null` | `Value::Null` |
| `Bool` | `Value::Bool` |
| `I32` | `Value::Int` in `i32::MIN ..= i32::MAX` |
| `I64` | `Value::Int` in `i64::MIN ..= i64::MAX` |
| `I128` | any `Value::Int` |
| `U32` | `Value::Int` in `0 ..= u32::MAX` |
| `U64` | `Value::Int` in `0 ..= u64::MAX` |
| `U128` | `Value::UInt` or `Value::Int` in `0 ..= i128::MAX` |
| `F32` | `Value::F32` |
| `F64` | `Value::F64` |
| `Str` | `Value::Str` |
| `Bytes` | `Value::Bytes` |
| `Array(k)` | `Value::TypedArray` of element kind `k` |
| `Optional(t)` | `Value::Null`, `Value::Undefined` or whatever `t` accepts |
| `List(t)` | `Value::Seq`, every element checked against `t` |
| `Map(t)` | `Value::Map`, every value checked against `t` |
| `Named(n)` | `Value::Map` when `n` is a record, `Value::Variant` when `n` is a union |
| `Dynamic` | any value |

`check_value` coerces no number. An integer where `F64` is declared is a mismatch. So is a float where `I64` is declared. `conform` does the rewriting before a check.

### `Field`

A record member or a method parameter. Serialises its type under the key `type`. `#[non_exhaustive]`.

* `pub name: String`
* `pub ty: Type`
* `pub present: bool`: the key is in every value, so an `Optional` field is `null` rather than absent. Omitted from JSON when `false`.
* `pub fn new(name: impl Into<String>, ty: Type) -> Self`
* `pub fn present(self, present: bool) -> Self`

### `Variant`

One arm of a union. The payload serialises under the key `type` and is omitted when absent. `#[non_exhaustive]`.

* `pub tag: String`
* `pub payload: Option<Type>`
* `pub fn unit(tag: impl Into<String>) -> Self`: an arm with no payload.
* `pub fn with(tag: impl Into<String>, payload: Type) -> Self`

### `TypeDef`

What a `Type::Named` resolves to.

* `Record { fields: Vec<Field> }`
* `Union { variants: Vec<Variant> }`

### `Method`

One method signature. `#[non_exhaustive]`.

* `pub params: Vec<Field>`: empty when absent from JSON.
* `pub returns: Type`
* `pub cache: Option<Freshness>`: how long an answer may be reused. Omitted from JSON when `None`.
* `pub writes: Vec<String>`: the tags a successful call drops. Omitted from JSON when empty.
* `pub fn new(params: Vec<Field>, returns: Type) -> Self`
* `pub fn cached(self, freshness: Freshness) -> Self`
* `pub fn writes<I, S>(self, tags: I) -> Self` where `I: IntoIterator<Item = S>`, `S: Into<String>`

### `Freshness`

How long a method's answer holds. `Default`. `#[non_exhaustive]`: build one with `ttl` and the setters.

* `pub ttl: String`: a duration such as `30s`, `5m`, `2h`, `1d` or plain seconds.
* `pub tags: Vec<String>`: the tags a write drops it under. Omitted from JSON when empty.
* `pub scope: Scope`: default `Private`.
* `pub stale: Option<String>`: a window after `ttl` in which the last answer is served while a refresh runs. `Shared` scope only.
* `pub fn ttl(ttl: impl Into<String>) -> Self`
* `pub fn tags<I, S>(self, tags: I) -> Self`
* `pub fn shared(self) -> Self`
* `pub fn per_subject(self) -> Self`
* `pub fn scope(self, scope: Scope) -> Self`
* `pub fn stale(self, window: impl Into<String>) -> Self`

### `Scope`

Who a cached answer may be served to. `Copy`, `Default`. `#[non_exhaustive]`.

* `Private`, the default: an identified call neither reads nor writes the cache. Anonymous calls share one entry.
* `Shared`: every call reads one entry, identified or not.
* `Subject`: one entry per identity subject. Anonymous calls share one more.
* `pub fn as_str(&self) -> &'static str`

### `Service`

A named set of methods, in declaration order. `Default`. `#[non_exhaustive]`.

* `pub methods: IndexMap<String, Method>`: empty when absent from JSON.
* `pub fn new() -> Self`
* `pub fn method(self, name: impl Into<String>, method: Method) -> Self`: a repeated name replaces the earlier entry.

### `Contract`

The contract artifact. `Default`. `#[non_exhaustive]`.

* `pub types: IndexMap<String, TypeDef>`
* `pub services: IndexMap<String, Service>`
* Both maps are empty when absent from JSON and omitted from JSON when empty.

Building and storing:

* `pub fn new() -> Self`
* `pub fn define(self, name: impl Into<String>, def: TypeDef) -> Self`
* `pub fn record(self, name: impl Into<String>, fields: Vec<Field>) -> Self`
* `pub fn union(self, name: impl Into<String>, variants: Vec<Variant>) -> Self`
* `pub fn service(self, name: impl Into<String>, service: Service) -> Self`
* `pub fn method(&self, service: &str, method: &str) -> Option<&Method>`
* `pub fn to_json(&self) -> String`: pretty JSON. Panics only if serialisation fails.
* `pub fn from_json(source: &str) -> Result<Self, serde_json::Error>`

Combining:

* `pub fn merge(&mut self, other: Contract, file: &str) -> Result<(), ContractError>`: takes every type and service of `other`. A name this contract already defines is `DuplicateType` or `DuplicateService` naming `file`.
* `pub fn adopt(&mut self, other: Contract, file: &str) -> Result<(), ContractError>`: `merge`, except a type or service defined identically in both is accepted.
* `pub fn namespaced(&self, name: &str) -> Contract`: every type and service renamed `<name>:<its name>`, with every `named` reference following.
  * Every cache tag and `writes` tag is prefixed the same way, except a tag starting with `@`.
  * A key or tag that already carries the prefix is left alone.

Checking:

* `pub fn validate(&self) -> Result<(), ContractError>`: every `Named` in every field, variant payload, parameter and return type resolves. Every `cache.ttl` and `cache.stale` is a duration and `stale` is only on `Shared` scope.
  * An unresolved name is `UnknownType`, with `path` `Type.field`, `Service.method.param` or `Service.method()`.
  * A bad cache policy is `Cache` at `Service.method`.
* `pub fn check_value(&self, ty: &Type, value: &Value, path: &str) -> Result<(), ContractError>`: checks one value at `path`. Descending appends `[i]` for a list index and `.<key>` for a record field, a map key or a variant payload.
* `pub fn check_call(&self, service: &str, method: &str, args: &ValueMap) -> Result<(), ContractError>`: checks arguments against the parameters at path `service.method`. An `Optional` parameter may be absent. Any other absent parameter is `MissingField` and an undeclared argument is `UnknownField`.
* `pub fn check_return(&self, service: &str, method: &str, value: &Value) -> Result<(), ContractError>`: checks a value against the return type at path `service.method()`.
* A record is checked strictly. An absent non-optional field is `MissingField` and an undeclared field is `UnknownField`.
* A union value must carry a declared tag. A unit arm carries no payload and a payload arm carries one.

Conforming:

* `pub fn conform(&self, ty: &Type, value: &mut Value)`: rewrites `value` in place where JSON's single number type left a shape the contract names exactly. It descends optionals, lists, maps, record fields and variant payloads.
  * An `Int` or `UInt` declared `F64` or `F32` becomes that float.
  * An `F64` or `F32` declared as an integer type becomes `Int` when it is whole, finite and below 2^53 in magnitude.
  * A `Str` declared as a union becomes the unit variant of that tag when there is one.
  * Any other non-variant value declared as a union is wrapped in the one payload variant it fits, when exactly one fits.
  * Nothing else changes. `check_value` reports what remains.
* `pub fn conform_text(&self, ty: &Type, value: &mut Value)`: `conform` for a value whose scalars are all strings, such as a form body. It descends optionals and record fields, then runs `conform`.
  * A string that parses becomes the declared integer or float.
  * For `Bool`, `on`, `true`, `1` and `yes` become `true`. `off`, `false`, `0` and `no` become `false`.
  * An empty string declared `Optional` becomes `Null`.
  * A string that does not parse is left for `check_value`.
  * Must not be called on a value that arrived typed.
* `pub fn conform_return(&self, service: &str, method: &str, value: &mut Value)`: `conform` against the method's return type. An unknown method changes nothing.

## 2. The Registry

Module `registry`, re-exported at the crate root.

### `Services`

Named clients over transports, built once per process and shared as an `Arc`. Not `Clone`.

* `pub fn builder() -> ServicesBuilder`
* `pub fn contract(&self) -> &Contract`
* `pub fn bind(self: &Arc<Self>, identity: Option<Identity>, credentials: Arc<dyn Credentials>) -> ServiceHandle`: one request's identity and credential custody, fixed into a handle for application code. Every call from one handle carries the same `Call::request`.
* `pub fn bind_anonymous(self: &Arc<Self>) -> ServiceHandle`: `bind(None, Arc::new(NoCredentials))`.
* `pub fn data_cache(&self) -> Option<&DataCache>`: the cache, when the builder asked for one and the contract declares a cached or writing method.
* `pub fn invalidate_tags<I, S>(&self, tags: I)` where `I: IntoIterator<Item = S>`, `S: AsRef<str>`: drops every cached answer under the tags. Does nothing without a cache.

A call through a bound handle runs in this order:

* `check_call`. `UnknownService` and `UnknownMethod` fail as `FailureKind::NotFound` and every other error as `FailureKind::Invalid`.
* The transport is chosen: the service's own, else the default. With neither, the call fails as `FailureKind::Unavailable`.
* The interceptor chain, then the transport.
* With response checking on, `conform_return` then `check_return`. A failed check is `FailureKind::Internal`.

### `ServicesBuilder`

`Default`, which is what `Services::builder()` returns.

* `pub fn contract(self, contract: Contract) -> Self`: default is an empty contract.
* `pub fn intercept(self, interceptor: Arc<dyn Interceptor>) -> Self`: appends to the chain, which runs in registration order.
* `pub fn transport(self, service: impl Into<String>, transport: Arc<dyn Transport>) -> Self`
* `pub fn default_transport(self, transport: Arc<dyn Transport>) -> Self`: for every service without its own.
* `pub fn check_responses(self, check: bool) -> Self`: turns `check_return` on or off. Default `true`. Arguments are always checked.
* `pub fn data_cache(self, capacity: u64) -> Self`: answers every method whose contract declares `cache` from memory, at most `capacity` entries per method. The cache runs last in the chain.
* `pub fn build(self) -> Arc<Services>`: panics on a cache policy `try_build` refuses.
* `pub fn try_build(self) -> Result<Arc<Services>, DataCacheError>`
* With `data_cache` set and no method declaring `cache` or `writes`, no cache is installed.

## 3. Calls and Custody

Module `call`, re-exported at the crate root.

### `Call`

One outbound call as it travels the chain. `#[non_exhaustive]`: build one with `new`.

* `pub service: String`
* `pub method: String`
* `pub args: ValueMap`
* `pub identity: Option<Identity>`
* `pub metadata: ValueMap`
* `pub credentials: Arc<dyn Credentials>`
* `pub request: u64`: the request the call is made for.
* `pub fn new(service: impl Into<String>, method: impl Into<String>, args: ValueMap) -> Self`: no identity, empty metadata, `NoCredentials` and a fresh `next_request()`.
* `pub fn with_identity(self, identity: Option<Identity>) -> Self`
* `pub fn with_credentials(self, credentials: Arc<dyn Credentials>) -> Self`
* `pub fn with_request(self, request: u64) -> Self`
* `pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>)`: writes a `Value::Str` entry.
* `pub fn metadata_str(&self, key: &str) -> Option<&str>`: `None` for an absent or non-string entry.

### `next_request`

* `pub fn call::next_request() -> u64`: a request number no other binding in this process holds, counting from 1.

### `Credentials`

Read and write access to the request's backend credentials. `Send + Sync`.

* `fn get(&self, key: &str) -> Option<Value>`
* `fn set(&self, key: &str, value: Value)`
* `fn snapshot(&self) -> ValueMap`: every credential held. Default empty.
* Implemented for `snapfire_fsr_session::TokenCell`.

### `NoCredentials`

`pub struct NoCredentials`. `Default`. `get` returns `None`, `set` discards and `snapshot` is empty.

## 4. Interceptors

Module `interceptor`, re-exported at the crate root.

### `Interceptor`

One step of the outbound chain. `Send + Sync`.

* `fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>>`

### `Next`

The rest of the chain. Only the registry constructs one. `Clone`.

* `pub fn run(self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>>`: runs the next interceptor. After the last one it runs the transport.
* Not calling `run` stops the call. Nothing after it runs, the transport included.
* A clone may run the rest of the chain a second time.

### `IdentityInterceptor`

Writes the identity's `subject` as metadata. Writes nothing when the call has no identity. `Default`.

* `pub fn new() -> Self`: key `x-sf-subject`.
* `pub fn key(self, key: impl Into<String>) -> Self`

### `CredentialInterceptor`

Reads one credential from custody and writes it as metadata.

* `pub fn bearer(credential: impl Into<String>) -> Self`: header `authorization`, scheme `"Bearer "`.
* `pub fn header(self, header: impl Into<String>) -> Self`
* `pub fn scheme(self, scheme: impl Into<String>) -> Self`
* `pub fn only(self, services: impl IntoIterator<Item = impl Into<String>>) -> Self`: attaches on calls to these services only. Without it, every service.
* `pub fn refreshing(self, refresher: Arc<dyn Refresher>) -> Self`: renews the credentials through `refresher`.
* `pub const EXPIRY_MARGIN_SECS: u64 = 30`
* Writes `{scheme}{token}` under the header. Writes nothing when custody holds no such credential or holds one that is not `Value::Str`.
* With a refresher and the credential in custody, it renews before attaching when `expires_at` is at most `EXPIRY_MARGIN_SECS` from now. `expires_at` is Unix seconds as `Int`, `UInt` or `F64`.
* With a refresher, a call that fails as `FailureKind::Unauthorized` is renewed once and run again. It is not run again when the renewal changed nothing or a renewal already ran for this call.

### `Refresher`

Renews the credentials custody holds. `Send + Sync`.

* `fn refresh(&self, tokens: ValueMap) -> BoxFuture<'static, Option<ValueMap>>`: given `Credentials::snapshot`, the credentials that changed. Each is written back through `Credentials::set`. `None` or an empty map when nothing was renewed.

### `TraceInterceptor`

Writes a request id and traces the call. `Default`.

* `pub fn new() -> Self`: key `x-sf-request-id`.
* `pub fn key(self, key: impl Into<String>) -> Self`
* Writes `Call::request` as 16 lowercase hex digits, zero-padded, when the key is not already set.
* Emits a `tracing` debug event `call` on target `fsr::service` with `service`, `method` and `request_id`.
* Wraps the rest of the chain in an info span `call` on target `fsr::trace` with `service`, `method`, `cache` and `fibre.outcome`.
* `cache` starts as `none`. `DataCache` records `hit`, `stale` or `miss`.
* `fibre.outcome` is `ok` or the failure kind's `as_str`.

### `DataCache`

The interceptor `ServicesBuilder::data_cache` installs. `Clone` shares the caches. `Debug`, `Interceptor`.

* `pub fn from_contract(contract: &Contract, capacity: u64) -> Result<Self, DataCacheError>`: one `fibre_cache` per cached method, built on first use.
* `pub fn is_empty(&self) -> bool`: no method declares `cache` and none declares `writes`.
* `pub fn policies(&self) -> Vec<(String, Freshness)>`: `service.method` and its policy, sorted.
* `pub fn writers(&self) -> Vec<(String, Vec<String>)>`: `service.method` and the tags it drops, sorted.
* `pub fn hits(&self) -> u64`
* `pub fn misses(&self) -> u64`
* `pub fn invalidate_tags<I, S>(&self, tags: I)` where `I: IntoIterator<Item = S>`, `S: AsRef<str>`
* A key is the service, the method, the subject under `Subject` scope, the current generation of each of the policy's tags and the canonical arguments.
* A successful call to a method with `writes` moves each tag's generation on, so entries under it are no longer reached.
* A miss runs the caller's own call, credentials included. A `stale` refresh runs as a fresh `Call` with no identity or credentials.
* A failed call is never stored. A failed refresh keeps the last answer.

### `cache::canonical`

* `pub fn canonical(value: &Value, out: &mut String)`: a deterministic rendering of `value`, maps by sorted key.
* `pub struct cache::CallKey { pub service: String, pub method: String, pub args: ValueMap, .. }`: a cache key. `Clone`, `Debug`, `PartialEq`, `Eq`, `Hash` over the canonical form. `#[non_exhaustive]`.
* `pub struct cache::Loaded(pub Result<Value, ServiceError>)`: a cached entry.

## 5. Transports

Module `transport`, re-exported at the crate root.

### `Transport`

The last step of the chain. `Send + Sync`.

* `fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>>`

### `LocalTransport`

In-process implementations, keyed `service.method`. `Default`.

* `pub fn new() -> Self`
* `pub fn method<F, Fut>(self, path: impl Into<String>, f: F) -> Self` where `F: Fn(Call) -> Fut + Send + Sync + 'static`, `Fut: Future<Output = Result<Value, ServiceError>> + Send + 'static`
* A path with no closure fails as `FailureKind::NotFound`.

### `MockTransport`

Canned responses and a record of every call. `Default`.

* `pub fn new() -> Self`
* `pub fn returns(self, path: impl Into<String>, value: Value) -> Self`
* `pub fn fails(self, path: impl Into<String>, kind: FailureKind, message: impl Into<String>) -> Self`: `path` splits at the first `.` into the error's service and method.
* `pub fn calls(&self) -> Vec<(String, ValueMap, ValueMap)>`: one `(path, args, metadata)` per call, in order.
* `pub fn last_metadata(&self, key: &str) -> Option<String>`: one string metadata entry of the most recent call.
* A path with no canned response fails as `FailureKind::NotFound`. The call is recorded before the lookup.

### `transport::unavailable`

* `pub fn unavailable(call: &Call, message: impl Into<String>) -> ServiceError`: a `FailureKind::Unavailable` error naming the call's service and method.

## 6. HTTP and OpenAPI

Module `http` and module `openapi`, re-exported at the crate root. `HttpTransport` needs the `http` feature. The rest needs none.

### `Route`

How one contract method reaches HTTP. `Debug`, `Clone`. `#[non_exhaustive]`.

* `pub method: String`: the HTTP verb.
* `pub path: String`: appended to the base URL.
* `pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self`
* `pub fn get(path: impl Into<String>) -> Self`
* `pub fn post(path: impl Into<String>) -> Self`
* A `{name}` segment takes the argument of that name, which is then not sent in the body or the query.
* A `{name}` whose argument the call does not carry fails the call as `FailureKind::Invalid`.
* A string, integer, float or boolean renders as its text. Any other value renders through its `Debug` form.

### `HttpTransport`

A transport over `reqwest` with rustls. Feature `http`.

* `pub const HTTP_USER_AGENT: &str`: `snapfire_fsr_service/<crate version>`.
* `pub const HTTP_TIMEOUT: Duration`: 30 seconds.
* `pub fn new(base: impl Into<String>) -> Self`: a default `reqwest::Client`, user agent `HTTP_USER_AGENT`, timeout `HTTP_TIMEOUT`. Trailing slashes are trimmed from `base`.
* `pub fn with_timeout(base: impl Into<String>, timeout: Duration) -> Self`: as `new` with `timeout`.
* `pub fn with_client(base: impl Into<String>, client: reqwest::Client) -> Self`: no user agent or timeout of its own; `client`'s apply.
* `pub fn user_agent(self, agent: impl Into<String>) -> Self`: the `User-Agent` header every call sends.
* `pub fn timeout(self, timeout: Duration) -> Self`: how long a call waits for the whole answer. Past it the call fails as `FailureKind::Timeout`.
* `pub fn route(self, path: impl Into<String>, route: Route) -> Self`: the route for one `service.method`.
* `pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Self`: a header every call carries, set before the call's metadata. It is not metadata, so no interceptor sees it.
* `pub fn query(self, name: impl Into<String>, value: impl Into<String>) -> Self`: a query parameter every call carries, after any the arguments become.

Request shape:

* With no route: `POST {base}/{service}/{method}` with the arguments as a JSON body.
* With a route: its verb and `{base}{path}` after substitution.
* `GET` and `DELETE` send the remaining arguments as query parameters. Every other verb sends them as the JSON body, integers written as digits.
* Every metadata entry becomes a request header of the same name. A non-string entry fails the call as `FailureKind::Internal`.
* A verb that is not a valid HTTP method fails as `FailureKind::Internal`.

Response handling:

* 2xx with a body: parsed as JSON and converted with `snapfire_fsr_core::json::to_value`. A lone surrogate escape is replaced with U+FFFD. A parse failure is `FailureKind::Internal`.
* 2xx with an empty body: `Value::Null`.
* Non-2xx: kind from `kind_for_status`. The message is `{status}: {trimmed body}` for a 4xx other than 401, 403, 404 and 408. Otherwise it is `{status}`.
* A send failure: `FailureKind::Timeout` when reqwest reports a timeout, else `FailureKind::Unavailable`.

### `kind_for_status`

* `pub fn kind_for_status(status: u16) -> FailureKind`: `FailureKind::from_status(status)`.
* A status with its own kind maps to it, such as 401 `Unauthorized`, 403 `Forbidden`, 404 `NotFound`, 409 `Conflict`, 429 `TooManyRequests`, 502 `BadGateway`, 503 `Unavailable` and 504 `Timeout`.
* Any other 4xx is `Invalid`. Anything else, 2xx included, is `Internal`.

### `import`

* `pub fn import(document: &str, default_service: &str) -> Result<Imported, ImportError>`: an OpenAPI document in JSON as a contract and its routes.
* Operations under `get`, `put`, `post`, `delete` and `patch` are read. Each joins the service named by its first tag, else `default_service`.
* A method is named by `operationId`, else by the verb followed by each path segment in PascalCase.
* Each method gets a `Route` of its verb and path.
* Parameters are `path` or `query`. A `path` parameter or a `required` one is its type, any other is `Optional`. Another location is `Unsupported`.
* An `application/json` request body that is an object spreads into named parameters. Any other JSON body is one parameter `body`. A body in another media type is `Unsupported`.
* The return type is the first 2xx or `default` response's `application/json` schema. A 2xx with no JSON body returns `Null`. So does an operation with no success response.
* `x-sf-cache` on an operation is read as a `Freshness`. `x-sf-writes` is read as the tags it writes.
* Scalars: `string` is `Str` or `Bytes` for format `byte` or `binary`, `boolean` is `Bool`, `integer` is `I32`, `U32` or `U64` by format and `I64` otherwise, `number` is `F32` for format `float` and `F64` otherwise, `null` is `Null`.
* `nullable: true` or a two-type `type` array with `null` is `Optional`. `items` is `List`. An object with only `additionalProperties` is `Map`.
* `enum` of strings, `oneOf`, `anyOf`, `allOf` and objects become named types. `allOf` branches must be objects and merge with later branches winning.
* A reference outside `#/components/schemas/`, `additionalProperties: true` and a union of more than one non-null type are `Unsupported`.
* The contract is validated before it is returned.

### `Imported`

What `import` returns. `Debug`, `Clone`. `#[non_exhaustive]`.

* `pub contract: Contract`
* `pub routes: Vec<(String, Route)>`: keyed `service.method`.
* `pub base_url: Option<String>`: the first `servers` entry's `url`.

## 7. Protobuf and gRPC

Modules `proto` and `grpc`, re-exported at the crate root. Feature `grpc`.

### `import_proto`

* `pub fn import_proto(path: &Path, default_service: &str) -> Result<ImportedProto, ImportError>`: compiles the file with its directory, `snapfire/fsr.proto` and the Google well-known types as include paths.
* `pub fn import_proto_source(name: &str, source: &str, default_service: &str) -> Result<ImportedProto, ImportError>`: the same over source text. Only `snapfire/fsr.proto` and the well-known types can be imported.
* With one service in the file, it is named `default_service`. With several, each keeps its proto name.
* Method names become lowerCamel. Parameters are the request message's fields.
* `google.protobuf.Empty` as the request is no parameters. As the response it is a `Null` return.
* A file that does not compile is `Malformed`.
* A streaming method, a map keyed by anything but `string`, `Any` and `FieldMask` are `Unsupported`.
* `int32`, `sint32`, `sfixed32` are `I32`. `int64`, `sint64`, `sfixed64` are `I64`. `uint32`, `fixed32` are `U32`. `uint64`, `fixed64` are `U64`.
* `float`, `double`, `bool`, `string` and `bytes` are their scalars. An enum is `Str`.
* `repeated` is `List` and `map<string, T>` is `Map`.
* A message field, a proto3 `optional` field and a `oneof` member are `Optional`.
* `Outer.Inner` is the record `OuterInner`, with the package dropped.
* `Timestamp` and `Duration` are `Str`. The wrapper types are `Optional` of their scalar. `Struct`, `Value` and `ListValue` are `Dynamic`.
* A method with `(snapfire.fsr.cache)` gets `Method::cached` with the matching `Freshness`. `scope` defaults to `Private` and `stale` to `None`.
* A cache option without `ttl` is `Malformed` at `<service>.<Method> (snapfire.fsr.cache)`.
* A method with `(snapfire.fsr.writes)` gets `Method::writes` with the tags in order.
* An option used in a file that does not import `snapfire/fsr.proto` is the compiler's `Malformed` at the file.

### `FSR_PROTO`

* `pub const FSR_PROTO_NAME: &str = "snapfire/fsr.proto"`
* `pub const FSR_PROTO: &str`: its source.
  * `package snapfire.fsr` extends `google.protobuf.MethodOptions` with `Cache cache = 50100` and `repeated string writes = 50101`.
  * `Cache` holds `string ttl`, `repeated string tags`, `Scope scope` and `optional string stale`.
  * `Scope` is `PRIVATE`, `SHARED` or `SUBJECT`.
* `pub fn fsr_proto_include(dir: &Path) -> io::Result<PathBuf>`: writes `snapfire/fsr.proto` under `dir` and returns `dir`, for use as an include path.

### `ImportedProto`

What `import_proto` returns. `Debug`, `Clone`. `#[non_exhaustive]`.

* `pub contract: Contract`
* `pub pool: prost_reflect::DescriptorPool`
* `pub methods: Vec<(String, GrpcMethod)>`: keyed `service.method` as the contract names them.

### `GrpcMethod`

How one method reaches a gRPC server. `Debug`, `Clone`, `PartialEq`, `Eq`. `#[non_exhaustive]`.

* `pub path: String`: `/pkg.Service/Method`.
* `pub input: String`: the request message's full name in the pool.
* `pub output: String`: the response message's full name in the pool.

### `GrpcTransport`

A transport to a gRPC server.

* `pub fn new(base: &str, imported: &ImportedProto) -> Result<Self, String>`: `base` is `http://host:port` or `https://...`. The channel opens lazily on the first call.
* `pub fn with_channel(channel: tonic::transport::Channel, imported: &ImportedProto) -> Self`
* `pub fn metadata(self, name: impl Into<String>, value: impl Into<String>) -> Self`: metadata every call carries, before the call's own.
* The arguments become the request message field by field. Call metadata with string values becomes request metadata. A key or value that is not valid gRPC metadata is skipped.
* The response becomes a map with every field present. An unset message or presence-tracking field is `Null`, an enum is its value name and `Timestamp` and `Duration` are strings.
* An argument the message lacks is `Invalid` before the call. So is one outside its field's width.
* A method not in `methods` is `NotFound`.
* A channel that cannot be opened is `Unavailable`.
* A gRPC status maps through `kind_for_code`, with the status message.
* A response that does not decode is `Internal`.
* `pub fn grpc::encode_request(descriptor: &MessageDescriptor, args: &ValueMap) -> Result<Vec<u8>, String>`
* `pub fn grpc::decode_response(descriptor: &MessageDescriptor, bytes: &[u8]) -> Result<Value, String>`
* `pub fn grpc::from_message(message: &DynamicMessage) -> Result<Value, String>`

### `grpc::kind_for_code`

* `pub fn kind_for_code(code: tonic::Code) -> FailureKind`

| Code | Kind |
| --- | --- |
| `NotFound` | `NotFound` |
| `InvalidArgument`, `OutOfRange`, `FailedPrecondition` | `Invalid` |
| `Unauthenticated` | `Unauthorized` |
| `PermissionDenied` | `Forbidden` |
| `ResourceExhausted` | `TooManyRequests` |
| `Unimplemented` | `NotImplemented` |
| `AlreadyExists`, `Aborted` | `Conflict` |
| `DeadlineExceeded` | `Timeout` |
| `Unavailable` | `Unavailable` |
| anything else | `Internal` |

## 8. GraphQL

Module `graphql`, re-exported at the crate root. Feature `graphql`.

### `import_graphql`

* `pub fn import_graphql(source: &str, service: &str) -> Result<ImportedGraphql, ImportError>`: `source` is one document holding the schema's SDL and the named operations.
* Every operation is validated against the schema. A document that does not parse or validate is `Malformed` with the compiler's diagnostics.
* Each named query or mutation is a method of `service`, named lowerCamel.
* Its variables are the parameters. A nullable variable or one with a default is `Optional`.
* The return type is the record `<Operation>Query` or `<Operation>Mutation`, one field per response key with `present` set. An alias names its field.
* A nested selection is the record named by appending the PascalCase key: `PostBySlugQuery`, `PostBySlugQueryPost`, `PostBySlugQueryPostAuthor`.
* A fragment spread or inline fragment on the selection's own type merges its fields.
* `Int` is `I32`, `Float` is `F64`, `String` and `ID` are `Str`, `Boolean` is `Bool` and a custom scalar is `Str`.
* A nullable type is `Optional` and a list is `List`.
* An enum is a union of unit variants under the enum's name. An input object is a record under its name.
* A subscription, an anonymous operation and a selection on an interface or union that picks fields by the answer's type are `Unsupported`.

### `ImportedGraphql`

What `import_graphql` returns. `Debug`, `Clone`. `#[non_exhaustive]`.

* `pub contract: Contract`
* `pub service: String`
* `pub operations: IndexMap<String, GraphqlOperation>`: keyed by method name.

### `GraphqlOperation`

`Debug`, `Clone`. `#[non_exhaustive]`.

* `pub name: String`: the operation's name.
* `pub document: String`: the text sent for it, the operation and every fragment it spreads directly or through another fragment.

### `GraphqlTransport`

A transport that posts operations to one GraphQL endpoint.

* `pub fn new(endpoint: impl Into<String>, imported: &ImportedGraphql) -> Self`: a 30 second timeout.
* `pub fn with_client(endpoint: impl Into<String>, imported: &ImportedGraphql, client: reqwest::Client) -> Self`
* `pub fn header(self, name: impl Into<String>, value: impl Into<String>) -> Self`: a header every call carries.
* `pub fn query(self, name: impl Into<String>, value: impl Into<String>) -> Self`: a query parameter every call carries.
* A call is a `POST` to `endpoint` with the JSON body `{ "query", "variables", "operationName" }`. The arguments are the variables and a unit variant is sent as its tag.
* Call metadata becomes headers. A non-string entry fails the call as `Internal`.
* The answer is the body's `data`.
* A body carrying `errors` fails with the first one's `message`. Its kind comes from `extensions.code`: `NOT_FOUND` is `NotFound`, `UNAUTHENTICATED` is `Unauthorized`, `FORBIDDEN` is `Forbidden`, `BAD_USER_INPUT`, `BAD_REQUEST`, `GRAPHQL_VALIDATION_FAILED` and `GRAPHQL_PARSE_FAILED` are `Invalid` and anything else is `Internal`.
* Without `errors`, a non-2xx status maps through `kind_for_status` with the status as the message.
* A body without `data` is `Internal`. A method with no operation is `Internal`.
* A send failure is `Timeout` when reqwest reports a timeout, else `Unavailable`.

## 9. Builds Without a Feature

### Stand-in types

Without `grpc`, the crate root still exports `import_proto`, `ImportedProto` and `GrpcTransport`. Without `graphql`, it still exports `import_graphql`, `ImportedGraphql` and `GraphqlTransport`.

* `import_proto` and `import_graphql` return `ImportError::Feature` naming the feature.
* The stand-in `ImportedProto` and `ImportedGraphql` hold `pub contract: Contract` and can never be constructed.
* The stand-in transports keep `new`, `metadata`, `header` and `query` with the same signatures. They implement `Transport` and can never be constructed.

## 10. TypeScript Declarations

Module `typescript`.

### `Flavour`

Which side of the wire a declaration is for. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`.

* `Server`: every integer width is `bigint`.
* `Client`: every integer width is `bigint | number`.

### `typescript::declarations`

* `pub fn declarations(contract: &Contract) -> String`: the server-side declarations.
  * A generated-file header comment, then `type_declarations(contract, Flavour::Server)`, then `export interface Services` with one nested object per service and one method per contract method.
  * A method returns `Promise<T>`. One with no params takes no argument. One whose params are all optional takes `args?`.
* `pub fn type_declarations(contract: &Contract, flavour: Flavour) -> String`: every named type, without `Services`.
  * One `export interface` per record. One `export type` per union with arms `{ tag: "x" }` or `{ tag: "x"; payload: T }`.
  * An `Optional` field prints as `name?: T | null`. With `present` set it prints as `name: T | null`.
  * A name that is not an identifier is quoted.

### `typescript::type_name`

* `pub fn type_name(ty: &Type) -> String`: `type_name_for(ty, Flavour::Server)`.
* `pub fn type_name_for(ty: &Type, flavour: Flavour) -> String`

| Type | TypeScript |
| --- | --- |
| `Null` | `null` |
| `Bool` | `boolean` |
| any integer width | `bigint` for `Server`, `bigint \| number` for `Client` |
| `F32`, `F64` | `number` |
| `Str` | `string` |
| `Bytes` | `Uint8Array` |
| `Array(kind)` | the matching typed array, `BigInt64Array` and `BigUint64Array` for the 64-bit kinds |
| `Optional(T)` | `T \| null` |
| `List(T)` | `T[]`, parenthesised when `T` is optional or a `Client` integer |
| `Map(T)` | `Record<string, T>` |
| `Named(n)` | `n` |
| `Dynamic` | `unknown` |

## 11. Services Written in Rust

Module `rust`, re-exported at the crate root. `#[service]` and `#[derive(Record)]` from `snapfire_fsr_macros` expand against these. A `#[service]` `impl` gains a `Transport` answering its `pub` methods and a `DeclaredService` carrying its contract.

### `ContractType`

The contract type a Rust type crosses the boundary as.

* `fn contract_type() -> Type`
* `fn define(contract: &mut Contract)`: adds the records the type mentions. Default adds nothing.

| Rust type | Contract type |
| --- | --- |
| `()` | `Null` |
| `bool` | `Bool` |
| `i8`, `i16`, `i32` | `I32` |
| `u8`, `u16`, `u32` | `U32` |
| `i64`, `isize` | `I64` |
| `u64`, `usize` | `U64` |
| `i128` | `I128` |
| `u128` | `U128` |
| `f32` | `F32` |
| `f64` | `F64` |
| `String`, `str` | `Str` |
| `snapfire_fsr_core::Dynamic` | `Dynamic` |
| `&T` | `T`'s type |
| `Option<T>` | `Optional` |
| `Vec<T>` | `List` |
| `HashMap<String, T>`, `BTreeMap<String, T>`, `IndexMap<String, T>` | `Map` |
| `Result<T, ServiceError>` | `T`'s type |

`#[derive(Record)]` implements it for a struct as `Named`, with `define` inserting the record and each field's own records.

### `DeclaredService`

* `const NAME: &'static str`: the service name, the type's name in snake case.
* `fn contract() -> Contract`: the service under `NAME` with one method per `pub` fn and every record the signatures name.
  * Parameters are fields under their camelCase names. The return is the return type.
  * `#[cache]` and `#[writes]` set the method's policy.
  * A parameter typed `Caller` is not a field.

### `Caller`

Who called a `#[service]` method. `Debug`, `Clone`. `#[non_exhaustive]`.

* `pub service: String`
* `pub method: String`
* `pub identity: Option<Identity>`: `None` for an anonymous visitor.
* `pub metadata: ValueMap`: what the interceptors added.
* `pub fn of(call: &Call) -> Self`
* `pub fn require(&self) -> Result<&Identity, ServiceError>`: `FailureKind::Unauthorized` naming the method when the caller is anonymous.
* A method takes it as a parameter typed `Caller`, at most once. The contract does not list it.
* The call's credentials are not on it.

## 12. Types From Other Crates

### Runtime types

`ServiceHandle`, `ServiceCaller`, `Identity`, `FailureKind` and `ServiceError` are `snapfire_fsr_runtime`'s and are not re-exported here. The [runtime reference](../runtime/API_REFERENCE.md) documents the first two under Services and the rest under Error Handling.

* A default `ServiceHandle` is unbound. Every call through it fails as `FailureKind::Unavailable`.

### `TokenCell`

`snapfire_fsr_session::TokenCell`, documented in the [session reference](../session/API_REFERENCE.md). It implements `Credentials`.

## 13. Error Handling

### `ContractError`

Returned by every checking call and by `merge` and `adopt`. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`. `#[non_exhaustive]`.

| Variant | `Display` |
| --- | --- |
| `UnknownService(String)` | ``no service `{0}` in the contract`` |
| `Cache { path, reason }` | `{path}: {reason}` |
| `UnknownMethod { service, method }` | ``service `{service}` has no method `{method}` `` |
| `UnknownType { path, name }` | ``contract names type `{name}` at {path}, which it does not define`` |
| `UnknownField { path, field }` | ``{path}: unknown field `{field}` `` |
| `MissingField { path, field }` | ``{path}: missing field `{field}` `` |
| `UnknownVariant { path, tag, expected }` | ``{path}: unknown variant `{tag}`, expected one of {expected}`` |
| `Mismatch { path, expected, found }` | `{path}: expected {expected}, found {found}` |
| `DuplicateType { name, file }` | ``type `{name}` is already defined and `{file}` defines it again`` |
| `DuplicateService { name, file }` | ``service `{name}` is already defined and `{file}` defines it again`` |

Every field is a `String`.

* In `UnknownVariant`, `expected` is the union's tags joined with `, `.
* In `Mismatch`, `expected` is `Type::describe` or `no payload`.
* In `Mismatch`, `found` is the value's shape: `null`, `undefined`, `bool`, `int <n>`, `uint <n>`, `f32`, `f64`, `str`, `bytes`, `array<kind>`, `list`, `map`, ``variant `tag` ``, `ref` or `no payload`.

How the registry maps them onto `FailureKind`:

| Where | `ContractError` | `FailureKind` |
| --- | --- | --- |
| `check_call` | `UnknownService`, `UnknownMethod` | `NotFound` |
| `check_call` | every other variant | `Invalid` |
| `check_return` | any variant | `Internal` |

### `DataCacheError`

Returned by `DataCache::from_contract` and `ServicesBuilder::try_build`. `Debug`, `std::error::Error`. `#[non_exhaustive]`.

* `Ttl { method: String, ttl: String }`: ``{method}: cache.ttl `{ttl}` is not a duration``.
* `Stale { method: String, stale: String }`: ``{method}: cache.stale `{stale}` is not a duration``.
* `StaleScope { method: String }`: `stale` on a scope other than `Shared`.
* `Build { method: String, error: String }`: `fibre_cache` refused the policy.

### `ImportError`

Returned by `import`, `import_proto`, `import_proto_source` and `import_graphql`. `Debug`, `Clone`, `PartialEq`, `Eq`, `std::error::Error`. `#[non_exhaustive]`.

* `Malformed { at: String, what: String }`: `{at}: {what}`. A contract that fails `validate` after an OpenAPI import is `Malformed` at `#`.
* `Unsupported { at: String, what: String }`: `{at}: {what} is not supported`.
* `Feature { at: String, contract: &'static str, feature: &'static str }`: the importer is not in this build. ``{at}: a {contract} contract needs the `{feature}` feature of snapfire_fsr_service, which this build does not turn on``.
