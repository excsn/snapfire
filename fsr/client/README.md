# @snapfire/fsr/client

The browser half of SnapFire FSR. It decodes the tagged JSON the server encoded, mounts island components over server-rendered markup, fills streamed slots as they arrive, navigates between routes by patching segments instead of reloading and calls server actions by id. The server halves it pairs with are `snapfire_fsr_payload`, which owns the JSON pair, the HTML serialiser and the row protocol; and `snapfire_fsr_runtime`, which owns segments, streaming and the action endpoint. Task-by-task instructions live in [README.USAGE.md](README.USAGE.md) and every signature is in [API_REFERENCE.md](API_REFERENCE.md).

The package is browser-native ES modules. There is no bundler step, no `node_modules` and no runtime dependency: the core entry imports nothing outside itself; React lives behind a separate entry so a page that mounts Vue, Svelte or web components never loads it.

## Install

Installation is an import map entry, not a package manager install. An application installs nothing at all. `snapfire_fsr_host` carries the modules and answers `/static/js/fsr` out of its own binary, so the client an application runs is the version of the host serving it. `fsr bundle` writes the same files under `serve/static/js/fsr` for a web server that answers the prefix before a request reaches the host. A `[[static]]` root on that route takes the prefix back, for an application that means to serve a client of its own.

Building the package is for working on it and uses `snapfirec` rather than `tsc`, since `tsc` drops the `.map` and `.min.js` outputs and the build facts file:

```sh
cd fsr/client
snapfirec --source-map --minify compact --public-path /static/js/fsr --import-map importmap.json
```

`host/embedded/client/` holds the copy the host serves, since `include_str!` cannot reach outside the crate, so a rebuilt `dist/` is copied there: the `.js`, the `.min.js`, the `.d.ts` and the `.map`, all four sets. The maps carry the TypeScript, since `tsconfig.json` sets `inlineSources`. A host built with `client_source_maps` serves them. The host carries both builds and `document.client` picks which one the plain names answer with, minified unless `server.dev` is on. A binary built with `default-features = false` on `snapfire_fsr_host` drops the readable set, saving about 215 KiB and serving minified whatever the setting says. A minified module imports its siblings as `./boot.min.js`, so those spellings are answered too and the graph a page loads follows its entry point. The host adds an import map entry for each module the application's bundle imports to the map it serves, wherever the application's map names none. `fsr dev` and `fsr build` compile against that map. An application that runs snapfirec itself compiles against its own map, so that map names the entry points its code imports. This is the `advanced_tera_app` example's:

```json
{
  "imports": {
    "react": "/static/js/vendor/react/react.js",
    "react-dom/client": "/static/js/vendor/react/react-dom-client.js",
    "@snapfire/fsr/client": "/static/js/fsr/index.js",
    "@snapfire/fsr/client/react": "/static/js/fsr/react.js",
    "react/jsx-runtime": "/static/js/vendor/react/react-jsx-runtime.js"
  }
}
```

| Entry point | Output file | Bare imports it needs |
| --- | --- | --- |
| `@snapfire/fsr/client` | `dist/index.js` | none |
| `@snapfire/fsr/client/react` | `dist/react.js` | `react`, `react-dom/client` |
| `@snapfire/fsr/client/vue` | `dist/vue.js` | `vue` |
| `@snapfire/fsr/client/svelte` | `dist/svelte.js` | `svelte`, `svelte/reactivity` |
| `@snapfire/fsr/client/jsx-runtime` | `dist/jsx-runtime.js` | none |

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
| Re-render a mounted island with new props, keeping its DOM and state | `patchIsland`, `reactPatcher` |
| End the islands in a subtree you are about to remove | `discard`, `reactUnmounter` |
| Delay hydration until the island scrolls into view | `when: "visible"` |
| Place a link, a picture, another framework's island, a video embed or a place in any framework | `Link`, `Picture`, `Mount`, `Embed`, `Place` from `@snapfire/fsr/components` |
| Mount React components | `reactMounter` from `@snapfire/fsr/client/react` |
| Mount Vue components | `vueMounter` from `@snapfire/fsr/client/vue` |
| Read the store as a Vue ref | `useStore` from `@snapfire/fsr/client/vue` |
| Mount Svelte components | `svelteMounter` from `@snapfire/fsr/client/svelte` |
| Read the store as a Svelte store | `useStore` from `@snapfire/fsr/client/svelte`, then `$count` |
| Place another framework's island in markup a Svelte component writes itself | `{@attach island(module, props)}` from `@snapfire/fsr/client/svelte` |
| Mount anything else | your own `Mounter` |
| Take over link clicks and history | `enableNavigation` |
| Warm a route before its link is clicked | `prefetch`, on by default for hovered, focused and touched links |
| Drop payloads held from before a mutation | `clearRouterCache`, which `refresh` does for you |
| Go to a route from code | `navigate` |
| Re-fetch the current route after a mutation | `refresh` |
| Follow the server and revalidate when it says so | `live`, over the host's event stream |
| Send as fast as someone types and take rows back | `socket`, one WebSocket per topic |
| Call a server action by id | `action` |
| Match on why an action failed | `ActionFailure` and its `kind` |
| Render a page or load a route, query it by role and act as a user, under `fsr test` | `render`, `load`, `screen`, `userEvent`, `fireEvent`, `describe`, `it`, `expect` and `fn` from `@snapfire/fsr/client/testing` |
| Parse a whole payload response yourself | `parsePayload` |
| Parse a single node row | `decodeNode` |
| Turn a decoded node back into HTML | `nodeToHtml`, `renderSegment` |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text.
