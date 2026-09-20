# snapfire_fsr_macros

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_macros.svg)](https://crates.io/crates/snapfire_fsr_macros)
[![Docs.rs](https://docs.rs/snapfire_fsr_macros/badge.svg)](https://docs.rs/snapfire_fsr_macros)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Two attributes and a derive. `#[native]` on an `impl` block marks the methods a lowered TypeScript body may reach as `ctx.native.<module>.<method>()`. `#[service]` on an `impl` block declares a service a body calls as `ctx.services.<name>.<method>()`, served in process with its contract written from the signatures. `#[derive(Record)]` on a struct makes it a record either may name.

Each attribute writes its dispatcher and emits the block unchanged, so the methods stay ordinary Rust. A module holds another and calls it directly and only what the block declares `pub` ever crosses into TypeScript.

## Install

```toml
[dependencies]
snapfire_fsr_macros = "0"
```

`#[service]` and `#[derive(Record)]` expand to code naming `snapfire_fsr_service`, `snapfire_fsr_runtime` and `snapfire_fsr_core`, so those are dependencies of the crate that uses them; `#[native]` needs the latter two.

## A native module

```rust
use snapfire_fsr_macros::native;

#[derive(Clone)]
pub struct Digest;

#[native]
impl Digest {
  pub fn words(&self, bodies: Vec<String>) -> i64 {
    bodies.iter().map(|b| b.split_whitespace().count() as i64).sum()
  }
}
```

```rust
Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.native("digest", Arc::new(Digest)))
```

Rust is synchronous and so is a native method. A `fn` answers a value and an `async fn` answers a promise; the build reads which off the signature and types the TypeScript call accordingly.

## A service

```rust
use snapfire_fsr_macros::{service, Record};
use snapfire_fsr_runtime::{FailureKind, ServiceError};

#[derive(Record)]
pub struct Server {
  pub name: String,
  pub load: f64,
}

#[derive(Clone)]
pub struct Fleet { /* ... */ }

#[service]
impl Fleet {
  #[cache(ttl = "15s", tags = ["servers"], scope = "shared")]
  pub async fn list(&self, section: String) -> Result<Vec<Server>, ServiceError> {
    // ...
  }

  #[writes("servers")]
  pub fn add(&self, name: String, load: f64) -> u32 {
    // ...
  }
}
```

```rust
Host::from(env!("CARGO_MANIFEST_DIR")).map(|b| b.service(Arc::new(fleet)))
```

The attribute writes two impls. `Transport` answers a `Call` by matching its method name, decoding each argument from `call.args` under the camelCased name of the Rust parameter and encoding the answer. `DeclaredService` carries `NAME`, the type's name in snake case, together with `contract()`, the service as a `Contract`: one method per `pub` fn with its parameters as fields and its return as the type, one record per `Record` the signatures name. A method returning `Result<T, ServiceError>` declares `T` and its error travels as the call's failure; any other return type cannot fail. The types are resolved by the compiler through `snapfire_fsr_service::ContractType`, which the scalars, `String`, `Option`, `Vec`, the string-keyed maps and `Record` implement.

`#[cache]` and `#[writes]` on a method are the policy `Method::cached` and `Method::writes` hold, in the spelling the proto option and `x-sf-cache` use: `ttl`, `tags`, `scope` (`private`, `shared` or `subject`) and `stale`. The attribute takes them off the method it emits.

A TypeScript body calls the service as `services.fleet.list({ section })`. `fsr build` reads the block with `syn` before the crate compiles, writes its contract to `generated/contracts/rust.json` and types the call in `generated/services.d.ts`; the host merges that contract with the one `contract()` returns and refuses to boot when the two disagree, which is what a stale build looks like.

## What it expects

| Rule | Why |
| --- | --- |
| Every method takes `&self` | The module is registered as a trait object |
| The type is `Clone` | An async method needs an owned handle to outlive the call |
| Arguments are plain names | The name becomes the TypeScript argument key, camelCased |
| Only `pub` methods cross | Everything else stays private Rust for composition |
| A service argument or return is in the value model | The contract is checked at the wire; a type outside it is a compile error at the impl `ContractType` cannot find |
| A record's fields are named and all `pub` | A private field could not cross and the build reads only the `pub` ones |

The declarations TypeScript sees are read from the same signatures by `snapfire_fsr_cli`, which parses the source rather than expanding these macros, so the two halves cannot drift.
