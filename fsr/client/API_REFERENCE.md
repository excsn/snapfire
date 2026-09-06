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
  * [Row](#row)
  * [parseRow](#parserow)
  * [linesOf](#linesof)
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
  * [loadEntry](#loadentry)
  * [boot](#boot)
  * [patchIsland](#patchisland)
  * [DOM Contract](#dom-contract)
  * [isServerIsland](#isserverisland)
  * [morph](#morph)
  * [mountServer](#mountserver)
* [6. Navigation](#6-navigation)
  * [enableNavigation](#enablenavigation)
  * [prefetch](#prefetch)
  * [clearRouterCache](#clearroutercache)
  * [navigate](#navigate)
  * [refresh](#refresh)
  * [applyHead](#applyhead)
* [7. Actions](#7-actions)
  * [action](#action)
* [8. The Store](#8-the-store)
  * [StoreKey](#storekey)
  * [key](#key)
  * [get](#get)
  * [set](#set)
  * [clear](#clear)
  * [subscribe](#subscribe)
  * [transaction](#transaction)
  * [derive](#derive)
  * [optimistic](#optimistic)
  * [seed](#seed)
  * [adopt](#adopt)
  * [reset](#reset)
  * [snapshot](#snapshot)
* [9. The Locale](#9-the-locale)
  * [currentLocale](#currentlocale)
  * [subscribeLocale](#subscribelocale)
  * [setLocale](#setlocale)
  * [catalog, setCatalog and adoptCatalog](#catalog-setcatalog-and-adoptcatalog)
  * [adoptLocale](#adoptlocale)
  * [currentDocumentPath](#currentdocumentpath)
  * [localePath](#localepath)
* [10. The React Mounter](#10-the-react-mounter)
  * [reactMounter](#reactmounter)
  * [useHoisted](#usehoisted)
  * [withHoisted](#withhoisted)
  * [Island](#island)
  * [island(component, options)](#islandcomponent-options)
  * [Slot](#slot)
  * [useStore](#usestore)
  * [useLocale](#uselocale)
  * [Link](#link)
  * [reactPatcher](#reactpatcher)
* [11. The Standard Library](#11-the-standard-library)
  * [localeTag](#localetag)
  * [intl](#intl)
  * [text](#text)
  * [time](#time)
  * [crypto](#crypto)
  * [id](#id)
  * [t](#t)
  * [native](#native)
* [12. Error Handling](#12-error-handling)
  * [ActionFailure](#actionfailure)
  * [Thrown Errors](#thrown-errors)
  * [Silent Degradations](#silent-degradations)

## 1. Entry Points

Two ES module entry points, resolved through an import map. There is no package manifest and no default export.

| Specifier | Built file | Exports | Bare imports |
| --- | --- | --- | --- |
| `@snapfire/fsr-client` | `dist/index.js` | everything in sections 2 to 9, plus `ActionFailure` | none |
| `@snapfire/fsr-client/react` | `dist/react.js` | `reactMounter`, `useStore`, `useLocale` and the placement elements | `react`, `react-dom/client` |
| `@snapfire/fsr-client/store` | `dist/store.js` | section 8, which the core entry re-exports | none |

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
* `n?: string`, the slot this segment fills in its parent; absent at the root.
* `p?: number[]`, the path to the subtree relative to the parent segment's node. `[]` means the whole node, `[i]` means child `i` of a `seq`.
* `s?: number`, the slot id for a deferred segment. A segment carries `p` or `s`, never both.
* `c: Segment[]`, child segments.
* `keep?: string[]`, slots of this segment the payload left unfilled and the browser keeps as they stand.

### Payload

A parsed response.

* `format: number`, the `fmt` field of the `V` row.
* `encoding: string`, the `enc` field of the `V` row.
* `tree: SfNode`, the `N` row.
* `segments: Segment | null`, the `G` row when the response carried one.
* `heads: Head[]`, the `H` rows in arrival order: the eager wave's, then one per resolution that described the document.
* `seeds: { [key: string]: SfValue }[]`, the `T` rows in arrival order, each already decoded.
* `locale: string | null`, the `L` row, the locale the response was rendered in as the application spells it; `null` when the server sent none.
* `entry: string | null`, the `E` row, a module to load before the response's islands can mount, a mounted site's entry; `null` when the document's own entry covers them.
* `catalog: { [key: string]: string } | null`, the `D` row; `null` when the server sent none.
* `resolutions: { slot: number; node: SfNode }[]`, the `S` rows in arrival order.

### Head

* `title?: string`, `description?: string`. A field left out keeps what the document has.

### decodeNode

* `decodeNode(row: unknown): SfNode`

Reads one node row: `["t", text]`, `["r", html]`, `["q", children]`, `["c", { m, p, ch, s }]` or `["p", slot, fallback]`. `p` is run through `decodeValue` and must decode to a map; `ch` defaults to empty; `s` is `null` when absent. Throws on an unknown row kind.

### parsePayload

* `parsePayload(text: string): Payload`

Reads a whole response body, one row per line, skipping empty lines, through `parseRow`. Throws when no `N` row was present.

### Row

* `type Row = { tag: "V"; format: number; encoding: string } | { tag: "N"; tree: SfNode } | { tag: "G"; segments: Segment } | { tag: "H"; head: Head } | { tag: "T"; seed: { [key: string]: SfValue } } | { tag: "L"; locale: string } | { tag: "E"; entry: string } | { tag: "D"; catalog: { [key: string]: string } } | { tag: "S"; slot: number; node: SfNode }`

One row of a payload, discriminated by its tag.

### parseRow

* `parseRow(line: string): Row`

Reads one row: its tag, a space, then its body, decoded as the tag says. Throws when the tag is not `V`, `N`, `G`, `H`, `T`, `L`, `E`, `D` or `S`.

### linesOf

* `linesOf(res: Response): AsyncGenerator<string>`

The rows of a response body as they arrive: the byte stream read through `res.body.getReader()`, decoded as UTF-8 and cut at newlines, empty lines skipped, an unterminated last line yielded when the stream ends. A response without a body stream yields the rows of `res.text()` at once, which is what the `fsr test` runner's `fetch` returns.

### Row Grammar

Each row is a tag character, a space, then its body, terminated by a newline. The eager wave is `V`, `N`, then whichever of `H`, `T`, `L`, `E` and `D` the render produced, then `G`, which closes it; every row after `G` is a resolution or what one carried.

| Row | Body | Meaning |
| --- | --- | --- |
| `V` | `{"fmt":1,"enc":"json"}` | Format version and encoding |
| `N` | one node row | The initial tree |
| `G` | one segment object | The segment sidecar |
| `H` | `{"title":…,"description":…}` | What the route says about the document |
| `T` | an encoded value map | The store keys the route seeded |
| `L` | a JSON string | The locale the response was rendered in |
| `E` | a JSON string | A module to import before the response's islands can mount, a mounted site's entry |
| `D` | a JSON object of strings | The locale's message catalog, dotted keys to messages, sent unless the request's `x-sf-catalog` named that locale |
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

Mounts every unmounted island marker under `root`. Selects `sf-i:not([data-sf-mounted])`, skips a marker with no `data-sf-module`, stamps `data-sf-mounted` before scheduling, then schedules according to the `data-sf-when` of the `sf-s` region the marker sits in, when a page or layout placed it with one, else the entry's timing. Idempotent, so rescanning a root that is already mounted does nothing.

Props are read from `script[data-sf-props="<marker id>"]`, searched inside `root` first and then across the document. A missing or empty script yields `{}`.

A marker whose module id no registry knows is left as rendered and remembered, not reported. Every id still unregistered is warned about once the document settles: on `DOMContentLoaded`, after which every deferred module script has run, or on a microtask when the state is already `complete`, and never while an entry named by `loadEntry` is still loading. A mounted site's islands are missing on every scan that precedes its entry, so a first miss is the healthy path rather than a defect.

### loadEntry

* `loadEntry(src: string): void`

Imports `src` once, however many times it is asked for, then rescans the document so the islands it registered mount. A failure warns `sf: loading <src> failed` and forgets `src`, so a later payload naming it tries again. Call it before the scan that will miss those islands: while it is in flight no miss is reported.

The navigator calls it with a payload's `E` row, which is how a mounted site's entry reaches the browser on the first navigation into that site.

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
| `<sf-s data-sf-name="…">` | a layout's markup, around a named slot: a parallel segment or the region an intercept opens in, empty when nothing fills it | `Slot` and `reactMounter`, which adopt it; `navigate`, which fills and empties it |
| `<sf-s data-sf-island data-sf-when="…">` | a page's or layout's markup, around a component placed as an island | `Island`, which adopts it; `scan`, which reads the timing |
| `<sf-s data-sf-island data-sf-mode="server">` | the same region for an island in server mode | `scan`, which mounts the `sf-i` inside through `mountServer` and never through the registry |
| `data-sf-on="click:0 change:1"` | the renderer, on an element of a server-mode island that binds handlers | `mountServer`, which delegates those event types on the island |
| `data-sf-key` | the renderer, from an element's `key`, in server mode only | `morph`, which moves a keyed element rather than recreating it |
| `data-sf-pending` | `mountServer`, on the island while a round trip is out | the application's styles |
| `$s` in the props script | the renderer, the initial values of a server-mode island's state | `mountServer` |
| `$h` in the props script | the renderer, the island's hoisted values | `reactMounter`, which lifts it into `useHoisted`'s context |
| `<script type="application/json" data-sf-segments>` | streamed HTML response | `enableNavigation` |

### mountServer

* `function mountServer(el: Element, module: string, props: Props): void`

Mounts `el` as an island in server mode: keeps `props` less `$s`, which is the state, and listens on `el` for every event type its markup binds. An event on a bound element posts `{ props, state, handler, event }` to `/_sf/island/<module>`, with `event` carrying the target's `value`, `checked` and `name` and the key of a keyboard event, then stores the answered `state` and patches the answered `html` in with `morph`. `submit` is prevented. While a round trip is out the island carries `data-sf-pending` and a further event is dropped. A failed round trip is a `console.warn` and the island is left as it was.

### isServerIsland

* `function isServerIsland(el: Element): boolean`

True when `mountServer` mounted `el`. `patchIsland` on such an element gives it the new props and renders it again on the server from them and the state it holds.

### morph

* `function morph(el: Element, html: string): void`

Patches `el`'s children to match `html`: a text or comment node by content, an element by tag and position or by `data-sf-key`, attributes by name, with the nodes it can keep kept. A form control that has focus keeps its value; one that does not takes the server's. An `sf-i` inside is matched but never entered.

## 6. Navigation

Segment patching in place of a page load. The functions share one module-level sidecar, one id allocator, the document's current path and one router cache: payload text by the origin, the slot asked for and `pathname + search`, or the fetch still bringing it, held for `cacheMs` on the clock `performance.now` reads.

A request for a payload says where it comes from: `x-sf-from` carries the document's path and search, which lets the server render the target into a slot of a live layout, an intercept; `x-sf-into` names that slot outright; a full navigation sends neither. `interface NavigateOptions { full?: boolean; into?: string }` chooses, and an anchor chooses with `data-sf-full` and `data-sf-into`.

### enableNavigation

* `enableNavigation(options?: NavigationOptions): void`
* `interface NavigationOptions { prefetch?: PrefetchTiming; cacheMs?: number }`; `type PrefetchTiming = "hover" | "viewport" | "none"`. `prefetch` is the document's timing for a link that names none of its own, `"hover"` by default; `cacheMs` defaults to 30000.

Registers `refresh` as `window.__sf.refresh`, which the host's development script calls, reads `script[data-sf-segments]` into the module's current sidecar, sets `cacheMs` when given, then installs a `click` listener on `document` and a `popstate` listener on `window`. `mouseover`, `focusin` and passive `touchstart` listeners on `document` call `prefetch` for a link whose timing is `"hover"`. A link whose timing is `"viewport"` is observed by one `IntersectionObserver` instead, prefetched as it enters the view and unobserved there, so it is fetched once; the links are observed at `enableNavigation`, after every applied payload and on `sf:fill`, since a navigation brings new ones. A link's own `data-sf-prefetch` decides its timing and the option decides the rest. Where `IntersectionObserver` does not exist, viewport timing observes nothing rather than throwing. The href of the enclosing `a[href]`, unless it carries `data-sf-native` or `data-sf-prefetch="none"`.

A click is ignored when `defaultPrevented` is set, when `button` is not 0, when any of `metaKey`, `ctrlKey`, `shiftKey` or `altKey` is held, when the target has no enclosing `a[href]` or when the href resolves to another origin. Otherwise the default is prevented and `navigate` is called with the path plus search and the anchor's `data-sf-full` and `data-sf-into` as its options.

### prefetch

* `prefetch(href: string, options?: NavigateOptions): Promise<void>`

Resolves `href` against the location; another origin resolves at once. When the router cache holds a fresh feed for the origin, the options and `<pathname><search>`, still arriving or finished less than `cacheMs` ago, that feed is used; otherwise the payload form is fetched with the headers the options call for and held as a feed from its first row, its time being when the last row arrived. Resolves once the feed is whole. A non-ok response holds nothing.

### clearRouterCache

* `clearRouterCache(): void`

Drops every held payload and forgets every fetch in flight, whose result is then discarded when it lands.

### navigate

* `navigate(href: string, push?: boolean, options?: NavigateOptions): Promise<void>`

Takes the payload for the origin, the options and `<pathname><search>` from the router cache while its feed is still arriving or finished less than `cacheMs` ago, or fetches `<pathname><search>` with `__payload` appended to the query string, joined with `&` when a search string is present and `?` when it is not, with `x-sf-from` set to the document's current path unless `full` or `into` is given and `x-sf-into` set to `into`. A fetched payload is held as a feed of rows from its first. A non-ok response hands over to `window.location.assign(href)`. Otherwise the rows are read as they arrive through `linesOf` and `parseRow`: at the `G` row the eager wave is applied, history is pushed when `push` is true (its default), the current path is moved to the target, then the window scrolls to the top unless the payload was an intercept, which opens in place; each `S` row after it fills its slot and rescans, each `H` row retitles and each `T` row seeds, and the promise resolves once the last row has been applied. A feed that ends before `G`, or an eager wave that cannot be patched, hands over to `window.location.assign(href)`. A `navigate` or `refresh` begun later takes the document, and the rows still arriving for this one stop applying.

Applying walks the old and new segment spines together. The first key mismatch replaces that region from the new payload. Children pair by slot name when every child on both sides carries one, else in order, where a differing child count replaces the parent region. A kept region whose node is an island takes the new props through `patchIsland` when they differ from its props script, which is rewritten. A child the old side had and the new side lacks is emptied, delimiters included, and its region takes back what it held before navigation first filled it, its fallback or nothing, unless the new segment's `keep` names its slot, in which case it is carried over untouched. A child the new side has and the old side lacks is written into the parent's `<sf-s data-sf-name>` region, found under the parent's own island. A new child that is slot-addressed replaces the old child's region (its slot element while it is still streaming) with the pending node and its fallback. Resolved slots are filled after the diff, each delimited by its segment key, then the document is rescanned. A missing sidecar, a missing `G` row, a region whose comment pair cannot be found in the DOM or a named slot the parent's markup lacks falls back to `window.location.reload()`.

### refresh

* `refresh(): Promise<void>`

Drops the router cache, re-fetches the current `pathname` and `search` with `__payload` appended, with `x-sf-into` naming the slot the current URL was intercepted into when it was, and applies it as `navigate` does, row by row as it streams, with one difference: a kept leaf region that is not an island is replaced anyway. Every kept island, layout or page, takes its new props in place and keeps its DOM and its state; an open intercept re-renders in its slot over the page it keeps.

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

## 8. The Store

One keyed map per document, outside every island root, so two islands can show the same value. Its own entry point, `@snapfire/fsr-client/store`, re-exported from the core entry. Module state: there is one store per document, not one per import.

A route seeds it from its loaders. The server renders components against the same seed, so a seeded key hydrates without a flash. The seed reaches the browser as `script[data-sf-store]` in a document, as a `T` row in a payload and as a `__sfStore(…)` call in a streamed resolution.

### StoreKey

* `type StoreKey<T> = string & { readonly __store?: T }`

A key is the string it names. The type parameter is a phantom, carried for the reader and the compiler.

### key

* `key<T>(id: string): StoreKey<T>`

Names a key. Declaring one in a module the build can follow is what lets a component in another file use it: the lowerer reads `key()` through an import and takes the string.

### get

* `get<T>(k: StoreKey<T>): T | undefined`

What the key holds, or `undefined` when nothing has set it.

### set

* `set<T>(k: StoreKey<T>, value: T): void`

Writes the key and notifies its listeners. A write of the value already held notifies nobody.

### clear

* `clear<T>(k: StoreKey<T>): void`

Forgets the key and notifies, so readers fall back to their initial value.

### subscribe

* `subscribe(k: StoreKey<unknown> | string, listener: (value: unknown, key: string) => void): () => void`

Registers a listener and returns the function that removes it.

### transaction

* `transaction(work: () => void): void`

Runs `work` with notifications collapsed: each key dirtied fires once afterwards, however many times it was written. A nested call defers to the outermost. Synchronous.

### derive

* `derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void`

Registers a key computed from others and computes it once now. It recomputes whenever a source changes.

### optimistic

* `optimistic<T, R>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R>`

Sets the key to `guess`, awaits `remote` and returns its result. A rejection restores what the key held, or clears it when it held nothing, and rethrows. A success leaves the guess in place: the revalidation an action runs carries the seed that replaces it.

### seed

* `seed(values: { [key: string]: SfValue }): void`

Writes a whole map in one transaction. The navigator calls it for every `T` row of a payload before it patches the DOM, so a kept island renders once with the new value.

### adopt

* `adopt(): void`

Reads the document's `script[data-sf-store]`, then any seed a streamed resolution left on `window.__sfSeed` before this module loaded, and installs `window.__sfSeedApply` so later resolutions seed as they arrive. Called when the module loads and again by `boot`, since a document written after the module ran carries a seed nobody has read. Idempotent.

### reset

* `reset(): void`

Forgets every key and notifies nobody, which is what a new document calls for: the listeners of the old one went with its roots, and the derived keys stay registered for the next seed. The spec runner's `load` calls it before each document.

### snapshot

* `snapshot(): { [key: string]: unknown }`

Every key the store holds, for a test or a debugger.

## 9. The Locale

The document's locale as the application spells it, `fr_FR` or `fr`. The server writes it on the document as `<html lang="fr-FR" data-sf-locale="fr_FR">` and into every payload as an `L` row; `boot` adopts the attribute and a navigation applies the row.

### currentLocale

* `currentLocale(): string`

The locale the document is in; an empty string before any document said.

### subscribeLocale

* `subscribeLocale(listener: (tag: string) => void): () => void`

Calls `listener` whenever the locale changes. The returned function stops it.

### catalog, setCatalog and adoptCatalog

* `type Catalog = { readonly [key: string]: string }`: a locale's message table, dotted keys to messages, merged over the default locale's on the server.
* `catalog(tag: string): Catalog | null`: the table held for `tag`, `null` when none arrived.
* `setCatalog(tag: string, table: Catalog): void`: holds `table` for `tag`. A navigation applies a payload's `D` row this way and sends `x-sf-catalog: <tag>` for the locale it holds so the server omits the row.
* `adoptCatalog(): void`: reads `<script type="application/json" data-sf-i18n="<tag>">` from the document, which `boot` does after `adoptLocale`; a script that does not parse is left out.

### setLocale

* `setLocale(tag: string): void`

Makes `tag` the document's locale: `<html lang>` in BCP 47 spelling, `data-sf-locale` as written, every listener told. The same tag again does nothing. The navigator calls it with each payload's `L` row.

### adoptLocale

* `adoptLocale(): void`

Reads `data-sf-locale` off the document element and sets it. Nothing written leaves the current locale. `boot` calls it before the first scan, so an island reading the locale hydrates against what the server rendered.

### localePath

* `localePath(to: string, from?: string): string`

The page the document is showing, under locale `to`: its path with the current locale's prefix replaced. Nothing else is rewritten, and `from` is used as it stands when given, so this is the caller asking for one segment to change rather than a link being rewritten behind them.

`from` defaults to `currentDocumentPath()` rather than `location.pathname`, and the difference matters: an intercepted navigation puts the target's URL in the address bar while the page underneath stays. A language switcher inside a drawer opened over `/agents` reads `/settings` from the address bar and `/agents` from here, and `/agents` is the page the reader is on.

A path already under the current prefix has it swapped rather than stacked, so `/fr_FR/help` to `en_US` is `/en_US/help`. A query is carried. The result is always prefixed, including for the default locale, which is what remembers the choice.

### currentDocumentPath

* `currentDocumentPath(): string`

The page the document is showing, which is not always what the address bar says: an intercepted navigation changes the URL and leaves the document rooted where it was. Empty before `enableNavigation` runs.

## 10. The React Mounter

Its own entry point, so the core package never imports React.

### reactMounter

* `const reactMounter: Mounter`

Creates the element with `createElement(component, props, children)`, then calls `hydrateRoot(el, element)` when `hydrate` is true and `createRoot(el).render(element)` when it is false. Returns the hydration root or the root. A `$h` entry in `props` is the island's hoisted table: it is lifted out before the component sees its props and provided through `withHoisted`.

### useHoisted

* `function useHoisted(module: string): HoistReader`
* `interface HoistReader { r<T>(id: number, compute: () => T): T; l<A extends unknown[], R>(f: (...args: A) => R): (...args: A) => R; c(id: number, hit: (html: { __html: string }) => ReactElement, miss: () => ReactElement): ReactElement }`
* `type Hoisted = { readonly [key: string]: unknown }`

The reader the build binds at the top of every component it rewrote, keyed under `module`. `r` returns the table's value for `<module>|<id>`, or `<module>|<id>@<i>.<j>` inside loops, and calls `compute` when the table has no such key or there is no table. `l` wraps a JSX `.map` callback: while it runs, its index argument is on the loop path, and the element it returns is placed under a provider carrying that path with the element's own `key`, so a component it renders keys its hoists below the iteration that placed it. The path starts from the enclosing provider's, so it continues through nested components. `c` is a static subtree: when the table holds a string under the key, `hit` renders the element with that markup as its inner HTML, which React neither renders nor hydrates inside; otherwise `miss` renders the original JSX.

### withHoisted

* `function withHoisted(table: Hoisted | null, element: ReactElement): ReactElement`

`element` under `table`, the way the mounter places an island under the table its props carried. `null` makes every read compute. The testing module's `render` uses it with the table the server render produced.

The element is wrapped in a regions provider: the root itself and every `sf-s[data-sf-island]` under `el` that is not inside a nested island, in document order, which each `Island` rendered under this root takes in turn. `children` is set when `el` holds an `<sf-s>` without `data-sf-island` or `data-sf-name` that is not inside a nested island, which is what a layout's markup looks like: one `<sf-s>` element with `dangerouslySetInnerHTML` set to the markup it already holds and `suppressHydrationWarning`, created once per `el` and passed unchanged on every render, so React adopts the child segment at hydration and never reconciles it. Every `sf-s[data-sf-name]` under `el` and not inside a nested island is passed the same way as a prop of that name, so a layout reads a parallel slot as `{feed}`. The page inside hydrates in its own root.

### Island

* `function Island({ when, mode, children }: IslandProps): ReactElement`
* `interface IslandProps { when?: MountTiming; mode?: "server"; children?: ReactNode }`; `mode` rides as `data-sf-mode`, and `island(component, { when, mode })` takes the same.

Places its one child component as an island of its own. The build lowers the use, so on the server the child renders as a nested client node inside `<sf-s data-sf-island>`, with `data-sf-when` when `when` is given, and its own props script; the child is never rendered by this element. In the browser it renders that `<sf-s>` with `dangerouslySetInnerHTML` set to the markup the next region under the root already holds and `suppressHydrationWarning`, taken once per instance from the mounter's regions, so the outer root adopts the region and never reconciles it while `scan` mounts the child in its own root. Mounted fresh with no server markup it renders an empty region.

### island(component, options)

* `function island<P extends object>(component: ComponentType<P>, options?: { when?: MountTiming }): (props: P) => ReactElement`

`component` as a component that places it with `Island` and `options.when` wherever it is used: `const LazyChart = island(Chart, { when: "visible" })`, then `<LazyChart series={data} />`. The build recognises the module-level `const` the same way.

### Slot

* `function Slot({ name, children }: SlotProps): ReactElement`
* `interface SlotProps { name: string; children?: ReactNode }`

A named slot of a layout: the region a parallel segment under `slots/<name>/` renders into, or the one an intercept `page.<name>.tsx` opens in. On the server the build lowers the use to `<sf-s data-sf-name>` around the segment, or around `children`, the fallback, while nothing fills it; the children are never rendered by this element. In the browser it renders that `<sf-s>` with `dangerouslySetInnerHTML` set to the markup the region of that name under the root already holds and `suppressHydrationWarning`, taken once per instance, so the root adopts the region and never reconciles it while `navigate` fills and empties it. A layout that destructures a prop named after a `slots/` directory gets the same region as that prop, from `reactMounter`, and needs no `Slot`.

### useStore

* `function useStore<T>(k: StoreKey<T>, initial: T): [T, (next: T) => void]`

A store key as component state, over `useSyncExternalStore`. Reads the store's value, or `initial` while nothing has set the key; `initial` is captured on the first render, so a fresh object literal there is safe. The setter writes the store, which re-renders every component reading that key in any root.

The build lowers the call, so the key must be a string literal or a `key()` it can follow through an import; anything else is residue naming the line. On the server the read becomes the seed's value with `initial` as the fallback, which is why a seeded key hydrates without a flash. The setter is dropped by lowering, like any handler.

### useLocale

* `useLocale(): string`

The document's locale, re-rendering the island when a navigation changes it. The build lowers the call, so the server renders the same value the browser adopts.

### Link

* `function Link({ full, into, prefetch, native, ...rest }: LinkProps): ReactElement`
* `interface LinkProps extends AnchorHTMLAttributes<HTMLAnchorElement> { full?: boolean; into?: string; prefetch?: PrefetchTiming; native?: boolean }`

An `<a>` with the rest of its props, carrying `data-sf-full="true"` when `full`, `data-sf-into` when `into`, `data-sf-prefetch` when `prefetch` and `data-sf-native="true"` when `native`, which is what the navigator reads off a clicked or hovered anchor. The build lowers the use to the same `<a>`.

### reactPatcher

* `const reactPatcher: Patcher`

Calls `render` on the root the mounter returned with `createElement(component, props, children)`, the same `children` element as at mount, so a layout re-renders with new props and its page's DOM is untouched.

Requires `react` and `react-dom/client` in the page's import map. A component compiled from `.tsx` under `"jsx": "react-jsx"` additionally needs `react/jsx-runtime` there, since `snapfirec` lowers JSX through the automatic runtime.

## 11. The Standard Library

`@snapfire/fsr-client/std`: the browser half of the standard library the server's interpreter carries under the same names. Every `render` member agrees with the server byte for byte under the same locale, which is `currentLocale()`; a member marked server only has no such promise and the build refuses it on a component's render path. Under `fsr test` the engine has no `Intl`, so the `intl` members ask the runner through `__sf.ext` and the Rust half answers.

### localeTag

* `localeTag(): string`: `currentLocale()` as BCP 47, `fr-FR` for `fr_FR`; `en` before any document says.

### intl

* `intl.number(n: number | bigint, options?: NumberOptions): string`; `interface NumberOptions { minimumFractionDigits?: number; maximumFractionDigits?: number }`. `Intl.NumberFormat(localeTag(), options)`.
* `intl.currency(n: number | bigint, code: string): string`: `style: "currency"` with `currencyDisplay: "code"`, `USD 1,234.50`.
* `intl.date(when: number | string, style?: DateStyle): string`; `type DateStyle = "short" | "medium" | "long" | "full"`, `medium` by default; `dateStyle` with `timeZone: "UTC"`. A string goes through `time.parse` and throws when it is not the ISO subset.
* `intl.plural(n: number | bigint): string`: `Intl.PluralRules(localeTag()).select`.

### text

* `text.slug(s: string): string`: NFD, marks dropped, lowercased, every run outside `a-z0-9` one hyphen, none at either end.
* `text.truncate(s: string, max: number, ellipsis?: string): string`: the first `max` code points and `ellipsis`, `…` by default, when `s` is longer.

### time

Instants are milliseconds since the epoch and every calendar field is UTC.

* `time.format(when: number, pattern: string): string`: `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced, every other character kept.
* `time.add(when: number, amount: number, unit: string): number` and `time.diff(later: number, earlier: number, unit: string): number` with `unit` one of `ms`, `s`, `m`, `h`, `d`; another unit throws.
* `time.parse(s: string): number | null`: `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`; `null` for anything else.
* `time.now(): number`: `Date.now()`. Server only on a render path.

### crypto

* `crypto.hash(s: string): string`: SHA-256 of the UTF-8 bytes as lowercase hex, computed synchronously.
* `crypto.verify(s: string, hash: string): boolean`: constant time over the hash's length, case insensitive.
* `crypto.random(bytes: number): string`: at most 1024 bytes from `getRandomValues`, as hex. Server only on a render path.

### id

* `id.new(): string`: `crypto.randomUUID()`. Server only on a render path, where the server's is a UUID version 7.

### t

* `t(key: string, args?: { [name: string]: unknown }): string`: the message under `key` in `catalog(currentLocale())`; with `args.count` a number or bigint, `key.<intl.plural(count)>` then `key.other` then `key`; `{name}` replaced by `String(args[name])` for a scalar argument and left as written otherwise; the key itself when the table lacks every form or no table is held. Lowered by the build to `i18n.t`, the server's, and hoisted when its inputs are props only.

### native

* `native<F extends (...args: never[]) => unknown>(name: string, f?: F): F`: declares the browser half of a native pair under `name`, `module.member`, whose Rust half the host registers under the same name. With `f`, the pair has `render` reach and `f` is returned and registered on `globalThis.__sf_natives` for the runner; without, it has `body` reach and the returned function throws `<name> runs on the server only`. `name` must be a string literal, since the build reads the declaration.

## 12. Error Handling

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
| No island registered for a marker's module id | Marker left as rendered and remembered; `console.warn("sf: no island registered for <id>")` only once the document settles and it is still unregistered |
| An entry module fails to import | `console.warn("sf: loading <src> failed", err)`, and `src` is forgotten so a later payload retries |
| A loader or mounter rejects | `console.warn("sf: mounting <id> failed", err)`, marker left as rendered |
| A marker with no `data-sf-module` | Skipped, not marked mounted |
| Missing or empty props script | Mounted with `{}` |
| Non-ok response from `navigate` | `window.location.assign(href)` |
| Missing sidecar, missing `G` row, mismatched child counts or a region not found | `window.location.reload()` |
| `refresh` finds no slot element for a resolution | That resolution is dropped, the rest proceed |
