# snapfire_vue

[![Crates.io](https://img.shields.io/crates/v/snapfire_vue.svg)](https://crates.io/crates/snapfire_vue)
[![Docs.rs](https://docs.rs/snapfire_vue/badge.svg)](https://docs.rs/snapfire_vue)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Compiles Vue single-file components for `snapfirec` without Node.js or any JavaScript runtime on the machine. The crate carries Vue's own browser build of `@vue/compiler-sfc` and runs it in QuickJS.

Install the `snapfirec-vue` binary and `snapfirec` compiles every `.vue` file it finds. You never call it yourself. The same compile is available in process through the `Compiler` type, for a tool that wants it without the binary.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install snapfire_vue
```

That puts `snapfirec-vue` on `PATH`, which is all `snapfirec` needs. As a library:

```toml
[dependencies]
snapfire_vue = "0"
```

No features.

## What to reach for

| Problem | Piece |
| --- | --- |
| Compile the `.vue` files of a project | `snapfirec`, with `snapfirec-vue` installed. Nothing to pass |
| See which Vue compiler is carried | `snapfirec-vue --version` |
| Compile one component in process | `Compiler::new()`, then `compile` |
| Write a component's script in TypeScript | `lang="ts"`. The block comes back as TypeScript and the build strips the types |
| Scope a component's styles | `<style scoped>`. The `data-v-` attribute is in both the module and the stylesheet |
| Put a block's content in a file beside the component | `<style src="./card.css">`. The plugin asks for the file and the build supplies it |
| Read a component's template and script without compiling it | `Compiler::describe` in process. Over the wire, a `Request` with `kind` `describe`. `fsr build` uses it to render Vue components on the server |
| Render a component with Vue's own server renderer | `Compiler::ssr_module` then `Compiler::render_module`, given the runtime as modules |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](../LICENSE) file for the full license text.
