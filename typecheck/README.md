# snapfire_typecheck

MPL-2.0.

Typechecking for TypeScript, as a peer process rather than a library inside a compiler. `snapfirec` strips types and never reads one for meaning; this crate puts a `tsc` of a requested version on the machine and runs it over the same tsconfig, so a wrong prop type or a misspelled field is a build failure instead of something the browser discovers. The engine is TypeScript 7, one self-contained native binary per platform, fetched from the npm registry over plain HTTPS and verified against a hash before it runs: no npm client, no Node and no package manager. The library is the resolution and the diagnostics; `snapfiretc` is the executable `fsr` and `snapfirec` spawn. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_typecheck = { path = "../typecheck" }
```

```sh
cargo install --path typecheck
```

No features. The crate depends on `reqwest` with rustls for the one HTTPS GET, `flate2` and `tar` for the tarball, `sha2` and `base64` for the integrity check and `serde_json` for the registry document and the JSON report.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Check a project from the command line | `snapfiretc --root <dir> --config tsconfig.json` |
| Check one from Rust | `resolve` then `check` |
| Pin the TypeScript a project checks with | `--tsc-version`, else `[typecheck] version` in an application's configuration |
| Use a compiler you already have | `--tsc <path>`, whose `--version` must be the requested one |
| Find out which compiler would run, without checking | `snapfiretc --which` |
| Put the cache somewhere else | `--cache <dir>` or `$SNAPFIRE_CACHE` |
| Read diagnostics from a tool | `--format json` |
| Refuse to fetch on a machine with no network | `--offline` |

## Status

Active. Not yet run on Windows.
