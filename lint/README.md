# snapfire_lint

[![Crates.io](https://img.shields.io/crates/v/snapfire_lint.svg)](https://crates.io/crates/snapfire_lint)
[![Docs.rs](https://docs.rs/snapfire_lint/badge.svg)](https://docs.rs/snapfire_lint)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

A linter that needs no Node, for development and CI: ESLint's rules, ESLint's levels and ESLint's suppression comments. It reads TypeScript and JavaScript in every dialect with the recommended rules of ESLint, eslint-plugin-react with its hooks rules, jsx-a11y and typescript-eslint, plus six rules ESLint configurations often turn on, such as `no-console` and `import/no-anonymous-default-export`. It has the rules of eslint-plugin-unicorn, eslint-plugin-promise and eslint-plugin-jsdoc too, off until a configuration names them. All 356 are ported from [oxlint](https://oxc.rs/docs/guide/usage/linter) with their tests and run on oxc. It reads a Vue component's script with eslint-plugin-vue's rules as oxlint ports them, its `vue3-essential` preset on by default, its template with four rules of that set and Svelte components with every warning Svelte's compiler gives. It reads Markdown and MDX with `@eslint/markdown`'s recommended correctness rules. It reads Tera templates with tera's own parser. Levels live in `fsr.lint.toml`. It applies safe fixes, writes SARIF for CI and translates an ESLint configuration. Rules from outside the crate plug in as a `Check`. `fsr lint` is the command that runs it over an application, with FSR's own rules added. This crate is new. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

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
| Give the React or jsx-a11y rules their settings | `[settings]` in `fsr.lint.toml` |
| Skip vendored or generated files | `ignores` in the file; `Config::ignore` from Rust |
| Silence one finding in source | `// fsr-lint-disable-next-line <rule> -- reason` or ESLint's comment |
| Lint Vue and Svelte templates | `Linter::with_compilers(Box::new(Plugins::default()))` |
| Apply the safe fixes | `fsr lint --fix`; `Linter::fix_dir` from Rust |
| Fail CI on warnings too | `fsr lint --deny-warnings`; `Report::failed(true)` from Rust |
| Annotate a pull request | `fsr lint --sarif`; `Report::to_sarif` from Rust |
| Feed findings to another tool | `fsr lint --json`; `Report::to_json` from Rust |
| Bring an ESLint configuration over | `fsr migrate`; `eslint::translate_eslintrc` or `eslint::translate_flat` from Rust |
| Add a rule of your own | `Check` with `Linter::new_with` |
| List every rule and its default | `fsr lint --rules`; `rules()` from Rust |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file for the full license text. The JavaScript and TypeScript rules, the rule framework under `src/js` and the `<script>` loader are derived from oxlint, Copyright (c) 2024-present VoidZero Inc. & Contributors, under the MIT License in `LICENSE-oxc`; each derived file says so at its top. The templates under `tests/tera_inputs` are tera's own, Copyright (c) 2025 Vincent Prouillet, under the MIT License in `tests/tera_inputs/LICENSE`.
