# API Reference: snapfire_fsr_lint

FSR's lint rules and base levels for `snapfire_lint`.

## Contents

* [1. Functions](#1-functions)
  * [checks](#checks)
  * [base](#base)
* [2. Rules](#2-rules)
  * [fsr/no-browser-globals-in-render](#fsrno-browser-globals-in-render)
  * [fsr/no-file-read-in-render](#fsrno-file-read-in-render)
  * [fsr/output-name-clash](#fsroutput-name-clash)

## 1. Functions

### checks

* `pub fn checks() -> Vec<Box<dyn snapfire_lint::Check>>`: the three rules of section 2, for `Linter::new_with`.

### base

* `pub fn base() -> snapfire_lint::Config`: `react/no-unknown-property` at `error` with `ignore` listing htmx 2's and htmx 4's attributes. Put a project's configuration over it with `Config::over`.

## 2. Rules

### fsr/no-browser-globals-in-render

* Default `error`, category `correctness`.
* Files: `.tsx` and `.jsx`; and server modules, whose names hold `.loader.` or start with `actions.`, `route.` or `middleware.`.
* Finds: an unbound reference to `alert`, `cancelAnimationFrame`, `confirm`, `document`, `getComputedStyle`, `history`, `innerHeight`, `innerWidth`, `IntersectionObserver`, `localStorage`, `location`, `matchMedia`, `MutationObserver`, `navigator`, `prompt`, `requestAnimationFrame`, `ResizeObserver`, `screen`, `sessionStorage` or `window`. In a server module, anywhere. Otherwise when its nearest enclosing function, skipping callbacks of `useMemo`, `useState` and `useReducer`, is a component or there is none. A component is a function named with a capital or a module's default export.
* Skips: the operand of `typeof`; a reference under an `if`, a `?:` test or a `&&` left side whose source holds `typeof `.

### fsr/no-file-read-in-render

* Default `error`, category `perf`.
* Files: `.tsx` and `.jsx` that are not server modules.
* Finds: a call `file.<method>(...)` whose `file` is bound by `import { file } from "@snapfire/fsr/host"`, placed in render as above.

### fsr/output-name-clash

* Default `error`, category `correctness`.
* Files: every script except `.d.ts`, `.d.mts` and `.d.cts`; `.vue` and `.svelte` components are skipped.
* Output name: the file name lowercased with `.tsx`, `.jsx`, `.ts`, `.js`, `.mts`, `.mjs`, `.cts` or `.cjs` replaced by `.js`.
* Finds: another file in the same directory with the same output name. Reported once, on the file whose lowercased name sorts after the other's, at offset 0. The directory is listed from disk, so `lint_source` on a path that does not exist finds nothing.
