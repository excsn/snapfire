# snapfire_fsr_core

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_core.svg)](https://crates.io/crates/snapfire_fsr_core)
[![Docs.rs](https://docs.rs/snapfire_fsr_core/badge.svg)](https://docs.rs/snapfire_fsr_core)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The data types every SnapFire FSR crate shares: values, the page tree a response renders to, the render plan and stable content hashes.

* `Value` is any data that crosses between server and browser.
* `Node` is the tree a response renders to: text, trusted markup, islands and streaming holes.
* `PlanNode` describes what a request will render before anything is loaded.
* `Fingerprint` gives a stable 64-bit hash of a value, a tree or a plan, for cache keys.
* `ext` is what the Rust half of a native extension is written against.

`Value` holds more than JSON can. It keeps `i128` and `u128` integers, `f32` apart from `f64`, raw bytes, typed numeric arrays, tagged variants and references to an action or a module. It also keeps JavaScript's `undefined` apart from `null`. Encodings in other crates either keep all of that or say what they drop.

The crate has no runtime, opens no socket and renders nothing. Every other FSR crate builds on it. Depend on it directly when you write loaders, actions, evaluators or extensions in Rust.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_core = "0"
```

## Quick start

```rust
use snapfire_fsr_core::{Fingerprint, ModuleId, Node, TypedArray, Value, ValueMap};

let mut props = ValueMap::default();
props.insert("series", Value::TypedArray(TypedArray::F64(vec![1.0, 2.5, 3.0])));
props.insert("title", Value::str("servers"));

let page = Node::Seq(vec![
  Node::raw("<section><h1>Servers</h1>"),
  Node::Client { module: ModuleId::new("components/ServerChart.tsx", "default"), props, children: Vec::new(), rendered: false },
  Node::raw("</section>"),
]);

println!("{:016x}", page.fingerprint());
```

Two values with the same content always have the same fingerprint, whatever order their map keys were inserted in.

## What to reach for

| You want to | Use |
| :--- | :--- |
| Represent data crossing the server/browser boundary | `Value` |
| Hold a props map or a loader result | `ValueMap`, aliased as `Props` and `Data` |
| Build an integer without picking a variant by hand | `Value::int`, `Value::uint` |
| Carry a numeric series without one `Value` per element | `Value::TypedArray` |
| Carry a Rust enum that arrives in TypeScript as a tagged union | `Value::Variant` |
| Point at a server action or a client module | `Value::Ref`, `Value::action_ref` |
| Describe what a response renders to | `Node` |
| Emit trusted markup | `Node::Raw`, wrapping `Html` |
| Mount an island with props | `Node::Client` |
| Leave a hole a later stream row fills | `Node::Pending` and its `SlotId` |
| Name a component by source path and export | `ModuleId` |
| Describe what a request renders, before anything loads | `PlanNode` |
| Mark a segment that streams instead of blocking | `PlanNode::deferred`, with `fallback` |
| Name the loader a segment waits on | `PlanNode::data_source` |
| Get a stable content hash for a cache key | `Fingerprint::fingerprint` |
| Write or read JSON for code outside FSR | `json::stringify`, `json::to_value` |
| Parse a lifetime like `30s` or `5m` | `parse_duration` |
| Write the Rust half of a native extension | `ext::Reach`, `ext::Ambient`, `ext::Fail` |
| Read an extension's arguments | `ext::number`, `ext::text`, `ext::text_opt`, `ext::option` |
| Hold the message catalogs `i18n.t` reads | `ext::Catalogs` |
