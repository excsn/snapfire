# snapfire_fsr_lower

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

The recogniser for SnapFire FSR. It reads a TypeScript loader or actions module with the swc parser snapfirec uses and lowers each body to the IR in `snapfire_fsr_ir`, so the body runs in Rust with no JavaScript engine. A body that uses anything outside the IR is residue, reported with the file, the line, the column and the construct, so the developer knows exactly what stopped it. It also reads schema modules, exported interfaces in the subset the contract holds, into contract types, which is how a session schema or an action input declared in TypeScript reaches the runtime. The recogniser follows the syntax, not a type checker: every read is typed by where it comes from on `ctx`, which is what makes a syntactic pass sufficient for the bodies it accepts. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_lower = { path = "../lower" }
```

The crate has no Cargo features. It depends on `snapfire_fsr_ir` for the tree it produces, on `snapfire_fsr_service` for the contract types a schema becomes and on `swc_core` with `common`, `ecma_ast` and `ecma_parser` for parsing.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Lower a `loader.ts` with an exported `load` | `lower_loader` |
| Lower an `actions.ts` with `export const x = action(...)` entries | `lower_actions` |
| Read the input type an action names in `action<T>(...)` | `LoweredAction::input` |
| Turn `schemas/*.ts` interfaces into contract types | `read_schema` |
| Read `export const defaults` from the session schema | `read_session_defaults` |
| Lower with defaults folded into session reads | `lower_loader_with`, `lower_actions_with` |
| Tell residue from a parse error or a missing export | `LowerError` |
| Print where a body stopped being IR | `Residue` and its `Display` |

## Status

Pre-release and unpublished, with no stability guarantee on any signature here. The five bodies of the `shopping_react_ts` example lower to exactly the IR the interpreter's own tests hand-write, which `tests/shopping.rs` asserts, alongside residue cases for an unfollowable import, `try`, a lambda with statements and a write outside the session. The `fsr` binary in `snapfire_fsr_cli` is the only caller.
