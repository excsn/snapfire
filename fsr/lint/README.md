# snapfire_fsr_lint

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_lint.svg)](https://crates.io/crates/snapfire_fsr_lint)
[![Docs.rs](https://docs.rs/snapfire_fsr_lint/badge.svg)](https://docs.rs/snapfire_fsr_lint)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

FSR's own lint rules and the levels FSR sets under a project's, as `Check`s and a base `Config` for `snapfire_lint`. `fsr lint` runs them. The rules know what only FSR knows: that every page renders on the server first, which files only the server runs, what `@snapfire/fsr/host`'s `file` is and what name snapfirec gives each output. This crate is new. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_lint = "0.1"
```

No features.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Lint an FSR application with FSR's rules | `fsr lint app` |
| Run FSR's rules from Rust | `Linter::new_with(config.over(base()), checks())` |
| Find imports of packages vendored only for another | `ExtraneousDependencies::new(vendored_for)` beside `checks()` |
| Read what a rule finds | `fsr lint --rules`, then each rule's docs |
| List every rule `fsr lint` runs from Rust | `rules()` |
