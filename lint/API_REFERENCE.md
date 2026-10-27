# API Reference: snapfire_lint

A linter for TypeScript and JavaScript with ESLint's recommended rules, levels and suppression comments.

## Contents

* [1. Constants](#1-constants)
  * [CONFIG_FILE](#config_file)
  * [PARSE](#parse)
  * [UNUSED_DISABLE](#unused_disable)
* [2. Configuration](#2-configuration)
  * [Config](#config)
  * [The File](#the-file)
* [3. Linting](#3-linting)
  * [Linter](#linter)
  * [Level](#level)
* [4. The Report](#4-the-report)
  * [Report](#report)
  * [Finding](#finding)
  * [Suppressed](#suppressed)
* [5. Rules](#5-rules)
  * [rules](#rules)
  * [RuleInfo](#ruleinfo)
  * [Suppression Comments](#suppression-comments)
* [6. Error Handling](#6-error-handling)
  * [ConfigError](#configerror)

## 1. Constants

### CONFIG_FILE

* `pub const CONFIG_FILE: &str = "fsr.lint.toml"`: the file a project's levels live in, at its root.

### PARSE

* `pub const PARSE: &str = "lint/parse"`: the rule of a file that does not parse. Default `error`. No other rule runs on the file and no comment silences it.

### UNUSED_DISABLE

* `pub const UNUSED_DISABLE: &str = "lint/unused-disable"`: the rule of a suppression comment that silences nothing. Default `warn`. Only a comment whose every named rule this crate has is reported.

## 2. Configuration

### Config

The layers of levels and the ignored globs. `Default` is no layers, `inherit = true` and no ignores, which runs every rule at its default level. `Clone`, `Debug`.

* `Config::parse(text: &str, path: &Path) -> Result<Config, ConfigError>`: the file's text; `path` is named in errors.
* `Config::load(root: &Path) -> Result<Config, ConfigError>`: `root/fsr.lint.toml`; `Config::default()` when it does not exist.
* `Config::discover(dir: &Path) -> Result<Config, ConfigError>`: every `fsr.lint.toml` from `dir` up to the first ancestor holding `.git`, inclusive, each over the one above. Without a `.git` above `dir`, only `dir`'s own file.
* `Config::over(self, parent: Config) -> Config`: `parent`'s layers and ignores, then this configuration's. With `inherit = false`, `self` unchanged.
* `Config::ignore(&mut self, glob: &str) -> Result<(), ConfigError>`: adds a glob a directory walk skips, relative to the walk's root.

### The File

| Key | Value |
| --- | --- |
| `inherit` | `true` (default) or `false` |
| `ignores` | list of globs |
| `[rules]` | table of rule id to a level, either alone or first in a list followed by the rule's ESLint options |
| `[[overrides]]` | `files`, a list of globs (required); `rules`, a table as `[rules]` |

* A level is `"off"`, `"warn"`, `"error"`, `0`, `1` or `2`. Anything else is `ConfigError::Parse`.
* A rule id without `/` is `eslint/<id>`. `@typescript-eslint/<name>` is `eslint/<name>` for the 18 typescript-eslint rules that extend an ESLint rule, else `typescript/<name>`.
* `[rules]` applies before every `[[overrides]]`, whatever order the file writes them in. Overrides apply in file order.
* A layer giving only a level keeps the options an earlier layer gave.
* Any other key, at the top or in an `[[overrides]]`, is `ConfigError::Parse`.

## 3. Linting

### Linter

The rules a configuration turns on, built once with each set of options the configuration gives them.

* `Linter::new(config: Config) -> Result<Linter, ConfigError>`: refuses an unknown rule id (`UnknownRule`) and options a rule does not take (`Options`).
* `Linter::lint_source(&self, path: &Path, text: &str) -> Report`: one file's text as if read from `path`. The extension picks the front end; overrides match `path` as given. An extension no front end reads gives `Report::default()`.
* `Linter::lint_dir(&self, root: &Path) -> Report`: every file under `root` whose extension a front end reads, in path order, skipping what `.gitignore` and the ignores match. Unreadable files are skipped. Findings carry the walked path; globs match the path relative to `root`.

Extensions read: `js`, `mjs`, `cjs`, `jsx`, `ts`, `mts`, `cts` and `tsx`, plus the `<script>` blocks of `vue`, `svelte` and `astro`. JavaScript is parsed with JSX on. Globals are the builtins, `browser` and `es2026`.

### Level

`enum Level { Off, Warn, Error }`, ordered. `Copy`, `Eq`, `Hash`, `Ord`.

* `Level::as_str(self) -> &'static str`: `"off"`, `"warn"` or `"error"`.

## 4. The Report

### Report

`#[non_exhaustive]`. `Clone`, `Debug`, `Default`.

* `files: usize`: the files a front end read.
* `findings: Vec<Finding>`: sorted by line and column within a file, files in walk order.
* `suppressed: Vec<Suppressed>`
* `Report::errors(&self) -> usize`, `Report::warnings(&self) -> usize`
* `Report::failed(&self, deny_warnings: bool) -> bool`: true on an error; also true on a warning when `deny_warnings`.
* `Report::render(&self) -> String`: one finding per line, `level rule path:line:column message`, help indented beneath, then `lint     N errors, N warnings in N files`. `level` is `error` or `warning`.
* `Report::to_json(&self) -> serde_json::Value`: `{ files, errors, warnings, findings: [{ rule, level, path, line, column, message, help }], suppressed: [{ ..., reason }] }`. `level` is `"warn"` or `"error"`; `help` and `reason` may be `null`.

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

## 5. Rules

### rules

* `pub fn rules() -> Vec<RuleInfo>`: every rule, sorted by id: `lint/parse`, `lint/unused-disable` and the 61 `eslint/` rules of ESLint's recommended set.

### RuleInfo

`#[non_exhaustive]`. `Clone`, `Debug`.

* `id: &'static str`
* `default: Level`: `Error` for a recommended rule, else `Off`; `lint/unused-disable` is `Warn`.
* `category: &'static str`: one of `correctness`, `suspicious`, `pedantic`, `perf`, `style`, `restriction` and `nursery`; `lint` for the two linter rules.
* `fixable: bool`: the rule offers a fix or a suggestion for some findings.
* `docs: &'static str`: Markdown.

### Suppression Comments

| Prefix | Covers |
| --- | --- |
| `fsr-lint-disable-next-line`, `eslint-disable-next-line` | the line after the comment's last line |
| `fsr-lint-disable-line`, `eslint-disable-line` | the comment's first line |
| `fsr-lint-disable`, `eslint-disable` | from the comment to the next matching enable; to the end of the file without one |
| `fsr-lint-enable`, `eslint-enable` | ends a range for the rules it names; for every rule when it names none |

* The prefix is the first word inside `//`, `/* */`, `<!-- -->` or `{# #}`. Rule ids follow, separated by commas, normalized as in [The File](#the-file). None names every rule.
* ` -- ` ends the rule list; what follows, trimmed, is the reason.
* An enable naming a rule inside a range that disables every rule re-enables only that rule.

## 6. Error Handling

### ConfigError

`#[non_exhaustive]`. `Debug`, `Display`, `std::error::Error`.

* `Parse { path: PathBuf, message: String }`: the file cannot be read or does not parse as TOML. Also a key, level or glob in it that is not one.
* `UnknownRule { path: PathBuf, rule: String }`: a layer names a rule this crate does not have.
* `Options { path: PathBuf, rule: String, message: String }`: the options do not deserialize into the rule's options.
