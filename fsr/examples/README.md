# FSR examples

Fifteen applications. They are one cargo workspace of their own, separate from the workspace that builds the framework, so every crate here resolves the way a crate outside this repository would.

Each has a `README.md` saying what it shows and where. They are grouped by what renders in the browser. Within a group, read in order: each carries the part the ones before it do not reach.

### React

| | What it is | Read it for |
| --- | --- | --- |
| [shopping_react_ts](shopping_react_ts/README.md) | A storefront with a catalog, a cart and a checkout | The whole model in one place: TypeScript loaders and actions lowered and run by the Rust host, React pages rendered on the server and hydrated in the browser, two services described only by an OpenAPI document and a `.proto` |
| [ops_console_react_ts](ops_console_react_ts/README.md) | An operations console over a fleet of build agents | What the storefront never touches: a store the islands share, nested layouts, a route handler, middleware, a variant per slot and both kinds of intercept |
| [portal_react_ts](portal_react_ts/README.md) | A company shell with a header, a directory and a sign-in | Three **sites** mounted under it, sharing one session, one store and one navigation: one a linked working tree, one installed from an archive, one fetched from an HTTP store |
| [billing_site_react_ts](portal_react_ts/sites/billing/README.md) | The site the portal mounts and an application in its own right | The `[site]` section: every id prefixed `billing:`, every route under `/billing`, all built against the shell's contract; plus the same artifact running alone |
| [gallery_react_ts](gallery_react_ts/README.md) | Four phone photos on a wall | Every image and font path on two pages: photos stored on their side and shown upright, a priority lead with its preload, a keyed record, an SVG served as it is, `data-sf-raw`, the plain `img` rewrite, a remote image source, a stylesheet under `src/` naming an image and a font, an import only the browser's build sees, a family vendored from a provider with its ranges, a family linked from one and a second origin standing in for a CDN |
| [handbook_react_ts](handbook_react_ts/README.md) | A documentation site with no server at all | A different build of the same framework: every route is fixed, so the binary writes the whole site to `site/` and exits and anything can serve it |
| [arrivals_react_ts](arrivals_react_ts/README.md) | An arrivals board over services that stall on purpose | Streaming where you can see it: the board goes out rendered with a skeleton per panel and each parallel slot fills as its service answers |
| [chat_react_ts](chat_react_ts/README.md) | Rooms, messages and who said what | The server talking to a page nobody asked: an action keeps a message, the host publishes the room's topic and every window following it revalidates, with the topic itself behind an authorisation rule |
| [wave_react_ts](wave_react_ts/README.md) | Google Wave: blips nested in blips, presence and everyone's typing visible before it is kept | The seam that goes the other way: a WebSocket per topic whose rows land in the store, beside actions and `live` for everything durable |
| [conference_react_ts](conference_react_ts/README.md) | A one-day conference programme | The application with no Rust in it: routes, loaders and actions in TypeScript alone, compiled to a plan the stock host reads at boot, over a service that is an OpenAPI document and a file of canned answers |
| [blog_site_react_ts](portal_react_ts/sites/blog/README.md) | A blog the portal mounts at `/blog`, its posts baked in at build | A static site under a shell that reads the session: the shell's `fsr prerender` renders each page ahead and splices it under the live layout, the same site prerenders its own documents when it runs alone, one per post from `paths`; a slug off the blog answers 404 and React comes from the shell contract, so nothing is vendored and there is no Rust project |
| [status](portal_react_ts/sites/status/README.md) | A status page the portal mounts at `/status` | A site the shell fetches itself: the table names a version the cache lacks and the portal takes it from `[sites] store`, verified against its manifest |
| [uploads_react_ts](uploads_react_ts/README.md) | A file posted to an action, with JavaScript and without | Multipart where the body never sees a parser: a part with a filename arrives as the built-in `Upload` beside the text fields, refused above `server.max_upload` before the body runs, posted both by a native form and by `upload` from the page |

### Vue

| | What it is | Read it for |
| --- | --- | --- |
| [recipes_vue_ts](recipes_vue_ts/README.md) | A household recipe box | A second framework on the same seam: every interactive piece a `.vue` file compiled by `snapfirec-vue` and mounted by Vue, the pages static templates that load no framework, no React anywhere in the application |

### No framework

| | What it is | Read it for |
| --- | --- | --- |
| [toolshed_web_ts](toolshed_web_ts/README.md) | A street's tool library | Custom elements the browser upgrades where the server wrote their markup, one inside a shadow root the server wrote, plus htmx regions swapping fragments the host renders, one segment of a route at a time |

### Tera

| | What it is | Read it for |
| --- | --- | --- |
| [noticeboard_tera](noticeboard_tera/README.md) | A building's noticeboard, every page a Tera template | A template route on the stock host with no Rust project: `page.tera` and `layout.tera` under `routes/`, loaders lowered beside them, a partial included by its path, `fsr serve app` as the whole server |
| [advanced_tera_app](advanced_tera_app/) | A Rust application rendering Tera templates on the stock host | The framework with no TypeScript at all: routes, loaders and actions bound in Rust, form-encoded actions for a page with no JavaScript, rendering through the `Evaluator` seam |

### Mixed

| | What it is | Read it for |
| --- | --- | --- |
| [uni](uni/README.md) | A desk board under a Tera layout | Three interaction models on one page: a React island, a Vue island and an htmx region, one store between the two runtimes, one router replacing a Vue segment with a React one, plus the measured weight of all three |

### Conformance

Three applications that pin down behaviour rather than show an application. Each is `fsr dev` or `fsr test` on its own directory; none has a cargo target.

| | What it pins down |
| --- | --- |
| [conformance/adapters_react18](conformance/adapters_react18/) | The client's React adapter on React 18, beside the Vue adapter and a custom element on one page: one store between them, nested islands across frameworks, the request context in specs and a server island reading the store. `fsr/client`'s type check reads its declarations |
| [conformance/adapters_react19](conformance/adapters_react19/) | The same pages and specs on React 19, plus `/streamed`: a layout and a streamed page seeding one store key, two streamed slots seeding another, an intercept over the page and islands that hydrate late, which is where the store's per-segment seeding and hydration from the rendered values are pinned |
| [conformance/lowering](conformance/lowering/) | What the build lowers, one route per kind: array and string methods, destructuring, statements, dates, `Intl`, `Map` and `Set`, regular expressions, hooks, Vue children, slots and models and server-rendered `style` attributes under a `[document.csp]`. It builds with `[build] strict`, so anything on it falling back to the browser stops the build |

## Running one

Install the two tools once, then make the browser build of the client library, which git does not carry:

```sh
cargo install snapfire_compiler snapfire_fsr_cli
cd fsr/client && snapfirec --source-map --public-path /static/js/fsr --import-map importmap.json
```

After that each example is `cargo run -p <name>` from this directory. Its `build.rs` emits the plan, the generated TypeScript and the browser bundle, so there is no step before or after. `conference_react_ts`, `blog_site_react_ts`, `uploads_react_ts`, `recipes_vue_ts`, `toolshed_web_ts` and `noticeboard_tera` are the exceptions: none has a cargo target, so `fsr dev app` or `fsr serve app` is the only way to run them. The recipes and `uni` need `snapfirec-vue` on `PATH`, from `cargo install snapfire_vue`. `uni` keeps its browser tree under `js/`, so its bundle is built by hand, the way the tera application's is; its README has the line. For the loop that rebuilds as files change, use `fsr dev` on the app directory anywhere:

```sh
cd shopping_react_ts && fsr dev app
```

`fsr test <app>` runs an example's body tests and page specs. `cargo test` here runs every example's Rust tests, which is every example but the conference, the blog, the recipes, the tool shed and the noticeboard.

## Ports

The storefront and the tera application both take 8080, so run one at a time or override with `--listen`.

| Port | Application |
| --- | --- |
| 8080 | `shopping_react_ts` and `advanced_tera_app` |
| 8081, 8082 | the storefront's own HTTP and gRPC backends, in the same binary |
| 8090 to 8092 | `ops_console_react_ts` and its backends |
| 8100 | `portal_react_ts` |
| 8101 | `billing_site_react_ts` running alone |
| 8102 | `blog_site_react_ts` running alone, which has no binary of its own |
| 8103 | the status site running alone, which has no binary of its own |
| 8108 | `uploads_react_ts`, which has no binary of its own |
| 8110 | `handbook_react_ts`, only under `fsr dev`; the built site is files |
| 8120 | `arrivals_react_ts` |
| 8130 | `chat_react_ts` |
| 8140 | `wave_react_ts` |
| 8150 | `conference_react_ts`, which has no binary of its own |
| 8160 | `recipes_vue_ts`, which has no binary of its own |
| 8170 | `toolshed_web_ts`, which has no binary of its own |
| 8180 | `uni` |
| 8190 | `noticeboard_tera`, which has no binary of its own |
| 8199 | the portal's site store, a file server over `portal_react_ts/deploy/store` |
| 8210 | `conformance/adapters_react18` |
| 8220 | `conformance/adapters_react19` |
| 8230 | `conformance/lowering` |

## The portal and its sites

The sites under `portal_react_ts/sites/` are mounts, not dependencies. The portal reads each one from the artifact its `[sites.<name>]` row names, and the three rows name three different kinds: blog a linked working tree, billing a version installed from an archive, status a version the portal fetches from an HTTP store. `portal_react_ts/README.md` builds, packs, installs and serves them in order, then runs the portal. `GET /__fsr/sites` lists what is mounted, with each artifact's version and content hash.

## What these are not

They are not a template to copy wholesale. Each one keeps things it would not need in production, so a reader can see the seam: the storefront serves its own backends from the same binary, the console mocks its fleet, the billing site runs alone on the portal's vendor tree and has a guard only the portal can satisfy. The crate `README.md` files say which parts are the demonstration and which are the application.
