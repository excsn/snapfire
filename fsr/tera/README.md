# snapfire_fsr_tera

MPL-2.0. Version 0.1.0, pre-release and unpublished.

The Tera evaluator for SnapFire FSR. It implements `snapfire_fsr_runtime::Evaluator` by rendering a Tera template to a string and splitting that string into payload chunks: literal markup becomes a raw node, an `island()` call becomes a client node the browser mounts, a `slot()` call becomes the stitch point where a plan child's subtree lands. There is no JavaScript engine here, no hydration protocol, no participation in the module graph; the whole crate is one file, which is the point. The runtime half of the seam, the `Evaluator` trait plus the assembler that stitches slots, lives in `snapfire_fsr_runtime`. To wire one up, read the [usage guide](README.USAGE.md); for signatures and constraints, the [API reference](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_tera = { path = "../tera" }
snapfire_fsr_runtime = { path = "../runtime" }
snapfire_fsr_core = { path = "../core" }
tera = { version = "2", features = ["fast"] }
```

| Feature | Effect |
| --- | --- |
| (none) | The crate declares no Cargo features. |

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

## Status

Pre-release, unpublished, no stability guarantee on any name here. The crate carries no tests of its own: it is exercised end to end by the `advanced_tera_app` example under `fsr/examples/`, whose 21 integration tests cover rendering, streaming, actions, sessions, auth and services through these templates.
