# snapfire_fsr

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

The binding rule of SnapFire FSR. A plan file names routes, data sources, actions and components; this crate binds each name to what answers it and refuses to start when something is unanswered or claimed twice. With nothing but the file, every lowered row binds itself to the interpreter and the report says `lowered` on every line. Rust takes a name back with `source_override` or `action_override`, adds one with `source`, `action` or `route` and the report says `rust` or `rust override` where it did. The result is an `App`: the matcher, the resolver, the runtime, the services, the action registry and the report, which `snapfire_fsr_host` wraps in an HTTP edge. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr = { path = "../app" }
```

No features. The crate depends on `snapfire_fsr_plan` to read the file, `snapfire_fsr_ir` to bind lowered rows, `snapfire_fsr_runtime` for the blocks it assembles and `snapfire_fsr_service` for the contract an action's input is checked against.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Start from a plan file | `App::from_manifest` |
| Start from routes written in Rust | `App::builder(Routes)` |
| Write a route's plan in Rust | `Plan::of`, `source`, `deferred`, `fallback`, `error`, `cache_key`, `slot` |
| Add a route the file does not have | `AppBuilder::route` |
| Replace a route the file has | `AppBuilder::route_override` |
| Answer a data source in Rust | `AppBuilder::source`, `source_impl` |
| Take a lowered loader back | `AppBuilder::source_override` |
| Answer or take back an action | `AppBuilder::action`, `action_impl`, `action_override` |
| Render a module in Rust | `AppBuilder::evaluator` |
| Give lowered actions their input check | `AppBuilder::contract` |
| Reach the services | `AppBuilder::services` |
| Cache evaluated subtrees | `AppBuilder::cache` |
| See who answers what | `App::report`, `Report`, `Owner` |

## Status

Pre-release and unpublished. `snapfire_fsr_host` builds every stock host through it and `shopping_react_ts` runs on that, with one route added in Rust beside the plan file's three. The crate's 14 tests cover an unanswered source, an override that names nothing, the report, a route claimed twice, a replaced route, a pattern the matcher refuses, routes with no plan file, a bad manifest, the plan builder's numbering, a module the builder cannot parse, a hand-built node, a lowered row binding itself, Rust taking a lowered name back only as an override and an action override that names nothing.
