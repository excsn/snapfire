# API Reference: @snapfire/fsr/client

The browser half of SnapFire FSR: value decoding, payload reading, island hydration, streamed slot filling, navigation, actions and the store.

## Contents

* [1. Entry Points](#1-entry-points)
* [2. Values](#2-values)
  * [SfValue](#sfvalue)
  * [RefValue](#refvalue)
  * [VariantValue](#variantvalue)
  * [DoubleValue](#doublevalue)
  * [Constructors and Guards](#constructors-and-guards)
  * [decodeValue](#decodevalue)
  * [encodeValue](#encodevalue)
  * [Tag Encoding](#tag-encoding)
* [3. Payload Reading](#3-payload-reading)
  * [SfNode](#sfnode)
  * [Segment](#segment)
  * [Head](#head)
  * [Payload](#payload)
  * [Row](#row)
  * [decodeNode](#decodenode)
  * [parseRow](#parserow)
  * [parsePayload](#parsepayload)
  * [linesOf](#linesof)
  * [Row Grammar](#row-grammar)
* [4. Rendering](#4-rendering)
  * [IdAlloc](#idalloc)
  * [nodeToHtml](#nodetohtml)
  * [renderSegment](#rendersegment)
  * [regionSources](#regionsources)
* [5. Islands](#5-islands)
  * [Props](#props)
  * [Mounter](#mounter)
  * [Patcher](#patcher)
  * [Unmounter](#unmounter)
  * [Pauser](#pauser)
  * [MountTiming](#mounttiming)
  * [IslandEntry](#islandentry)
  * [Adapter](#adapter)
  * [DeferredIsland](#deferredisland)
  * [Piece](#piece)
  * [registerIsland](#registerisland)
  * [registerDeferredIsland](#registerdeferredisland)
  * [registerModule](#registermodule)
  * [registeredIslands](#registeredislands)
  * [deferredIslands](#deferredislands)
  * [rendersNested](#rendersnested)
  * [boot](#boot)
  * [scan](#scan)
  * [loadEntry](#loadentry)
  * [timingFired](#timingfired)
  * [patchIsland](#patchisland)
  * [islandState](#islandstate)
  * [markerProps](#markerprops)
  * [serverRendered](#serverrendered)
  * [childrenRegion and islandChildren](#childrenregion-and-islandchildren)
  * [adoptNodes](#adoptnodes)
  * [readPieces and piecesOf](#readpieces-and-piecesof)
  * [loadedModule and loadModule](#loadedmodule-and-loadmodule)
  * [markRendered and unmarked](#markrendered-and-unmarked)
  * [defineMounter](#definemounter)
  * [hydrationMismatch](#hydrationmismatch)
  * [discard](#discard)
  * [applyStyles](#applystyles)
  * [mountServer](#mountserver)
  * [isServerIsland](#isserverisland)
  * [morph](#morph)
  * [DOM Contract](#dom-contract)
* [6. Navigation](#6-navigation)
  * [NavigateOptions](#navigateoptions)
  * [NavigationOptions and PrefetchTiming](#navigationoptions-and-prefetchtiming)
  * [enableNavigation](#enablenavigation)
  * [navigate](#navigate)
  * [refresh](#refresh)
  * [prefetch](#prefetch)
  * [clearRouterCache](#clearroutercache)
  * [pendingLink and subscribePendingLink](#pendinglink-and-subscribependinglink)
  * [applyHead](#applyhead)
  * [loadScript](#loadscript)
  * [loadDeclaredScripts](#loaddeclaredscripts)
  * [live](#live)
  * [socket](#socket)
  * [enablePwa](#enablepwa)
  * [enableTelemetry](#enabletelemetry)
* [7. Actions and Sign-In](#7-actions-and-sign-in)
  * [action](#action)
  * [upload](#upload)
  * [signIn](#signin)
  * [enrol](#enrol)
  * [answer](#answer)
  * [AuthStep and AuthOptions](#authstep-and-authoptions)
* [8. The Store](#8-the-store)
  * [Contribution](#contribution)
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
  * [contribute](#contribute)
  * [retain](#retain)
  * [take and restore](#take-and-restore)
  * [setSlotOrder](#setslotorder)
  * [isPending and whenSettled](#ispending-and-whensettled)
* [9. The Locale](#9-the-locale)
  * [currentLocale](#currentlocale)
  * [subscribeLocale](#subscribelocale)
  * [subscribeMessages](#subscribemessages)
  * [setLocale](#setlocale)
  * [adoptLocale](#adoptlocale)
  * [catalog, setCatalog and adoptCatalog](#catalog-setcatalog-and-adoptcatalog)
  * [localePath](#localepath)
  * [markLinks](#marklinks)
  * [currentDocumentPath](#currentdocumentpath)
  * [currentAddressPath](#currentaddresspath)
* [10. The React Mounter](#10-the-react-mounter)
  * [reactMounter](#reactmounter)
  * [reactPatcher](#reactpatcher)
  * [reactUnmounter](#reactunmounter)
  * [reactPauser](#reactpauser)
  * [Island and island](#island-and-island)
  * [Slot](#slot)
  * [Mount](#mount)
  * [foreign](#foreign)
  * [Link](#link)
  * [Picture](#picture)
  * [Embed](#embed)
  * [Place](#place)
  * [useStore](#usestore)
  * [useLocale](#uselocale)
  * [useLinkStatus](#uselinkstatus)
  * [useHoisted](#usehoisted)
  * [useBrowser](#usebrowser)
  * [withHoisted](#withhoisted)
  * [createContext and use](#createcontext-and-use)
* [11. The Vue Mounter](#11-the-vue-mounter)
  * [vueMounter](#vuemounter)
  * [vuePatcher](#vuepatcher)
  * [vueUnmounter](#vueunmounter)
  * [Mount (Vue)](#mount-vue)
  * [Link, Picture, Embed and Place (Vue)](#link-picture-embed-and-place-vue)
  * [useStore (Vue)](#usestore-vue)
  * [useLocale (Vue)](#uselocale-vue)
  * [useLinkStatus (Vue)](#uselinkstatus-vue)
* [12. The Svelte Mounter](#12-the-svelte-mounter)
  * [svelteMounter](#sveltemounter)
  * [sveltePatcher](#sveltepatcher)
  * [svelteUnmounter](#svelteunmounter)
  * [TreePiece](#treepiece)
  * [registerTree](#registertree)
  * [held, opaque and nested](#held-opaque-and-nested)
  * [useStore (Svelte)](#usestore-svelte)
  * [useLocale (Svelte)](#uselocale-svelte)
  * [useLinkStatus and holdLink (Svelte)](#uselinkstatus-and-holdlink-svelte)
  * [link, picture and embed (Svelte)](#link-picture-and-embed-svelte)
  * [island (Svelte)](#island-svelte)
  * [mount (Svelte)](#mount-svelte)
* [13. The JSX Element Builder](#13-the-jsx-element-builder)
  * [JSX](#jsx)
  * [jsx and jsxs](#jsx-and-jsxs)
  * [Placements](#placements)
* [14. Custom Elements and htmx](#14-custom-elements-and-htmx)
  * [shadowOf](#shadowof)
  * [HtmxProcessor](#htmxprocessor)
  * [bindHtmx](#bindhtmx)
* [15. The Standard Library](#15-the-standard-library)
  * [intl](#intl)
  * [text](#text)
  * [time](#time)
  * [crypto](#crypto)
  * [path](#path)
  * [log](#log)
  * [i18n and t](#i18n-and-t)
  * [file](#file)
  * [id](#id)
  * [system](#system)
  * [clipboard](#clipboard)
  * [Scope](#scope)
  * [native](#native)
* [16. Testing](#16-testing)
  * [test and it](#test-and-it)
  * [describe](#describe)
  * [Hooks](#hooks)
  * [expect](#expect)
  * [Matchers](#matchers)
  * [Asymmetric Matchers](#asymmetric-matchers)
  * [Mock Functions](#mock-functions)
  * [ctx](#ctx)
  * [render and renderHook](#render-and-renderhook)
  * [load](#load)
  * [Queries](#queries)
  * [screen and within](#screen-and-within)
  * [waitFor](#waitfor)
  * [fireEvent](#fireevent)
  * [userEvent](#userevent)
  * [settle and advance](#settle-and-advance)
  * [assert](#assert)
* [17. The Desktop Bridge](#17-the-desktop-bridge)
  * [openFile and openExternal](#openfile-and-openexternal)
  * [FileFilter and OpenFileOptions](#filefilter-and-openfileoptions)
* [18. Error Handling](#18-error-handling)
  * [ActionFailure](#actionfailure)
  * [AuthFailure](#authfailure)
  * [AssertionError](#assertionerror)
  * [Thrown Errors](#thrown-errors)
  * [Silent Degradations](#silent-degradations)

## 1. Entry Points

ES module entry points, resolved through an import map. There is no package manifest and no default export. Every module under `dist/` is also addressable as `@snapfire/fsr/client/<name>`.

| Specifier | Built file | Exports | Bare imports |
| --- | --- | --- | --- |
| `@snapfire/fsr/client` | `dist/index.js` | sections 2, 3 and 5 to 9, plus `nodeToHtml` and `renderSegment`, less the members marked as from another module | none |
| `@snapfire/fsr/client/store` | `dist/store.js` | all of section 8 | none |
| `@snapfire/fsr/client/render` | `dist/render.js` | section 4 | none |
| `@snapfire/fsr/client/react` | `dist/react.js` | section 10 | `react`, `react-dom/client` |
| `@snapfire/fsr/client/vue` | `dist/vue.js` | section 11 | `vue` |
| `@snapfire/fsr/client/svelte` | `dist/svelte.js` | section 12 | `svelte`, `svelte/reactivity` |
| `@snapfire/fsr/client/jsx-runtime` | `dist/jsx-runtime.js` | section 13 | none |
| `@snapfire/fsr/template` | `dist/template.js` | `Embed`, `Island`, `island`, `Link`, `Picture`, `Slot`, `createContext` and `use` from the JSX runtime, types `ImageAsset`, `PictureProps` and `LinkProps` | none |
| `@snapfire/fsr/client/elements` | `dist/elements.js` | `shadowOf` | none |
| `@snapfire/fsr/client/htmx` | `dist/htmx.js` | `bindHtmx`, `HtmxProcessor` | none |
| `@snapfire/fsr/client/testing` | `dist/testing.js` | section 16 | none |
| `@snapfire/fsr` | `dist/native.js` | `native` | none |
| `@snapfire/fsr/desktop` | `dist/desktop.js` | section 17 | none |

* `@snapfire/fsr/host` and `@snapfire/fsr/terminal` are read by the build and are never in the import map.
* `importmap.json` in the package root is the map `snapfirec --import-map` checks the bare imports against.

## 2. Values

### SfValue

The union a value decodes into and encodes from.

* `null | undefined | boolean | number | bigint | string`
* `Uint8Array | Int8Array | Int16Array | Uint16Array | Int32Array | Uint32Array | BigInt64Array | BigUint64Array | Float32Array | Float64Array`
* `SfValue[]`
* `Date | URL | Map<SfValue, SfValue> | Set<SfValue>`
* `RefValue | VariantValue | DoubleValue`
* `{ [key: string]: SfValue }`

### RefValue

A reference to a server action or a client module.

* `type RefValue = { readonly kind: "action" | "module"; readonly id: string }`
* Frozen and branded with `Symbol.for("sf.ref")`. `isRef` tests the brand, not the shape.

### VariantValue

A tagged union arm with an optional payload.

* `type VariantValue = { readonly tag: string; readonly payload?: SfValue }`
* Frozen and branded with `Symbol.for("sf.variant")`. The `payload` key is absent, not `undefined`, on a variant without one.

### DoubleValue

A number the server reads as a double whatever its value.

* `type DoubleValue = { readonly value: number }`
* Frozen and branded with `Symbol.for("sf.double")`. `decodeValue` never returns one: a double decodes to a plain `number`.

### Constructors and Guards

* `ref(kind: "action" | "module", id: string): RefValue`
* `actionRef(id: string): RefValue`
* `moduleRef(id: string): RefValue`
* `variant(tag: string, payload?: SfValue): VariantValue`
* `f64(value: number): DoubleValue`
* `isRef(v: unknown): v is RefValue`
* `isVariant(v: unknown): v is VariantValue`
* `isDouble(v: unknown): v is DoubleValue`

### decodeValue

* `decodeValue(json: unknown): SfValue`: the server's tagged JSON as JavaScript values.
* Untagged JSON passes through, recursing into arrays and objects.
* An object whose `$` key holds a string is a tag. An object whose `$` holds anything else is a plain object.
* An `i` or `u` integer within `Number.MIN_SAFE_INTEGER` to `Number.MAX_SAFE_INTEGER` inclusive is a `number`, otherwise a `bigint`.
* A `u8` typed array decodes to a `Uint8Array` marked to encode back as `ta`.
* Throws on an unknown tag or an unknown typed array kind.

### encodeValue

* `encodeValue(v: SfValue): unknown`: the tagged JSON the server decodes.
* A finite `number` is bare. The server reads an integral one as an integer and a fractional one as an `f64`.
* `NaN`, `Infinity` and `-Infinity` are `{ $: "f", v: "nan" | "inf" | "-inf" }`.
* A `DoubleValue` is `{ $: "f", v }`.
* `undefined` is `{ $: "undef" }`. An object property holding `undefined` is left out.
* A `bigint` in the `i128` range is `i`. One above it and within `u128` is `u`. Outside both it throws `bigint outside the value model's integer range`.
* A `Uint8Array` is bytes (`b`), unless `decodeValue` produced it from a `u8` typed array, which goes back as `ta` with kind `u8`.
* Another typed array is `ta` with its kind, covering only its own `byteOffset` and `byteLength` window.
* `Uint8ClampedArray` is not part of the model and encodes as an object.
* A `Date`, `Map`, `Set` and `URL` are `var` with tag `$date`, `$map`, `$set` and `$url`.
* A `Temporal` `Instant`, `PlainDate`, `PlainTime`, `PlainDateTime`, `ZonedDateTime` or `Duration` is `var` with tag `$temporal` and payload `[type, text]`. A browser without `Temporal` reads the text.
* A plain object owning a `$` key is the `m` tag's pair list.

### Tag Encoding

Tags are objects with a `$` discriminant.

| Tag | Fields | JavaScript |
| --- | --- | --- |
| `i` | `v`, decimal string | `number` inside the safe range, else `bigint` |
| `u` | `v`, decimal string | `number` inside the safe range, else `bigint` |
| `f` | `v`, number or `"nan"`, `"inf"`, `"-inf"` | `number` |
| `f32` | `v`, number or one of the same strings | `number` |
| `b` | `v`, base64 | `Uint8Array` |
| `ta` | `k`, kind; `v`, base64 little-endian | matching typed array |
| `m` | `v`, array of `[key, value]` pairs | plain object |
| `var` | `t`, tag; `p`, optional payload | a `Date`, `Map`, `Set`, `bigint`, `URL` or Temporal value when `t` is `$date`, `$map`, `$set`, `$bigint`, `$url` or `$temporal` |
| `ref` | `k`, `"action"` or `"module"`; `id` | `RefValue` |
| `undef` | none | `undefined` |

* Typed array kinds `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32` and `f64` map to `Int8Array`, `Uint8Array`, `Int16Array`, `Uint16Array`, `Int32Array`, `Uint32Array`, `BigInt64Array`, `BigUint64Array`, `Float32Array` and `Float64Array`.
* The decoded byte length must be a multiple of the element size. The typed array constructor throws otherwise.

## 3. Payload Reading

The format the server answers with when a route is requested with `__payload` in its query string.

### SfNode

One node of the payload tree, discriminated by `kind`.

* `{ kind: "text"; text: string }`
* `{ kind: "raw"; html: string }`
* `{ kind: "seq"; children: SfNode[] }`
* `{ kind: "client"; module: string; props: { [key: string]: SfValue }; encoded?: unknown; children: SfNode[]; rendered: boolean }`: an island. `encoded` is the props as the payload carried them. `children` is its markup. `rendered` is whether the server rendered it.
* `{ kind: "pending"; slot: number; fallback: SfNode }`
* `{ kind: "placed"; target: string; children: SfNode[] }`: content written at `target`. `body` is the end of the body and `#name` is the element a `Place` of that name writes.
* `{ kind: "outlet"; name: string }`: where content placed at `#name` goes.

### Segment

One node of the segment sidecar tree.

* `k: string`: the segment key. The part before any `?` names the module that pairs an old segment with a new one.
* `d?: string`: 16 hex digits fingerprinting what the segment rendered, its child segments elided. Absent for a deferred segment.
* `n?: string`: the slot this segment fills in its parent. Absent at the root.
* `p?: number[]`: the path to the subtree from the parent segment's node. `[]` is the whole node and `[i]` is child `i` of a `seq`.
* `s?: number`: the slot id of a deferred segment. A segment carries `p` or `s`, never both.
* `c: Segment[]`: child segments.
* `keep?: string[]`: slots of this segment the payload left unfilled and the browser keeps as they stand.

### Head

What a route says about the document. A field left out keeps what the document has.

* `interface Head { title?: string; description?: string; head?: HeadRow[]; scripts?: Record<string, string>[]; theme?: string | null; scheme?: string | null; forced?: boolean }`
* `interface HeadRow { tag: string; attrs: [string, string][]; children?: string }`: one head element, its attributes in order and the text of a tag that takes content. From `@snapfire/fsr/client/reader`.
* `head`: every head element besides script rows, replacing the ones the document holds.
* `scripts`: the attributes of each script row the route's `meta` declared.
* `theme`: `<html data-sf-theme>`, `null` for the root theme. Absent when the application has no themes.
* `scheme`: `<html data-sf-scheme>`, `null` leaving it off.
* `forced`: `<html data-sf-theme-forced>`.

### Payload

A parsed response.

* `format: number`: the `fmt` field of the `V` row.
* `encoding: string`: the `enc` field of the `V` row.
* `tree: SfNode`: the `N` row.
* `segments: Segment | null`: the `G` row.
* `heads: Head[]`: the `H` rows in arrival order.
* `seeds: Contribution[][]`: the `T` rows in arrival order, decoded.
* `locale: string | null`: the `L` row.
* `catalog: { [key: string]: string } | null`: the `D` row.
* `entry: string | null`: the `E` row.
* `styles: string[]`: the last `C` row, empty when there is none.
* `resolutions: { slot: number; node: SfNode }[]`: the `S` rows in arrival order.

### Row

* `type Row = { tag: "V"; format: number; encoding: string } | { tag: "N"; tree: SfNode } | { tag: "G"; segments: Segment } | { tag: "H"; head: Head } | { tag: "T"; contributions: Contribution[] } | { tag: "L"; locale: string } | { tag: "E"; entry: string } | { tag: "C"; styles: string[] } | { tag: "D"; catalog: { [key: string]: string } } | { tag: "S"; slot: number; node: SfNode; segments: Segment[] }`

### decodeNode

* `decodeNode(row: unknown): SfNode`: reads one node row.
* Rows: `["t", text]`, `["r", html]`, `["q", children]`, `["c", { m, p, ch, r }]`, `["p", slot, fallback]`, `["l", target, children]` and `["o", name]`.
* In a `c` row, `p` goes through `decodeValue`, `ch` defaults to empty and `r: true` sets `rendered`.
* Throws `unknown node row kind: <kind>`.

### parseRow

* `parseRow(line: string): Row`: one row, its tag, a space, then its body.
* Throws `unknown payload row tag: <tag>` for a tag outside `V`, `N`, `G`, `H`, `T`, `L`, `E`, `C`, `D` and `S`.

### parsePayload

* `parsePayload(text: string): Payload`: a whole response body, one row per line through `parseRow`, empty lines skipped.
* Throws `payload has no N row`.

### linesOf

* `linesOf(res: Response): AsyncGenerator<string>`: the rows of a response body as they arrive, decoded as UTF-8 and cut at newlines.
* Empty lines are skipped and an unterminated last line is yielded when the stream ends.
* A response without a body stream yields the rows of `res.text()` at once.

### Row Grammar

Each row is a tag character, a space, then its body, ended by a newline. The eager wave is `V`, `N`, any of `H`, `T`, `L`, `E`, `C` and `D`, then `G`, which closes it. Every row after `G` belongs to a resolution.

| Row | Body | Meaning |
| --- | --- | --- |
| `V` | `{"fmt":1,"enc":"json"}` | Format version and encoding |
| `N` | one node row | The initial tree |
| `G` | one segment object | The segment sidecar |
| `H` | a `Head` object | What the route says about the document |
| `T` | a list of `{k, p, v}` objects | What the route's segments seeded, ahead of the `S` row it belongs to |
| `L` | a JSON string | The locale the response was rendered in |
| `E` | a JSON string | A module to import before the response's islands can mount |
| `C` | a JSON array of strings | Stylesheets the response needs beyond the document's own |
| `D` | a JSON object of strings | The locale's message catalog, omitted when the request's `x-sf-catalog` named that locale |
| `S` | slot id, a space, then `{"n": node row, "g": [segments]}` | One resolved slot |

* `S` rows arrive in completion order, not slot order. A resolution may open further slots that arrive later in the same stream.

## 4. Rendering

From `@snapfire/fsr/client/render`. `nodeToHtml` and `renderSegment` are also on the core entry.

### IdAlloc

* `interface IdAlloc { next: number; segment?: string; island?: string }`: the id counter shared by every call contributing to one document. `segment` and `island` name the placed content being written.

### nodeToHtml

* `nodeToHtml(node: SfNode, ids: IdAlloc): string`: a node tree as markup.
* `text` has `&`, `<` and `>` escaped. `raw` is written verbatim.
* `client` is an `<sf-i>` marker around its children, with `data-sf-ssr` when `rendered`, then `<script type="application/json" data-sf-props="…">`. Every `<` in the props JSON is written `<`.
* `pending` is `<div data-sf-slot="N">` around its fallback.
* `placed` is `<template data-sf-placed="KEY" data-sf-island="NAME">` around its children.
* Each island takes the next id from `ids`: `sf-c0`, `sf-c1` and upward.

### renderSegment

* `renderSegment(node: SfNode, seg: Segment, ids: IdAlloc): string`: a segment's subtree between `<!--sf-g:key-->` and `<!--/sf-g-->`, child segments at their sidecar positions.
* Slot-addressed children are skipped.
* In the key, `%` is written `%25` and `-` is written `%2D`.
* Throws `segment path walks through a non-seq node`.

### regionSources

* `regionSources(node: SfNode, ids: IdAlloc): Map<string, RegionSource>`: the island regions inside `node`, keyed by the `$k` prop each client node carries.
* `interface RegionSource { props: { [key: string]: SfValue }; encoded?: unknown; html: string; nested: Map<string, RegionSource>; children: string | null; markup: { [n: string]: string } | null }`
* `childrenOf(node: SfNode, ids: IdAlloc): string | null`: the markup of an island's `<sf-s data-sf-children>` region outside any nested island. `null` for a node that is not an island or has none.
* `markupOf(node: SfNode, ids: IdAlloc): { [n: string]: string } | null`: the markup of each `data-sf-m` markup prop region, by region number.
* Constants: `CHILDREN_ATTR = "data-sf-children"`, `SSR_ATTR = "data-sf-ssr"`, `MARKUP_ATTR = "data-sf-m"`, `REGION_KEY = "$k"`, `PLACED_ATTR = "data-sf-placed"`, `PLACED_OPEN = "data-sf-l"`, `PLACED_END = "data-sf-l-end"`, `PLACED_ISLAND = "data-sf-island"`, `PLACED_TO = "data-sf-to"`.

## 5. Islands

### Props

* `type Props = { [key: string]: SfValue }`

### Mounter

* `type Mounter = (module: unknown, props: Props, el: Element, hydrate: boolean, children: Piece[] | null) => unknown`: mounts an island at `el`.
* `module` is what the entry's `loader` resolved to.
* `hydrate` is true when the marker carries `data-sf-ssr`.
* `children` is what the server placed as the island's children, `null` when it placed none.
* The return value is the handle passed to the entry's `Patcher`, `Unmounter` and `Pauser`.

### Patcher

* `type Patcher = (handle: unknown, module: unknown, props: Props, el: Element) => void`: re-renders the island at `el` with `props`, keeping its DOM and state.

### Unmounter

* `type Unmounter = (handle: unknown, el: Element) => void`: ends the island mounted at `el`. Called by `discard`.

### Pauser

* `type Pauser = (handle: unknown, el: Element, paused: boolean) => boolean | Promise<void>`: with `paused` true, hides the island at `el` in place with its state kept and its effects stopped. With `paused` false, shows it again.
* Showing may return a promise that settles once the island is shown.
* `false` means the framework cannot pause. The navigator then ends the island and mounts it again later.

### MountTiming

* `type MountTiming = "load" | "visible" | "idle"`
* `"load"` mounts as soon as the marker is scanned.
* `"visible"` mounts on the first intersection seen by an `IntersectionObserver`, at once where there is none.
* `"idle"` mounts in `requestIdleCallback`. Where there is none it mounts in a 1 ms `setTimeout`.

### IslandEntry

What a module id is registered with.

* `interface IslandEntry { loader: () => Promise<unknown>; mount: Mounter; when?: MountTiming; patch?: Patcher; unmount?: Unmounter; pause?: Pauser }`
* `when` defaults to `"load"`.
* Without `patch`, a navigation that keeps the island's place replaces it with a new marker, losing its state.
* Without `unmount`, `discard` drops the root as it stands.
* Without `pause`, a held page ends the island.

### Adapter

* `interface Adapter { mount: Mounter; patch?: Patcher; unmount?: Unmounter; pause?: Pauser }`: the fields of an `IslandEntry` an adapter module supplies.

### DeferredIsland

* `interface DeferredIsland { loader: () => Promise<unknown>; adapter: () => Promise<Adapter>; when?: MountTiming }`: an island registered before its adapter is loaded.

### Piece

One node of an island's children as the server wrote it.

* `type Piece = TextPiece | CommentPiece | ElementPiece | IslandPiece`
* `interface TextPiece { kind: "text"; text: string }`
* `interface CommentPiece { kind: "comment"; text: string }`
* `interface ElementPiece { kind: "element"; node: Element; tag: string; attrs: { [name: string]: string }; children: Piece[] }`
* `interface IslandPiece { kind: "island"; marker: Element; script: Element | null; moduleId: string; entry: IslandEntry | undefined; props: Props; encoded: unknown; when: MountTiming; ssr: boolean; children: Piece[] | null }`: `entry` is undefined while no registry names the module.

### registerIsland

* `registerIsland(moduleId: string, entry: IslandEntry): void`: registers or replaces the entry for a module id in the page-wide registry.
* `moduleId` must equal the marker's `data-sf-module` exactly. Only markers scanned afterwards are affected.

### registerDeferredIsland

* `registerDeferredIsland(moduleId: string, island: DeferredIsland): void`: registers or replaces an island whose adapter is imported when a scan first finds its marker.
* The adapter is imported once for every island registered with the same `adapter` function. Each is then registered as `registerIsland` would and the document is scanned again.
* An adapter already loaded is used at once.
* A missing island is not reported while an adapter import is in flight.

### registerModule

* `registerModule(file: string, loader: () => Promise<unknown>): void`: the loader for a file a function prop is imported from, by its build path such as `src/App.js`.
* A `moduleRef("src/App.js#header")` among an island's props resolves to that export before the island mounts or is patched.
* A reference to an unregistered file stays a reference and warns `sf: no module registered for <file>`.

### registeredIslands

* `registeredIslands(): ReadonlyMap<string, IslandEntry>`: every registered island, by module id.

### deferredIslands

* `deferredIslands(): ReadonlyMap<string, DeferredIsland>`: every deferred island whose adapter has not loaded yet, by module id.

### rendersNested

* `rendersNested(mount: Mounter, takes?: (el: Element) => boolean): void`: declares that `mount`'s adapter renders islands of its own framework nested inside the islands it mounts. A scan leaves those to the parent's adapter.
* `takes` picks which of them the adapter renders. The scan mounts the rest as roots of their own.
* An island in a layout's segment, a named slot or a server-mode region is always a root of its own.
* `nestsIn(outer: IslandEntry | undefined, inner: IslandEntry | undefined): boolean`: whether `outer`'s adapter renders `inner` in its tree.

### boot

* `boot(): void`: scans the whole document and scans again on every `sf:fill` event on `document`.
* Scans at once when the DOM is past `loading`, otherwise on `DOMContentLoaded`.
* Also calls `adopt`, `adoptLocale`, `adoptCatalog`, `enableEmbeds`, `enablePwa` and `loadDeclaredScripts`. It sets `window.__sf.script` to `loadMarkedScript`.
* A second call scans again and adds no second listener.

### scan

* `scan(root: ParentNode): void`: mounts every `sf-i:not([data-sf-scheduled])` under `root`. Idempotent.
* Skips a marker with no `data-sf-module`. Stamps `data-sf-scheduled` on the rest.
* Timing comes from the `data-sf-when` of the enclosing `sf-s` region, else the entry's `when`.
* `data-sf-mounted` is stamped once the mounter has run.
* Modules load in parallel. Islands mount in the order they were scheduled, which is document order within a scan.
* Props come from `script[data-sf-props="<marker id>"]`, searched under `root` and then the whole document. A missing or empty script gives `{}`.
* A marker inside `<sf-s data-sf-mode="server">` mounts through `mountServer`.
* A marker whose module id no registry knows is left as rendered. Each id still unregistered once the document settles warns `sf: no island registered for <id>`, never while an entry named by `loadEntry` is loading.

### loadEntry

* `loadEntry(src: string): void`: imports `src` once, then rescans the document.
* No missing island is reported while it is in flight.
* A failure warns `sf: loading <src> failed` and forgets `src`. A later call imports it again.

### timingFired

* `timingFired(when: MountTiming, find: () => Element | null): Promise<void>`: settles when a timing fires, by the rules of `MountTiming`.
* `find` returns the element `visible` observes. When it returns `null` it is asked again on the next task.

### patchIsland

* `patchIsland(el: Element, props: Props, regions?: unknown, children?: string | null, encoded?: unknown, markup?: { [n: string]: string } | null): Promise<boolean>`: re-renders the island at `el` with `props` through its entry's `patch`, keeping its DOM and state.
* `regions`, `children` and `markup` are read back by the adapter through `islandState`.
* `children` is the new markup of the island's children region. The adapter writes it when it differs, remounting any island inside it.
* A server island renders again on the server, with `encoded` as its props when given.
* `false` when nothing is mounted at `el`, the mount failed or the entry has no `patch`.

### islandState

* `islandState(el: Element): { props: Props; regions: unknown; children: string | null; markup: { [n: string]: string } | null } | null`: what the island at `el` last mounted or patched with. `null` when nothing is mounted there. From `@snapfire/fsr/client/boot`.

### markerProps

* `markerProps(marker: Element): { script: Element | null; props: Props; encoded: unknown }`: the props script that is the marker's next element sibling, decoded and as written. An absent or empty script gives `{}`.

### serverRendered

* `serverRendered(el: Element): boolean`: whether the marker carries `data-sf-ssr`.

### childrenRegion and islandChildren

* `childrenRegion(el: Element): Element | null`: the island's `<sf-s data-sf-children>`, else the inert `<template data-sf-children>` after its markup. Never one inside a nested island.
* `islandChildren(el: Element): Piece[] | null`: that region as pieces, `null` when there is none.

### adoptNodes

* `adoptNodes(to: Element, from: ParentNode | null): void`: moves the nodes inside `from` into `to`, replacing what `to` held. A template gives its content.
* Moves nothing when `to` is `from` or `from` is empty.

### readPieces and piecesOf

* `readPieces(nodes: ArrayLike<Node>): Piece[]`: DOM nodes as pieces. An `sf-i` marker and the props script after it are one island piece.
* `piecesOf(html: string): Piece[]`: markup as pieces.

### loadedModule and loadModule

* `loadedModule(entry: IslandEntry): unknown`: the entry's module once loaded, else `undefined`.
* `loadModule(entry: IslandEntry): Promise<unknown>`: loads it and holds it for `loadedModule`.

### markRendered and unmarked

* `markRendered(el: Element): void`: stamps `data-sf-scheduled` and `data-sf-mounted` on a marker an adapter rendered in its parent's tree. A scan leaves such a marker alone.
* `unmarked(html: string): string`: `html` with both attributes removed.

### defineMounter

* `const defineMounter: Mounter`: does nothing. The mounter for an island whose module defines a custom element, where the import is the mount.

### hydrationMismatch

* `hydrationMismatch(el: Element, detail?: unknown): void`: logs `sf: <module> did not hydrate to the server's markup` with `console.error`, `detail` beside it, for the marker at or above `el`.
* The React, Vue and Svelte mounters call it once per island that did not hydrate.

### discard

* `discard(root: ParentNode): void`: ends every island under `root` and `root` itself when it is a marker, nested islands first.
* A mount still waiting on its timing is called off. One whose loader is in flight mounts nothing. A mounted one goes to its entry's `unmount`. A server island stops following the store.
* Afterwards `islandState` returns `null` for those markers and `patchIsland` returns `false`.
* Call it before removing markers from the document, unless the removal is a mounted root's own render.

### applyStyles

* `applyStyles(hrefs: string[], timeout?: number): Promise<void>`: brings the client-owned stylesheets, `link[data-sf-css]`, to exactly `hrefs`.
* New links go after every other stylesheet. The promise resolves once they load or after `timeout` ms, 2000 by default. The links `hrefs` no longer names are then removed.
* A stylesheet without `data-sf-css` is never removed.

### mountServer

* `mountServer(el: Element, module: string, encoded: unknown): void`: mounts `el` as a server-mode island from its props as the server encoded them.
* `$s` in `encoded` is its state and `$sk` the store keys it reads.
* It listens on `el` for every event type its markup's `data-sf-on` binds. `submit` is prevented.
* An event posts `{ props, state, handler, event }` to `/_sf/island/<module>`. `handler` is the bound token, a number for an index and a string for a name. `event` carries the target's `value`, `checked` and `name` and a keyboard event's key.
* Each post sends the browser's values of the keys it reads as the `$store` prop. The answer's `state` is kept, its `html` is patched in with `morph` and its `store` keys are written.
* An answer carrying `revalidate` calls `refresh` once the patch is in.
* While a post is out, the island carries `data-sf-pending` and further events are dropped.
* A write to a key it reads by another island posts again with no handler. A write during a post posts once more when that one lands.
* A failed post warns `sf: island <module> step failed with <status>: <text>` and leaves the island as it was.
* `endServer(el: Element): void` stops it following the store. `discard` calls it. From `@snapfire/fsr/client/server`.

### isServerIsland

* `isServerIsland(el: Element): boolean`: whether `mountServer` mounted `el`.

### morph

* `morph(el: Element, html: string, before?: string): void`: patches `el`'s children to match `html`, keeping the nodes it can.
* Text and comment nodes match by content, elements by tag and position or by `data-sf-key`, attributes by name.
* With `before`, the markup last written there, only what changed between `before` and `html` is applied. Nodes and attributes `before` does not account for are left as they stand.
* A focused form control keeps its value. An unfocused one takes the new one.
* An `sf-i` is kept as it stands. When its props script changed, the script takes the new text and the island takes the new props through `patchIsland`.

### DOM Contract

What the server writes and this package reads.

| Marker | Written by | Read by |
| --- | --- | --- |
| `<sf-i id data-sf-module>` | the HTML serialiser, `nodeToHtml` | `scan` |
| `<script type="application/json" data-sf-props="<id>">` | the HTML serialiser, `nodeToHtml` | `scan` |
| `data-sf-ssr` on `<sf-i>` | the HTML serialiser, on an island whose markup the server rendered | `serverRendered` |
| `<sf-s data-sf-children>`, else `<template data-sf-children>` after the island's markup | the renderer, around an island's children | `islandChildren` |
| `<!--[-->` … `<!--]-->` around a Svelte island's children, each in `<!--[0-->`, `<!--[1-->`, `<!--[2-->` or `<!--[-1-->` | the renderer, under `svelte/server` | `svelteMounter`, Svelte's hydration |
| `data-sf-scheduled` | `scan`, on the marker it takes | `scan`, `navigate`, React's `Island` |
| `data-sf-mounted` | the mount, once the mounter has run | tests and anything asking whether an island is live |
| `<div data-sf-slot="N">` | the HTML serialiser, `nodeToHtml` | the fill script, `refresh`, `navigate` |
| `<template data-sf-fill="N">` | the streamed HTML response | the fill script |
| `<template data-sf-l="KEY" data-sf-island="NAME">` … `<template data-sf-l-end>` at the end of the body | the HTML serialiser; the navigator, from `<template data-sf-placed>` | `navigate`; the Vue mounter, for teleport hydration |
| `data-sf-to` on placed content | the HTML serialiser, for content whose place has not arrived | `scan`, which moves it into the place |
| `sf:fill` `CustomEvent` on `document`, `detail` the slot id | the fill script; `navigate` and `refresh`, once per `S` row | `boot`, `enableNavigation` |
| `sf:navigate` `CustomEvent` on `document`, `detail` `{ path }` | `navigate` and `refresh`, once the eager wave is applied | `bindHtmx` and any other library that processes new markup |
| `sf:update-ready` `CustomEvent` on `document` | `enablePwa` | the application |
| `sf_state` cookie | the host, a new generation whenever a written session is saved or destroyed | `navigate`, `prefetch` |
| `<!--sf-g:key-->` and `<!--/sf-g-->` | the segment writer, `renderSegment` | `navigate`, `refresh` |
| `<sf-s>` | a layout's markup, around its child segment | `reactMounter` |
| `<sf-s data-sf-name="…">` | a layout's markup, around a named slot | `Slot`, `reactMounter`, `navigate` |
| `<sf-s data-sf-island data-sf-region="…" data-sf-when="…">` | a page's or layout's markup, around a component placed as an island | `Island`, `scan` |
| `<sf-s data-sf-island data-sf-mode="server">` | the same region for a server-mode island | `scan` |
| `data-sf-on="click:0 change:1"` | the renderer, on an element of a server-mode island; a Tera template writes names, `click:filter` | `mountServer` |
| `data-sf-key` | the renderer, from an element's `key`, in server mode only | `morph` |
| `data-sf-pending` | `mountServer` while a post is out; `navigate` on the clicked anchor | the application's styles |
| `data-sf-css` on `<link rel="stylesheet">` | the renderer; `applyStyles` | `applyStyles` |
| `$s`, `$sk`, `$sv`, `$h`, `$k`, `$aw` in the props script | the renderer: state, read keys, store values rendered from, hoisted values, region key, awaited keys | `mountServer`, the mounters, `boot` |
| `<script type="application/json" data-sf-segments>` | the streamed HTML response | `enableNavigation` |
| `script[data-sf-store]` | the HTML serialiser; a fragment, with `data-sf-fragment` | `adopt` |
| `<html lang data-sf-locale>` | the HTML serialiser | `adoptLocale` |
| `<script type="application/json" data-sf-i18n="<tag>">` | the HTML serialiser | `adoptCatalog` |
| `script[data-sf-script]` | the host, for a script row | `loadDeclaredScripts` |
| `<meta name="sf:worker">` | the host under `[pwa]` outside development | `enablePwa` |
| `<meta name="sf:images">` | the host, from the build's image manifest | `Picture` |

## 6. Navigation

The functions in this section share one document state: the segment sidecar, the current path and a router cache of payloads keyed by origin path, target slot and `pathname + search`.

### NavigateOptions

* `interface NavigateOptions { full?: boolean; into?: string; replace?: boolean; keep?: boolean; scroll?: boolean; transition?: boolean; activity?: boolean }`
* `full`: requests the document's rendering of the target, never an intercept. Sends no `x-sf-from`.
* `into`: renders the target into this slot of the nearest live layout that declares it. Sent as `x-sf-into`.
* Without `full` or `into`, `x-sf-from` carries the document's path and search.
* `replace`: replaces the current history entry.
* `keep`: whether a segment whose key changed within the same module is morphed in place rather than replaced. Defaults to true when the target has the current pathname.
* `scroll`: whether the window scrolls to the fragment's element or the top. Defaults to true.
* `transition`: runs the swap inside `document.startViewTransition`, over the document's `transitions`. Skipped without `startViewTransition`, under `(prefers-reduced-motion: reduce)` and while `document.visibilityState` is `hidden`.
* `activity`: holds the regions the navigation replaces and puts back one held under the incoming segment's key, over the document's `activity`.
* An anchor sets `full`, `into`, `keep`, `transition` and `activity` with `data-sf-full`, `data-sf-into`, `data-sf-keep`, `data-sf-transition` and `data-sf-activity`. `data-sf-keep="false"`, `data-sf-transition="none"` and `data-sf-activity="none"` are `false`. Any other value is `true`.

Under `activity`:

* A held region keeps its DOM. Each island whose entry has `pause` is paused. Every other island with an `unmount` is ended and mounted afresh when the region comes back. An island without `unmount` stays as it is.
* A region put back whose segments all carry their held digests keeps its DOM and its islands take the payload's props. Otherwise it is morphed to the payload's markup with form values left as they are.
* A step through history to a page put back restores its scroll.
* Held regions are kept in least recently held order up to `activityLimit`. `clearRouterCache`, `refresh` and a change of `sf_state` generation drop them.

### NavigationOptions and PrefetchTiming

* `interface NavigationOptions { prefetch?: PrefetchTiming; cacheMs?: number; transitions?: boolean; activity?: boolean; activityLimit?: number }`
* `type PrefetchTiming = "hover" | "viewport" | "none"`
* `prefetch`: the timing of a link without its own `data-sf-prefetch`. Defaults to `"hover"`.
* `cacheMs`: how long a fetched payload answers a navigation. Defaults to 30000.
* `transitions`: the document's `transition` setting. Defaults to false.
* `activity`: the document's `activity` setting. Defaults to false.
* `activityLimit`: how many pages `activity` holds. Defaults to 10.

### enableNavigation

* `enableNavigation(options?: NavigationOptions): void`: takes over same-origin link clicks and history for the document.
* Reads `script[data-sf-segments]` as the current sidecar and sets `window.__sf.refresh` to `refresh`.
* Installs a `click` listener on `document` and a `popstate` listener on `window`.
* Prefetches a `"hover"` link on `mouseover`, `focusin` and passive `touchstart`. Observes a `"viewport"` link with one `IntersectionObserver` and prefetches it once as it enters the view.
* A click is left to the browser when `defaultPrevented` is set, `button` is not 0, a modifier key is held, there is no enclosing `a[href]`, the anchor carries `data-sf-native` or the href is another origin. Otherwise it calls `navigate` with the anchor's options.
* `data-sf-prefetch="none"` turns prefetch off for that link.
* A second call on the same document keeps the listeners and the sidecar and changes only the options it names. A new document is wired afresh.

### navigate

* `navigate(href: string, push?: boolean, options?: NavigateOptions): Promise<void>`: moves to `href` by payload. Resolves once the last row has been applied.
* `push` defaults to true. False is a step through history.
* A fragment of the page already showing scrolls without a fetch.
* Fetches `<pathname><search>` with `__payload` appended, unless the router cache holds a payload for the same request fetched less than `cacheMs` ago under the current `sf_state` generation.
* The response's `C` stylesheets are added before the swap and the ones it no longer names are removed after it.
* At the `G` row the eager wave is applied, history is pushed or replaced and the window scrolls. An intercept does not scroll. A step through history restores the entry's recorded scroll once every paused island is shown.
* `sf:navigate` is dispatched after the eager wave. Each later `S` row fills its slot, rescans and dispatches `sf:fill`. Each `H` row goes to `applyHead` and each `T` row to `contribute`.
* A step back to the page an open intercept sits over makes no request.
* Each history entry's state carries an `sfEntry` id.
* A segment whose digest both responses agree on keeps its region. The first key mismatch replaces that region. Under `keep` it morphs the region instead.
* A mounted island the new markup places again keeps its DOM and state and takes the new props.
* A child segment the new side lacks empties back to its fallback, unless the new segment's `keep` names its slot.
* A non-ok response, a feed that ends before `G` or a payload that cannot be patched falls back to `window.location.assign(href)`.
* A `navigate` or `refresh` started later takes over and this one stops applying rows.

### refresh

* `refresh(): Promise<void>`: drops the router cache and held pages, re-fetches the current URL as a payload and applies it as `navigate` does.
* Sends `x-sf-into` naming the open intercept's slot when there is one.
* A kept region that is not an island is replaced when its digest moved. It is also replaced when neither response carries a digest.
* Every kept island takes its new props in place.
* A call while a refresh is running returns that refresh's promise and queues one more run, shared by every call in the meantime.
* Falls back to `window.location.reload()` when there is no sidecar, the response is not ok or the payload cannot be applied. A request that gets no response leaves the page as it is.
* A `404` payload is applied like any other.

### prefetch

* `prefetch(href: string, options?: NavigateOptions): Promise<void>`: fetches `href`'s payload into the router cache and resolves once it is whole.
* Another origin resolves at once.
* A fresh cached payload for the same request is reused. One from an earlier `sf_state` generation is dropped.
* A non-ok response is not cached.

### clearRouterCache

* `clearRouterCache(): void`: drops every cached payload and discards every fetch in flight when it lands.

### pendingLink and subscribePendingLink

* `pendingLink(): Element | null`: the anchor whose click started the navigation in flight.
* `subscribePendingLink(watcher: () => void): () => void`: calls `watcher` when it changes. Returns the unsubscriber.
* The anchor carries `data-sf-pending` from the start of the fetch until the eager wave is applied.
* A later navigation clears the mark and marks its own anchor.
* A navigation not started by a click and a fragment-only step mark nothing. A fallback to a full load leaves the mark on.

### applyHead

* `applyHead(head: Head): void`: applies one `Head` to the document. `navigate` and `refresh` call it for every `H` row.
* `title` sets `document.title`. `description` sets `meta[name="description"]`, creating it when absent.
* `head` replaces the elements between `<!--sf-head-->` and `<!--/sf-head-->`, script rows aside. An unchanged element stays in place. A document without the comments is left alone.
* Each entry of `scripts` goes to `loadMarkedScript`.
* `theme` and `scheme` set `data-sf-theme` and `data-sf-scheme` on `<html>`, `null` removing them. `forced` sets or removes `data-sf-theme-forced`. A missing key leaves its attribute.

### loadScript

* `loadScript(src: string, options?: ScriptOptions): Promise<void>`: adds `<script src>` to `document.head` once per resolved URL. Resolves on `load` and rejects with `Error("<src> did not load")` on `error`.
* `interface ScriptOptions { when?: ScriptTiming; attrs?: Record<string, string | boolean> }`
* `type ScriptTiming = "load" | "idle" | "interaction"`
* `when` defaults to `"load"`, which adds the element at once. `"idle"` waits for `requestIdleCallback`. `"interaction"` waits for the first `pointerdown`, `keydown`, `touchstart` or `scroll` on `document`.
* `attrs` are set on the element, `true` as an empty attribute and `false` left off. `async` is set unless `attrs` names it.
* A second call for the same URL returns the first call's promise.

### loadDeclaredScripts

* `loadDeclaredScripts(root?: ParentNode): void`: calls `loadMarkedScript` for every `script[data-sf-script]` under `root`, the document by default.
* `loadMarkedScript(marker: Record<string, string>): void`: loads `data-src` at the timing `data-sf-script` names. Every other attribute but `type` is set as written and `data-type` is set as `type`.
* A failure is logged with `console.error` as `sf: <message>`.

### live

* `live(topics: string[], options?: LiveOptions): () => void`: follows `topics` over the host's event stream. Returns the function that closes it.
* `interface LiveOptions { onTopic?: (topic: string) => void; path?: string }`
* `onTopic` defaults to `refresh()`. `path` defaults to `/_sf/live`.
* The browser reconnects a dropped stream on its own.
* The stream closes on `pagehide` and opens again on a `pageshow` from the back-forward cache. A topic published meanwhile is not replayed.
* Returns a no-op when `topics` is empty or `EventSource` is absent.

### socket

* `socket(topic: string, options?: SocketOptions): Socket`: a WebSocket on `topic` that reconnects when it drops.
* `interface SocketOptions { onRow?: (key: string, value: unknown) => void; onOpen?: () => void; onClose?: () => void; path?: string; backoffMs?: number }`
* `interface Socket { send(key: string, value: SfValue): void; open(): boolean; close(): void }`
* `path` defaults to `/_sf/socket`. `backoffMs` defaults to 500 and doubles on each retry up to a minute.
* `onOpen` runs on every connection. `onClose` runs on every drop, not after `close()`.
* Rows from the server are written into the store under their keys unless `onRow` is given.
* `send` while the socket is down drops the row.
* Returns a socket whose methods do nothing where `WebSocket` is absent.

### enablePwa

* `enablePwa(): void`: keeps `online` current and registers the service worker the document names. `boot` calls it. A second call on the same document does nothing.
* `const online: StoreKey<boolean>`: the key `sf/online`, from `navigator.onLine` and the window's `online` and `offline` events.
* `const updateReady: StoreKey<boolean>`: the key `sf/update-ready`.
* Registers `<meta name="sf:worker" content="…">` with scope `/`. A registration failure warns `fsr: the service worker did not register`.
* When a newer worker takes over a page an older one controlled, it sets `updateReady` and dispatches `sf:update-ready` on the document.

### enableTelemetry

* `enableTelemetry(options: TelemetryOptions): void`: posts this page view's web vitals as JSON to `options.beacon` once, when the page is hidden.
* `interface TelemetryOptions { beacon: string; app: string; version: string; sample?: number; slowMs?: number }`
* `sample`: the share of page views that report, 0 to 1. Defaults to 1.
* `slowMs`: a page whose largest paint takes at least this many ms adds `traceId` from `<meta name="sf-trace">`. Left out, no report carries one.
* The report holds `app`, `version`, `route` from `<meta name="sf-route">`, a device class and `samples` of `ttfb_ms`, `fcp_ms`, `lcp_ms` and `inp_ms`. It holds no URL.

## 7. Actions and Sign-In

### action

* `action(id: string, opts?: { revalidate?: boolean }): (input?: SfValue) => Promise<SfValue>`: a callable for an action id.
* A call POSTs `JSON.stringify(encodeValue(input))` to `/_sf/action/${encodeURIComponent(id)}` with `content-type: application/json` and `x-sf-from` set to the page's path and search. `input` defaults to `{}`.
* Success returns `decodeValue` of the JSON body.
* A non-ok status throws `ActionFailure`.
* When the response carries `x-sf-redirect`, the call awaits `navigate` to that path.
* Otherwise `revalidate`, true by default, awaits `refresh()` before returning.

### upload

* `upload(id: string, form: FormData, opts?: { revalidate?: boolean }): Promise<SfValue>`: POSTs a `FormData` to the same endpoint. Each file part reaches the action as an `Upload`.
* Sends `accept: application/json` and `x-sf-from`. The browser sets the multipart content type.
* The form must carry `_csrf`, taken from the page's `csrf_token` prop. An anonymous visitor has a token only under `[session] csrf = "always"`.
* Failures, redirects and `revalidate` behave as in `action`.

### signIn

* `signIn(opts?: AuthOptions): Promise<AuthStep>`: begins a sign-in with `GET /auth/login?return_to=<opts.returnTo or the current path and query>&method=<opts.method>`, asking for JSON, then runs the flow.
* A `redirect` or `complete` step moves the browser with `window.location.assign` unless `opts.navigate` is `false`. The step is returned.
* A `present` step of kind `webauthn.get` or `webauthn.create` runs `navigator.credentials.get` or `create` over `data.publicKey` (`data` when it has none). `challenge`, `user.id` and each `allowCredentials` and `excludeCredentials` id are decoded from base64url. `data.mediation` is passed through.
* The passkey answer is `{ credential }` with `id`, `rawId`, `type`, `response` and `extensions` and every byte field as base64url.
* Any other `present` step goes to `opts.present`. Its answer is posted with `answer`.
* The loop ends when a step leaves the page, completes or goes unanswered.

### enrol

* `enrol(csrf: string, opts?: AuthOptions): Promise<AuthStep>`: begins adding a method for the signed-in user with `POST /auth/enrol`, `csrf` sent as `x-sf-csrf`, then runs the flow as `signIn` does.

### answer

* `answer(params: Record<string, unknown>): Promise<AuthStep>`: posts `params` as JSON to `/auth/callback` and returns the next step without following it.

### AuthStep and AuthOptions

* `type AuthStep = { next: "redirect"; to: string } | { next: "present"; kind: string; data: Record<string, unknown> } | { next: "complete"; to: string }`
* `interface AuthOptions { returnTo?: string; method?: string; present?: (step: { kind: string; data: Record<string, unknown> }) => Promise<Record<string, unknown> | undefined> | Record<string, unknown> | undefined; navigate?: boolean }`
* `navigate` defaults to true.

## 8. The Store

One keyed map per document, shared by every island. `@snapfire/fsr/client/store` exports the whole section. The core entry exports `key`, `get`, `set`, `clear`, `subscribe`, `transaction`, `derive`, `optimistic`, `seed`, `adopt`, `reset`, `snapshot` and the types `StoreKey` and `StoreListener`.

* The store is the merge of the contributions the route's segments seeded, one per segment.
* A deeper segment wins a key an outer one also sets. At one depth, the slot ranked later by `[store] slot_order` wins, by name where the list says nothing.
* An island's write stands until a contribution names its key again.
* A segment's contribution leaves with the segment.
* Contributions arrive as `script[data-sf-store]` in a document, `T` rows in a payload and `__sfStore(…)` calls in a streamed resolution.

### Contribution

* `interface Contribution { k: string; p: string[]; v: { [key: string]: unknown }; d?: { [key: string]: unknown }; w?: string[]; o?: string }`: what one segment seeded.
* `k`: the segment key. `p`: the slot names from the root to it. `v`: the values, decoded.
* `d`: what the segment's `defaults` export seeded. It sits under every `v` and under an island's write.
* `w`: the keys a deferred segment will seed. A contribution with `w` keeps the values its segment last seeded until the seed lands.
* `o`: on a fragment's seed for a segment the document does not show, the segment whose region the fragment landed in.

### StoreKey

* `type StoreKey<T> = string & { readonly __store?: T }`: a key is the string it names. `T` is a phantom type.
* `type StoreListener = (value: unknown, key: string) => void`

### key

* `key<T>(id: string): StoreKey<T>`: names a key. The build reads `key()` through an import as its string.

### get

* `get<T>(k: StoreKey<T>): T | undefined`: what the key holds, `undefined` when nothing has set it.

### set

* `set<T>(k: StoreKey<T>, value: T): void`: writes the key and notifies its listeners. Writing the value already held notifies nobody.

### clear

* `clear<T>(k: StoreKey<T>): void`: forgets the key and notifies.

### subscribe

* `subscribe(k: StoreKey<unknown> | string, listener: StoreListener): () => void`: registers a listener. Returns the function that removes it.

### transaction

* `transaction(work: () => void): void`: runs `work` synchronously with notifications collapsed. Each key written fires once afterwards.
* A nested call defers to the outermost.

### derive

* `derive<T>(k: StoreKey<T>, sources: StoreKey<unknown>[] | string[], compute: (read: <V>(source: StoreKey<V>) => V | undefined) => T): void`: registers a key computed from others and computes it now.
* It recomputes when a source changes and when the locale or a catalog changes.
* A derived value beats a contribution for the same key and loses to an island's write.
* A `derive` at module level in a browser file, with a compute the build can lower, is also computed by the server.

### optimistic

* `optimistic<T, R>(k: StoreKey<T>, guess: T, remote: () => Promise<R>): Promise<R>`: sets the key to `guess`, awaits `remote` and returns its result.
* A rejection restores the previous value and rethrows. A key that held nothing is cleared.
* A success leaves the guess in place.

### seed

* `seed(values: { [key: string]: SfValue }): void`: writes a map in one transaction as a contribution outside any segment, outermost of all. Each call patches it.

### adopt

* `adopt(root?: ParentNode): void`: reads every unread `script[data-sf-store]` under `root` (the document by default) and marks each with `data-sf-adopted`. Idempotent.
* Also reads every list a streamed resolution left on `window.__sfSeed` and installs `window.__sfSeedApply` for later ones.
* Runs when the module loads and again in `boot`. Call it after swapping in a fragment.
* A fragment's script carries `data-sf-fragment`. A contribution in it for a segment the document does not show is held under `<owner region key>><route without query>`. The next fragment of that route in that region replaces it.
* `decodeContributions(encoded: unknown): Contribution[]`: the decoding `adopt` applies.

### reset

* `reset(): void`: forgets every contribution, write and derived value without notifying. Derived keys stay registered.

### snapshot

* `snapshot(): { [key: string]: unknown }`: every key the store holds.

### contribute

* `contribute(list: Contribution[]): void`: replaces each segment's contribution in one transaction. The navigator calls it for every `T` row before patching the DOM.
* A key a contribution's `v` names drops an island's write to it. A key only `d` names keeps it.

### retain

* `retain(segments: Iterable<string>): void`: drops the contribution of every segment not named, notifying the keys that moved.
* A fragment's contribution held under `o` stays while that segment is named.
* The contribution `seed` writes always stays.

### take and restore

* `take(segments: Iterable<string>): Contribution[]`: removes and returns the contributions of the named segments and of the segments inside their regions, in one transaction.
* `restore(list: Contribution[]): void`: contributes them again, except where the store already holds a contribution for that segment.

### setSlotOrder

* `setSlotOrder(list: string[]): void`: sets the slot ranking for parallel slots at one depth, then remerges and notifies every key that moved.
* A listed name comes after every unlisted one and a later name wins. Unlisted names stand in name order.
* `adopt` reads it from the document's `data-sf-slot-order`.

### isPending and whenSettled

* `isPending(k: string): boolean`: whether a contribution's `w` awaits the key and outranks every contribution holding it. A derived key is pending while one of its sources is.
* `whenSettled(keys: string[]): Promise<void>`: resolves once none of `keys` is pending, at once when none is. The same promise is returned for the same set of pending keys.
* `boot` waits on it before mounting an island whose props carry `$aw`.

## 9. The Locale

The document's locale as the application spells it, `fr_FR` or `fr`. The server writes it as `<html lang="fr-FR" data-sf-locale="fr_FR">` and as each payload's `L` row.

### currentLocale

* `currentLocale(): string`: the document's locale. Empty before any document set one.

### subscribeLocale

* `subscribeLocale(listener: (tag: string) => void): () => void`: calls `listener` when the locale changes. Returns the unsubscriber.

### subscribeMessages

* `subscribeMessages(listener: () => void): () => void`: calls `listener` after `setLocale` changes the locale and after `setCatalog`. Returns the unsubscriber.

### setLocale

* `setLocale(tag: string): void`: sets `<html lang>` in BCP 47 spelling and `data-sf-locale` as written, then tells every listener. The same tag again does nothing.

### adoptLocale

* `adoptLocale(): void`: sets the locale from the document element's `data-sf-locale`. Without the attribute the locale stays.

### catalog, setCatalog and adoptCatalog

* `type Catalog = { readonly [key: string]: string }`: a locale's message table, dotted keys to messages.
* `catalog(tag: string): Catalog | null`: the table held for `tag`.
* `setCatalog(tag: string, table: Catalog): void`: holds `table` for `tag`, replacing what was held.
* `adoptCatalog(): void`: reads `<script type="application/json" data-sf-i18n="<tag>">` from the document. A script that does not parse is skipped.
* A navigation sends `x-sf-catalog: <tag>` when a catalog for the current locale is held.

### localePath

* `localePath(to: string, from?: string): string`: the path of the page being shown, with the current locale's prefix replaced by `to`.
* `from` defaults to `currentDocumentPath()`, then `location.pathname` and `location.search`. A given `from` is used as it stands.
* A query is carried. The result always carries a prefix, the default locale's included: `/fr_FR/help` to `en_US` is `/en_US/help`.

### markLinks

* `markLinks(root?: ParentNode): void`: sets or removes `aria-current` on every `<a data-sf-link>` under `root`, the document by default.
* An anchor is judged against `currentAddressPath()`. One carrying `data-sf-current="document"` is judged against `currentDocumentPath()`.
* `enableNavigation` calls it after every navigation and every slot fill.

### currentDocumentPath

* `currentDocumentPath(): string`: the page the document is showing, which an intercept leaves in place while the address bar changes. Empty before `enableNavigation` runs.

### currentAddressPath

* `currentAddressPath(): string`: the path and search the navigator last put in the address bar. Equal to `currentDocumentPath()` unless an intercept is open. Empty before `enableNavigation` runs.

## 10. The React Mounter

`@snapfire/fsr/client/react`. Needs `react` and `react-dom/client` in the import map. A component needs `react/jsx-runtime` too when it is a `.tsx` compiled under `"jsx": "react-jsx"`.

### reactMounter

* `const reactMounter: Mounter`: renders `createElement(component, props, children)` with `hydrateRoot` when `hydrate` is true and `createRoot(el).render` otherwise. Returns the root.
* `children` renders the pieces in this root's tree. A React island among them renders there, inside the context and error boundaries above it.
* A nested React island timed `visible` or `idle` hydrates behind its own Suspense boundary. One the server did not render renders once its module loads.
* An element with no React island under it is kept as the server wrote it and morphed on a patch.
* An island of another framework among the children is left to `scan`.
* A `$h` prop is lifted out before the component sees its props and provided through `withHoisted`.
* An `<sf-s>` that is not an island or children region, outside any nested island, is passed as the `children` element and never reconciled.
* Each `sf-s[data-sf-name]` outside a nested island is passed as a prop of that name.
* With React 19.2 or later, every root renders under `<Activity mode="visible">`.

### reactPatcher

* `const reactPatcher: Patcher`: renders the root again with the new props and the same `children` element. A hydrated root takes the new element through its holder rather than `render`.

### reactUnmounter

* `const reactUnmounter: Unmounter`: unmounts the root. A hydrated root whose hydration has not committed unmounts once it commits.

### reactPauser

* `const reactPauser: Pauser`: renders the root again under `<Activity mode="hidden">` or `"visible"`.
* Showing returns a promise that settles once the root commits or a second has passed.
* Returns `false` without `Activity` and for a hydrated root that has not committed.

### Island and island

* `Island({ when, mode, children }: IslandProps): ReactElement`: places its one child component as an island of its own.
* `interface IslandProps { when?: MountTiming; mode?: "server"; define?: string; children?: ReactNode }`
* `mode: "server"` rides as `data-sf-mode`. `define` names a custom element module imported at the island's timing and cannot be combined with `mode`.
* The build lowers the use to `<sf-s data-sf-island data-sf-region>` around a nested client node. The child is never rendered by this element.
* In the browser it renders that region with `dangerouslySetInnerHTML` and `suppressHydrationWarning`. `scan` mounts the child in its own root.
* A placement claims its region on its first render and keeps it for its life. The build passes the region key as `__sfKey`, which is lifted off the child.
* After each render, the mounted root takes the parent's new props. A placement with no region takes its markup from the current payload. When no payload describes it, it renders its child inline.
* `island<P extends object>(component: ComponentType<P>, options?: { when?: MountTiming; mode?: "server" }): (props: P) => ReactElement`: `component` wrapped in `Island` with `options`. The build reads a module-level `const LazyChart = island(Chart, { when: "visible" })` the same way.

### Slot

* `Slot({ name }: SlotProps): ReactElement`: a named slot of a layout, the region a `slots/<name>/` segment or a `page.<name>.tsx` intercept renders into.
* `interface SlotProps { name: string; children?: ReactNode }`: `children` is the fallback the server renders while nothing fills it.
* The build lowers it to `<sf-s data-sf-name>`. In the browser it adopts the existing region's markup once and `navigate` fills and empties it.
* A layout prop named after a `slots/` directory receives the same region from `reactMounter`.

### Mount

* `Mount({ module, props, when }: MountProps): ReactElement`: places an island by module id, for an island another framework mounts.
* `interface MountProps { module: string; props?: Props; when?: MountTiming }`
* Renders `<sf-s data-sf-island>`, writes a marker and props script into it and scans it. A later render with different `props` patches the island.

### foreign

* `foreign<P extends Props>(module: string): ((props: P) => ReactElement) & { [FOREIGN]: string }`: a React component that renders `Mount` for `module` with its props.
* `const FOREIGN: unique symbol`: `Symbol.for("sf.foreign")`.
* Throws `<module> is mounted by another framework, so it takes no React children` when given `children`.
* `fsr build` replaces an import of another framework's component in a `.tsx` or `.jsx` file with `const Name = foreign("<file>#<export>")`.

### Link

* `Link(props: LinkProps): ReactElement`: an `<a>` with the rest of its props and the attributes the navigator reads.
* `interface LinkProps extends AnchorHTMLAttributes<HTMLAnchorElement> { full?: boolean; into?: string; prefetch?: PrefetchTiming; native?: boolean; keep?: boolean; match?: "exact" | "prefix" | "none"; current?: "url" | "document" }`
* `full` writes `data-sf-full="true"`, `into` writes `data-sf-into`, `prefetch` writes `data-sf-prefetch`, `native` writes `data-sf-native="true"` and `keep` writes `data-sf-keep` as `"true"` or `"false"`.
* Every link carries `data-sf-link`. `match="exact"`, the default, sets `aria-current="page"` on the path its `href` names. `match="prefix"` sets `aria-current="true"` on that path and below it. `match="none"` sets nothing.
* An author-written `aria-current` is left alone. An `href` with a query or a fragment never matches.
* `current="url"`, the default, judges against the address bar. `current="document"` judges against the page under an open intercept and writes `data-sf-current="document"`.
* `match` and `current` must be literals.
* The server writes the mark at first paint. `markLinks` keeps it current.

### Picture

* `Picture(props: PictureProps): ReactElement`: the same markup the server writes for these props.
* `interface PictureProps extends Omit<ImgHTMLAttributes<HTMLImageElement>, "src"> { src: ImageAsset | string; source?: string; priority?: boolean; widths?: number[]; quality?: number | { avif?: number; webp?: number } }`
* `interface ImageAsset { src: string; width: number; height: number; animated?: boolean; sources?: { type: string; srcset: string }[] }`: `sources` is present on the server only.
* An imported image is a `<picture>` with a `<source>` per policy format, each `srcSet` naming `<stem>.<width>.<format>` for every policy width below the image's own and then its own, then the `<img>`.
* `sizes` defaults to `(max-width: {width}px) 100vw, {width}px`. `loading` defaults to `lazy`, `eager` under `priority`. `decoding` defaults to `async`. `priority` also sets `fetchPriority="high"`, lowercase under React 18.
* An SVG, a GIF or an `animated` asset is the `<img>` alone.
* A string `src` with a `source` naming an entry of the policy's `sources` gets a `srcSet` from that template per policy width and `sizes` defaulting to `100vw`. Any other string is `<img src>` as written.
* `widths` replaces the policy's. `quality` does not change the markup.
* The policy is `<meta name="sf:images">`: `widths`, `formats`, `base` prefixed to a root-relative `src` and `sources`. Without the meta, the policy's defaults apply.

### Embed

* `Embed(props: EmbedOptions): ReactElement`: the placeholder for a video hosted elsewhere.
* `interface EmbedOptions { provider: "youtube" | "vimeo"; id: string; title: string; className?: string }`
* The placeholder is a `div` with `data-sf-embed` and `data-sf-embed-title` holding a `button` with the still, the title and a play mark.
* A component the browser alone renders shows YouTube's still from its CDN and no still for Vimeo.
* `enableEmbeds(): void`, from `@snapfire/fsr/client/embed`: replaces a placeholder with its player's iframe when its button is clicked. `boot` calls it once per document.
* `playEmbed(button: Element): void`, from `@snapfire/fsr/client/embed`: that swap for the placeholder `button` sits in.

### Place

* `Place({ name, as, ...attrs }: PlaceProps): ReactElement`: the element that content placed at `#name` is written into, `<div id="name">` unless `as` names another tag.
* `interface PlaceProps { name: string; as?: string; [attribute: string]: unknown }`
* `name` is letters, digits, `-` and `_`.
* It renders empty `dangerouslySetInnerHTML` with `suppressHydrationWarning` and never touches its children.
* Content that arrives before its place carries `data-sf-to` and `scan` moves it in. An island waiting for its place does not mount while a streamed slot is pending.
* An island whose place never arrives has its content dropped and mounts afresh.

### useStore

* `useStore<T>(k: StoreKey<T>, initial: T): [T, (next: T) => void]`: a store key as component state, over `useSyncExternalStore`.
* `initial` is used while nothing has set the key and is captured on the first render.
* The setter writes the store.
* In a root the server did not render, the first render suspends on a pending key.
* `k` must be a string literal or a `key()` the build can follow through an import.

### useLocale

* `useLocale(): string`: the document's locale. Re-renders when a navigation changes it.

### useLinkStatus

* `useLinkStatus(): { pending: boolean }`: whether `pendingLink()` is the enclosing link: the `Link` the component renders inside, else the closest `a` around the island.
* `pending` is false outside a link and in the server snapshot.

### useHoisted

* `useHoisted(module: string): HoistReader`: the reader the build binds at the top of each component it rewrote.
* `interface HoistReader { r<T>(id: number, compute: () => T): T; l<A extends unknown[], R>(f: (...args: A) => R): (...args: A) => R; c(id: number, hit: (html: { __html: string }) => ReactElement, miss: () => ReactElement): ReactElement; k(id: number): string; p(id: number, element: ReactElement): ReactElement; e(index: number): void; x(): void; w<T>(value: T): T; id(id: number): string }`
* `type Hoisted = { readonly [key: string]: unknown }`: the server's hoisted values, keyed `<module>|<id>` or `<module>|<id>@<i>.<j>` inside loops.
* `r` returns the table's value for the key, else `compute()`.
* `c` renders `hit` with the table's markup for the key, else `miss`. With neither it throws, naming the key.
* `id` returns `_S_<key>_`, the string the server wrote for a `useId()` call.

### useBrowser

* `useBrowser(): <T>(compute: () => T) => T | null`: the reader the build binds as `__sfb` for a value only the browser knows.
* Returns `null` until the component has mounted, then the component renders again with `compute()`.

### withHoisted

* `withHoisted(table: Hoisted | null, element: ReactElement): ReactElement`: `element` under `table`. `null` makes every read compute.

### createContext and use

* `createContext` and `use`: React's `createContext` and `useContext`, for a template file an island imports.

## 11. The Vue Mounter

`@snapfire/fsr/client/vue`. Needs `vue` in the import map.

### vueMounter

* `const vueMounter: Mounter`: mounts the module's default export (the module itself when it is the component) with `createSSRApp` when `hydrate` is true and `createApp` otherwise. Returns the app.
* `$h`, `$k` and `$s` are lifted off the props.
* `children` is the component's default slot. A Vue island among them renders there, inside the `provide` and `onErrorCaptured` above it.
* A nested Vue island timed `visible` or `idle` hydrates through `hydrateOnVisible` or `hydrateOnIdle`.
* An element with no Vue island under it is kept as the server wrote it. An island of another framework is left to `scan`.
* A `<template data-sf-children>` region is removed once read.

### vuePatcher

* `const vuePatcher: Patcher`: assigns the new props into the reactive props it holds for `el`, deleting keys the new props lack.
* New children markup from `islandState` renders in the slot.
* Does nothing for an element nothing mounted.

### vueUnmounter

* `const vueUnmounter: Unmounter`: unmounts the app and forgets the props and children held for `el`.

### Mount (Vue)

* `const Mount: Component`: props `module`, `props` and `when`. The Vue counterpart of React's `Mount`.

### Link, Picture, Embed and Place (Vue)

* `const Link: Component`, `const Picture: Component`, `const Embed: Component`, `const Place: Component`: Vue components with the props and markup of React's `Link`, `Picture`, `Embed` and `Place`.

### useStore (Vue)

* `useStore<T>(key: StoreKey<T>, initial: T): { value: T }`: a store key as a Vue ref. Writing `.value` writes the store.
* Unsubscribes when the current scope is disposed. Call it in `setup`.

### useLocale (Vue)

* `useLocale(): Readonly<Ref<string>>`: the document's locale as a ref. Call it in `setup`.

### useLinkStatus (Vue)

* `useLinkStatus(): { readonly pending: boolean }`: React's `useLinkStatus` as a reactive object, judged against the Vue `Link` around the component or the closest `a` around the island. Call it in `setup`.

## 12. The Svelte Mounter

`@snapfire/fsr/client/svelte`. Needs `svelte` and `svelte/reactivity` in the import map. Svelte 5.5 or later.

### svelteMounter

* `const svelteMounter: Mounter`: mounts the module's default export (the module itself when it is the component) with Svelte's `mount`. With `hydrate` true it uses Svelte's `hydrate`. Returns what that call returned.
* `$h`, `$k`, `$s` and `$sv` are lifted off the props.
* Props are read through `createSubscriber`. A patch re-renders only what reads a changed prop.
* `children` reaches the component as its `children` snippet, the one `registerTree` was given.
* A nested Svelte island the server rendered at `load` hydrates in this tree. One timed `visible` or `idle` hydrates later as a root of its own with the tree's contexts.
* An element with no such island under it is kept, morphed on a patch and scanned for other frameworks' islands.

### sveltePatcher

* `const sveltePatcher: Patcher`: replaces the props and notifies every read. New children markup renders in place.

### svelteUnmounter

* `const svelteUnmounter: Unmounter`: calls Svelte's `unmount` and forgets the props and children held for `el`.

### TreePiece

One item of an island's children as `Pieces.svelte` renders it.

* `type TreePiece = TreeIsland | TreeElement | TreeOpaque | TreeHtml`
* `interface TreeIsland { kind: "island"; id: string; moduleId: string; region: string | null; when: string | null; mode: string | null; ssr: boolean; props: Props; children: TreePiece[] | null; entry: IslandEntry | undefined; timing: MountTiming }`: branch `[0` of the server's markers.
* `interface TreeElement { kind: "element"; tag: string; attrs: { [name: string]: string }; children: TreePiece[] }`: branch `[1`.
* `interface TreeOpaque { kind: "opaque"; tag: string; attrs: { [name: string]: string }; html: string }`: branch `[2`.
* `interface TreeHtml { kind: "html"; html: string }`: branch `[-1`.

### registerTree

* `registerTree(tree: Snippet): void`: takes the snippet `generated/components/Pieces.svelte` exports. The island registry `fsr build` writes calls it.

### held, opaque and nested

Called by `Pieces.svelte` and `Island.svelte` under `generated/components`.

* `held(given: () => TreePiece[] | undefined): () => TreePiece[]`: the pieces given, else the children of the enclosing island.
* `opaque(): (piece: TreeOpaque) => (el: Element) => () => void`: the attachment on a kept element. It writes, morphs and scans its content and discards its islands when the element leaves.
* `nested(piece: () => TreeIsland): { readonly component: Component<{ [key: string]: unknown }> | null; readonly props: { [key: string]: unknown }; attach: (el: Element) => () => void }`: a nested island's component and props, loaded at its timing.

### useStore (Svelte)

* `useStore<T>(key: StoreKey<T>, initial: T): { subscribe: (run: (value: T) => void) => () => void; set: (value: T) => void; update: (fn: (value: T) => T) => void }`: a store key as a Svelte store, `$count` to read and `$count = 3` to write.

### useLocale (Svelte)

* `useLocale(): { subscribe: (run: (value: string) => void) => () => void }`: the locale as a readable store.

### useLinkStatus and holdLink (Svelte)

* `useLinkStatus(): { subscribe: (run: (value: { pending: boolean }) => void) => () => void }`: a readable store of `{ pending }`. Call it while the component initialises.
* `holdLink(anchor: () => Element | null): void`: puts the anchor `useLinkStatus` judges against in context. FSR's `Link.svelte` calls it.

### link, picture and embed (Svelte)

* `link(options: LinkOptions): Record<string, unknown>`: the anchor attributes of `Link`, for `<a {...link({ href })}>`.
* `picture(options: PictureOptions): { img: Record<string, unknown>; sources: Record<string, unknown>[] | null }`: the `<img>` attributes and one set per `<source>`, `sources` `null` when the policy derives no variants.
* `embed(options: EmbedOptions): { outer: Record<string, string>; button: Record<string, string>; still: Record<string, string> | null; title: Record<string, string>; play: Record<string, string> }`: the attributes of the placeholder's parts.

### island (Svelte)

* `island(module: string, props?: Props, when?: MountTiming): (element: Element) => () => void`: an attachment placing an island by module id inside the element, `<div {@attach island("src/ui/Chart.tsx#default", { label })}></div>`.
* A props change patches the island. Removing the element discards it.

### mount (Svelte)

* `mount(module: string, props?: Props, when?: MountTiming): (region: Element) => () => void`: the attachment the Svelte `Mount` puts on its `<sf-s data-sf-island>` region, with the same behaviour as `island`.

## 13. The JSX Element Builder

`@snapfire/fsr/client/jsx-runtime`, what TSX compiles against in an application without React. It builds element descriptions and mounts nothing.

### JSX

* `namespace JSX { type Element = FsrElement; type ElementType = string | FsrComponent; interface IntrinsicElements { [tag: string]: Record<string, unknown> } }`

### jsx and jsxs

* `jsx(type: FsrElement["type"], props: Record<string, unknown> | null, key?: unknown): FsrElement`
* `const jsxs: typeof jsx`
* `const Fragment: unique symbol`: `Symbol.for("sf.fragment")`.
* `interface FsrElement { type: string | typeof Fragment | FsrComponent; props: Record<string, unknown>; key?: unknown }`
* `type FsrComponent = (props: Record<string, unknown>) => FsrNode`
* `type FsrNode = FsrElement | string | number | bigint | boolean | null | undefined | FsrNode[]`

### Placements

* `Link(props: LinkOptions & { children?: FsrNode }): FsrElement`: an anchor with the attributes of React's `Link`.
* `Picture(props: PictureOptions): FsrElement`: the `<picture>` or `<img>` of React's `Picture`.
* `Place({ name, as, ...attrs }: { name: string; as?: string; [attribute: string]: unknown }): FsrElement`: `<div id="name">` unless `as` names another tag.
* `Mount({ when }: { module: string; props?: Record<string, unknown>; when?: "load" | "visible" | "idle" }): FsrElement`: an empty `<sf-s data-sf-island>`.
* `Embed(props: EmbedOptions): FsrElement`: the video placeholder.
* `Island(props: { children?: FsrNode }): FsrElement` and `Slot(props: { children?: FsrNode }): FsrElement`: a fragment around their children.
* `island<P extends Record<string, unknown>>(component: (props: P) => FsrNode): (props: P) => FsrNode`: returns `component` unchanged.
* `createContext<T>(defaultValue: T): FsrContext<T>` and `use<T>(context: FsrContext<T>): T`: `use` always returns the default.
* `interface FsrContext<T> { (props: { value: T; children?: FsrNode }): FsrElement; readonly __sfContext: T; Provider: (props: { value: T; children?: FsrNode }) => FsrElement }`

## 14. Custom Elements and htmx

`@snapfire/fsr/client/elements` and `@snapfire/fsr/client/htmx`. Neither imports anything outside the package.

### shadowOf

* `shadowOf(element: HTMLElement, internals?: ElementInternals): ShadowRoot | null`: the shadow root of a custom element whose template the server wrote.
* Returns the root the parser attached. A closed root is reached through `internals`.
* With no root and a `<template shadowrootmode>` child, it attaches a root with that mode and with `delegatesFocus`, `clonable` and `serializable` from the `shadowrootdelegatesfocus`, `shadowrootclonable` and `shadowrootserializable` attributes. The template's content is cloned in and the template removed.
* `null` when the element has neither. Also `null` for a closed root without `internals`.

### HtmxProcessor

* `interface HtmxProcessor { process(element: Element): void }`: the method `bindHtmx` calls. htmx's default export satisfies it.

### bindHtmx

* `bindHtmx(htmx: HtmxProcessor): () => void`: wires htmx and the client to each other's markup. Returns the function that removes the listeners.
* On `htmx:after:settle` (htmx 4) and `htmx:afterSettle` (htmx 2) on `document.body`: `adopt()` then `scan(document)`.
* Once at binding and on every `sf:navigate` and `sf:fill` on `document`: `htmx.process(document.body)`.
* On `htmx:before:history:restore` on `document`, while `enableNavigation` has wired the document: `preventDefault()`.

## 15. The Standard Library

Members the server runs under the same names. A member that runs in the browser gives the same answer as the server under the same locale, `currentLocale()`.

| Module | Members | From |
| --- | --- | --- |
| `intl`, `text`, `time`, `crypto`, `i18n`, `path`, `log` and `t` | the browser members | `@snapfire/fsr/client` and `@snapfire/fsr/client/<module>` |
| the same plus `file` and `id`, with the host-only members | everything a host body runs | `@snapfire/fsr/host` |
| `system`, `clipboard` and `Scope` | what a terminal program runs | `@snapfire/fsr/terminal` |
| `native` | | `@snapfire/fsr` |

* A host-only member called in a browser throws `<name> runs in a host only`. A terminal-only member throws `<name> runs in a terminal page only`.

### intl

* `intl.localeTag(): string`: `currentLocale()` as BCP 47, `fr-FR` for `fr_FR`, `en` before any document sets one.
* The build lowers the platform's `Intl` formatters, `toLocaleString`, `toLocaleDateString` and `localeCompare`. The server answers them as the browser does.

### text

* `text.slug(s: string): string`: NFD, marks dropped, lowercased, every run outside `a-z0-9` one hyphen, none at either end.
* `text.truncate(s: string, max: number, ellipsis?: string): string`: the first `max` code points and `ellipsis`, `…` by default, when `s` is longer.
* `text.frontMatter<T>(source: string, options?: FrontMatterOptions): { data: T; content: string }`: the YAML block between two delimiter lines at the top of `source` as `data` and the rest as `content`, as gray-matter reads them. Host only.
* `text.withFrontMatter(content: string, data: { [key: string]: unknown }, options?: FrontMatterOptions): string`: `data` as YAML between the delimiter lines, then `content` ending in a line break, as gray-matter's `stringify` writes them. Empty `data` writes no block. Host only.
* `interface FrontMatterOptions { delimiters?: string | [string, string] }`: one string for both lines or `[open, close]`, `---` by default.

### time

Instants are milliseconds since the epoch. Every calendar field is UTC.

* `time.format(when: number, pattern: string): string`: `YYYY`, `MM`, `DD`, `HH`, `mm`, `ss` and `SSS` replaced, every other character kept.
* `time.add(when: number, amount: number, unit: string): number` and `time.diff(later: number, earlier: number, unit: string): number`: `unit` is `ms`, `s`, `m`, `h` or `d`. Another unit throws. `diff` is fractional.
* `time.parse(s: string): number | null`: `YYYY-MM-DD`, optionally `THH:MM`, `:SS`, `.fff` and `Z` or `±HH:MM`. A date alone or a time without a zone is UTC. `null` for anything else.

### crypto

* `crypto.hash(s: string): string`: SHA-256 of the UTF-8 bytes as lowercase hex, synchronous.
* `crypto.verify(s: string, hash: string): boolean`: constant time over the hash's length, case insensitive.
* `crypto.digest(algorithm: string, encoding: string, ...parts: string[]): string`: `sha256` of the parts' UTF-8 bytes in order, as `hex` or `base64`. Another algorithm or encoding throws.
* `crypto.random(bytes: number, encoding?: "hex" | "base64"): string`: at most 1024 bytes from `getRandomValues`, `hex` by default. Host only.

### path

POSIX paths as text.

* `path.join(...parts: string[]): string`: the parts joined by `/` and normalized, `.` for none.
* `path.normalize(p: string): string`: `.` and empty segments dropped, `..` resolved against what precedes it.
* `path.dirname(p: string): string`, `path.basename(p: string, ext?: string): string`, `path.extname(p: string): string` and `path.isAbsolute(p: string): boolean`.
* `path.resolve(...parts: string[]): string`: the parts right to left until one is absolute, the working directory before them otherwise. Host only.
* `path.relative(from: string, to: string): string`: the path from `from` to `to`, both resolved. Host only.

### log

* `log.trace`, `log.debug`, `log.info`, `log.warn`, `log.error` and `log.fatal`: `(message: string, fields?: unknown) => null`.
* On the server each is a `tracing` event under `fsr::app`, `fatal` at error level. In the browser each is a console line, `trace` and `debug` through `console.debug` and `fatal` through `console.error`.

### i18n and t

* `t(key: string, args?: { [name: string]: unknown }): string`: the message under `key` in `catalog(currentLocale())`. Also `i18n.t`.
* With `args.count` a number or bigint, it tries `key.<plural category>`, then `key.other`, then `key`.
* `{name}` is replaced by `String(args[name])` for a scalar argument and left as written otherwise.
* Returns the key itself when no form is found or no table is held.

### file

The files a host body reads and writes: the directories `[files] roots` names on a web host and the machine a terminal program runs on. Host only. A failure throws an `Error` whose `code` is the POSIX name, `ENOENT` for a missing file.

* `file.read(path: string): string`: the file as text, bytes that are not UTF-8 as U+FFFD.
* `file.write(path: string, data: unknown): void`: `data` as the file's text, a value that is not text as JSON.
* `file.exists(path: string): boolean`
* `file.list(path: string): string[]`: a directory's entry names, sorted.
* `file.makeDir(path: string, options?: { recursive?: boolean }): void`
* `file.stat(path: string): FileStat`
* `interface FileStat { isFile: boolean; isDirectory: boolean; size: number; mtimeMs: number }`

### id

* `id.new(): string`: a UUID, version 7 on the server. Host only.

### system

The machine a terminal program runs on. Terminal only.

* `system.env(name: string): string | null`: the environment variable, `null` when unset.
* `system.home(): string`, `system.tempDir(): string` (no trailing `/`) and `system.cwd(): string`.
* `system.platform(): string`: `macos`, `linux`, `windows` or another name Rust gives the operating system.
* `system.lineEnd(): string`: `"\r\n"` on Windows, `"\n"` elsewhere.
* `system.memory(): number`: the resident set of the running page, in bytes.
* `system.spawn(command: string, args?: string[], options?: SpawnOptions): Spawned`: a process with its output piped.
* `interface SpawnOptions { cwd?: string; env?: { [name: string]: string }; shell?: boolean | string; stdio?: "inherit" | "pipe" }`: `stdio: "inherit"` for a process that owns the terminal.
* `interface Spawned { stdin: SpawnedInput; stdout: SpawnedOutput; stderr: SpawnedOutput; on(event: "exit" | "close", f: (code: number | null, signal: string | null) => void): void; on(event: "error", f: (error: Error) => void): void; off(event: "exit" | "close" | "error", f: (...args: never[]) => void): void }`
* `interface SpawnedOutput { on(event: "data", f: (chunk: string) => void): void; setEncoding(encoding: string): void }`: output arrives as text and `setEncoding` changes nothing.
* `interface SpawnedInput { write(text: string): void; end(): void }`: standard input is a pipe only when the page writes it.

### clipboard

The clipboard of a terminal program. Terminal only.

* `clipboard.write(text: string): void`: through OSC 52 when the terminal names it in its device attributes, over SSH too. Otherwise through `pbcopy`, `clip`, `wl-copy`, `xclip` or `xsel`, the first the platform has.
* `clipboard.read(): string`: the clipboard of the machine the program runs on, through the matching paste command. It never reads through OSC 52.

### Scope

* `class Scope<T> { run<R>(value: T, fn: (...args: never[]) => R, ...args: never[]): R; get(): T | undefined; set(value: T): void }`: a value scoped to a call.
* `run` holds `value` while `fn` runs. `get` returns `undefined` outside any `run`. `set` holds a value from then on.
* A terminal program keeps the value across the awaits inside `fn`.

### native

* `native<F extends (...args: never[]) => unknown>(name: string, f?: F): F`: declares the browser half of a native pair named `module.member`, whose Rust half the host registers under the same name.
* With `f`, the pair has `render` reach. `f` is returned and registered on `globalThis.__sf_natives`.
* Without `f`, the pair has `body` reach. The returned function throws `<name> runs on the server only`.
* `name` must be a string literal.

## 16. Testing

`@snapfire/fsr/client/testing`: what a page spec imports under `fsr test`, which runs it in QuickJS over linkedom. Every call that acts on the page returns a promise that settles the engine before it resolves. Also exports `f64`.

### test and it

* `test(name: string, body?: TestBody, timeout?: number): void`: registers a test. `it` is the same function.
* `test.only`, `test.skip`, `test.todo(name: string)`, `test.each`, `test.skipIf(condition)`, `test.runIf(condition)`, `test.fails` and `test.concurrent`. `xit` and `xtest` are `test.skip`. `fit` is `test.only`.
* `type TestBody = (done: DoneCallback) => unknown`
* `interface DoneCallback { (error?: unknown): void; fail(error?: unknown): void }`
* A body may be async or take `done` and call it.
* `each` takes an array of rows or a template table. An array row is spread over the body's parameters and any other row is one argument. A template table's first row names the columns and its rows reach the body as objects.
* The name takes `%s`, `%d`, `%i`, `%f`, `%j`, `%o` and `%p` for the arguments in order, `%#` for the row's index, `%$` for its number and `$field` for a field of an object row.
* An `only` anywhere in the file skips every test it does not cover.
* A test with no body is a todo. `fails` passes when its body throws.
* `concurrent` runs in order like any other test. `timeout` is ignored.

### describe

* `describe(name: string, body: () => void): void`: groups tests. With `only`, `skip`, `each`, `skipIf`, `runIf` and `concurrent`. `xdescribe` and `fdescribe`.
* The body runs at once and throws when it returns a promise.
* A test's reported name is the names of its enclosing blocks and its own, joined with ` > `.

### Hooks

* `beforeAll`, `afterAll`, `beforeEach` and `afterEach`: `(body: TestBody, timeout?: number) => void`, scoped to the enclosing `describe` or the file.
* `beforeAll` runs before the first test of its block that runs. A `beforeAll` that fails fails every test in its block.
* `afterAll` runs after the file's last test, for every block a test ran in, innermost first. A failure is reported as a test named `afterAll`.
* `beforeEach` hooks run outermost first. `afterEach` hooks run innermost first, after a failure too.

### expect

* `expect(received: unknown, message?: string): Assertion`
* `interface Assertion extends Matchers<void> { not: Matchers<void>; resolves: Matchers<Promise<void>> & { not: Matchers<Promise<void>> }; rejects: Matchers<Promise<void>> & { not: Matchers<Promise<void>> } }`
* `expect.assertions(count: number)`, `expect.hasAssertions()`, `expect.extend(matchers: Record<string, MatcherFunction>)` and `expect.unreachable(message?: string): never`.
* `type MatcherFunction = (this: MatcherState, received: any, ...expected: any[]) => MatcherResult | Promise<MatcherResult>`
* `interface MatcherResult { pass: boolean; message: () => string }`
* `message` leads the report when the expectation fails.
* `.resolves` and `.rejects` take a promise or a function returning one and return a promise to await. A promise that settled the other way fails with what it settled to.
* `assertions` and `hasAssertions` are checked when the test's body finishes.

### Matchers

* Values: `toBe` (`Object.is`, with a bigint equal to the whole number it stands for), `toEqual` (deep, `undefined` properties counted absent, the same bigint allowance), `toStrictEqual` (deep, `undefined` properties and prototypes counted, no allowance), `toBeTruthy`, `toBeFalsy`, `toBeNull`, `toBeUndefined`, `toBeDefined`, `toBeNaN`, `toBeGreaterThan`, `toBeGreaterThanOrEqual`, `toBeLessThan`, `toBeLessThanOrEqual`, `toBeCloseTo(n, digits = 2)`, `toContain`, `toContainEqual`, `toHaveLength`, `toHaveProperty(path, value?)`, `toMatch(string | RegExp)`, `toMatchObject`, `toThrow`, `toThrowError`, `toBeInstanceOf`, `toBeTypeOf`, `toSatisfy` and `toBeOneOf`.
* Mock functions: `toHaveBeenCalled`, `toHaveBeenCalledOnce`, `toHaveBeenCalledTimes`, `toHaveBeenCalledWith`, `toHaveBeenCalledExactlyOnceWith`, `toHaveBeenLastCalledWith`, `toHaveBeenNthCalledWith`, `toHaveReturned`, `toHaveReturnedTimes`, `toHaveReturnedWith`, `toHaveLastReturnedWith` and `toHaveNthReturnedWith`. Aliases: `toBeCalled`, `toBeCalledTimes`, `toBeCalledWith`, `lastCalledWith`, `nthCalledWith`, `toReturn`, `toReturnTimes`, `toReturnWith`, `lastReturnedWith` and `nthReturnedWith`.
* Markup: `toBeInTheDocument`, `toHaveTextContent(string | RegExp, { normalizeWhitespace }?)`, `toHaveAttribute(name, value?)`, `toHaveClass(...names, { exact }?)`, `toBeVisible`, `toBeDisabled`, `toBeEnabled`, `toBeRequired`, `toBeInvalid`, `toBeValid`, `toBeChecked`, `toBePartiallyChecked`, `toHaveValue`, `toHaveDisplayValue`, `toHaveFocus`, `toBeEmptyDOMElement`, `toContainElement`, `toContainHTML`, `toHaveStyle`, `toHaveFormValues`, `toHaveAccessibleName`, `toHaveAccessibleDescription` and `toHaveRole`.
* `toMatchSnapshot`, `toMatchInlineSnapshot`, `toThrowErrorMatchingSnapshot` and `toThrowErrorMatchingInlineSnapshot` fail, saying `fsr test` keeps no snapshot files.
* `toThrow` reads what was thrown as its kind followed by its message. A string must appear in that text, a RegExp must match it, a class must be what was thrown and an object must match it.
* `toHaveTextContent` with a string passes when the whitespace-normalised text contains it.
* A call matcher refuses a value that is not a mock function. A markup matcher refuses a value that is not an element.
* Visibility and style matchers read attributes and inline styles only. `toBeVisible` fails on a `hidden` attribute, an inline `display: none`, `visibility: hidden` or `opacity: 0` on the element or an ancestor and the inside of a closed `<details>`.

### Asymmetric Matchers

* `expect.any(type)`, `expect.anything()`, `expect.objectContaining(object)`, `expect.arrayContaining(array)`, `expect.stringContaining(string)`, `expect.stringMatching(string | RegExp)` and `expect.closeTo(n, digits?)`.
* `expect.not.objectContaining`, `expect.not.arrayContaining`, `expect.not.stringContaining` and `expect.not.stringMatching`.
* `interface AsymmetricMatcher { $$typeof: symbol; asymmetricMatch(other: unknown): boolean; toString(): string }`
* `any(String)`, `any(Number)`, `any(Boolean)`, `any(BigInt)`, `any(Symbol)`, `any(Function)` and `any(Object)` match those kinds of value. Any other constructor matches its instances.
* `anything` matches anything but `null` and `undefined`.

### Mock Functions

* `fn<A extends unknown[], R>(impl?: (...args: A) => R): MockInstance<A, R>`
* `spyOn<T extends object, K extends keyof T>(object: T, key: K, access?: "get" | "set"): MockInstance`: replaces the member with a mock that calls the original until told otherwise.
* `isMockFunction(value: unknown): value is MockInstance`, `clearAllMocks()`, `resetAllMocks()` and `restoreAllMocks()`.
* `const vi: Vi` and `const jest: Vi`, the same object: `fn`, `spyOn`, `isMockFunction`, `mocked`, `clearAllMocks`, `resetAllMocks`, `restoreAllMocks`, `useFakeTimers`, `useRealTimers`, `isFakeTimers`, `advanceTimersByTime(ms): Promise<void>`, `advanceTimersByTimeAsync(ms): Promise<void>` and `waitFor`.
* A `MockInstance` records calls in `mock.calls`, `mock.results`, `mock.instances`, `mock.contexts` and `mock.invocationCallOrder`, with `mock.lastCall`.
* `mockImplementation`, `mockReturnValue`, `mockResolvedValue` and `mockRejectedValue` set what it answers. Their `Once` forms answer the next call only. `mockReturnThis`, `mockName`, `getMockName` and `getMockImplementation` are also there.
* `mockClear` forgets the calls. `mockReset` also forgets the answers. `mockRestore` puts back a spy's original.
* A mock answering a `ctx` service method through `mockResolvedValue` or `mockRejectedValue` settles at once.
* Timers are always fake. `useFakeTimers` and `useRealTimers` change nothing and `isFakeTimers` returns true.

### ctx

* `ctx(mock?: Mock): TestCtx`: the request an action runs under when a rendered page calls it or a route loads.
* `interface Mock<Input = unknown> { session?: Record<string, unknown>; services?: Record<string, Record<string, Method>>; native?: Record<string, Record<string, Method>>; input?: Input; params?: Record<string, string>; query?: Record<string, string>; identity?: { subject: string; claims?: Record<string, unknown> }; locale?: string; path?: string }`
* `interface TestCtx { readonly id: number; readonly locale: string; readonly session: Record<string, unknown>; readonly trace: { calls: ServiceCall[] } }`
* `interface ServiceCall { service: string; method: string; args: Record<string, unknown> }`
* A service method is a mock function or a function of its arguments that answers synchronously.
* `native` answers the application's own Rust modules.
* `locale` defaults to the host's default. `path` is the path the request matched.
* `session` and `trace` are current after every call.

### render and renderHook

* `render(element: Placed, options?: { ctx?: TestCtx; hydrate?: boolean }): Promise<Rendered>`: places `element` in the document and mounts it.
* `type Placed = ReactElement | { type: unknown; props: Record<string, unknown> }`
* `interface Rendered extends BoundQueries { container: HTMLElement; baseElement: HTMLElement; root: Root; hydrated: string | null; composed: string | null; unmount(): void; rerender(element: Placed): Promise<void>; asFragment(): DocumentFragment; debug(element?: Element, maxLength?: number): void }`
* A registered island of any framework, a `foreign` component's included, is placed as a page places it and mounted through its adapter. When the module lowers, the server's markup is written first and the adapter hydrates over it. A mismatch fails the test. `rerender` patches the island and `root` throws.
* A React component no registry knows mounts in a React root. So does a registered one whose props cannot be encoded. It hydrates when the build lowered it.
* `hydrate: false` mounts fresh.
* Any other component is refused.
* A page or layout the build marked `static` is written as the server's markup, its islands are mounted and it is named in `composed`. `rerender` writes the markup again.
* `hydrated` names the module that hydrated.
* Markup is written with `setHTMLUnsafe` where the DOM has it.
* `registerComposition(moduleId: string, loader: () => Promise<unknown>): void` and `registerLowered(moduleId: string, loader: () => Promise<unknown>): void`: called by the spec runner for every composition route module a spec imports and every other lowered component.
* `renderHook(hook, options?: { initialProps?; ctx?; wrapper? }): Promise<{ result: { current: Result }; rerender(props?): Promise<void>; unmount(): void }>`
* `act<T>(body: () => T | Promise<T>): Promise<T>`: runs `body` and settles.
* `cleanup(): void`: discards every island in the body and empties it. The runner calls it after every test.

### load

* `load(path: string, options?: { ctx?: TestCtx }): Promise<{ status: number; path: string }>`: loads the document the stock host renders for `path` and runs it as a browser would.
* Follows up to five redirects.
* Ends the islands of the current page, installs the new document, evaluates the application's entry module and island modules again, mounts its islands and enables navigation.
* A custom element the application defines again replaces its earlier definition. One a package defined keeps its definition.
* Throws when the response is not a document.

### Queries

* Kinds `Role`, `Text`, `LabelText`, `PlaceholderText`, `AltText`, `Title`, `DisplayValue` and `TestId`, each as `getBy`, `getAllBy`, `queryBy`, `queryAllBy`, `findBy` and `findAllBy`.
* `type Matcher = string | RegExp | ((content: string, element: Element | null) => boolean)`
* `interface MatcherOptions { exact?: boolean; normalizer?: (text: string) => string; trim?: boolean; collapseWhitespace?: boolean }`. `SelectorMatcherOptions` adds `selector` and `ignore`.
* `interface ByRoleOptions { name?: Matcher; description?: Matcher; hidden?: boolean; level?: number; checked?: boolean; selected?: boolean; pressed?: boolean; expanded?: boolean; current?: boolean | string; busy?: boolean; queryFallbacks?: boolean }`
* `configure(next: { testIdAttribute?: string; asyncUtilTimeout?: number }): void`
* `getBy` returns the one match and throws `TestingLibraryElementError` for none or several. `getAllBy` returns every match and throws for none. `queryBy` returns the match or `null` and throws for several. `queryAllBy` returns every match. `findBy` and `findAllBy` are `getBy` and `getAllBy` under `waitFor`.
* Text is whitespace-normalised. A string must equal it. With `exact: false` it need only appear in it, in any case.
* `ByText` reads an element's own text nodes and skips `script` and `style`.
* `ByLabelText` finds the control a label names by `for` or contains. It also finds an element whose `aria-label` or `aria-labelledby` text matches.
* `ByRole` matches an element's first `role` token or the role its tag implies. It skips what `hidden`, `aria-hidden` or an inline `display: none` hides unless `hidden` is set.
* `name` is matched against the accessible name: `aria-labelledby`, `aria-label`, a control's labels, alt text, the content of a role named from content, then `title`.
* A role query that finds nothing lists every role present with its name.
* A node as the second argument searches under it.

### screen and within

* `const screen: Screen`: every query over the document's body when it runs, plus `debug(element?, maxLength?)` and `logTestingPlaygroundURL()`.
* `within(container: ParentNode): BoundQueries`
* `prettyDOM(node?: Element | Document | null, maxLength = 7000): string`, `logRoles(container?: ParentNode): void` and `getDefaultNormalizer(options?: { trim?: boolean; collapseWhitespace?: boolean }): (text: string) => string`.

### waitFor

* `waitFor<T>(callback: () => T | Promise<T>, options?: WaitForOptions): Promise<T>`: retries `callback` after settling, moving the harness clock by `interval` between tries, until `timeout` has passed on that clock.
* `WaitForOptions` holds `timeout` (1000 by default), `interval` (50 by default) and `onTimeout?: (error: Error) => Error`.
* Throws the callback's last error at the timeout.
* `waitForElementToBeRemoved(target: Element | Element[] | null | (() => Element | Element[] | null), options?: WaitForOptions): Promise<void>`: refuses a target that is not in the document to begin with.

### fireEvent

* `fireEvent(node: Element | Document | Window, event: Event): Promise<boolean>`: dispatches one event and settles. Resolves false when a listener cancelled it.
* `fireEvent.<name>(node, init?: EventInit | string): Promise<boolean>` for `click`, `dblClick` and the mouse, pointer, key, focus, form, touch, drag, clipboard, wheel, scroll, load, error, animation and transition events.
* `createEvent(name: string, node, init?: EventInit): Event`
* `type EventInit = Record<string, unknown> & { target?: Record<string, unknown> }`: `target` properties are set on the element first, `value` through the setter typing reaches.
* `change` also takes the new value as a string and dispatches `input` before `change`. `keyDown`, `keyUp` and `keyPress` take a key as a string.
* A click runs the browser's default action. A checkbox toggles and a radio checks before listeners see it, undone when a listener cancels and followed by `input` and `change` when none does. A label clicks its control. A submit button submits its form and a reset button resets it.
* `mouseEnter` and `mouseLeave` also dispatch `mouseover` and `mouseout`. The pointer pair does likewise. `focus` and `blur` also dispatch `focusin` and `focusout`.

### userEvent

* `userEvent.setup(options?: UserOptions): UserEvent`: a session with its own held modifier keys and hover. Every `UserEvent` method is on `userEvent` too.
* `UserOptions` takes `skipHover`. `delay` and `advanceTimers` are accepted and ignored.
* `interface UserEvent { click; dblClick; tripleClick; hover; unhover; tab(options?: { shift?: boolean }); type(element, text, options?: { skipClick?: boolean }); keyboard(text); clear(element); selectOptions(element, values); deselectOptions(element, values); upload(element, files); paste(text) }`, each returning `Promise<void>`.
* `click` hovers, presses, releases, moves focus to the nearest focusable element and clicks with the default actions of `fireEvent.click`. A disabled control gets the pointer events and nothing else.
* `type` clicks the element (focuses it under `skipClick`) and then types.
* `keyboard` takes key descriptors: a character is itself, `{Enter}` a key by name, `[KeyA]` a key by code, `{Shift>}` holds a key until `{/Shift}` and `{{` or `[[` a literal bracket.
* A character types into a focused text control through `keydown`, `keypress`, the value setter and `input`, honouring `maxlength`. Backspace deletes.
* Enter in a text input submits its form, through its submit button when it has one or directly when it has a single text field. Enter on a button or a link clicks it. Space clicks a focused button, checkbox or radio.
* Tab moves focus through the tabbable elements, positive `tabindex` first.
* `clear` empties an editable element.
* `selectOptions` chooses `<select>` options by value, text or element. On another element it clicks the named `role=option` elements.
* `upload` sets an input's `files`.

### settle and advance

* `settle(): Promise<void>`: runs everything due now: microtasks, action calls, their re-renders and timers already due.
* `advance(ms: number): Promise<void>`: moves the clock `ms` forward and settles. Timers due by then fire in order.
* Time never passes on its own.
* `show(value: unknown): string`: a value as the report prints it. `1n` and `1` stay distinct and strings are quoted.

### assert

* `assert.ok(value, message?)`, `assert.equal(actual, expected, message?)`, `assert.match(actual, pattern, message?)`, `assert.throws(run, match?)` and `assert.rejects(run, match?)`.
* `equal` is deep with the bigint allowance. `match` takes a string to contain or a RegExp. `throws` and `rejects` match a string against the kind and message of what was thrown.

## 17. The Desktop Bridge

`@snapfire/fsr/desktop`: what a page can ask of the window `fsr dev --desktop` or `fsr serve --desktop` opened. The host maps it when the bundle imports it.

### openFile and openExternal

* `openFile(options?: OpenFileOptions): Promise<string[] | null>`: the system's open dialog. Resolves to the chosen absolute paths, `null` when the user cancels.
* `openExternal(url: string): Promise<void>`: opens the link in the system's browser or mail app. Only `http:`, `https:` and `mailto:` links with something after the scheme. Anything else rejects.
* Both reject with `<name> runs only in an fsr --desktop window` when `window.__sfDesktop` is absent.
* Each call posts `{ id, method, args }` over the window's IPC channel and settles through `window.__sfDesktop.settle(id, ok, value)`. A refusal rejects with an `Error` carrying the window's message.

### FileFilter and OpenFileOptions

* `interface FileFilter { name: string; extensions: string[] }`: a label and its extensions without the dot.
* `interface OpenFileOptions { multiple?: boolean; filters?: FileFilter[] }`: `multiple` defaults to false.

## 18. Error Handling

### ActionFailure

Thrown by an action callable and `upload` when the server answers with a non-ok status.

* `class ActionFailure extends Error { readonly kind: string; constructor(kind: string, message: string) }`, `name` `"ActionFailure"`.
* From a JSON body with a string `kind`: that `kind` and the body's `message`, else `statusText`.
* From any other body: the status's own kind, `invalid` for any other 4xx and `internal` for anything else. The message is the body's text, else `statusText`, else `HTTP <status>`.
* Each 4xx and 5xx status has a kind named after it, such as `unauthorized` (401), `forbidden` (403), `request_timeout` (408), `too_many_requests` (429) and `bad_gateway` (502). Three are not: `invalid` (400), `unavailable` (503) and `timeout` (504).

### AuthFailure

Thrown by `signIn`, `enrol` and `answer`.

* `class AuthFailure extends Error { readonly kind: string; constructor(kind: string, message: string) }`, `name` `"AuthFailure"`.
* `kind` is the host's `error` field: `denied` or `invalid`.
* A non-JSON 403, such as a failed CSRF check, is `denied`. Any other non-JSON body is `invalid`.
* `unsupported` when the browser has no WebAuthn. `denied` when the passkey prompt was dismissed.

### AssertionError

* `class AssertionError extends Error`, `name` `"AssertionError"`: what a failed `expect` or `assert` throws. From `@snapfire/fsr/client/testing`.

### Thrown Errors

| Thrown by | When |
| --- | --- |
| `decodeValue` | `unknown value tag: <tag>`, `unknown typed array kind: <kind>` |
| `decodeNode` | `unknown node row kind: <kind>` |
| `parseRow`, `parsePayload` | `unknown payload row tag: <tag>`. `parsePayload` also throws `payload has no N row` |
| `encodeValue` | `bigint outside the value model's integer range` |
| `renderSegment` | `segment path walks through a non-seq node` |
| an action callable, `upload` | `ActionFailure` on a non-ok status. The fetch's own error when the request never completes |
| `signIn`, `enrol`, `answer` | `AuthFailure` on a refused step or a passkey step the browser cannot run. The browser's own error when the passkey prompt fails |
| `foreign` component | `<module> is mounted by another framework, so it takes no React children` |
| `useHoisted`'s `c` | a read with no markup and no `miss` |
| `time.add`, `time.diff`, `crypto.digest` | an unknown unit, algorithm or encoding |
| a host-only or terminal-only member in a browser | `<name> runs in a host only`, `<name> runs in a terminal page only` |
| a `native` without `f` | `<name> runs on the server only` |
| `loadScript` | rejects with `<src> did not load` |
| `openFile`, `openExternal` | reject with `<name> runs only in an fsr --desktop window` or the window's message |
| a `getBy` or `getAllBy` query | `TestingLibraryElementError` |

### Silent Degradations

| Situation | Behaviour |
| --- | --- |
| No island registered for a marker's module id | Marker left as rendered. `console.warn("sf: no island registered for <id>")` once the document settles and it is still unregistered |
| An entry module fails to import | `console.warn("sf: loading <src> failed", err)`. `src` is forgotten so a later call retries |
| An adapter module fails to import | `console.warn("sf: loading the adapter for <id> failed", err)` |
| A loader or mounter rejects | `console.warn("sf: mounting <id> failed", err)`. Marker left as rendered |
| A module reference names an unregistered file | `console.warn("sf: no module registered for <file>")`. The prop stays a reference |
| A marker with no `data-sf-module` | Skipped |
| Missing or empty props script | Mounted with `{}` |
| A server island's post fails | `console.warn("sf: island <module> step failed with <status>: <text>")`. Island left as it was |
| A declared script fails to load | `console.error("sf: <message>")` |
| The service worker does not register | `console.warn("fsr: the service worker did not register", error)` |
| `navigate` gets a non-ok response, no `G` row or a payload it cannot patch | `window.location.assign(href)`, with `console.warn` naming the patch failure |
| `refresh` has no sidecar, a non-ok response or a payload it cannot patch | `window.location.reload()` |
| `refresh` gets no response at all | Page left as it is |
| A streamed row fails to apply | `console.warn("sf: a streamed payload stopped applying", err)`. Later rows are not applied |
