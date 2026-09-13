# uni

The mixed application: a Tera layout holding a React island, a Vue island and an htmx region, on one page, from one payload.

What it shows is the seam. A placement carries a module id and nothing else, so the browser dispatches each one to whatever mounter its registry entry names: React for one island, Vue for the next, no mounter at all for a region htmx drives. The server does not know which is which. Neither does the router: it replaces a segment holding a Vue component with one holding a React component and keeps the layout, because a segment is a segment.

```sh
cargo run -p uni                      # http://127.0.0.1:8180/board
```

The browser side is built separately, as it is in every example with a Rust project:

```sh
cd js && snapfirec --root . --config tsconfig.build.json --source-map --minify compact \
  --public-path /static/js/app --import-map importmap.json
```

That needs `snapfirec-vue` on `PATH`, `cargo install snapfire_vue`, since one component is a single-file component.

## The pieces

| File | What it is |
| --- | --- |
| `templates/document.tera` | the document: `<html>`, the head and one slot, so the layout beneath it is a region the navigator can patch |
| `templates/layout.tera` | the layout: a masthead with the React island and the server-mode one, `content` for the page and `tape` for the region beside it |
| `templates/board.tera` | the board page, whose one island is Vue |
| `templates/news.tera` | the news page, whose one island is React |
| `templates/tape.tera` | the region htmx polls; no island, no mounter, no framework |
| `js/src/ui/Watch.tsx` | React, in the masthead: the symbol the desk is watching, holding a Vue sparkline |
| `js/src/ui/Sparkline.vue` | Vue, inside that React island: the watched symbol's trail |
| `js/src/ui/Holdings.vue` | Vue, on the board: a table whose rows each hold a React chip |
| `js/src/ui/Chip.tsx` | React, inside each Vue row: the day's move, with a click that changes the watched symbol |
| `js/src/ui/Feed.tsx` | React, on the news page: filters the headlines by the watched symbol |
| `js/src/ui/Lot.tsx` | the one component the server renders itself: lowered by `build.rs` into the plan, placed in server mode, mounting nothing; its buttons call an action, which the host dispatches when the handler runs |
| `js/src/main.ts` | five `registerIsland` calls, two mounters, one `bindHtmx`, one `live` |
| `src/routes.rs` | the plans: a document over a layout over a page, with `tape` as a second child |
| `src/loaders.rs` | four sources in Rust, one per segment |
| `src/actions.rs` | `desk.buy`, which writes the session both islands are rendered from |
| `build.rs` | lowers `Lot.tsx` to the IR and writes the plan the host reads |

## One store, two runtimes

`js/src/ui/Watch.tsx` exports the key:

```ts
export const watchedKey = key<string>("uni/watched");
```

React reads it with `useStore` from `@snapfire/fsr-client/react`, Vue with `useStore` from `@snapfire/fsr-client/vue`. Both are the same store under two adapters, so clicking a row in the Vue table moves the symbol in the React masthead without either knowing the other exists. Each island is also rendered on the server with that symbol as a prop, so the first paint agrees with the store before any script runs.

## Four mounting models, one page

| The piece | How it comes alive |
| --- | --- |
| the masthead, the chips, the feed | React hydrates or mounts them, `reactMounter` |
| the table, the sparkline | Vue mounts them fresh, `vueMounter` |
| the lot stepper | nothing mounts: every click posts to the host, Rust runs the handler, dispatches the `desk.lot` action it calls, renders the component again and the browser patches the markup, then refreshes the page's data the way it does after any action |
| the tape | nothing mounts: htmx swaps a fragment the host rendered |

Two of those ship a framework and two do not.

## One island inside another, across frameworks

`Mount` places an island by module id rather than by component, which is what a tree of one framework needs to hold an island of another:

```tsx
<Mount module="js/src/ui/Sparkline.vue#default" props={{ points: quote.trail, up: quote.change >= 0 }} />
```

The React masthead holds that Vue sparkline; every Vue table row holds a React chip the same way, through `Mount` from the Vue entry. Neither parent renders the child: it writes the marker the boot runtime reads, the registry decides the mounter, then a change to the parent's props patches the nested island in place rather than tearing it down. Clicking a chip, React inside Vue, moves the store, which moves the masthead, React, which patches the sparkline, Vue inside React.

## One action, both frameworks patched

`desk.buy` is a Rust action the React masthead posts to. It writes the session; the revalidation that follows re-renders the layout and the board, with the navigator patching each island in place from the payload rather than remounting it: the same `<div class="watch">` element, the same `<table>`, new numbers in both.

That is what a keyed placement buys. `key=` on a Tera `island(...)` writes `data-sf-region` into the markup and `$k` into the props, which is what the navigator matches on. Without a key an island is either kept exactly as it stands or replaced wholesale, which loses most of the point.

## One push, both frameworks

The desk's clock moves every four seconds, publishes `prices`, and `live(["prices"])` in the entry module revalidates the route. Nobody clicked: the masthead and the table take the new price together, each patched in place.

## What a fragment costs

The tape asks for itself: `hx-get="?__fragment=tape"` on a ten second trigger. The host renders the whole route, picks that slot out and writes it with no shell, no layout and no sidecar. Nothing in it mounts, so the region costs no framework at all. `bindHtmx` is what keeps the two libraries aware of each other after a swap or a navigation.

## The honest cost

Two runtimes on one page pay for both, and this page pays for three things that mount differently. Measured from the files this page loads, gzip at level 9:

| | raw | gzip |
| --- | --- | --- |
| React, with its adapter | 154.5K | 50.1K |
| Vue, with its adapter | 118.4K | 46.3K |
| htmx, with its binding | 165.3K | 37.0K |
| the fsr client | 61.0K | 17.7K |
| this application | 10.3K | 3.6K |
| everything the page loads | 509.5K | 154.7K |

Read it as the price of the claim rather than as a recommendation. React and Vue together are 96K compressed before a line of the application runs, which is why mixing is for a migration or for a page that genuinely holds two teams' work, not for a default. htmx is the largest raw file here and the cheapest thing on the page in what it asks of the framework: nothing mounts it, nothing hydrates it and the region it drives is markup the server already rendered.

The fsr client is served from the host binary unminified, which is why 60.8K raw is a fair share of it; minified it is 43.2K.
