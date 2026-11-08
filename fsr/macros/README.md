# snapfire_fsr_macros

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_macros.svg)](https://crates.io/crates/snapfire_fsr_macros)
[![Docs.rs](https://docs.rs/snapfire_fsr_macros/badge.svg)](https://docs.rs/snapfire_fsr_macros)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Lets the TypeScript an FSR application runs on the server call your Rust code.

Put an attribute on an ordinary `impl` block and its `pub` methods become callable from a loader, an action or a route handler:

```rust
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
```

```ts
const words = ctx.native.digest.words({ bodies });
const servers = await ctx.services.fleet.list({ section: "eu" });
```

The block itself is emitted unchanged, so the methods stay plain Rust you can call and test directly.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_macros = "0"
```

The code the macros write names other FSR crates, so the crate that uses them depends on those too:

| Macro | Also depend on |
| --- | --- |
| `#[native]` | `snapfire_fsr_runtime`, `snapfire_fsr_core` |
| `#[service]`, `#[derive(Record)]` | `snapfire_fsr_service`, `snapfire_fsr_runtime`, `snapfire_fsr_core` |

A `#[native]` or `#[service]` type must be `Clone + Send + Sync + 'static`. The compile error says so when it isn't.

## What it covers

| Task | Item |
| --- | --- |
| Let a body call synchronous or async Rust as `ctx.native.<name>.<method>()` | `#[native]`, registered with the host builder's `native(name, ...)` |
| Serve a service in process as `ctx.services.<name>.<method>()`, with its contract written from the Rust signatures | `#[service]`, registered with `service(...)` |
| Pass a struct to or from either | `#[derive(Record)]` |
| Rename a record's key or leave it out on `None` | `#[record(name = "...")]`, `#[record(optional)]` |
| Cache a service method or invalidate its tags | `#[cache(...)]`, `#[writes(...)]` |
| Know who is calling | a `Caller` parameter |

Method, argument and field names are camelCased in TypeScript, so `fetch_count` is called as `fetchCount`. An `Option` field is written as `null` on `None` unless it is `#[record(optional)]`.
