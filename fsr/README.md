# SnapFire FSR (Full Stack Runtime)

SnapFire FSR provides a full-stack application model built from composable primitives. *Designed to be your unstoppable, high-performance daily driver.*

***Write the UI in whatever you want. FSR runs the application.***

## Why FSR?

Most modern full-stack frameworks are JavaScript wrappers trying to solve systems-level performance problems with duct tape.

FSR is a native, systems-level engine built from the ground up, preserving the declarative ergonomics found in frameworks like Next.js but implementing them directly on a high-performance Rust core. TypeScript is the application language; Rust is the runtime.

* **Byte-identical SSR, faster.** Components compile down to an intermediate representation the host renders in Rust, with no JavaScript engine in the serving path. The output matches React's server renderer byte for byte and beats React in QuickJS on every page of the storefront; the numbers are in [docs/benches/render.md](docs/benches/render.md).
* **Many teams, one site, one render.** A team builds its routes as a *site*, an artifact with its own name and path prefix, and a shell mounts it from a table in its configuration. The site's pages render inside the shell's root layout, share its session, its store, its import map and its vendored React, and a click from the shell into a site is a soft navigation that keeps the header's island alive. This is what Next.js calls multi-zones, minus the proxy, the second sign-in and the document load at every boundary. Moving a version is moving a pointer: the host rereads its table on a signal and swaps the mount in place, hash-pinned and refused if it does not match. See [the sites chapter](docs/guide/205-sites.md).
* **Resilient parallel loaders.** The plan assembler resolves segments concurrently and isolates a backend failure to the segment that asked. One failing service degrades one region of the page, never the request.
* **Island hydration without mismatches.** React hydrates over the server's own markup, timed per island: on load, when visible or when idle. The storefront renders every page with zero console errors, and a hydration mismatch fails a test under `fsr test` before it reaches a browser.
* **Type-safe server actions.** Every action gets a typed, generated call site; input is checked against the schema before the body runs; the session is mutated from a page click and the route revalidates in place.
* **Streaming by convention.** A `loading.tsx` beside a page defers it: the document ships with the fallback and the page streams into its slot when the loader finishes.
* **Slots without sigils.** A layout's parallel segments live under `slots/` beside it and a route's `page.modal.tsx` opens in a layout's slot on a click, over the page the browser already has; a document load of the same URL is the full page. No `@`, `(.)` or `default.tsx`.
* **Tests without Node.** Body tests replay a loader or action through the same interpreter that serves it. Page specs run in QuickJS over a DOM inside the same process, hydrate over the server's markup, click through actions and navigate between routes.

## Core architecture

FSR treats backend logic and client presentation as one set of composable blocks: a plan file that names routes, loaders, actions and components; a contract merged from your services' own OpenAPI and proto documents; a runtime that matches, loads, evaluates, assembles and streams; a client that hydrates islands and navigates by segment. Chapter [000 of the guide](docs/guide/000-what-fsr-is-made-of.md) lays out the vocabulary and [900](docs/guide/900-the-parts-bin.md) lists every block by the itch it scratches.

## Where to start

```sh
cd examples/shopping_react_ts
cargo build
../../../target/debug/fsr dev app
```

The [storefront example](examples/shopping_react_ts/README.md) is a checkout in four commands, and the [ops console](examples/ops_console_react_ts/README.md) beside it exercises what the storefront does not: the store the islands share, nested layouts, a variant per slot and both kinds of intercept. The [portal](examples/portal_react_ts/README.md) mounts the [billing site](examples/billing_site_react_ts/README.md) under a path of its own: one team's build output served under another's header, one session and one navigation across both. The [guide](docs/guide/README.md) is the learning layer, one question per chapter with a lab on the running example. Each crate under this directory carries a `README.md`, a `README.USAGE.md` and an `API_REFERENCE.md`.

## Status

Pre-release and unpublished. The storefront runs on the stock host, its four loaders and three actions in TypeScript, every page rendered in Rust and hydrated by React. What is built, what is half built and what is not started is tracked against the Next.js surface in the working notes, and the guide says where each seam is when you want to own one.
