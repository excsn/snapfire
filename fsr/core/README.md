# Snapfire FSR Core (`snapfire_fsr_core`)

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)
![Status: pre-release](https://img.shields.io/badge/status-pre--release-orange?style=flat-square)

The vocabulary crate of Snapfire FSR, the Full Stack Runtime. It holds the four things every other crate in the platform has to agree on before it can say anything: the `Value` model, the payload `Node` tree, the `PlanNode` render plan and canonical fingerprinting over all three. It depends on no other FSR crate, has no runtime, opens no socket and renders nothing; everything above it (`snapfire_fsr_payload` for encodings, `snapfire_fsr_runtime` for request handling, the evaluators, the session layer, the service layer) is written in these types. Task-by-task instructions live in the [usage guide](README.USAGE.md); every signature is in the [API reference](API_REFERENCE.md).

The value model is sovereign: it decides what can exist; encodings are ranked projections of it. An encoding is either lossless over the model or a declared degradation, never a silent one, which is why `Value` is not limited to what JSON can express. It carries `i128` and `u128` integers, `f32` separately from `f64`, raw bytes, typed numeric arrays, tagged variants and references. JSON tags whatever it cannot spell natively.

## Install

```toml
[dependencies]
snapfire_fsr_core = { path = "../core" }
```

The crate has no Cargo features. It compiles with two dependencies and no optional surface.

| Dependency | Why |
| :--- | :--- |
| `indexmap` | `ValueMap` and `Params` preserve insertion order for serialization |
| `xxhash-rust` (`xxh3`) | The 64-bit digest behind `Fingerprint` |

## What to reach for

| You want to | Reach for |
| :--- | :--- |
| Represent any data crossing the server/browser boundary | `Value` |
| Hold a props map or a loader result | `ValueMap`, aliased as `Props` and `Data` |
| Build an integer without picking a variant by hand | `Value::int`, `Value::uint` |
| Carry a numeric series without one `Value` per element | `Value::TypedArray` |
| Carry a Rust enum that arrives in TypeScript as a discriminated union | `Value::Variant` |
| Point at a server action or a client module | `Value::Ref`, `Value::action_ref` |
| Describe what a response renders to | `Node` |
| Emit trusted markup an evaluator produced | `Node::Raw`, wrapping `Html` |
| Mount a hydratable island with props | `Node::Client` |
| Leave a hole a later stream row fills | `Node::Pending` and its `SlotId` |
| Name a component by source path and export | `ModuleId` |
| Describe what a request renders, before anything is loaded | `PlanNode` |
| Declare that a segment streams rather than blocks | `PlanNode::deferred`, with `fallback` |
| Name the loader a segment waits on | `PlanNode::data_source` |
| Get a stable content hash for a cache key | `Fingerprint::fingerprint` |
| Decide whether two values mean the same thing | Compare fingerprints, not `PartialEq` |

## Status

Pre-release and unpublished. The API is not stable and carries no compatibility guarantee; consumers take it by path dependency inside the workspace. It is exercised end to end by the `advanced_tera_app` example under `fsr/examples/` and carries 19 tests of its own: `tests/vocabulary.rs` pins the fingerprint's canonical rules and `ModuleId` parsing, `tests/walk_fixtures.rs` pins the two hand-walked pages as literal `Node` and `PlanNode` values. `benches/fingerprint.rs` measures hashing a page tree and a nested map under criterion.
