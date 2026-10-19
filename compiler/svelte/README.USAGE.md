# Usage Guide: snapfire_svelte

Compiling Svelte components through `snapfirec` and the same compile from Rust.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Compiling Through snapfirec](#compiling-through-snapfirec)
* [Compiling in Process](#compiling-in-process)
* [Writing a Component It Reads](#writing-a-component-it-reads)
  * [Runes](#runes)
  * [Legacy Mode](#legacy-mode)
  * [Languages](#languages)
* [Writing a Runes Module](#writing-a-runes-module)
* [Reading What Comes Out](#reading-what-comes-out)
* [Error Handling](#error-handling)

## Core Concepts

* **Component**: a `.svelte` file with markup, an optional `<script>` and an optional `<style>`.
* **The carried compiler**: Svelte's `compiler/index.js` from the 5.57.2 package, one bundle that imports nothing, embedded in the crate and run in QuickJS. Nothing is fetched and no Node is asked of the machine.
* **Worker**: `snapfirec-svelte` as `snapfirec` runs it, one process per build kept for its length, answering batches over stdin and stdout.
* **Compiler**: the library's handle to one QuickJS context, on a thread of its own with room for the parser's recursion.
* **Runes mode**: Svelte 5's `$props`, `$state` and `$derived`.
* **Legacy mode**: a Svelte 4 component, `export let` and `$:`, which Svelte 5 still compiles.
* **Scope class**: `svelte-<hash>`, which Svelte adds to the component's elements and to every rule in its style block.
* **Outcome**: what one component became: compiled or failed with diagnostics.

## Quick Start

Through `snapfirec`, which is the usual way:

```sh
cargo install snapfire_svelte
snapfirec --root . --import-map importmap.json
```

```text
   Plugin:   snapfirec-svelte 0.1.0 (svelte 5.57.2)
   Compiling SVELTE: "src/Card.svelte"
   Compiling CSS: "src/Card.svelte"
```

In process:

```rust
use snapfire_compiler_wire::{Options, Outcome};
use snapfire_svelte::Compiler;

let compiler = Compiler::new()?;
let source = std::fs::read_to_string("src/Card.svelte")?;
match compiler.compile("src/Card.svelte", &source, &Options::default(), &Default::default())? {
  Outcome::Ok(compiled) => println!("{}", compiled.js),
  Outcome::Failed { diagnostics } => eprintln!("{diagnostics:?}"),
  _ => unreachable!("a compile answers Ok or Failed"),
}
```

## Compiling Through snapfirec

Nothing is configured. `snapfirec` finds `snapfirec-svelte` on `PATH` when a project holds a `.svelte` file, hands every such file to it in one batch and writes what comes back beside the other outputs:

```text
dist/src/Card.js            the component module
dist/src/Card.svelte.css    its scoped stylesheet
```

An import of the component names the module it became:

```ts
import Card from "./Card.svelte";
```

```js
import Card from "./Card.js";
```

What to check when it does not:

```sh
snapfirec-svelte --version
```

```text
snapfirec-svelte 0.1.0 (svelte 5.57.2)
```

A build that finds a `.svelte` file and no `snapfirec-svelte` stops and names the binary, `cargo install snapfire_svelte` and the file that wanted it.

## Compiling in Process

`Compiler::new` boots the context once. Every compile after it is a call into that context, so one `Compiler` serves a whole build:

```rust
let compiler = Compiler::new()?;
for path in ["src/Card.svelte", "src/List.svelte"] {
  let source = std::fs::read_to_string(path)?;
  let outcome = compiler.compile(path, &source, &Options::default(), &Default::default())?;
}
```

`Options::production` leaves out what Svelte's development build carries for the developer, the file name the runtime's warnings print among it:

```rust
let options = Options { production: true, ..Options::default() };
let outcome = compiler.compile("src/Card.svelte", &source, &options, &Default::default())?;
```

## Writing a Component It Reads

### Runes

```svelte
<script lang="ts">
  let { title, count }: { title: string; count: number } = $props();
  let n = $state(count);
  const doubled = $derived(n * 2);
</script>

<div class="card">
  <h2>{title}</h2>
  <button onclick={() => n++}>{n} / {doubled}</button>
</div>

<style>
  .card { padding: 12px; }
</style>
```

### Legacy Mode

A Svelte 4 component compiles unchanged:

```svelte
<script>
  export let title;
  export let count = 0;
  $: doubled = count * 2;
</script>

<button on:click={() => count++}>{title}: {count} / {doubled}</button>
```

### Languages

`<script lang="ts">` and `<script lang="js">` are compiled; Svelte strips the types itself. `<style>` is CSS. A block in any other language is refused by name rather than handed to a compiler that would misread it:

```svelte
<style lang="scss">
  $c: red;
</style>
```

The refusal is an `Error` diagnostic whose message is `style lang="scss" needs a compiler this plugin does not carry`, with the line of the opening tag.

## Writing a Runes Module

Shared `$state` lives in a module named `name.svelte.ts` or `name.svelte.js`, which `snapfirec` hands to this plugin rather than compiling it as plain TypeScript:

```ts
// src/cart.svelte.ts
export const cart: { count: number } = $state({ count: 0 });

export function add(n: number = 1) {
  cart.count += n;
}
```

```svelte
<script lang="ts">
  import { cart, add } from "./cart.svelte.js";
</script>

<button onclick={() => add()}>{cart.count}</button>
```

The module becomes `cart.svelte.js` beside the others. Svelte's `compileModule` reads no TypeScript, so `snapfirec` strips a `.svelte.ts` file's types before sending it and the plugin compiles JavaScript. In process, hand `compile` the stripped source under the `.svelte.ts` name:

```rust
let outcome = compiler.compile("src/cart.svelte.ts", "export const cart = $state({ count: 0 });\n", &Options::default(), &Default::default())?;
```

## Reading What Comes Out

```rust
if let Outcome::Ok(compiled) = compiler.compile("src/Card.svelte", &source, &Options::default(), &Default::default())? {
  assert_eq!(compiled.lang, snapfire_compiler_wire::Lang::Js);
  let module = compiled.js;
  let sheet = compiled.css;
  let warnings = compiled.diagnostics;
}
```

* `js` imports Svelte's client runtime, `svelte/internal/client`, as a bare specifier for the import map to resolve.
* `css` is `None` for a component with no style block.
* `diagnostics` holds Svelte's warnings, an `<img>` with no `alt` among them; a warning never stops the compile.
* `deps` is always empty, since a component reads no other file.
* `source_map` is Svelte's map as JSON when `Options::source_map` is set, `None` otherwise.

## Error Handling

A component that does not compile is an `Outcome::Failed`, with Svelte's message, the file and the line:

```rust
match compiler.compile("src/Card.svelte", "<div>{#if open}</div>\n", &Options::default(), &Default::default())? {
  Outcome::Failed { diagnostics } => {
    let first = &diagnostics[0];
    eprintln!("{}:{}: {}", first.file.as_deref().unwrap_or("?"), first.line.unwrap_or(0), first.message);
  }
  other => println!("{other:?}"),
}
```

`SvelteError` is the plugin's own failure, never the component's: a context that would not boot or a compiler thread that has gone.

```rust
use snapfire_svelte::{Compiler, SvelteError};

match Compiler::new() {
  Ok(compiler) => {}
  Err(SvelteError::Js(detail)) => eprintln!("the plugin could not start: {detail}"),
}
```
