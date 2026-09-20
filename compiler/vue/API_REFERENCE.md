# API Reference: snapfire_vue

Vue single-file components compiled in QuickJS, as the `snapfirec-vue` plugin and as a library.

## Contents

* [1. The Compiler](#1-the-compiler)
  * [Compiler](#compiler)
* [2. The Binary](#2-the-binary)
  * [snapfirec-vue](#snapfirec-vue)
* [3. Error Handling](#3-error-handling)
  * [VueError](#vueerror)
  * [fatal](#fatal)

## 1. The Compiler

### Compiler

* `pub struct Compiler`
* `Compiler::new() -> Result<Compiler, VueError>`: boots one QuickJS context holding `@vue/compiler-sfc` and the driver, on a thread of its own with a 64 MiB stack of which the engine may use 48 MiB. Expensive once; blocks until the boot has finished or failed.
* `version(&self) -> Result<String, VueError>`: what the carried compiler reports itself as, `3.5.13`.
* `compile(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: one component. `filename` is what a diagnostic names and what the scope id hashes, so relative to the project. `files` are the sibling files a block's `src` names, by the specifier as written; a specifier the block names that `files` lacks is answered with `Outcome::Needs`.
* `describe(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: the component as Vue's parser reads it, `Outcome::Described`, with the refusals `compile` has. `template` is the tree `describeNode` in the driver writes, see the usage guide; `script` is the setup block or, when there is none, the plain one, with `plain` set when a plain block exists; `bindings` is `compileScript`'s binding metadata less its own `__` keys; `scope` is the `data-v-` id when any style is scoped.
* `ssr_module(&self, filename: &str, source: &str, options: &Options, files: &BTreeMap<String, String>) -> Result<Outcome, VueError>`: the component as a module for Vue's server renderer, `_sfc_main.ssrRender` bound in place of `render`, in the script's language.
* `render_module(&self, js: &str, props: &str, children: Option<&str>, modules: &BTreeMap<String, String>) -> Result<String, VueError>`: the markup `@vue/server-renderer` writes for the module `js`, which must be JavaScript, with `props` as JSON and `children` as markup for the default slot inside an `<sf-s data-sf-children>` region, under the root the client's Vue mounter uses. `modules` are the sources of every bare specifier the module imports, `vue` and `vue/server-renderer` among them, since the plugin carries no runtime. Each call declares the module under a name of its own.
* Dropping the `Compiler` ends its thread and joins it.
* `Outcome::Ok` carries the module as `_sfc_main` with its render function, scope id and file bound, `lang` `Ts` when a script block was typed, every style block as one `css` string, `deps` naming each `src` and `source_map` always `None`. `Outcome::Failed` carries the compiler's diagnostics, at least one, with the file and the line when it gave one. A script block in a language other than `js`, `ts`, `jsx` or `tsx` is refused by name; so is a style block in a language other than `css`.

## 2. The Binary

### snapfirec-vue

* `snapfirec-vue`: boots a `Compiler`, writes a `Hello` with `name` `vue`, the crate version, `@vue/compiler-sfc <version>` and `.vue`, then reads `Request` lines from stdin and writes one `Response` line per request until stdin closes, answering a `Compile` request with `compile` and a `Describe` request with `describe`. A unit whose answer raised `VueError` is answered `Outcome::Failed` with the error as its diagnostic; the worker goes on.
* `snapfirec-vue --version`: prints `snapfirec-vue <crate version> (@vue/compiler-sfc <version>)` and exits 0.
* Any other argument is refused on stderr with exit status 2. A request line that does not parse ends the process with status 1.

## 3. Error Handling

### VueError

* `pub enum VueError { Js(String) }`
* The plugin's own failure, never the component's: a runtime or context that would not boot, a thread that would not start or has gone or a thrown value with its message and stack. `Display` is `the Vue compiler: <detail>`.

### fatal

* `pub fn fatal(diagnostics: &[Diagnostic]) -> bool`
* Whether any diagnostic is an `Error`, which is what stops a build.
