# Usage Guide: snapfire_vue

Compiling Vue single-file components through `snapfirec` and the same compile from Rust.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Compiling Through snapfirec](#compiling-through-snapfirec)
* [Compiling in Process](#compiling-in-process)
* [Writing a Component It Reads](#writing-a-component-it-reads)
* [Linking a Block to a File](#linking-a-block-to-a-file)
* [Reading What Comes Out](#reading-what-comes-out)
* [Error Handling](#error-handling)

## Core Concepts

* **Single-file component**: a `.vue` file with `<script>`, `<template>` and `<style>` blocks.
* **The carried compiler**: Vue's browser build of `@vue/compiler-sfc`, embedded in the crate and run in QuickJS. Nothing is fetched and no Node is asked of the machine.
* **Worker**: `snapfirec-vue` as `snapfirec` runs it, one process per build kept for its length, answering batches over stdin and stdout.
* **Compiler**: the library's handle to one QuickJS context, on a thread of its own with room for the parser's recursion.
* **Scope id**: `data-v-<hash>`, derived from the file's name and content, written on the rendered elements and into every scoped rule.
* **Sibling file**: the file a block's `src` names, which the plugin asks for and never opens itself.
* **Outcome**: what one component became: compiled, failed with diagnostics or in need of files.

## Quick Start

Through `snapfirec`, which is the usual way:

```sh
cargo install snapfire_vue
snapfirec --root . --import-map importmap.json
```

```text
   Plugin:   snapfirec-vue 0.1.0 (@vue/compiler-sfc 3.5.13)
   Compiling VUE: "src/Card.vue"
   Compiling CSS: "src/Card.vue"
```

In process:

```rust
use snapfire_compiler_wire::{Options, Outcome};
use snapfire_vue::Compiler;

let compiler = Compiler::new()?;
let source = std::fs::read_to_string("src/Card.vue")?;
match compiler.compile("src/Card.vue", &source, &Options::default(), &Default::default())? {
  Outcome::Ok(compiled) => println!("{}", compiled.js),
  Outcome::Failed { diagnostics } => eprintln!("{diagnostics:?}"),
  Outcome::Needs { files } => eprintln!("send {files:?} first"),
}
```

## Compiling Through snapfirec

Nothing is configured. `snapfirec` finds `snapfirec-vue` on `PATH` when a project holds a `.vue` file, hands every such file to it in one batch and writes what comes back beside the other outputs. What to check when it does not:

```sh
snapfirec-vue --version
```

```text
snapfirec-vue 0.1.0 (@vue/compiler-sfc 3.5.13)
```

The binary is not run by hand otherwise; it reads batches from stdin and an argument other than `--version` is refused.

## Compiling in Process

`Compiler::new` boots QuickJS with the compiler in it, which is the expensive step; `compile` is cheap after it and the same `Compiler` answers as many components as you have.

```rust
let compiler = Compiler::new()?;
for name in ["Card", "List", "Row"] {
  let source = std::fs::read_to_string(format!("src/{name}.vue"))?;
  let outcome = compiler.compile(&format!("src/{name}.vue"), &source, &Options::default(), &Default::default())?;
}
```

`filename` is what a diagnostic names and what the scope id hashes, so pass it relative to the project rather than absolute. `version()` is what the carried compiler reports itself as.

## Writing a Component It Reads

Scripts in JavaScript or TypeScript, styles in CSS:

```vue
<script setup lang="ts">
import { ref } from "vue";
const props = defineProps<{ title: string }>();
const n = ref(0);
</script>

<template>
  <div class="card">
    <h2>{{ title }}</h2>
    <button @click="n++">{{ n }}</button>
  </div>
</template>

<style scoped>
.card { padding: 12px; }
</style>
```

A `lang` the plugin does not carry, `scss` or `pug`, is refused with the block named rather than emitted wrong. A `defineProps<T>()` whose `T` is declared in the file is fine; one imported from another file is not resolved.

## Linking a Block to a File

A block may name its content by file. The plugin does not open it: the first compile answers `Needs`, the caller reads the file and compiles again with it in `files`, keyed by the specifier as written.

```rust
use std::collections::BTreeMap;

let first = compiler.compile("src/Card.vue", &source, &Options::default(), &Default::default())?;
let files: BTreeMap<String, String> = match first {
  Outcome::Needs { files } => files.into_iter().map(|f| (f.clone(), std::fs::read_to_string(format!("src/{f}")).unwrap())).collect(),
  other => return Ok(other),
};
let compiled = compiler.compile("src/Card.vue", &source, &Options::default(), &files)?;
```

Under `snapfirec` this round trip is the build's; the file becomes a dependency the watcher follows.

## Reading What Comes Out

`Compiled::js` is one module: the script block's output with the component as `_sfc_main`, the template's render function bound to it, the scope id and the file name on it and `export default _sfc_main`. Its imports are the source's, plus `vue` for the render helpers, left for the build to resolve. `lang` is `Ts` when a script block was typed, so the build strips the types. `css` is every style block as one string, scoped rules carrying `[data-v-…]`. `deps` names every `src` the blocks pointed at.

```rust
let Outcome::Ok(compiled) = outcome else { unreachable!() };
assert!(compiled.js.contains("export default _sfc_main"));
assert!(compiled.css.as_deref().unwrap_or("").contains("[data-v-"));
```

## Error Handling

`VueError` is the crate's one error and it means the plugin, not the component: a compiler that would not boot, a thread that died or a thrown value with its stack. A component that does not compile is `Outcome::Failed` with the compiler's own diagnostics, each with the file and the line.

```rust
match compiler.compile("src/Card.vue", &source, &Options::default(), &Default::default()) {
  Ok(Outcome::Ok(compiled)) => emit(compiled),
  Ok(Outcome::Failed { diagnostics }) => report(diagnostics),
  Ok(Outcome::Needs { files }) => fetch_and_retry(files),
  Err(snapfire_vue::VueError::Js(why)) => eprintln!("the plugin itself: {why}"),
}
```

The worker turns a `VueError` into an `Outcome::Failed` for that unit and goes on, so one thrown value never takes the batch down.
