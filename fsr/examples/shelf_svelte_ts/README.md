# shelf_svelte_ts

A reading shelf: six books, a page per book, how far you have read each one kept in the session and a masthead that counts the ones you have finished.

The pages are TypeScript templates the build lowers and the host renders, as in every other example. Every interactive piece is a Svelte component: a `.svelte` file under `src/ui/`, compiled by `snapfirec-svelte` out of process, placed by a page as an island and mounted by Svelte in the browser. There is no React anywhere in the application. The templates import `Island`, `Link` and `Children` from `@snapfire/fsr-authoring/template`, so the typecheck reads their JSX through the dialect's own declarations.

## Running it

`snapfirec-svelte` has to be on PATH, since `snapfirec` hands every `.svelte`, `.svelte.ts` and `.svelte.js` file to it. Without it the build stops with the binary it looked for and the command that installs it.

```sh
fsr dev app
```

`fsr serve app` serves what is already built; `fsr test app` runs the suite. The shelf is on <http://127.0.0.1:8165/>.

Svelte itself is vendored as published, `fsr add --published app svelte@5.57.2`, so `app/vendor/svelte/` holds the package's own tree and the import map names each subpath. `snapfirec-svelte` compiles with that version's compiler, which `fsr` fetches into `app/.fsr/compilers/` and never ships.

## How it is put together

| Piece | What it is |
| --- | --- |
| `schemas/session.ts` | the session's shape and its defaults: the book being read, pages read per book, finished per book |
| `schemas/shelf.ts` | the two actions' input types |
| `src/books.ts` | the six books, a module constant the loaders and the actions read |
| `src/store.ts` | `shelf/done`, the one store key, seeded by the root layout |
| `routes/layout.tsx` | the masthead and the side column, over `layout.loader.ts` |
| `routes/page.tsx` | the shelf, filtered by `?shelf=` |
| `routes/book/[id]/` | the book, its loader and the `progress` and `pick` actions |
| `src/ui/Tally.svelte` | the masthead island, written in legacy mode |
| `src/ui/Progress.svelte` | the book's island, written with runes |
| `src/ui/Recent.svelte` | the side column's island, placed with `<Island when="idle">` |
| `src/ui/recent.svelte.ts` | a runes module both `Progress` and `Recent` import |
| `routes/stats/` | the reading stats, over a loader that reads the session |
| `src/ui/Stats.svelte` | the stats island, which renders the four components below |
| `src/ui/Table.svelte` | a table that renders the row snippet its caller passes |
| `src/ui/Legend.svelte` | a legacy component with a named slot, `$$slots` and `$$restProps` |
| `src/ui/Bars.svelte`, `src/ui/Dots.svelte` | the two views `Stats` picks between at run time |

## The three islands

`Progress` holds the page count in `$state`, derives the percentage with `$derived` and saves through `actions.book.$id.progress`. Finishing a book or starting it again moves `shelf/done` through `optimistic`, so the masthead changes before the action answers and goes back if it fails.

`Tally` is a Svelte 4 component: `export let`, a `$:` statement and a store read as `$count`. `useStore` from `@snapfire/fsr-client/svelte` returns a Svelte store over the FSR key, so a legacy component subscribes the way it always has.

`Recent` reads `recent.svelte.ts`, which exports a `$state` object and a `visit` function. `Progress` calls `visit` when it mounts. Both islands import the same module, so they share the one object and the list updates as you open books. The list is browser state and starts empty on a reload.

`recent.svelte.ts` sits beside `Recent.svelte`. TypeScript alone would resolve an import of the component to that module, so the build writes a declaration for `Recent.svelte` under `generated/shadowed/` and maps `@src/ui/Recent.svelte` to it. A template imports the component through the alias for that reason; a relative import of it is refused with the alias to use.

## The stats page

`/stats` is one island, `Stats`, written to use most of what Svelte's templates offer, so the build's Svelte front end is exercised by an application as well as by its tests.

- `{#each rows as { id, title, pages, read, done }, i}` destructures inside a snippet that `Table` renders with `{@render row(item, i)}`. `{const share = …}` declares a value inside it.
- `class:done` and `style:--share` sit on each row, under a `<style>` Svelte scopes to the component.
- A `<select bind:value>` orders the rows and a checkbox switches the view: `View` is `Bars` or `Dots`, chosen at run time and placed inside `{#key share}`.
- The heading is `<svelte:element this={$heading}>`, its tag from a `derived` store over a `writable`.
- The total is written with `{@html}`, the legend's default slot holds an `{#await}` and the fine print sits in a `<svelte:boundary>`.
- `Legend` is a Svelte 4 component: `export let`, `$:` statements, a named `<slot>` the caller fills with `let:average`, `{#if $$slots.title}` and `{...$$restProps}`.

## What the server writes

The build lowers every component, so the server writes their markup with Svelte's hydration markers and Svelte hydrates it in the browser. `fsr build --verbose` lists them as `lowered` with `svelte` in the detail column. The page reads `Tally`'s count from the store seed, `Progress`'s page and percentage from the session and `Recent`'s empty list from `recent.svelte.ts`, whose `$state` holds its initial value on the server. `src/store.ts` and `recent.svelte.ts` are compiled for the browser because a Svelte file imports them.

## What each thing proves

| Capability | Where |
| --- | --- |
| A dynamic segment with a loader that refuses | `/book/{id}` answers 404 for an id off the shelf |
| An action that writes the session | `progress` clamps the page, marks the book finished and answers the count |
| Optimistic store writes from Svelte | `Progress` around `optimistic(doneCount, ...)` |
| A legacy component beside runes | `Tally` in the masthead, `Progress` on the page |
| A shared runes module | `recent.svelte.ts`, written by one island and read by another |
| FSR's components in a Svelte file | `Recent` writes `<Link>` from `@snapfire/fsr/components`, which the build gives as a Svelte component |
| Islands on load and on idle | `<Island when="load">` around `Tally` and `Progress`, `<Island when="idle">` around `Recent` |
| Svelte's template features rendered on the server | `/stats`: snippets passed to a child, slots, scoped styles, stores, a component chosen at run time, `svelte:element`, `{#key}`, `{#await}`, `{@html}` and `<svelte:boundary>` |
| Client navigation keeping layout state | open two books: the side column keeps its list while the page region is replaced |

## Tests

`tests/book/actions.test.ts`, `tests/book/loader.test.ts` and `tests/shelves.test.ts` are body tests: lowered, run against a mocked context and read back from its session. `tests/progress.spec.tsx` and `tests/recent.spec.tsx` load pages in the test DOM, click through the Svelte islands and check what the session, the masthead and the side column say afterwards. `tests/stats.spec.tsx` loads the stats page, checks what the server wrote and reorders and redraws it in the test DOM.
