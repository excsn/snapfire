# snapfire_fsr_lint

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_lint.svg)](https://crates.io/crates/snapfire_fsr_lint)
[![Docs.rs](https://docs.rs/snapfire_fsr_lint/badge.svg)](https://docs.rs/snapfire_fsr_lint)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The lint rules that catch mistakes specific to FSR applications, added on top of the general rules in [`snapfire_lint`](../../lint/README.md).

Every FSR page renders on the server first, so some code that is fine in a browser-only app breaks there. These rules find that code before it ships:

| Rule | Finds |
| --- | --- |
| `fsr/no-browser-globals-in-render` | `window`, `document`, `localStorage` and other browser-only globals read while a component renders or in a server-only file |
| `fsr/no-file-read-in-render` | a read through `@snapfire/fsr/host`'s `file` while a component renders |
| `fsr/no-module-state-in-render` | a component render that writes a variable declared at the top of its module |
| `fsr/no-focused-tests` | a `test.only`, `it.only` or `describe.only` from `@snapfire/fsr/testing` left in |
| `fsr/output-name-clash` | two files in one directory that compile to the same output name, such as `format.ts` and `Format.js` |
| `import/no-extraneous-dependencies` | an import of a package the app only has because another package needs it |

The crate also sets FSR's defaults under a project's own configuration. It allows htmx attributes on JSX elements. In a route's markdown page it does not count the front matter title as the first heading.

Most applications never depend on this crate. Run `fsr lint app` instead. Use it directly when you run the linter from your own Rust tooling.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_lint = "0.1"
```

The crate has no features.

## Quick start

```rust
use snapfire_lint::{Config, Linter};

let config = Config::discover(project_root)?.over(snapfire_fsr_lint::base());
let linter = Linter::new_with(config, snapfire_fsr_lint::checks())?;
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Lint an FSR application with FSR's rules | `fsr lint app` |
| Run FSR's rules from Rust | `Linter::new_with(config.over(base()), checks())` |
| Find imports of packages vendored only for another | `ExtraneousDependencies::new(vendored_for)` beside `checks()` |
| Lint an application without React | `base_for(false)`, which turns React's hooks rules off |
| Read what a rule finds | `fsr lint --rules`, then each rule's docs |
| List every rule `fsr lint` runs from Rust | `rules()` |
