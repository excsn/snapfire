# vuejs.org on the server

The corpus in `fsr/examples/corpus` builds the 35 examples and tutorial steps from vuejs.org (CC BY 4.0, at `40aa88af0094f7bab4aaf786e55c748a6a251d88`), each in its Composition API and its Options API form, through FSR as one application per program and records what the build made of each in `vue_outcomes.txt`. Run it from `fsr/examples` with `snapfirec-vue` on `PATH`: `cargo test -p corpus --test vuejs_org`. The counts here are from `d78acb65`.

| Outcome | Programs | Meaning |
| --- | --- | --- |
| Lowered whole | 52 | the server renders every component on the page as Vue's own server renderer writes it |
| Client side rendered, lowerable | 6 | a component renders in the browser because it uses something the build does not render on the server yet |
| Client side rendered by design | 12 | a component renders in the browser for a reason that holds under any server renderer |
| Refused | 0 | the build stops |
| Total | 70 | |

A component that is client side rendered still works: it renders in the browser as it would under Vue and its HTML is missing from the first response. The corpus checks what the build lowers. The lowerer's own tests render each construct and compare the markup byte for byte with Vue's server renderer running in the build's JavaScript engine.

## How causes hide each other

The build stops a module at its first cause and a component that fails never lowers its children. A program names the first thing in each component the build could not render, so removing one cause can uncover the next.

## What sends a component to the browser

### Lowerable

| Cause | Programs | What the server does not render yet |
| --- | --- | --- |
| `reduce` with an accumulator it writes to, `(o[key] = 1), o`, in the grid | 2 | A write to an object inside an expression |
| `<Teleport>` to a target, in the modal | 2 | Collecting the content Vue's server writes for the target beside the page |
| A child that emits during setup, tutorial step 13 | 2 | A child's setup writing its parent's state before the rest of the parent renders |

### By design

| Cause | Programs | Why it stays in the browser |
| --- | --- | --- |
| `fetch`, in setup or in `created` | 2 | The data belongs in the route's loader |
| The current time: `performance.now()`, `new Date()` read in the viewer's time zone | 4 | The server and the browser read the clock at different moments |
| An npm package: `marked`, `js-confetti` | 4 | The build follows the application's own files, not a package's |
| `new Function`, which the cells example builds its formulas with | 2 | Code evaluated from text |

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1` and bring the counts here and in the table in [the FSR README](../../README.md#how-much-renders-on-the-server) up to that commit.
