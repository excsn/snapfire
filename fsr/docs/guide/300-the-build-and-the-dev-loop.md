# 300. The build and the dev loop

The question this chapter answers: what does `fsr build` produce, why is none of it committed and what does `fsr dev` do when a file changes?

**For:** everyone.

## One command, four things

`fsr build <app>` walks `app/` and writes `app/generated/`:

- **The plan file**, `generated/plan.json`: routes, lowered bodies, declared actions and render trees. The host reads it at boot.
- **The contracts**, one file per client document plus one for the schemas, under `generated/contracts/`. The host merges them at boot.
- **The TypeScript the application is written against**: `fsr.ts` with `Ctx` and `ActionCtx` per route, `services.d.ts` from the contract, `client.ts` with every page's props type and every action's typed callable, `islands.ts` registering every page for the browser, `testing.ts` for the tests.
- **Both tsconfigs**: `tsconfig.json` for the editor and `tsc`, with the aliases and the type roots, plus `tsconfig.build.json` for the bundle.

`fsr check <app>` does the same without writing, prints the report and exits non-zero when anything is residue. It is the command for a pull request.

Nothing under `generated/` is committed. It is rebuilt from `routes/`, `schemas/` and `clients/` in well under a second; committing it would mean every branch that touches a loader conflicts with every other on a JSON file nobody edits by hand. The example's `.gitignore` says which directories those are; a new application copies it.

## The bundle is a separate tool

snapfirec compiles the browser side, `src/`, the pages under `routes/` and the two generated modules the browser needs, into `dist/`, following `tsconfig.build.json`. It is a general TypeScript compiler that knows nothing about fsr; the build writes the tsconfig it needs and `fsr dev` runs it. The bundle must follow the build, since it compiles the island registry the build writes, which is the one ordering mistake a fresh checkout can make and the reason `fsr dev` exists.

The bundle's own facts file, `dist/.snapfire-build.json`, is what the host reads to infer the public path and the entry script, so the two tools meet through a file rather than through configuration.

They meet through a second directory too. For every component whose render-path calls or static subtrees the server computes for the browser, the build writes a copy of the module under `app/.fsr-bundle/` with those calls turned into reads of what the server delivered, and `fsr dev` passes `--overlay .fsr-bundle` so snapfirec compiles the copy in place of the source at the same path. The source is never touched, the editor and `fsr check` never see the copy, and the directory is rebuilt on every build, so a component that stops qualifying stops being overlaid. It is not committed either; the example's `.gitignore` lists it beside `generated/`.

## A Cargo project runs the build for you

An application with a Rust project puts three lines in `build.rs`: build, write, done. `cargo build` then regenerates `generated/` whenever `routes/`, `schemas/` or `clients/` change, so the host never boots against a stale plan. The storefront does this, which is why its README's manual steps are `cargo build`, the bundle, `cargo run`, in that order.

## `fsr dev`

`fsr dev <app>` is those steps in a loop. It generates, bundles, builds the Cargo project beside the app and runs it, or runs the stock host when there is no such project, then watches. What it does on a change depends on what changed:

| Changed | It does |
| --- | --- |
| a page, a component or CSS | rebundles; the open page refreshes itself |
| a loader, an action, a schema or a client document | regenerates, rebundles, reloads the running server in place |
| Rust under `src/`, `build.rs`, `Cargo.toml` or `config/` | rebuilds, restarts |
| `config/` beside an app with no Cargo project | reloads the stock host in place, restarts when refused |
| a test | nothing; `fsr test` is its own command |

The browser follows along. A served document in development carries a script listening on the host's `/__fsr/events`; the loop tells the host after a rebundle and the host tells every open document. A stylesheet edit re-links the stylesheets and refreshes the route's payload in place, so the layout keeps its state; an edited module reloads the page, since the one it hydrated with is stale; a restart drops the stream and the reconnect does the same check. `server.dev = false` turns it off, and `prerender` never writes the script.

The reload rule is the one worth knowing: when the generated files actually differ the loop asks the running server to reload its tables in place, `POST /__fsr/reload`, and restarts only when the reload is refused, a changed `[session]` for one, or when the Rust project changed. Editing a page never restarts, because the bundle's output names are stable and the host reads it from disk; a reload keeps every session, which chapter 204 explains. A step that fails leaves the running server up and waits, so a typo never takes the page down. Stopping the loop stops the server with it.

Without a Cargo project beside the app, the loop runs `fsr serve` on the app instead: the stock host over `config/app.toml`, restarted on the same rule, with `config/` watched in place of `src/`. `fsr serve <app> [--listen <addr>]` is that host on its own, for production, the way `cargo run` is for a project that has one; it refuses a configuration whose `[app] dir` names a different directory.

## Rendering once what never changes

A route whose loader reads no parameter, no query, no session, no identity and no clock renders the same for every request. The host can tell, because the loader is data it can read. The boot report lists such routes under `prerender`; `fsr prerender app` (or the project's binary with `--prerender`) renders each once into `server.prerender` and the host answers those from the file from then on, with `x-sf-prerendered: 1` so you can see it. The storefront's `/about` qualifies; every other page reads the cart from the session, so none of them does. There is nothing to declare: a loader that starts reading the session takes its route back to per-request rendering at the next build.

## The lab

Run `fsr dev app` in the storefront, open a product in the browser and type into the header's search box. Add `h1 { letter-spacing: 3px; }` to `styles/app.css` and save: the heading spreads out and the search text is still there, since only the stylesheet and the payload moved. Now edit the button text in `routes/product/[id]/page.tsx` and save: the log shows the report again, since the plan changed, then a restart, and the page reloads with the new text. Now edit `routes/index/page.loader.ts`, remove `tag: query.tag` from the call and save: the log shows the report again, since the plan changed, then a restart. Put it back.

Then break something: add `try {` without a closing brace to the loader. The loop prints the parse error with its line and waits; the previous server keeps serving. Fix it and the loop picks up where it left off.
