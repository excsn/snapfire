# snapfire_fsr_cli

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

`fsr`, the build tool for a Snapfire FSR application. It walks `app/routes/`, turns the directory convention into routes, lowers every `loader.ts` and `actions.ts` to the IR and writes `app/plan.json`, the file the host reads at boot. It also builds the contract from the OpenAPI documents under `app/clients/` and the interfaces under `app/schemas/`. It writes the TypeScript a body is written against: `generated/services.d.ts`, `generated/fsr.ts` with `Ctx`, `ActionCtx`, `action` and `fail`, plus `generated/contract.json` for the host `generated/islands.ts`, the island registry for every module discovery named, plus `generated/client.ts`, the types a page imports: the contract in client flavour, each page's props inferred from its loader and one typed callable per action. The app maps `@snapfire/fsr` to that generated module in its `tsconfig.json`, so bodies import from the bare name. It compiles nothing. snapfirec builds the browser modules, while `fsr` only reads the TypeScript that runs on the server. The library half exposes the same build for tests and for a host that wants to run it in process. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install --path fsr/cli
```

The crate has no Cargo features. It depends on `snapfire_fsr_lower` for the recogniser, `snapfire_fsr_ir` for the bodies and `snapfire_fsr_plan` for the file it writes.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Emit the plan file for an application | `fsr build <app>` |
| See what a build would emit without writing | `fsr check <app>` |
| Name the document module or the slot pages land in | `--shell`, `--slot` |
| Give bodies a typed `services` | a document under `app/clients/<name>.openapi.json` |
| Give bodies a typed `session` or an action a typed input | an interface under `app/schemas/` |
| Give a fresh session its starting values | `export const defaults` beside `Session` |
| Register the page islands in the browser | `generated/islands.ts`, called from `main.ts` |
| Type a page's props or call an action from a page | `generated/client.ts` |
| Mount pages with something other than React | `Options::mounter_module` and `Options::mounter` |
| Run the build from Rust | `build` and `write` |
| Read what was discovered, imported and lowered | `Report` |

## Status

Pre-release and unpublished. `shopping_react_ts` is built with it: its three loaders and two actions are TypeScript under `app/routes/`, typed by `generated/fsr.ts`. `tsc --strict` passes over the whole app. `app/plan.json` and `app/generated/` are the checked-in output. Layouts, per-route `loading.tsx` and `error.tsx` are read into the plan but no example exercises them yet. Residue stops a build with the diagnostic, since no engine exists to run it.
