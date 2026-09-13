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

Two runtimes on one page pay for both, and this page pays for three things that mount differently. Measured from the files this page loads, gzip at level 9:

| | raw | gzip |
| --- | --- | --- |
| React, with its adapter | 153.5K | 49.9K |
| Vue, with its adapter | 116.9K | 45.8K |
| htmx, with its binding | 165.3K | 37.0K |
| the fsr client | 60.8K | 17.6K |
| this application | 6.6K | 2.2K |
| everything the page loads | 503.1K | 152.6K |

Read it as the price of the claim rather than as a recommendation. React and Vue together are 96K compressed before a line of the application runs, which is why mixing is for a migration or for a page that genuinely holds two teams' work, not for a default. htmx is the largest raw file here and the cheapest thing on the page in what it asks of the framework: nothing mounts it, nothing hydrates it and the region it drives is markup the server already rendered.

The fsr client is served from the host binary unminified, which is why 60.8K raw is a fair share of it; minified it is 43.2K.
