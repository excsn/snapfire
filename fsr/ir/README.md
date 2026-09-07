# snapfire_fsr_ir

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_ir.svg)](https://crates.io/crates/snapfire_fsr_ir)
[![Docs.rs](https://docs.rs/snapfire_fsr_ir/badge.svg)](https://docs.rs/snapfire_fsr_ir)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The lowered form of a loader or action body for SnapFire FSR plus the interpreter that runs it. A body is a small typed tree over the value model: reads from the request context, literals, field access, arithmetic and comparison, lambdas over typed arrays, service calls, session writes and guards. The runtime executes it directly over `Value`, through the same service handle and session cell a Rust data source uses, so a TypeScript loader lowered to this form runs with no JavaScript engine in the path. The build produces the tree; this crate carries its JSON form, its interpreter and the two adapters that make a body answer a data source id or an action id. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_ir = "0.5"
```

It depends on `snapfire_fsr_core` for the value model and on `snapfire_fsr_runtime` for `RequestCtx`, `ServiceHandle`, `SessionCell`, `FailureKind` and the `DataSource` and `ActionHandler` traits; it brings in `serde` and `serde_json` for the JSON form and `futures-util` for the parallel issue of independent calls.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Build a body in Rust | `Stmt`, `Expr` and the `Expr` constructors |
| Read a body out of a plan file | `ast::from_json` |
| Write a body into a plan file | `ast::to_json` |
| Run a body against a request | `Interpreter::run` |
| Pin the clock a body reads | `Interpreter::with_clock` and `Clock` |
| Make a body answer a data source id | `IrSource` |
| Make a body answer an action id | `IrAction` |
| See which session keys a body wrote | `Outcome::written` |
| Map a failed body onto a status | `Fail::kind`, a `FailureKind` |
| Find what a body reads or whether it calls | `Expr::free_vars`, `Expr::has_call` |
