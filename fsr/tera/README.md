# snapfire_fsr_tera

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_tera.svg)](https://crates.io/crates/snapfire_fsr_tera)
[![Docs.rs](https://docs.rs/snapfire_fsr_tera/badge.svg)](https://docs.rs/snapfire_fsr_tera)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Renders FSR pages and layouts from Tera templates.

A template renders to HTML on the server. Inside it, `island(...)` places a client component the browser mounts, `slot(...)` marks where a child page goes and `head()` marks where the document head goes. `on(...)` binds an island's event handlers. No JavaScript engine runs on the server.

An application on the stock host needs none of the Rust API below. With the host's `tera` feature, which `fsr serve` has by default, a `page.tera` or `layout.tera` under `routes/` is a route. Every `.tera` file under the app is loaded at boot by its path and rendered with the loader's data.

Depend on this crate directly when you build your own `Tera` instance in a Rust host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_tera = "0"
snapfire_fsr_runtime = "0"
snapfire_fsr_core = "0"
tera = { version = "2", features = ["fast"] }
```

You build the `Tera` instance and hand it over, so your own filters, functions and tests work in every template.

## Quick start

`templates/page.tera`:

```jinja
<section>
  <h1>{{ params.section }}</h1>
  {{ island(module="components/ServerChart.tsx#default", props=chart) }}
  <main>{{ slot(name="content") }}</main>
</section>
```

The wiring:

```rust
use std::sync::Arc;

use snapfire_fsr_core::ModuleId;
use snapfire_fsr_runtime::Evaluators;
use snapfire_fsr_tera::{register_markers, TeraEvaluator};

let mut tera = tera::Tera::new();
register_markers(&mut tera);
tera.add_raw_templates([("page.tera", include_str!("../templates/page.tera"))]).expect("templates parse");

let mut evaluators = Evaluators::new();
evaluators.register(|m: &ModuleId| m.path.ends_with(".tera"), Arc::new(TeraEvaluator::new(tera)));
```

Call `register_markers` before adding templates. Tera checks function names when it parses a template.

## What to reach for

| You want to | Use |
| --- | --- |
| Render `.tera` modules into page output | `TeraEvaluator::new(tera)` |
| Teach a `Tera` instance the FSR functions | `register_markers(&mut tera)` |
| Send `.tera` modules to this evaluator | `Evaluators::register(predicate, Arc::new(evaluator))` |
| Mount a client component inside the markup | `island(module="...", props=...)` in the template |
| Bind an island's event handlers | `on(click="...")` in the template |
| Leave a hole for a child page | `slot(name="...")` in the template |
| Place the document head | `head()` in the template |
| Ask whether a template was added | `TeraEvaluator::has(name)` |
| Detect a marker token in a rendered string | `MARKER` |
