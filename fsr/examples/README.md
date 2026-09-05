# FSR examples

Five applications, each carrying the part of FSR the ones before it do not reach. They are one cargo workspace of their own, separate from the workspace that builds the framework, so every crate here resolves the way a crate outside this repository would.

Read them in this order. Each has a `README.md` saying what it shows and where.

| | What it is | Read it for |
| --- | --- | --- |
| [shopping_react_ts](shopping_react_ts/README.md) | A storefront with a catalog, a cart and a checkout | The whole model in one place: TypeScript loaders and actions lowered and run by the Rust host, React pages rendered on the server and hydrated in the browser, two services described only by an OpenAPI document and a `.proto` |
| [ops_console_react_ts](ops_console_react_ts/README.md) | An operations console over a fleet of build agents | What the storefront never touches: a store the islands share, nested layouts, a route handler, middleware, a variant per slot and both kinds of intercept |
| [portal_react_ts](portal_react_ts/README.md) | A company shell with a header, a directory and a sign-in | A **site** mounted under a path from another team's build output, sharing one session, one store and one navigation |
| [billing_site_react_ts](billing_site_react_ts/README.md) | The site the portal mounts, and an application in its own right | The `[site]` section: every id prefixed `billing:`, every route under `/billing`, built against the shell's contract, and the same artifact running alone |
| [advanced_tera_app](advanced_tera_app/) | A Rust application rendering Tera templates on the stock host | The framework with no TypeScript at all: routes, loaders and actions bound in Rust, form-encoded actions for a page with no JavaScript, rendering through the `Evaluator` seam |

## Running one

Build the two tools once from the repository root, then the browser build of the client library, which git does not carry:

```sh
cargo build -p snapfire_compiler -p snapfire_fsr_cli
cd fsr/client && ../../target/debug/snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json
```

After that each example is `cargo run -p <name>` from this directory. Its `build.rs` emits the plan, the generated TypeScript and the browser bundle, so there is no step before or after. For the loop that rebuilds as files change, use `fsr dev` on the app directory instead:

```sh
cd shopping_react_ts && ../../../target/debug/fsr dev app
```

`fsr test <app>` runs an example's body tests and page specs. `cargo test` here runs every example's Rust tests.

## Ports

The storefront and the tera application both take 8080, so run one at a time or override with `--listen`.

| Port | Application |
| --- | --- |
| 8080 | `shopping_react_ts`, and `advanced_tera_app` |
| 8081, 8082 | the storefront's own HTTP and gRPC backends, in the same binary |
| 8090 to 8092 | `ops_console_react_ts` and its backends |
| 8100 | `portal_react_ts` |
| 8101 | `billing_site_react_ts` running alone |

## The portal and the site together

`billing_site_react_ts` is a mount, not a dependency. Build it, then run the portal, which reads its artifact from the path in `[sites.billing]`:

```sh
cargo build -p billing_site_react_ts
cargo run -p portal_react_ts
```

`http://127.0.0.1:8100/` is the portal; `/billing` is the site under the portal's header, with one sign-in covering both. `GET /__fsr/sites` lists what is mounted, with each artifact's version and content hash.

## What these are not

They are not a template to copy wholesale. Each one keeps things it would not need in production, so a reader can see the seam: the storefront serves its own backends from the same binary, the console mocks its fleet, the billing site keeps the static roots and vendor tree it only uses when running alone. The crate `README.md` files say which parts are the demonstration and which are the application.
