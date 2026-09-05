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
* [Placing a Component as an Island](#placing-a-component-as-an-island)
* [Filling a Layout's Slots](#filling-a-layouts-slots)
* [Writing a Mounter for Another Framework](#writing-a-mounter-for-another-framework)
* [Rescanning After Streamed Content Arrives](#rescanning-after-streamed-content-arrives)
* [Enabling Navigation](#enabling-navigation)
* [Prefetching and the Router Cache](#prefetching-and-the-router-cache)
* [Navigating and Refreshing From Code](#navigating-and-refreshing-from-code)
* [Calling an Action](#calling-an-action)
  * [Skipping Revalidation](#skipping-revalidation)
* [Decoding Values From the Server](#decoding-values-from-the-server)
  * [Wide Integers](#wide-integers)
  * [Typed Arrays and Bytes](#typed-arrays-and-bytes)
  * [Variants and References](#variants-and-references)
* [Encoding Values For the Server](#encoding-values-for-the-server)
* [Reading a Payload Response by Hand](#reading-a-payload-response-by-hand)
* [Rendering Nodes Back to HTML](#rendering-nodes-back-to-html)
* [Error Handling](#error-handling)

## Core Concepts

* **Value model**: the set of things that can cross the boundary. It is wider than JSON, so the encoding tags whatever JSON cannot spell natively.
* **Tagged JSON**: an object carrying a `$` key names a value the plain JSON grammar cannot carry, such as a wide integer, a typed array or a variant. Untagged JSON passes through untouched.
* **Node**: one entry in the payload tree. Its five kinds are `text`, `raw`, `seq`, `client` and `pending`.
* **Island**: a `client` node. The server renders it inside an `<sf-i>` marker with its props in a sibling JSON script tag; the browser mounts a component over that markup.
* **Module id**: the string that names a component, source path plus export, for example `components/ServerChart.tsx#default`. It is the key `registerIsland` is called with and the value of the marker's `data-sf-module`.
* **Mounter**: the function that turns a loaded module plus props into a mounted component in an element. React has one in the `/react` entry; every other framework plugs in the same way.
* **Hydration timing**: per island, `"load"`, `"visible"` or `"idle"`. It decides when the loader runs, not whether the island exists.
* **Slot**: a hole a deferred segment fills later. It renders as `<div data-sf-slot="N">` holding a fallback until its content arrives.
* **Segment**: a region of the page with a comparable key, delimited in the HTML by `<!--sf-g:key-->` and `<!--/sf-g-->` comments. Same key across two responses means the region survives navigation.
* **Segment sidecar**: the `G` row or the `script[data-sf-segments]` tag in an HTML response, carrying the segment tree the navigator diffs against.
* **Payload response**: the line-oriented wire format, requested by adding `__payload` to a route's query string. One `V` row, one `N` row, an optional `G` row, an optional `H` row with the document's title and description and one `S` row per resolved slot, each with an `H` row of its own when the slot's segment described the document.
* **Action**: a server function with a stable id. The client holds the id, never a URL shape.
* **Revalidation**: re-fetching the current route after a mutation and replacing the top-level segment regions, so the layout's DOM and its island state survive.

## Quick Start

### The Application Entry

One module registers every island, boots them and takes over navigation:

```ts
import { boot, enableNavigation, registerIsland } from "@snapfire/fsr-client";
import { reactMounter } from "@snapfire/fsr-client/react";

registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
});

registerIsland("components/LatencyChart.tsx#default", {
  loader: () => import("./LatencyChart.js").then((m) => m.default),
  mount: reactMounter,
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

`--import-map` fails the build when a bare import has no entry, which is what keeps `react` and `react-dom/client` declared. Serve `dist/` at the prefix given to `--public-path`, then point the page's map at the two entry points:

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
snapfirec --source-map --minify compact --public-path /static/js/app --import-map importmap.json
```

## Registering an Island

An entry is a loader, a mounter, an optional patcher and an optional timing. The loader resolves to whatever the mounter expects, which for `reactMounter` is the component itself; the patcher re-renders the island with new props when a navigation or a revalidation keeps its DOM:

```ts
import { registerIsland } from "@snapfire/fsr-client";
import { reactMounter } from "@snapfire/fsr-client/react";

registerIsland("components/ServerChart.tsx#default", {
  loader: () => import("./ServerChart.js").then((m) => m.default),
  mount: reactMounter,
  patch: reactPatcher,
});
```

The key must be the exact `data-sf-module` string the server wrote. A layout is registered like any island; `reactMounter` recognises the `<sf-s>` in its markup and hands the component a child element it never reconciles, so the page inside hydrates in its own root and a navigation swaps it under the live layout. A marker whose module id is not registered is left server-rendered and logged:

```
sf: no island registered for components/ServerChart.tsx#default
```

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

## Placing a Component as an Island

A page or a layout is an island; a component inside one is part of that island's root until it is placed as its own. `Island` from the React adapter does that, with the timing on the use:

```tsx
import { Island } from "@snapfire/fsr-client/react";
import { OrderHelp } from "@src/ui/OrderHelp";

<Island when="visible">
  <OrderHelp orderId={order.id} />
</Island>
```

The build lowers the use: the server renders `OrderHelp` with its props as a nested island inside an `<sf-s data-sf-island data-sf-when="visible">` region of the page's markup, with its own props script, and registers the module in `generated/islands.ts`. In the browser the page's root adopts the region as it stands and never reconciles it, while `scan` mounts `OrderHelp` in a root of its own when it scrolls into view, so its state is its own and the page's re-renders leave it alone. `island(OrderHelp, { when: "visible" })` at module level gives a component that places itself the same way wherever it is used. `when` on the region wins over the registry's timing for that use; a use without one takes the registry's, else `"load"`. A component from another framework works the same way once its module has a mounter registered, since the registry picks the mounter by module.

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

Both are `<sf-s data-sf-name>` regions in the server's markup, which the layout's root adopts and never reconciles, the way it adopts `children`. Navigation fills and empties them: a soft navigation to a route with a `page.modal.tsx` writes the variant into the `modal` region of the nearest live layout that declares it and leaves the page alone, and the navigation away empties it again. A document load renders the page, never the variant. The promo keeps its DOM across every page under the layout, since its key never changes.

The navigator sends the document's path with every soft request, which is how the server knows an intercept applies. A link says otherwise with `Link`:

```tsx
import { Link } from "@snapfire/fsr-client/react";

<Link href={`/product/${id}`} full>Full details</Link>
<Link href={`/product/${id}`} into="modal">Quick look</Link>
```

`full` asks for the document's rendering of the target whatever the origin; `into` names the slot outright, for a link the server would not match. On any anchor the same is `data-sf-full` and `data-sf-into`. `refresh` re-renders an open intercept in its slot over the page it keeps.

## Writing a Mounter for Another Framework

A `Mounter` receives the loaded module, the decoded props, the marker element and whether server-rendered markup is already inside it. Its return value is kept by the caller, so return whatever the framework needs for teardown:

```ts
import { registerIsland, type Mounter, type Props } from "@snapfire/fsr-client";
import { createApp, type Component } from "vue";

const vueMounter: Mounter = (module, props, el, hydrate) => {
  const app = createApp(module as Component, props as Record<string, unknown>);
  if (!hydrate) el.replaceChildren();
  app.mount(el);
  return app;
};

registerIsland("components/Counter.vue#default", {
  loader: () => import("./Counter.js").then((m) => m.default),
  mount: vueMounter,
});
```

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

`scan` is what does the work. It is idempotent: it only matches `sf-i` markers without `data-sf-mounted` and it stamps that attribute before scheduling. Call it directly for markup you inserted yourself:

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

It also hangs `refresh` on `window.__sf`, which is how the stock host's development script refreshes an open page in place after a change. A click is left alone when it is already default-prevented, is not the primary button, carries a modifier key, has no enclosing `a[href]` or points at another origin. Everything else fetches the route's payload and patches only the segments whose keys changed, so the layout's DOM, its scroll position and any island state above the changed region survive. A kept island whose props changed is re-rendered in place through its patcher rather than replaced. When the sidecar is missing or a segment's region cannot be found in the DOM, the navigator falls back to a full load rather than guessing.

## Prefetching and the Router Cache

A link's payload is fetched when the pointer moves over it, when it takes focus or when it is touched, then held for thirty seconds. The click that follows applies the held payload with no round trip. So does a back or forward that lands on a route fetched inside the window. `enableNavigation` takes both settings:

```ts
enableNavigation({ prefetch: "none" });
enableNavigation({ cacheMs: 5_000 });
```

A link that should not be warmed says so, which is right for a link whose loader is expensive or whose route logs the visit:

```html
<a href="/reports/yearly" data-sf-prefetch="none">Yearly report</a>
```

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

A third argument chooses how the target is asked for: `{ full: true }` is the document's rendering of a route that would otherwise open in a layout's slot, `{ into: "modal" }` names the slot outright. Without either the request carries the document's path, and the server intercepts when the target has a `page.<slot>.tsx` under a layout the origin shares.

```ts
await navigate("/product/7", true, { full: true });
```

`refresh` drops the router cache, re-fetches the current route and hands every kept island its new props in place, which is the revalidation an action performs for you: a layout's cart count follows the mutation and a page keeps what the user typed.

```ts
await refresh();
```

Both request the payload form of the URL by appending `__payload` to the query string, `navigate` through the router cache; both fall back to a full load when the response is not usable.

A navigation retitles the document: every `H` row of the applied payload goes through `applyHead`, which sets `document.title` and the description meta, so a streamed page that arrives after its slot retitles the document when its `S` row does. Call it yourself when you set the head from somewhere else:

```ts
import { applyHead } from "@snapfire/fsr-client";

applyHead({ title: "Cart · Shopping" });
```

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

JavaScript has one number type, so the mapping in this direction is not symmetric with the one coming back. A `number` goes out as bare JSON and reaches the server as an integer when it is integral, otherwise as an `f64`; use a `bigint` when the server must see a wide integer. `NaN` and the two infinities go out as the `f` tag's symbols. A `Uint8Array` the page made goes out as bytes; one that arrived as a `u8` typed array is marked on decode and goes back as the typed array it was, so a round trip keeps the server's kind. A plain object that has its own `$` key is escaped into the `m` tag's pair list, so a map with a `$` key survives instead of being read back as a tag.

## Reading a Payload Response by Hand

`parsePayload` reads a complete response body: the `V` version row, the `N` tree row, an optional `G` segment row, `H` head rows and one `S` row per resolved slot:

```ts
import { parsePayload } from "@snapfire/fsr-client";

const res = await fetch("/servers/eu?__payload");
const payload = parsePayload(await res.text());

console.log(payload.format, payload.encoding);
for (const { slot, node } of payload.resolutions) {
  console.log("slot", slot, "resolved to", node.kind);
}
```

`decodeNode` reads one row on its own, which is what a reader consuming the stream incrementally needs:

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

The kinds are the server's `FailureKind` spellings: `unauthorized`, `not_found`, `invalid`, `conflict`, `timeout`, `unavailable` and `internal`. A response whose body is not the JSON failure shape, a proxy's page or a refusal in plain text, is still an `ActionFailure`: the kind is read back from the status, `400` as `invalid`, `401` and `403` as `unauthorized`, `404` as `not_found`, `409` as `conflict`, `503` as `unavailable`, `504` as `timeout` and anything else as `internal`, with the body's text as the message. Rethrow anything that is not an `ActionFailure`, as the block above does, since a request that never completes rejects with the fetch's own error.

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
