# shopping_react_ts

A storefront built the way Snapfire FSR expects an application to be built: the loaders and actions are TypeScript, lowered at build time and run by the Rust host; the pages are React islands hydrated in the browser; the services it calls are described only by the documents they publish, one OpenAPI and one `.proto`.

It is its own cargo workspace, so it builds the way a crate outside this repository would.

## Run it

Six commands, from a fresh checkout. Only the last one is needed again afterwards.

```sh
# 1. the two tools, from the repository root
cargo build -p snapfire_compiler -p snapfire_fsr_cli

# 2. the browser build of the fsr client library
cd fsr/client
../../target/debug/snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json

# 3. type declarations for the editor, into a gitignored types/
cd ../examples/shopping_react_ts
../../../target/debug/fsr types app

# 4. the plan, the contracts and the generated TypeScript, written by build.rs
cargo build

# 5. the browser bundle, which needs step 4's generated modules
cd app
../../../../target/debug/snapfirec --config tsconfig.build.json --source-map --public-path /static/js/app --import-map importmap.json

# 6. run it
cd .. && cargo run
```

Then open <http://127.0.0.1:8080>. Boot prints the routes, the sources, the actions, the services and what the host inferred.

Steps 2 and 5 are the browser bundles, which git does not carry. Step 5 must follow step 4, because it compiles the island registry that `build.rs` writes. Step 3 is best effort: skip it and the app still runs, the editor just types every import as `any`. There is no `npm install`, because `app/vendor/` holds the runtime modules and is committed.

## Three servers, one binary

| Port | What |
| --- | --- |
| 8080 | the FSR host, the only one the browser talks to |
| 8081 | the shopping service over HTTP, described by `app/clients/shopping.openapi.json` |
| 8082 | the inventory service over gRPC, described by `app/clients/inventory.proto` |

The two backends stand in for services this application does not own. It reaches both through one typed registry; neither the loaders nor the pages can tell which transport a call uses.

## What is where

```
app/                    the TypeScript application
  routes/               a directory per route: page.tsx, loader.ts, actions.ts
  schemas/              the session shape and each action's input
  clients/              the service documents, imported into the contract
  src/ui/               components the pages share
  styles/               plain CSS, linked into the head by convention
  vendor/               React and SweetAlert2, committed, no npm
  importmap.json        the bare specifiers the browser resolves
config/app.toml         listen address, session key, each service's base URL
src/backend/            the two services this example pretends not to own
src/routes.rs           the one route added in Rust
build.rs                runs the fsr build, so cargo build is enough
```

Generated output is not committed. `build.rs` writes `app/generated/` on every `cargo build`: the plan file, one contract per document, the TypeScript a body is written against and both tsconfigs.

## Changing it

| You changed | Do this |
| --- | --- |
| a loader, an action, a schema or a service document | `cargo run`, since `build.rs` rebuilds the plan and the types |
| a page, a component or the CSS | step 5 again, then reload |
| the fsr client library under `fsr/client/src` | step 2 again, then step 5 |
| a dependency the browser loads | `fsr add app <name>@<version>`, then `fsr types app` |

## Tests

```sh
cargo test
```

24 tests covering the catalogue and its ordering rules, the storefront over a mock transport with no backend running, plus the gRPC transport against a real tonic server on a port it picks.
