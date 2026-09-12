# snapfire_vue

License: same as the workspace. Status: active.

Vue single-file components compiled for `snapfirec`, with no JavaScript runtime on the machine: the crate carries Vue's own browser build of `@vue/compiler-sfc` and runs it in QuickJS. The binary is `snapfirec-vue`, the plugin `snapfirec` finds on `PATH` for every `.vue` file; the library behind it is `Compiler`, for a tool that wants the same compile in process. The usage guide is [README.USAGE.md](README.USAGE.md) and the surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install snapfire_vue
```

That puts `snapfirec-vue` on `PATH`, which is all `snapfirec` needs. As a library:

```toml
[dependencies]
snapfire_vue = "0.1"
```

No features.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Compile the `.vue` files of a project | `snapfirec`, with `snapfirec-vue` installed; nothing to pass |
| See which Vue compiler is carried | `snapfirec-vue --version` |
| Compile one component in process | `Compiler::new()`, then `compile` |
| Keep a typed script block typed | Nothing: a `lang="ts"` block is handed back as TypeScript for the build to strip |
| Scope a component's styles | `<style scoped>`; the `data-v-` attribute is in the module and the sheet |
| Put a block's content in a file beside the component | `<style src="./card.css">`; the plugin asks for the file and the build supplies it |

## Status

Carries `@vue/compiler-sfc` 3.5.13. Compiles `<script>`, `<script setup>`, `<template>` and `<style>`, in JavaScript and TypeScript for scripts and CSS for styles; a block in another language is refused by name. No source maps yet.
