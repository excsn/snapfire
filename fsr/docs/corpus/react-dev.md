# react.dev on the server

The corpus in `fsr/examples/corpus` builds the 641 code examples from react.dev through FSR as one application per example and records what the build made of each in `outcomes.txt`. Run it from `fsr/examples` with `cargo test -p corpus`. The counts here are from `552d3434`.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Lowered whole | 532 | the server renders every component on the page |
| Partly client side rendered | 101 | at least one component renders in the browser; the outcome names each cause |
| Refused | 3 | the build stops |
| Not a working example (excluded) | 5 | react.dev shows it broken on purpose or as an empty exercise |
| Total | 641 | |

A component that is client side rendered still works: it renders in the browser as it would under React and its HTML is missing from the first response. The corpus checks what the build lowers, not that the lowered page renders; the lowerer's own tests render each shape and the lowering conformance app's `/idioms` page runs them in a browser. A `console` call on a render path writes to the server's log on each render under the target `fsr::console` and again in the browser where the component hydrates, as Next.js and React's own server renderer do; one that logs a value only the browser knows is left to the browser. The guide's chapter 111 covers it.

## How causes hide each other

The build stops a module at its first cause and a component that fails never lowers its children. An example names the first thing in each component the build could not render, so removing one cause can uncover the next.

## What sends a component to the browser

Every cause left is blocked. The last column is what it waits on. An example can name more than one cause, so the rows add up to more than 101.

| Cause | Examples | Waits on |
| --- | --- | --- |
| `use` of a promise, including after an early `return` | 19 | Nothing on the server: the data belongs in the route's loader |
| A class component | 13 | Lowering a stateless `render()`; until then the class renders in the browser |
| `ShowRenderedHTML.js` | 9 | Not in the corpus, since react.dev's sandbox supplies it; it also renders children to a string through `react-dom/server` |
| The viewer's time zone, `toLocaleTimeString` and `getHours` | 7 | Nothing: the server does not know the viewer's zone |
| `useId` | 7 | A decision on matching the id React derives from the tree at hydration |
| A locale named where it formats, `new Intl.NumberFormat('en-US')` kept as a value among them | 7 | A decision: formatting under a named locale is refused on purpose |
| `Math.random`, three of them behind `new Array` | 7 | Nothing: nondeterministic |
| A function value into a component that renders in the browser: a render prop, a method in an object literal or an imported function passed as a prop | 7 | A design for a function crossing into a hydrating component's props |
| `createPortal` | 6 | Nothing: React's server renderer has no portals |
| An `async` component | 4 | Nothing on the server: what it awaits belongs in the loader |
| `useSyncExternalStore` without `getServerSnapshot` | 3 | Nothing: React's server renderer refuses it too |
| `++` on a module-level counter | 3 | A decision on module state evaluated at load |
| A `'use server'` action, a promise cache or a promise in state | 3 | The promise row |
| The current time on a render path | 2 | Nothing: nondeterministic |
| A write to `foo` in an error boundary demo | 2 | Nothing: the component throws on purpose |
| Setting state during render, `reference-react-usestate-14` | 1 | A decision on re-render semantics |
| `Children.forEach`, `reference-react-children-3` | 1 | The IR walking a placement's children one by one |
| `createElement`, `reference-react-createelement-1` | 1 | A rewrite to JSX the browser source rewrite can follow, since the rewritten nodes' spans would sit inside the call |
| `.get()` on `useFormStatus`'s `data`, `.entries()` on typed props, `window` | 3 | `FormData`, typed values across props and the viewer's device |

## What the build refuses

| Refusal | Examples | Waits on |
| --- | --- | --- |
| An `App.js` the extraction left empty, `reference-react-suspense-3` and `reference-react-dom-server-resume-1` | 2 | A re-fetch of the two blocks from react.dev at the commit `NOTICE.md` names; the repo has no extraction script |
| `Date.now()` on the render path of a server component, `learn-rsc-sandbox-test-6` | 1 | FSR knowing a component is server only |

The five that are not working examples are the JSX react.dev shows broken on purpose (`learn-describing-the-ui-3`; `learn-writing-markup-with-jsx-1`, `-3`) and the exercise stubs with no export (`learn-your-first-component-3`, `-10`).

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1 cargo test -p corpus` and bring the counts here up to that commit.
