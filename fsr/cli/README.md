# snapfire_fsr_cli

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

`fsr`, the build tool for a SnapFire FSR application. It walks `app/routes/`, turns the directory convention into routes, lowers every `page.loader.ts` and `actions.ts` to the IR and writes `app/generated/plan.json`, the file the host reads at boot. It also builds the contract from the OpenAPI documents and `.proto` files under `app/clients/` and the interfaces under `app/schemas/`. It writes the TypeScript a body is written against: `generated/services.d.ts`, `generated/fsr.ts` with `Ctx`, `ActionCtx`, `action` and `fail`, plus `generated/contracts/`, one contract file per client document and one for the schemas, which the host merges at boot, `generated/islands.ts`, the island registry for every module discovery named, plus `generated/client.ts`, the types a page imports: the contract in client flavour, each page's props inferred from its loader and one typed callable per action. It writes the app's `tsconfig.json`, mapping `@snapfire/fsr` to that generated module and every package under `types/` to its declarations, plus `tsconfig.build.json` for snapfirec. `fsr add` vendors a package's runtime modules from esm.sh into `vendor/` and points the import map at them; `fsr types` fetches the declarations of every package the import map names into `types/` from the package or DefinitelyTyped and writes the fsr packages' own. An application with an `xwpm.wmf` is xwpm's: both commands delegate to it and the build reads the directories it names. No npm, no `node_modules`. It compiles nothing. snapfirec builds the browser modules, while `fsr` only reads the TypeScript that runs on the server. The library half exposes the same build for tests and for a host that wants to run it in process. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```sh
cargo install --path fsr/cli
```

The crate has no Cargo features. It depends on `snapfire_fsr_lower` for the recogniser, `snapfire_fsr_ir` for the bodies, `snapfire_fsr_plan` for the file it writes and `reqwest` with rustls for esm.sh and the npm registry.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Emit the plan file, the generated types and the tsconfigs | `fsr build <app>` |
| See what a build would emit without writing | `fsr check <app>` |
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
| Run the build from Rust | `build` and `write` |
| Read what was discovered, imported and lowered | `Report` |

## Status

Pre-release and unpublished. `shopping_react_ts` is built with it: its four loaders and three actions are TypeScript under `app/routes/`, typed by `generated/fsr.ts`. React, its JSX runtime, `react-dom/client` and sweetalert2 are vendored by `fsr add`; `@types/react`, `@types/react-dom`, their dependencies and sweetalert2's own declarations are fetched by `fsr types`. `tsc --strict` passes over the whole app against the generated `tsconfig.json`, with no `package.json` and no `node_modules`. Only `app/vendor/` is checked in; `app/plan.json`, `app/generated/`, the tsconfigs, `app/types/` and `app/dist/` are ignored; the crate's `build.rs` runs the build so `cargo build` needs no step before it. The xwpm path is detection and delegation only; no example runs it. Per-route `loading.tsx` and `error.tsx` and a top-level `not-found.tsx` are read into the plan and the storefront's product route streams behind its `loading.tsx`; `layout.tsx` wraps the pages beneath it with `layout.loader.ts` as its loader, hydrated as an island the navigator keeps across a click; the storefront's header lives in one. A `slots/<name>/` beside a layout is a parallel segment the layout places, and a `page.<slot>.tsx` beside a page is the rendering a soft navigation opens in that slot; the storefront's promo strip and quick-look modal are those. A `route.ts` exporting `GET`, `POST`, `PUT`, `PATCH` or `DELETE` is a handler the host answers with JSON before any page is matched; the storefront's `/api/cart` is one. A `middleware.ts` at the top of the app runs before every request and continues, redirects, rewrites, responds or adds headers; the storefront's redirects `/basket` to `/cart`. `fsr prerender` renders the routes that read nothing of the request into files the host serves. `fsr serve` runs the stock host over an app whose configuration is beside it, and `fsr dev` falls back to it when no Cargo project wraps the app. `fsr test` runs the body tests and the page specs, building that host over the spec's mocks so a spec can load a route and click through it. Residue stops a build with the diagnostic, since no engine exists to run it.
