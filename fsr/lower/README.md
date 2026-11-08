# snapfire_fsr_lower

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_lower.svg)](https://crates.io/crates/snapfire_fsr_lower)
[![Docs.rs](https://docs.rs/snapfire_fsr_lower/badge.svg)](https://docs.rs/snapfire_fsr_lower)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Turns the TypeScript an FSR application runs on the server into code Rust runs directly.

`fsr build` uses it to read loaders, actions, route handlers, middleware and components. Each one becomes a tree that [`snapfire_fsr_ir`](../ir/README.md) runs without a JavaScript engine. FSR calls this step lowering.

When a file uses something it can't translate, the build tells you where it stopped:

```
routes/x/page.loader.ts:5:18: `slugify` is not bound here; an import the build cannot follow or a name from outside the body
```

It never guesses. A body either translates whole or is reported.

Most applications never depend on this crate. `fsr` calls it for you. Use it directly when you build tooling around FSR or run the build inside your own host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_lower = "0"
```

## Quick start

```rust
use snapfire_fsr_lower::{lower_actions, lower_loader};

let body = lower_loader("routes/page.loader.ts", r#"
  export async function load({ params, services }) {
    return { products: await services.shopping.listProducts({ tag: params.tag }) };
  }
"#)?;

let actions = lower_actions("routes/cart/actions.ts", r#"
  export const checkout = action(async ({ session, services }) => {
    const order = await services.shopping.placeOrder({ lines: Object.entries(session.cart ?? {}) });
    session.cart = {};
    return order;
  });
"#)?;
assert_eq!(actions[0].export, "checkout");
```

## What it reads

| File | Call |
| --- | --- |
| `page.loader.ts` with an exported `load` | `lower_loader` |
| `actions.ts` with `export const x = action(...)` | `lower_actions`, with each action's input type on `LoweredAction::input` |
| `route.ts` with exports named for HTTP methods | `lower_handlers` |
| `middleware.ts` | `lower_middleware` |
| A React, Vue or Svelte component | `component::ComponentSet` |
| Interfaces under `schemas/` | `read_schema` |
| `export const defaults` in the session schema | `read_session_defaults` |

`lower_loader`, `lower_actions`, `lower_handlers` and `lower_middleware` each have a `_with` form that takes the session defaults. With them, a session read falls back to its declared default.

## When a file doesn't translate

`LowerError` separates a parse error and a missing export from a construct the build can't translate. Every other variant carries a `Residue` with the file, line, column and message.

A `Residue` prints as the line shown above. When it has them, the components it was reached through and a hint for the rewrite follow on their own indented lines.
