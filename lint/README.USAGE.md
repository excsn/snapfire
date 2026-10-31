# Usage Guide: snapfire_lint

How to lint a project, set rule levels, silence a finding, read the report, apply fixes, bring an ESLint configuration over and add a rule of your own.

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
  * [Giving Rules Their Settings](#giving-rules-their-settings)
* [Silencing a Finding](#silencing-a-finding)
* [Reading the Report](#reading-the-report)
* [Listing the Rules](#listing-the-rules)
* [Linting Vue and Svelte Components](#linting-vue-and-svelte-components)
* [Linting Markdown and MDX](#linting-markdown-and-mdx)
* [Linting Tera Templates](#linting-tera-templates)
* [Applying Fixes](#applying-fixes)
* [Translating an ESLint Configuration](#translating-an-eslint-configuration)
* [Adding a Rule of Your Own](#adding-a-rule-of-your-own)
* [Error Handling](#error-handling)

## Core Concepts

* **Rule** is one check with an id that names its plugin, `eslint/no-debugger`. A rule ported from ESLint or one of its plugins keeps that rule's name: `react/rules-of-hooks`, `jsx-a11y/alt-text`, `typescript/no-explicit-any`, `vue/require-v-for-key`, `markdown/heading-increment`.
* **Front end** reads one kind of file and runs its rules: JavaScript and TypeScript, Vue and Svelte templates, Markdown and MDX, Tera.
* **Level** is how loud a rule is: `off`, `warn` or `error`, also written `0`, `1` and `2`.
* **Default level** is `error` for every rule in its plugin's recommended set and `off` for the rest, the way `eslint:recommended` sets them. Svelte's warnings and a few style rules default to `warn`.
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
* **`lint/compiler`** is the finding for a `.vue` or `.svelte` file whose framework compiler could not be asked.
* **Compilers** answer for the Vue and Svelte compilers; `Plugins` asks `snapfirec-vue` and `snapfirec-svelte`.
* **Check** is a rule from outside the crate.

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

### Giving Rules Their Settings

`[settings]` is ESLint's `settings`, read by the React and jsx-a11y rules: the React version, the link and form components, which components stand for which elements.

```toml
[settings.react]
version = "19.0"
linkComponents = [{ name = "Link", linkAttribute = "to" }]

[settings."jsx-a11y".components]
Image = "img"
Button = "button"
```

With `Image = "img"`, `jsx-a11y/alt-text` checks `<Image>` as it checks `<img>`. A nearer file's settings merge over the one above it, table by table.

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

A rule in the recommended set of ESLint, eslint-plugin-react, jsx-a11y or typescript-eslint defaults to `error`, as does one in eslint-plugin-vue's `vue3-essential`, as the presets a TypeScript, React or Vue application extends set it. The rest default to `off` and are turned on in `fsr.lint.toml`: `eslint/no-console` and the other rules past those sets. So is every rule of eslint-plugin-unicorn, eslint-plugin-promise and eslint-plugin-jsdoc, which an ESLint configuration turns on by extending their presets. `rule.recommended` says which rules a plugin's preset holds.

## Linting Vue and Svelte Components

The `<script>` blocks of a `.vue`, `.svelte` or `.astro` file are linted as their own sections in the dialect their `lang` names, with every JavaScript and TypeScript rule and, in a `.vue` file, eslint-plugin-vue's script rules: `defineProps` and `defineEmits`, computed properties, lifecycle hooks after `await` and the rest oxlint ports. A rule that needs both blocks of a component reads `<script>` and `<script setup>` together. Each finding points into the original file.

Templates are read by the framework's own compiler. Give the linter something that can ask it; `Plugins` asks the `snapfirec-vue` and `snapfirec-svelte` plugins, starting each once and keeping it for the run:

```rust
use snapfire_lint::{Config, Linter, Plugins};

let linter = Linter::new(Config::default())?.with_compilers(Box::new(Plugins::default()));
let report = linter.lint_source(Path::new("List.vue"), "<template>\n  <li v-for=\"x in xs\">{{ x }}</li>\n</template>\n");
assert_eq!(report.findings[0].rule, "vue/require-v-for-key");
```

| Rule | Finds |
| --- | --- |
| `vue/require-v-for-key` | an element with `v-for` and no `:key` |
| `vue/no-use-v-if-with-v-for` | `v-if` on the element that has `v-for` |
| `vue/require-component-is` | `<component>` with no `is` |
| `vue/no-template-key` | a `key` on a `<template>` with no `v-for` |
| `vue/valid-v-if`, `vue/valid-v-for`, `vue/valid-v-model` and the rest of `vue/valid-v-*` | a directive in the wrong place or one missing a value, argument or modifier it needs |
| `vue/no-duplicate-attributes`, `vue/no-dupe-v-else-if`, `vue/no-child-content` | an attribute set twice, a branch tested twice, children `v-html` overwrites |
| `vue/no-deprecated-*` | what Vue 3 removed: `.sync`, `.native`, `slot`, `slot-scope`, `v-is`, `$listeners` and the rest |
| `svelte/<code>` | each warning Svelte's compiler gives, `a11y_missing_attribute` as `svelte/a11y-missing-attribute`, at `warn` |

Svelte's codes are not listed ahead of time, so any `svelte/` id is accepted in `fsr.lint.toml`. A warning from a `snapfirec-svelte` that gives no code is `svelte/warning`. Svelte's own `<!-- svelte-ignore code -->` still silences a warning; so does `<!-- eslint-disable-next-line svelte/a11y-missing-attribute -->`.

Without compilers only the scripts are linted. When a plugin cannot be started the file gets one `lint/compiler` warning and its scripts are still linted. A component the compiler refuses is `lint/parse`.

Anything holding the compilers can answer instead of the plugins, by implementing `Compilers`:

```rust
use snapfire_compiler_wire::{Outcome, Unit};
use snapfire_lint::Compilers;

struct InProcess { vue: snapfire_vue::Compiler, svelte: snapfire_svelte::Compiler }

impl Compilers for InProcess {
  fn describe(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    match ext {
      "vue" => self.vue.describe(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
      _ => self.svelte.describe(&unit.filename, &unit.source, &unit.options).map_err(|e| e.to_string()),
    }
  }
  fn compile(&self, ext: &str, unit: Unit) -> Result<Outcome, String> {
    match ext {
      "vue" => self.vue.compile(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
      _ => self.svelte.compile(&unit.filename, &unit.source, &unit.options, &unit.files).map_err(|e| e.to_string()),
    }
  }
}
```

## Linting Markdown and MDX

`.md` and `.mdx` files are parsed by markdown-rs with GFM and front matter. They are checked by the correctness rules of `@eslint/markdown`'s recommended set, under its names, each at `error`:

| Rule | Finds |
| --- | --- |
| `markdown/heading-increment` | a heading more than one level deeper than the one before |
| `markdown/no-multiple-h1` | a second top-level heading; a front-matter `title` counts as the first |
| `markdown/no-empty-links`, `markdown/no-empty-images` | a link or image with no destination |
| `markdown/require-alt-text` | an image with no alternative text |
| `markdown/fenced-code-language` | a fenced code block that names no language |
| `markdown/no-duplicate-definitions`, `markdown/no-empty-definitions` | a link definition given twice; one with no URL |
| `markdown/no-missing-link-fragments` | `[x](#fragment)` that no heading of the document makes |
| `markdown/no-reversed-media-syntax` | `(text)[url]` |
| `markdown/no-missing-atx-heading-space` | `#Heading` with no space |

```md
<!-- eslint-disable-next-line markdown/heading-increment -->
### Skipped level
```

`markdown/no-multiple-h1` and `markdown/heading-increment` count a front-matter `title` as the first heading, as `@eslint/markdown` does. Their `frontmatterTitle` option names another key by a regular expression over its line. With `""` none counts, which fits a site whose front matter is the document's title rather than a heading:

```toml
[rules]
"markdown/no-multiple-h1" = ["error", { frontmatterTitle = "" }]
```

In MDX a comment is `{/* fsr-lint-disable-next-line markdown/heading-increment */}`. An MDX file's imports and expressions are read as MDX but not linted as JavaScript yet.

## Linting Tera Templates

A `.tera` file is handed to tera itself: what it cannot parse is `lint/parse`, at the place tera names. Unknown filters, functions and missing parents depend on what the host registers, so they are left to the host, which checks them when it loads the templates.

| Rule | Finds | Level |
| --- | --- | --- |
| `tera/no-debug` | `__tera_context`, which prints every value the template was given | `error` |
| `tera/no-safe-filter` | `\| safe`, which turns escaping off | `warn` |
| `tera/no-unused-set` | a `{% set %}` nothing reads; `set_global` is not checked | `warn` |

```jinja
{# fsr-lint-disable-next-line tera/no-safe-filter -- rendered from our own markdown #}
<div>{{ html | safe }}</div>
```

## Applying Fixes

`fix_source` applies every safe fix a rule offers and returns the new text; `fix_dir` does it for each file of a walk and returns the files it changed. Neither writes anything.

```rust
let fixed = linter.fix_source(Path::new("a.ts"), "export const p = /a  b/;\n");
assert_eq!(fixed.text, "export const p = /a {2}b/;\n");
assert_eq!(fixed.applied, 1);

for (path, fixed) in linter.fix_dir(Path::new("app")) {
  std::fs::write(&path, &fixed.text)?;
}
```

Only fixes a rule marks safe are applied, not suggestions. A finding a comment silences or a rule turned off is not fixed. Passes repeat until nothing changes, at most ten. `fsr lint --fix` writes the files and then reports what is left.

## Translating an ESLint Configuration

```rust
use snapfire_lint::eslint;

let rc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(".eslintrc.json")?)?;
let translated = eslint::translate_eslintrc(&rc, &snapfire_lint::rules());
std::fs::write("fsr.lint.toml", &translated.toml)?;
for note in &translated.notes {
  println!("{note}");
}

let flat = eslint::translate_flat(&std::fs::read_to_string("eslint.config.js")?, Path::new("eslint.config.js"), &snapfire_lint::rules());
```

`translate_eslintrc` takes an `.eslintrc` as JSON or `package.json`'s `eslintConfig`: `rules`, `overrides`, `ignorePatterns` and `settings` carry over. `translate_flat` reads an `eslint.config.js`, `.mjs` or `.ts` without running it: an `export default` list of objects written out literally, also when one call wraps the list, each with `files`, `ignores`, `rules` and `settings`. A preset whose rules run by default, such as `eslint:recommended` or `js.configs.recommended`, needs nothing written. A preset of unicorn, promise or jsdoc, `plugin:unicorn/recommended` or `unicorn.configs.recommended`, is written out as its rules at the level the preset gives each, since those rules are off by default; a rule the configuration sets after it wins. eslint-plugin-vue's `vue3-strongly-recommended` and `vue3-recommended` are written out the same way, at `warn` (`error` in their `-error` forms). The rule list is what a rule is checked against. Pass `snapfire_lint::rules()`. A linter that runs `Check`s of its own passes its `rules()` instead, so their rules carry over too. Every rule not in the list, every other preset, `env`, `globals`, `parser` and any part of a flat config that is not a literal is a note. `fsr migrate` runs this over the application it migrates and writes `fsr.lint.toml`.

## Adding a Rule of Your Own

A `Check` sees each script, a whole file or one `<script>` block, with its path, source and oxc's semantic model. It reports a byte offset and a message. Levels, overrides and suppression comments apply to it as to any rule.

```rust
use snapfire_lint::oxc_ast::AstKind;
use snapfire_lint::{Check, Config, File, Level, Linter, RuleInfo};

struct NoFoo;

impl Check for NoFoo {
  fn info(&self) -> RuleInfo {
    RuleInfo::new("x/no-foo", Level::Warn, "correctness", "### What it does\n\nDisallows `foo`.\n")
  }

  fn check(&self, file: &File<'_>, found: &mut dyn FnMut(usize, String)) {
    for node in file.semantic.nodes().iter() {
      if let AstKind::IdentifierReference(ident) = node.kind()
        && ident.name == "foo"
      {
        found(ident.span.start as usize, "`foo` is not allowed".to_owned());
      }
    }
  }
}

let linter = Linter::new_with(Config::default(), vec![Box::new(NoFoo)])?;
```

`oxc_ast`, `oxc_semantic` and `oxc_span` are re-exported so a check uses the same oxc as the crate. `Linter::rules` lists the checks beside the crate's rules. `snapfire_fsr_lint` holds FSR's checks this way.

## Error Handling

`Config::parse`, `Config::load`, `Config::discover`, `Config::ignore`, `Linter::new` and `Linter::new_with` return `ConfigError`. A finding is never an error; it is in the report.

```rust
use snapfire_lint::ConfigError;

match Linter::new(config) {
  Ok(linter) => linter,
  Err(ConfigError::UnknownRule { path, rule }) => panic!("{}: no rule {rule}", path.display()),
  Err(ConfigError::Options { rule, message, .. }) => panic!("{rule}: {message}"),
  Err(e) => panic!("{e}"),
};
```
