# API Reference: snapfire_svelte

Svelte components compiled in QuickJS, as the `snapfirec-svelte` plugin and as a library.

## Contents

* [1. The Compiler](#1-the-compiler)
  * [Compiler](#compiler)
* [2. The Binary](#2-the-binary)
  * [snapfirec-svelte](#snapfirec-svelte)
* [3. Error Handling](#3-error-handling)
  * [SvelteError](#svelteerror)

## 1. The Compiler

### Compiler

* `pub struct Compiler`
* `Compiler::new() -> Result<Compiler, SvelteError>`: boots one QuickJS context holding `svelte/compiler` 5.57.2 and the driver, on a thread of its own with a 64 MiB stack of which the engine may use 48 MiB. Expensive once; blocks until the boot has finished or failed.
* `version(&self) -> Result<String, SvelteError>`: what the carried compiler reports itself as, `5.57.2`.
* `compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, SvelteError>`: one component, compiled for the browser with `generate: 'client'` and `css: 'external'`, in development mode unless `options.production`. `filename` is what a diagnostic names, so relative to the project. `files` is never read.
* Dropping the `Compiler` ends its thread and joins it.
* `Outcome::Ok` carries the module in `js`, `lang` always `Js`, the style block's rules as `css` or `None`, `deps` empty, `source_map` `None` and Svelte's warnings as `Warning` diagnostics with the file, the line and the 1-based column. `Outcome::Failed` carries at least one `Error` diagnostic. It is Svelte's own with its line and column. Otherwise it is a refusal naming a `<script>` in a language other than `ts` or `js` or a `<style>` in a language other than `css`, with the line of its opening tag. A compile never answers `Needs` or `Described`.

## 2. The Binary

### snapfirec-svelte

* `snapfirec-svelte`: boots a `Compiler`, writes a `Hello` with `name` `svelte`, the crate version, `svelte <version>` and `.svelte`, then reads `Request` lines from stdin and writes one `Response` line per request until stdin closes. A `Compile` request is answered with `compile`. A `Describe` request is answered `Outcome::Failed`, one diagnostic per unit saying the plugin does not describe a component. A unit whose answer raised `SvelteError` is answered `Outcome::Failed` with the error as its diagnostic; the worker goes on.
* `snapfirec-svelte --version`: prints `snapfirec-svelte <crate version> (svelte <version>)` and exits 0.
* Any other argument is refused on stderr with exit status 2. A request line that does not parse ends the process with status 1.

## 3. Error Handling

### SvelteError

* `pub enum SvelteError { Js(String) }`
* The plugin's own failure, never the component's: a runtime or context that would not boot, a thread that would not start or has gone or a thrown value with its message and stack. `Display` is `the Svelte compiler: <detail>`.
