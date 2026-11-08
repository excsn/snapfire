# snapfire_lint

[![Crates.io](https://img.shields.io/crates/v/snapfire_lint.svg)](https://crates.io/crates/snapfire_lint)
[![Docs.rs](https://docs.rs/snapfire_lint/badge.svg)](https://docs.rs/snapfire_lint)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

A linter for TypeScript, JavaScript, Vue, Svelte, Markdown, MDX and Tera templates that needs no Node.js. It uses ESLint's rule names, ESLint's levels and ESLint's suppression comments, so an existing ESLint setup carries over.

It has 356 script rules ported from [oxlint](https://oxc.rs/docs/guide/usage/linter) together with their tests.

These rule sets run without any configuration:

* the recommended rules of ESLint, eslint-plugin-react, jsx-a11y and typescript-eslint
* eslint-plugin-vue's `vue3-essential` script and template rules
* every warning Svelte's compiler gives
* `@eslint/markdown`'s recommended rules, for Markdown and MDX
* Tera syntax checks, using Tera's own parser

The eslint-plugin-unicorn, eslint-plugin-promise and eslint-plugin-jsdoc rules are there too, off until a configuration turns them on.

Levels live in `fsr.lint.toml`. The linter can apply safe fixes, write SARIF for code review and translate an ESLint configuration. `fsr lint` runs it over an FSR application with FSR's own rules added.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_lint = "0"
```

No features.

## What to reach for

| Problem | Piece |
| --- | --- |
| Lint an FSR application | `fsr lint app` |
| Lint a directory from Rust | `Config::discover`, `Linter::new`, `Linter::lint_dir` |
| Lint one source string | `Linter::lint_source` |
| Change a rule's level or options | `[rules]` in `fsr.lint.toml` |
| Change levels for some files | `[[overrides]]` with `files` globs |
| Give the React or jsx-a11y rules their settings | `[settings]` in `fsr.lint.toml` |
| Skip vendored or generated files | `ignores` in the file. `Config::ignore` from Rust |
| Silence one finding in source | `// fsr-lint-disable-next-line <rule> -- reason` or ESLint's comment |
| Lint Vue and Svelte templates | `Linter::with_compilers(Box::new(Plugins::default()))` |
| Apply the safe fixes | `fsr lint --fix`. From Rust, `Linter::fix_dir` |
| Fail CI on warnings too | `fsr lint --deny-warnings`. From Rust, `Report::failed(true)` |
| Annotate a pull request | `fsr lint --sarif`. From Rust, `Report::to_sarif` |
| Feed findings to another tool | `fsr lint --json`. From Rust, `Report::to_json` |
| Bring an ESLint configuration over | `fsr migrate`. From Rust, `eslint::translate_eslintrc` or `eslint::translate_flat` |
| Add a rule of your own | `Check` with `Linter::new_with` |
| List every rule and its default | `fsr lint --rules`. From Rust, `rules()` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](LICENSE) file for the full license text.

The JavaScript and TypeScript rules, the rule framework under `src/js` and the `<script>` loader are derived from oxlint, Copyright (c) 2024-present VoidZero Inc. & Contributors, under the MIT License in [LICENSE-oxc](LICENSE-oxc). Each derived file says so at its top.

The templates under `tests/tera_inputs` are Tera's own, Copyright (c) 2025 Vincent Prouillet, under the MIT License in `tests/tera_inputs/LICENSE`.
