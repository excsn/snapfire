# snapfire_tailwind

[![Crates.io](https://img.shields.io/crates/v/snapfire_tailwind.svg)](https://crates.io/crates/snapfire_tailwind)
[![Docs.rs](https://docs.rs/snapfire_tailwind/badge.svg)](https://docs.rs/snapfire_tailwind)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Builds Tailwind CSS v4.3.2 stylesheets in Rust, with no Node.js and no file scanning. You give it the stylesheet and the class names your pages use. It returns the CSS Tailwind would write for them.

The stylesheet is configured the way Tailwind's own is, with `@import "tailwindcss"`, `@theme`, `@utility`, `@custom-variant` and `@source inline(...)`.

`snapfirec` and `fsr` use it to build a Tailwind stylesheet from the classes in an application's sources. Use the crate directly when your own tool knows the class names.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_tailwind = "0"
```

No features.

## Quick start

```rust
use snapfire_tailwind::{CompileOptions, Compiler};

fn main() -> Result<(), snapfire_tailwind::Error> {
  let css = "@import \"tailwindcss\";\n@theme {\n  --color-brand: #e11d48;\n}";
  let mut compiler = Compiler::new(css, CompileOptions::default())?;
  let out = compiler.build(["flex", "p-4", "bg-brand", "hover:underline"]);
  assert!(out.contains(".bg-brand"));
  Ok(())
}
```

Run `optimize` over the result to get what Tailwind's CLI writes.

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

The compile pipeline is a port of `packages/tailwindcss` from Tailwind CSS v4.3.2. It covers the CSS parser, candidates, variants, utilities, `@apply`, `@import`, `theme()` and the compat utilities.

Tailwind's own test files are extracted into fixtures and run against it. Every compile fixture passes except two. Their expected output comes from the older Lightning CSS release that Tailwind's npm build pins.

JavaScript plugins and configs (`@plugin` and `@config`) are refused with Tailwind's error, because the crate runs no JavaScript.

File scanning, editor tooling and source maps of the generated CSS are not included. Editor tooling means class sorting, IntelliSense and canonicalising class lists.

## License

The crate is under the Mozilla Public License 2.0, in [LICENSE](LICENSE). The Tailwind CSS sources and tests it carries are under the MIT license, in [css/LICENSE-TAILWIND](css/LICENSE-TAILWIND).
