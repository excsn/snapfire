# snapfire_svelte

[![Crates.io](https://img.shields.io/crates/v/snapfire_svelte.svg)](https://crates.io/crates/snapfire_svelte)
[![Docs.rs](https://docs.rs/snapfire_svelte/badge.svg)](https://docs.rs/snapfire_svelte)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Compiles Svelte components for `snapfirec` without Node.js or any JavaScript runtime on the machine. The crate carries Svelte's own compiler (`svelte/compiler` 5.57.2) and runs it in QuickJS.

Install the `snapfirec-svelte` binary and `snapfirec` compiles every `.svelte` file it finds. You never call it yourself. The same compile is available in process through the `Compiler` type, for a tool that wants it without the binary.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

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

| Problem | Piece |
| --- | --- |
| Compile the `.svelte` files of a project | `snapfirec`, with `snapfirec-svelte` installed. Nothing to pass |
| See which Svelte compiler is carried | `snapfirec-svelte --version` |
| Compile one component in process | `Compiler::new()`, then `compile` |
| Write a component with runes | `$props`, `$state` and `$derived`, compiled as written |
| Keep shared state in a runes module | `name.svelte.ts` or `name.svelte.js`. `snapfirec` strips a `.svelte.ts` file's types before Svelte compiles it |
| Keep a Svelte 4 component | `export let`, `$:` and `on:` compile unchanged in legacy mode |
| Type a component's script | `<script lang="ts">`. Svelte strips the types and the module comes back as JavaScript |
| Scope a component's styles | A `<style>` block. Svelte scopes it by default and puts the class in both the module and the stylesheet |
| Let `fsr build` render a component on the server | `describe`, which hands back the component's scripts and its template as JSON |

## Status

Compiles components for the browser. It also describes a component's template and scripts so `fsr build` can render the component on the server in Rust.

`ssr_module` and `render_module` run Svelte's own server renderer in QuickJS. Tests compare the markup FSR renders against what they write.

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](../LICENSE) file for the full license text. The carried compiler in `vendor/compiler.js` is Svelte's, under the MIT license in [vendor/LICENSE.md](vendor/LICENSE.md).
