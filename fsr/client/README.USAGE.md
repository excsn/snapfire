# Usage Guide: @snapfire/fsr-client

How to build the package, register and hydrate islands, keep up with a streamed response, navigate without reloading, call actions and move values across the boundary without losing their type.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
  * [The Application Entry](#the-application-entry)
  * [A Component](#a-component)
  * [What the Server Emits](#what-the-server-emits)
* [Building and Serving the Package](#building-and-serving-the-package)
* [Registering an Island](#registering-an-island)
* [Choosing When an Island Hydrates](#choosing-when-an-island-hydrates)
* [Defining an Element When It Is Needed](#defining-an-element-when-it-is-needed)
* [Adopting a Shadow Root the Server Wrote](#adopting-a-shadow-root-the-server-wrote)
* [Importing FSR's Components](#importing-fsrs-components)
* [Placing an Island of Another Framework](#placing-an-island-of-another-framework)
* [Placing a Component as an Island](#placing-a-component-as-an-island)
* [Placing an Island in Server Mode](#placing-an-island-in-server-mode)
* [Filling a Layout's Slots](#filling-a-layouts-slots)
* [Sharing State Between a Layout and Its Page](#sharing-state-between-a-layout-and-its-page)
* [Mounting Vue Components](#mounting-vue-components)
* [Mounting Svelte Components](#mounting-svelte-components)
* [Writing a Mounter for Another Framework](#writing-a-mounter-for-another-framework)
* [Rescanning After Streamed Content Arrives](#rescanning-after-streamed-content-arrives)
* [Enabling Navigation](#enabling-navigation)
* [Marking the Link to the Page Being Shown](#marking-the-link-to-the-page-being-shown)
* [Showing That a Link Is Opening](#showing-that-a-link-is-opening)
* [Placing a Video Embed](#placing-a-video-embed)
* [Declaring a Place for Teleported Content](#declaring-a-place-for-teleported-content)
* [Placing a Picture](#placing-a-picture)
* [Prefetching and the Router Cache](#prefetching-and-the-router-cache)
* [Animating a Navigation](#animating-a-navigation)
* [Keeping the Page a Visitor Left](#keeping-the-page-a-visitor-left)
* [Loading a Third-Party Script](#loading-a-third-party-script)
* [Navigating and Refreshing From Code](#navigating-and-refreshing-from-code)
* [Wiring Another Library to the Navigator](#wiring-another-library-to-the-navigator)
* [Following the Server](#following-the-server)
* [Showing Whether the Page Is Online](#showing-whether-the-page-is-online)
* [Sending as Fast as Someone Types](#sending-as-fast-as-someone-types)
* [Calling an Action](#calling-an-action)
  * [Skipping Revalidation](#skipping-revalidation)
* [Sharing State Across Islands](#sharing-state-across-islands)
* [Reading the Locale](#reading-the-locale)
* [Formatting With the Standard Library](#formatting-with-the-standard-library)
* [Declaring a Native Pair](#declaring-a-native-pair)
  * [Seeding the Store From a Loader](#seeding-the-store-from-a-loader)
  * [Reading a Key in a Component](#reading-a-key-in-a-component)
  * [Writing From Anywhere](#writing-from-anywhere)
  * [Optimistic Updates](#optimistic-updates)
  * [Derived Keys and Transactions](#derived-keys-and-transactions)
* [Decoding Values From the Server](#decoding-values-from-the-server)
  * [Wide Integers](#wide-integers)
  * [Typed Arrays and Bytes](#typed-arrays-and-bytes)
  * [Variants and References](#variants-and-references)
* [Encoding Values For the Server](#encoding-values-for-the-server)
* [Reading a Payload Response by Hand](#reading-a-payload-response-by-hand)
* [Rendering Nodes Back to HTML](#rendering-nodes-back-to-html)
* [Calling the Desktop Window](#calling-the-desktop-window)
* [Error Handling](#error-handling)

## Core Concepts

* **Value model**: the set of things that can cross the boundary. It is wider than JSON, so the encoding tags whatever JSON cannot spell natively.
* **Tagged JSON**: an object carrying a `$` key names a value the plain JSON grammar cannot carry, such as a wide integer, a typed array or a variant. Untagged JSON passes through untouched.
* **Node**: one entry in the payload tree. Its five kinds are `text`, `raw`, `seq`, `client` and `pending`.
* **Island**: a `client` node. The server renders it inside an `<sf-i>` marker with its props in a sibling JSON script tag; the browser mounts a component over that markup.
* **Module id**: the string that names a component, source path plus export, for example `components/ServerChart.tsx#default`. It is the key `registerIsland` is called with and the value of the marker's `data-sf-module`.
* **Mounter**: the function that turns a loaded module plus props into a mounted component in an element. React has one in the `/react` entry, Vue one in the `/vue` entry and Svelte one in the `/svelte` entry; every other framework plugs in the same way.
* **FSR's components**: `Link`, `Picture`, `Mount`, `Embed` and `Place`. Every framework imports them from `@snapfire/fsr/components` and the build points that import at the importing file's framework.
* **Hydration timing**: per island, `"load"`, `"visible"` or `"idle"`. It decides when the loader runs, not whether the island exists.
* **Slot**: a hole a deferred segment fills later. It renders as `<div data-sf-slot="N">` holding a fallback until its content arrives.
* **Segment**: a region of the page with a comparable key, delimited in the HTML by `<!--sf-g:key-->` and `<!--/sf-g-->` comments. Same key across two responses means the region survives navigation.
* **Segment sidecar**: the `G` row or the `script[data-sf-segments]` tag in an HTML response, carrying the segment tree the navigator diffs against.
* **Segment digest**: the fingerprint each segment carries of what it rendered, its child segments elided. It says whether that segment changed, where the key only says which segment it is.
* **Payload response**: the line-oriented wire format, requested by adding `__payload` to a route's query string. One `V` row, one `N` row, an optional `G` row, an optional `H` row with the document's title and description and one `S` row per resolved slot, each with an `H` row of its own when the slot's segment described the document.
* **Action**: a server function with a stable id. The client holds the id, never a URL shape.
* **Revalidation**: re-fetching the current route after a mutation and replacing the top-level segment regions, so the layout's DOM and its island state survive.
* **Store**: one keyed map for the whole document, outside every island root. Islands do not share React context, so this is how two of them show the same number.
* **Seed**: what a route's segments settled the store on for this request, one contribution per segment merged by position, rendered into the document as `script[data-sf-store]` and carried by a navigation's `T` rows ahead of the markup. The server renders from it and each island's props carry the values it was rendered from, so a component reading a key hydrates without a flash or a mismatch.

## Quick Start

### The Application Entry

One module registers every island, boots them and takes over navigation:

```ts
import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { reactMounter, reactPatcher, reactUnmounter } from "@snapfire/fsr-client/react";

registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
});

registerIsland("components/LatencyChart.tsx#default", {
  loader: () => import("./LatencyChart.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
  when: "visible",
});

boot();
enableNavigation();
```

### A Component

Props arrive already decoded, so a numeric series is a real `Float64Array` rather than an array of numbers. Under `"jsx": "react-jsx"` the component needs no React import for JSX:

```tsx
export default function LatencyChart({ series }: { series: Float64Array }) {
  const points = Array.from(series);
  const avg = points.reduce((a, b) => a + b, 0) / (points.length || 1);
  return (
    <p className="sf-latency">
      latency avg {avg.toFixed(2)}ms over {points.length} samples
    </p>
  );
}
```

### What the Server Emits

The markup the client looks for, one island and one unfilled slot:

```html
<sf-i id="sf-i0" data-sf-module="components/ServerChart.tsx#default">…server-rendered markup…</sf-i>
<script type="application/json" data-sf-props="sf-i0">{"series":{"$":"ta","k":"f64","v":"…"}}</script>
<div data-sf-slot="1">…fallback…</div>
```

## Building and Serving the Package

Build with `snapfirec`, from the package directory:

```sh
cd fsr/client
snapfirec --source-map --minify compact --public-path /static/js/fsr --import-map importmap.json
```

`--import-map` fails the build when a bare import has no entry, which is what keeps `react` and `react-dom/client` declared.

Nothing serves `dist/` in an application. `snapfire_fsr_host` carries the modules and answers `/static/js/fsr` from its own binary. `fsr bundle` writes them under `serve/static/js/fsr` for a web server that answers the prefix first. A rebuilt `dist/` reaches the host by being copied into `host/embedded/client/`, which is where `include_str!` reads it: the `.js`, the `.min.js`, the `.d.ts` and the `.map`. The host serves the maps, which carry the TypeScript, only when built with its `client_source_maps` feature; without it the modules go out without their `sourceMappingURL` comment. Both builds ride in the binary and `document.client` decides which one `/static/js/fsr/<name>.js` answers with, `auto` (minified unless `server.dev` is on), `readable` or `minified`. The readable set is the `client_readable` feature, on by default; without it the binary is about 215 KiB smaller and answers minified whatever `document.client` says. Point the page's map at the two entry points:

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

Build the application's own modules the same way, against that map:

```sh
cd fsr/examples/advanced_tera_app/js
snapfirec -c tsconfig.build.json --source-map --minify compact --public-path /static/js/app --import-map importmap.json
```

Two tsconfigs, the way an FSR application's build writes two. `tsconfig.build.json` is what snapfirec compiles from and names no declarations, since a `paths` entry pointing at a `.d.ts` would be rewritten into the output as an import of a file that does not exist. `tsconfig.json` is the one an editor picks up and the one to typecheck, carrying `strict`, `noEmit` and a `paths` entry per package in the map:

```sh
fsr types .            # fills types/ from the import map
snapfiretc --root . --config tsconfig.json
```

## Registering an Island

An entry is a loader, a mounter, an optional patcher, an optional unmounter and an optional timing. The loader resolves to whatever the mounter expects, which for `reactMounter` is the component itself; the patcher re-renders the island with new props when a navigation or a revalidation keeps its DOM; the unmounter ends it when a navigation takes its marker out of the document, so its effects clean up:

```ts
import { registerIsland } from "@snapfire/fsr-client";
import { reactMounter, reactPatcher, reactUnmounter } from "@snapfire/fsr-client/react";

registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
  unmount: reactUnmounter,
});
```

The key must be the exact `data-sf-module` string the server wrote. A page or a layout is composition: the server renders it, nothing renders it again in the browser and it is not registered. The one exception is a layout whose state reaches one of its slots, which the build keeps as a React root and reports under `kept`; `reactMounter` recognises the `<sf-s>` in its markup and hands the component a child element it never reconciles, so the page inside keeps its own region and a navigation swaps it under the live layout. A marker whose module id is not registered is left server-rendered and logged:

```
sf: no island registered for components/ServerChart.tsx#default
```

The warning waits for the document to settle, because a miss is not yet a defect. A mounted site puts a second entry module in the head and registers its islands when that module runs, which is after the shell's `boot` has already scanned them; the same holds for the entry a payload names on the first navigation into a site. So a module id missing at scan time is remembered rather than reported and warned about only once every deferred script has run and nothing named by `loadEntry` is still in flight.

## Choosing When an Island Hydrates

`when` is per island, not per page; it defaults to `"load"`:

```ts
registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
});

registerIsland("components/LatencyChart.tsx#default", {
  loader: () => import("./LatencyChart.js").then((m) => m.default),
  mount: reactMounter,
  when: "visible",
});

registerIsland("components/Preferences.tsx#default", {
  loader: () => import("./Preferences.js").then((m) => m.default),
  mount: reactMounter,
  when: "idle",
});
```

Pick `"load"` for anything above the fold or interactive immediately, `"visible"` for content the reader has to scroll to, `"idle"` for work that can wait for a quiet main thread. `"visible"` observes the element with an `IntersectionObserver` and disconnects on the first intersection; `"idle"` uses `requestIdleCallback` where the browser has it and a 1ms `setTimeout` where it does not.

## Defining an Element When It Is Needed

A custom element has no mount: the browser upgrades it the moment `customElements.define` runs, wherever its markup already is. So the thing worth deferring is the definition, which is what the island timing schedules when a template writes `<Island define>`:

```tsx
<Island when="visible" define="@src/elements/time-ago.ts">
  <ul>
    {loans.map((loan) => (
      <li key={loan.tool}>
        <time-ago datetime={loan.back}>back {loan.back}</time-ago>
      </li>
    ))}
  </ul>
</Island>
```

The child is an element rather than a component, its markup is the server's, with nothing mounted over it. The build registers the module with `defineMounter`, whose mount does nothing, so the island machinery imports the module at the timing asked for, at which point the elements inside upgrade themselves. A definition module must be a module: give it an `export` so it can be imported dynamically.

Without this the definition runs when the entry module does, which is right for an element the first paint needs, such as a masthead's, but wasteful for one below the fold.

## Adopting a Shadow Root the Server Wrote

The server writes a custom element's shadow template as a `<template shadowrootmode>` inside the element, open unless the template declares its own root. The parser attaches it before any script runs. Markup that arrives through `innerHTML` keeps the template as an ordinary child instead, which is what an htmx swap does. `shadowOf` answers both:

```ts
import { shadowOf } from "@snapfire/fsr-client/elements";

class LoanPlanner extends HTMLElement {
  static formAssociated = true;
  #internals = this.attachInternals();

  connectedCallback(): void {
    const range = shadowOf(this, this.#internals)?.querySelector<HTMLInputElement>("input[name=days]");
    range?.addEventListener("input", () => this.#internals.setFormValue(range.value));
  }
}
```

It returns the root the parser attached. With none, it attaches one from the template child with the mode and options that template carries, takes the template out and returns that. A closed root is reached through the element's `ElementInternals`, so pass them when the template says `shadowrootmode="closed"`. Without them `shadowOf` cannot tell a closed root from none and returns `null`. The navigator writes what it applies with `setHTMLUnsafe` where the browser has it, so a page reached by a link keeps its shadow roots without this.

## Importing FSR's Components

`Link`, `Picture`, `Mount`, `Embed` and `Place` are imported from `@snapfire/fsr/components` in a `.tsx`, a `.vue` or a `.svelte` file and written the same way in each:

```tsx
import type { Children } from "@snapfire/fsr-authoring/template";
import { Link, Place } from "@snapfire/fsr/components";

export default function Layout({ children }: { children: Children }) {
  return (
    <>
      <Link href="/tonight">Tonight</Link>
      <Place name="notices" className="notices" />
      {children}
    </>
  );
}
```

```svelte
<script lang="ts">
  import { Link, Picture } from "@snapfire/fsr/components";
</script>

<Link href="/next" class="next">next</Link>
<Picture src="/pictures/shelf.png" alt="shelf" width={320} height={200} />
```

The server renders each one the same way whatever framework the file is in. `fsr build` rewrites the import in the browser bundle for each file that has it:

| Importing file | The import becomes |
| --- | --- |
| `.vue` | `@snapfire/fsr-client/vue` |
| `.svelte` | one import per component, from the Svelte component the build writes for it under `generated/components` and compiles with the application's Svelte, so a file loads only the components it names. `@generated/components` exports all five for a file that wants them together. |
| any other, reached by an island | that island's adapter |
| any other, in an application that serves React | `@snapfire/fsr-client/react` |
| any other | `@snapfire/fsr-client/jsx-runtime` |

`tsconfig.json` maps the specifier to React's declarations when the application serves React and to the template dialect's otherwise. The per-framework modules keep exporting the same components, so an existing import of them still works.

The import has meaning only to `fsr build`. A bundle compiled with `snapfirec` directly imports the framework's own module.

## Placing an Island of Another Framework

`Island` places a React child. A React tree holding an island **another** framework mounts needs the module id instead, since React cannot render a Vue component and should not try:

```tsx
import { Mount } from "@snapfire/fsr/components";

<Mount module="src/ui/Sparkline.vue#default" props={{ points, up }} when="visible" />;
```

It writes the marker the boot runtime reads, with the props beside it, mounting nothing itself: the registry entry for that module decides the mounter. When the props change the nested island is patched in place rather than torn down. The Vue entry exports the same component for the other direction, so a Vue table row can hold a React chip:

```vue
<Mount module="src/ui/Chip.tsx#default" :props="{ symbol: row.symbol }" />
```

Nothing rendered these on the server, so both are mounted fresh. `Island` stays the one to use for a child of your own framework, since it adopts the region the server rendered.

## Placing a Component as an Island

A page or a layout is composition, so a component it renders inline that holds state of its own is placed as an island by the renderer, mounting on load. `Island` from the React adapter places one by hand, with the timing on the use:

```tsx
import { Island } from "@snapfire/fsr-client/react";
import { OrderHelp } from "@src/ui/OrderHelp";

<Island when="visible">
  <OrderHelp orderId={order.id} />
</Island>
```

The build lowers the use: the server renders `OrderHelp` with its props as a nested island inside an `<sf-s data-sf-island data-sf-when="visible">` region of the page's markup with its own props script. It registers the module in `generated/islands.ts`. In the browser `scan` mounts `OrderHelp` in a root of its own when it scrolls into view, so its state is its own and a navigation that keeps the page leaves it alone.

A placement inside a `.map` is one region per item and the build names each of them, so the page can grow a list without an island taking a region that belongs to another item. When the page revalidates, every island under it takes the new props: a placement that is still there keeps its root, its DOM and its state, one the new data added is mounted from the payload and one it dropped goes with its item. Nothing about that is written by hand. `island(OrderHelp, { when: "visible" })` at module level gives a component that places itself the same way wherever it is used. `when` on the region wins over the registry's timing for that use; a use without one takes the registry's, else `"load"`. A component from another framework works the same way once its module has a mounter registered, since the registry picks the mounter by module.

## Placing an Island in Server Mode

An island whose events round-trip to the server. The browser holds its props and state, every bound event is sent to the server, Rust runs the handler and renders the component again and the markup that comes back is patched into place. No component code runs in the browser, so the component has no JavaScript half to keep in step with the server.

```tsx
<Island when="visible" mode="server">
  <OrderHelp orderId={order.id} />
</Island>
```

`island(OrderHelp, { mode: "server" })` is the same as a module-level alias, declared beside the use or exported from the component's own module and imported, by name or through a namespace import; the build follows the import and places the island the alias declared. The component is written as React; the mode decides where its handlers run. The build lowers each handler, which must be `const`s, calls to state setters and calls to actions (`e.preventDefault()` aside) and refuses the mode over a handler it cannot lower, in the island's component or in one it renders. It refuses it over a slot the island renders too, `{children}` included, since a step renders the island's own component alone. A component inside the island may hold state and handlers of its own: each instance is addressed by its place in the island's markup, its state rides in the island's under that address and a click inside it round-trips through the island. An island placed inside it keeps its marker and children across a patch and takes the props the render gave it. An action a handler calls, `void save({ id })` with `save` an `action("desk.save")`, is dispatched by the host during the round trip, with the session and everything else the action route would give it; the answer tells the island to refresh the page's data the way a browser-mode call does, so what the action wrote reaches every other island on the page. The server marks each bound element with `data-sf-on="click:0"` and the island's initial state rides in its props as `$s`; `scan` mounts an `sf-i` under an `sf-s[data-sf-mode="server"]` through `mountServer` instead of the registry, with `data-sf-pending` on the island while a round trip is out and a second event during one dropped. A list the server renders keeps a moved element across a patch when the element carries a `key`, which prints as `data-sf-key` in this mode.

## Filling a Layout's Slots

A layout has one slot for its page and as many named ones as it places. A parallel segment under `slots/<name>/` beside the layout arrives as a prop of that name; a slot an intercepted route opens in is placed with `Slot`:

```tsx
import { Slot } from "@snapfire/fsr-client/react";

export default function Layout({ cartCount, children, promo }: LayoutProps & { children: ReactNode; promo: ReactNode }) {
  return (
    <>
      <Header cartCount={cartCount} />
      {promo}
      {children}
      <Slot name="modal" />
    </>
  );
}
```

Both are `<sf-s data-sf-name>` regions in the server's markup, the way `children` is a bare `<sf-s>`. Children of `Slot` or `{promo ?? <p>…</p>}` for the prop form, are the fallback the region shows while nothing fills it, rendered by the server and put back when a navigation empties the slot. Navigation fills and empties them: a soft navigation to a route with a `page.modal.tsx` writes the variant into the `modal` region of the nearest live layout that declares it and leaves the page alone and the navigation away empties it again. A document load renders the page, never the variant. The promo keeps its DOM across every page under the layout, since its key never changes.

The navigator sends the document's path with every soft request, which is how the server knows an intercept applies. A link says otherwise with `Link`:

```tsx
import { Link } from "@snapfire/fsr/components";

<Link href={`/product/${id}`} full>Full details</Link>
<Link href={`/product/${id}`} into="modal">Quick look</Link>
```

`full` asks for the document's rendering of the target whatever the origin; `into` names the slot outright, for a link the server would not match. On any anchor the same is `data-sf-full` and `data-sf-into`. `refresh` re-renders an open intercept in its slot over the page it keeps.

A navigation that changes only the query keeps the page's islands: its region is morphed rather than replaced, so every island the new markup places again keeps its DOM and its state and takes the new props. `navigate` says otherwise for one navigation with `keep`, as does a `Link` with `keep` or an anchor with `data-sf-keep`. `replace` puts the target in place of the current history entry and `scroll: false` leaves the window where it is. Together they suit a control that navigates on every change:

```ts
import { navigate } from "@snapfire/fsr-client";

void navigate(`/wave/${id}?at=${step}`, true, { replace: true, scroll: false });
void navigate("/?category=printing", true, { keep: false });
```

## Sharing State Between a Layout and Its Page

A layout and its page are composition and share no framework root, so React context a layout provides reaches none of the page's islands. A value both read lives in the store, which every island's adapter binds:

```tsx
// src/ui/ModeSwitch.tsx, placed in the layout
import { useStore } from "@snapfire/fsr-client/react";
import { mode } from "@src/store";

export function ModeSwitch() {
  const [current, setMode] = useStore(mode, "light");
  return <button onClick={() => setMode(current === "light" ? "dark" : "light")}>{current}</button>;
}
```

```tsx
// src/ui/Abstract.tsx, placed in the page
import { useStore } from "@snapfire/fsr-client/react";
import { mode } from "@src/store";

export function Abstract({ text }: { text: string }) {
  const [current] = useStore(mode, "light");
  return <p className={`abstract ${current}`}>{text}</p>;
}
```

A navigation swaps the page's region and leaves the layout's DOM alone, so the switch keeps its state and the new page's islands read the store as it stands. A static layout whose own markup changed is morphed around the page after the page's region is settled, so `keep: false` still replaces the page. The conference example's two layouts are composition: a click from one talk to another renders the new talk under the same crumbs and the same masthead.

## Mounting Vue Components

The `/vue` entry is the second mounter the package ships. Register a `.vue` module with it and Vue mounts the component in the marker, creating the app over the server's markup when there is any, which is what the build writes for a component it lowered. It creates the app fresh when there is none, which is what a component the build could not read gets:

```ts
import { registerIsland } from "@snapfire/fsr-client";
import { vueMounter, vuePatcher, vueUnmounter } from "@snapfire/fsr-client/vue";

registerIsland("src/ui/Tonight.vue#default", {
  loader: () => import("../src/ui/Tonight.vue").then((m) => m.default),
  mount: vueMounter,
  patch: vuePatcher,
  unmount: vueUnmounter,
});
```

`fsr build` writes exactly that registration for every `.vue` island a template places, so an application never writes it by hand. The mounter holds the island's props in a reactive object and renders the component through a one-element wrapper, which is what lets `vuePatcher` hand a mounted island new props in place rather than tearing it down. `vueUnmounter` unmounts the app when a navigation takes the marker out of the document. The runtime's own keys on the props, the hoisted table and the region key, never reach the component.

The island's children are its default slot. The server writes them in an `<sf-s data-sf-children>` region where the template placed its `<slot />`, which Vue hydrates as an element the wrapper rendered and whose content is adopted as it stands, nested islands included. When the template placed no slot this render, a panel behind a closed `v-if` for one, the server writes them after the markup in an inert `<template data-sf-children>`; the mounter reads it before Vue hydrates and takes it out of the document, so the slot has its content when the template opens.

Inside the component the store is a ref:

```ts
import { useStore } from "@snapfire/fsr-client/vue";
import { plannedCount } from "@src/store";

const held = useStore(plannedCount, props.count);
held.value += 1;
```

Call it in `setup`: the subscription ends with the component's scope. Writing `.value` writes the store, so every other island reading that key follows, React or Vue.

A React island uses a Vue component as JSX. In an application that serves React, the build turns the import into a `foreign` component that places the Vue island by id, mounted fresh in the region the server leaves empty for it:

```tsx
import { useState } from "react";
import Chart from "./Chart.vue";

export function Panel() {
  const [n, setN] = useState(0);
  return (
    <section>
      <button onClick={() => setN(n + 1)}>{n}</button>
      <Chart label="sales" />
    </section>
  );
}
```

`Chart` takes props and no children: React children cannot cross into a Vue component. The build refuses them.


## Mounting Svelte Components

The `/svelte` entry mounts a Svelte 5 component, runes or legacy mode. `fsr build` registers every `.svelte` island a template places with it, the same way it registers a `.vue` one:

```ts
import { registerIsland } from "@snapfire/fsr-client";
import { svelteMounter, sveltePatcher, svelteUnmounter } from "@snapfire/fsr-client/svelte";

registerIsland("src/ui/Counter.svelte#default", {
  loader: () => import("../src/ui/Counter.svelte").then((m) => m.default),
  mount: svelteMounter,
  patch: sveltePatcher,
  unmount: svelteUnmounter,
});
```

When the server rendered the component, the mounter hydrates that markup rather than mounting fresh. The region the server wrote the island's children in is kept as it stands. A patch hands the component new props without remounting it, so a `$state` the component holds survives a revalidation. The island's children arrive as its `children` snippet:

```svelte
<script lang="ts">
  import type { Snippet } from "svelte";
  import { useStore } from "@snapfire/fsr-client/svelte";
  import { cartCount } from "@src/store";

  let { label, children }: { label: string; children?: Snippet } = $props();
  const count = useStore(cartCount, 0);
</script>

<p>{label}: {$count}</p>
<button onclick={() => ($count = $count + 1)}>add</button>
{@render children?.()}
```

`useStore` hands back a Svelte store, so `$count` reads the key and `$count = …` writes it, in a legacy component as in a runes one. Every other island reading the key follows, whether React, Vue or Svelte mounts it.

A `.svelte` file imports FSR's components from `@snapfire/fsr/components`, as [Importing FSR's Components](#importing-fsrs-components) shows. The entry also exports the helpers they are built on, for markup an author writes themselves. `link` is an anchor's attributes, `picture` the image's parts and `island` an attachment that places another framework's island by module id:

```svelte
<script lang="ts">
  import { island, link, picture } from "@snapfire/fsr-client/svelte";

  let { label }: { label: string } = $props();
  const image = picture({ src: "/pictures/shelf.png", alt: "shelf", width: 320, height: 200 });
</script>

<a {...link({ href: "/next" })}>next</a>
<img {...image.img} />
<div {@attach island("src/ui/Chart.tsx#default", { label })}></div>
```

When `label` changes, Svelte runs the attachment again and the chart is patched in place rather than mounted again.
## Writing a Mounter for Another Framework

A `Mounter` receives the loaded module, the decoded props, the marker element and whether server-rendered markup is already inside it. Its return value is kept by the caller and handed back to the entry's `Unmounter` when a navigation takes the marker out of the document, so return whatever the framework needs for teardown:

```ts
import { registerIsland, type Mounter, type Props, type Unmounter } from "@snapfire/fsr-client";
import { mount, unmount, type Component } from "svelte";

const svelteMounter: Mounter = (module, props, el, hydrate) => {
  if (!hydrate) el.replaceChildren();
  return mount(module as Component, { target: el, props: props as Record<string, unknown> });
};

const svelteUnmounter: Unmounter = (handle) => {
  void unmount(handle as Record<string, unknown>);
};

registerIsland("components/Counter.svelte#default", {
  loader: () => import("./Counter.js").then((m) => m.default),
  mount: svelteMounter,
  unmount: svelteUnmounter,
});
```

An entry without `unmount` has its root dropped as it stands, which leaves the framework's effects running on a detached element.

`hydrate` is true when the marker has child nodes. The React mounter in the `/react` entry reads exactly the same flag:

```ts
export const reactMounter: Mounter = (component, props, el, hydrate) => {
  const element = createElement(component as never, props as never);
  if (hydrate) {
    return hydrateRoot(el, element);
  }
  const root = createRoot(el);
  root.render(element);
  return root;
};
```

## Rescanning After Streamed Content Arrives

`boot` scans the document once the DOM is ready and again on every `sf:fill` event, which the server's inline fill script dispatches after it moves a resolved template into its slot. Islands inside a late chunk mount without any extra call:

```ts
import { boot } from "@snapfire/fsr-client";

boot();
```

`scan` is what does the work. It is idempotent: it only matches `sf-i` markers without `data-sf-scheduled` and it stamps that attribute as it takes each one. `data-sf-mounted` is a different statement, stamped when the mounter has run, so an island waiting on `visible` or `idle` carries the first and not yet the second. Call `scan` directly for markup you inserted yourself:

```ts
import { scan } from "@snapfire/fsr-client";

const host = document.querySelector("#panel")!;
host.innerHTML = fetchedMarkup;
scan(host);
```

Props are looked up by the marker's `id` inside the scanned root first, then across the document, so a fragment carrying its own props script works either way.

## Enabling Navigation

`enableNavigation` reads the segment sidecar the server embedded, intercepts same-origin link clicks and owns history from then on:

```ts
import { enableNavigation } from "@snapfire/fsr-client";

enableNavigation();
```

It wires the document once. A mounted site's entry module calls it again when a navigation imports the site's islands, and that call keeps the page the navigation installed, the spine the navigator holds and the listeners already on the document; only an option it names, `prefetch` or `cacheMs`, changes.

It also hangs `refresh` on `window.__sf`, which is how the stock host's development script refreshes an open page in place after a change. A click is left alone when it is already default-prevented, is not the primary button, carries a modifier key, has no enclosing `a[href]` or points at another origin. Everything else fetches the route's payload and patches only the segments that rendered something different, so the layout's DOM, its scroll position and any island state above the changed region survive. Sameness is the digest each segment carries, not its key, which is what keeps a pane that ignores a query parameter when the URL's query moves under it. A kept island whose props changed is re-rendered in place through its patcher rather than replaced and one whose digest held is left alone entirely. When the sidecar is missing or a segment's region cannot be found in the DOM, the navigator falls back to a full load rather than guessing.

## Marking the Link to the Page Being Shown

A `<Link>` is marked `aria-current` when it points at the page the document is showing, so a nav needs no state of its own and the styling is a selector:

```tsx
import { Link } from "@snapfire/fsr/components";

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <>
      <nav>
        <Link href="/billing" match="prefix">Billing</Link>
        <Link href="/billing/overdue">Overdue</Link>
      </nav>
      {children}
    </>
  );
}
```

```css
nav a[aria-current] {
  background: #eef3fc;
}

nav a[aria-current="page"] {
  font-weight: 600;
}
```

On `/billing/overdue` the section link carries `aria-current="true"` and the page's own carries `aria-current="page"`, so a screen reader is told about one current page rather than two. The default rule is `match="exact"`, which marks only the path the `href` names; `match="none"` leaves the anchor alone, as does an `aria-current` you write yourself. An `href` carrying a query or a fragment never matches, since the path a request matched holds neither.

The server writes the mark from the path the request matched, so it is right at first paint, in a document with no script and in a prerendered one. A navigation re-renders the page segment and leaves the layout alone, so the navigator re-reads every marked link afterwards rather than waiting for the nav to render again.

An intercepted navigation changes the address and leaves the page beneath in place. A link is judged by the address: a modal's link to its own full page is marked while the modal is open and a nav item whose section the target is outside goes dark until it closes. A nav that describes the page beneath says `current="document"` and is judged by that page instead, on the server and in the browser alike:

```tsx
<nav>
  <Link href="/agents" match="prefix" current="document">Agents</Link>
  <Link href="/help" current="document">Help</Link>
</nav>
<Link href="/settings" into="drawer" aria-label="Settings">⚙</Link>
```

Open the drawer from the agent list and `Agents` keeps its mark while the gear takes one. `current="url"` is the default and the two agree whenever no intercept is open. On a plain anchor the same is `data-sf-current="document"` beside `data-sf-link`.

## Showing That a Link Is Opening

The link a click follows carries `data-sf-pending` from the click until the page it names is shown, so a stylesheet can show the wait with no script of its own:

```css
a[data-sf-pending] {
  opacity: 0.7;
}
```

A component inside the link reads the same thing with `useLinkStatus`, the counterpart of Next's hook. Each adapter has one:

```tsx
import { useLinkStatus } from "@snapfire/fsr-client/react";

export default function Opening() {
  const { pending } = useLinkStatus();
  return <span className="opening">{pending ? "opening…" : ""}</span>;
}
```

```tsx
<Link href={`/talk/${talk.id}`}>
  {talk.title}
  <Opening />
</Link>
```

The component reads the `Link` it renders inside when an island renders that link. Otherwise it reads the link the server placed the island in. So the same component works in a layout the server renders and inside an island. The server renders it with `pending` false, since nothing is pending at first paint. `pending` turns false once the eager part of the new page is shown, before its deferred segments arrive. A second click moves it to the second link. A navigation started from code with `navigate` has no link and marks none. Vue's `useLinkStatus()` returns a reactive `{ pending }`; Svelte's is a store of `{ pending }`, read as `$status.pending`. `pendingLink()` and `subscribePendingLink(watcher)` from `@snapfire/fsr-client` are the neutral form for any other library.

## Animating a Navigation

Turn on view transitions and the navigator swaps the page inside `document.startViewTransition`, so the browser animates from the old page to the new one. With no CSS the browser crossfades the whole page:

```ts
enableNavigation({ transitions: true });
```

The animation is CSS. An element given a `view-transition-name` holds still while the rest fades. The named pseudo-elements take any animation, a slide for one:

```css
header.site {
  view-transition-name: site-header;
}

::view-transition-old(root),
::view-transition-new(root) {
  animation-duration: 180ms;
}
```

A link turns it off for itself with `data-sf-transition="none"` and on with `data-sf-transition` when the document leaves it off. From code, `navigate(href, true, { transition: false })` does the same. The navigator skips the transition where the browser has no view transitions, when the visitor's system asks for reduced motion and when the tab is hidden, so the swap happens at once in those cases. Only a navigation animates: a `refresh` after an action and a scroll to a fragment on the same page do not.

## Keeping the Page a Visitor Left

Turn on activity and a navigation keeps the page it replaces out of the document instead of ending it. Going back to that page, through history or by a link, puts the held page back and patches it to the new payload:

```ts
enableNavigation({ transitions: true, activity: true, activityLimit: 10 });
```

What comes back is what the visitor left: the same elements, text typed into a field, a video at the second it was paused on and a custom element with whatever it held, since the browser disconnects the element and connects it again. A step back through history also returns the window to where it was scrolled, once the page's paused islands are shown again; a link scrolls as any navigation does. A React island on React 19.2 or later is paused under React's `<Activity>` and comes back with its state. A Vue island would keep running while its DOM is out of the document, as would a React island on an earlier React. Such an island is unmounted when the page is held and mounts afresh from the payload when the page comes back. A server island answers again the same way.

`activityLimit` is how many pages are held, 10 by default, the one left longest ago dropped first. An action that revalidates, a `refresh` and a move of the session drop every held page, so nothing from before a mutation comes back after it. A link opts out with `data-sf-activity="none"` and in with `data-sf-activity` when the document leaves it off; `navigate(href, true, { activity: false })` does the same from code.

## Loading a Third-Party Script

`loadScript` adds a vendor's classic script once per document at the timing it is given and resolves when the script has run:

```ts
import { loadScript } from "@snapfire/fsr-client";

await loadScript("https://chat.example/widget.js", { when: "interaction" });
```

Calling it again for the same URL returns the same promise, so a module can call it on every navigation without adding a second element. `"idle"` waits for the main thread to be idle and `"interaction"` for the visitor's first key, pointer, touch or scroll. A route that needs a script declares it in its `meta` with `scriptTag` from `@snapfire/fsr/head` instead; the boot runtime and the navigator load those rows through the same function, as guide chapter 305 describes.

## Prefetching and the Router Cache

A link's payload is fetched when the pointer moves over it, when it takes focus or when it is touched, then held for thirty seconds. The click that follows applies the held payload with no round trip. So does a back or forward that lands on a route fetched inside the window. `enableNavigation` takes the document's timing and the lifetime:

```ts
enableNavigation({ prefetch: "none" });
enableNavigation({ prefetch: "viewport" });
enableNavigation({ cacheMs: 5_000 });
```

`"viewport"` fetches a link's payload as it scrolls into view rather than waiting for a pointer, which is what a long list of links wants; each link is fetched once and then left alone. It costs a request per link on screen, so it is the document's choice rather than the default.

A link to the page being shown is never prefetched under either timing. After a view transition the pointer rests on the link it clicked, which now names the page on screen.

A link decides for itself, whichever way the document leans. `none` is right for a link whose loader is expensive or whose route logs the visit and `viewport` for the one link on the page that is almost certainly next:

```html
<a href="/reports/yearly" data-sf-prefetch="none">Yearly report</a>
<a href="/checkout" data-sf-prefetch="viewport">Checkout</a>
```

A held payload is valid only under the session generation it was fetched in. The host sets `sf_state`, a cookie the page reads, with a fresh value whenever it saves a written session, whoever wrote it: the client's own `action`, a form htmx posted straight to the action route, a request another tab made. A payload prefetched before the write is dropped at the next lookup rather than applied, which is what keeps a hovered link from showing the page as it was before the click that changed it.

From code, `prefetch` warms a route ahead of time and `clearRouterCache` drops everything held. `refresh` drops it on its own, so an action that revalidates never leaves a payload from before the mutation behind:

```ts
import { clearRouterCache, prefetch } from "@snapfire/fsr-client";

await prefetch("/cart");
clearRouterCache();
```

## Navigating and Refreshing From Code

`navigate` takes an href and pushes history by default; pass `false` to replay a history entry without pushing a new one:

```ts
import { navigate, refresh } from "@snapfire/fsr-client";

await navigate("/servers/eu");
await navigate("/servers/eu", false);
```

A third argument chooses how the target is asked for: `{ full: true }` is the document's rendering of a route that would otherwise open in a layout's slot, `{ into: "modal" }` names the slot outright. Without either the request carries the document's path and the server intercepts when the target has a `page.<slot>.tsx` under a layout the origin shares.

```ts
await navigate("/product/7", true, { full: true });
```

`refresh` drops the router cache, re-fetches the current route and hands every kept island its new props in place, which is the revalidation an action performs for you: a layout's cart count follows the mutation and a page keeps what the user typed. The URL has not moved, so every key matches and the digests decide: a region whose output the mutation changed is patched in place, so an element that stands where it stood keeps its DOM and a scrolled list keeps its scroll. Every other region is untouched.

```ts
await refresh();
```

Both request the payload form of the URL by appending `__payload` to the query string, `navigate` through the router cache; both fall back to a full load when the response is not usable. A response with an error status is usable when it is a payload, which is what a route whose loader failed with `not_found` answers, so the navigation lands on the error page the server rendered.

Both apply the payload as it streams. The eager wave, every row up to the `G` sidecar, lands as one patch: the changed segments are swapped in, a deferred one showing the fallback its `loading.tsx` renders, history moves and the window scrolls. A step back or forward scrolls to where that entry was left, held page or not. Each `S` row then fills its slot as it arrives, with the head and store rows that follow it. The promise resolves once the payload has been applied whole. A click on a route whose loader is slow therefore shows the fallback then the fill (the way a document load does) and a click that joins a hover's fetch still in flight streams from wherever it is. A later navigation stops the rows of an earlier one from applying.

```ts
const done = navigate("/servers/eu"); // the fallback is in the document once the sidecar arrives
await done; // every slot filled
```

A navigation retitles the document: every `H` row of the applied payload goes through `applyHead`, which sets `document.title` and the description meta, so a streamed page that arrives after its slot retitles the document when its `S` row does. Call it yourself when you set the head from somewhere else:

```ts
import { applyHead } from "@snapfire/fsr-client";

applyHead({ title: "Cart · Shopping" });
```

## Wiring Another Library to the Navigator

For htmx there is an entry that does it, `bindHtmx`. The whole wiring is one line:

```ts
import htmx from "htmx.org";
import { bindHtmx } from "@snapfire/fsr-client/htmx";

bindHtmx(htmx);
```

htmx is passed in rather than imported, so the binding works with whatever version the import map names. The call returns the function that takes the listeners off again. While navigation is enabled the binding also cancels htmx 4's history restore, so a step back or forward is the navigator's and htmx does not fetch the page again and swap the body under it. What it does is worth knowing, since another library needs the same two directions written by hand.

A navigation writes markup this package did not read and another library may have to: htmx wires its `hx-` attributes when it processes a node; a node the navigator wrote has not been processed. The navigator dispatches `sf:navigate` on `document` once it has applied a payload's eager wave, with the path in `detail` and `sf:fill` for every deferred segment it fills afterwards, the same event the server's fill script dispatches for a streamed one. A library that wires markup listens to both:

```ts
const rewire = () => library.process(document.body);
document.addEventListener("sf:navigate", rewire);
document.addEventListener("sf:fill", rewire);
```

The other direction is `adopt` and `scan`. A fragment the host renders, one segment of a route with nothing around it, ends with the same inert seed script a document carries, so after a library swaps one in, `adopt()` reads the seeds nothing has read yet and `scan()` mounts any island the fragment placed:

```ts
import { adopt, scan } from "@snapfire/fsr-client";

document.body.addEventListener("library:settled", () => {
  adopt();
  scan(document);
});
```

`adopt` takes a root to look under and marks every script it reads, so calling it for the whole document after each swap reads each seed once.

## Following the Server

`refresh()` asks. `live()` waits to be told: it opens the host's event stream and every publish of a topic it asked for revalidates the route, so the page follows the server without polling it.

```tsx
import { useEffect } from "react";
import { live } from "@snapfire/fsr-client";

export default function Live({ topic }: { topic: string }) {
  useEffect(() => live([topic]), [topic]);
  return <span className="live">live</span>;
}
```

`live` returns the function that closes the stream, which is what an effect wants back. The server side is one call, `Host::publish("board")`. The browser reconnects on its own if the connection drops. Leaving the page closes the stream too, and coming back to it with the back button opens it again, though a topic published in between is not replayed.

What arrives is the topic and nothing else: no data rides on the stream. The route's loaders answer that, the ordinary way, when `refresh()` runs. A page that wants to do something other than revalidate passes `onTopic`:

```ts
live(["prices"], { onTopic: (topic) => console.log(`${topic} moved`) });
```

Nothing happens where `EventSource` is absent, so the same component renders on the server without a guard.

## Showing Whether the Page Is Online

`boot` keeps the `online` store key current and, under the host's `[pwa]`, registers its worker. An island reads the key like any other. `updateReady` turns true once a newer build's worker has taken over the page:

```tsx
import { online, updateReady } from "@snapfire/fsr-client";
import { useStore } from "@snapfire/fsr-client/react";
import { Link } from "@snapfire/fsr/components";

export default function Status() {
  const [connected] = useStore(online, true);
  const [ready] = useStore(updateReady, false);
  return (
    <div className="status">
      {connected ? null : <p>Offline: you are reading saved pages.</p>}
      {ready ? <Link href="/" full>A new version is ready</Link> : null}
    </div>
  );
}
```

A script that is not an island listens for `sf:update-ready` on the document instead. The page keeps running on the modules it loaded until the next full load.

## Sending as Fast as Someone Types

`live` hears. `socket` speaks. It opens one WebSocket on a topic, writes what comes back into the store and reconnects on its own.

```ts
import { socket } from "@snapfire/fsr-client";

const wire = socket(`wave/${id}`, { onOpen: () => setConnected(true), onClose: () => setConnected(false) });
wire.send("typing", { parent, body });
```

What the server makes of a sent row is its business: it answers with rows, they land in the store under their keys and every island reading one of those keys re-renders. Nothing on this seam is durable and a send while the connection is down is dropped rather than queued, because the next keystroke supersedes it anyway.

One connection is enough for a page. Hold it in a module rather than in a component, hand each island a share and close it when the last one goes, which is what `wave_react_ts` does in `src/ui/wire.ts`.

## Calling an Action

`action` turns a stable id into a callable. Input and result both cross as encoded values:

```ts
import { action, type SfValue } from "@snapfire/fsr-client";

const addServer = action("add_server");

const created: SfValue = await addServer({ name: "eu-3", load: 0.25 });
```

Calling with no argument sends an empty map:

```ts
const reload = action("reload_all");
await reload();
```

### Skipping Revalidation

A successful call revalidates the current route by default. Turn it off for a read-only action or when several calls should settle before the page refreshes:

```ts
const search = action("search_servers", { revalidate: false });
const hits = await search({ q: "eu" });

const addServer = action("add_server", { revalidate: false });
await addServer({ name: "eu-3", load: 0.25 });
await addServer({ name: "eu-4", load: 0.5 });
await refresh();
```

## Placing a Video Embed

`Embed` is a YouTube or Vimeo video as a still with a play button; a click puts the player in its place:

```tsx
import { Embed } from "@snapfire/fsr/components";

<Embed provider="youtube" id="f_WuRfuMXQw" title="Rick Astley - Never Gonna Give You Up" />
```

The build writes the placeholder on the server and `boot` listens for the click, so the page needs no island for it. Place it where the server renders: the build refuses an `Embed` inside a component that hydrates.

## Declaring a Place for Teleported Content

`Place` is an element other components write into from elsewhere on the page. Declare it in the shell, a layout, a page or a component an island renders:

```tsx
import { Island } from "@snapfire/fsr-authoring/template";
import { Place } from "@snapfire/fsr/components";

<header>
  <Place name="notices" className="notices" />
</header>
```

A Vue component anywhere on the page teleports into it by id:

```vue
<Teleport to="#notices">
  <p v-if="held" class="notice">On tonight's list</p>
</Teleport>
```

The server writes the teleported content inside `<div id="notices">` wherever the island that placed it sits. Vue hydrates it there. A place inside an island keeps that content through the island's hydration and renders, since each framework's `Place` leaves the element's children alone: React's is written with empty inner HTML React does not compare. An island that mounts afresh builds a new place element. The content another island placed in the old one is gone until that island mounts again. Content a streamed segment or a navigation brings later is moved into the place by `scan`. A `<Teleport defer>` is left to the browser, since Vue never updates a deferred Teleport it has hydrated.

## Placing a Picture

`Picture` writes in the browser what the build wrote on the server for an imported image, so a page that mounts the component hydrates over markup that matches:

```tsx
import { Picture } from "@snapfire/fsr/components";
import hero from "../img/hero.png";

export function Banner() {
  return <Picture src={hero} alt="The harbour at dusk" sizes="(min-width: 60em) 50vw, 100vw" priority />;
}
```

The import is an `ImageAsset`, the hashed URL with the width and height the bundle read from the file. The component writes a `<picture>` with a `<source>` per format and the original as its `<img>`, choosing the widths the way the build did: every policy width below the image's own, then its own. The policy comes from the document's `<meta name="sf:images">`, which the host writes from the build.

A string `src` is a remote image. With a named source it goes through that source's template:

```tsx
<Picture src={product.photo} source="cms" alt="" width={800} height={600} />
```

Without one it is an `<img src>` as written. `priority` loads the image eagerly at high fetch priority; the preload in the head is the server's, from the page's plan or its `meta`.

## Sharing State Across Islands

Every island is its own root, so React context reaches none of the others. A number two of them show, a filter one sets and another reads, a menu that must close when something else opens: those live in the store, a keyed map the whole document shares.

A key is a string. `key` names one and gives it a type:

```ts
// src/store.ts
import { key } from "@snapfire/fsr-client/store";

export const cartCount = key<number>("cart/count");
```

### Seeding the Store From a Loader

Export `store` beside `load`, a function of the data the loader returned. What it returns is written to the store before anything mounts and the server renders from the same values, so the first paint and the hydration agree:

```ts
// routes/layout.loader.ts
export async function load({ session }: Ctx) {
  const cartCount = Object.values(session.cart).reduce((n, q) => n + q, 0n);
  return { cartCount };
}

export const store = ({ data }: { data: { cartCount: bigint } }) => ({ "cart/count": Number(data.cartCount) });
```

Every segment on the route may export one. They merge outermost first, so a page wins a key its layout also sets. A `store` export names its keys as literal strings, never through an imported `key`.

A `store` seed is authoritative: an action's answer revalidates the route, every seeded key is seeded again and a value an island wrote in the browser is replaced. A key the visitor owns, a quantity they stepped or a size they picked, goes under `defaults` instead. It has the same shape as `store` and the server renders the first paint from it the same way, but in the browser it sits under an island's write, so the visitor's choice survives the route seeding again and the server's value takes the key only while nothing in the browser holds it:

```ts
export const store = ({ data }: { data: Order }) => ({ "order/name": data.name });
export const defaults = ({ data }: { data: Order }) => ({ "order/qty": data.qty, "order/size": data.sizes[0].label });
```

A `store` value wins a key a `defaults` of any segment also names.

### Reading a Key in a Component

`useStore` is the React binding. It reads like `useState` and returns the store's value or the initial one while nothing has set the key:

```tsx
import { useStore } from "@snapfire/fsr-client/react";
import { cartCount } from "@src/store";

export function CartBadge() {
  const [items] = useStore(cartCount, 0);
  return <span className="badge">{items}</span>;
}
```

The build lowers this call, so the key has to be something it can read: a string literal or a `key()` it can follow through an import. A key computed at runtime is refused with a diagnostic.

### Writing From Anywhere

The setter writes the store and every component reading that key re-renders, whichever root it sits in:

```tsx
const [items, setItems] = useStore(cartCount, 0);
<button onClick={() => setItems(items + 1)}>one more</button>
```

Outside a component and outside React entirely, the module functions do the same:

```ts
import { get, set, subscribe } from "@snapfire/fsr-client";

set(cartCount, 3);
const held = get(cartCount);
const stop = subscribe(cartCount, (value) => console.log("now", value));
```

### Optimistic Updates

`optimistic` shows a value at once, runs the call and puts the key back if it fails. A successful action revalidates and the seed that revalidation carries replaces the guess with what the server settled on:

```ts
import { optimistic } from "@snapfire/fsr-client";

await optimistic(cartCount, (get(cartCount) ?? 0) + quantity, () =>
  actions.cart.addToCart({ product_id: id, quantity: BigInt(quantity) }),
);
```

### Derived Keys and Transactions

A derived key is computed from others and recomputed whenever one of them changes:

```ts
import { derive, key } from "@snapfire/fsr-client";

const subtotal = key<number>("cart/subtotal");
const shipping = key<number>("cart/shipping");
const total = key<number>("cart/total");

derive(total, [subtotal, shipping], (read) => (read(subtotal) ?? 0) + (read(shipping) ?? 0));
```

`transaction` collapses notifications, so a listener hears once per key however many times the block wrote it:

```ts
transaction(() => {
  set(subtotal, 4200);
  set(shipping, 500);
});
```

Write the `derive` at module level in a file the browser loads, such as `src/main.ts`, with its key and sources as `key()` constants or strings and its compute as a function written in the call. The build lowers that compute and the server computes the key the same way after the loaders' seeds merge, so the markup React hydrates against already holds the derived value. A derive inside a function runs in the browser only, as does one whose compute does not lower. The build report names its line and the server holds every island that reads the key. Register the derivation before `boot`: computing the value the first paint already holds notifies nobody and from then on every change to a source updates it.

The store lives as long as the document. A soft navigation keeps it and writes whatever the new route seeded; a full load starts it again from that document's seed. Anything that must outlive a reload belongs in the session.

## Reading the Locale

The host resolves a locale for every request and writes it on the document, `<html lang="fr-FR" data-sf-locale="fr_FR">` and into every payload as an `L` row. `boot` adopts the attribute before the first scan; a navigation applies the row. A payload for a route a mounted site serves carries an `E` row naming the site's entry module; the navigator imports it once, then scans again, so the site's islands register and mount on first arrival without a document load. An island reads it with `useLocale`, which the build lowers, so the server renders the same value the browser hydrates against.

```tsx
import { localePath } from "@snapfire/fsr-client";
import { useLocale } from "@snapfire/fsr-client/react";

export function LanguagePicker() {
  const locale = useLocale();

  function go(tag: string, event: { preventDefault: () => void }) {
    event.preventDefault();
    window.location.assign(localePath(tag));
  }

  return (
    <nav aria-label="Language">
      <a href="/en_US/" data-sf-native className={locale === "en_US" ? "on" : ""} onClick={(e) => go("en_US", e)}>English</a>
      <a href="/fr_FR/" data-sf-native className={locale === "fr_FR" ? "on" : ""} onClick={(e) => go("fr_FR", e)}>Français</a>
    </nav>
  );
}
```

A link is served exactly as written: `/fr_FR/` is the French document and `/cart` is whatever the request resolves to, the cookie the host wrote when a prefix chose French, then the browser's `Accept-Language`, then the default. Nothing is ever prefixed behind your back.

Which is why a switcher has to say where it is going. `localePath("fr_FR")` is the page the document is showing with the current locale's prefix replaced, so choosing a language keeps the reader where they are instead of sending them to whatever page the switcher happens to live on. It reads `currentDocumentPath()` rather than `location.pathname` and the difference is real: an intercepted navigation puts the target's URL in the address bar while the page underneath stays, so a switcher in a drawer opened over `/agents` sees `/settings` in the address bar and `/agents` here.

The call sits in the handler rather than the `href` for two reasons. The `href` is then a literal a browser with no JavaScript can follow and the component stays one the build can lower: a library call in the render tree is residue and would drop the whole page to client rendering.

Outside React, `currentLocale` reads the locale and `subscribeLocale` follows it.

```ts
import { currentLocale, subscribeLocale } from "@snapfire/fsr-client";

const stop = subscribeLocale((tag) => document.title = `${document.title} (${tag})`);
console.log(currentLocale());
```

## Formatting With the Standard Library

`@snapfire/fsr-client/std` is the browser half of the standard library the server's interpreter carries: the same names, the same answers under the document's locale, which every member reads from `currentLocale()` so the value React hydrates against is the one Rust rendered.

```tsx
import { intl, text, time } from "@snapfire/fsr-client/std";

export function Receipt({ cents, placed, title }: { cents: number; placed: string; title: string }) {
  return (
    <p id={text.slug(title)}>
      {intl.currency(cents / 100, "USD")} on {intl.date(placed, "long")}, {intl.plural(3)} of {time.format(time.parse(placed) ?? 0, "YYYY-MM-DD")}
    </p>
  );
}
```

`intl.number` takes `{ minimumFractionDigits, maximumFractionDigits, locale }`; `intl.date` takes `short`, `medium`, `long` or `full` and formats in UTC or takes `{ weekday, year, month, day, locale }` as `Intl.DateTimeFormat` takes them with a text month or weekday; `intl.currency` spells the ISO code, `USD 1,234.50`, unless its options say `display: "symbol"` or `"narrowSymbol"`, which write the symbol CLDR gives the locale on both halves. A `locale` written in the code formats the same for every viewer; without one the document's locale formats. `time` holds instants as milliseconds since the epoch, `time.parse` reads one ISO 8601 subset and `time.now()` is `Date.now()`. `crypto.hash` is a synchronous SHA-256, `crypto.verify` compares against one and `crypto.random(n)` draws bytes as hex; `id.new()` is `crypto.randomUUID()`. Under `fsr test` the engine has no `Intl`, so `intl` asks the runner, which answers with the Rust half.

`t(key, args?)` reads the message under `key` from the table the document embedded for its locale, `catalog(currentLocale())`, which the server built from `locales/<tag>.toml` merged over the default locale's; `args.count` picks `key.one` or `key.other` through `intl.plural` and `{name}` placeholders are filled from `args`. A navigation into another locale receives that locale's table in the payload, so `t` follows `currentLocale()` without a reload.

```tsx
import { t } from "@snapfire/fsr-client/std";

export function Watching({ count }: { count: number }) {
  return <span>{t("agents.watching", { count })}</span>;
}
```

The build lowers every call to the plan, so the server computes the same member; `time.now`, `crypto.random` and `id.new` are refused on a component's render path and belong in a handler or a body.

## Declaring a Native Pair

`native` declares the browser half of a function whose Rust half the host registers under the same name, `module.member`; the declaration lives in an `ext/` module so the build reads it.

```ts
import { native } from "@snapfire/fsr-client/std";

export const queueLabel = native("fleet.queueLabel", (depth: number): string => (depth === 0 ? "idle" : `${depth} queued`));
export const token = native<() => string>("fleet.token");
```

With a function the pair has `render` reach and the function is what the browser runs; without one it has `body` reach and calling it in the browser throws, since only the server holds it. The name must be a string literal. Under `fsr test`, `native` also registers the function for the runner, which answers the server's calls to the pair with it; a `body` pair gets its stand-in the same way, from the spec:

```ts
import { native } from "@snapfire/fsr-client/std";
import { load, test } from "@snapfire/fsr-client/testing";

test("the agents page loads with a token", async () => {
  native("fleet.token", () => "spec-token");
  await load("/agents");
});
```

## Decoding Values From the Server

Island props arrive decoded. Call `decodeValue` yourself only for JSON you fetched by hand:

```ts
import { decodeValue } from "@snapfire/fsr-client";

const res = await fetch("/api/summary");
const value = decodeValue(await res.json());
```

### Wide Integers

An integer inside the JSON-safe range arrives as a plain `number`. Anything wider arrives tagged and decodes to a `bigint`:

```ts
decodeValue(42);                                  // 42
decodeValue({ $: "i", v: "-170141183460469231731687303715884105728" }); // bigint
decodeValue({ $: "u", v: "340282366920938463463374607431768211455" });  // bigint
```

The narrowing rule is the safe-integer range: a tagged integer inside it becomes a `number`, one outside it stays a `bigint`.

### Maps, Sets, Dates and Bigints

A `var` tag of `$map`, `$set`, `$date` or `$bigint` is a JavaScript value the server's model has no arm for. It decodes to the real type, so an island's `Map<string, number>` prop is a `Map` and `instanceof` holds:

```ts
decodeValue({ $: "var", t: "$map", p: [["alice", 100]] });  // Map { "alice" => 100 }
decodeValue({ $: "var", t: "$set", p: ["react", "next"] }); // Set { "react", "next" }
decodeValue({ $: "var", t: "$date", p: 1749988800000 });     // Date, 2025-06-15T12:00:00.000Z
decodeValue({ $: "var", t: "$bigint", p: 5 });               // 5n
```

The server writes these around an island's props whose type the build knows: a prop made with `new Map`, `new Set`, `new Date` or a bigint where the component is placed or typed `Map<K, V>`, `Set<T>`, `Date` or `bigint` where the component declares its props.

### Typed Arrays and Bytes

A `ta` tag becomes the matching typed array; a `b` tag becomes a `Uint8Array`:

```ts
const props = decodeValue(json) as { series: Float64Array; blob: Uint8Array };
const points = Array.from(props.series);
```

The kinds map one to one: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`, `f64` to `Int8Array`, `Uint8Array`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `BigInt64Array`, `BigUint64Array`, `Float32Array`, `Float64Array`. The 64-bit integer arrays hold `bigint` elements, as those constructors always do.

### Variants and References

A variant is a tag with an optional payload; a reference names an action or a module. Both are branded, so test them with the guards rather than by shape:

```ts
import { isRef, isVariant } from "@snapfire/fsr-client";

const v = decodeValue(json);
if (isVariant(v)) {
  if (v.tag === "Loaded") render(v.payload);
} else if (isRef(v) && v.kind === "action") {
  await action(v.id)();
}
```

Build them for a round trip with `variant`, `ref`, `actionRef` and `moduleRef`:

```ts
import { actionRef, moduleRef, variant } from "@snapfire/fsr-client";

variant("Loaded", { rows: 12 });
variant("Empty");
actionRef("add_server");
moduleRef("components/ServerChart.tsx#default");
```

## Encoding Values For the Server

`action` encodes its input for you. Call `encodeValue` directly when posting somewhere else:

```ts
import { encodeValue } from "@snapfire/fsr-client";

await fetch("/api/import", {
  method: "POST",
  headers: { "content-type": "application/json" },
  body: JSON.stringify(encodeValue({ id: 9007199254740993n, samples: new Float32Array([1.5, 2.5]) })),
});
```

JavaScript has one number type, so the mapping in this direction is not symmetric with the one coming back. A `number` goes out as bare JSON and reaches the server as an integer when it is integral, otherwise as an `f64`; use a `bigint` when the server must see a wide integer. `NaN` and the two infinities go out as the `f` tag's symbols. A `Uint8Array` the page made goes out as bytes; one that arrived as a `u8` typed array is marked on decode and goes back as the typed array it was, so a round trip keeps the server's kind. A plain object that has its own `$` key is escaped into the `m` tag's pair list, so a map with a `$` key survives instead of being read back as a tag. A `Map` goes out as a `$map` variant of its `[key, value]` pairs, a `Set` as a `$set` variant of its items and a `Date` as a `$date` variant of its milliseconds; the server reads each as the plain value it works with, an object keyed by each key's text, an array and a number. A `bigint` goes out as the `i` tag, since the server's integer is what it stands for.

## Reading a Payload Response by Hand

`parsePayload` reads a complete response body: the `V` version row, the `N` tree row, the `H`, `T`, `L`, `E` and `D` rows of the eager wave, the `G` segment row that closes it and one `S` row per resolved slot:

```ts
import { parsePayload } from "@snapfire/fsr-client";

const res = await fetch("/servers/eu?__payload");
const payload = parsePayload(await res.text());

console.log(payload.format, payload.encoding);
for (const { slot, node } of payload.resolutions) {
  console.log("slot", slot, "resolved to", node.kind);
}
```

`linesOf` yields a response's rows as its body streams and `parseRow` reads one, which is how the navigator consumes a payload. The `G` row is the last of the eager wave, so a reader that acts on it has the whole tree and every fallback in hand:

```ts
import { linesOf, parseRow } from "@snapfire/fsr-client";

for await (const line of linesOf(await fetch("/servers/eu?__payload"))) {
  const row = parseRow(line);
  if (row.tag === "G") console.log("eager wave complete, root segment", row.segments.k);
  if (row.tag === "S") console.log("slot", row.slot, "resolved to", row.node.kind);
}
```

`decodeNode` reads one node row on its own:

```ts
import { decodeNode } from "@snapfire/fsr-client";

const node = decodeNode(JSON.parse(line.slice(line.indexOf(" ", 2) + 1)));
```

## Rendering Nodes Back to HTML

`nodeToHtml` turns a decoded node into markup with island markers and props scripts in it, ready for `scan`. It takes an id allocator, which must be shared across every call that contributes to the same document so ids stay unique:

```ts
import { nodeToHtml, scan } from "@snapfire/fsr-client";

const ids = { next: 0 };
const host = document.querySelector("#panel")!;
host.innerHTML = nodeToHtml(payload.tree, ids);
scan(host);
```

Client-allocated ids use the `sf-c` prefix, so they can never collide with the server's `sf-i` sequence. `renderSegment` does the same for a subtree that has to stay navigable, wrapping it in the comment delimiters the navigator looks for:

```ts
import { renderSegment } from "@snapfire/fsr-client";

const html = renderSegment(payload.tree, payload.segments!, ids);
```

## Calling the Desktop Window

In a window opened by `fsr dev --desktop` or `fsr serve --desktop`, page code can ask the process for what a browser does not give it. This is experimental and has two calls so far.

```tsx
import { useState } from "react";
import { desktop } from "@snapfire/fsr-client";

export function Attach() {
  const [paths, setPaths] = useState<string[]>([]);
  async function pick() {
    const chosen = await desktop.openFile({ multiple: true, filters: [{ name: "Images", extensions: ["png", "jpg"] }] });
    if (chosen) setPaths(chosen);
  }
  return (
    <div>
      <button type="button" onClick={pick}>Attach</button>
      <button type="button" onClick={() => desktop.openExternal("https://example.com/help")}>Help</button>
      <ul>{paths.map((path) => <li key={path}>{path}</li>)}</ul>
    </div>
  );
}
```

`desktop.isDesktop()` says whether the page is in a desktop window, so the same page can hide what a browser cannot do. Outside one, `openFile` and `openExternal` reject. The calls run in the browser, so a component using them is an island.

## Error Handling

A failed action rejects with `ActionFailure`, carrying the server's failure kind and message:

```ts
import { action, ActionFailure, navigate } from "@snapfire/fsr-client";

const addServer = action("add_server");

try {
  await addServer({ name: "eu-3", load: 0.25 });
} catch (err) {
  if (!(err instanceof ActionFailure)) throw err;
  switch (err.kind) {
    case "unauthorized":
      await navigate("/auth/login");
      break;
    case "invalid":
    case "conflict":
      showFieldError(err.message);
      break;
    case "timeout":
    case "unavailable":
      showRetry(err.message);
      break;
    default:
      showBanner(err.message);
  }
}
```

The kinds are the server's `FailureKind` spellings, one per HTTP error status: `unauthorized`, `forbidden`, `not_found`, `too_many_requests`, `invalid` for 400 and the rest. A response whose body is not the JSON failure shape, a proxy's page or a refusal in plain text, is still an `ActionFailure`: the kind is read back from the status, `403` as `forbidden`, `429` as `too_many_requests`, a 4xx no kind names as `invalid` and a status that is not an error as `internal`, with the body's text as the message. Rethrow anything that is not an `ActionFailure`, as the block above does, since a request that never completes rejects with the fetch's own error.

The rest of the package reports differently. Mount problems never reject anything the caller can see: an unregistered module id and a loader or mounter that throws are both logged with `console.warn`; the island stays as the server rendered it. Navigation degrades instead of throwing: a non-ok payload response, a missing sidecar or a segment region that cannot be found in the DOM all fall back to a full page load. The two decoders do throw, on an unknown value tag, an unknown typed array kind, an unknown node row kind, an unknown row tag or a body with no `N` row:

```ts
try {
  const payload = parsePayload(await res.text());
  apply(payload);
} catch (err) {
  console.warn("sf: unreadable payload", err);
  window.location.reload();
}
```
