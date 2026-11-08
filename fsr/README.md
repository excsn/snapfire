# SnapFire FSR (Full Stack Runtime)

***Write the UI in whatever you want. FSR runs the application.***

You write the application in TypeScript and Rust runs it. Routes, data loading, actions, services, identity, caching, rendering and client navigation belong to the runtime, not to a UI framework. React, Vue, Svelte and Tera can all render in the same application.

FSR keeps the declarative style of frameworks like Next.js and runs it on a Rust core.

```sh
cargo install snapfire_compiler snapfire_fsr_cli
cd examples/shopping_react_ts
cargo build
fsr dev app
```

## Why FSR?

* **Server rendering without JavaScript.** Rust renders your TypeScript components directly. A page that needs no JavaScript never starts a JavaScript engine.

* **Many teams, one site.** Teams build and version their own sites and mount them into one shell. Navigation, session and application state are shared. See [the sites chapter](docs/guide/205-sites.md).

* **Typed backend services.** OpenAPI and protobuf contracts generate typed service clients. HTTP, gRPC and mock transports sit behind the same interface.

* **A failing backend doesn't take the page down.** Loaders run in parallel. When one fails, only the part of the page that asked for it shows an error.

* **Islands that hydrate cleanly.** React hydrates over the server's markup, per island: on load, when visible or when idle. `fsr test` fails on a hydration mismatch before it reaches a browser.

* **Type-safe server actions.** Actions work from JavaScript and from plain HTML forms. Each one gets a typed call site. The runtime validates input against the schema and checks CSRF before the action runs.

* **Streaming.** Put a `loading.tsx` beside a page. The document ships with the fallback and the page streams in when its loader finishes.

* **Parallel and intercepted routes from folder names.** There is no special routing syntax.

* **Tests without Node.** `fsr test` tests loaders, actions, rendering, hydration, navigation and page behaviour inside the application.

## Rendering modes

You don't pick a mode per route. Each route gets whatever mix the build and its loader allow.

| Mode | Supported | How |
|---|---|---|
| SSR | Yes, the default | Pages and layouts always render on the server, in Rust, without a JavaScript engine. See [003](docs/guide/003-rendered-where-it-is-cheapest.md). |
| SSG / prerender | Yes | `fsr prerender` writes a file for every route whose loader reads no params, query, session, identity, host or clock. Nothing is declared. A `paths` export lists the pages of a route with a parameter. See [300](docs/guide/300-the-build-and-the-dev-loop.md#prerendering). |
| ISR | Yes, on your schedule | `Host::prerender` rewrites the prerendered files while the host serves, from the application's own timer. Files are swapped atomically and a failed loader keeps the last file. See [300](docs/guide/300-the-build-and-the-dev-loop.md#prerendering-again-while-the-host-serves). |
| Streaming | Yes | A `loading.tsx` ships the fallback and streams the page in when its loader finishes. Parallel slots stream with their own loading and error boundaries. See [100](docs/guide/100-routes-loaders-and-pages.md#errors-and-loading). |
| SWR | Yes, on service data | A method's contract declares `ttl`, `stale`, `scope` and `tags`. `stale` serves the last answer while a refresh runs. A method that `writes` a tag drops it. See [service](service/README.USAGE.md#caching-a-methods-answers). |
| Islands | Yes | React, Vue and Svelte islands hydrate on load, when visible or when idle, several frameworks on one page. See [104](docs/guide/104-islands-in-another-framework.md). |
| Server-mode islands | Yes | An island with no JavaScript half. Its events run on the server and the browser patches in the markup that comes back. See [102](docs/guide/102-components-the-server-renders.md#server-mode-islands). |
| Server islands (deferred fragments) | Yes, as streaming | A segment behind `loading.tsx` or a parallel slot renders after the document and streams into its place. |
| SPA navigation | Yes | Client navigation fetches a payload and swaps the page region, keeping the layout and its islands. A link's payload is fetched on hover, focus or touch. See [100](docs/guide/100-routes-loaders-and-pages.md#client-navigation). |
| Client-only SPA | No | Pages always render on the server. Only islands render in the browser. |
| Static export | Partly | There is no export command. The handbook example builds a static site from the prerender output, the client modules and the static roots, then serves it from a plain file server. |

## How much renders on the server

We build each framework's own published examples as FSR applications and compare what FSR renders against the framework's own server renderer. A component FSR can't render on the server still works. It renders in the browser and is missing from the first response.

| Corpus | Examples | Rendered on the server | After migration | Left to the browser by design | SSR pending |
|---|---|---|---|---|---|
| [react.dev](docs/corpus/react-dev.md) | 634 | 575 (90.7%) | 582 (91.8%) | 59 | 0 |
| [svelte.dev](docs/corpus/svelte-dev.md) | 167 | 151 (90.4%) | 152 (91.0%) | 16 | 0 |
| [vuejs.org](docs/corpus/vuejs-org.md) | 70 | 58 (82.9%) | 58 (82.9%) | 12 | 0 |

"By design" means the example uses something a server can't reproduce exactly: the current time, `Math.random`, a `fetch` during render or an npm package the build doesn't follow. Leaving those out, the server renders 100% of the rest for all three frameworks.

"After migration" counts by-design examples that render on the server once their data read moves into the route's loader. `fsr migrate` makes that change for react.dev. The svelte.dev one was made by hand. Each corpus page lists the examples and why.

Next.js's own examples go through `fsr migrate` and then build as FSR applications ([outcomes](examples/corpus/next_outcomes.txt)). They are whole applications with backends, CSS modules and component libraries, so most stop at the build:

| Corpus | Examples | Rendered on the server after migration | Some components in the browser | Refused at build | Not migrated, CommonJS |
|---|---|---|---|---|---|
| Next.js | 223 | 44 (19.7%) | 65 | 111 | 3 |

A refused build names the line it stopped at. `fsr migrate` reads ES modules only, so an application that reaches a CommonJS file isn't migrated and the file is named.

## Desktop apps (experimental)

The same application runs as a desktop app, as a lighter replacement for Electron. An Electron app ships its own Chromium and Node. An FSR app opens the operating system's webview (WebKit on macOS and Linux, WebView2 on Windows) and runs the Rust server in the same process. The pages, loaders, actions and islands are the ones the website serves.

```sh
cargo install snapfire_fsr_cli --features desktop
fsr dev app --desktop
fsr serve app --desktop
```

Desktop mode has been tried on macOS only. The Windows and Linux code is written but hasn't been built or run. Packaging and signing don't exist yet. [Opening the App in a Window](cli/README.USAGE.md#opening-the-app-in-a-window) covers the window, its menu, cookies and the `@snapfire/fsr/desktop` bridge to the system file dialog and browser.

## Terminal apps (experimental)

FSR runs [Ink](https://github.com/vadimdemedes/ink) components in the terminal with no Node and no React. Claude Code, Gemini CLI, Qwen Code, Shopify CLI and the TypeScript Codex CLI draw their terminals with Ink. FSR runs the same components inside its Rust binary.

```tsx
import { useState } from "react";
import { Box, Text, useInput } from "@snapfire/fsr/terminal/ink";

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
fsr new mytool --terminal
fsr dev mytool/app
```

`fsr dev` redraws the app on every edit and keeps its state. `fsr run mytool/app` runs it once, as a user would. `fsr migrate` rewrites an existing Ink file's `ink` imports. [Running a Terminal App](cli/README.USAGE.md#running-a-terminal-app) lists what runs and what doesn't yet. The [counter example](examples/counter_terminal_ts/README.md) is the page above.

Ink's own examples run through terminal mode as one application each:

| Corpus | Examples | Runs in the terminal | Not run by design | Refused |
|---|---|---|---|---|
| [Ink](docs/corpus/ink.md) | 29 | 26 (89.7%) | 3 | 0 |

The three not run use `react-router` or an npm package the build doesn't follow. FSR's own routes replace `react-router`.

## Architecture

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

Application developers write TypeScript and never need to touch Rust. Platform developers can use Rust to add extensions, custom hosts, transports and renderers. They can also replace any part of the runtime.

[Chapter 000](docs/guide/000-what-fsr-is-made-of.md) of the guide explains the parts. [Chapter 900](docs/guide/900-the-parts-bin.md) lists each one by the problem it solves.

## Learn more

* [The guide](docs/guide/README.md) teaches FSR one question per chapter, with a lab on the running example.
* [The examples](examples/README.md) are complete applications in reading order, with what each one shows and the ports it uses.
* [The `fsr` CLI](cli/README.md) lists every command. `fsr lint app` runs ESLint's rules and FSR's own with no Node, covered in [chapter 307](docs/guide/307-linting.md).

## Special thanks

FSR stands on the work of these projects and the people who maintain them:

- [swc](https://swc.rs) parses, transforms and minifies TypeScript and JavaScript in `snapfirec`, the build and `fsr migrate`.
- [Oxc](https://oxc.rs) writes the `.d.ts` declarations `snapfirec` emits.
- [Lightning CSS](https://lightningcss.dev) parses, transforms and minifies every stylesheet.
- [Tailwind CSS](https://tailwindcss.com): `snapfire_tailwind` follows its v4 specification and carries its sources and tests.
- [markdown-rs](https://github.com/wooorm/markdown-rs) and the [MDX](https://mdxjs.com) team: FSR reads MDX through markdown-rs.
- [Taffy](https://github.com/DioxusLabs/taffy) lays out terminal apps with flexbox.
- [Ink](https://github.com/vadimdemedes/ink) defines the components and hooks terminal apps run.
- [TypeScript](https://www.typescriptlang.org): `snapfiretc` runs its native compiler.
- [QuickJS](https://bellard.org/quickjs/) and [rquickjs](https://github.com/DelSkayn/rquickjs) run the Vue and Svelte compilers and the test engine.
- [Vue](https://vuejs.org), [Svelte](https://svelte.dev) and [React](https://react.dev), whose compilers and server output FSR matches.
- [ICU4X](https://github.com/unicode-org/icu4x) formats dates, numbers, lists and plurals.
- [syntect](https://github.com/trishume/syntect) highlights code blocks.
- [Tokio](https://tokio.rs), [hyper](https://hyper.rs), [axum](https://github.com/tokio-rs/axum) and [tower](https://github.com/tower-rs/tower) serve every request.
- [Tera](https://keats.github.io/tera/) renders Tera templates.

*Designed to be your unstoppable, high-performance daily driver*

Most modern full-stack frameworks are JavaScript wrappers trying to solve systems-level performance problems with duct tape.

[Excerion Sun LLC](https://www.excsn.com)
