# snapfire_fsr_tera

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_tera.svg)](https://crates.io/crates/snapfire_fsr_tera)
[![Docs.rs](https://docs.rs/snapfire_fsr_tera/badge.svg)](https://docs.rs/snapfire_fsr_tera)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The Tera evaluator for SnapFire FSR. It implements `snapfire_fsr_runtime::Evaluator` by rendering a Tera template to a string and splitting that string into payload chunks: literal markup becomes a raw node, an `island()` call becomes a client node the browser mounts, a `slot()` call becomes the stitch point where a plan child's subtree lands. There is no JavaScript engine here, no hydration protocol, no participation in the module graph; the whole crate is one file, which is the point. The runtime half of the seam, the `Evaluator` trait plus the assembler that stitches slots, lives in `snapfire_fsr_runtime`. To wire one up, read the [usage guide](README.USAGE.md); for signatures and constraints, the [API reference](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_tera = "0.5"
snapfire_fsr_runtime = "0.5"
snapfire_fsr_core = "0.5"
tera = { version = "2", features = ["fast"] }
```

A `Tera` instance is built by the application and handed over, so an application's own filters, functions and tests stay available inside every template this evaluator renders.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Render `.tera` modules into payload nodes | `TeraEvaluator::new(tera)` |
| Teach a `Tera` instance the marker functions | `register_markers(&mut tera)` |
| Send `.tera` modules to this evaluator | `Evaluators::register(predicate, Arc::new(evaluator))` |
| Mount a client component inside rendered markup | `island(module="...", props=...)` in the template |
| Leave a hole for a plan child's subtree | `slot(name="...")` in the template |
| Place the document head the assembler computed | `head()` in the template |
| Detect a marker token in a rendered string | `MARKER` |
