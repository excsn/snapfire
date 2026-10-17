# react.dev on the server

The corpus in `fsr/examples/corpus` builds the 641 code examples from react.dev through FSR as one application per example and records what the build made of each in `outcomes.txt`. Run it from `fsr/examples` with `cargo test -p corpus`. The counts here are from `1d8b297a`.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Lowered whole | 545 | the server renders every component on the page |
| Client side rendered, lowerable | 26 | a component renders in the browser because it uses something the build does not render on the server yet |
| Client side rendered by design | 63 | a component renders in the browser for a reason that holds under any server renderer: the viewer's clock, zone or device, a promise the render awaits, a portal |
| Refused | 0 | the build stops |
| Not a working example (excluded) | 7 | react.dev shows it broken on purpose, as an empty exercise or as a page with no `App` |
| Total | 641 | |

A component that is client side rendered still works: it renders in the browser as it would under React and its HTML is missing from the first response. The corpus checks what the build lowers, not that the lowered page renders; the lowerer's own tests render each shape and the lowering conformance app's `/idioms` page runs them in a browser. A `console` call on a render path writes to the server's log on each render under the target `fsr::console` and again in the browser where the component hydrates, as Next.js and React's own server renderer do; one that logs the viewer's clock or another value the server would compute differently is left to the browser. The guide's chapter 111 covers it.

## How causes hide each other

The build stops a module at its first cause and a component that fails never lowers its children. An example names the first thing in each component the build could not render, so removing one cause can uncover the next.

## What sends a component to the browser

An example can name more than one cause, so the rows add up to more than the counts above. An example with any cause in the by design table is counted there.

### Lowerable

| Cause | Examples | What the server does not render yet |
| --- | --- | --- |
| `useId` | 7 | An id that matches the one React derives from the tree at hydration |
| A locale named where it formats, `new Intl.NumberFormat('en-US')` kept as a value among them | 7 | Formatting under a locale written into the code rather than the request's |
| A function value into a component that renders in the browser: a render prop, a method in an object literal or an imported function passed as a prop | 7 | A function passed into a component that hydrates |
| `++` on a module-level counter | 3 | Module state written when the module loads |
| Setting state during render, `reference-react-usestate-14` | 1 | Rendering again after state is set during render |
| `createElement`, `reference-react-createelement-1` | 1 | `createElement` calls read as the JSX they stand for |
| `.entries()` on typed props, `learn-rsc-sandbox-test-5` | 1 | `Map`, `Set`, `Date` and `BigInt` values passed as props and read with `instanceof` |

### By design

| Cause | Examples | Why it stays in the browser |
| --- | --- | --- |
| `use` of a promise, including after an early `return` | 19 | The data belongs in the route's loader |
| `ShowRenderedHTML.js` | 9 | Not in the corpus, since react.dev's sandbox supplies it; it also renders children to a string through `react-dom/server` |
| The viewer's time zone, `toLocaleTimeString` and `getHours` | 7 | The server does not know the viewer's zone |
| `Math.random`, three of them behind `new Array` | 7 | Nondeterministic |
| `createPortal` | 6 | React's server renderer has no portals |
| An `async` component | 4 | What it awaits belongs in the loader |
| `useSyncExternalStore` without `getServerSnapshot` | 3 | React's server renderer refuses it too |
| A `'use server'` action, a promise cache or a promise in state | 3 | The promise row |
| The current time on a render path, read inline or through a helper | 3 | Nondeterministic |
| A write to `foo` in an error boundary demo | 2 | The component throws on purpose |
| `window` | 1 | The viewer's device |

## What is not a working example

These are excluded from the counts above.

| Examples | What they are |
| --- | --- |
| `learn-describing-the-ui-3`, `learn-writing-markup-with-jsx-1`, `-3` | JSX react.dev shows broken on purpose: several roots, unclosed tags, `class=` |
| `learn-your-first-component-3`, `-10` | Exercise stubs with no export |
| `reference-react-suspense-3`, `reference-react-dom-server-resume-1` | Streaming demos whose `App.js` is empty in react.dev itself; their code is the `src/index.js` the corpus leaves out, which renders with `react-dom/server` inside the browser |

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1 cargo test -p corpus` and bring the counts here up to that commit.
