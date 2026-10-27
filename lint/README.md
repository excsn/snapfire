# snapfire_lint

[![Crates.io](https://img.shields.io/crates/v/snapfire_lint.svg)](https://crates.io/crates/snapfire_lint)
[![Docs.rs](https://docs.rs/snapfire_lint/badge.svg)](https://docs.rs/snapfire_lint)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

A linter for TypeScript and JavaScript that needs no Node: ESLint's rules, ESLint's levels and ESLint's suppression comments, for development and CI. It reads `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs`, `.mts` and `.cts` files, plus the `<script>` blocks of `.vue`, `.svelte` and `.astro` files. The rules are ESLint's 61 recommended rules, ported from [oxlint](https://oxc.rs/docs/guide/usage/linter) with their tests and run on oxc's parser and semantic model. Levels live in `fsr.lint.toml`. `fsr lint` is the command that runs it over an application. This crate is new and has ESLint's core rules only: React, jsx-a11y, TypeScript and template rules come later. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_lint = "0.1"
```

No features.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Lint an FSR application | `fsr lint app` |
| Lint a directory from Rust | `Config::discover`, `Linter::new`, `Linter::lint_dir` |
| Lint one source string | `Linter::lint_source` |
| Change a rule's level or options | `[rules]` in `fsr.lint.toml` |
| Change levels for some files | `[[overrides]]` with `files` globs |
| Skip vendored or generated files | `ignores` in the file; `Config::ignore` from Rust |
| Silence one finding in source | `// fsr-lint-disable-next-line <rule> -- reason` or ESLint's comment |
| Fail CI on warnings too | `fsr lint --deny-warnings`; `Report::failed(true)` from Rust |
| Feed findings to another tool | `fsr lint --json`; `Report::to_json` from Rust |
| List every rule and its default | `fsr lint --rules`; `rules()` from Rust |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file for the full license text. The rules, the rule framework under `src/js` and the `<script>` loader are derived from oxlint, Copyright (c) 2024-present VoidZero Inc. & Contributors, under the MIT License in `LICENSE-oxc`; each derived file says so at its top.
