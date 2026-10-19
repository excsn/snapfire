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
* `compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, SvelteError>`: one component, compiled for the browser with `generate: 'client'` and `css: 'external'`, in development mode unless `options.production`. `filename` is what a diagnostic names, so relative to the project. `files` is never read. A `filename` ending in `.svelte.ts` or `.svelte.js` is a runes module, compiled with `compileModule` and never refused for its language; a `.svelte.ts` source must arrive with its types stripped, which `snapfirec` does before sending it.
* `describe(&self, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError>`: one component's scripts for a host that follows their imports, read with the compiler `options.compiler` names or the carried one. The answer is `Outcome::Described` with no template, one `Error` diagnostic saying the plugin does not describe a component yet and `script` holding the source with everything outside the `<script module>` and instance blocks blanked to spaces, so `line` and `column` are 1 and every import stands at its own line and column. `lang` is `Ts` when either block says `lang="ts"`. A component with no script has `script` `None`. A component that does not parse, that Svelte's analysis refuses or that names a language the plugin does not carry is answered `Outcome::Failed` as a compile would be.
* The answer's `template` is JSON: `runes`, whether Svelte compiles the component in runes mode; `instance` and `module`, each script's content as a UTF-8 byte range or `null`; `fragment`, the template. Every fragment is `{ hoisted, nodes, standalone, text_first }`: `hoisted` holds the snippets, `{@const}` tags and the other nodes Svelte's server compiler visits first; `nodes` the rest with comments dropped and whitespace trimmed and joined as Svelte's `clean_nodes` does; `standalone` whether the fragment is one static component or one static render tag, which then takes no anchor after it, never for an element's content; `text_first` whether a component's, a snippet's, an each body's or the root's fragment starts with text, which then takes an anchor before it. A node's `type` is `text` (`data`), `expr` and `html` (`code`, `at`), `const` (`id`, `init`), `render` (`callee`, `args`, `optional`, `dynamic`), `element` (`name`, `svg`, `attrs`, `fragment`), `component` (`name`, `dynamic`, `attrs`, `fragment`), `slot`, `if` (`elseif`, `test`, `consequent`, `alternate`), `each` (`expression`, `context`, `index`, `key`, `body`, `fallback`), `snippet` (`name`, `params`, `body`) or `other` (`kind`, Svelte's node type). An expression is `{ code, at }`, its source and its 1-based line and column. An attribute is `attr` (`name`, `value`, `true` or a list of `{ text }` and expressions), `spread`, `attach` or `directive` (`kind`, `name`, `modifiers` and an `expression`; `style:` carries a `value` in place of the expression). `dynamic` follows Svelte's analysis: a component is dynamic in runes mode when its name binds to anything but an import, a function, a snippet or a plain declaration, a render tag when its callee does.
* Dropping the `Compiler` ends its thread and joins it.
* `Outcome::Ok` carries the module in `js`, `lang` always `Js`, the style block's rules as `css` or `None`, `deps` empty, `source_map` Svelte's map as JSON when `options.source_map` and `None` otherwise and Svelte's warnings as `Warning` diagnostics with the file, the line and the 1-based column. `Outcome::Failed` carries at least one `Error` diagnostic. It is Svelte's own with its line and column. Otherwise it is a refusal naming a `<script>` in a language other than `ts` or `js` or a `<style>` in a language other than `css`, with the line of its opening tag. A compile never answers `Needs` or `Described`.

## 2. The Binary

### snapfirec-svelte

* `snapfirec-svelte`: boots a `Compiler`, writes a `Hello` with `name` `svelte`, the crate version, `svelte <version>` and the extensions `.svelte`, `.svelte.ts` and `.svelte.js`, then reads `Request` lines from stdin and writes one `Response` line per request until stdin closes. A `Compile` request is answered with `compile`. A `Describe` request is answered with `describe`. A unit whose answer raised `SvelteError` is answered `Outcome::Failed` with the error as its diagnostic; the worker goes on.
* `snapfirec-svelte --version`: prints `snapfirec-svelte <crate version> (svelte <version>)` and exits 0.
* Any other argument is refused on stderr with exit status 2. A request line that does not parse ends the process with status 1.

## 3. Error Handling

### SvelteError

* `pub enum SvelteError { Js(String) }`
* The plugin's own failure, never the component's: a runtime or context that would not boot, a thread that would not start or has gone or a thrown value with its message and stack. `Display` is `the Svelte compiler: <detail>`.
