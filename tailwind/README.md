# snapfire_tailwind

License: MPL-2.0 for the crate, MIT for the Tailwind CSS sources and tests it carries. Status: Tailwind CSS v4.3.2.

Tailwind CSS v4 in Rust. A stylesheet configures it the way Tailwind's own does, with `@import "tailwindcss"`, `@theme`, `@utility`, `@custom-variant` and `@source inline(...)`. The crate builds that stylesheet for the class names you give it, with no Node and no file scanning. The [usage guide](README.USAGE.md) walks through each task and the [API reference](API_REFERENCE.md) lists the surface.

## Install

```toml
[dependencies]
snapfire_tailwind = "0.3"
```

The crate has no features. It depends on Lightning CSS for the optimize pass Tailwind runs on its output.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Build a stylesheet for a set of classes | `Compiler::new`, then `Compiler::build` |
| Resolve `@import` from your own files | `CompileOptions::load_stylesheet`, falling back to `tailwind_stylesheet` |
| Get the output Tailwind's CLI writes | `optimize` over what `build` returned |
| Tell a misspelt class from a real one | `Compiler::invalid_candidates` |
| Read what the stylesheet uses | `Compiler::features`, `Compiler::sources`, `Compiler::root` |
| Spell a class the way Tailwind prefers | `Compiler::canonical_candidate` |

## Status

The compile pipeline is a port of Tailwind CSS v4.3.2's `packages/tailwindcss`: the CSS parser, candidates, variants, utilities, `@apply`, `@import`, `theme()` and the compat utilities. Tailwind's own test files are extracted into fixtures and run against it; every compile fixture passes except two whose expected output comes from the older Lightning CSS release Tailwind's npm build pins. JavaScript plugins and configs (`@plugin`, `@config`) are refused with Tailwind's error, since the crate runs no JavaScript. File scanning, editor tooling (class order, IntelliSense, canonicalising class lists) and source maps of the generated CSS are not part of it.
