# snapfire_svelte

[![Crates.io](https://img.shields.io/crates/v/snapfire_svelte.svg)](https://crates.io/crates/snapfire_svelte)
[![Docs.rs](https://docs.rs/snapfire_svelte/badge.svg)](https://docs.rs/snapfire_svelte)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Svelte components compiled for `snapfirec`, with no JavaScript runtime on the machine: the crate carries Svelte's own compiler (`svelte/compiler` 5.57.2) and runs it in QuickJS. The binary is `snapfirec-svelte`, the plugin `snapfirec` finds on `PATH` for every `.svelte` file; the library behind it is `Compiler`, for a tool that wants the same compile in process. The usage guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install snapfire_svelte
```

That puts `snapfirec-svelte` on `PATH`, which is all `snapfirec` needs. As a library:

```toml
[dependencies]
snapfire_svelte = "0"
```

No features.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Compile the `.svelte` files of a project | `snapfirec`, with `snapfirec-svelte` installed; nothing to pass |
| See which Svelte compiler is carried | `snapfirec-svelte --version` |
| Compile one component in process | `Compiler::new()`, then `compile` |
| Write a component with runes | `$props`, `$state`, `$derived`; compiled as written |
| Keep shared state in a runes module | `name.svelte.ts` or `name.svelte.js`; compiled with `compileModule`, a `.svelte.ts` stripped of its types by `snapfirec` first |
| Keep a Svelte 4 component | `export let`, `$:` and `on:` compile unchanged in legacy mode |
| Type a component's script | `<script lang="ts">`; Svelte strips the types, so the module comes back as JavaScript |
| Scope a component's styles | a `<style>` block; scoping is Svelte's default and the class is in the module and the sheet |
| Read a component rather than compile it | Not available yet: a `describe` request is refused |

## Status

Compiles for the browser. Describing a component for `fsr build` to lower is not built. Neither is rendering one on the server.

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text. The carried compiler in `vendor/compiler.js` is Svelte's, under the MIT license in `vendor/LICENSE.md`.
