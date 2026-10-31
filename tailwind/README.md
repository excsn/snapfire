# snapfire_tailwind

[![Crates.io](https://img.shields.io/crates/v/snapfire_tailwind.svg)](https://crates.io/crates/snapfire_tailwind)
[![Docs.rs](https://docs.rs/snapfire_tailwind/badge.svg)](https://docs.rs/snapfire_tailwind)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

License: MPL-2.0 for the crate, MIT for the Tailwind CSS sources and tests it carries. Status: Tailwind CSS v4.3.2.

Tailwind CSS v4 in Rust. A stylesheet configures it the way Tailwind's own does, with `@import "tailwindcss"`, `@theme`, `@utility`, `@custom-variant` and `@source inline(...)`. The crate builds that stylesheet for the class names you give it, with no Node and no file scanning. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_tailwind = "0.3"
```

The crate has no features. It depends on Lightning CSS for the optimize pass Tailwind runs on its output.

## What to reach for

| Problem | Piece |
| --- | --- |
| Build a stylesheet for a set of classes | `Compiler::new`, then `Compiler::build` |
| Resolve `@import` from your own files | `CompileOptions::load_stylesheet`, falling back to `tailwind_stylesheet` |
| Get the output Tailwind's CLI writes | `optimize` over what `build` returned |
| Tell a misspelt class from a real one | `Compiler::invalid_candidates` |
| Read what the stylesheet uses | `Compiler::features`, `Compiler::sources`, `Compiler::root` |
| Spell a class the way Tailwind prefers | `Compiler::canonical_candidate` |

## Status

The compile pipeline is a port of Tailwind CSS v4.3.2's `packages/tailwindcss`: the CSS parser, candidates, variants, utilities, `@apply`, `@import`, `theme()` and the compat utilities. Tailwind's own test files are extracted into fixtures and run against it; every compile fixture passes except two whose expected output comes from the older Lightning CSS release Tailwind's npm build pins. JavaScript plugins and configs (`@plugin`, `@config`) are refused with Tailwind's error, since the crate runs no JavaScript. File scanning, editor tooling (class order, IntelliSense, canonicalising class lists) and source maps of the generated CSS are not part of it.
