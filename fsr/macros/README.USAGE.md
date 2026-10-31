# Usage Guide: snapfire_fsr_macros

How to give a lowered TypeScript body Rust to call: a native module, a service and the records they pass.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Writing a Native Module](#writing-a-native-module)
* [Declaring a Service](#declaring-a-service)
  * [Failing a Call](#failing-a-call)
  * [Reading the Caller](#reading-the-caller)
  * [Setting a Cache Policy](#setting-a-cache-policy)
* [Declaring a Record](#declaring-a-record)
* [Calling It from TypeScript](#calling-it-from-typescript)
* [Staying Within the Rules](#staying-within-the-rules)
* [Error Handling](#error-handling)

## Core Concepts

* **Native module**: an `impl` block marked `#[native]`, whose `pub` methods a body reaches as `ctx.native.<module>.<method>()`.
* **Service**: an `impl` block marked `#[service]`, which a body calls as `ctx.services.<name>.<method>()`, served in process.
* **Record**: a struct with `#[derive(Record)]`, a value a native method or a service passes or returns.
* **Dispatcher**: the impl an attribute writes beside the block, matching a call's method name and decoding its arguments. The block itself is emitted unchanged.
* **Contract**: the service as `snapfire_fsr_service::Contract`, one method per `pub` fn and one record per `Record` the signatures name.
* **Caller**: a `snapfire_fsr_service::Caller` parameter, filled from the call rather than the arguments.
* **Policy**: what `#[cache]` and `#[writes]` say of a method, the same `Method::cached` and `Method::writes` a proto option or `x-sf-cache` sets.
* **Key**: the name an argument or a record field has in TypeScript, the Rust name camelCased unless `#[record(name = "...")]` gives another.

## Quick Start

```rust
use std::sync::Arc;

use snapfire_fsr_macros::{native, service, Record};

#[derive(Clone)]
pub struct Digest;

#[native]
impl Digest {
  pub fn words(&self, bodies: Vec<String>) -> i64 {
    bodies.iter().map(|b| b.split_whitespace().count() as i64).sum()
  }
}

#[derive(Record)]
pub struct Server {
  pub name: String,
  pub load: f64,
}

#[derive(Clone)]
pub struct Fleet;

#[service]
impl Fleet {
  pub fn list(&self, section: String) -> Vec<Server> {
    vec![Server { name: section, load: 0.5 }]
  }
}

let host = Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.native("digest", Arc::new(Digest)).service(Arc::new(Fleet)));
```

```ts
const words = ctx.native.digest.words({ bodies });
const servers = await ctx.services.fleet.list({ section: "eu" });
```

## Writing a Native Module

```rust
use snapfire_fsr_macros::native;

#[derive(Clone)]
pub struct Digest;

#[native]
impl Digest {
  pub fn words(&self, bodies: Vec<String>) -> i64 {
    bodies.iter().map(|b| b.split_whitespace().count() as i64).sum()
  }

  pub async fn fetch_count(&self, url: String) -> i64 {
    // ...
  }
}
```

```rust
Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.native("digest", Arc::new(Digest)))
```

A `fn` answers a value and an `async fn` answers a promise; the build reads which off the signature and types the TypeScript call to match. A method a body names that the block does not declare answers `FailureKind::NotFound`.

## Declaring a Service

```rust
use snapfire_fsr_macros::{service, Record};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

#[derive(Clone)]
pub struct Fleet { /* ... */ }

#[service]
impl Fleet {
  pub async fn list(&self, section: String) -> Result<Vec<Server>, ServiceError> {
    // ...
  }

  pub fn add(&self, name: String, load: f64) -> u32 {
    // ...
  }
}
```

```rust
Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.service(Arc::new(fleet)))
```

The service's name is the type's in snake case, `fleet`. Each argument is decoded from the call under the camelCased name of its Rust parameter.

### Failing a Call

```rust
pub async fn get(&self, id: String) -> Result<Server, ServiceError> {
  self.find(&id).ok_or_else(|| ServiceError::new(FailureKind::NotFound, "fleet", "get", format!("no server {id}")))
}
```

A method returning `Result<T, ServiceError>` declares `T` and its error travels as the call's failure. Any other return type cannot fail.

### Reading the Caller

```rust
use snapfire_fsr_service::Caller;

pub fn mine(&self, caller: Caller) -> Result<Vec<Server>, ServiceError> {
  let identity = caller.require()?;
  Ok(self.owned_by(&identity.subject))
}
```

`caller.identity` is the session's identity, `None` for an anonymous visitor; `caller.metadata` is what the interceptors added; `caller.require()?` answers `FailureKind::Unauthorized` when nobody is signed in. The parameter never appears in the contract and the call's credentials stay in the transport.

### Setting a Cache Policy

```rust
#[cache(ttl = "15s", tags = ["servers"], scope = "shared")]
pub async fn list(&self, section: String) -> Result<Vec<Server>, ServiceError> {
  // ...
}

#[writes("servers")]
pub fn add(&self, name: String, load: f64) -> u32 {
  // ...
}
```

`ttl`, `tags`, `scope` (`private`, `shared` or `subject`) and `stale` are the spelling the proto option and `x-sf-cache` use. `#[writes]` names the tags a call invalidates. The attribute is taken off the method it emits.

## Declaring a Record

```rust
use snapfire_fsr_macros::Record;

#[derive(Record)]
pub struct Server {
  pub name: String,
  #[record(name = "created_at")]
  pub created: String,
  pub note: Option<String>,
  #[record(optional)]
  pub owner: Option<String>,
}
```

Each `pub` field is keyed camelCased or as `#[record(name = "...")]` names it. An `Option` field is written as `null` on `None`; `#[record(optional)]` leaves its key out instead.

## Calling It from TypeScript

```ts
const servers = await ctx.services.fleet.list({ section });
const words = ctx.native.digest.words({ bodies });
```

`fsr build` reads the block with `syn` before the crate compiles, writes the service's contract to `generated/contracts/rust.json` and types the call in `generated/services.d.ts`. The host merges that contract with the one `contract()` returns and refuses to boot when the two disagree, which is what a stale build looks like. The declarations come from parsing the source, not from expanding these macros, so the two halves cannot drift.

## Staying Within the Rules

```rust
#[derive(Clone)]
pub struct Fleet;

#[service]
impl Fleet {
  pub fn list(&self, section: String) -> Vec<Server> { /* crosses into TypeScript */ }

  fn load(&self) -> f64 { /* stays private Rust */ }
}
```

| Rule | Why |
| --- | --- |
| Every method takes `&self` | The module is registered as a trait object |
| The type is `Clone` | An async method needs an owned handle to outlive the call |
| Arguments are plain names | The name becomes the TypeScript argument key, camelCased |
| Only `pub` methods cross | Everything else stays private Rust for composition |
| A service argument or return is in the value model | The contract is checked at the wire; a type outside it is a compile error at the impl `ContractType` cannot find |
| A record's fields are named and all `pub` | A private field could not cross and the build reads only the `pub` ones |
| A method takes `Caller` at most once | It is the one parameter the call fills rather than the caller |

## Error Handling

Each rule above is a compile error at the item that breaks it:

```text
error: a native method takes `&self`
error: a service is an `impl` of a named type
error: a cache or writes policy sits on a `pub` method; a private one is not in the contract
error: a record's fields are named
```

A type that is not `Clone` is refused where the impl names it. At run time a method a body names that the block lacks answers `FailureKind::NotFound`. An argument that is missing or of another type answers `FailureKind::Invalid`, naming its key.
