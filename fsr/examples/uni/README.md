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
| `templates/layout.tera` | the layout: a masthead with the React island, `content` for the page and `tape` for the region beside it |
| `templates/board.tera` | the board page, whose one island is Vue |
| `templates/news.tera` | the news page, whose one island is React |
| `templates/tape.tera` | the region htmx polls; no island, no mounter, no framework |
| `js/src/ui/Watch.tsx` | React, in the masthead: the symbol the desk is watching |
| `js/src/ui/Holdings.vue` | Vue, on the board: a table whose row click writes that symbol |
| `js/src/ui/Feed.tsx` | React, on the news page: filters the headlines by the watched symbol |
| `js/src/main.ts` | three `registerIsland` calls, two mounters, one `bindHtmx` |
| `src/routes.rs` | the plans: a layout over a page, with `tape` as a second child |
| `src/loaders.rs` | four sources in Rust, one per segment |

## One store, two runtimes

`js/src/ui/Watch.tsx` exports the key:

```ts
export const watchedKey = key<string>("uni/watched");
```

React reads it with `useStore` from `@snapfire/fsr-client/react`, Vue with `useStore` from `@snapfire/fsr-client/vue`. Both are the same store under two adapters, so clicking a row in the Vue table moves the symbol in the React masthead without either knowing the other exists. Each island is also rendered on the server with that symbol as a prop, so the first paint agrees with the store before any script runs.

## What a fragment costs

The tape asks for itself: `hx-get="?__fragment=tape"` on a ten second trigger. The host renders the whole route, picks that slot out and writes it with no shell, no layout and no sidecar. Nothing in it mounts, so the region costs no framework at all. `bindHtmx` is what keeps the two libraries aware of each other after a swap or a navigation.

## The honest cost

Two runtimes on one page pay for both. The vendor tree here is React at 152 KiB, Vue at 116 KiB and htmx at 168 KiB, before compression and before the client itself. That is the number the composition claim has to carry, which is why this example exists rather than a paragraph saying mixing is possible.
