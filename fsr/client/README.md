# @snapfire/fsr/client

The code an FSR page runs in the browser. It brings the server's markup to life, moves between pages without a full reload and calls your server actions.

It handles the browser side of an FSR application:

* **Islands.** It hydrates each React, Vue or Svelte component over the HTML the server already rendered, on load, when it scrolls into view or when the browser is idle.
* **Streaming.** It fills the parts of a page that arrive later in the same response.
* **Navigation.** A link click fetches the next page's data and swaps in the parts that changed. The layout and the state of its islands stay.
* **Actions.** It calls a server action by name and refreshes the page with the result.
* **Shared state.** It holds a store that every island on the page reads, whichever framework mounts it.

You import from it in an island or in your application's entry module. If your application has no island code of its own, you never import it: `fsr build` writes the entry that boots it.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

There is nothing to install. The FSR host serves these modules at `/static/js/fsr`. The client a page runs is always the version of the host serving it. The host also adds an import map entry for each client module your bundle imports.

The modules are plain ES modules with no package manager step. The core entry imports nothing outside itself. React, Vue and Svelte each sit behind their own entry. A page loads only the frameworks its islands use.

| Entry point | Bare imports it needs in the import map |
| --- | --- |
| `@snapfire/fsr/client` | none |
| `@snapfire/fsr/client/react` | `react`, `react-dom/client` |
| `@snapfire/fsr/client/vue` | `vue` |
| `@snapfire/fsr/client/svelte` | `svelte`, `svelte/reactivity` |
| `@snapfire/fsr/client/jsx-runtime` | none |

A `.tsx` component compiled under `"jsx": "react-jsx"` also needs `react/jsx-runtime` in the map, even when it has no React import of its own.

If you compile your modules with `snapfirec` yourself rather than through `fsr`, your import map has to name the client entry points your code imports. This is the `advanced_tera_app` example's map:

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

## Quick start

The application's entry module registers its islands, mounts them and turns on client navigation. This is the whole entry of the `arrivals_react_ts` example:

```ts
import { boot, enableNavigation } from "@snapfire/fsr/client";
import { registerIslands } from "@generated/islands.js";

registerIslands();
boot();
enableNavigation();
```

An island calls an action by name. When the call succeeds, the page refreshes in place with the new data:

```tsx
import { action } from "@snapfire/fsr/client";

const lot = action("desk.lot");

export default function Lot({ size }: { size: bigint }) {
  return (
    <div className="lot">
      <button type="button" onClick={() => void lot({ by: -10 })}>&minus;</button>
      <output>{`${size}`}</output>
      <button type="button" onClick={() => void lot({ by: 10 })}>+</button>
    </div>
  );
}
```

An island can follow a topic the server publishes. Each publish reloads the route's data and patches the page:

```tsx
import { useEffect } from "react";
import { live } from "@snapfire/fsr/client";

export default function Follow({ room }: { room: string }) {
  useEffect(() => live([`room/${room}`]), [room]);
  return <span className="live">live</span>;
}
```

## What to reach for

| You want to | Reach for |
| --- | --- |
| Turn the server's encoded values back into JavaScript values | `decodeValue` |
| Send values to the server without losing their width or type | `encodeValue` |
| Send a wide integer the server reads as `i128` or `u128` | a JavaScript `bigint` |
| Send a whole number the server must read as a float | `f64` |
| Send a numeric series without one value per element | a typed array such as `Float64Array` |
| Send a Rust enum as a discriminated union | `variant`, `isVariant` |
| Point at a server action or a client module | `actionRef`, `moduleRef`, `ref`, `isRef` |
| Say which component mounts for a module id | `registerIsland` |
| Mount every island on the page and keep up with streamed parts | `boot` |
| Mount islands inside a subtree you inserted yourself | `scan` |
| Give a mounted island new props, keeping its DOM and state | `patchIsland`, `reactPatcher` |
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
| Fetch a route before its link is clicked | `prefetch`, on by default for hovered, focused and touched links |
| Drop payloads fetched before a change | `clearRouterCache`, which `refresh` also does |
| Go to a route from code | `navigate` |
| Reload the current route's data after a change | `refresh` |
| Refresh when the server publishes a topic | `live` |
| Send as fast as someone types and take rows back | `socket`, one WebSocket per topic |
| Call a server action by id | `action` |
| Send a file to an action | `upload` |
| Match on why an action failed | `ActionFailure` and its `kind` |
| Sign in from the page | `signIn`, `enrol`, `answer` |
| Render a page or load a route, query it by role and act as a user, under `fsr test` | `render`, `load`, `screen`, `userEvent`, `fireEvent`, `describe`, `it`, `expect` and `fn` from `@snapfire/fsr/client/testing` |
| Parse a whole payload response yourself | `parsePayload` |
| Parse a single node row | `decodeNode` |
| Turn a decoded node back into HTML | `nodeToHtml`, `renderSegment` |
| Open a file dialog or an external link from an `fsr --desktop` window | `openFile`, `openExternal` from `@snapfire/fsr/desktop`, which is experimental |

## License

This project is licensed under the **Mozilla Public License Version 2.0 (MPL-2.0)**. See the `LICENSE` file in the repository for the full license text.
