# snapfire_fsr_macros

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_macros.svg)](https://crates.io/crates/snapfire_fsr_macros)
[![Docs.rs](https://docs.rs/snapfire_fsr_macros/badge.svg)](https://docs.rs/snapfire_fsr_macros)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Two attributes and a derive. `#[native]` on an `impl` block marks the methods a lowered TypeScript body may reach as `ctx.native.<module>.<method>()`. `#[service]` on an `impl` block declares a service a body calls as `ctx.services.<name>.<method>()`, served in process with its contract written from the signatures. `#[derive(Record)]` on a struct makes it a record either may name, each `pub` field keyed camelCased or as `#[record(name = "...")]` names it. An `Option` field is written as `null` on `None`; `#[record(optional)]` leaves its key out instead. Each attribute emits its block unchanged beside the dispatcher it writes, so the methods stay ordinary Rust. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_macros = "0"
```

`#[service]` and `#[derive(Record)]` expand to code naming `snapfire_fsr_service`, `snapfire_fsr_runtime` and `snapfire_fsr_core`, so those are dependencies of the crate that uses them; `#[native]` needs the latter two. A `#[native]` or `#[service]` type derives `Clone`, since each call's future owns a copy; the error says so when it does not.

## What to reach for

| Problem | Piece |
| --- | --- |
| Let a body call synchronous or async Rust | `#[native]` on an `impl` block, registered with `native(name, ...)` |
| Serve a service in process with its contract written from Rust | `#[service]`, registered with `service(...)` |
| Pass a struct to or from either | `#[derive(Record)]` |
| Rename or omit a record's key | `#[record(name = "...")]`, `#[record(optional)]` |
| Cache a service method or invalidate its tags | `#[cache(...)]`, `#[writes(...)]` |
| Know who is calling | a `Caller` parameter |
