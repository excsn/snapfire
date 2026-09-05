# SnapFire FSR (Full Stack Runtime)

SnapFire FSR provides a full-stack application model built from composable primitives.

***Write the UI in whatever you want. FSR runs the application.***

TypeScript is the application language; Rust is the runtime. 

Routes, data loading, actions, services, identity, caching, rendering and client navigation are runtime primitives rather than features owned by a particular UI framework.

## Why FSR?

FSR is a native, systems-level engine built from the ground up, preserving the declarative ergonomics found in frameworks like Next.js but implementing them directly on a high-performance Rust core.

* **Native SSR.** TypeScript components compile to an intermediate representation rendered directly by Rust. Pages that do not need JavaScript do not start a JavaScript runtime.

* **Many teams, one site, one render.** Multiple teams can build independently versioned sites and mount them into one shell while sharing navigation, session, and application state. See [the sites chapter](docs/guide/205-sites.md).

* **Backend services are first-class.** OpenAPI and protobuf contracts generate typed service clients. HTTP, gRPC, and mock transports share the same application boundary.

* **One application model.** Routes, loaders, actions, services, identity, caching, rendering, and navigation are runtime primitives. React, Svelte, Vue, and Tera can participate in the same application. The UI layer is replaceable.

* **Resilient parallel loaders.**  A failing backend degrades its segment instead of taking down the entire page. The plan assembler resolves segments concurrently and isolates a backend failure to the segment that asked.

* **Island hydration without mismatches.** React hydrates over the server's own markup, timed per island: on load, when visible or when idle. The storefront renders every page with zero console errors, and a hydration mismatch fails a test under `fsr test` before it reaches a browser.

* **Type-safe server actions.** Typed actions work from JavaScript or ordinary HTML forms, with schema validation and CSRF protection handled by the runtime. Every action gets a typed, generated call site; input is checked against the schema before the body runs; the session is mutated from a page click and the route revalidates in place.

* **Streaming and Islands.** A `loading.tsx` beside a page defers it: the document ships with the fallback and the page streams into its slot when the loader finishes. Pages can stream deferred content and hydrate interactive islands on load, visibility, or idle.

* **Slots without sigils.** Parallel and intercepted routes use ordinary filesystem conventions instead of special routing syntax.

* **Tests without Node.** The runtime can test loaders, actions, rendering, hydration, navigation, and page behavior inside the same application environment.

## Architecture

FSR treats backend logic and client presentation as one set of composable blocks. 

```text
                FSR Application
                      │
       ┌──────────────┼──────────────┐
       ▼              ▼              ▼
    Routes          Services       Components
       │              │              │
 Loaders/Actions   OpenAPI/gRPC   React/Svelte/Vue
       │              │              │
       └──────────────┼──────────────┘
                      ▼
                Rust Runtime
```

The goal is simple:

> **Application developers write the application. Platform developers can replace the machinery underneath it.**

Rust is normally invisible. It is the escape hatch for extensions, custom hosts, transports, renderers, or replacement building blocks.

Chapter [000 of the guide](docs/guide/000-what-fsr-is-made-of.md) lays out the vocabulary and [900](docs/guide/900-the-parts-bin.md) lists every block by the itch it scratches.

## Try it

```sh
cd examples/shopping_react_ts
cargo build
../../../target/debug/fsr dev app
```

See the [storefront](examples/shopping_react_ts/README.md), [ops console](examples/ops_console_react_ts/README.md), [portal](examples/portal_react_ts/README.md).


## Education

The [guide](docs/guide/README.md) is the learning layer, one question per chapter with a lab on the running example.

Each crate under this directory carries a `README.md`, a `README.USAGE.md` and an `API_REFERENCE.md`.

## Status

Pre-release and unpublished.

The storefront currently runs on the stock host with TypeScript loaders and actions, Rust-side rendering, and React hydration.

*Designed to be your unstoppable, high-performance daily driver*

Most modern full-stack frameworks are JavaScript wrappers trying to solve systems-level performance problems with duct tape.

[Excerion Sun LLC](https://www.excsn.com)
