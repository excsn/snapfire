# shopping_react_ts

A storefront built the way SnapFire FSR expects an application to be built: the loaders and actions are TypeScript, lowered at build time and run by the Rust host; the pages are React islands hydrated in the browser; the services it calls are described only by the documents they publish, one OpenAPI and one `.proto`.

`fsr/examples/` is its own cargo workspace, separate from the one that builds the framework, so both examples resolve the way a crate outside this repository would.

## Run it

Four commands, from a fresh checkout. Only the last one is needed again afterwards.

```sh
# 1. the two tools, from the repository root
cargo build -p snapfire_compiler -p snapfire_fsr_cli

# 2. the browser build of the fsr client library
cd fsr/client
../../target/debug/snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json

# 3. type declarations for the editor, into a gitignored types/
cd ../examples/shopping_react_ts
../../../target/debug/fsr types app

# 4. generate, bundle, build and run, then keep doing so as files change
../../../target/debug/fsr dev app
```

Then open <http://127.0.0.1:8080>. Boot prints the routes, the sources, the actions, the modules rendered on the server, the services and what the host inferred.

Step 2 is the browser build of the client library, which git does not carry. Step 3 is best effort: skip it and the app still runs, the editor just types every import as `any`. There is no `npm install`, because `app/vendor/` holds the runtime modules and is committed.

Step 4 is three commands the loop runs for you and redoes as files change. By hand they are, in this order:

```sh
cargo build                     # the plan, the contracts and the generated TypeScript, written by build.rs
cd app && ../../../../target/debug/snapfirec --config tsconfig.build.json --source-map --public-path /static/js/app --import-map importmap.json
cd .. && cargo run
```

The bundle must follow the build, because it compiles the island registry that `build.rs` writes.

## Three servers, one binary

| Port | What |
| --- | --- |
| 8080 | the FSR host, the only one the browser talks to |
| 8081 | the shopping service over HTTP, described by `app/clients/shopping.openapi.json` |
| 8082 | the inventory service over gRPC, described by `app/clients/inventory.proto` |

The shopping document says how long its answers hold: `listProducts` and `getProduct` carry `x-sf-cache`, shared for thirty seconds under the `catalog` tag, the list with a two-minute stale window, and `placeOrder` carries `x-sf-writes: ["catalog"]`, so an order drops both. `[cache.data]` in `config/app.toml` turns that on and the report lists it under `cached`; the inventory over gRPC is uncached, since a proto carries no annotation yet.

The two backends stand in for services this application does not own. It reaches both through one typed registry; neither the loaders nor the pages can tell which transport a call uses.

## What is where

```
app/                    the TypeScript application
  routes/               a directory per route: page.tsx, page.loader.ts, actions.ts, loading.tsx; index, product, cart and order
  tests/                body tests, page specs and the client library's specs, run by fsr test
  schemas/              the session shape and each action's input
  clients/              the service documents, imported into the contract
  src/ui/               components the pages share
  styles/               plain CSS, linked into the head by convention
  vendor/               React and SweetAlert2, committed, no npm
  importmap.json        the bare specifiers the browser resolves
  tsconfig.json         generated: the @app, @routes, @src, @schemas and @generated aliases
config/app.toml         listen address, session key, each service's base URL
src/backend/            the two services this example pretends not to own
src/routes.rs           the one route added in Rust
build.rs                runs the fsr build, so cargo build is enough
```

The pages are rendered on the server without a JavaScript engine: the build lowers each `page.tsx` and the components under `src/ui/` to a render tree in the plan file, the host renders it in Rust and React hydrates over the markup in the browser. The `rendered` rows at boot say which modules that covers; a module the build cannot lower is listed as `client` with the line that decided it and mounts in the browser only.

Generated output is not committed. `build.rs` writes `app/generated/` on every `cargo build`: the plan file, one contract per document, the TypeScript a body is written against and both tsconfigs.

## Changing it

While `fsr dev app` runs:

| You changed | It does |
| --- | --- |
| a page, a component or the CSS | rebundles; reload the page |
| a loader, an action, a schema or a service document | regenerates, rebundles and restarts the server |
| Rust under `src/`, `build.rs`, `Cargo.toml` or `config/` | rebuilds and restarts the server |
| the fsr client library under `fsr/client/src` | nothing; step 2 again, then save any page |
| a dependency the browser loads | nothing; `fsr add app <name>@<version>`, then `fsr types app` |

A step that fails leaves the running server up and waits for the next change. A restart drops the in-memory sessions, so a page edit never causes one: the server is restarted only when the generated files differ.

## Tests

```sh
cargo test                       # the Rust suite
../../../target/debug/fsr test app   # the body tests, page specs and client specs under app/, no Node
```

Imports across folders use the aliases the build writes into `tsconfig.json`, `@src/ui/Header` or `@generated/client`, and snapfirec turns them into relative paths in the bundle. The body tests are `app/tests/cart/loader.test.ts` and `app/tests/cart/actions.test.ts`: each builds a context with `ctx({ session, services, input })`, mocking a service method as a plain function, runs the loader or action and asserts on what came back, on `c.session` and on `c.trace.calls`. They are TypeScript the build lowers and the host's interpreter replays, so the body under test runs where it runs in production. A mock that answers something the contract rejects fails with the method's name. The page specs, `tests/cart/page.spec.tsx`, `tests/index/page.spec.tsx`, `tests/product/page.spec.tsx` and `tests/ui/Header.spec.tsx`, render a page into a DOM inside the same `fsr test` run: QuickJS in process, linkedom for the document, React's development build so a hydration mismatch is reported in words. A page the build lowered is hydrated over the server's own markup; a click that calls an action runs the lowered action through the interpreter under the spec's `ctx`, with the mocked services behind the contract. `tests/product/loading.spec.tsx` hydrates the loading module the product route streams behind and `tests/order/page.spec.tsx` the order page a checkout lands on, after the toast has run its course. `tests/navigation.spec.tsx` loads the catalog through the stock host, built inside the runner over `config/app.toml` with the spec's mocks as its transport, clicks through to the cart and asserts the document survived while the page swapped. `tests/client/` holds the client library's own specs, values, payload, boot and actions, run here because this app vendors React and serves the library. The compiled specs and the test-only builds land in `app/.fsr-test/`, which is ignored.

27 tests covering the catalogue, its ordering rules and reading an order back, the storefront over a mock transport with no backend running, plus the gRPC transport against a real tonic server on a port it picks.
