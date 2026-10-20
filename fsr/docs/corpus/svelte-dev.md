# svelte.dev on the server

The corpus in `fsr/examples/corpus` builds the 167 examples and tutorial solutions from svelte.dev (MIT, at `89f941caf5a2229356fc38facae6ee0dca3122e7`) through FSR as one application per example and records what the build made of each in `svelte_outcomes.txt`. A tutorial exercise starts from the files of the one before it, as the tutorial does. Run it from `fsr/examples` with `snapfirec-svelte` on `PATH`: `cargo test -p corpus --test svelte_dev`. The counts here are from `8a3db060`.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Rendered on the server | 150 | the server renders every component on the page the way Svelte's own server renderer does |
| Browser rendered, SSR pending | 1 | a component renders in the browser because it uses something FSR can't render on the server yet |
| Left to the browser by design | 16 | a component uses something the server can't reproduce exactly, such as the current time or `Math.random` |
| Refused | 0 | the build stops |
| Total | 167 | |

A component rendered in the browser still works the way it does under Svelte. Its HTML is just missing from the first response. The corpus checks what the build lowers. The lowerer's own tests render each construct and compare the markup byte for byte with `svelte/server` running in the build's JavaScript engine.

## Only the first cause is listed

The build stops a module at its first cause. A component that fails never lowers its children. An example names the first thing in each component the build could not render, so removing one cause can uncover the next. `tutorial-advanced-svelte-advanced-reactivity-raw-state` names an assignment inside an expression in `data.js`; the data it assigns comes from `Math.random`, so it is counted by design.

## What sends a component to the browser

### Lowerable

| Cause | Examples | What the server does not render yet |
| --- | --- | --- |
| A call through a context value to a function that only schedules `$effect`, `tutorial-advanced-svelte-context-context-api` | 1 | Following a function value through a context and reading a body of browser-only calls as doing nothing |

### By design

| Cause | Examples | Why it stays in the browser |
| --- | --- | --- |
| The current time: `new Date()`, `SvelteDate` and a `readable` store that sets it when subscribed | 5 | The server and the browser read the clock at different moments |
| An npm package: `marked`, `d3-scale`, `mapbox-gl` and a CSS-in-JS tag | 6 | The build follows the application's own files, not a package's |
| `Math.random` | 2 | Nondeterministic |
| `<svelte:head>` | 2 | What goes in the document head belongs in a loader's `meta` export |
| `new Audio` | 1 | A browser object |

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1` and bring the counts here and in the table in [the FSR README](../../README.md#how-much-renders-on-the-server) up to that commit.
