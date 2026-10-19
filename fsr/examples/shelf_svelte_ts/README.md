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

## The three islands

`Progress` holds the page count in `$state`, derives the percentage with `$derived` and saves through `actions.book.$id.progress`. Finishing a book or starting it again moves `shelf/done` through `optimistic`, so the masthead changes before the action answers and goes back if it fails.

`Tally` is a Svelte 4 component: `export let`, a `$:` statement and a store read as `$count`. `useStore` from `@snapfire/fsr-client/svelte` returns a Svelte store over the FSR key, so a legacy component subscribes the way it always has.

`Recent` reads `recent.svelte.ts`, which exports a `$state` object and a `visit` function. `Progress` calls `visit` when it mounts. Both islands import the same module, so they share the one object and the list updates as you open books. The list is browser state and starts empty on a reload.

`recent.svelte.ts` sits beside `Recent.svelte`. TypeScript alone would resolve an import of the component to that module, so the build writes a declaration for `Recent.svelte` under `generated/shadowed/` and maps `@src/ui/Recent.svelte` to it. A template imports the component through the alias for that reason; a relative import of it is refused with the alias to use.

## What the server writes

The server has no body for a Svelte component. It writes each island's marker and props with nothing between them. Svelte mounts rather than hydrates. `fsr build --verbose` lists the three as `foreign` with the plugin's reason. The build still reads each component's scripts, so `src/store.ts` and `recent.svelte.ts` are compiled for the browser because a Svelte file imports them.

## What each thing proves

| Capability | Where |
| --- | --- |
| A dynamic segment with a loader that refuses | `/book/{id}` answers 404 for an id off the shelf |
| An action that writes the session | `progress` clamps the page, marks the book finished and answers the count |
| Optimistic store writes from Svelte | `Progress` around `optimistic(doneCount, ...)` |
| A legacy component beside runes | `Tally` in the masthead, `Progress` on the page |
| A shared runes module | `recent.svelte.ts`, written by one island and read by another |
| Islands on load and on idle | `<Island when="load">` around `Tally` and `Progress`, `<Island when="idle">` around `Recent` |
| Client navigation keeping layout state | open two books: the side column keeps its list while the page region is replaced |

## Tests

`tests/book/actions.test.ts`, `tests/book/loader.test.ts` and `tests/shelves.test.ts` are body tests: lowered, run against a mocked context and read back from its session. `tests/progress.spec.tsx` and `tests/recent.spec.tsx` load pages in the test DOM, click through the Svelte islands and check what the session, the masthead and the side column say afterwards.
