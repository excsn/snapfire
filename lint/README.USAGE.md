# Usage Guide: snapfire_lint

How to lint a project, set rule levels, silence a finding and read the report.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Linting a Directory](#linting-a-directory)
* [Linting One Source](#linting-one-source)
* [Setting Levels](#setting-levels)
  * [Giving a Rule Options](#giving-a-rule-options)
  * [Changing Levels for Some Files](#changing-levels-for-some-files)
  * [Skipping Files](#skipping-files)
  * [Layering Files Down a Repository](#layering-files-down-a-repository)
* [Silencing a Finding](#silencing-a-finding)
* [Reading the Report](#reading-the-report)
* [Listing the Rules](#listing-the-rules)
* [Vue, Svelte and Astro Files](#vue-svelte-and-astro-files)
* [Error Handling](#error-handling)

## Core Concepts

* **Rule** is one check with an id that names its plugin, `eslint/no-debugger`. A rule ported from ESLint keeps ESLint's name.
* **Level** is how loud a rule is: `off`, `warn` or `error`, also written `0`, `1` and `2`.
* **Default level** is `error` for every rule in ESLint's recommended set and `off` for the rest, the way `eslint:recommended` sets them.
* **Configuration** is `fsr.lint.toml`, the levels and options for a project.
* **Layer** is one table of levels: the file's `[rules]` or one `[[overrides]]`. Later layers win.
* **Override** is a layer that applies only to the files its `files` globs match.
* **Ignores** are globs of files a directory walk skips.
* **Finding** is one thing a rule found: the rule, the level, the file, the line and column, the message and sometimes help.
* **Suppression comment** is a comment that silences findings on a line or in a range.
* **Suppressed** is a finding a comment silenced, kept in the report with the comment's reason.
* **Report** is what a run found, with its error and warning counts.
* **`lint/parse`** is the finding for a file that does not parse. No rule runs on such a file.
* **`lint/unused-disable`** is the finding for a suppression comment that silences nothing.

## Quick Start

From the command line, over an FSR application:

```sh
fsr lint app
# error    eslint/no-debugger                 app/routes/page.tsx:12:3  `debugger` statement is not allowed
#                                             Remove the debugger statement
# lint     1 error, 0 warnings in 14 files
```

From Rust:

```rust
use std::path::Path;

use snapfire_lint::{Config, Linter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let config = Config::discover(Path::new("."))?;
  let report = Linter::new(config)?.lint_dir(Path::new("app"));
  println!("{}", report.render());
  std::process::exit(if report.failed(false) { 1 } else { 0 });
}
```

## Linting a Directory

`lint_dir` walks the directory and lints every file a front end reads, skipping what `.gitignore` and the configuration's `ignores` match. Paths in findings are the walked paths; globs match paths relative to the directory walked.

```rust
let linter = Linter::new(Config::load(Path::new("."))?)?;
let report = linter.lint_dir(Path::new("app"));
assert_eq!(report.files, 14);
```

`Config::load` reads the directory's own `fsr.lint.toml` and gives the defaults when there is none. `Config::discover` reads every file from there up to the repository; see [Layering Files Down a Repository](#layering-files-down-a-repository).

## Linting One Source

`lint_source` lints text as if read from a path. The path picks the dialect and is what overrides match against.

```rust
let report = Linter::new(Config::default())?.lint_source(Path::new("src/page.tsx"), "debugger;\n");
assert_eq!(report.findings[0].rule, "eslint/no-debugger");
assert_eq!((report.findings[0].line, report.findings[0].column), (1, 1));
```

A file whose extension no front end reads gives an empty report with `files` at 0.

## Setting Levels

`[rules]` maps a rule id to a level. An ESLint name without a plugin means the `eslint/` rule, so an ESLint configuration's names work as written.

```toml
[rules]
"eslint/no-debugger" = "warn"
"no-empty" = "off"
"eslint/no-cond-assign" = 2
```

A `@typescript-eslint/` name means the `typescript/` rule, except for the typescript-eslint rules that extend an ESLint rule of the same name, such as `no-unused-vars`, which mean the `eslint/` rule run here.

```toml
[rules]
"@typescript-eslint/no-unused-vars" = "warn"
```

An unknown rule id is an error naming the id. So is a level that is not one.

### Giving a Rule Options

A list gives the level first and then the rule's ESLint options.

```toml
[rules]
"eslint/no-unused-vars" = ["error", { argsIgnorePattern = "^_", caughtErrors = "none" }]
"eslint/no-empty" = ["error", { allowEmptyCatch = true }]
```

Options a rule does not take are an error naming the rule. A later layer that gives only a level keeps the options an earlier one gave.

### Changing Levels for Some Files

Each `[[overrides]]` applies to the files its globs match, after `[rules]` and in the order written.

```toml
[rules]
"eslint/no-debugger" = "error"

[[overrides]]
files = ["tests/**", "**/*.test.ts"]
rules = { "eslint/no-debugger" = "off" }

[[overrides]]
files = ["tests/strict/**"]
rules = { "eslint/no-debugger" = "error" }
```

### Skipping Files

`ignores` lists globs a directory walk skips, beside what `.gitignore` already skips.

```toml
ignores = ["routes/legacy/**", "src/generated.ts"]
```

A tool that knows which directories are not the project's adds them itself:

```rust
let mut config = Config::discover(root)?;
config.ignore("vendor/**")?;
```

`fsr lint` skips the application's vendor tree and fetched declarations this way.

### Layering Files Down a Repository

`Config::discover` reads every `fsr.lint.toml` from the directory up to the one holding `.git`, each over the one above it. A file sets only what differs from the one above.

```text
repo/fsr.lint.toml              "eslint/no-debugger" = "warn"
repo/sites/notes/fsr.lint.toml  "eslint/no-empty" = "error"
```

```rust
let config = Config::discover(Path::new("repo/sites/notes"))?;
```

The notes site gets `no-debugger` at `warn` from the repository and `no-empty` at `error` from its own file. `inherit = false` at the top of a file starts it from the defaults instead. Outside a repository only the directory's own file is read. `Config::over` does the same for two configurations already in hand:

```rust
let site = Config::load(site_dir)?.over(Config::load(shell_dir)?);
```

## Silencing a Finding

A comment silences the rules it names, separated by commas. A comment naming no rule silences every rule. Text after ` -- ` is the reason. Both forms are read:

```ts
// fsr-lint-disable-next-line eslint/no-debugger -- kept for the profiler
debugger;

debugger; // eslint-disable-line no-debugger

/* fsr-lint-disable eslint/no-empty, eslint/no-debugger */
if (ready) {}
/* fsr-lint-enable eslint/no-empty, eslint/no-debugger */

/* eslint-disable */
```

| Comment | Covers |
| --- | --- |
| `fsr-lint-disable-next-line`, `eslint-disable-next-line` | the line after the comment |
| `fsr-lint-disable-line`, `eslint-disable-line` | the comment's own line |
| `fsr-lint-disable` and `fsr-lint-enable`, `eslint-disable` and `eslint-enable` | the lines between them; to the end of the file with no enable |

A silenced finding moves to `Report::suppressed` with its reason. A comment naming only rules this crate has, that silences nothing, is reported as `lint/unused-disable` at `warn`. A comment that names no rule is never reported, nor one that names a rule this crate does not have, since what it was written for may not run here. A comment inside a string is not a directive. `lint/parse` cannot be silenced.

## Reading the Report

```rust
let report = linter.lint_dir(Path::new("app"));
for finding in &report.findings {
  println!("{} {} {}:{}:{} {}", finding.level.as_str(), finding.rule, finding.path.display(), finding.line, finding.column, finding.message);
}
for silenced in &report.suppressed {
  println!("silenced {} because {:?}", silenced.finding.rule, silenced.reason);
}
let fails_ci = report.failed(true);
```

`render` gives the terminal form: one finding per line, its help beneath it, then the totals.

```text
warning  eslint/no-debugger                 a.ts:1:1  `debugger` statement is not allowed
                                            Remove the debugger statement
lint     0 errors, 1 warning in 1 file
```

`to_json` gives every field:

```json
{
  "files": 1,
  "errors": 1,
  "warnings": 0,
  "findings": [{ "rule": "eslint/no-debugger", "level": "error", "path": "a.ts", "line": 1, "column": 1, "message": "`debugger` statement is not allowed", "help": "Remove the debugger statement" }],
  "suppressed": []
}
```

Lines and columns are 1-based. Columns count characters.

## Listing the Rules

```rust
for rule in snapfire_lint::rules() {
  println!("{} {} {} fixable={}", rule.default.as_str(), rule.category, rule.id, rule.fixable);
}
```

```sh
fsr lint --rules
# error  correctness  eslint/constructor-super
# error  correctness  eslint/for-direction
# ...
```

Each rule's `docs` is its documentation from the port: what it does, why and examples.

## Vue, Svelte and Astro Files

The `<script>` blocks of a `.vue`, `.svelte` or `.astro` file are linted as their own sections in the dialect their `lang` names. Each finding points into the original file. Templates are not linted yet.

```rust
let report = linter.lint_source(Path::new("Counter.vue"), "<template><p>{{ n }}</p></template>\n<script setup lang=\"ts\">\nlet n = 1;\ndebugger;\n</script>\n");
assert_eq!(report.findings[0].line, 4);
```

## Error Handling

`Config::parse`, `Config::load`, `Config::discover`, `Config::ignore` and `Linter::new` return `ConfigError`. A finding is never an error; it is in the report.

```rust
use snapfire_lint::ConfigError;

match Linter::new(config) {
  Ok(linter) => linter,
  Err(ConfigError::UnknownRule { path, rule }) => panic!("{}: no rule {rule}", path.display()),
  Err(ConfigError::Options { rule, message, .. }) => panic!("{rule}: {message}"),
  Err(e) => panic!("{e}"),
};
```
