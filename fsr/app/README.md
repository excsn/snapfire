# snapfire_fsr

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr.svg)](https://crates.io/crates/snapfire_fsr)
[![Docs.rs](https://docs.rs/snapfire_fsr/badge.svg)](https://docs.rs/snapfire_fsr)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Turns the output of `fsr build` into a running FSR application that lets you replace any part of it with Rust.

`fsr build` writes a plan file. It lists the application's routes, data sources, actions and components, most of them translated from TypeScript so Rust can run them. This crate connects each name in the plan to the code that answers it and gives you an `App`. It refuses to start when a name has no answer or two answers.

With only the plan file, everything the build translated answers for itself. You can then take any piece back in Rust: a data source, an action, a route or a handler. The boot report shows who answers each name:

```
routes    /                      plan file
          /cart                  plan file
sources   index                  lowered
          cart                   rust override
actions   cart.addToCart         lowered
```

`lowered` means the build translated it from TypeScript. `rust` and `rust override` mean your Rust code answers it.

Most applications use this through `snapfire_fsr_host`, which wraps the `App` in an HTTP server. Depend on it directly to write parts of your application in Rust or to build your own host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr = "0"
snapfire_fsr_core = "0"
```

## Quick start

```rust
use snapfire_fsr::App;
use snapfire_fsr_core::{Data, Value};

fn main() -> Result<(), snapfire_fsr::BindError> {
  let text = std::fs::read_to_string("app/generated/plan.sexp").expect("run `fsr build app` first");
  let app = App::from_manifest(&text)?
    .source_override("cart", |_ctx| async move {
      let mut data = Data::default();
      data.insert("count".to_owned(), Value::int(0));
      Ok(data)
    })
    .build()?;
  print!("{}", app.report);
  Ok(())
}
```

`source_override` takes a name the plan already has. Use `source` for a name the plan does not have. `build` fails if an override names nothing or a plain `source` claims a name the build already translated.

## What to reach for

| You want to | Use |
| --- | --- |
| Start from a plan file | `App::from_manifest` |
| Start from routes written in Rust | `App::builder(Routes)` |
| Write a route's plan in Rust | `Plan::of`, `source`, `deferred`, `fallback`, `error`, `cache_key`, `slot` |
| Add a route the plan does not have | `AppBuilder::route` |
| Replace a route the plan has | `AppBuilder::route_override` |
| Answer a data source in Rust | `AppBuilder::source`, `source_impl` |
| Take a translated loader back | `AppBuilder::source_override` |
| Answer or take back an action | `AppBuilder::action`, `action_impl`, `action_override` |
| Answer a route handler or the middleware in Rust | `AppBuilder::handler`, `middleware` and their `_override` forms |
| Render a module in Rust | `AppBuilder::evaluator` |
| Check translated actions' input | `AppBuilder::contract` |
| Reach the backend services | `AppBuilder::services` |
| Cache rendered subtrees | `AppBuilder::cache` |
| See who answers what | `App::report`, `Report`, `Owner` |
