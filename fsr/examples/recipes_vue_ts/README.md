# recipes_vue_ts

A household recipe box: six recipes, a page per recipe, what is planned for tonight kept in the session and two panels beside them of which one is always down.

What it shows is a second framework fitting the seam. The pages are TypeScript templates the build lowers and the host renders, the same as every other example. Every interactive piece is a Vue single-file component: a `.vue` file under `src/ui/`, compiled by `snapfirec-vue` out of process, placed by a page as an island and mounted by Vue in the browser. There is no React anywhere in the application: not in the import map, not in the vendor tree, not in the bundle.

## Running it

`snapfirec-vue` has to be on PATH, since `snapfirec` hands every `.vue` file to it. Without it the build stops with the binary it looked for, the command that installs it and the file that needed it.

```sh
fsr dev app
```

`fsr serve app` serves what is already built; `fsr test app` runs the suite. The box is on <http://127.0.0.1:8160/>.

## How it is put together

| Piece | What it is |
| --- | --- |
| `clients/kitchen.openapi.json` | the one service, four methods, two of them carrying a cache policy |
| `clients/kitchen.mock.json` | what those four methods answer, `listMarket` with a failure |
| `schemas/session.ts` | the session's shape and its defaults, which is what makes `session.planned` typed |
| `schemas/kitchen.ts` | the action's input type, named in the contract the build emits |
| `routes/layout.tsx` | the masthead, the nav and the two panels, over `layout.loader.ts` |
| `routes/page.tsx` | the box, filtered by `?course=` |
| `routes/recipe/layout.tsx` | a second layout between the masthead and the recipe |
| `routes/recipe/[id]/` | the recipe, its actions and the boundary that catches an id off the box |
| `routes/tonight/` | the session read back as a page, cached by nothing |
| `routes/slots/notes/` | a parallel segment with its own loader and fallback |
| `routes/slots/market/` | the same, behind a service that fails |
| `src/ui/Tonight.vue` | the masthead island: store-backed count, Vue state for the panel |
| `src/ui/PlanRecipe.vue` | the action call, optimistic against the store |
| `src/ui/Scaler.vue` | placed with `<Island when="visible">`, a computed over the ingredients |

## A page that mounts nothing

Read `app/generated/islands.ts` after a build. It registers three modules, all of them `.vue`, all with the Vue mounter, and imports nothing else. No route module is in it.

That is the build reading each template: a page or layout with no state and no handlers has nothing for the browser to change, so it is marked `static` in the report, left out of the registry and left out of the bundle. The server renders its markup and the client's navigator swaps it as markup. The islands inside it are mounted by the document's own scan, so a Vue island sits under a static layout the way it would under a React one.

The page a Vue island is placed on has nothing of the island in it. The server has no body for a `.vue` component, so it writes the island's marker and its props and nothing between them, and Vue mounts rather than hydrates. Look at `/recipe/3` before the scripts run: the plan button is not there yet.

## The scoped styles

Each component's `<style scoped>` comes out of the plugin as `src/ui/<Name>.vue.css` beside its module, scoped to a `data-v-` attribute the emitted render function writes. The compiler lists those files in its build facts and the host links every one of them in the document head, so the styles are in place before the first island mounts.

## What each thing proves

| Capability | Where |
| --- | --- |
| Nested layout plus dynamic segment | `routes/layout.tsx` over `routes/recipe/layout.tsx` over `/recipe/{id}` |
| Two loaders resolving in parallel | the two independent calls in `routes/page.loader.ts`, plus the two slots beside the page |
| An action mutating and revalidating | `plan` guards against the box, writes the session and the count in the masthead moves |
| One island on load, one on visible | `<Island when="load">` around the plan control, `<Island when="visible">` around the scaler |
| A segment whose service call fails | `listMarket` answers `$fail`, the panel falls to `slots/market/error.tsx` and the page is otherwise whole |
| A cached segment plus an uncached one | `getBox` and `listRecipes` carry `x-sf-cache`, `listNotes` and `listMarket` do not |
| Metadata from loader data | `export const meta` in the recipe loader and the tonight loader |
| Client navigation preserving layout state | open the masthead panel, click a recipe: the page region is replaced and the panel stays open |

The one thing the suite cannot tell apart is island timing, since the spec harness reports every observed element as in view. Scroll the recipe page in a browser instead: the scaler mounts when it comes into view and not before.

## Deploying it

```sh
fsr bundle app --out dist
```

74 files, no `.tsx` and no `.vue` among them, three of them the component stylesheets. The thing that goes beside the tree is `fsr` itself: this application has no binary of its own.

```sh
fsr serve dist/app
```

## The lab

Take `snapfirec-vue` off PATH and build. The error names the binary, `cargo install snapfire_vue` and the three files that wanted it.

Break a component. Put an unclosed tag in `src/ui/Scaler.vue` and build: the plugin's diagnostic comes back with the file and the line, the other two components compile and the build stops.

Give a page state. Add a `useState` to `routes/page.tsx` and read the report: the page stops being `static`, it appears in the registry with the React mounter and the bundle now asks the import map for `react/jsx-runtime`, which this application does not have. That line is the whole reason the static rule exists.
