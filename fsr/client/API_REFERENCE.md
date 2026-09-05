# API Reference: @snapfire/fsr-client

The browser half of SnapFire FSR: payload decoding, island hydration, streamed slot filling, segment navigation and the action client.

## Contents

* [1. Entry Points](#1-entry-points)
* [2. Values](#2-values)
  * [SfValue](#sfvalue)
  * [RefValue](#refvalue)
  * [VariantValue](#variantvalue)
  * [Constructors and Guards](#constructors-and-guards)
  * [decodeValue](#decodevalue)
  * [encodeValue](#encodevalue)
  * [Tag Encoding](#tag-encoding)
* [3. Payload Reading](#3-payload-reading)
  * [SfNode](#sfnode)
  * [Segment](#segment)
  * [Payload](#payload)
  * [Head](#head)
  * [decodeNode](#decodenode)
  * [parsePayload](#parsepayload)
  * [Row Grammar](#row-grammar)
* [4. Rendering](#4-rendering)
  * [nodeToHtml](#nodetohtml)
  * [renderSegment](#rendersegment)
* [5. Islands](#5-islands)
  * [Props](#props)
  * [Mounter](#mounter)
  * [MountTiming](#mounttiming)
  * [IslandEntry](#islandentry)
  * [registerIsland](#registerisland)
  * [scan](#scan)
  * [boot](#boot)
  * [patchIsland](#patchisland)
  * [DOM Contract](#dom-contract)
* [6. Navigation](#6-navigation)
  * [enableNavigation](#enablenavigation)
  * [prefetch](#prefetch)
  * [clearRouterCache](#clearroutercache)
  * [navigate](#navigate)
  * [refresh](#refresh)
  * [applyHead](#applyhead)
* [7. Actions](#7-actions)
  * [action](#action)
* [8. The React Mounter](#8-the-react-mounter)
  * [reactMounter](#reactmounter)
* [9. Error Handling](#9-error-handling)
  * [ActionFailure](#actionfailure)
  * [Thrown Errors](#thrown-errors)
  * [Silent Degradations](#silent-degradations)

## 1. Entry Points

Two ES module entry points, resolved through an import map. There is no package manifest and no default export.

| Specifier | Built file | Exports | Bare imports |
| --- | --- | --- | --- |
| `@snapfire/fsr-client` | `dist/index.js` | everything in sections 2 to 7, plus `ActionFailure` | none |
| `@snapfire/fsr-client/react` | `dist/react.js` | `reactMounter` | `react`, `react-dom/client` |

The core entry imports nothing outside the package, so a page that mounts no React islands never loads React.

`dist/` is produced by `snapfirec` from `tsconfig.json` (`target: es2022`, `rootDir: src`, `outDir: dist`, `sourceMap`, `declaration`). `importmap.json` in the package root is the map `--import-map` checks the bare imports against.

Not re-exported from either entry, though the modules define them: `IdAlloc`, `escapeKey` and `subtreeAt` in `render.ts`. A caller of `nodeToHtml` or `renderSegment` passes an object literal for the allocator.

## 2. Values

The value model as JavaScript sees it and the pair of functions that move it across the boundary.

### SfValue

The union a decoded value inhabits.

* `null | boolean | number | bigint | string`
* `Uint8Array | Int8Array | Int16Array | Uint16Array | Int32Array | Uint32Array | BigInt64Array | BigUint64Array | Float32Array | Float64Array`
* `SfValue[]`
* `RefValue | VariantValue`
* `{ [key: string]: SfValue }`

### RefValue

A reference to a server action or a client module.

* `readonly kind: "action" | "module"`
* `readonly id: string`

Frozen and branded with `Symbol.for("sf.ref")`. Test with `isRef`, not by shape.

### VariantValue

A tagged union arm, with an optional payload.

* `readonly tag: string`
* `readonly payload?: SfValue`

Frozen and branded with `Symbol.for("sf.variant")`. The `payload` key is absent, not `undefined`, for a payload-free variant.

### Constructors and Guards

* `ref(kind: "action" | "module", id: string): RefValue`
* `actionRef(id: string): RefValue`
* `moduleRef(id: string): RefValue`
* `variant(tag: string, payload?: SfValue): VariantValue`
* `isRef(v: unknown): v is RefValue`
* `isVariant(v: unknown): v is VariantValue`

### decodeValue

* `decodeValue(json: unknown): SfValue`

Turns the server's JSON into JavaScript values. Untagged JSON passes through untouched, recursing into arrays and objects. An object whose `$` key holds a string is read as a tag; an object whose `$` holds anything else is a plain object.

A tagged integer (`i`, `u`) becomes a `number` when it lies within `Number.MIN_SAFE_INTEGER` to `Number.MAX_SAFE_INTEGER` inclusive, otherwise a `bigint`. Base64 payloads (`b`, `ta`) decode through `atob`. Throws on an unknown tag or an unknown typed array kind.

### encodeValue

* `encodeValue(v: SfValue): unknown`

Produces the tagged JSON the server decodes. The mapping is not symmetric with `decodeValue`, because JavaScript has one number type:

* A finite `number` is emitted bare; the server reads an integral one as an integer and a fractional one as an `f64`.
* `NaN`, `Infinity` and `-Infinity` are emitted as `{ $: "f", v: "nan" | "inf" | "-inf" }`.
* A `bigint` in the `i128` range is emitted as `i`, one above it and within `u128` as `u`. Outside both it throws `bigint outside the value model's integer range`.
* `Uint8Array` is emitted as bytes (`b`), unless it came out of `decodeValue` as a `u8` typed array, which is marked with a symbol and goes back as `ta` with kind `u8`. `Uint8ClampedArray` is not part of the model and falls through to object encoding.
* Other typed arrays are emitted as `ta` with their kind, honouring `byteOffset` and `byteLength`, so a view over a larger buffer encodes only its own window.
* A plain object owning a `$` key is escaped into the `m` tag's pair list.

### Tag Encoding

Tags are objects with a `$` discriminant. Both halves of the pair agree on these:

| Tag | Fields | JavaScript |
| --- | --- | --- |
| `i` | `v`, decimal string | `number` inside the safe range, else `bigint` |
| `u` | `v`, decimal string | `number` inside the safe range, else `bigint` |
| `f` | `v`, number or `"nan"`, `"inf"`, `"-inf"` | `number` |
| `f32` | `v`, number or symbol | `number` |
| `b` | `v`, base64 | `Uint8Array` |
| `ta` | `k`, kind; `v`, base64 little-endian | matching typed array |
| `m` | `v`, array of `[key, value]` pairs | plain object |
| `var` | `t`, tag; `p`, optional payload | `VariantValue` |
| `ref` | `k`, `"action"` or `"module"`; `id` | `RefValue` |

Typed array kinds: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`, `f64`, mapping to `Int8Array`, `Uint8Array`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `BigInt64Array`, `BigUint64Array`, `Float32Array`, `Float64Array`. Element counts must divide the decoded byte length evenly; the typed array constructor throws otherwise.

## 3. Payload Reading

The wire format the server emits when a route is requested with `__payload` in its query string.

### SfNode

One node of the payload tree, discriminated by `kind`.

* `{ kind: "text"; text: string }`
* `{ kind: "raw"; html: string }`
* `{ kind: "seq"; children: SfNode[] }`
* `{ kind: "client"; module: string; props: { [key: string]: SfValue }; children: SfNode[]; ssr: SfNode | null }`
* `{ kind: "pending"; slot: number; fallback: SfNode }`

### Segment

One node of the segment sidecar tree.

* `k: string`, the segment key. Equal keys across two responses mean the region is kept.
* `p?: number[]`, the path to the subtree relative to the parent segment's node. `[]` means the whole node, `[i]` means child `i` of a `seq`.
* `s?: number`, the slot id for a deferred segment. A segment carries `p` or `s`, never both.
* `c: Segment[]`, child segments.

### Payload

A parsed response.

* `format: number`, the `fmt` field of the `V` row.
* `encoding: string`, the `enc` field of the `V` row.
* `tree: SfNode`, the `N` row.
* `segments: Segment | null`, the `G` row when the response carried one.
* `heads: Head[]`, the `H` rows in arrival order: the eager wave's, then one per resolution that described the document.
* `resolutions: { slot: number; node: SfNode }[]`, the `S` rows in arrival order.

### Head

* `title?: string`, `description?: string`. A field left out keeps what the document has.

### decodeNode

* `decodeNode(row: unknown): SfNode`

Reads one node row: `["t", text]`, `["r", html]`, `["q", children]`, `["c", { m, p, ch, s }]` or `["p", slot, fallback]`. `p` is run through `decodeValue` and must decode to a map; `ch` defaults to empty; `s` is `null` when absent. Throws on an unknown row kind.

### parsePayload

* `parsePayload(text: string): Payload`

Reads a whole response body, one row per line, skipping empty lines. Throws when a line's tag is not `V`, `N`, `G`, `H` or `S`. Throws when no `N` row was present.

### Row Grammar

Each row is a tag character, a space, then its body, terminated by a newline.

| Row | Body | Meaning |
| --- | --- | --- |
| `V` | `{"fmt":1,"enc":"json"}` | Format version and encoding |
| `N` | one node row | The initial tree |
| `G` | one segment object | The segment sidecar |
| `S` | slot id, a space, then a node row | One resolved slot |

`S` rows arrive in completion order, not slot order; a resolution may introduce further slots that arrive later in the same stream.

## 4. Rendering

Turning decoded nodes back into the markup the boot runtime and the navigator expect.

### nodeToHtml

* `nodeToHtml(node: SfNode, ids: { next: number }): string`

Serialises a node tree. Text has `&`, `<` and `>` escaped; `raw` is emitted verbatim; a `client` node becomes an `<sf-i>` marker plus a sibling `<script type="application/json" data-sf-props="…">`, rendering `ssr` when present and its `children` otherwise; a `pending` node becomes `<div data-sf-slot="N">` around its fallback.

The allocator is mutated in place, one increment per island. Ids are `sf-c0`, `sf-c1` and upward, a prefix that cannot collide with the server's `sf-i` sequence, so one allocator must be shared by every call contributing to the same document. Props JSON has every `<` escaped to the JSON escape `\u003c`, so the payload can never terminate the script tag.

### renderSegment

* `renderSegment(node: SfNode, seg: Segment, ids: { next: number }): string`

Serialises a segment's subtree wrapped in `<!--sf-g:key-->` and `<!--/sf-g-->`, recursing into child segments at their sidecar positions and calling `nodeToHtml` for everything else. Slot-addressed children are skipped, since their DOM region is the `data-sf-slot` element. `%` becomes `%25` and `-` becomes `%2D` in the key, so a key can never contain `--` and close the comment. Throws when a segment path walks through a node that is not a `seq`.

## 5. Islands

Registration, timing and the scan that mounts markers.

### Props

* `type Props = { [key: string]: SfValue }`

### Mounter

* `type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean) => unknown`

`module` is whatever the island's `loader` resolved to. `hydrate` is true when the marker element already has child nodes, which is the case for any island the server rendered. The return value is ignored by the caller, so it is free for the framework's handle.

### MountTiming

* `type MountTiming = "load" | "visible" | "idle"`

`"load"` mounts as soon as the marker is scanned. `"visible"` observes the element with an `IntersectionObserver` and mounts on the first intersection, disconnecting immediately. `"idle"` mounts in `requestIdleCallback` where the browser provides it and in a 1ms `setTimeout` where it does not.

### IslandEntry

What a module id is registered with.

* `loader: () => Promise<unknown>`, resolving to the value passed to `mount`.
* `mount: Mounter`
* `when?: MountTiming`, defaulting to `"load"`. Per island, not per page.

### registerIsland

* `registerIsland(moduleId: string, entry: IslandEntry): void`

Registers or replaces the entry for a module id in the process-wide registry. `moduleId` must equal the marker's `data-sf-module` exactly. Registration only affects markers scanned afterwards.

### scan

* `scan(root: ParentNode): void`

Mounts every unmounted island marker under `root`. Selects `sf-i:not([data-sf-mounted])`, skips a marker with no `data-sf-module`, stamps `data-sf-mounted` before scheduling, then schedules according to the entry's timing. Idempotent, so rescanning a root that is already mounted does nothing.

Props are read from `script[data-sf-props="<marker id>"]`, searched inside `root` first and then across the document. A missing or empty script yields `{}`.

### boot

* `boot(): void`

Scans the whole document, immediately when the DOM is past `loading` and on `DOMContentLoaded` otherwise, then scans again on every `sf:fill` event on `document`. That event is dispatched by the server's inline fill script after it moves a resolved template into its slot, which is what mounts islands inside streamed chunks. Calling `boot` again scans again and adds no second listener; the listener is registered once per document.

### patchIsland

* `patchIsland(el: Element, props: Props): Promise<boolean>`

Re-renders the island mounted at `el` with `props`, in place, through the entry's `patch`; the DOM and the island's state survive. Resolves false when nothing is mounted there, the mount failed or the entry has no patcher.

* `type Patcher = (handle: unknown, module: unknown, props: Props, el: Element) => void`; `IslandEntry.patch?: Patcher`. `handle` is what the mounter returned.

### DOM Contract

What the server writes and this package reads.

| Marker | Written by | Read by |
| --- | --- | --- |
| `<sf-i id data-sf-module>` | HTML serialiser, `nodeToHtml` | `scan` |
| `<script type="application/json" data-sf-props="<id>">` | HTML serialiser, `nodeToHtml` | `scan` |
| `data-sf-mounted` | `scan` | `scan` |
| `<div data-sf-slot="N">` | HTML serialiser, `nodeToHtml` | the fill script, `refresh`, `navigate` |
| `<template data-sf-fill="N">` | the streamed HTML response | the fill script |
| `sf:fill` `CustomEvent` on `document`, `detail` is the slot id | the fill script | `boot` |
| `<!--sf-g:key-->` and `<!--/sf-g-->` | segment writer, `renderSegment`, the fill of a streamed segment | `navigate`, `refresh` |
| `<sf-s>` | a layout's markup, around its child segment | `reactMounter`, which adopts it without reconciling it |
| `<script type="application/json" data-sf-segments>` | streamed HTML response | `enableNavigation` |

## 6. Navigation

Segment patching in place of a page load. The functions share one module-level sidecar, one id allocator and one router cache: payload text by `pathname + search` or the fetch still bringing it, held for `cacheMs` on the clock `performance.now` reads.

### enableNavigation

* `enableNavigation(options?: NavigationOptions): void`
* `interface NavigationOptions { prefetch?: PrefetchTiming; cacheMs?: number }`; `type PrefetchTiming = "hover" | "none"`. `prefetch` defaults to `"hover"`, `cacheMs` to 30000.

Reads `script[data-sf-segments]` into the module's current sidecar, sets `cacheMs` when given, then installs a `click` listener on `document` and a `popstate` listener on `window`. With `prefetch` at `"hover"`, `mouseover`, `focusin` and passive `touchstart` listeners on `document` call `prefetch` with the href of the enclosing `a[href]`, unless it carries `data-sf-native` or `data-sf-prefetch="none"`.

A click is ignored when `defaultPrevented` is set, when `button` is not 0, when any of `metaKey`, `ctrlKey`, `shiftKey` or `altKey` is held, when the target has no enclosing `a[href]` or when the href resolves to another origin. Otherwise the default is prevented and `navigate` is called with the path plus search.

### prefetch

* `prefetch(href: string): Promise<void>`

Resolves `href` against the location; another origin resolves at once. When the router cache holds a fresh payload for `<pathname><search>` or a fetch for it is in flight, resolves at once; otherwise fetches the payload form and holds the text with the time it arrived. A non-ok response holds nothing.

### clearRouterCache

* `clearRouterCache(): void`

Drops every held payload and forgets every fetch in flight, whose result is then discarded when it lands.

### navigate

* `navigate(href: string, push?: boolean): Promise<void>`

Takes `<pathname><search>` from the router cache while its entry is younger than `cacheMs`, joins a fetch already in flight for it or fetches `<pathname><search>` with `__payload` appended to the query string, joined with `&` when a search string is present and `?` when it is not. A fetched text is held. A non-ok response hands over to `window.location.assign(href)`. Otherwise the payload is parsed and applied, history is pushed when `push` is true (its default), then the window scrolls to the top.

Applying walks the old and new segment spines together, children paired in order whether positioned or slot-addressed. The first key mismatch replaces that region from the new payload; a differing child count replaces the parent region. A kept region whose node is an island takes the new props through `patchIsland` when they differ from its props script, which is rewritten. A new child that is slot-addressed replaces the old child's region (its slot element while it is still streaming) with the pending node and its fallback. Resolved slots are filled after the diff, each delimited by its segment key, then the document is rescanned. A missing sidecar, a missing `G` row or a region whose comment pair cannot be found in the DOM falls back to `window.location.reload()`.

### refresh

* `refresh(): Promise<void>`

Drops the router cache, re-fetches the current `pathname` and `search` with `__payload` appended and applies it as `navigate` does, with one difference: a kept leaf region that is not an island is replaced anyway. Every kept island, layout or page, takes its new props in place and keeps its DOM and its state.

Falls back to `window.location.reload()` when there is no sidecar, when the response is not ok or when the payload cannot be applied.

### applyHead

* `applyHead(head: Head): void`

Sets `document.title` when `head.title` is given and the `meta[name="description"]` content when `head.description` is, creating that element under `document.head` when there is none. `navigate` and `refresh` call it for every `H` row of the payload they apply, in order.

## 7. Actions

### action

* `action(id: string, opts?: { revalidate?: boolean }): (input?: SfValue) => Promise<SfValue>`

Builds a callable for a stable action id. The client holds ids, never URLs.

The call POSTs to `/_sf/action/${encodeURIComponent(id)}` with `content-type: application/json` and `JSON.stringify(encodeValue(input))` as the body. `input` defaults to `{}`. On a non-ok status it throws `ActionFailure`: from the body's `kind` and `message` when the body is the JSON failure shape, otherwise with the kind the status stands for (`400` `invalid`, `401` and `403` `unauthorized`, `404` `not_found`, `409` `conflict`, `503` `unavailable`, `504` `timeout`, anything else `internal`); the message is then the body's text, the `statusText` or `HTTP <status>`. On success it returns `decodeValue` of the JSON body.

`revalidate` defaults to true, which awaits `refresh()` after a successful call and before the result is returned. Pass `{ revalidate: false }` for a read-only action or to batch several mutations behind one manual `refresh`.

## 8. The React Mounter

Its own entry point, so the core package never imports React.

### reactMounter

* `const reactMounter: Mounter`

Creates the element with `createElement(component, props, children)`, then calls `hydrateRoot(el, element)` when `hydrate` is true and `createRoot(el).render(element)` when it is false. Returns the hydration root or the root.

`children` is set when `el` holds an `<sf-s>` that is not inside a nested island, which is what a layout's markup looks like: one `<sf-s>` element with `dangerouslySetInnerHTML` set to the markup it already holds and `suppressHydrationWarning`, created once per `el` and passed unchanged on every render, so React adopts the child segment at hydration and never reconciles it. The page inside hydrates in its own root.

### reactPatcher

* `const reactPatcher: Patcher`

Calls `render` on the root the mounter returned with `createElement(component, props, children)`, the same `children` element as at mount, so a layout re-renders with new props and its page's DOM is untouched.

Requires `react` and `react-dom/client` in the page's import map. A component compiled from `.tsx` under `"jsx": "react-jsx"` additionally needs `react/jsx-runtime` there, since `snapfirec` lowers JSX through the automatic runtime.

## 9. Error Handling

### ActionFailure

Thrown by an action callable when the server answers with a non-ok status.

* `extends Error`
* `constructor(kind: string, message: string)`
* `readonly kind: string`
* `name` is `"ActionFailure"`
* `message` is the server's message, the text of a body that is not the failure shape or the response's `statusText`

The kinds the runtime emits: `unauthorized` (401), `not_found` (404), `invalid` (400), `conflict` (409), `timeout` (504), `unavailable` (503), `internal` (500).

### Thrown Errors

| Thrown by | When |
| --- | --- |
| `decodeValue` | `unknown value tag: <tag>`, `unknown typed array kind: <kind>` |
| `decodeNode` | `unknown node row kind: <kind>` |
| `parsePayload` | `unknown payload row tag: <tag>`, `payload has no N row` |
| `encodeValue` | a `bigint` outside the `i128` and `u128` ranges |
| `renderSegment` | `segment path walks through a non-seq node` |
| an action callable | `ActionFailure` on any non-ok status; the fetch's own error when the request never completes |

### Silent Degradations

Not every failure surfaces as a rejection.

| Situation | Behaviour |
| --- | --- |
| No island registered for a marker's module id | `console.warn("sf: no island registered for <id>")`, marker left as rendered |
| A loader or mounter rejects | `console.warn("sf: mounting <id> failed", err)`, marker left as rendered |
| A marker with no `data-sf-module` | Skipped, not marked mounted |
| Missing or empty props script | Mounted with `{}` |
| Non-ok response from `navigate` | `window.location.assign(href)` |
| Missing sidecar, missing `G` row, mismatched child counts or a region not found | `window.location.reload()` |
| `refresh` finds no slot element for a resolution | That resolution is dropped, the rest proceed |
