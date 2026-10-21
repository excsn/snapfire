# SnapFire FSR (Full Stack Runtime)

SnapFire FSR provides a full-stack application model built from composable primitives.

***Write the UI in whatever you want. FSR runs the application.***

TypeScript is the application language; Rust is the runtime. 

Routes, data loading, actions, services, identity, caching, rendering and client navigation are runtime primitives rather than features owned by a particular UI framework.

## Why FSR?

FSR is a native, systems-level engine built from the ground up, preserving the declarative ergonomics found in frameworks like Next.js but implementing them directly on a high-performance Rust core.

* **Native SSR.** TypeScript components compile to an intermediate representation rendered directly by Rust. Pages that do not need JavaScript do not start a JavaScript runtime.

* **Many teams, one site, one render.** Multiple teams can build independently versioned sites and mount them into one shell while sharing navigation, session and application state. See [the sites chapter](docs/guide/205-sites.md).

* **Backend services are first-class.** OpenAPI and protobuf contracts generate typed service clients. HTTP, gRPC and mock transports share the same application boundary.

* **One application model.** Routes, loaders, actions, services, identity, caching, rendering and navigation are runtime primitives. React, Svelte, Vue and Tera can participate in the same application. The UI layer is replaceable.

* **Resilient parallel loaders.**  A failing backend degrades its segment instead of taking down the entire page. The plan assembler resolves segments concurrently and isolates a backend failure to the segment that asked.

* **Island hydration without mismatches.** React hydrates over the server's own markup, timed per island: on load, when visible or when idle. The storefront renders every page with zero console errors and a hydration mismatch fails a test under `fsr test` before it reaches a browser.

* **Type-safe server actions.** Typed actions work from JavaScript or ordinary HTML forms, with schema validation and CSRF protection handled by the runtime. Every action gets a typed, generated call site; input is checked against the schema before the body runs; the session is mutated from a page click and the route revalidates in place.

* **Streaming and Islands.** A `loading.tsx` beside a page defers it: the document ships with the fallback and the page streams into its slot when the loader finishes. Pages can stream deferred content and hydrate interactive islands on load, visibility or idle.

* **Slots without sigils.** Parallel and intercepted routes use ordinary filesystem conventions instead of special routing syntax.

* **Tests without Node.** The runtime can test loaders, actions, rendering, hydration, navigation and page behavior inside the same application environment.

## Rendering modes

FSR does not make you pick a mode per route. Each route gets whatever mix the build and its loader allow.

| Mode | Supported | How |
|---|---|---|
| SSR | Yes, the default | Pages and layouts always render on the server, in Rust, without a JavaScript engine. See [003](docs/guide/003-rendered-where-it-is-cheapest.md). |
| SSG / prerender | Yes | `fsr prerender` writes a file for every route whose loader reads no params, query, session, identity, host or clock. Nothing is declared; a `paths` export lists the pages of a route with a parameter. See [300](docs/guide/300-the-build-and-the-dev-loop.md#prerendering). |
| ISR | Yes, on your schedule | `Host::prerender` rewrites the prerendered files while the host serves, from the application's own timer. Files are swapped atomically and a failed loader keeps the last file. See [300](docs/guide/300-the-build-and-the-dev-loop.md#prerendering-again-while-the-host-serves). |
| Streaming | Yes | A `loading.tsx` ships the fallback and streams the page in when its loader finishes. Parallel slots stream with their own loading and error boundaries. See [100](docs/guide/100-routes-loaders-and-pages.md#errors-and-loading). |
| SWR | Yes, on service data | A method's contract declares `ttl`, `stale`, `scope` and `tags`; `stale` serves the last answer while a refresh runs and a method that `writes` a tag drops it. See [service](service/README.USAGE.md#caching-a-methods-answers). |
| Islands | Yes | React, Vue and Svelte islands hydrate on load, when visible or when idle, several frameworks on one page. See [104](docs/guide/104-islands-in-another-framework.md). |
| Server-mode islands | Yes | An island with no JavaScript half: its events run on the server and the browser patches the markup that comes back. See [102](docs/guide/102-components-the-server-renders.md#server-mode-islands). |
| Server islands (deferred fragments) | Yes, as streaming | A segment behind `loading.tsx` or a parallel slot renders after the document and streams into its place. |
| SPA navigation | Yes | Client navigation fetches a payload and swaps the page region, keeping the layout and its islands; a link's payload is fetched on hover, focus or touch. See [100](docs/guide/100-routes-loaders-and-pages.md#client-navigation). |
| Client-only SPA | No | Pages always render on the server; only islands render in the browser. |
| Static export | Partly | There is no export command. The handbook example builds a static site from the prerender output, the client modules it writes and the static roots the boot report lists, then serves it from a plain file server. |

## How much renders on the server

Each corpus builds a framework's own published examples through FSR as one application per example and checks every component the server renders against that framework's own server renderer. A component the build cannot render yet still works: it renders in the browser and is missing from the first response.

| Corpus | Examples | Rendered on the server | Rendered on the server after migration | Left to the browser by design | Browser rendered, SSR pending |
|---|---|---|---|---|---|
| [react.dev](docs/corpus/react-dev.md) | 634 | 575 (90.7%) | 579 (91.3%) | 59 | 0 |
| [svelte.dev](docs/corpus/svelte-dev.md) | 167 | 151 (90.4%) | 152 (91.0%) | 16 | 0 |
| [vuejs.org](docs/corpus/vuejs-org.md) | 70 | 58 (82.9%) | 58 (82.9%) | 12 | 0 |

The "by design" column counts examples that use something the server can't reproduce exactly: the current time, `Math.random`, a `fetch` during render or an npm package the build doesn't follow. Not counting those, the server renders 100% of the rest on all three.

The migration column counts the by-design examples that render on the server once the rewrite a migration makes is applied: an awaited read moved into the route's loader and seeded into the store, a `<svelte:head>` moved into the loader's `meta`. For react.dev `fsr migrate` makes the rewrite and `examples/corpus/migrated_outcomes.txt` holds what each example then builds to, leaving out a read only a later render makes; the svelte.dev one was applied by hand. The rest load their data after a click, a navigation or typing, read the clock or need a contract for a remote API.

## Desktop apps (experimental)

The same application runs as a desktop app, a high-performance replacement for Electron. Electron ships a Chromium and a Node runtime inside every app. FSR opens the operating system's own webview (WebKit on macOS and Linux, WebView2 on Windows) and runs the Rust host in the same process, so the app carries no browser and no JavaScript runtime of its own. The pages, loaders, actions and islands are the ones the website serves.

```sh
cargo install snapfire_fsr_cli --features desktop
fsr dev app --desktop
fsr serve app --desktop
```

Desktop mode is experimental. Today the window loads the host over a loopback port. Its menu bar has back, forward, reload and the clipboard. Its title is `[desktop] name` followed by the page's title. It is tried on macOS only; the Windows and Linux code is written but has not been built or run. Page code reaches the process through `desktop` from `@snapfire/fsr-client`, which opens the system file dialog and hands links to the system browser. Packaging and signing do not exist yet. See [Opening the App in a Window](cli/README.USAGE.md#opening-the-app-in-a-window).

## Terminal apps (experimental)

FSR runs [Ink](https://github.com/vadimdemedes/ink) components in the terminal: Ink's `Box`, `Text`, `Spacer`, `Newline` and `useInput`, imported from `ink` itself, so an Ink file needs no changes to run. Ink is what Claude Code, Gemini CLI, Qwen Code, Shopify CLI and the TypeScript Codex CLI draw their terminals with. Ink runs every one of them on Node. FSR lowers the same components and runs them inside its Rust binary with no Node and no React: flexbox layout through taffy, each key stepping the component's state and only the cells that changed rewritten. That is the base for high-performance terminal apps of that kind.

```tsx
import { useState } from "react";
import { Box, Text, useInput } from "ink";

export default function Counter() {
  const [count, setCount] = useState(0);
  useInput((input, key) => {
    if (input === "+" || key.upArrow) setCount(count + 1);
  });
  return (
    <Box borderStyle="round" paddingX={1}>
      <Text>Count: {count}</Text>
    </Box>
  );
}
```

```sh
cargo install snapfire_fsr_cli --features terminal
fsr dev app --terminal
```

Terminal mode is experimental. Today it runs the app's root page `routes/page.tsx` with every component it places, each with its own state, keys and timers. It reads React's `useState`, `useReducer` and `useRef` with Ink's `useInput`, `useFocus`, `useFocusManager`, `useApp().exit`, `useWindowSize`, `useStdout`, `useStderr`, `useAnimation`, `useBoxMetrics`, `useCursor` and `Static`; Tab, Shift+Tab and Esc move the focus as Ink's do. A mount effect may start `setInterval` and `setTimeout` timers. Local time reads this machine's time zone. A Box may be sized in percentages. `ms`, `strip-ansi`, `string-width` and `wrap-ansi` run natively. A mount effect may spawn a process with `node:child_process` and read its output. A handler or a timer sets state, dispatches or calls an action. An effect runs again when its dependencies change. A promise made with `new Promise`, an `await` and a `Suspense` boundary waiting on a thrown promise run as they do in Ink. `Transform` and `measureElement` are not supported yet. Editing the page while it runs redraws it with its state kept. See [Running a Terminal App](cli/README.USAGE.md#running-a-terminal-app) and the [counter example](examples/counter_terminal_ts/README.md).

The corpus runs Ink's own examples through terminal mode as one application each:

| Corpus | Examples | Runs in the terminal | Not run by design | Refused |
|---|---|---|---|---|
| [Ink](docs/corpus/ink.md) | 29 | 24 (82.8%) | 3 | 2 |

The "by design" column counts examples built on `react-router` (FSR's own routes stand in for it) and on an npm package the build does not follow: `@faker-js/faker` and `p-queue` with `delay`. Not counting those, 24 of the 26 run (92.3%). A refused example uses something terminal mode cannot run yet, such as `useApp().suspendTerminal`. The corpus page lists each cause.

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

Rust is normally invisible. It is the escape hatch for extensions, custom hosts, transports, renderers or replacement building blocks.

Chapter [000 of the guide](docs/guide/000-what-fsr-is-made-of.md) lays out the vocabulary and [900](docs/guide/900-the-parts-bin.md) lists every block by the itch it scratches.

## Try it

```sh
cargo install snapfire_compiler snapfire_fsr_cli
cd examples/shopping_react_ts
cargo build
fsr dev app
```

[examples/](examples/README.md) is the index: thirteen applications in reading order, what each one shows and the ports they take.

## Education

The [guide](docs/guide/README.md) is the learning layer, one question per chapter with a lab on the running example.

Each crate under this directory carries a `README.md`, a `README.USAGE.md` and an `API_REFERENCE.md`.

*Designed to be your unstoppable, high-performance daily driver*

Most modern full-stack frameworks are JavaScript wrappers trying to solve systems-level performance problems with duct tape.

[Excerion Sun LLC](https://www.excsn.com)
