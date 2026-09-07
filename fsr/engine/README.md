# snapfire_fsr_engine

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_engine.svg)](https://crates.io/crates/snapfire_fsr_engine)
[![Docs.rs](https://docs.rs/snapfire_fsr_engine/badge.svg)](https://docs.rs/snapfire_fsr_engine)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

QuickJS in process, for `fsr test` and nothing else. One engine holds one context: a DOM from linkedom, timers on a virtual clock, a `fetch` the host answers and the application's compiled modules resolved through its import map. That is what lets a page spec render a real React tree, click a real button and see a real action answer, without a browser and without a network.

Nothing here runs at request time. A page renders in Rust through the IR, so the serving path holds no JavaScript engine; this crate exists because a component test needs a DOM and the interpreter cannot supply one.

## Install

```toml
[dependencies]
snapfire_fsr_engine = "0.5"
```

It builds QuickJS through `rquickjs`, so a build takes longer than the rest of the workspace and the binary is larger. That is the reason it is a crate of its own rather than a feature of the runner: nothing that only serves has to compile it.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Run a spec file in its own context | `Engine` |
| Answer the module a spec imports | `Resolution` |
| Answer a `fetch` the page made | `FetchResponse` |
| Hand a mocked service call to Rust and wait for its answer | `JsCalls` |
| Call a native pair's browser half | `native` |
