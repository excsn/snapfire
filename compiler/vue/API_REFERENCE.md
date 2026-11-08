# API Reference: snapfire_vue

Vue single-file components compiled in QuickJS, as the `snapfirec-vue` plugin and as a library.

## Contents

* [1. The Compiler](#1-the-compiler)
  * [`Compiler`](#compiler)
  * [`Outcome`](#outcome)
  * [`SERVER_RUNTIME`](#server_runtime)
  * [`fatal`](#fatal)
* [2. The Described Template](#2-the-described-template)
  * [Template](#template)
  * [Node](#node)
  * [Prop](#prop)
* [3. The Binary](#3-the-binary)
  * [`snapfirec-vue`](#snapfirec-vue)
* [4. Error Handling](#4-error-handling)
  * [`VueError`](#vueerror)

## 1. The Compiler

### Compiler

One QuickJS context holding Vue's browser build of `@vue/compiler-sfc` 3.5.44, on a thread of its own.

* `pub struct Compiler`
* `pub fn new() -> Result<Compiler, VueError>`: boots the context on a thread with a 64 MiB stack, of which the engine may use 48 MiB. Blocks until the boot has finished or failed.
* `pub fn version(&self) -> Result<String, VueError>`: the carried compiler's version, `3.5.44`.
* `pub fn compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: one component compiled for the browser.
* `filename` is the name diagnostics use and the input the scope id is hashed from, relative to the project.
* `files` holds the sibling files a block's `src` names, keyed by the specifier as written. A specifier missing from `files` is answered with `Outcome::Needs`.
* `pub fn describe(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: the component as Vue's parser reads it, as `Outcome::Described`. Refuses what `compile` refuses.
* The described `template` is the JSON in section 2.
* The described `script` is the `<script setup>` block. Without one, it is the plain block. `plain` is set when a plain block exists. `line` and `column` are where the block's content starts.
* The described `bindings` are Vue's script binding metadata, without keys starting with `__`.
* The described `scope` is the `data-v-` id when any style is scoped. `slotted` is set when a scoped style has a `:slotted()` rule.
* `pub fn ssr_module(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: the component as a module for Vue's server renderer, with `_sfc_main.ssrRender` bound in place of `render`. The module is in the script's language.
* `pub fn render_module(&self, js: &str, props: &str, children: Option<&str>, modules: &BTreeMap<String, String>) -> Result<String, VueError>`: the markup `@vue/server-renderer` writes for the module `js`, with `props` as JSON.
* `js` must be JavaScript.
* `children`, when given, fills the default slot inside `<sf-s data-sf-children>`. The component renders under a wrapper that writes nothing of its own.
* `modules` holds the source of every bare specifier the module imports, `vue` and `vue/server-renderer` among them. Each call declares the module under a name of its own.
* The content of each `<Teleport>` follows the markup as `<!--sf-placed:TARGET-->CONTENT<!--/sf-placed-->`, in the order Vue collected the targets.
* Dropping the `Compiler` ends its thread and joins it.

### Outcome

`snapfire_compiler_wire::Outcome`, as `compile` fills it.

* `Ok`: `js` is the module as `_sfc_main` with its render function, scope id and file bound. `lang` is `Ts` when a script block is `ts` or `tsx`.
* `Ok` also carries every style block as one `css` string, `deps` naming each `src` and `source_map` always `None`.
* `Failed`: the compiler's diagnostics, at least one, with the file and the line when it gave one.
* A `<script>` whose `lang` is not `js`, `ts`, `jsx` or `tsx` is refused by name. So is a `<style>` whose `lang` is not `css`.

### SERVER_RUNTIME

* `pub const SERVER_RUNTIME: &[(&str, &str)]`: each bare specifier a server-compiled component and `vue/server-renderer` import, with its file among Vue's published esm-bundler builds.
* `vue`: `vue.runtime.esm-bundler.js`.
* `vue/server-renderer`: `server-renderer.esm-bundler.js`.
* `@vue/runtime-dom`, `@vue/runtime-core`, `@vue/reactivity` and `@vue/shared`: `runtime-dom.esm-bundler.js`, `runtime-core.esm-bundler.js`, `reactivity.esm-bundler.js` and `shared.esm-bundler.js`.

### fatal

* `pub fn fatal(diagnostics: &[Diagnostic]) -> bool`: whether any diagnostic is an `Error`.

## 2. The Described Template

### Template

The `template` value of `Outcome::Described`, present when the component has a `<template>`.

* `{ children }`: the template's top-level nodes.

### Node

Each node is an object with a `node` field. Comments are dropped.

* `element`: `tag`, `kind`, `props`, `children`, `line` and `column`. `kind` is `0` for an element, `1` for a component, `2` for a slot and `3` for a `<template>`.
* `text`: `content`.
* `interpolation`: `content`, the expression's source, with `line` and `column`.
* `other`: `type`, Vue's node type number, with `line` and `column`.
* `line` and `column` are where the node starts, `0` when Vue gave no place.

### Prop

* `attribute`: `prop`, `name`, `value` (`null` without one), `line` and `column`.
* `directive`: `prop`, `name`, `arg`, `argStatic`, `exp`, `expLine`, `expColumn`, `modifiers`, `line` and `column`.
* `arg` and `exp` are `null` when absent. `argStatic` is `true` without an `arg`.
* `line` and `column` are the prop's own. `expLine` and `expColumn` are its expression's.

## 3. The Binary

### snapfirec-vue

* `snapfirec-vue`: boots a `Compiler`, writes a `Hello`, then reads `Request` lines from stdin and writes one `Response` line per request until stdin closes.
* The `Hello` has `name` `vue`, the crate version, `compiler` `@vue/compiler-sfc <version>` and the extension `.vue`.
* A `Compile` request is answered with `compile` and a `Describe` request with `describe`.
* A unit whose answer raised `VueError` is answered `Outcome::Failed` with the error as its diagnostic. The worker keeps running.
* `snapfirec-vue --version`: prints `snapfirec-vue <crate version> (@vue/compiler-sfc <version>)` and exits 0.
* Any other argument is refused on stderr with exit status 2. A request line that does not parse ends the process with status 1.

## 4. Error Handling

### VueError

* `pub enum VueError { Js(String) }`. `Debug`, `std::error::Error`.
* The plugin's own failure, never the component's: a runtime or context that would not boot, a thread that would not start or has gone and a thrown value with its message and stack.
* Displays as `the Vue compiler: <detail>`.
