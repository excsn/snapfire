# snapfire_typecheck

[![Crates.io](https://img.shields.io/crates/v/snapfire_typecheck.svg)](https://crates.io/crates/snapfire_typecheck)
[![Docs.rs](https://docs.rs/snapfire_typecheck/badge.svg)](https://docs.rs/snapfire_typecheck)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Typechecks a TypeScript project with a real `tsc` of the version you ask for, without Node.js or npm. A wrong prop type or a misspelled field fails the build instead of turning up in the browser.

`snapfirec` strips types without checking them. This crate supplies the check. It finds a `tsc` of the requested version, runs it over the same `tsconfig.json` and hands back the diagnostics with TypeScript's own codes.

The compiler is TypeScript 7, a single native binary per platform. When it is not already cached or on `PATH`, it is downloaded from the npm registry over HTTPS and checked against a known hash before it runs.

Most people meet it as `snapfiretc`, the command `snapfirec --typecheck` and `fsr` run for you. Use the library to run the same check from your own Rust tool.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install snapfire_typecheck
```

That puts `snapfiretc` on `PATH`. As a library:

```toml
[dependencies]
snapfire_typecheck = "0"
```

No features.

## Quick start

```sh
snapfiretc --root ./my-app --config tsconfig.json
```

```text
src/cart.ts(12,7): error TS2322: Type 'string' is not assignable to type 'number'.
tsc 7.0.2 from cache
```

The same check from Rust:

```rust
use snapfire_typecheck::{check, resolve, Options};
use std::path::Path;

let resolved = resolve(&Options::default())?;
let diagnostics = check(&resolved.tsc, Path::new("./my-app"), Path::new("tsconfig.json"))?;
for diagnostic in diagnostics.iter().filter(|d| d.is_error()) {
  println!("{diagnostic}");
}
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Check a project from the command line | `snapfiretc --root <dir> --config tsconfig.json` |
| Check one from Rust | `resolve` then `check` |
| Pin the TypeScript a project checks with | `--tsc-version`, else `[typecheck] version` in an application's configuration |
| Use a compiler you already have | `--tsc <path>`, whose `--version` must be the requested one |
| Find out which compiler would run, without checking | `snapfiretc --which` |
| Put the cache somewhere else | `--cache <dir>` or `$SNAPFIRE_CACHE` |
| Read diagnostics from a tool | `--format json` |
| Refuse to fetch on a machine with no network | `--offline` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the [LICENSE](LICENSE) file for the full license text.
