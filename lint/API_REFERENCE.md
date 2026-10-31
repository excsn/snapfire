# API Reference: snapfire_lint

A linter for TypeScript, JavaScript, Vue, Svelte, Markdown, MDX and Tera with ESLint's rules, levels and suppression comments.

## Contents

* [1. Constants](#1-constants)
  * [CONFIG_FILE](#config_file)
  * [PARSE](#parse)
  * [UNUSED_DISABLE](#unused_disable)
  * [COMPILER](#compiler)
* [2. Configuration](#2-configuration)
  * [Config](#config)
  * [The File](#the-file)
* [3. Linting](#3-linting)
  * [Linter](#linter)
  * [Level](#level)
  * [Fixed](#fixed)
* [4. Front Ends](#4-front-ends)
  * [Scripts](#scripts)
  * [Vue and Svelte](#vue-and-svelte)
  * [Compilers](#compilers)
  * [Plugins](#plugins)
  * [Markdown and MDX](#markdown-and-mdx)
  * [Tera](#tera)
* [5. The Report](#5-the-report)
  * [Report](#report)
  * [Finding](#finding)
  * [Suppressed](#suppressed)
* [6. Rules](#6-rules)
  * [rules](#rules)
  * [RuleInfo](#ruleinfo)
  * [Suppression Comments](#suppression-comments)
* [7. Checks](#7-checks)
  * [Check](#check)
  * [File](#file)
* [8. ESLint Configurations](#8-eslint-configurations)
  * [Translation](#translation)
  * [translate_eslintrc](#translate_eslintrc)
  * [translate_flat](#translate_flat)
* [9. Error Handling](#9-error-handling)
  * [ConfigError](#configerror)

## 1. Constants

### CONFIG_FILE

* `pub const CONFIG_FILE: &str = "fsr.lint.toml"`: the file a project's levels live in, at its root.

### PARSE

* `pub const PARSE: &str = "lint/parse"`: the rule of a file that does not parse. Default `error`. No other rule runs on the file and no comment silences it.

### UNUSED_DISABLE

* `pub const UNUSED_DISABLE: &str = "lint/unused-disable"`: the rule of a suppression comment that silences nothing. Default `warn`. Only a comment whose every named rule this linter has is reported.

### COMPILER

* `pub const COMPILER: &str = "lint/compiler"`: the rule of a `.vue` or `.svelte` file whose compiler could not be asked. Default `warn`. Its scripts are still linted.

## 2. Configuration

### Config

The layers of levels, the ignored globs and the settings. `Default` is no layers, `inherit = true`, no ignores and no settings, which runs every rule at its default level. `Clone`, `Debug`.

* `Config::parse(text: &str, path: &Path) -> Result<Config, ConfigError>`: the file's text; `path` is named in errors.
* `Config::load(root: &Path) -> Result<Config, ConfigError>`: `root/fsr.lint.toml`; `Config::default()` when it does not exist.
* `Config::discover(dir: &Path) -> Result<Config, ConfigError>`: every `fsr.lint.toml` from `dir` up to the first ancestor holding `.git`, inclusive, each over the one above. Without a `.git` above `dir`, only `dir`'s own file.
* `Config::over(self, parent: Config) -> Config`: `parent`'s layers and ignores, then this configuration's; settings merged table by table, this configuration's winning. With `inherit = false`, `self` unchanged.
* `Config::ignore(&mut self, glob: &str) -> Result<(), ConfigError>`: adds a glob a directory walk skips, relative to the walk's root.

### The File

| Key | Value |
| --- | --- |
| `inherit` | `true` (default) or `false` |
| `ignores` | list of globs |
| `[settings]` | ESLint's `settings`: `react` and `jsx-a11y` are read |
| `[rules]` | table of rule id to a level, either alone or first in a list followed by the rule's ESLint options |
| `[[overrides]]` | `files`, a list of globs (required); `rules`, a table as `[rules]` |

* A level is `"off"`, `"warn"`, `"error"`, `0`, `1` or `2`. Anything else is `ConfigError::Parse`.
* A rule id without `/` is `eslint/<id>`. `@typescript-eslint/<name>` is `eslint/<name>` for the 18 typescript-eslint rules that extend an ESLint rule, else `typescript/<name>`. `react-hooks/<name>` is `react/<name>`.
* Any `svelte/<code>` id is accepted; Svelte's warning codes are not listed ahead of time.
* `[rules]` applies before every `[[overrides]]`, whatever order the file writes them in. Overrides apply in file order.
* A layer giving only a level keeps the options an earlier layer gave.
* Any other key, at the top or in an `[[overrides]]`, is `ConfigError::Parse`.

## 3. Linting

### Linter

The rules a configuration turns on, built once with each set of options the configuration gives them.

* `Linter::new(config: Config) -> Result<Linter, ConfigError>`: `new_with(config, Vec::new())`.
* `Linter::new_with(config: Config, checks: Vec<Box<dyn Check>>) -> Result<Linter, ConfigError>`: refuses an unknown rule id (`UnknownRule`), options a rule does not take (`Options`) and settings the plugins cannot read (`Parse`). A check's id is a known rule id.
* `Linter::with_compilers(self, compilers: Box<dyn Compilers>) -> Self`: lint the templates of `.vue` and `.svelte` files through `compilers`.
* `Linter::rules(&self) -> Vec<RuleInfo>`: `rules()` and every check's `info`, sorted by id.
* `Linter::lint_source(&self, path: &Path, text: &str) -> Report`: one file's text as if read from `path`. The extension picks the front end; overrides match `path` as given. An extension no front end reads gives `Report::default()`.
* `Linter::lint_dir(&self, root: &Path) -> Report`: every file of `files(root)`. Unreadable files are skipped. Findings carry the walked path; globs match the path relative to `root`.
* `Linter::files(&self, root: &Path) -> Vec<PathBuf>`: the files under `root` a front end reads, in path order, skipping what `.gitignore` and the ignores match.
* `Linter::fix_source(&self, path: &Path, text: &str) -> Fixed`: `text` with every safe fix of the scripts' rules applied, pass after pass until none applies, at most ten. A finding a comment silences or a rule turned off is not fixed. Writes nothing.
* `Linter::fix_dir(&self, root: &Path) -> Vec<(PathBuf, Fixed)>`: `fix_source` over `files(root)`, the files with at least one fix applied.

### Level

`enum Level { Off, Warn, Error }`, ordered. `Copy`, `Eq`, `Hash`, `Ord`.

* `Level::as_str(self) -> &'static str`: `"off"`, `"warn"` or `"error"`.

### Fixed

`#[non_exhaustive]`. `Clone`, `Debug`.

* `text: String`
* `applied: usize`: the fixes applied across every pass.

## 4. Front Ends

### Scripts

`js`, `mjs`, `cjs`, `jsx`, `ts`, `mts`, `cts` and `tsx`, plus the `<script>` blocks of `vue`, `svelte` and `astro`, each linted at its offset in its file in the dialect its `lang` names. JavaScript is parsed with JSX on. Globals are the builtins, `browser` and `es2026`. The rules are ESLint's, eslint-plugin-react's, jsx-a11y's and typescript-eslint's recommended sets; `react/react-in-jsx-scope` defaults to `off`, since the automatic JSX runtime needs no `React` in scope.

### Vue and Svelte

With [Compilers](#compilers): a `.vue` file's template is described and checked by the `vue/` rules; a `.svelte` file is compiled and each warning is a finding `svelte/<code>` with `_` as `-`, default `warn`, `svelte/warning` when the compiler gives no code. An `Err` from the compilers is one `lint/compiler` finding at line 1. A `Failed` outcome's errors are `lint/parse`. `<!-- -->` comments are read as suppression comments.

### Compilers

`pub trait Compilers: Send + Sync`, asked about one component at a time; `ext` is `vue` or `svelte`.

* `fn describe(&self, ext: &str, unit: Unit) -> Result<Outcome, String>`: asked for `.vue` files.
* `fn compile(&self, ext: &str, unit: Unit) -> Result<Outcome, String>`: asked for `.svelte` files.

`Unit` and `Outcome` are `snapfire_compiler_wire`'s; the unit carries `Options::default()` and the file's path as its `filename` and `path`.

### Plugins

`pub struct Plugins`, `Default`. A [Compilers](#compilers) that starts `snapfirec-<ext>` from `PATH` on first use and keeps it for its own lifetime. A worker that fails is dropped and started again on the next file.

### Markdown and MDX

`md`, `markdown` and `mdx`, parsed by markdown-rs with GFM and front matter; `mdx` with MDX's constructs. A parse error is `lint/parse`. `<!-- -->` comments are read as suppression comments, as is `{/* */}` in MDX. The `markdown/` rules default to `error`. `markdown/no-multiple-h1` and `markdown/heading-increment` take `{ frontmatterTitle }`: a regular expression matched against each line of the front matter, by default `^\s*['"]?title['"]?\s*[:=]`; a matching line counts as the first heading. With `""` none counts. MDX's JavaScript is not linted.

### Tera

`tera`. The template is added to a fresh `tera::Tera`; a `SyntaxError` is `lint/parse` at its range's start. Other errors, such as an unknown filter or a missing parent, are not findings. `{# #}` comments are read as suppression comments.

## 5. The Report

### Report

`#[non_exhaustive]`. `Clone`, `Debug`, `Default`.

* `files: usize`: the files a front end read.
* `findings: Vec<Finding>`: sorted by line and column within a file, files in walk order.
* `suppressed: Vec<Suppressed>`
* `Report::errors(&self) -> usize`, `Report::warnings(&self) -> usize`
* `Report::failed(&self, deny_warnings: bool) -> bool`: true on an error; also true on a warning when `deny_warnings`.
* `Report::render(&self) -> String`: one finding per line, `level rule path:line:column message`, help indented beneath, then `lint     N errors, N warnings in N files`. `level` is `error` or `warning`.
* `Report::to_json(&self) -> serde_json::Value`: `{ files, errors, warnings, findings: [{ rule, level, path, line, column, message, help }], suppressed: [{ ..., reason }] }`. `level` is `"warn"` or `"error"`; `help` and `reason` may be `null`.
* `Report::to_sarif(&self) -> serde_json::Value`: SARIF 2.1.0, one run of tool `fsr lint` with each rule that has a finding, one result per finding with `ruleId`, `level` `error` or `warning`, the message and help as `message.text`, `startLine` and `startColumn`.

### Finding

`#[non_exhaustive]`. `Clone`, `Debug`, `PartialEq`.

* `rule: String`: `eslint/no-debugger`.
* `level: Level`: never `Off`.
* `path: PathBuf`
* `line: usize`, `column: usize`: 1-based; the column counts characters.
* `message: String`
* `help: Option<String>`

### Suppressed

`#[non_exhaustive]`. `Clone`, `Debug`, `PartialEq`.

* `finding: Finding`
* `reason: Option<String>`: the comment's text after ` -- `.

## 6. Rules

### rules

* `pub fn rules() -> Vec<RuleInfo>`: this crate's rules, sorted by id: the three `lint/` rules, the 356 script rules (`eslint/` 64, `react/` 20, `jsx-a11y/` 31, `typescript/` 18, `import/` 1, `unicorn/` 137, `promise/` 16, `jsdoc/` 23, `vue/` 46), the 42 `vue/` template rules, `svelte/*` standing for Svelte's warnings, the 21 `markdown/` rules (the 16 of `@eslint/markdown`'s recommended set at `error`, its other five `off`) and the three `tera/` rules.

### RuleInfo

`#[non_exhaustive]`. `Clone`, `Debug`.

* `RuleInfo::new(id: &'static str, default: Level, category: &'static str, docs: &'static str) -> RuleInfo`: `fixable` false.
* `id: &'static str`
* `default: Level`: `error` for a rule in the recommended set of ESLint, eslint-plugin-react, jsx-a11y or typescript-eslint and for one in eslint-plugin-vue's `vue3-essential`; `off` for the rest, unicorn's, promise's and jsdoc's recommended rules among them.
* `recommended: bool`: the rule is in its plugin's own recommended set, which extending the plugin's preset turns on. `RuleInfo::new` gives `false`.
* `category: &'static str`: one of `correctness`, `suspicious`, `pedantic`, `perf`, `style`, `restriction` and `nursery`; `lint` for the linter's own rules.
* `fixable: bool`: the rule offers a fix or a suggestion for some findings.
* `docs: &'static str`: Markdown.

### Suppression Comments

| Prefix | Covers |
| --- | --- |
| `fsr-lint-disable-next-line`, `eslint-disable-next-line` | the line after the comment's last line |
| `fsr-lint-disable-line`, `eslint-disable-line` | the comment's first line |
| `fsr-lint-disable`, `eslint-disable` | from the comment to the next matching enable; to the end of the file without one |
| `fsr-lint-enable`, `eslint-enable` | ends a range for the rules it names; for every rule when it names none |

* The prefix is the first word inside `//`, `/* */`, `<!-- -->`, `{/* */}` or `{# #}`. Rule ids follow, separated by commas, normalized as in [The File](#the-file). None names every rule.
* ` -- ` ends the rule list; what follows, trimmed, is the reason.
* An enable naming a rule inside a range that disables every rule re-enables only that rule.

## 7. Checks

### Check

`pub trait Check: Send + Sync`, a rule from outside the crate.

* `fn info(&self) -> RuleInfo`: read once, by `Linter::new_with`.
* `fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String))`: called for each script of a file a level other than `off` applies to; `found` takes a byte offset into `file.source` and a message.

`oxc_ast`, `oxc_semantic` and `oxc_span` are re-exported at the crate root.

### File

`#[non_exhaustive]`.

* `path: &Path`: the file's path; for a `<script>` block, its component's.
* `source: &str`: the script.
* `semantic: &oxc_semantic::Semantic`: built with the node store and the control-flow graph.

## 8. ESLint Configurations

### Translation

`#[non_exhaustive]`. `Clone`, `Debug`, `Default`.

* `toml: String`: the `fsr.lint.toml` text; empty when nothing carried over.
* `notes: Vec<String>`: each rule, preset or setting with no counterpart; each part of the source not read.

### translate_eslintrc

* `pub fn eslint::translate_eslintrc(config: &serde_json::Value, rules: &[RuleInfo]) -> Translation`: a rule carries over when `rules` has its id; an `.eslintrc` as JSON or `package.json`'s `eslintConfig`. `rules`, `overrides` (`files`, `rules`), `ignorePatterns` (`dir/` as `dir/**`) and `settings` carry over. `extends` naming a preset outside `eslint:recommended`, the react, react-hooks, jsx-a11y, typescript-eslint, vue, svelte and markdown recommended sets is a note. `env`, `globals`, `parser`, `parserOptions`, `processor` and `excludedFiles` are notes.

### translate_flat

* `pub fn eslint::translate_flat(source: &str, path: &Path, rules: &[RuleInfo]) -> Translation`: a rule carries over when `rules` has its id; an `eslint.config.{js,mjs,cjs,ts,mts}` parsed with oxc and not run. Read: `export default [ ... ]`, also when one call wraps the list, `export default f([ ... ])`. Each element written out literally is read: one with only `ignores` adds global ignores; one with `rules` and no `files`, before any with `files`, adds to `[rules]`; each other becomes an `[[overrides]]`, with `files = ["**"]` when it names none. An element naming a recommended preset (`configs.recommended`, `flat/recommended`, `recommended-latest`) needs nothing. Any other element, a spread among them, is a note quoting it.

## 9. Error Handling

### ConfigError

`#[non_exhaustive]`. `Debug`, `Display`, `std::error::Error`.

* `Parse { path: PathBuf, message: String }`: the file cannot be read or does not parse as TOML. Also a key, level, glob or setting in it that is not one.
* `UnknownRule { path: PathBuf, rule: String }`: a layer names a rule this linter does not have.
* `Options { path: PathBuf, rule: String, message: String }`: the options do not deserialize into the rule's options.
