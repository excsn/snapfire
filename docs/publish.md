# Publishing

## Workspaces

Two of them. Run `cargo publish -p <name>` from the right one.

* repository root: `compiler`, `compiler/wire`, `compiler/vue`, `web`, `typecheck`
* `fsr/`: the sixteen `snapfire_fsr*` crates

Examples are not published.

## One at a time

Never run two publishes at once and never background one to start another.

`cargo publish` returns only once the crate is live in the registry index. That wait is what the next crate's verification build needs.

## Why the order is forced

`cargo publish` verifies the packaged tarball against the registry, not against the path dependency it built with. A dependency bumped on disk but not yet published fails the verification with unresolved imports, uploading nothing.

## Order

Dependency order. Publish only the crates whose version changed, keeping the relative order of the rest.

From `fsr/`:

1. `snapfire_fsr_core`
2. `snapfire_fsr_macros`
3. `snapfire_fsr_engine`
4. `snapfire_fsr_payload`
5. `snapfire_fsr_runtime`
6. `snapfire_fsr_session`
7. `snapfire_fsr_service`
8. `snapfire_fsr_ir`
9. `snapfire_fsr_plan`
10. `snapfire_fsr_lower`
11. `snapfire_fsr_auth`
12. `snapfire_fsr_tera`
13. `snapfire_fsr`
14. `snapfire_fsr_host`
15. `snapfire_fsr_sites`

From the repository root:

16. `snapfire_compiler_wire`
17. `snapfire_vue`
18. `snapfire_compiler`

Back in `fsr/`:

19. `snapfire_fsr_cli`

The cli is last because it is the only crate reaching across both workspaces: it needs `snapfire_compiler_wire` live first.

`snapfire` and `snapfire_typecheck` depend on nothing here and publish at any time.

## Checking

```sh
curl -s https://crates.io/api/v1/crates/<name> | sed -n 's/.*"max_version":"\([^"]*\)".*/\1/p'
```

A version matching the crate's own `Cargo.toml` is already published and cargo refuses it.

## Before publishing

The `[workspace.dependencies]` requirements match each crate's own `version`. `Cargo.lock` matches every manifest. Commit and push, since a published version is permanent.
