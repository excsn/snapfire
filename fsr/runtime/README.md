# snapfire_fsr_runtime

[![License: MPL 2.0](https://img.shields.io/badge/License-MPL_2.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The request blocks of SnapFire FSR: matching a path, resolving it to a plan, loading the plan's data, evaluating its modules, assembling the payload tree and streaming it out. It works in the vocabulary types of `snapfire_fsr_core` (`Value`, `Node`, `PlanNode`) and hands its result to the encoders in `snapfire_fsr_payload`. Task-by-task instructions live in the [usage guide](README.USAGE.md); the full surface is in the [API reference](API_REFERENCE.md).

```text
path
  |  Matcher            -> RouteMatch { entry, params }
  |  Resolver           -> PlanNode tree
  |  DataSource         -> every non-deferred node's data, in parallel, to completion
  |  Evaluator          -> a Chunk stream per module, only for nodes the cache missed
  |  assemble           -> Assembly { tree, pending, segments }
  v  html_stream / wire_stream
response
```

Data resolves before rendering starts and deferral is declared in the plan rather than discovered mid-render, so an evaluation is a plain `(module, props)` call; a cache hit never reaches an engine at all.

## Install

```toml
[dependencies]
snapfire_fsr_runtime = { path = "../runtime" }
snapfire_fsr_core = { path = "../core" }
```

The crate has no cargo features. Everything below is always compiled.

| Dependency it pulls in | Why |
| :--- | :--- |
| `matchit` | the path router behind `MatchitMatcher` |
| `fibre_cache` | the sharded, TinyLFU-bounded, TTL-expiring store behind `FibreCache` |
| `futures-util` | the boxed futures and streams every seam is written in |
| `xxhash-rust` | the subtree data fingerprint that goes into a cache key |
| `tracing` | the `fsr::load`, `fsr::cache`, `fsr::stream` and `fsr::action` events |

## What to reach for

| You want to | Reach for |
| :--- | :--- |
| Turn a path into a route entry plus params | `Matcher`, `MatchitMatcher` |
| Turn a route entry into a render plan | `Resolver`, `TableResolver` |
| Load a segment's data before anything renders | `DataSources::insert_fn` |
| Turn a module plus props into nodes | `Evaluator`, `Chunk`, `NodeChunks` |
| Ship a module the server has no engine for | `NullEvaluator` |
| Wire the pipeline together once | `Runtime::builder` |
| Turn a plan plus a request into a payload | `assemble`, `Assembly` |
| Hold a region open and fill it later | `deferred` and `fallback` on the plan node |
| Skip evaluation for an unchanged subtree | `cache_key` on the plan node, plus a `NodeCache` |
| Get a bounded, expiring subtree cache | `FibreCache::bounded` |
| Drop everything one plan key produced | `NodeCache::invalidate` |
| Keep DOM and island state across a navigation | `SegmentKeyer`, `DefaultKeyer`, `SegmentInfo` |
| Carry params, session and CSRF into a loader | `RequestCtx` |
| Read who the request is | `Identity`, `SessionCell::identity` |
| Call the service layer from a loader or action | `ServiceHandle::call` |
| Handle a mutation posted by the browser | `ActionRegistry` |
| Say what kind of failure it was | `FailureKind`, `FailureKind::http_status` |
| Stream the first HTML response | `html_stream`, `FILL_SCRIPT` |
| Stream a client navigation payload | `wire_stream`, `segments_to_json` |

## Status

Pre-release and unpublished: version 0.1.0, edition 2024, MPL-2.0, no crates.io release. The public surface is not stable. It carries 20 integration tests across `tests/assembler.rs`, `tests/cache.rs`, `tests/failure.rs` and `tests/streaming.rs`. It is exercised end to end by the `advanced_tera_app` example under `fsr/examples/`, which serves matched routes, a cached page, a deferred chart and a service-backed action off this crate.
