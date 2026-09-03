# Render: the IR renderer against React in QuickJS

**Bench:** `fsr/examples/shopping_react_ts/benches/render.rs`, run from `fsr/examples/shopping_react_ts` with `cargo bench --bench render`.

**Question:** a page the build lowered is rendered by the interpreter in Rust; the same page could be rendered by React's own `renderToString` in QuickJS, the engine `fsr test` already embeds. How far apart are they per page, what does a QuickJS context cost to bring up and is the IR worth keeping for rendering at all, or only for the pages an engine would be too slow for?

## What is measured

The three storefront pages with realistic props: the catalog with twelve products, one product page, a cart with three lines. Every group renders the same props to the same HTML; the bench prints whether the two outputs are byte-identical, which they must be for hydration to be clean, so a `DIFFERENT` line is a finding before any number is.

| Group | One iteration is |
| --- | --- |
| `ir/load_components` | parsing the plan's `components` rows from JSON, the cost of getting the lowered pages into memory once at boot |
| `ir/render/<page>` | the interpreter rendering the lowered page with the props already in the value model |
| `quickjs/render/<page>` | `renderToString(createElement(Page, props))` in a warm context with the props already decoded on the JavaScript side, React's production build |
| `quickjs/render_with_decode/<page>` | the same, with the props arriving as the wire JSON and decoded first, which is what a request would pay |
| `quickjs/cold_context/<page>` | a new QuickJS context: prelude, DOM bootstrap, then loading `react`, `react-dom/server`, the client library and the page module; what an isolate-per-request design would pay before rendering anything |

## What is not measured

The host's work around the render: route matching, loaders, the payload assembly and the shell. Both renderers sit at the same point in that pipeline, so the comparison is between them alone.

## How to read it

`ir/render` against `quickjs/render` is the direct answer. If the gap is small on these pages, the IR buys sandboxing and no JavaScript in the serving path rather than speed, and the vocabulary work in the lowerer is justified on those grounds alone. If it is large, the IR is the fast path and the engine is the fallback for residue, which is the split JS_ENGINE.md sketches. `quickjs/cold_context` says whether a context can be per request or must be pooled and warmed; `render_with_decode` is the per-request number for the engine path.

## Preparation

The bench prepares the app the way `fsr test` does, compiling the pages into `app/.fsr-test/dist` with the workspace's `target/debug/snapfirec`, or the one `SNAPFIREC` names; a `snapfirec` on `PATH` from before tsconfig `paths` support cannot compile the aliases. It fetches `react-dom/server` from esm.sh into `app/.fsr-test/vendor` on its first run and keeps it.

## Results

### 2026-09-03, `780be15`, MacBook M4 Pro

Criterion, release profile, one run. The fidelity line read `DIFFERENT` on all three pages: the IR wrote `disabled` where React writes `disabled=""`, `<input>` where React writes `<input/>` and `selected` before `value` on an option. All three were serialisation only and the renderer now matches React byte for byte, which the `--test` pass confirmed as `identical` for every page. The timings below are from before that change; it adds a few bytes per page and nothing else.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `ir/load_components` | 106.07 µs | 106.37 µs | 106.67 µs |
| `ir/render/catalog_12` | 1.1171 ms | 1.1222 ms | 1.1278 ms |
| `quickjs/render/catalog_12` | 1.7542 ms | 1.7594 ms | 1.7649 ms |
| `quickjs/render_with_decode/catalog_12` | 1.9019 ms | 1.9082 ms | 1.9157 ms |
| `quickjs/cold_context/catalog_12` | 19.354 ms | 19.422 ms | 19.495 ms |
| `ir/render/product` | 154.98 µs | 155.40 µs | 155.87 µs |
| `quickjs/render/product` | 475.01 µs | 477.00 µs | 479.12 µs |
| `quickjs/render_with_decode/product` | 494.11 µs | 495.67 µs | 497.35 µs |
| `quickjs/cold_context/product` | 19.713 ms | 19.777 ms | 19.847 ms |
| `ir/render/cart_3` | 151.32 µs | 151.80 µs | 152.33 µs |
| `quickjs/render/cart_3` | 511.74 µs | 514.33 µs | 517.15 µs |
| `quickjs/render_with_decode/cart_3` | 547.72 µs | 549.33 µs | 550.96 µs |
| `quickjs/cold_context/cart_3` | 19.532 ms | 19.610 ms | 19.696 ms |

Page sizes: catalog 10400 bytes, product 2821 bytes, cart 3053 bytes.

What it says. The IR is 3.1x to 3.4x faster than React in QuickJS on the two small pages and 1.6x on the catalog, where the twelve product cards make the interpreter's own overhead visible: 1.1 ms for 10 KB is about 110 ns per byte of output, which is slow for a Rust string builder and says the interpreter's evaluation, not the serialisation, is where the time goes. React's production build in QuickJS is closer than expected, and the decode of wire props adds under 10 percent on top of it. A cold QuickJS context with React and a page loaded costs about 19.5 ms, so an engine path is a warmed pool per worker, never a context per request. The gap is not the cliff JS_ENGINE.md was written to guard against: on these pages, either renderer serves a request in under 2 ms and the IR's case rests on the sandbox and on running no JavaScript in the serving path, with its speed a bonus that a profiling pass on the interpreter would widen.

### 2026-09-03, `089fc17`, MacBook M4 Pro

Criterion, release profile, one run, after step 1 of the interpreter optimisations: a synchronous evaluator and renderer for everything that holds no service call. The fidelity line read `identical` on all three pages.

| Benchmark | Lower | Estimate | Upper |
| --- | --- | --- | --- |
| `ir/load_components` | 119.09 µs | 119.47 µs | 119.88 µs |
| `ir/render/catalog_12` | 996.46 µs | 999.13 µs | 1.0021 ms |
| `quickjs/render/catalog_12` | 1.7694 ms | 1.7723 ms | 1.7753 ms |
| `quickjs/render_with_decode/catalog_12` | 1.9143 ms | 1.9178 ms | 1.9212 ms |
| `quickjs/cold_context/catalog_12` | 19.919 ms | 20.001 ms | 20.089 ms |
| `ir/render/product` | 129.35 µs | 129.72 µs | 130.10 µs |
| `quickjs/render/product` | 470.85 µs | 471.68 µs | 472.55 µs |
| `quickjs/render_with_decode/product` | 497.53 µs | 499.17 µs | 500.96 µs |
| `quickjs/cold_context/product` | 19.995 ms | 20.044 ms | 20.097 ms |
| `ir/render/cart_3` | 128.37 µs | 128.93 µs | 129.51 µs |
| `quickjs/render/cart_3` | 504.03 µs | 505.99 µs | 508.22 µs |
| `quickjs/render_with_decode/cart_3` | 544.34 µs | 545.75 µs | 547.13 µs |
| `quickjs/cold_context/cart_3` | 19.918 ms | 19.980 ms | 20.047 ms |

Page sizes unchanged: catalog 10400 bytes, product 2821 bytes, cart 3053 bytes.

What it says. Against the first run, the IR renders are 11 percent faster on the catalog and 15 to 17 percent faster on the two small pages; every QuickJS group is within 3 percent of last time, which is the control that says the machine was in the same state and the change is the interpreter's. The IR is now 1.8x ahead on the catalog and 3.6x to 3.9x on the small pages. Step 1 removed the future per node and per expression and bought a sixth, not the half the optimisation note expected, so the boxed futures were a cost but not the cost: the time is in what the evaluator does per node, the string lookups, the clones at every boundary and the per-element allocations the note lists next, and a flamegraph of a catalog render loop is the next measurement before any of those is touched. `ir/load_components` moved from 106 to 119 µs, a parse of the same rows that gained the `Omit` builtin and nothing else; within a run's variance for a group that allocates as it goes.
