# react.dev on the server

The corpus in `fsr/examples/corpus` builds the 641 code examples from react.dev through FSR as one application per example and records what the build made of each in `outcomes.txt`. Run it from `fsr/examples` with `cargo test -p corpus`. The counts here are from `9e63db43`.

The aim is for the server to render every example it can. What is left at the end goes to the browser for a reason that holds in React's own server renderer too or waits on a decision named below.

| Outcome | Now | Target | Meaning |
| --- | --- | --- | --- |
| Lowered whole | 429 | 519 | the server renders every component on the page |
| Partly client side rendered | 175 | 114 | at least one component renders in the browser; the outcome names each cause |
| Refused | 32 | 3 | the build stops |
| Not a working example (excluded) | 5 | 5 | react.dev shows it broken on purpose or as an empty exercise |
| Total | 641 | 641 | |

A component that is client side rendered still works: it renders in the browser as it would under React and its HTML is missing from the first response.

The target is read from the sources and not yet built. `learn-typescript-3` and `-4`, `reference-react-dom-components-index-1` and `reference-react-suspense-13` may each show one more cause once their first is gone, which puts the worst case near 515 lowered. Lowering `console.log` on the render path to nothing on the server would add 6 and put it at 525; the HTML is the same and only the server's log line is lost.

## How causes hide each other

The build stops a module at its first cause and a component that fails never lowers its children. An example names the first thing in each component the build could not render, so removing one cause can uncover the next. 16 of the 175 name more than one cause today. The causes found behind others by reading the sources are listed as hidden below.

## Status

**Unblocked** means FSR can build the lowering now with no decision pending. **Blocked** names what it waits on. Files are under `fsr/lower/src/` unless named.

### React hooks and components

| Cause | Examples | Status | What it takes |
| --- | --- | --- | --- |
| `Suspense` | 18 | Unblocked | `component.rs` `component_module` and `component_ref`: the children inside React's `<!--$-->` and `<!--/$-->` markers, not the fallback. 16 of the 18 then stop at `use` of a promise or an `async` component |
| `use` of a promise or `Suspense` waiting on one | 9 now, 21 once `Suspense` lowers | Blocked | The data belongs in the route's loader |
| `use` of a context after a `return` | 3 | Unblocked | `component.rs` `component` and `tree_of`: `use` and `useContext` of a context are allowed after a `return`, as React allows them |
| `useEffectEvent` | 7 | Unblocked | `meaning.rs` `LIBRARY`; `let_stmt` treats it as `useCallback`, a handler the server never calls |
| `useActionState` | 7 | Unblocked | `let_stmt`, shaped like `reducer_stmt`: `[initialState, handler, false]`. All 7 then stop at a named locale in `Total.js` |
| `useId` | 7 | Blocked | A decision on matching the id React derives from the tree at hydration |
| `createPortal` | 6 | Blocked | React's server renderer has no portals |
| `Activity` | 5 | Unblocked | `component.rs` `component_ref`: the children when `mode` is not `"hidden"`, with React's markers |
| `useSyncExternalStore` with `getServerSnapshot` | 2 | Unblocked | `let_stmt` and `lib.rs` `call` for a hook's `return`: the server snapshot |
| `useSyncExternalStore` without `getServerSnapshot` | 3 | Blocked | React's server renderer refuses it too |
| `useTransition` | 4 | Unblocked | `let_stmt`: `[false, handler]`. All 4 then stop at `use` of a promise |
| `useOptimistic` | 4 | Unblocked | `let_stmt`: `[passed value, handler]` |
| `useDeferredValue` | 3 | Unblocked | `let_stmt`: the initial value when given, otherwise the value. All 3 then stop at `use` of a promise |
| `useFormStatus` (hidden) | 1 | Unblocked | `let_stmt`: `{ pending: false, data: null }` |
| `createElement` | 1 | Unblocked | `child_expr` and `holds_markup` with a meaning in `meaning.rs`: the same tree as the JSX |

### Names the build cannot bind

| Cause | Examples | Status | What it takes |
| --- | --- | --- | --- |
| `dispatch` from `useReducer` | 12 | Unblocked | `component.rs` `reducer_stmt` and `let_stmt`: bind the name to an `Expr::Handler` value, never a lambda, since `function_into_browser` refuses a lambda; `handler_prop` accepts a name in `self.handlers`; a provider's `value={dispatch}` in `element` |
| A setter or handler from context or a hook: `setCurrentUser`, `setTheme`, `setName`, `handleChange` | 5 | Unblocked | The same handler value as `dispatch` |
| A form action: `search`, `publish` | 2 | Unblocked | `component.rs` `element`: `action` and `formAction` naming a declared function lower as a handler and render the placeholder action React's server renderer writes |
| `window.devicePixelRatio` | 1 | Blocked | The viewer's device |

### Statements and expressions

| Cause | Examples | Status | What it takes |
| --- | --- | --- | --- |
| A call for its effect: `forEach`, `reverse`, `pop`, `shift`, an arrow passed to a custom hook | 13 | Unblocked | `component.rs` `rebind_stmt`: `xs.forEach(f)` and `Children.forEach(children, f)` fold as `rebind_for_of` does, a bare `return` acting as `continue`; `reverse`, `pop` and `shift` on a writable local beside `push` and `unshift`. `inline_hook`: an arrow passed to a custom hook binds as a handler, as `returned` already does |
| `console.log` on the render path | 6: 5 behind `memo`, 1 hidden behind `useOptimistic` | Blocked | A decision: lowering it to nothing keeps the HTML and loses only the server's log line |
| Setting state during render, `reference-react-usestate-14` | 1 | Blocked | A decision on re-render semantics |
| A `'use server'` action or a promise cache called for its effect | 2 | Blocked | The function value and promise rows below |
| `Array(n)` and `new Array` | 14 | Unblocked | `lib.rs` `Lowerer::call` and `new_expr`: `Array(n)` is a range of `n` nulls; `.fill(v)` joins the method dispatch |
| `return` inside a loop, `calculateWinner` (hidden behind `Array(n)`) | 8 | Blocked | A loop that returns early |
| A write at a computed index of a local (hidden behind `new Array`) | 1 | Blocked | A write the lowerer does not fold |
| An expression statement in a handler that does not lower | 3 | Unblocked | `component.rs` `lower_loaded` and `unbound_is_followable`: a global reached only from such a handler leaves the handler to the browser instead of failing the component |
| An expression statement passed into a component that hydrates or a promise in state | 3 | Blocked | The function value and promise rows |
| A `throw` in a `switch`: a statement a straight-line body cannot hold, a `case` that falls through | 9 | Unblocked | `component.rs` `tree_of`, `block_to_expr_inner` and `switch_arms`: a `throw` is a render failure on that path |
| A numeric property name | 5 | Unblocked | `lib.rs` `prop_name`: the key as JavaScript's canonical string. `fsr/ir/src/interp.rs` `index` and computed keys: a map indexed by a number reads the number's string key |
| `++` on a module-level counter | 3 | Blocked | A decision on module state evaluated at load |
| A method in an object literal | 2 | Blocked | The function value row |
| A write to something other than a local | 1 | Unblocked | The custom hook arrow in the effect row |
| A write to a parameter | 1 | Unblocked | `component.rs` `function_to_lambda`: parameters are writable |
| A bare `return` | 1 | Unblocked | `component.rs` `component`: renders nothing |
| `setTimeout` in a handler | 1 | Unblocked | The handler fallback. It then stops at `use` of a promise |
| `.has()` on a `Set` in state | 1 | Unblocked | `let_stmt` useState: record the kind from `new Set()` so `has` lowers to `Includes` |
| `.entries()` on typed values across props, `learn-rsc-sandbox-test-5` | 1 | Blocked | `Map`, `Set`, `Date` and `BigInt` props and `instanceof` |

### What the page reads from its surroundings

| Cause | Examples | Status | What it takes |
| --- | --- | --- | --- |
| The viewer's time zone, `toLocaleTimeString` and `getHours` | 7 | Blocked | The viewer's zone |
| `Math.random` | 6 | Blocked | Nondeterministic |
| The current time on a render path | 2 | Blocked | Nondeterministic |
| A locale named where it formats | 1 now, 7 once `useActionState` lowers | Blocked | Deliberate: changing it is a design decision |

### Imports and props

| Cause | Examples | Status | What it takes |
| --- | --- | --- | --- |
| `ShowRenderedHTML.js` | 9 | Blocked | Not in the corpus, since react.dev's sandbox supplies it; it also renders children to a string through `react-dom/server` |
| An `async` component | 4 | Blocked | What it awaits belongs in the loader |
| A data module, `./inspirations` and `./quotes` | 3 | Unblocked | `component.rs` `find_value`: read `export default <expr>` |
| A function passed as a prop to a component that renders in the browser, `renderItem` and `getHeader` | 3 | Blocked | A design for a function crossing into a hydrating component's props. 8 examples wait on it with the method and action rows |

## What the build refuses

| Refusal | Examples | Status | What it takes | After |
| --- | --- | --- | --- | --- |
| A class component, `class X extends Component` or `PureComponent` | 13 | Unblocked | `component.rs`: a `find_class` beside `find_function` and `decl_function`; `lower_loaded` returns `LowerError::Residue` naming the class instead of `MissingExport`. Lowering a stateless `render()` is the step after | 13 partly client side rendered |
| A component wrapped in `memo(f)` or `forwardRef(f)` | 9 | Unblocked | `component.rs`: `decl_function` and the `ExportDefaultExpr` arm of `find_function` read `memo(f)` as `f` and `forwardRef(f)` as `f`; `ComponentLowerer::bind_props` binds the second parameter to `$props.ref` | 4 lowered, 5 partly client side rendered on `console.log` |
| react.dev's TypeScript shim, `export default App = AppTSX;` in `App.js` beside `App.tsx` | 5 | Unblocked | `fsr/examples/corpus/tests/corpus.rs` `app_of`: skip `App.js` when `App.tsx` is there | 5 lowered |
| `App` exported by name only | 2 | Unblocked | `corpus.rs` `app_of`: import `{ App }` when there is no default export | 1 lowered, 1 partly client side rendered on `onspeak` not read as a handler |
| An `App.js` the extraction left empty, `reference-react-suspense-3` and `reference-react-dom-server-resume-1` | 2 | Blocked | A re-fetch of the two blocks from react.dev at the commit `NOTICE.md` names; the repo has no extraction script | Stays refused |
| `Date.now()` on the render path of a server component, `learn-rsc-sandbox-test-6` | 1 | Blocked | FSR cannot know a component is server only | Stays refused |

A class or wrapped component placed inside another is refused as "no exported" and its name, which does not say what the build could not read. The class and wrapper changes replace that message.

The five that are not working examples are the JSX react.dev shows broken on purpose (`learn-describing-the-ui-3`; `learn-writing-markup-with-jsx-1`, `-3`) and the exercise stubs with no export (`learn-your-first-component-3`, `-10`).

After a change to the lowerer moves outcomes, bless the run with `CORPUS_BLESS=1 cargo test -p corpus` and bring the counts here up to that commit.
