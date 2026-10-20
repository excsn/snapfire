# react.dev on the server

The corpus in `fsr/examples/corpus` builds the 641 code examples from react.dev through FSR as one application per example and records what the build made of each in `outcomes.txt`. Run it from `fsr/examples` with `cargo test -p corpus`. The counts here are from `006b649d`.

| Outcome | Examples | Meaning |
| --- | --- | --- |
| Rendered on the server | 575 | the server renders every component on the page |
| Browser rendered, SSR pending | 0 | a component renders in the browser because it uses something FSR can't render on the server yet |
| Left to the browser by design | 59 | a component uses something the server can't reproduce exactly, such as the current time, `Math.random`, a promise the render awaits or a portal |
| Refused | 0 | the build stops |
| Not a working example (excluded) | 7 | react.dev shows it broken on purpose, as an empty exercise or as a page with no `App` |
| Total | 641 | |

A component rendered in the browser still works the way it does under React. Its HTML is just missing from the first response. The corpus checks what the build lowers, not that the lowered page renders; the lowerer's own tests render each shape and the lowering conformance app's `/idioms` page runs them in a browser. A `console` call on a render path writes to the server's log on each render under the target `fsr::console` and again in the browser where the component hydrates, as Next.js and React's own server renderer do; one that logs the viewer's clock or another value the server would compute differently is left to the browser. The viewer's clock written as text, `<p>{new Date().toLocaleTimeString()}</p>`, is left to the browser the same way: the server writes nothing for it, the component hydrates with the value null and computes it once mounted, which is the `useEffect` fix Next.js has you write, written by the build. Such an example counts as lowered whole. A function passed to a component that hydrates, a render prop among them, crosses as a module reference: the build exports an arrow from its file and the island imports it before it mounts, so such an example counts as lowered whole too. The guide's chapter 111 covers both.

## Only the first cause is listed

The build stops a module at its first cause. A component that fails never lowers its children. An example names the first thing in each component the build could not render, so removing one cause can uncover the next.

## What sends a component to the browser

An example can name more than one cause, so the rows add up to more than the counts above. An example with any cause in the by design table is counted there.

### By design

| Cause | Examples | Why it stays in the browser |
| --- | --- | --- |
| `use` of a promise, including after an early `return` | 19 | The data belongs in the route's loader |
| `ShowRenderedHTML.js` | 9 | Not in the corpus, since react.dev's sandbox supplies it; it also renders children to a string through `react-dom/server` |
| `Math.random`, three of them behind `new Array` | 7 | Nondeterministic |
| `createPortal` | 6 | React's server renderer has no portals |
| A `'use server'` action, a promise cache or a promise in state | 6 | The promise row |
| An `async` component | 4 | What it awaits belongs in the loader |
| `useSyncExternalStore` without `getServerSnapshot` | 3 | React's server renderer refuses it too |
| The viewer's hour choosing a class name, with `document` written during render | 2 | A value only the browser knows that shapes markup rather than text, which the first render in the browser would have to match |
| A write to `foo` in an error boundary demo | 2 | The component throws on purpose |
| `window` | 1 | The viewer's device |

## What is not a working example

These are excluded from the counts above.

| Examples | What they are |
| --- | --- |
| `learn-describing-the-ui-3`, `learn-writing-markup-with-jsx-1`, `-3` | JSX react.dev shows broken on purpose: several roots, unclosed tags, `class=` |
| `learn-your-first-component-3`, `-10` | Exercise stubs with no export |
| `reference-react-suspense-3`, `reference-react-dom-server-resume-1` | Streaming demos whose `App.js` is empty in react.dev itself; their code is the `src/index.js` the corpus leaves out, which renders with `react-dom/server` inside the browser |

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1 cargo test -p corpus` and bring the counts here and in the table in [the FSR README](../../README.md#how-much-renders-on-the-server) up to that commit.
