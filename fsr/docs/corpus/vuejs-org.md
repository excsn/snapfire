# vuejs.org on the server

The corpus in `fsr/examples/corpus` builds the 35 examples and tutorial steps from vuejs.org (CC BY 4.0, at `40aa88af0094f7bab4aaf786e55c748a6a251d88`), each in its Composition API and its Options API form, through FSR as one application per program and records what the build made of each in `vue_outcomes.txt`. Run it from `fsr/examples` with `snapfirec-vue` on `PATH`: `cargo test -p corpus --test vuejs_org`. The counts here are from `8e5253ab`.

| Outcome | Programs | Meaning |
| --- | --- | --- |
| Rendered on the server | 58 | the server renders every component on the page the way Vue's own server renderer does |
| Browser rendered, SSR pending | 0 | a component renders in the browser because it uses something FSR can't render on the server yet |
| Left to the browser by design | 12 | a component uses something the server can't reproduce exactly, such as the current time or `Math.random` |
| Refused | 0 | the build stops |
| Total | 70 | |

A component rendered in the browser still works the way it does under Vue. Its HTML is just missing from the first response. The corpus checks what the build lowers. The lowerer's own tests render each construct and compare the markup byte for byte with Vue's server renderer running in the build's JavaScript engine.

## Only the first cause is listed

The build stops a module at its first cause. A component that fails never lowers its children. A program names the first thing in each component the build could not render, so removing one cause can uncover the next.

## What sends a component to the browser

### By design

| Cause | Programs | Why it stays in the browser |
| --- | --- | --- |
| `fetch`, in setup or in `created` | 2 | The data belongs in the route's loader |
| The current time: `performance.now()`, `new Date()` read in the viewer's time zone | 4 | The server and the browser read the clock at different moments |
| An npm package: `marked`, `js-confetti` | 4 | The build follows the application's own files, not a package's |
| `new Function`, which the cells example builds its formulas with | 2 | Code evaluated from text |

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1` and bring the counts here and in the table in [the FSR README](../../README.md#how-much-renders-on-the-server) up to that commit.
