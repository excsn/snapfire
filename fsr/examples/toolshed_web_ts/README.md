# toolshed_web_ts

A street's tool library: eight tools on four shelves, a page per tool, what the visitor has reserved kept in the session and two panels beside them of which one is always down.

What it shows is FSR with no framework in it at all. The pages are TypeScript templates the build lowers and the host renders, the same as every other example. The interactive pieces are custom elements: three `.ts` files under `src/elements/` that the browser upgrades where the server already wrote their markup, one of them inside a shadow root the server wrote too. The regions that talk to the server are htmx: an attribute asks the host for one segment of the route as a fragment and swaps it in; a form posts an action and gets the re-rendered fragment back. Nothing mounts, nothing hydrates and the island registry the build writes is empty.

## Running it

```sh
fsr dev app
```

`fsr serve app` serves what is already built; `fsr test app` runs the suite. The shed is on <http://127.0.0.1:8170/>.

## How it is put together

| Piece | What it is |
| --- | --- |
| `clients/shed.openapi.json` | the one service, four methods, two of them carrying a cache policy |
| `clients/shed.mock.json` | what those four methods answer, `getWeather` with a failure |
| `schemas/session.ts` | the session's shape and its defaults, which is what makes `session.reserved` typed |
| `schemas/shed.ts` | the action's input type, named in the contract the build emits |
| `routes/layout.tsx` | the masthead, the nav, the tally and the two panels, over `layout.loader.ts` |
| `routes/page.tsx` | the shelves, filtered by `?category=`, with the filter chips as htmx regions |
| `routes/tool/layout.tsx` | a second layout between the masthead and the tool |
| `routes/tool/[id]/` | the tool, its actions, the reserve form and the boundary that catches an id off the shelves |
| `routes/reserved/` | the session read back as a page, cached by nothing |
| `routes/slots/loans/` | a parallel segment with its own loader and fallback, polled by htmx |
| `routes/slots/weather/` | the same, behind a service that fails |
| `src/elements/shed-tally.ts` | the masthead count: a disclosure the element wires and a store key it follows |
| `src/elements/loan-planner.ts` | a range input inside a declarative shadow root, form-associated so the loan length posts with the reservation |
| `src/elements/time-ago.ts` | a due date rewritten as a distance from today, inside the polled panel |
| `src/main.ts` | boots the client, enables navigation and calls `bindHtmx`, which tells htmx and the client about each other |
| `vendor/htmx/htmx.esm.js` | htmx 2.0.10, committed, since an application carries its vendor tree |

## A page that mounts nothing

Read `app/generated/islands.ts` after a build. It registers nothing. Every template is marked `static` in the report, since none has state or handlers, so no route module is bundled and no mounter is imported. The bundle is `src/**/*` and the two generated files. The import map has three entries: the client, its store and htmx.

## Custom elements

A template writes a custom element the way it writes any element, `<shed-tally count={reserved}>`, with its light DOM inside. The lowerer takes a hyphenated tag as markup and the dialect's declarations type it as one, so a typo in an ordinary tag is still caught and `hx-get` on an anchor is an attribute like any other. The server renders the element's children; the browser upgrades the element when its definition runs, which is the module `main.ts` imports. There is no island around it and no mounter behind it, since the browser is the mounter.

`loan-planner` goes one further. Its template writes `<template shadowrootmode="open">` inside the element, so the parser attaches the shadow root before any script runs and the planner is styled and laid out from the first paint. A fragment swapped in later is parsed by `innerHTML`, which attaches no declarative shadow roots, so the element attaches its own from the template it finds; the same file handles both. Its slider is a form field too: a control inside a shadow root has no form owner, so the element declares `static formAssociated = true` and hands the length to the form through `ElementInternals.setFormValue`, under the `name` on its host. Once the tool is reserved the server writes the length back, disabled.

`shed-tally` reads the store. `subscribe` from `@snapfire/fsr-client/store` is the whole adapter: no hook, no ref, a callback. The layout's loader seeds the key and every fragment reseeds it, so the count moves when a reservation is made from a page the layout was not re-rendered for.

## htmx over fragments

A GET of any route with `__fragment` in the query answers the page segment alone, as markup with nothing around it: no shell, no layout, no segment delimiters, every deferred slot resolved before it is sent. `__fragment=loans` answers the parallel slot of that name instead, wherever it sits on the route. The shelf chips are `hx-get="/?category=Garden&__fragment"` with `hx-target="closest .page"`; the loans panel is `hx-get="?__fragment=loans"` on a fifteen second trigger. The host renders the whole route for either, layouts included and then picks the segment out of it.

The reserve form is `hx-post="/_sf/action/tool.$id.reserve?__fragment"`. A form-encoded action is answered with a redirect to the page that posted it. When the action's URL carried `__fragment` the redirect carries it too, so what htmx receives after the round trip is the tool page rendered from the session the action just wrote. The form also has a plain `action` and `method`, so it posts and lands back on the page with no JavaScript at all.

A fragment ends with the same inert seed script a document carries. `bindHtmx(htmx)` from `@snapfire/fsr-client/htmx` is the whole wiring: on `htmx:afterSettle` it calls `adopt()`, which reads every seed nothing has read yet, then `scan()`, which would mount any island the fragment placed; on `sf:navigate` and `sf:fill`, which the navigator dispatches after it applies a payload, it calls `htmx.process` so the forms and regions the navigator wrote are wired. Without that second direction a reserve form reached by clicking a tool name would post natively and reload the document.

## What each thing proves

| Capability | Where |
| --- | --- |
| Nested layout plus dynamic segment | `routes/layout.tsx` over `routes/tool/layout.tsx` over `/tool/{id}` |
| Two loaders resolving in parallel | the two independent calls in `routes/page.loader.ts`, plus the two slots beside the page |
| An action mutating and revalidating | `reserve` guards against the shelves, writes the session and the fragment that comes back is rendered from it |
| One island on load, one on visible | none: nothing here is an island; the elements are defined when `main.ts` runs and upgrade wherever their markup is |
| A segment whose service call fails | `getWeather` answers `$fail`, the panel falls to `slots/weather/error.tsx` and the page is otherwise whole |
| A cached segment plus an uncached one | `getShed` and `listTools` carry `x-sf-cache`, `listLoans` and `getWeather` do not |
| Metadata from loader data | `export const meta` in the tool loader and the reserved loader |
| Client navigation preserving layout state | open the tally panel, click a tool: the page region is replaced and the panel stays open |

Two of the interactions the suite cannot drive. The spec harness has no XHR, so htmx loads under it but never fetches; the specs fetch the fragments themselves and assert on what comes back. Custom elements are defined in the harness's registry and the specs assert on the markup the server wrote rather than on what an element did with it. Open the browser for those two.

## Deploying it

```sh
fsr bundle app --out dist
```

42 files, no `.tsx` among them, htmx one of them. The thing that goes beside the tree is `fsr` itself: this application has no binary of its own.

```sh
fsr serve dist/app
```

## The lab

Ask for a fragment by hand: `curl 'http://127.0.0.1:8170/?category=Party&__fragment'`. The answer starts at `<section` and ends with the seed script. Ask for `?__fragment=weather`: the error boundary, alone. Ask for `?__fragment=nope`: 404 and one line saying which slot the route does not have.

Open a tool, open the network panel and click "Reserve it". One POST answered 303, one GET of `/tool/3?__fragment` answered with the fragment and the count in the masthead moved without the masthead being touched.

Take the `bindHtmx(htmx)` call out of `main.ts`, rebuild, click a tool name from the shelves and reserve it. The document reloads: the form the navigator wrote was never processed by htmx, so the browser posted it natively. Put the call back.

Give a page state. Add a `useState` to `routes/page.tsx` and read the report: the page stops being `static`, it appears in the registry with the React mounter and the bundle asks the import map for `react/jsx-runtime`, which this application does not have.
