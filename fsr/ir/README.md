# snapfire_fsr_ir

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_ir.svg)](https://crates.io/crates/snapfire_fsr_ir)
[![Docs.rs](https://docs.rs/snapfire_fsr_ir/badge.svg)](https://docs.rs/snapfire_fsr_ir)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Runs an FSR application's server TypeScript in Rust, with no JavaScript engine.

`fsr build` translates each loader, action, route handler, middleware and component into a small typed tree. This crate holds that tree and runs it. A translated loader reads the request, calls services and writes the session through the same handles a Rust data source uses. A translated component renders the same markup React, Vue or Svelte would write on the server, so the browser hydrates over it.

Most applications never depend on this crate. `fsr` and the FSR host use it for you. Use it directly when you build tooling around FSR, run translated bodies in your own host or write a body by hand in Rust.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_ir = "0"
```

`intl_compiled` builds every locale's data into the crate, about 20 MB, which the `intl` functions format from when a program installs none. Without it a program installs the data a build cut for its locales, through `IntlData::install`, before it formats anything.

`tz_compiled` and `tz_system` choose where `Date`'s local time reads its zone rules.

## Quick start

The loader `return { products: await services.shopping.listProducts({ tag: query.tag }) }`, written in Rust and run against a request:

```rust
use snapfire_fsr_core::Value;
use snapfire_fsr_ir::{Expr, Interpreter, Stmt};

let body = vec![Stmt::Return(Expr::object(vec![(
  "products",
  Expr::call("shopping", "listProducts", vec![("tag", Expr::Query("tag".into()))]),
)]))];

let outcome = Interpreter::default().run(&body, &ctx, None).await?;
let Value::Map(data) = outcome.value else { unreachable!("a loader returns an object") };
```

`ctx` is the `snapfire_fsr_runtime::RequestCtx` the host builds for the request.

## What it covers

* **Bodies.** Loaders, actions, route handlers, middleware and themes, with service calls, session writes, guards, loops, `try` and closures.
* **Components.** Server rendering byte for byte as React 18 and 19, Vue 3 and Svelte 5 write it, plus islands that round-trip their events to the server.
* **A standard library.** `intl`, `text`, `time`, `crypto`, `encoding`, `path`, `file`, `i18n` and `log`, each answering the same on the server and in the browser where it runs in both.
* **JavaScript semantics.** Numbers, strings in UTF-16 code units, sorting, regular expressions and `Math` follow what V8 does.
* **Ahead-of-time compilation.** A production build can compile a body to Rust and run it in place of the interpreter.

## What to reach for

| Problem | Piece |
| --- | --- |
| Build a body in Rust | `Stmt`, `Expr` and the `Expr` constructors |
| Read or write a body as s-expressions | `sexpr::parse`, `sexpr::print`, `sexpr::stmt_from_sx` |
| Run a body against a request | `Interpreter::run` |
| Pin the clock a body reads | `Interpreter::with_clock` and `Clock` |
| Render a translated component | `Interpreter::render`, `IrEvaluator` |
| Make a body answer a data source id | `IrSource` |
| Make a body answer an action id | `IrAction` |
| Add a function bodies can call | `Extensions::register`, `Interpreter::with_extensions` |
| See which session keys a body wrote | `Outcome::written` |
| Map a failed body onto a status | `Fail::kind`, a `FailureKind` |
| Find what a body reads or whether it calls | `Expr::free_vars`, `Expr::has_call` |
