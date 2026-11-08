# snapfire_fsr_engine

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_engine.svg)](https://crates.io/crates/snapfire_fsr_engine)
[![Docs.rs](https://docs.rs/snapfire_fsr_engine/badge.svg)](https://docs.rs/snapfire_fsr_engine)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Runs an FSR application's pages in an embedded JavaScript engine so `fsr test` can test components without a browser.

A page spec can render a React tree, click a button and see the action answer, with no browser and no network. The engine is QuickJS, running in the test process.

Each `Engine` holds one JavaScript context with:

* a DOM from linkedom
* timers on a virtual clock
* a `fetch` that the test runner answers
* the application's compiled modules, resolved through its import map

Formatting calls such as dates and currencies go to the runner, which answers them with the same Rust code the server uses.

Nothing here runs when a site serves a request. A served page renders in Rust from the tree [`snapfire_fsr_ir`](../ir/README.md) runs, with no JavaScript engine. This engine exists only for tests, which need a real DOM.

Most applications never depend on this crate. `fsr test` uses it for you. Use it directly when you build your own test runner around FSR pages.

## Install

```toml
[dependencies]
snapfire_fsr_engine = "0"
```

It compiles QuickJS through `rquickjs`. That makes the build slower and the binary larger, so a crate that only serves should not depend on it.

## What it covers

| Task | Item |
| --- | --- |
| Run a spec file in its own context | `Engine` |
| Answer what the page asks of the runner: contexts, server renders, formatting and `fetch` | the `Hooks` trait |
| Map an import specifier to a file | `Resolution` |
| Answer a `fetch` the page made | `FetchResponse` |
| Hand a mocked service call to the page and wait for its answer | `JsCalls` |
| Call a native pair's browser half | `native` |
| Size the stack of a thread an engine runs on | `NATIVE_STACK` |

A bare specifier resolves to its exact import map key, else to the longest key ending in `/` that it starts with.
