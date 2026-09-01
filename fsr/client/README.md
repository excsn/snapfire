# @snapfire/fsr-client

MPL-2.0. Pre-release, unpublished, part of the Snapfire FSR workspace.

The browser half of Snapfire FSR. It decodes the tagged JSON the server encoded, mounts island components over server-rendered markup, fills streamed slots as they arrive, navigates between routes by patching segments instead of reloading and calls server actions by id. The server halves it pairs with are `snapfire_fsr_payload`, which owns the JSON pair, the HTML serialiser and the row protocol, and `snapfire_fsr_runtime`, which owns segments, streaming and the action endpoint. Task-by-task instructions live in [README.USAGE.md](README.USAGE.md) and every signature is in [API_REFERENCE.md](API_REFERENCE.md).

The package is browser-native ES modules. There is no bundler step, no `node_modules` and no runtime dependency: the core entry imports nothing outside itself, and React lives behind a separate entry so a page that mounts Vue, Svelte or web components never loads it.

## Install

Installation is an import map entry, not a package manager install. Build the package with `snapfirec`, never `tsc`, since `tsc` drops the `.map` and `.min.js` outputs and the build facts file:

```sh
cd fsr/client
snapfirec --source-map --minify compact --public-path /static/js/fsr --import-map importmap.json
```

Serve the resulting `dist/` under the same prefix passed to `--public-path`, then name the two entry points in the page's import map. This is the map the `advanced_tera_app` example serves:

```json
{
  "imports": {
    "react": "/static/js/vendor/react/react.js",
    "react-dom/client": "/static/js/vendor/react/react-dom-client.js",
    "@snapfire/fsr-client": "/static/js/fsr/index.js",
    "@snapfire/fsr-client/react": "/static/js/fsr/react.js",
    "react/jsx-runtime": "/static/js/vendor/react/react-jsx-runtime.js"
  }
}
```

| Entry point | Output file | Bare imports it needs |
| --- | --- | --- |
| `@snapfire/fsr-client` | `dist/index.js` | none |
| `@snapfire/fsr-client/react` | `dist/react.js` | `react`, `react-dom/client` |

`react/jsx-runtime` is not imported by this package. It is what `snapfirec` emits for a `.tsx` component compiled under `"jsx": "react-jsx"`, so an application with JSX components needs the entry even though its components carry no React import.

## What to reach for

| You want to | Reach for |
| --- | --- |
| Turn the server's tagged JSON back into JS values | `decodeValue` |
| Send values to the server without losing width or type | `encodeValue` |
| Build a wide integer the server reads as `i128` or `u128` | a JS `bigint` |
| Carry a numeric series without one value per element | a typed array such as `Float64Array` |
| Carry a Rust enum as a discriminated union | `variant`, `isVariant` |
| Point at a server action or a client module | `actionRef`, `moduleRef`, `ref`, `isRef` |
| Say which component mounts for a module id | `registerIsland` |
| Mount every island on the page and keep up with streamed chunks | `boot` |
| Mount islands inside a subtree you inserted yourself | `scan` |
| Delay hydration until the island scrolls into view | `when: "visible"` |
| Mount React components | `reactMounter` from `@snapfire/fsr-client/react` |
| Mount anything else | your own `Mounter` |
| Take over link clicks and history | `enableNavigation` |
| Go to a route from code | `navigate` |
| Re-fetch the current route after a mutation | `refresh` |
| Call a server action by id | `action` |
| Match on why an action failed | `ActionFailure` and its `kind` |
| Parse a whole payload response yourself | `parsePayload` |
| Parse a single node row | `decodeNode` |
| Turn a decoded node back into HTML | `nodeToHtml`, `renderSegment` |

## Status

Pre-release and unpublished. It is not on npm and has no package manifest: consumers point an import map at the built `dist/`, which is how the `advanced_tera_app` example under `fsr/examples/` serves it. The package carries no test suite of its own; it is exercised through that example, whose Rust tests pin the HTML markers, the wire rows, the segment sidecar and the action responses this code reads. No API or wire compatibility guarantee is offered yet; the format number the reader reports is `FORMAT_VERSION` 1 from `snapfire_fsr_payload`.
