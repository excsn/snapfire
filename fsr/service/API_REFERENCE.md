# API Reference: snapfire_fsr_service

The typed service boundary for Snapfire FSR: the contract artifact, its checking, the registry, interceptors and transports.

## Contents

* [1. The Contract Artifact](#1-the-contract-artifact)
  * [ScalarKind](#scalarkind)
  * [Type](#type)
  * [Field](#field)
  * [Variant](#variant)
  * [TypeDef](#typedef)
  * [Method](#method)
  * [Service](#service)
  * [Contract](#contract)
* [2. The Registry](#2-the-registry)
  * [Services](#services)
  * [ServicesBuilder](#servicesbuilder)
* [3. Calls and Custody](#3-calls-and-custody)
  * [Call](#call)
  * [Credentials](#credentials)
  * [NoCredentials](#nocredentials)
* [4. Interceptors](#4-interceptors)
  * [Interceptor](#interceptor)
  * [Next](#next)
  * [IdentityInterceptor](#identityinterceptor)
  * [CredentialInterceptor](#credentialinterceptor)
  * [TraceInterceptor](#traceinterceptor)
* [5. Transports](#5-transports)
  * [Transport](#transport)
  * [LocalTransport](#localtransport)
  * [MockTransport](#mocktransport)
  * [unavailable](#unavailable)
* [6. The HTTP Transport](#6-the-http-transport)
  * [Route](#route)
  * [HttpTransport](#httptransport)
  * [kind_for_status](#kind_for_status)
* [7. Protobuf and gRPC](#7-protobuf-and-grpc)
  * [import_proto](#import_proto)
  * [ImportedProto](#importedproto)
  * [GrpcTransport](#grpctransport)
  * [kind_for_code](#kind_for_code)
* [8. The Runtime Seam](#8-the-runtime-seam)
  * [ServiceHandle](#servicehandle)
  * [ServiceCaller](#servicecaller)
  * [Identity](#identity)
  * [FailureKind](#failurekind)
  * [ServiceError](#serviceerror)
* [9. TypeScript Declarations](#9-typescript-declarations)
  * [declarations](#declarations)
  * [type_name](#type_name)
* [10. Error Handling](#10-error-handling)
  * [ContractError](#contracterror)

## 1. The Contract Artifact

Module `contract`, re-exported at the crate root. Every type here derives `Debug`, `Clone`, `PartialEq`, `Eq`, `Serialize` and `Deserialize`. Enums serialise externally tagged with `snake_case` variant names, so `Type::Str` is the JSON string `"str"` and `Type::Optional(Type::Str)` is `{"optional":"str"}`.

### ScalarKind

The element kind of a `Value::TypedArray`. Also `Copy`.

* `I8` `U8` `I16` `U16` `I32` `U32` `I64` `U64` `F32` `F64`
* `fn as_str(&self) -> &'static str` returns the lowercase width name, `"f64"` for `F64`.

### Type

The contract type vocabulary. Every variant projects onto exactly one shape of the value model; the integer widths are what stop a `u64` being silently truncated at 2^53 on the way to TypeScript.

* `Null` `Bool` `I32` `I64` `I128` `U32` `U64` `U128` `F32` `F64` `Str` `Bytes`
* `Array(ScalarKind)`
* `Optional(Box<Type>)`
* `List(Box<Type>)`
* `Map(Box<Type>)`, keys are always strings
* `Named(String)`, a reference to a `TypeDef` in the same contract
* `fn optional(inner: Type) -> Self`
* `fn list(inner: Type) -> Self`
* `fn map(values: Type) -> Self`
* `fn named(name: impl Into<String>) -> Self`
* `fn describe(&self) -> String` renders the type as it appears in error messages: `"u64"`, `"list<Server>"`, `"map<str, i64>"`, `"array<f64>"` and `"optional<str>"`. A `Named` renders as the bare name.

What each variant accepts when checked against a `Value`:

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
| `Array(k)` | `Value::TypedArray` whose element width equals `k` |
| `Optional(t)` | `Value::Null` or whatever `t` accepts |
| `List(t)` | `Value::Seq`, every element checked against `t` |
| `Map(t)` | `Value::Map`, every entry value checked against `t` |
| `Named(n)` | `Value::Map` when `n` is a record, `Value::Variant` when `n` is a union |

There is no numeric coercion in either direction: an integer offered where `F64` is declared is a mismatch; a float offered where `I64` is declared is a mismatch.

### Field

One named member of a record or one parameter of a method. Serialises with the type under the key `type`.

* `pub name: String`
* `pub ty: Type`
* `fn new(name: impl Into<String>, ty: Type) -> Self`

### Variant

One arm of a union. A payloadless arm is how a proto3 or OpenAPI enum lands here. The payload serialises under the key `type` and is omitted entirely when absent.

* `pub tag: String`
* `pub payload: Option<Type>`
* `fn unit(tag: impl Into<String>) -> Self` builds an arm with no payload
* `fn with(tag: impl Into<String>, payload: Type) -> Self`

### TypeDef

What a `Type::Named` resolves to.

* `Record { fields: Vec<Field> }`
* `Union { variants: Vec<Variant> }`

### Method

One method signature.

* `pub params: Vec<Field>`, defaults to empty when absent from JSON
* `pub returns: Type`
* `fn new(params: Vec<Field>, returns: Type) -> Self`

### Service

A named set of methods. Also `Default`. The map is an `IndexMap`, so declaration order survives into the artifact.

* `pub methods: IndexMap<String, Method>`, defaults to empty when absent from JSON
* `fn new() -> Self`
* `fn method(self, name: impl Into<String>, method: Method) -> Self` inserts and returns self; a repeated name replaces the earlier entry

### Contract

The neutral artifact. Also `Default`. Both maps are `IndexMap`s and both default to empty when absent from JSON.

* `pub types: IndexMap<String, TypeDef>`
* `pub services: IndexMap<String, Service>`
* `fn new() -> Self`
* `fn define(self, name: impl Into<String>, def: TypeDef) -> Self`
* `fn record(self, name: impl Into<String>, fields: Vec<Field>) -> Self`
* `fn union(self, name: impl Into<String>, variants: Vec<Variant>) -> Self`
* `fn service(self, name: impl Into<String>, service: Service) -> Self`
* `fn to_json(&self) -> String` writes pretty JSON; panics only if serialisation itself fails
* `fn from_json(source: &str) -> Result<Self, serde_json::Error>`
* `fn merge(&mut self, other: Contract, file: &str) -> Result<(), ContractError>`: takes every type and service of `other`; a name this contract already defines is `DuplicateType` or `DuplicateService` naming `file`.
* `fn method(&self, service: &str, method: &str) -> Option<&Method>`
* `fn validate(&self) -> Result<(), ContractError>` checks that every `Named` in every record field, variant payload, parameter and return type resolves. Fails on the first unresolved reference with `ContractError::UnknownType`, whose `path` reads `Type.field`, `Service.method.param` or `Service.method()`.
* `fn check_value(&self, ty: &Type, value: &Value, path: &str) -> Result<(), ContractError>` checks one value at a caller-supplied path. Descending appends `[i]` for a list index, `.tag` for a variant payload and `.key` for a record field or a map key.
* `fn check_call(&self, service: &str, method: &str, args: &ValueMap) -> Result<(), ContractError>` checks arguments against a method's parameters at path `service.method`. A parameter typed `Optional` may be absent; any other absent parameter is `MissingField`. Any argument the method does not declare is `UnknownField`.
* `fn check_return(&self, service: &str, method: &str, value: &Value) -> Result<(), ContractError>` checks a value against a method's return type at path `service.method()`.

Record checking is strict in both directions: an absent non-optional field is `MissingField` and a field the record does not declare is `UnknownField`. Union checking requires the tag to be declared, a unit arm to carry no payload and a payload arm to carry one.

## 2. The Registry

Module `registry`, re-exported at the crate root.

### Services

Named clients over transports behind one seam, built once per process and shared as an `Arc`. Not `Clone` itself; clone the `Arc`.

* `fn builder() -> ServicesBuilder` returns a builder with response checking already on
* `fn contract(&self) -> &Contract`
* `fn bind(self: &Arc<Self>, identity: Option<Identity>, credentials: Arc<dyn Credentials>) -> ServiceHandle` fixes one request's identity and credential custody into a handle safe to hand to application code
* `fn bind_anonymous(self: &Arc<Self>) -> ServiceHandle` binds with no identity and `NoCredentials`

Every call made through a bound handle runs `check_call` before any transport is selected, then the interceptor chain, then the transport, then `check_return` unless response checking was disabled. A service with no named transport falls back to the default transport; with neither, the call fails with `FailureKind::Unavailable`.

### ServicesBuilder

Also `Default`, though `Services::builder()` is the entry point that sets the intended defaults.

* `fn contract(self, contract: Contract) -> Self` replaces the contract; the default is an empty one
* `fn intercept(self, interceptor: Arc<dyn Interceptor>) -> Self` appends to the chain, which runs in registration order
* `fn transport(self, service: impl Into<String>, transport: Arc<dyn Transport>) -> Self` binds one named service
* `fn default_transport(self, transport: Arc<dyn Transport>) -> Self` binds the fallback for every service without one
* `fn check_responses(self, check: bool) -> Self` governs `check_return` only; arguments are checked either way. `Services::builder()` starts it at `true`.
* `fn build(self) -> Arc<Services>` gives every transport its own chain over the same interceptor list

## 3. Calls and Custody

Module `call`, re-exported at the crate root.

### Call

One outbound call as it travels the chain. Interceptors read the identity and write metadata; `credentials` is reachable here and nowhere in application code.

* `pub service: String`
* `pub method: String`
* `pub args: ValueMap`
* `pub identity: Option<Identity>`
* `pub metadata: ValueMap`, empty when the chain starts
* `pub credentials: Arc<dyn Credentials>`
* `fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>)` writes a `Value::Str` entry
* `fn metadata_str(&self, key: &str) -> Option<&str>` reads one back, returning `None` for an entry that is not a string

### Credentials

Read and write access to the request's backend credentials. `Send + Sync`. `snapfire_fsr_session::TokenCell` implements it; a write through `set` marks that cell dirty so the session persists it.

* `fn get(&self, key: &str) -> Option<Value>`
* `fn set(&self, key: &str, value: Value)`

### NoCredentials

The empty implementation used by `bind_anonymous`. Also `Default`. `get` always returns `None` and `set` discards.

## 4. Interceptors

Module `interceptor`, re-exported at the crate root. The chain is an ordered list of functions, not a workflow engine.

### Interceptor

`Send + Sync`.

* `fn call(&self, call: Call, next: Next) -> BoxFuture<'static, Result<Value, ServiceError>>`

### Next

The rest of the chain. Constructed only by the registry; an interceptor receives one and either consumes it or drops it.

* `fn run(self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>>` invokes the next interceptor (the transport when the list is exhausted). Not calling it short-circuits; nothing downstream, transport included, runs.

### IdentityInterceptor

Propagates who the request is onto every outbound call. Also `Default`.

* `fn new() -> Self` with key `x-sf-subject`
* `fn key(self, key: impl Into<String>) -> Self`

Writes the identity's `subject` under that key. Writes nothing when the request has no identity.

### CredentialInterceptor

Reads one credential out of custody and attaches it.

* `fn bearer(credential: impl Into<String>) -> Self` with header `authorization` and scheme `"Bearer "`
* `fn header(self, header: impl Into<String>) -> Self`
* `fn scheme(self, scheme: impl Into<String>) -> Self`

Writes `{scheme}{token}` under the header key. Writes nothing when custody holds no such credential or holds one that is not a `Value::Str`.

### TraceInterceptor

A request id that survives the whole fanout. Also `Default`.

* `fn new() -> Self` with key `x-sf-request-id` and a counter starting at 1
* `fn key(self, key: impl Into<String>) -> Self`

Writes a zero-padded 16-digit lowercase hex counter under that key, but only when the key is not already set, so an id minted at the edge is left alone. It also emits a `tracing` debug event on target `fsr::service` with fields `service`, `method` and `request_id`.

## 5. Transports

Module `transport`, re-exported at the crate root.

### Transport

The last step of the chain. `Send + Sync`.

* `fn call(&self, call: Call) -> BoxFuture<'static, Result<Value, ServiceError>>`

### LocalTransport

Implementations that happen to be in-process, keyed `service.method`. Also `Default`.

* `fn new() -> Self`
* `fn method<F, Fut>(self, path: impl Into<String>, f: F) -> Self` where `F: Fn(Call) -> Fut + Send + Sync + 'static` and `Fut: Future<Output = Result<Value, ServiceError>> + Send + 'static`

A path with no registered closure fails with `FailureKind::NotFound`.

### MockTransport

Canned responses plus a recording of what the chain produced. Also `Default`.

* `fn new() -> Self`
* `fn returns(self, path: impl Into<String>, value: Value) -> Self`
* `fn fails(self, path: impl Into<String>, kind: FailureKind, message: impl Into<String>) -> Self` splits `path` at the first `.` for the error's service and method
* `fn calls(&self) -> Vec<(String, ValueMap, ValueMap)>` returns one `(path, args, metadata)` triple per call, in order
* `fn last_metadata(&self, key: &str) -> Option<String>` reads one string metadata entry from the most recent call

A path with no canned response fails with `FailureKind::NotFound`. Recording happens before the response is looked up, so a call that finds no response still appears in `calls`.

### unavailable

* `fn unavailable(call: &Call, message: impl Into<String>) -> ServiceError` builds a `FailureKind::Unavailable` error naming the call's service and method.

## 6. The HTTP Transport

Module `http`, re-exported at the crate root. Built on `reqwest` with rustls.

### Route

How one contract method reaches the wire.

* `pub method: String`, the HTTP verb
* `pub path: String`, appended to the base URL
* `fn new(method: impl Into<String>, path: impl Into<String>) -> Self`
* `fn get(path: impl Into<String>) -> Self`
* `fn post(path: impl Into<String>) -> Self`

A `{name}` segment in the path takes the argument of that name and that argument is then not sent in the body or the query. A `{name}` naming an argument that is not present renders as an empty segment and consumes nothing. String, integer, float and boolean arguments render with their natural text; any other value renders through its `Debug` form.

### HttpTransport

The base URL has any trailing slashes trimmed.

* `fn new(base: impl Into<String>) -> Self` uses a default `reqwest::Client`
* `fn with_timeout(base: impl Into<String>, timeout: Duration) -> Self`
* `fn with_client(base: impl Into<String>, client: reqwest::Client) -> Self`
* `fn route(self, path: impl Into<String>, route: Route) -> Self` overrides one `service.method`

Request shape:

* With no route registered: `POST {base}/{service}/{method}`, arguments as a JSON body.
* With a route: that verb and `{base}{path}` after template substitution.
* For `GET` and `DELETE` the remaining arguments become query parameters; for every other verb they become the JSON body.
* Every metadata entry whose value is a `Value::Str` becomes a request header of the same name. Entries of any other shape are skipped.

Response handling:

* 2xx with a body: parsed as JSON, then converted with `snapfire_fsr_payload::json_to_value`. A parse or conversion failure is `FailureKind::Internal`.
* 2xx with an empty body: `Value::Null`.
* Non-2xx: an error whose kind comes from `kind_for_status` and whose message is `{status}: {trimmed body}`.
* A send failure: `FailureKind::Timeout` when reqwest reports a timeout, `FailureKind::Unavailable` otherwise.
* A verb string that is not a valid HTTP method: `FailureKind::Internal`.

### kind_for_status

* `fn kind_for_status(status: u16) -> FailureKind`

| Status | Kind |
| --- | --- |
| 401, 403 | `Unauthorized` |
| 404 | `NotFound` |
| 409 | `Conflict` |
| 408, 504 | `Timeout` |
| 502, 503 | `Unavailable` |
| any remaining 400 to 499 | `Invalid` |
| anything else, 2xx included | `Internal` |

The named statuses are matched before the 4xx range.

## 7. Protobuf and gRPC

Modules `proto` and `grpc`, behind the `grpc` feature, re-exported at the crate root. protox compiles the file, prost-reflect holds the descriptors and encodes the messages, tonic carries them.

### import_proto

* `fn import_proto(path: &Path, default_service: &str) -> Result<ImportedProto, ImportError>`: compiles the file with its directory and the Google well-known types as include paths.
* `fn import_proto_source(name: &str, source: &str, default_service: &str) -> Result<ImportedProto, ImportError>`: the same over source text; only the well-known types can be imported.
* One service in the file is named `default_service`; several keep their proto names. Method names go lowerCamel. Parameters are the request message's fields; `google.protobuf.Empty` is no parameters or a `Null` return. A streaming method, a map keyed by anything but `string` and `Any`, `Struct`, `Value`, `ListValue` or `FieldMask` are `ImportError::Unsupported`; a file that does not compile is `Malformed`.
* Types: `int32`, `sint32`, `sfixed32` to `I32`; `int64`, `sint64`, `sfixed64` to `I64`; `uint32`, `fixed32` to `U32`; `uint64`, `fixed64` to `U64`; `float`, `double`, `bool`, `string`, `bytes` to their scalars; `repeated` to `List`; `map<string, T>` to `Map`; an enum to `Str`; a message field, a proto3 `optional` or a `oneof` member to `Optional`; `Outer.Inner` to the record `OuterInner` with the package dropped; `Timestamp` and `Duration` to `Str`; the wrapper types to `Optional` of their scalar.

### ImportedProto

* `pub struct ImportedProto { pub contract: Contract, pub pool: prost_reflect::DescriptorPool, pub methods: Vec<(String, GrpcMethod)> }`, the key being `service.method` as the contract names them.
* `pub struct GrpcMethod { pub path: String, pub input: String, pub output: String }`: the request path `/pkg.Service/Method` and the full names of the request and response messages in `pool`.

### GrpcTransport

* `fn new(base: &str, imported: &ImportedProto) -> Result<Self, String>`: `base` is `http://host:port` or `https://`; the channel opens lazily on the first call, so construction needs no runtime.
* `fn with_channel(channel: tonic::transport::Channel, imported: &ImportedProto) -> Self`
* `impl Transport`: the arguments become the request message field by field (`grpc::encode_request`), the response message becomes a value with every field present, unset messages and presence-tracking fields as `Null`, enums as their value names, `Timestamp` and `Duration` as strings (`grpc::decode_response`, `grpc::from_message`). Metadata entries with string values become request metadata.
* Errors: an argument the message lacks or one outside its width is `Invalid` before the call; a method not in `methods` is `NotFound`; a channel that cannot be opened is `Unavailable`; a gRPC status maps through `kind_for_code` with the status message; a response that does not decode is `Internal`.

### kind_for_code

* `fn kind_for_code(code: tonic::Code) -> FailureKind`: `NotFound` to `NotFound`; `InvalidArgument`, `OutOfRange`, `FailedPrecondition` to `Invalid`; `Unauthenticated`, `PermissionDenied` to `Unauthorized`; `AlreadyExists`, `Aborted` to `Conflict`; `DeadlineExceeded` to `Timeout`; `Unavailable` to `Unavailable`; anything else to `Internal`.

## 8. The Runtime Seam

These types come from `snapfire_fsr_runtime` and are not re-exported here, but they appear in this crate's signatures.

### ServiceHandle

`ctx.services`. What `Services::bind` returns and the only service surface application code holds. `Clone` and `Default`.

* `fn new(caller: Arc<dyn ServiceCaller>) -> Self`
* `fn is_bound(&self) -> bool`
* `fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`

A default-constructed handle is unbound and every call through it fails with `FailureKind::Unavailable` rather than pretending to succeed.

### ServiceCaller

What a `ServiceHandle` wraps. `Send + Sync`. This crate's registry implements it privately, which is why `bind` is the only way to obtain one.

* `fn call(&self, service: &str, method: &str, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>`

### Identity

Who the request is.

* `pub subject: String`
* `pub claims: ValueMap`

### FailureKind

The failure taxonomy every error at this boundary carries.

* `Unauthorized` `NotFound` `Invalid` `Conflict` `Timeout` `Unavailable` `Internal`
* `fn as_str(&self) -> &'static str` gives `"unauthorized"`, `"not_found"`, `"invalid"`, `"conflict"`, `"timeout"`, `"unavailable"`, `"internal"`
* `fn http_status(&self) -> u16` gives 401, 404, 400, 409, 504, 503, 500 in that order

### ServiceError

* `pub kind: FailureKind`
* `pub service: String`
* `pub method: String`
* `pub message: String`
* `fn new(kind: FailureKind, service: impl Into<String>, method: impl Into<String>, message: impl Into<String>) -> Self`

`Display` reads `{service}.{method} failed ({kind}): {message}`.

## 9. TypeScript Declarations

### declarations

* `pub fn typescript::declarations(contract: &Contract) -> String`
* One `export interface` per record, one `export type` per union with arms `{ tag: "x" }` or `{ tag: "x"; payload: T }`, then `export interface Services` with one nested object per service and one method per contract method. A method with no params takes no argument; one whose params are all optional takes `args?`. Optional fields print as `name?: T | null`. Names that are not identifiers are quoted.

### type_name

* `pub fn typescript::type_name(ty: &Type) -> String`
* Every integer width is `bigint`; `F32` and `F64` are `number`; `Str` is `string`; `Bytes` is `Uint8Array`; `Array(kind)` is the matching typed array; `Optional(T)` is `T | null`; `List(T)` is `T[]`, parenthesised when `T` is optional; `Map(T)` is `Record<string, T>`; `Named(n)` is `n`.

## 10. Error Handling

### ContractError

What every checking call returns. `Debug`, `Clone`, `PartialEq`, `Eq` and `std::error::Error`.

The variants and their `Display` forms:

```
UnknownService(String)                     no service `{0}` in the contract
UnknownMethod { service, method }          service `{service}` has no method `{method}`
UnknownType { path, name }                 contract names type `{name}` at {path}, which it does not define
UnknownField { path, field }               {path}: unknown field `{field}`
MissingField { path, field }               {path}: missing field `{field}`
UnknownVariant { path, tag, expected }     {path}: unknown variant `{tag}`, expected one of {expected}
Mismatch { path, expected, found }         {path}: expected {expected}, found {found}
DuplicateType { name, file }               type `{name}` is already defined and `{file}` defines it again
DuplicateService { name, file }            service `{name}` is already defined and `{file}` defines it again
```

In `UnknownVariant`, `expected` is the union's declared tags joined with a comma and a space. In `Mismatch`, `expected` is `Type::describe` and `found` names the value's own shape, with integers carrying their value:

```
int 7        uint 340282366920938463463374607431768211455        f64
str          bytes        list        map        null        bool
array<f32>   variant `trial`          ref
```

The registry maps these onto `FailureKind` before application code sees them:

| Where | ContractError | FailureKind |
| --- | --- | --- |
| `check_call` | `UnknownService`, `UnknownMethod` | `NotFound` |
| `check_call` | every other variant | `Invalid` |
| `check_return` | any variant | `Internal` |

A contract-breaking response is `Internal` because it is the backend's defect, not the caller's.
