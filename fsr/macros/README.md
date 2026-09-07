# snapfire_fsr_macros

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_macros.svg)](https://crates.io/crates/snapfire_fsr_macros)
[![Docs.rs](https://docs.rs/snapfire_fsr_macros/badge.svg)](https://docs.rs/snapfire_fsr_macros)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

One attribute: `#[native]` on an `impl` block marks the methods a lowered TypeScript body may reach as `ctx.native.<module>.<method>()`.

The attribute writes the dispatcher and emits the block unchanged, so the methods stay ordinary Rust. A module holds another and calls it directly, and only what the block declares `pub` ever crosses into TypeScript.

## Install

```toml
[dependencies]
snapfire_fsr_macros = "0.5"
```

## Use

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

## What it expects

| Rule | Why |
| --- | --- |
| Every method takes `&self` | The module is registered as a trait object |
| The type is `Clone` | An async method needs an owned handle to outlive the call |
| Arguments are plain names | The name becomes the TypeScript argument key, camelCased |
| Only `pub` methods cross | Everything else stays private Rust for composition |

The declarations TypeScript sees are read from the same signature by `snapfire_fsr_cli`, which parses the source rather than expanding this macro, so the two halves cannot drift.
