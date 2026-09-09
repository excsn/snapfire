# snapfire_fsr_cli

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_cli.svg)](https://crates.io/crates/snapfire_fsr_cli)
[![Docs.rs](https://docs.rs/snapfire_fsr_cli/badge.svg)](https://docs.rs/snapfire_fsr_cli)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

`fsr`, the build tool for a SnapFire FSR application. It walks `app/routes/`, turns the directory convention into routes, lowers every `page.loader.ts` and `actions.ts` to the IR and writes `app/generated/plan.sexp`, the file the host reads at boot. `fsr doctor` reads that plan and the configuration back and reports what starts and serves without doing what it says, each finding with its remedy. It also builds the contract from the OpenAPI documents and `.proto` files under `app/clients/` and the interfaces under `app/schemas/`. It writes the TypeScript a body is written against: `generated/services.d.ts`, `generated/fsr.ts` with `Ctx`, `ActionCtx`, `action` and `fail`, plus `generated/contracts/`, one contract file per client document and one for the schemas, which the host merges at boot, `generated/islands.ts`, the island registry for every module discovery named, plus `generated/client.ts`, the types a page imports: the contract in client flavour, each page's props inferred from its loader and one typed callable per action. It writes the app's `tsconfig.json`, mapping `@snapfire/fsr` to that generated module and every package under `types/` to its declarations, plus `tsconfig.build.json` for snapfirec. `fsr add` vendors a package's runtime modules from esm.sh into `vendor/` and points the import map at them; `fsr types` fetches the declarations of every package the import map names into `types/` from the package or DefinitelyTyped and writes the fsr packages' own. An application with an `xwpm.wmf` is xwpm's: both commands delegate to it and the build reads the directories it names. No npm, no `node_modules`. It compiles nothing. snapfirec builds the browser modules, while `fsr` only reads the TypeScript that runs on the server. The library half exposes the same build for tests and for a host that wants to run it in process. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install --path fsr/cli
```

It depends on `snapfire_fsr_lower` for the recogniser, `snapfire_fsr_ir` for the bodies, `snapfire_fsr_plan` for the file it writes and `reqwest` with rustls for esm.sh and the npm registry.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Emit the whole artifact: the plan, the generated types, the tsconfigs and the browser bundle | `fsr build <app>` |
| See what a build would emit without writing | `fsr check <app>` |
| Check a deployment for what the host will not refuse to start over | `fsr doctor <app>` |
| Vendor a package for the browser | `fsr add <app> react@18.3.1` |
| Fetch declarations for the editor and `tsc` | `fsr types <app>` |
| Keep an xwpm application in step | an `xwpm.wmf` in the app; the same commands |
| Name the document module or the slot pages land in | `--shell`, `--slot` |
| Give bodies a typed `services` | `app/clients/<name>.openapi.json` or `app/clients/<name>.proto` |
| Give bodies a typed `session` or an action a typed input | an interface under `app/schemas/` |
| Give a fresh session its starting values | `export const defaults` beside `Session` |
| Register the page islands in the browser | `generated/islands.ts`, called from `main.ts` |
| Type a page's props or call an action from a page | `generated/client.ts` |
| Mount pages with something other than React | `Options::mounter_module` and `Options::mounter` |
| Run an application with no Rust beside it | `fsr serve <app>` or `fsr dev <app>` with no `Cargo.toml` beside it |
| Build one team's application as a site another mounts under a path | a `[site]` section beside the app; `fsr serve` on the shell mounts its `[sites]` table |
| Hold a mounted site to the bytes you meant to ship | `fsr sites install` pins what it installed; `fsr sites pin <shell>` repins |
| Run the build from Rust | `build` and `write` |
| Read what was discovered, imported and lowered | `Report` |
