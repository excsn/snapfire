# API Reference: snapfire_svelte

Svelte components compiled in QuickJS, as the `snapfirec-svelte` plugin and as a library.

## Contents

* [1. The Compiler](#1-the-compiler)
  * [`Compiler`](#compiler)
  * [`Outcome`](#outcome)
  * [`SERVER_RUNTIME`](#server_runtime)
* [2. The Described Template](#2-the-described-template)
  * [Template](#template)
  * [Fragment](#fragment)
  * [Node](#node)
  * [Attribute](#attribute)
* [3. The Binary](#3-the-binary)
  * [`snapfirec-svelte`](#snapfirec-svelte)
* [4. Error Handling](#4-error-handling)
  * [`SvelteError`](#svelteerror)

## 1. The Compiler

### Compiler

One QuickJS context holding `svelte/compiler` 5.57.2, on a thread of its own.

* `pub struct Compiler`
* `pub fn new() -> Result<Compiler, SvelteError>`: boots the context on a thread with a 64 MiB stack, of which the engine may use 48 MiB. Blocks until the boot has finished or failed.
* `pub fn version(&self) -> Result<String, SvelteError>`: the carried compiler's version, `5.57.2`.
* `pub fn compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, SvelteError>`: one component compiled for the browser with `generate: 'client'` and `css: 'external'`. Development mode unless `options.production`. `files` is never read.
* `filename` is the name diagnostics use, relative to the project.
* A `filename` ending in `.svelte.ts` or `.svelte.js` is a runes module, compiled with `compileModule` and never refused for its language. A `.svelte.ts` source must arrive with its types stripped.
* `pub fn describe(&self, filename: &str, source: &str, options: &Options) -> Result<Outcome, SvelteError>`: one component as `Outcome::Described`, read with the compiler `options.compiler` names or the carried one.
* The described `template` is the JSON in section 2. `diagnostics` is empty.
* The described `script` is the whole source with everything outside the `<script module>` and instance blocks replaced by spaces, newlines kept and byte offsets unchanged. `line` and `column` are 1. `lang` is `Ts` when either block has `lang="ts"`. `None` when the component has no script.
* `describe` answers `Outcome::Failed` for a component that does not parse, that Svelte's analysis refuses or that names a language the plugin does not carry.
* `pub fn ssr_module(&self, filename: &str, source: &str) -> Result<Outcome, SvelteError>`: the component or runes module compiled by the carried compiler with `generate: 'server'` in production mode, as `Outcome::Ok` with the module in `js`.
* `pub fn render_module(&self, entry: &str, modules: &BTreeMap<String, String>, props: &str, children: Option<&str>, runtime: &Path) -> Result<String, SvelteError>`: the body `svelte/server`'s `render` writes for the component `entry` names, with `props` as JSON.
* `modules` holds server modules keyed by an absolute path such as `/src/ui/Card.svelte`, which relative imports resolve against. Any other module is keyed by its bare specifier.
* `children`, when given, is passed as a `children` snippet that writes `<sf-s data-sf-children>` around it.
* `runtime` is a directory laid out as `SERVER_RUNTIME` names. Each render loads its modules under names of its own.
* Dropping the `Compiler` ends its thread and joins it.

### Outcome

`snapfire_compiler_wire::Outcome`, as `compile` fills it.

* `Ok`: `js` is the module and `lang` is always `Js`. `css` is the style block's rules when there is one. `deps` is empty. `source_map` is Svelte's map as JSON when `options.source_map` is set.
* `Ok` diagnostics are Svelte's warnings as `Warning`, with the file, the line and the 1-based column.
* `Failed`: at least one `Error` diagnostic. It is Svelte's own error with its line and column when Svelte raised one.
* `Failed` otherwise refuses a `<script>` whose `lang` is not `ts` or `js` or a `<style>` whose `lang` is not `css`. The diagnostic gives the line of the opening tag.
* `compile` never answers `Needs` or `Described`.

### SERVER_RUNTIME

* `pub const SERVER_RUNTIME: &[(&str, &str)]`: each bare specifier a server module and `svelte/server` import, with its path under the runtime directory.
* Specifiers: `svelte`, `svelte/server`, `svelte/internal/server`, `svelte/internal/flags/legacy`, `#client/constants`, `svelte/store`, `svelte/motion`, `svelte/internal/client`, `svelte/transition`, `svelte/easing`, `svelte/reactivity`, `esm-env`, `esm-env/browser`, `esm-env/development`, `esm-env/node`, `clsx` and `devalue`.
* Paths are into `svelte`, `esm-env`, `clsx` and `devalue` as published, from the `svelte@5.57.2` tag.
* `svelte` and `svelte/reactivity` map to their server entries. `svelte/store` maps to the stores the server entry re-exports, without `toStore` and `fromStore`.
* `esm-env/browser`, `esm-env/development` and `esm-env/node` all map to `esm-env/false.js`.

## 2. The Described Template

### Template

The `template` value of `Outcome::Described`.

* `runes`: whether Svelte compiles the component in runes mode.
* `scoping`: `read` when the elements a `<style>` scopes were read from Svelte's server compiler, `unread` when they could not be.
* `instance` and `module`: each script block's content as a UTF-8 byte range `[start, end]`. `null` without that block.
* `fragment`: the root fragment.

### Fragment

`{ hoisted, nodes, standalone, text_first }`.

* `hoisted`: the snippets, `{@const}`, `{@debug}` and declaration tags, `<svelte:head>`, `<svelte:body>`, `<svelte:window>`, `<svelte:document>` and `<title>`.
* `nodes`: every other node, comments dropped.
* Outside `<pre>` and `<textarea>`, whitespace at the start and end of `nodes` is trimmed and whitespace at either edge of a text node becomes one space. A text node of one space is dropped inside `select`, `tr`, `table`, `tbody`, `thead`, `tfoot`, `colgroup` and `datalist` and inside SVG outside `<text>`.
* A leading newline in `<pre>` is dropped.
* `standalone`: the fragment is one static component or one static render tag, which takes no anchor after it. Always `false` for an element's content.
* `text_first`: the fragment of a component, a snippet, an each body or the root starts with text or an expression, which takes an anchor before it.

### Node

Each node is an object with a `type`. Every type except `text` and `comment` carries `at`.

* An expression is `{ code, at }`: its source and its 1-based line and column.
* `text`: `data`.
* `comment`: `data`. Only appears after a lone `<script>` element.
* `expr` and `html`: `code` and `at`, for `{expr}` and `{@html expr}`.
* `const`: `id` and `init`, each an expression.
* `declare`: `items`, each `{ id, init }`.
* `render`: `callee`, `args`, `optional` and `dynamic`.
* `element`: `name`, `svg`, `mathml`, `scoped`, `attrs` and `fragment`. `scoped` is the `svelte-` class Svelte's CSS gives the element. `null` when it has none.
* `component`: `name`, `dynamic`, `svg`, `attrs`, `fragment` and `slots`. `svg` is whether the component sits in SVG. `slots` lists its children grouped by slot name as `{ name, fragment }`, snippets left out, ungrouped children under `default`.
* `slot`: `attrs` and `fragment`.
* `if`: `elseif`, `test`, `consequent` and `alternate`.
* `each`: `expression`, `context`, `index`, `key`, `body` and `fallback`.
* `snippet`: `name`, `params` and `body`.
* `key`: `expression` and `fragment`.
* `await`: `expression`, `value`, `error`, `pending`, `then` and `catch`.
* `special`: `kind`, `name`, `scoped`, `attrs`, `this` and `fragment`, plus `slots` for `svelte:component` and `svelte:self`. `kind` is Svelte's node type for a `svelte:` element or `<title>`.
* `other`: `kind`, Svelte's node type.
* `dynamic` on a component: in runes mode, its name binds to anything but an import, a function, a snippet or a plain declaration. A bound dotted name is dynamic too.
* `dynamic` on a render tag: its callee is not a plain name bound to an import, a function, a snippet or a plain declaration.

### Attribute

* `attr`: `name`, `value` and `at`. `value` is `true` or a list of `{ text }` and expressions.
* `spread` and `attach`: an expression.
* `directive`: `kind`, `name`, `modifiers`, `expression` and `at`. A `style:` directive carries `value`, shaped as on `attr`, in place of `expression`.

## 3. The Binary

### snapfirec-svelte

* `snapfirec-svelte`: boots a `Compiler`, writes a `Hello`, then reads `Request` lines from stdin and writes one `Response` line per request until stdin closes.
* The `Hello` has `name` `svelte`, the crate version, `compiler` `svelte <version>` and the extensions `.svelte`, `.svelte.ts` and `.svelte.js`.
* A `Compile` request is answered with `compile` and a `Describe` request with `describe`.
* A unit whose answer raised `SvelteError` is answered `Outcome::Failed` with the error as its diagnostic. The worker keeps running.
* `snapfirec-svelte --version`: prints `snapfirec-svelte <crate version> (svelte <version>)` and exits 0.
* Any other argument is refused on stderr with exit status 2. A request line that does not parse ends the process with status 1.

## 4. Error Handling

### SvelteError

* `pub enum SvelteError { Js(String) }`. `Debug`, `std::error::Error`.
* The plugin's own failure, never the component's: a runtime or context that would not boot, a thread that would not start or has gone and a thrown value with its message and stack.
* Displays as `the Svelte compiler: <detail>`.
