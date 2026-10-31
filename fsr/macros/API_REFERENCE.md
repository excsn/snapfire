# API Reference: snapfire_fsr_macros

The attributes and the derive that give a lowered TypeScript body Rust to call.

## Contents

* [1. Native Modules](#1-native-modules)
  * [native](#native)
* [2. Services](#2-services)
  * [service](#service)
  * [cache](#cache)
  * [writes](#writes)
* [3. Records](#3-records)
  * [Record](#record)
* [4. Error Handling](#4-error-handling)
  * [Compile Errors](#compile-errors)
  * [Call Failures](#call-failures)

## 1. Native Modules

### native

* `#[native]` on `impl Type { ... }`: emits the block unchanged and implements `snapfire_fsr_runtime::Native` for `Type`.
* `Native::call(&self, method, args: ValueMap) -> BoxFuture<'static, Result<Value, ServiceError>>` and `Native::call_sync(...) -> Option<Result<Value, ServiceError>>` dispatch on the method's name camelCased, `fetch_count` as `fetchCount`.
* Each `pub` method is callable; any other is not. Every method takes `&self`.
* Each argument is read from `args` under its parameter's name camelCased, through `snapfire_fsr_runtime::FromNativeValue`. The return value is encoded through `IntoNativeValue`; `()` answers `Value::Null`.
* A `fn` answers synchronously; an `async fn` answers a promise.
* `Type` must be `Clone + Send + Sync + 'static` (`snapfire_fsr_runtime::CallState`).
* Registered with the host builder's `native(name, Arc::new(value))`; a body calls it as `ctx.native.<name>.<method>({ ... })`.
* Dependencies of the crate using it: `snapfire_fsr_runtime`, `snapfire_fsr_core`.

## 2. Services

### service

* `#[service]` on `impl Type { ... }`: emits the block unchanged and implements `snapfire_fsr_service::Transport` and `snapfire_fsr_service::DeclaredService` for `Type`.
* `DeclaredService::NAME`: `Type`'s name in snake case, `Fleet` as `fleet`.
* `DeclaredService::contract() -> Contract`: one method per `pub` fn, named camelCased, its parameters as fields and its return as the type; one record per `Record` the signatures name.
* `Transport` answers a `Call` by its method name, each argument decoded from `call.args` under its parameter's name camelCased.
* A return of `Result<T, ServiceError>` declares `T` and fails the call with the error. Any other return cannot fail.
* A parameter of type `snapfire_fsr_service::Caller` is filled from the call and is not in the contract. At most one per method.
* Argument and return types implement `snapfire_fsr_service::ContractType`: the scalars, `String`, `Option`, `Vec`, string-keyed maps and `Record`.
* `Type` must be `Clone + Send + Sync + 'static`.
* Registered with the host builder's `service(Arc::new(value))`; a body calls it as `ctx.services.<NAME>.<method>({ ... })`.
* Dependencies of the crate using it: `snapfire_fsr_service`, `snapfire_fsr_runtime`, `snapfire_fsr_core`.

### cache

* `#[cache(ttl = "<duration>", tags = ["<tag>", ...], scope = "private" | "shared" | "subject", stale = "<duration>")]` on a `pub` method of a `#[service]` block: the method's `Method::cached` policy. `ttl` is required; each tag is a string literal.
* Removed from the emitted method.

### writes

* `#[writes("<tag>", ...)]` on a `pub` method of a `#[service]` block: the tags a call invalidates, `Method::writes`.
* Removed from the emitted method.

## 3. Records

### Record

* `#[derive(Record)]` on a struct with named `pub` fields and no type parameters: implements `IntoNativeValue`, `FromNativeValue` and `ContractType`.
* Each field is keyed by its name camelCased unless `#[record(name = "<key>")]` names it.
* An `Option` field is written as `null` on `None`. `#[record(optional)]`, on an `Option` field only, leaves the key out on `None` instead.

## 4. Error Handling

### Compile Errors

Each is reported at the item that breaks the rule:

* `` a native method takes `&self` ``, `` a service method takes `&self` ``
* `a native method's argument must be a plain name`, `a service method's argument must be a plain name`
* `` a service method takes `Caller` once ``
* `` a service is an `impl` of a named type ``
* `` a cache or writes policy sits on a `pub` method; a private one is not in the contract ``
* `a cache without a ttl`, `a tag is a string literal`
* `a record is a struct`, `a record's fields are named`, `` a record's fields are all `pub`; a private field cannot cross ``, `a record has no type parameters`
* `` `optional` is for an `Option` field, whose key it leaves out on `None` ``
* A type that is not `Clone`, `Send`, `Sync` and `'static` is refused at the impl. A type outside the value model is refused where `ContractType` or `FromNativeValue` is not implemented.

### Call Failures

* A method name the block does not declare: `FailureKind::NotFound`.
* An argument missing or not of the declared type: `FailureKind::Invalid`, the message naming its key.
* A `Result` method's `Err`: the call's failure as returned.
