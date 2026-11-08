# API Reference: snapfire_lint

A linter for TypeScript, JavaScript, Vue, Svelte, Markdown, MDX and Tera with ESLint's rules, levels and suppression comments.

## Contents

* [1. Constants](#1-constants)
  * [`CONFIG_FILE`](#config_file)
  * [`PARSE`](#parse)
  * [`UNUSED_DISABLE`](#unused_disable)
  * [`COMPILER`](#compiler)
* [2. Configuration](#2-configuration)
  * [`Config`](#config)
  * [`fsr.lint.toml`](#fsrlinttoml)
* [3. Linting](#3-linting)
  * [`Linter`](#linter)
  * [`Level`](#level)
  * [`Fixed`](#fixed)
* [4. Front Ends](#4-front-ends)
  * [`Compilers`](#compilers)
  * [`Plugins`](#plugins)
* [5. The Report](#5-the-report)
  * [`Report`](#report)
  * [`Finding`](#finding)
  * [`Suppressed`](#suppressed)
* [6. Rules](#6-rules)
  * [`rules`](#rules)
  * [`RuleInfo`](#ruleinfo)
* [7. Suppression Comments](#7-suppression-comments)
* [8. Checks](#8-checks)
  * [`Check`](#check)
  * [`File`](#file)
* [9. ESLint Configurations](#9-eslint-configurations)
  * [`Translation`](#translation)
  * [`translate_eslintrc`](#translate_eslintrc)
  * [`translate_flat`](#translate_flat)
* [10. Error Handling](#10-error-handling)
  * [`ConfigError`](#configerror)

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

The layers of levels, the ignored globs and the settings.

* `pub struct Config`. `Clone`, `Debug`, `Default`.
* `Default`: no layers, `inherit = true`, no ignores and no settings. Every rule runs at its default level.
* `pub fn parse(text: &str, path: &Path) -> Result<Config, ConfigError>`: a configuration from the file's text. `path` is named in errors.
* `pub fn load(root: &Path) -> Result<Config, ConfigError>`: `root/fsr.lint.toml`. `Config::default()` when it does not exist.
* `pub fn discover(dir: &Path) -> Result<Config, ConfigError>`: every `fsr.lint.toml` from `dir` up to the first ancestor holding `.git`, inclusive, each over the one above. Without a `.git` above `dir`, only `dir`'s own file.
* `pub fn over(self, parent: Config) -> Config`: `parent`'s layers and ignores, then this configuration's. Settings are merged table by table, this configuration's winning. With `inherit = false`, `self` unchanged.
* `pub fn ignore(&mut self, glob: &str) -> Result<(), ConfigError>`: adds a glob a directory walk skips, relative to the walk's root.

### fsr.lint.toml

| Key | Value |
| --- | --- |
| `inherit` | `true` (default) or `false` |
| `ignores` | list of globs |
| `[settings]` | ESLint's `settings`: `react` and `jsx-a11y` are read |
| `[rules]` | table of rule id to a level, either alone or first in a list followed by the rule's ESLint options |
| `[[overrides]]` | `files`: a list of globs, required. `rules`: a table shaped as `[rules]` |

* A level is `"off"`, `"warn"`, `"error"`, `0`, `1` or `2`. Anything else is `ConfigError::Parse`.
* A rule id without `/` is `eslint/<id>`. `@typescript-eslint/<name>` is `eslint/<name>` for the 18 typescript-eslint rules that extend an ESLint rule, else `typescript/<name>`. `react-hooks/<name>` is `react/<name>`.
* Any `svelte/<code>` id is accepted.
* `[rules]` applies before every `[[overrides]]`, whatever order the file writes them in. Overrides apply in file order.
* A layer giving only a level keeps the options an earlier layer gave.
* Any other key, at the top or in an `[[overrides]]`, is `ConfigError::Parse`.

## 3. Linting

### Linter

The rules a configuration turns on, each built once per set of options the configuration gives it.

* `pub struct Linter`
* `pub fn new(config: Config) -> Result<Linter, ConfigError>`: `new_with(config, Vec::new())`.
* `pub fn new_with(config: Config, checks: Vec<Box<dyn Check>>) -> Result<Linter, ConfigError>`: refuses an unknown rule id (`UnknownRule`), options a rule does not take (`Options`) and settings the plugins cannot read (`Parse`). A check's id is a known rule id.
* `pub fn with_compilers(mut self, compilers: Box<dyn Compilers>) -> Self`: lint the templates of `.vue` and `.svelte` files through `compilers`.
* `pub fn rules(&self) -> Vec<RuleInfo>`: `rules()` and every check's `info`, sorted by id.
* `pub fn lint_source(&self, path: &Path, text: &str) -> Report`: one file's text as if read from `path`. The extension picks the front end. Overrides match `path` as given. An extension no front end reads gives `Report::default()`.
* `pub fn lint_dir(&self, root: &Path) -> Report`: every file of `files(root)`. Unreadable files are skipped. Findings carry the walked path. Globs match the path relative to `root`.
* `pub fn files(&self, root: &Path) -> Vec<PathBuf>`: the files under `root` a front end reads, in path order, skipping what `.gitignore` and the ignores match.
* `pub fn fix_source(&self, path: &Path, text: &str) -> Fixed`: `text` with every safe fix of the script rules applied, pass after pass until none applies, at most ten passes. A finding a comment silences or a rule turned off is not fixed. Writes nothing.
* `pub fn fix_dir(&self, root: &Path) -> Vec<(PathBuf, Fixed)>`: `fix_source` over `files(root)`, returning the files with at least one fix applied.

### Level

* `pub enum Level { Off, Warn, Error }`. `Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`. Ordered `Off < Warn < Error`.
* `pub fn as_str(self) -> &'static str`: `"off"`, `"warn"` or `"error"`.

### Fixed

* `pub struct Fixed { pub text: String, pub applied: usize }`. `#[non_exhaustive]`. `Debug`, `Clone`.
* `applied`: the fixes applied across every pass.

## 4. Front Ends

The file's extension picks the front end.

* Scripts: `js`, `mjs`, `cjs`, `jsx`, `ts`, `mts`, `cts` and `tsx`, plus the `<script>` blocks of `vue`, `svelte` and `astro`. Each block is linted at its offset in its file, in the dialect its `lang` names.
* JavaScript is parsed with JSX on. Globals are the builtins, `browser` and `es2026`.
* Script rules on by default are the recommended sets of ESLint, eslint-plugin-react, jsx-a11y and typescript-eslint, except `react/react-in-jsx-scope`, which defaults to `off`.
* `vue` and `svelte`, with `Compilers` set: a `.vue` file's template is described and checked by the `vue/` rules. A `.svelte` file is compiled and each warning is a finding `svelte/<code>` with `_` as `-`, default `warn`. A warning without a code is `svelte/warning`.
* An `Err` from the compilers is one `lint/compiler` finding at line 1. A `Failed` outcome's errors are `lint/parse`. In `vue` and `svelte`, `<!-- -->` comments are read as suppression comments.
* Markdown: `md`, `markdown` and `mdx`, parsed with GFM and front matter. `mdx` adds MDX's constructs. A parse error is `lint/parse`. `<!-- -->` comments and, in MDX, `{/* */}` are read as suppression comments.
* The `markdown/` rules default to `error`, except the five outside `@eslint/markdown`'s recommended set.
* `markdown/no-multiple-h1` and `markdown/heading-increment` take `{ frontmatterTitle }`, a regular expression matched against each line of the front matter. Default `^\s*['"]?title['"]?\s*[:=]`. A matching line counts as the first heading. With `""` none counts.
* An MDX page's `import` and `export` blocks, `{expressions}` and attribute values are linted with the script rules, at their offsets in the page. A component tag counts as a use of its name. `no-unused-expressions` does not report a rendered value.
* Tera: `tera`. A `SyntaxError` from Tera's parser is `lint/parse` at its range's start. Other Tera errors, such as an unknown filter or a missing parent, are not findings. `{# #}` comments are read as suppression comments.

### Compilers

Asked about one component at a time.

* `pub trait Compilers: Send + Sync`
* `fn describe(&self, ext: &str, unit: snapfire_compiler_wire::Unit) -> Result<snapfire_compiler_wire::Outcome, String>`: asked for `.vue` files.
* `fn compile(&self, ext: &str, unit: snapfire_compiler_wire::Unit) -> Result<snapfire_compiler_wire::Outcome, String>`: asked for `.svelte` files.
* `ext` is `vue` or `svelte`. The unit carries `Options::default()` and the file's path as both its `filename` and `path`.

### Plugins

* `pub struct Plugins`. `Default`. Implements `Compilers`.
* Starts `snapfirec-<ext>` from `PATH` on first use and keeps it for its own lifetime. A worker that fails is dropped and started again on the next file.

## 5. The Report

### Report

What one run found.

* `pub struct Report { pub files: usize, pub findings: Vec<Finding>, pub suppressed: Vec<Suppressed> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Default`.
* `files`: the files a front end read.
* `findings`: sorted by line and column within a file, files in walk order.
* `pub fn errors(&self) -> usize`
* `pub fn warnings(&self) -> usize`
* `pub fn failed(&self, deny_warnings: bool) -> bool`: true on an error. Also true on a warning when `deny_warnings`.
* `pub fn render(&self) -> String`: one finding per line as `level rule path:line:column message`, help indented beneath, then `lint     N errors, N warnings in N files`. `level` is `error` or `warning`.
* `pub fn to_json(&self) -> serde_json::Value`: `{ files, errors, warnings, findings: [{ rule, level, path, line, column, message, help }], suppressed: [{ ..., reason }] }`. `level` is `"warn"` or `"error"`. `help` and `reason` may be `null`.
* `pub fn to_sarif(&self) -> serde_json::Value`: SARIF 2.1.0 with one run of the tool `fsr lint`, listing each rule that has a finding.
* Each SARIF result carries `ruleId`, `level` `error` or `warning`, the message and help as `message.text`, `startLine` and `startColumn`.

### Finding

One thing a rule found.

* `pub struct Finding { pub rule: String, pub level: Level, pub path: PathBuf, pub line: usize, pub column: usize, pub message: String, pub help: Option<String> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`.
* `rule`: such as `eslint/no-debugger`.
* `level`: never `Off`.
* `line` and `column`: 1-based. The column counts characters.

### Suppressed

A finding a comment silenced.

* `pub struct Suppressed { pub finding: Finding, pub reason: Option<String> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `PartialEq`.
* `reason`: the comment's text after ` -- `.

## 6. Rules

### rules

* `pub fn rules() -> Vec<RuleInfo>`: this crate's rules, sorted by id.
* The three `lint/` rules.
* The 356 script rules: `eslint/` 64, `react/` 20, `jsx-a11y/` 31, `typescript/` 18, `import/` 1, `unicorn/` 137, `promise/` 16, `jsdoc/` 23 and `vue/` 46.
* The 42 `vue/` template rules.
* `svelte/*`, standing for Svelte's warnings.
* The 21 `markdown/` rules: the 16 of `@eslint/markdown`'s recommended set at `error` and the other five `off`.
* The three `tera/` rules.

### RuleInfo

What a rule is, for listing.

* `pub struct RuleInfo { pub id: &'static str, pub default: Level, pub recommended: bool, pub category: &'static str, pub fixable: bool, pub docs: &'static str }`. `#[non_exhaustive]`. `Debug`, `Clone`.
* `pub const fn new(id: &'static str, default: Level, category: &'static str, docs: &'static str) -> RuleInfo`: `recommended` and `fixable` are `false`.
* `default`: `error` for a rule in eslint-plugin-vue's `vue3-essential` or in the recommended set of ESLint, eslint-plugin-react, jsx-a11y or typescript-eslint. `off` for the rest, including the recommended rules of unicorn, promise and jsdoc.
* `recommended`: the rule is in its plugin's own recommended set, which extending the plugin's preset turns on.
* `category`: one of `correctness`, `suspicious`, `pedantic`, `perf`, `style`, `restriction` and `nursery`. `lint` for the linter's own rules.
* `fixable`: the rule offers a fix or a suggestion for some findings.
* `docs`: Markdown.

## 7. Suppression Comments

| Prefix | Covers |
| --- | --- |
| `fsr-lint-disable-next-line`, `eslint-disable-next-line` | the line after the comment's last line |
| `fsr-lint-disable-line`, `eslint-disable-line` | the comment's first line |
| `fsr-lint-disable`, `eslint-disable` | from the comment to the next matching enable. Without one, to the end of the file |
| `fsr-lint-enable`, `eslint-enable` | ends a range for the rules it names. Naming none ends it for every rule |

* The prefix is the first word inside `//`, `/* */`, `<!-- -->`, `{/* */}` or `{# #}`. Rule ids follow, separated by commas, normalized as in [`fsr.lint.toml`](#fsrlinttoml). None names every rule.
* ` -- ` ends the rule list. What follows, trimmed, is the reason.
* An enable naming a rule inside a range that disables every rule re-enables only that rule.

## 8. Checks

`oxc_ast`, `oxc_semantic` and `oxc_span` are re-exported at the crate root.

### Check

A rule from outside the crate.

* `pub trait Check: Send + Sync`
* `fn info(&self) -> RuleInfo`: read once, by `Linter::new_with`.
* `fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String))`: called for each script of a file a level other than `off` applies to. `found` takes a byte offset into `file.source` and a message.
* Levels and suppression comments apply to a check's findings as to any rule.

### File

One script as a `Check` sees it.

* `pub struct File<'a> { pub path: &'a Path, pub source: &'a str, pub semantic: &'a oxc_semantic::Semantic<'a> }`. `#[non_exhaustive]`.
* `path`: the file's path. For a `<script>` block, its component's.
* `source`: the script.
* `semantic`: built with the node store and the control-flow graph.

## 9. ESLint Configurations

In the `eslint` module.

### Translation

* `pub struct Translation { pub toml: String, pub notes: Vec<String> }`. `#[non_exhaustive]`. `Debug`, `Clone`, `Default`.
* `toml`: the `fsr.lint.toml` text. Empty when nothing carried over.
* `notes`: each rule, preset or setting with no counterpart and each part of the source not read.

### translate_eslintrc

* `pub fn translate_eslintrc(config: &serde_json::Value, rules: &[RuleInfo]) -> Translation`: an `.eslintrc` as JSON or `package.json`'s `eslintConfig`. A rule carries over when `rules` has its id.
* `rules`, `overrides` (`files` and `rules`), `ignorePatterns` and `settings` carry over. An `ignorePatterns` entry `dir/` becomes `dir/**`.
* `extends` naming a preset other than `eslint:recommended` or the react, react-hooks, jsx-a11y, typescript-eslint, vue, svelte and markdown recommended sets is a note.
* `env`, `globals`, `parser`, `parserOptions`, `processor` and `excludedFiles` are notes.

### translate_flat

* `pub fn translate_flat(source: &str, path: &Path, rules: &[RuleInfo]) -> Translation`: an `eslint.config.{js,mjs,cjs,ts,mts}`, parsed and not run. A rule carries over when `rules` has its id.
* Reads `export default [ ... ]`, also when one call wraps the list as `export default f([ ... ])`. Each element written out literally is read.
* An element with only `ignores` adds global ignores.
* An element with `rules` and no `files`, before any element with `files`, adds to `[rules]`.
* Each other element becomes an `[[overrides]]`, with `files = ["**"]` when it names none.
* An element naming a recommended preset (`configs.recommended`, `flat/recommended` or `recommended-latest`) needs nothing.
* Any other element, a spread among them, is a note quoting it.

## 10. Error Handling

### ConfigError

`#[non_exhaustive]`. `Debug`, `std::error::Error`.

* `Parse { path: PathBuf, message: String }`: the file cannot be read or is not TOML. Also a key, level, glob or setting in it that is not valid. Displays as `<path>: <message>`.
* `UnknownRule { path: PathBuf, rule: String }`: a layer names a rule this linter does not have. Displays as ``<path>: no rule is called `<rule>` ``.
* `Options { path: PathBuf, rule: String, message: String }`: the options do not deserialize into the rule's options. Displays as ``<path>: `<rule>` does not take these options: <message>``.
