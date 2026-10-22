# 102. Components the server renders

The question this chapter answers: what may a page or component say so that the server can render it, what happens to the parts only the browser can run and how do you know which is which?

**For:** app developers.

The components in this chapter are React components, the storefront's, so they need a project that has React: `fsr new <dir> --with react` or `fsr use app react` on one that started bare, since a plain `fsr new` writes no framework. A bare application writes its pages and layouts in the same dialect from `@snapfire/fsr-authoring/template`; it has no hooks and none of the islands they make, since those are React's.

## What the build can read in a component

The build reads a page as an exported function whose parameter is `props` or a destructuring of it, whose body is `const`s and `let`s, inner functions, writes to its own locals, `if` and `switch` and a `return` of JSX on each path. An inner function that returns JSX can be called in the markup, as `{row(item)}`; a `.map` callback can hold the same statements a body does. Most pages fit that shape. The storefront's catalog, cart and product pages, its error page and the four components under `src/ui/` all read this way; `fsr build --verbose` lists each under `rendered` as `lowered`.

Inside the JSX, the build reads JSX's own constructs:

- An element with attributes: strings, expressions or bare booleans. `className` and its relatives become their HTML names; `style` takes a string or any object, a literal, a constant, a prop or a spread, each property named and given `px` the way React does; `key` and `ref` are dropped since the server has no use for them.
- Text and `{expr}`. Text keeps JSX's whitespace rule and decodes entities. An expression prints as React prints it: strings and numbers as text, `null` and booleans as nothing, an array item by item.
- `c ? <a /> : <b />` and `c && <a />`, an `if`, with `null` as an empty branch.
- `xs.map((x) => <li />)`, a loop; the callback may be a block of `const`s ending in `return`.
- `<Card product={p} />`, a component from this file or an import, rendered with the props given.
- `createElement(type, props, ...children)` from React, named or through `React.createElement`, read as the JSX it stands for: a string tag is an element, a component name or `UI.Item` a component, `null` props none, an object literal's fields the attributes and each further argument a child. A tag held in a variable stays a call.

The expressions in between are the same language a loader speaks, plus the pure functions a page reaches for: the `Math` functions, `toFixed`, `repeat`, `join`, `split`, `trim`, `includes`, `startsWith`, `endsWith`, `replace`, `slice`, `at`, `indexOf`, `concat`, `padStart`, `padEnd`, `substring`, `flatMap`, `toSorted`, `JSON.stringify`, `encodeURIComponent`, `toLocaleString("en-US")` and `Array.from({ length })`, plus dates in UTC, `Intl.NumberFormat` and `Intl.DateTimeFormat` under the request's locale or a locale written in the code, a currency with its symbol, a date by its `weekday`, `month`, `year` and `day`, a formatter kept in a `const`, `Map`, `Set`, `URLSearchParams` and its `set`, `append` and `delete` on a local, `URL` with its parts read and written on a local and regular expressions without the `y` flag or a nested unbounded quantifier such as `(a+)+`. Every one returns what JavaScript returns, so `5 - Math.round(x)` types like JavaScript; `"a a".replace(" ", "-")` is `"a-a"`, because `replace` takes the first occurrence only. Two edges are refused rather than approximated: `split("")`, since JavaScript splits one into UTF-16 code units, plus the second argument of `split`, `startsWith` and `endsWith`.

## Helpers and imports

A page calls helpers: `money(cents)`, `categoryLabel(key)`, `percentOff(price, list)`. The build follows the import, reads the helper as a function of `const`s, `if (c) return a;` chains and a `return`, then inlines it as a lambda at the call site. A module `const` such as the category list inlines as its value. Imports are followed on first use, so a helper module that also imports a browser library, the way `feedback.ts` imports SweetAlert2, costs nothing until a render actually reads from it, which a render never does since toasts are handlers.

Imports resolve by relative path or by the aliases [chapter 302](302-imports-and-aliases.md) describes, `@src/ui/Header` or `@generated/client`. A namespace import works as a tag: `<Ui.Card>` reaches `Card` in the file `import * as Ui` names. A rest in a destructuring is the object without the named keys, so `{ className, ...rest }` spread onto an element or a component passes everything else through. A bare specifier the render reaches, a chart library say, is residue, since the build cannot read it.

One bare specifier is not residue: `@snapfire/fsr-client/std`, the standard library. `intl.number(n)` groups a number for the document's locale, `intl.currency(n, "USD")`, `intl.date(when, "long")`, `intl.plural(n)`, `intl.list(items)` and `intl.compare(a, b)` do what their names say, `text.slug` and `text.truncate` shape strings, `time.format`, `time.add`, `time.diff` and `time.parse` work on instants in UTC and `crypto.hash` is SHA-256. Each is a pair: a Rust function the server calls and a JavaScript function the browser calls, agreeing byte for byte under the same locale, which is what lets the stars on a product card say `1,834` in English and `1 834` in French from one line of TypeScript. The same import works in a loader and so does any helper: a body follows imports the way a component does, so `count(n, "item")` from `ext/labels.ts` labels an order on the page and could label it in the loader that fetched it.

`t("help.title")` is the same idea for text: the message under that key in the locale's catalog, `locales/fr_FR.toml` beside the app, with `t("agents.watching", { count })` picking the plural form and filling `{count}`. The server reads the file, the browser reads the same table the document carried, so the console's help page is three `t` calls instead of two copies of the page.

Three members are the server's alone: `time.now`, `crypto.random` and `id.new` cannot run twice and agree, so a component's render path may not call them and the build says so, naming the line. A loader, an action, middleware and an event handler may. `ext/` is where the application's own extensions live, reached as `@ext/labels`: every export there must lower and a `native("fleet.queueLabel", f)` declaration there pairs a browser function with a Rust one the host registers under that name, which is how the console's agent rows print `3 queued` from Rust and from React alike.

## What the browser keeps

Six things in a component are the browser's and the build drops them rather than refusing them:

- **Event handlers.** Any `on*` attribute. The server writes the markup; the browser attaches the behaviour when it hydrates.
- **Inner functions.** The `add` and `search` functions the handlers call and a `const` holding an arrow. The browser runs them; read as a value on the server, passed to a child or held in an object, one is a function that renders nothing.
- **Hooks.** `const [quantity, setQuantity] = useState(1)` reads as `const quantity = 1`, which is exactly what a first render sees in the browser too. The setter is a handler. `useMemo(() => e)` reads as `e`, `useRef(x)` as `{ current: x }`, `useCallback` as a handler. `useEffect` and its layout and insertion variants are dropped whole, since the server never runs an effect and neither does React's own server renderer. `useReducer(reducer, initial)` reads as its initial state, with `dispatch` a handler. `useContext(Theme)` reads the value the nearest `<Theme.Provider value>` around it holds (inside nested components too) and the `createContext` default under none. React 19's `use(Theme)` reads it the same way. Either may sit inside a condition or in the markup, `{flag ? use(Theme) : "plain"}`; `use` of a promise stays residue, since the server does not await inside a component; an island is a React root of its own in the browser, so no provider outside it reaches it on the server either. A custom hook, declared in the file or imported from one under the app, is inlined where it is called: its hooks hold state of their own per call, a value it returns binds at the call site and a setter or a function it returns is a handler the browser runs. `useTransition()` reads as not pending, `useActionState(action, initial)` as `initial` with the action the browser's, `useOptimistic(value)` as `value`, `useDeferredValue(value, initial)` as `initial` when it is given, `useEffectEvent` as a handler, `useFormStatus()` as no form pending and `useSyncExternalStore` as what its `getServerSnapshot` returns, which is what React's server renderer gives each of them; without a server snapshot it stays residue, since React refuses it too. A setter or `dispatch` read as a value, passed to a child or held in a context, is a function that renders nothing, as it is in the browser. `useId()` reads as an id FSR spells from the component and its position, `_S_src/Form.tsx#default|0_`, `@c1` added under the second placement of a component: the server writes it and the browser's copy of the component computes the same string, so nothing crosses the wire and the two agree without React's tree walk. It is unique within an island, as React's is within a root. Call `useId` in the component; inside a custom hook it stays residue, so pass the id into the hook instead.
- **Setting state while rendering.** A component may call its own setter in its body, the pattern React's docs use to remember a previous prop: `if (prevCount !== count) { setPrevCount(count); setTrend(...) }`. As React does, the server runs the body again with the new state until no setter fires. It writes the markup once, from the last pass. Code in the pass that called the setter still reads the old state. A body that sets state on every pass fails the render with React's "Too many re-renders" message. A setter called inside the returned markup stays residue.
- **Form actions.** `<form action={save}>` or `<button formAction={save}>` with a function writes the placeholder `action` React's server renderer writes and the browser runs the function on submit.
- **Logging.** A `console` call writes to the server's log on every render and the browser logs again when the component hydrates; one that logs the viewer's clock or another value the server would compute differently is left to the browser. [Chapter 111](111-values-the-server-and-browser-disagree-on.md) covers both.
- **Module state.** A module-level `let` that a helper advances, `let nextId = 0` with `id: nextId++` in `createTodo`, is held in the store under `file#name`, seeded with what the module's initialisation leaves it at, so a constant like `initialTodos` built by three calls to the helper has ids 0, 1 and 2 on the server as it does in the browser. A handler calling the helper writes the store. A render calling it advances the request's own copy and the store stays as it was, so a counter that only feeds a log line reads the same on every server render.
- **The viewer's clock as text.** `{new Date().toLocaleTimeString()}` renders nothing on the server; the component hydrates with the value null, which matches, then computes it once mounted and renders again. Passed as a prop, the child renders again with it. In an attribute, in a condition or choosing markup it still sends the component to the browser. Chapter 111 has the rules.
- **Providers.** `<Theme value={mode}>` (React 19's spelling) and `<Theme.Provider value={mode}>` both provide `mode` to every read beneath them on the server, inside nested components too. The component then hydrates, because a React context is the browser's too.
- **Spellings.** The build reads each hook, context, placement and store key by what its name is bound to, not by how it is written, so `import { useState as useLocal }`, `import * as React from "react"`, a re-export through a file of your own and `const Provide = Theme.Provider` all reach the same thing. A local with an imported name, a `use` helper inside a component or a prop called `Link`, is the local.

An application can also write markup it produced itself. `<div dangerouslySetInnerHTML={{ __html: body }} />` writes that string into the document as markup and renders no children, the same on the server as in React, so a page whose loader returns rendered markdown is readable before the bundle runs. Nothing escapes or sanitises it: whoever produced the string is responsible for it.

Children and spreads work as they do in React. A component that takes `children` places them with `{children}` and the build renders what the caller wrote between the tags in the caller's scope, so a layout can wrap a page without the page knowing. `<Header {...header} />` spreads an object into props and `<h1 {...attrs}>` into attributes, later entries winning the way React merges them and a spread's `className` and a literal `class` are one attribute.

Markup can go where a value goes, as it does in React. `<Card header={<h2>{title}</h2>} />` renders the header where `Card` writes `{header}`, in the caller's scope as children are. A local works the same way: `const label = <b>{price}</b>`, a `let` chosen by branches (`let body; if (open) body = <Details />; else body = <Summary />;`) and `const items = rows.map((r) => <li key={r.id}>{r.name}</li>)` each render where they are placed. An array of markup renders its items in order, as React renders an array of children: `return [<Header key="h" />, <Main key="m" />]`, `const parts = [<b>{a}</b>, " and ", b]`, one built with `push` inside an `if` or with `concat` and one kept with `.filter(Boolean)`. `rows?.map((r) => <li>{r}</li>)` writes nothing while `rows` is missing. A tag can come from a table: `const { Heading } = toneClass[tone]` then `<Heading>` renders the `h1` or `h2` the table names, as long as every entry is a literal tag the build can read. A component passed as a prop renders where the callee places it, `<Button icon={Star} />` then `<Icon />` inside `Button`. A function passed as a prop is a value the callee calls, so a render prop works: `<List render={(item) => <b>{item}</b>} />` renders `render(item)` for each item. Children written as a function work the same way, `<Value>{(v) => <i>{v}</i>}</Value>`. `Children.count(children)` and `Children.toArray(children).length` count what the caller passed, as React counts it, so a grid can choose its columns on the server. `Children.map`, `Children.forEach`, `Children.toArray` with `slice` and the rest of the array methods, `cloneElement` and `isValidElement` work on what the caller passed too. A loop works as it does in a function: `for (const r of rows)` or `for (let i = 0; i < n; i++)` can add up totals and push markup, with `if (...) continue;` to skip an item. `xs.forEach((x, i) => …)` is the same loop, with a bare `return` in it skipping to the next item. `reverse`, `sort`, `pop` and `shift` on a local array rewrite it as `push` does. A loop in a helper that returns early gives the first value returned, as react.dev's `calculateWinner` does. A loop that stops with `break` stays with the browser. Markup inside data renders where it is read: `{ title: "Mail", icon: <i>@</i> }` passed to a `Tile` that writes `{tile.icon}` or a table of elements read by key, `icons[kind]`. Markup passed as a prop to a component that renders in the browser crosses as a region the server renders and the island adopts, so `<Toggle label={<b>Light</b>} />` arrives with the label in the HTML. A function passed to a component that renders in the browser reaches it by name: the build exports an arrow written in place from its file and the island mounts with a reference the browser resolves by importing the module, so `<TabSwitcher getHeader={(id) => id.toUpperCase()} />` works when `TabSwitcher` holds state. The arrow may read its own parameters, module-level names and globals; one that reads a prop or a local of the component is refused with that placement named, since the component is not in the browser to supply it. A function a module declares, `getHeader={header}`, is referenced by its export, which the build adds when the file does not export it. An object literal may hold methods, `{ connect() { … } }`, which are functions like any other.

`memo(f)` and `forwardRef(f)` render as `f`, with the second parameter of `forwardRef`'s function the `ref` the placement passed. `<Suspense>` renders its children inside the comments React writes for a completed boundary and never its fallback, since nothing on the server waits; `<Activity>` renders its children inside its own boundary comments unless `mode` is `"hidden"`, which React's server renders as nothing. A `throw` on a render path fails the render with its message, so a `switch` whose other cases throw renders the case taken. A class component, `class X extends Component` or `PureComponent`, renders its `render()` with `this.props` the props, `this.state` the initial state from a `state` field or the constructor and each method or arrow field a function the browser runs; it hydrates like a component with state.

Everything else outside the vocabulary is residue: `new` of something other than `Date`, `Map`, `Set`, `URL` and `URLSearchParams`, a hook the build cannot follow, a member expression as a tag whose object is not a namespace import. The report says `client` and names the line. A component holding residue renders in the browser only, as a React island. An application without React has no JSX framework to run it, so the build refuses it with the ways out: make it lower or write it as a Vue or Svelte component or a custom element. The page around it still lowers. `[build] strict = true` in the configuration or `--strict` for one run refuses residue in an application with React too, so nothing falls back to the browser without the build saying so. Residue in the page's own body moves into an island beside the page, the statement or element holding it with it.

A `Map`, a `Set`, a `Date`, a `URL` or a bigint keeps its type across a prop. The build knows the type where the value is made, `new Map(...)` or `5n` in the component that places the child and where the child declares it, `{ stats }: { stats: Map<string, number> }` or a `Props` type or interface in the same file. Either is enough: `stats instanceof Map`, `tags.has("next")`, `[...stats.entries()]`, `when.toISOString()` and `typeof big === "bigint"` lower in the child and an island mounts with a real `Map`, `Set`, `Date`, `URL` or `BigInt` in its props rather than the object, array, number or string the server works with. `instanceof` on a value the build cannot type is residue, with the hint to make the value in place or type the prop. A placement passing a `Map` where the child reads the prop as a `Set` is a residue at that placement.

## Hoisted values and subtrees

Without hoisting, a helper call on the render path would run twice, in Rust for the markup and in React at hydration. The build checks each one: when its inputs are props only, the server computes it and the browser reads the value the server delivered, calling the helper only where an input changed with browser state. A subtree with nothing the browser can change, meaning no handler, state, island or component inside it with state of its own, is delivered whole as markup. React neither renders nor hydrates inside it and the bundle carries no code for it: the server renders such a subtree in a branch on state too, whether or not the branch was taken, so the markup is there when the browser takes it. A subtree inside a loop over state stays with React, since the browser can grow the loop. The one case that keeps the subtree's code in the bundle is an island a `<Mount>` places, whose props the browser supplies.

You write nothing for this. It follows from a component being a function of its props. What reaches state stays a call in the browser: `money(total * qty)` with `qty` from `useState` is computed where `qty` lives, `money(l.price)` beside it is not. A call inside a lambda, `items.map((i) => money(i)).join(", ")`, stays as written too. `fsr build --verbose` says what was hoisted per component, as values and subtrees. The storefront's cards show the shape: the price and the discount are values, the card's body is a subtree, the "Add to cart" button that carries a handler is not inside it.

## Server-mode islands

An island in browser mode has a JavaScript half that React runs. An island in server mode has none: its events go to the server, Rust runs the handler and renders the island again from the new state and the browser patches the markup that comes back into the DOM, touching only what changed. The placement chooses it:

```tsx
<Island when="visible" mode="server">
  <OrderHelp orderId={order.id} />
</Island>
```

`OrderHelp` is the same component either way, two `useState`s and a button whose `onClick` flips one and, when it is opening, counts on the other. The build lowers the handler into the plan beside the state the way it lowers a loader: a handler may be `const`s, calls to state setters, `setOpen(!open)`, `setN((prev) => prev - 1)`, `setQty(Number(e.target.value))`, calls to actions, `void save({ id })` with `save` an `action("orders.save")`, a named function by name or called, with `e.preventDefault()` allowed and dropped. An `if` with `else` around any of those lowers too, `if (!open) setAsked(asked + 1)` or `if (!ok) return`. A key set inside a branch keeps its value where the branch did not run, so the answer carries only what the click changed. An action called from a server-mode handler is dispatched by the host inside the round trip, with the session the action route would give it; the island refreshes the page's data once its patch is in, so a click in an island with no JavaScript still moves what every other island renders from. In browser mode anything else in a handler stays in the browser; in server mode it is refused at build with the line, wherever inside the island it sits. A component inside the island may hold state and handlers of its own: each instance is addressed by its place in the island's markup, its state rides in the island's under that address and a click inside it round-trips through the island. One the render stops placing starts afresh when it returns, as it would under React. The storefront's `ContactHours`, inside `OrderHelp`, is one. A slot is refused the same way, `{children}` included: a step renders the island's own component and nothing else, so anything that filled the slot at first paint would be missing from the answer and the patch would take it out of the document. An island placed inside is fine, since the patch leaves its marker and children alone and hands it the props the render gave it. The server marks each bound element, the island's initial state rides in its props and the client mounts it with no React root, so no module is loaded for it.

The cost is a round trip per event, which the island shows as `data-sf-pending` while it is out, with no optimistic guess. A toggle, a quantity, a filter or a sort order fits; a text field the user types into continuously belongs in browser mode. Neither mode is the framework's preference; `fsr build --verbose` lists what runs each way:

```
islands   src/ui/OrderHelp.tsx#OrderHelp     server      1 handler
```

## Server-mode islands without a component

A server-mode island never ships its component, so it does not need one. The island's markup can be a template and its handlers Rust, which is the shape a Tera application wants. Nothing about the round trip changes:

```tera
{{ island(module="fleet.tera#default", props={}, state=fleet, mode="server", key="fleet") }}
```

The placement carries the state, since a template declares none. Inside `fleet.tera`, `{{ on(click="filter") }}` writes the attribute that binds a handler by name rather than by index, because there is no lowered handler list to index into:

```tera
<button value="busy" {{ on(click="filter") }}>busy</button>
```

The handler is registered on the host, one per name, answering with the state to render from next:

```rust
builder.island_handler("fleet.tera#default", "filter", |ctx, event| async move {
  let chosen = /* event.target.value, checked like any input */;
  Ok(state_with(chosen, servers(&ctx).await?))
})
```

It is ordinary Rust, so it calls services, reads the session and writes it, none of which a lowered handler may do. The example's filter re-reads its fleet on every step, so the card shows what a page render would rather than what the browser last saw.

The first paint comes from the same template, rendered by the evaluator as the page is assembled, so the card is in the document before any script runs. The browser only handles the round trip.

## Giving a component its own island

A page is composition: the server renders it and the browser never renders it again. A component with state or handlers that the page renders inline is placed as an island by the renderer, in a root of its own that mounts on load. To choose its timing, place it with `Island` from the React adapter:

```tsx
import { Island } from "@snapfire/fsr-client/react";

<Island when="visible">
  <OrderHelp orderId={order.id} />
</Island>
```

The build lowers the use: the server renders `OrderHelp` with its props as an island in a region of the page's markup and the browser mounts `OrderHelp` in its own root when it scrolls into view. `island(OrderHelp, { when: "visible" })` at module level is the same thing as a component. The storefront's order page does this for its help section, which is why the checklist's island timed on visibility is there.

A page's own state works the same way. The build moves the smallest part of the page's markup that uses its `useState`, its handlers or its effects into an island module beside the page, `page.island0.tsx`, which exists only in the bundle overlay. The page places it with the values it reads as props. The conference talk page holds a `clashes` toggle; `fsr build --verbose` shows where it went:

```
extracted routes/talk/[id]/page.tsx#default  routes/talk/[id]/page.island0.tsx#default (clashes, setClashes)
```

The child need not be React. A `.vue` file imported by a template and placed the same way is an island the build lowers through Vue's own parser: the server writes what Vue's server renderer would and Vue hydrates it or mounts it fresh where the file holds what the build does not read. Chapter 104 is that path, with the plugin that reads and compiles the file and the application that has no React in it.

## Sharing state between islands

Two islands are two roots, so a value both of them show cannot be a prop and cannot be context. It is a store key and `useStore` reads like `useState`:

```tsx
import { useStore } from "@snapfire/fsr-client/react";
import { cartCount } from "@src/store";

const [items, setItems] = useStore(cartCount, 0);
```

The build lowers the read to the route's seed with the initial value as its fallback, so the server renders the same number the browser will and the setter is a handler like any other. Writing the key re-renders every component reading it, in whichever root it sits, which is what makes the storefront's badge follow a click in the buy box.

The key has to be one the build can read: a string literal or a `key()` it can follow through an import, which is why the storefront declares its keys in [`src/store.ts`](../../examples/shopping_react_ts/app/src/store.ts) and imports them. A key computed at runtime is residue naming the line.

A mutation still goes through an action and the seed the revalidation carries is what the key ends up holding. `optimistic` puts the guess up first so the click lands before the round trip and restores what was there if the call fails:

```ts
await optimistic(cartCount, (get(cartCount) ?? 0) + quantity, () =>
  actions.cart.addToCart({ product_id: product.id, quantity: BigInt(quantity) }),
);
```

## Coming from signals

FSR has no `signal()`, because the store already does what signals are used for. A signal shared between components is a store key; a signal local to one component is `useState` in React or `ref` in Vue:

| With signals | In FSR |
|---|---|
| `const count = signal(0)` shared between components | `const count = key<number>("cart/count")` in a module, read with `useStore(count, 0)` |
| `computed(() => price.value * qty.value)` | `derive(total, [price, qty], read => (read(price) ?? 0) * (read(qty) ?? 0))` |
| `effect(() => log(count.value))` | `subscribe(count, value => log(value))` |
| `batch(() => { a.value = 1; b.value = 2; })` | `transaction(() => { set(a, 1); set(b, 2); })` |
| a signal one component owns | `useState` in React, `ref` or `computed` in Vue |

```ts
import { derive, key, subscribe, transaction, set } from "@snapfire/fsr-client/store";

export const price = key<number>("cart/price");
export const qty = key<number>("cart/qty");
export const total = key<number>("cart/total");

derive(total, [price, qty], read => (read(price) ?? 0) * (read(qty) ?? 0));
subscribe(total, value => console.log("total", value));
transaction(() => {
  set(price, 12);
  set(qty, 3);
});
```

The store does three things a signal library leaves to the application. A loader seeds it on the server, so the first paint already holds the value. A server-rendered island records the values it was rendered from, so the browser hydrates against what the server saw. `optimistic` puts a guess up while an action runs and takes it back if the action fails.

A `derive` written at module level in a file the browser loads is one the server applies too. The build lowers its compute and the host runs it after the loaders' seeds merge, so the first paint shows the derived value. A derived value beats a seed for the same key, as it does in the browser. A reader waits while a source is still pending from a streamed segment. `fsr build --verbose` lists each derived key under `derived`. A `derive` written inside a function stays the browser's, as does one whose compute does not lower. The report gives its line and the reason, with or without `--verbose`. The server holds every island that reads the key rather than painting it from a fallback. A key derived twice or derived keys that feed each other fail the build. `subscribe` and `transaction` run in the browser only.

Two things differ from signals. A key is a global string that the build has to read, so two instances of one island share it; state that belongs to one instance is `useState`, since a key built from an id at runtime is residue. An update re-renders the components that read the key through React or Vue; nothing patches a single text node the way Solid or Preact signals do.

## Writing components that lower

The pages in the storefront were written as ordinary React and seven of eight lowered on the first try. The eighth built a query string with `new URLSearchParams`, which the build cannot follow; it became a template string with `encodeURIComponent`. Write components as functions of props, keep state and effects in handlers and the server can render them. A component that needs more renders in the browser and the report says so; the build does not fail.

The rule behind this is that **a component is a function of its props**. Data comes from the loader. A component that fetched its own data could not be rendered without running it.

The build holds the rule at the door. An `async` component, `export default async function Page()` or an async arrow, is refused with the loader named: what it awaits belongs in `page.loader.ts`, which runs before the page renders. A top-level `await` in a Vue `<script setup>` gets the same answer. A component loaded on demand is a different thing and renders like any other: `const Heavy = lazy(() => import("./Heavy"))` in React or `defineAsyncComponent(() => import("./Heavy.vue"))` in Vue binds the name to that module, the server renders the placement with the rest of the page and the browser fetches the module when it hydrates, with the markup already in place.

## The lab

Open [`Header.tsx`](../../examples/shopping_react_ts/app/src/ui/Header.tsx). It uses `useState` twice, has a `search` function and a form with `onSubmit`. Run `fsr check app` and it is `lowered`; load the catalog and view the source: the header is in the HTML with the search box's initial value and no handlers. Type in the box and submit; the browser owns that and it works.

Now open [`layout.tsx`](../../examples/shopping_react_ts/app/routes/layout.tsx), which wraps every page beneath it. It renders `<Header>` and places the page where its `children` go. In the catalog's source the header closes and `<main class="page catalog">` opens inside the layout's region and `fsr build --verbose` lists `routes/layout.tsx#default` as `lowered` beside the pages under it.

The header's badge is a store read. Load the catalog and view the source: the count is in the HTML and a `script[data-sf-store]` near the end carries the seed the layout's loader produced. Open a product and add it to the cart; the badge moves before the response arrives, because [`page.tsx`](../../examples/shopping_react_ts/app/routes/product/%5Bid%5D/page.tsx) writes the key optimistically from a root the header does not share.

Now give the header a second hook: `const ref = useRef(null)` on the form. Check again. The report marks `Header` as `client`, with the line in `Header.tsx`. The layout still lowers around it and the header mounts fresh in the browser as a React island. Remove it.

Place an order and open it. The help section at the bottom is [`OrderHelp`](../../examples/shopping_react_ts/app/src/ui/OrderHelp.tsx) in server mode; in the source its button carries `data-sf-on="click:0"` and its region `data-sf-mode="server"` and the network panel shows no module loaded for it. Click the button: one request to `/_sf/island/…` answers the new state and the markup, the contact options appear and the heading above them is the same DOM node it was. Take `mode="server"` off the island in the order page and rebuild: the same click is now React's, with the module loaded and hydrated and nothing in `OrderHelp` changed.
