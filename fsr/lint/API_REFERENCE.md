# API Reference: snapfire_fsr_lint

FSR's lint rules and the base rule levels FSR sets under a project's, for `snapfire_lint`.

## Contents

* [1. Configuration and Checks](#1-configuration-and-checks)
  * [checks](#checks)
  * [base](#base)
  * [base_for](#base_for)
  * [rules](#rules)
  * [ExtraneousDependencies](#extraneousdependencies)
* [2. Rules](#2-rules)
  * [fsr/no-browser-globals-in-render](#fsrno-browser-globals-in-render)
  * [fsr/no-file-read-in-render](#fsrno-file-read-in-render)
  * [fsr/no-focused-tests](#fsrno-focused-tests)
  * [fsr/no-module-state-in-render](#fsrno-module-state-in-render)
  * [fsr/output-name-clash](#fsroutput-name-clash)
  * [import/no-extraneous-dependencies](#importno-extraneous-dependencies)

## 1. Configuration and Checks

### checks

* `pub fn checks() -> Vec<Box<dyn snapfire_lint::Check>>`: the five `fsr/` rules: `fsr/no-browser-globals-in-render`, `fsr/no-file-read-in-render`, `fsr/no-module-state-in-render`, `fsr/no-focused-tests` and `fsr/output-name-clash`.

### base

* `pub fn base() -> snapfire_lint::Config`: `react/no-unknown-property` at `error` with `ignore` listing htmx 2's and htmx 4's attributes. For `routes/**/*.md` and `routes/**/*.mdx`, `markdown/no-multiple-h1` and `markdown/heading-increment` at `error` with `frontmatterTitle = ""`. `base()` is `base_for(true)`.

### base_for

* `pub fn base_for(react: bool) -> snapfire_lint::Config`: `base()`, with `react/rules-of-hooks` and `react/exhaustive-deps` set to `off` when `react` is `false`.

### rules

* `pub fn rules() -> Vec<snapfire_lint::RuleInfo>`: every rule `fsr lint` runs, `snapfire_lint`'s, the five of `checks()` and `import/no-extraneous-dependencies`, at the level `base()` gives it.

### ExtraneousDependencies

The rule `import/no-extraneous-dependencies`. Implements `snapfire_lint::Check`.

* `pub struct ExtraneousDependencies { /* private */ }`
* `pub fn new(vendored_for: BTreeMap<String, Vec<String>>) -> Self`: maps each package or module vendored only as a dependency to the packages that import it, empty when they are not known. With an empty map it finds nothing.

## 2. Rules

### fsr/no-browser-globals-in-render

* Default `error`, category `correctness`.
* Files: `.tsx`, `.jsx` and server modules. A server module's file name holds `.loader.` or starts with `actions.`, `route.` or `middleware.`.
* Finds: an unbound reference to `alert`, `cancelAnimationFrame`, `confirm`, `document`, `getComputedStyle`, `history`, `innerHeight`, `innerWidth`, `IntersectionObserver`, `localStorage`, `location`, `matchMedia`, `MutationObserver`, `navigator`, `prompt`, `requestAnimationFrame`, `ResizeObserver`, `screen`, `sessionStorage` or `window`. In a server module, anywhere. Otherwise when its nearest enclosing function, skipping callbacks of `useMemo`, `useState` and `useReducer`, is a component or there is none. A component is a function named with a capital or a module's default export.
* Skips: the operand of `typeof`. A reference under an `if`, a `?:` test or a `&&` left side whose source holds `typeof `.

### fsr/no-file-read-in-render

* Default `error`, category `perf`.
* Files: `.tsx` and `.jsx` that are not server modules.
* Finds: a call `file.<method>(...)` whose `file` is bound by `import { file } from "@snapfire/fsr/host"`, when its nearest enclosing function, skipping callbacks of `useMemo`, `useState` and `useReducer`, is a component or there is none.

### fsr/no-focused-tests

* Default `error`, category `correctness`.
* Files: every script.
* Finds: `.only` read off a `test`, `it` or `describe` imported from `@snapfire/fsr/testing` or `@snapfire/fsr/client/testing`, under any local name, `test.only.each` among them. Also every use of `fit` or `fdescribe` imported from them.
* Skips: `.skip`, `.todo` and the other members. A `test` bound to anything else.

### fsr/no-module-state-in-render

* Default `error`, category `correctness`.
* Files: `.tsx` and `.jsx` that are not server modules.
* Finds: a write to a variable declared at the module's top level, where the function it runs in, callbacks of `useMemo`, `useState` and `useReducer` skipped, is a component. And a call, placed in render the same way, to a top-level function or `const` arrow of the file that writes such a variable.
* Skips: a write at the module's top level, `x ??= v`, `x ||= v` and a write under an `if` whose test is `!x`, `x == null`, `x === undefined`, `x === null` or `typeof x === "undefined"` for a top-level `x`. Changing an object a `const` holds is not a write.

### fsr/output-name-clash

* Default `error`, category `correctness`.
* Files: every script except `.d.ts`, `.d.mts`, `.d.cts`, `.vue` and `.svelte` files.
* Output name: the file name lowercased with `.tsx`, `.jsx`, `.ts`, `.js`, `.mts`, `.mjs`, `.cts` or `.cjs` replaced by `.js`.
* Finds: another file in the same directory with the same output name. Reported once, on the file whose lowercased name sorts after the other's, at offset 0. The directory is listed from disk. `lint_source` on a path that does not exist finds nothing.

### import/no-extraneous-dependencies

* Default `warn`, category `suspicious`.
* Files: every script.
* Finds: the source of an `import` declaration, an `export ... from`, an `export * from` or an `import()` with a string literal, when it equals a name in the map or sits under it followed by `/`. The message names the packages that import it.
* Skips: `import type`, an import whose specifiers are all `type`, `export type ... from` and `export type * from`.
