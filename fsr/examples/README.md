# FSR examples

Thirteen applications, each carrying the part of FSR the ones before it do not reach. They are one cargo workspace of their own, separate from the workspace that builds the framework, so every crate here resolves the way a crate outside this repository would.

Read them in this order. Each has a `README.md` saying what it shows and where.

| | What it is | Read it for |
| --- | --- | --- |
| [shopping_react_ts](shopping_react_ts/README.md) | A storefront with a catalog, a cart and a checkout | The whole model in one place: TypeScript loaders and actions lowered and run by the Rust host, React pages rendered on the server and hydrated in the browser, two services described only by an OpenAPI document and a `.proto` |
| [ops_console_react_ts](ops_console_react_ts/README.md) | An operations console over a fleet of build agents | What the storefront never touches: a store the islands share, nested layouts, a route handler, middleware, a variant per slot and both kinds of intercept |
| [portal_react_ts](portal_react_ts/README.md) | A company shell with a header, a directory and a sign-in | A **site** mounted under a path from another team's build output, sharing one session, one store and one navigation |
| [billing_site_react_ts](billing_site_react_ts/README.md) | The site the portal mounts and an application in its own right | The `[site]` section: every id prefixed `billing:`, every route under `/billing`, all built against the shell's contract; plus the same artifact running alone |
| [handbook_react_ts](handbook_react_ts/README.md) | A documentation site with no server at all | A different build of the same framework: every route is fixed, so the binary writes the whole site to `site/` and exits and anything can serve it |
| [arrivals_react_ts](arrivals_react_ts/README.md) | An arrivals board over services that stall on purpose | Streaming where you can see it: the board goes out rendered with a skeleton per panel and each parallel slot fills as its service answers |
| [chat_react_ts](chat_react_ts/README.md) | Rooms, messages and who said what | The server talking to a page nobody asked: an action keeps a message, the host publishes the room's topic and every window following it revalidates, with the topic itself behind an authorisation rule |
| [wave_react_ts](wave_react_ts/README.md) | Google Wave: blips nested in blips, presence and everyone's typing visible before it is kept | The seam that goes the other way: a WebSocket per topic whose rows land in the store, beside actions and `live` for everything durable |
| [conference_react_ts](conference_react_ts/README.md) | A one-day conference programme | The application with no Rust in it: routes, loaders and actions in TypeScript alone, compiled to a plan the stock host reads at boot, over a service that is an OpenAPI document and a file of canned answers |
| [recipes_vue_ts](recipes_vue_ts/README.md) | A household recipe box | A second framework on the same seam: every interactive piece a `.vue` file compiled by `snapfirec-vue` and mounted by Vue, the pages static templates that load no framework, no React anywhere in the application |
| [toolshed_web_ts](toolshed_web_ts/README.md) | A street's tool library | No framework at all: custom elements the browser upgrades where the server wrote their markup, one inside a shadow root the server wrote, plus htmx regions swapping fragments the host renders, one segment of a route at a time |
| [advanced_tera_app](advanced_tera_app/) | A Rust application rendering Tera templates on the stock host | The framework with no TypeScript at all: routes, loaders and actions bound in Rust, form-encoded actions for a page with no JavaScript, rendering through the `Evaluator` seam |
| [uni](uni/README.md) | A desk board under a Tera layout | Three interaction models on one page: a React island, a Vue island and an htmx region, one store between the two runtimes, one router replacing a Vue segment with a React one, plus the measured weight of all three |

## Running one

Install the two tools once, then make the browser build of the client library, which git does not carry:

```sh
cargo install snapfire_compiler snapfire_fsr_cli
cd fsr/client && snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json
```

After that each example is `cargo run -p <name>` from this directory. Its `build.rs` emits the plan, the generated TypeScript and the browser bundle, so there is no step before or after. `conference_react_ts`, `recipes_vue_ts` and `toolshed_web_ts` are the exceptions: none has a cargo target, so `fsr dev app` or `fsr serve app` is the only way to run them. The recipes and `uni` need `snapfirec-vue` on `PATH`, from `cargo install snapfire_vue`. `uni` keeps its browser tree under `js/`, so its bundle is built by hand, the way the tera application's is; its README has the line. For the loop that rebuilds as files change, use `fsr dev` on the app directory anywhere:

```sh
cd shopping_react_ts && fsr dev app
```

`fsr test <app>` runs an example's body tests and page specs. `cargo test` here runs every example's Rust tests, which is every example but the conference, the recipes and the tool shed.

## Ports

The storefront and the tera application both take 8080, so run one at a time or override with `--listen`.

| Port | Application |
| --- | --- |
| 8080 | `shopping_react_ts` and `advanced_tera_app` |
| 8081, 8082 | the storefront's own HTTP and gRPC backends, in the same binary |
| 8090 to 8092 | `ops_console_react_ts` and its backends |
| 8100 | `portal_react_ts` |
| 8101 | `billing_site_react_ts` running alone |
| 8110 | `handbook_react_ts`, only under `fsr dev`; the built site is files |
| 8120 | `arrivals_react_ts` |
| 8130 | `chat_react_ts` |
| 8140 | `wave_react_ts` |
| 8150 | `conference_react_ts`, which has no binary of its own |
| 8160 | `recipes_vue_ts`, which has no binary of its own |
| 8170 | `toolshed_web_ts`, which has no binary of its own |
| 8180 | `uni` |

## The portal and the site together

`billing_site_react_ts` is a mount, not a dependency. Build it, then run the portal, which reads its artifact from the path in `[sites.billing]`:

```sh
cargo build -p billing_site_react_ts
cargo run -p portal_react_ts
```

`http://127.0.0.1:8100/` is the portal; `/billing` is the site under the portal's header, with one sign-in covering both. `GET /__fsr/sites` lists what is mounted, with each artifact's version and content hash.

## What these are not

They are not a template to copy wholesale. Each one keeps things it would not need in production, so a reader can see the seam: the storefront serves its own backends from the same binary, the console mocks its fleet, the billing site keeps the static roots and vendor tree it only uses when running alone. The crate `README.md` files say which parts are the demonstration and which are the application.
