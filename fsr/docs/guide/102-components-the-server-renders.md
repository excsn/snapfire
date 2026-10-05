# 102. Components the server renders

The question this chapter answers: what may a page or component say so that the server can render it, what happens to the parts only the browser can run and how do you know which is which?

**For:** app developers.

The components in this chapter are React components, the storefront's, so they need a project that has React: `fsr new <dir> --with react` or `fsr use app react` on one that started bare, since a plain `fsr new` writes no framework. A bare application writes its pages and layouts in the same dialect from `@snapfire/fsr-authoring/template`; it has no hooks and none of the islands they make, since those are React's.

## What the build can read in a component

The build reads a page as an exported function whose parameter is `props` or a destructuring of it, whose body is `const`s and `let`s, inner functions, writes to its own locals, `if` and `switch` and a `return` of JSX on each path. Most pages fit that shape. The storefront's catalog, cart and product pages, its error page and the four components under `src/ui/` all read this way; the report lists each under `rendered` as `lowered`.

Inside the JSX, the build reads JSX's own constructs:

- An element with attributes: strings, expressions or bare booleans. `className` and its relatives become their HTML names; `style` takes an object literal; `key` and `ref` are dropped since the server has no use for them.
- Text and `{expr}`. Text keeps JSX's whitespace rule and decodes entities. An expression prints as React prints it: strings and numbers as text, `null` and booleans as nothing, an array item by item.
- `c ? <a /> : <b />` and `c && <a />`, an `if`, with `null` as an empty branch.
- `xs.map((x) => <li />)`, a loop; the callback may be a block of `const`s ending in `return`.
- `<Card product={p} />`, a component from this file or an import, rendered with the props given.

The expressions in between are the same language a loader speaks, plus the pure functions a page reaches for: the `Math` functions, `toFixed`, `repeat`, `join`, `split`, `trim`, `includes`, `startsWith`, `endsWith`, `replace`, `slice`, `at`, `indexOf`, `concat`, `padStart`, `padEnd`, `substring`, `flatMap`, `toSorted`, `JSON.stringify`, `encodeURIComponent`, `toLocaleString("en-US")` and `Array.from({ length })`, plus dates in UTC, `Intl.NumberFormat` and `Intl.DateTimeFormat` under the request's locale, `Map`, `Set`, `URLSearchParams` and regular expressions without backreferences or lookaround. Every one returns what JavaScript returns, so `5 - Math.round(x)` types like JavaScript; `"a a".replace(" ", "-")` is `"a-a"`, because `replace` takes the first occurrence only. Two edges are refused rather than approximated: `split("")`, since JavaScript splits one into UTF-16 code units, plus the second argument of `split`, `startsWith` and `endsWith`.

## Helpers and imports

A page calls helpers: `money(cents)`, `categoryLabel(key)`, `percentOff(price, list)`. The build follows the import, reads the helper as a function of `const`s, `if (c) return a;` chains and a `return`, then inlines it as a lambda at the call site. A module `const` such as the category list inlines as its value. Imports are followed on first use, so a helper module that also imports a browser library, the way `feedback.ts` imports SweetAlert2, costs nothing until a render actually reads from it, which a render never does since toasts are handlers.

Imports resolve by relative path or by the aliases [chapter 302](302-imports-and-aliases.md) describes, `@src/ui/Header` or `@generated/client`. A namespace import works as a tag: `<Ui.Card>` reaches `Card` in the file `import * as Ui` names. A rest in a destructuring is the object without the named keys, so `{ className, ...rest }` spread onto an element or a component passes everything else through. A bare specifier the render reaches, a chart library say, is residue, since the build cannot read it.

One bare specifier is not residue: `@snapfire/fsr-client/std`, the standard library. `intl.number(n)` groups a number for the document's locale, `intl.currency(n, "USD")`, `intl.date(when, "long")` and `intl.plural(n)` do what their names say, `text.slug` and `text.truncate` shape strings, `time.format`, `time.add`, `time.diff` and `time.parse` work on instants in UTC and `crypto.hash` is SHA-256. Each is a pair: a Rust function the server calls and a JavaScript function the browser calls, agreeing byte for byte under the same locale, which is what lets the stars on a product card say `1,834` in English and `1 834` in French from one line of TypeScript. The same import works in a loader and so does any helper: a body follows imports the way a component does, so `count(n, "item")` from `ext/labels.ts` labels an order on the page and could label it in the loader that fetched it.

`t("help.title")` is the same idea for text: the message under that key in the locale's catalog, `locales/fr_FR.toml` beside the app, with `t("agents.watching", { count })` picking the plural form and filling `{count}`. The server reads the file, the browser reads the same table the document carried, so the console's help page is three `t` calls instead of two copies of the page.

Three members are the server's alone: `time.now`, `crypto.random` and `id.new` cannot run twice and agree, so a component's render path may not call them and the build says so, naming the line. A loader, an action, middleware and an event handler may. `ext/` is where the application's own extensions live, reached as `@ext/labels`: every export there must lower and a `native("fleet.queueLabel", f)` declaration there pairs a browser function with a Rust one the host registers under that name, which is how the console's agent rows print `3 queued` from Rust and from React alike.

## What the browser keeps

Four things in a component are the browser's and the build drops them rather than refusing them:

- **Event handlers.** Any `on*` attribute. The server writes the markup; the browser attaches the behaviour when it hydrates.
- **Inner functions.** The `add` and `search` functions the handlers call and a `const` holding an arrow. Dropped by name; a reference to one outside a handler is residue.
- **Hooks.** `const [quantity, setQuantity] = useState(1)` reads as `const quantity = 1`, which is exactly what a first render sees in the browser too. The setter is a handler. `useMemo(() => e)` reads as `e`, `useRef(x)` as `{ current: x }`, `useCallback` as a handler. `useEffect` and its layout and insertion variants are dropped whole, since the server never runs an effect and neither does React's own server renderer. `useReducer(reducer, initial)` reads as its initial state, with `dispatch` a handler. `useContext(Theme)` reads the value the nearest `<Theme.Provider value>` around it holds (inside nested components too) and the `createContext` default under none; an island is a React root of its own in the browser, so no provider outside it reaches it on the server either. A custom hook, declared in the file or imported from one under the app, is inlined where it is called: its hooks hold state of their own per call, a value it returns binds at the call site and a setter or a function it returns is a handler the browser runs. `useId` stays residue, since React derives the id from the shape of the tree at hydration.
- **Providers.** `<Theme.Provider value={mode}>` where `Theme` is a `createContext` value the file declares or imports reads as its children, the value dropped, since the server renders nothing from it. The component then hydrates, because the context exists only in the browser.

An application can also write markup it produced itself. `<div dangerouslySetInnerHTML={{ __html: body }} />` writes that string into the document as markup and renders no children, the same on the server as in React, so a page whose loader returns rendered markdown is readable before the bundle runs. Nothing escapes or sanitises it: whoever produced the string is responsible for it.

Children and spreads work as they do in React. A component that takes `children` places them with `{children}` and the build renders what the caller wrote between the tags in the caller's scope, so a layout can wrap a page without the page knowing. `<Header {...header} />` spreads an object into props and `<h1 {...attrs}>` into attributes, later entries winning the way React merges them and a spread's `className` and a literal `class` are one attribute.

Everything else outside the vocabulary is residue: `new` of something other than `Date`, `Map`, `Set` and `URLSearchParams`, a hook the build cannot follow, a member expression as a tag whose object is not a namespace import. The report says `client` and names the line. A component holding residue renders in the browser only, as a React island. An application without React has no JSX framework to run it, so the build refuses it with the ways out: make it lower or write it as a Vue or Svelte component or a custom element. The page around it still lowers. `[build] strict = true` in the configuration or `--strict` for one run refuses residue in an application with React too, so nothing falls back to the browser without the build saying so. Residue in the page's own body moves into an island beside the page, the statement or element holding it with it.

## Hoisted values and subtrees

Without hoisting, a helper call on the render path would run twice, in Rust for the markup and in React at hydration. The build checks each one: when its inputs are props only, the server computes it and the browser reads the value the server delivered, calling the helper only where the server did not, a branch the server did not take or an input that changed with browser state. A subtree with nothing the browser can change, meaning no handler, state, island or component inside it with state of its own, is delivered whole as markup. React neither renders nor hydrates inside it.

You write nothing for this. It follows from a component being a function of its props. What reaches state stays a call in the browser: `money(total * qty)` with `qty` from `useState` is computed where `qty` lives, `money(l.price)` beside it is not. A call inside a lambda, `items.map((i) => money(i)).join(", ")`, stays as written too. The report says what was hoisted per component, as values and subtrees. The storefront's cards show the shape: the price and the discount are values, the card's body is a subtree, the "Add to cart" button that carries a handler is not inside it.

## Server-mode islands

An island in browser mode has a JavaScript half that React runs. An island in server mode has none: its events go to the server, Rust runs the handler and renders the island again from the new state and the browser patches the markup that comes back into the DOM, touching only what changed. The placement chooses it:

```tsx
<Island when="visible" mode="server">
  <OrderHelp orderId={order.id} />
</Island>
```

`OrderHelp` is the same component either way, two `useState`s and a button whose `onClick` flips one and, when it is opening, counts on the other. The build lowers the handler into the plan beside the state the way it lowers a loader: a handler may be `const`s, calls to state setters, `setOpen(!open)`, `setN((prev) => prev - 1)`, `setQty(Number(e.target.value))`, calls to actions, `void save({ id })` with `save` an `action("orders.save")`, a named function by name or called, with `e.preventDefault()` allowed and dropped. An `if` with `else` around any of those lowers too, `if (!open) setAsked(asked + 1)` or `if (!ok) return`. A key set inside a branch keeps its value where the branch did not run, so the answer carries only what the click changed. An action called from a server-mode handler is dispatched by the host inside the round trip, with the session the action route would give it; the island refreshes the page's data once its patch is in, so a click in an island with no JavaScript still moves what every other island renders from. In browser mode anything else in a handler stays in the browser; in server mode it is refused at build with the line, wherever inside the island it sits. A component inside the island may hold state and handlers of its own: each instance is addressed by its place in the island's markup, its state rides in the island's under that address and a click inside it round-trips through the island. One the render stops placing starts afresh when it returns, as it would under React. The storefront's `ContactHours`, inside `OrderHelp`, is one. A slot is refused the same way, `{children}` included: a step renders the island's own component and nothing else, so anything that filled the slot at first paint would be missing from the answer and the patch would take it out of the document. An island placed inside is fine, since the patch leaves its marker and children alone and hands it the props the render gave it. The server marks each bound element, the island's initial state rides in its props and the client mounts it with no React root, so no module is loaded for it.

The cost is a round trip per event, which the island shows as `data-sf-pending` while it is out, with no optimistic guess. A toggle, a quantity, a filter or a sort order fits; a text field the user types into continuously belongs in browser mode. Neither mode is the framework's preference; the report lists what runs each way:

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

A page's own state works the same way. The build moves the smallest part of the page's markup that uses its `useState`, its handlers or its effects into an island module beside the page, `page.island0.tsx`, which exists only in the bundle overlay. The page places it with the values it reads as props. The conference talk page holds a `clashes` toggle; the report shows where it went:

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

## Writing components that lower

The pages in the storefront were written as ordinary React and seven of eight lowered on the first try. The eighth built a query string with `new URLSearchParams`, which the build cannot follow; it became a template string with `encodeURIComponent`. Write components as functions of props, keep state and effects in handlers and the server can render them. A component that needs more renders in the browser and the report says so; the build does not fail.

The rule behind this is that **a component is a function of its props**. Data comes from the loader. A component that fetched its own data could not be rendered without running it.

## The lab

Open [`Header.tsx`](../../examples/shopping_react_ts/app/src/ui/Header.tsx). It uses `useState` twice, has a `search` function and a form with `onSubmit`. Run `fsr check app` and it is `lowered`; load the catalog and view the source: the header is in the HTML with the search box's initial value and no handlers. Type in the box and submit; the browser owns that and it works.

Now open [`layout.tsx`](../../examples/shopping_react_ts/app/routes/layout.tsx), which wraps every page beneath it. It renders `<Header>` and places the page where its `children` go. In the catalog's source the header closes and `<main class="page catalog">` opens inside the layout's region and the report lists `routes/layout.tsx#default` as `lowered` beside the pages under it.

The header's badge is a store read. Load the catalog and view the source: the count is in the HTML and a `script[data-sf-store]` near the end carries the seed the layout's loader produced. Open a product and add it to the cart; the badge moves before the response arrives, because [`page.tsx`](../../examples/shopping_react_ts/app/routes/product/%5Bid%5D/page.tsx) writes the key optimistically from a root the header does not share.

Now give the header a second hook: `const ref = useRef(null)` on the form. Check again. The report marks `Header` as `client`, with the line in `Header.tsx`. The layout still lowers around it and the header mounts fresh in the browser as a React island. Remove it.

Place an order and open it. The help section at the bottom is [`OrderHelp`](../../examples/shopping_react_ts/app/src/ui/OrderHelp.tsx) in server mode; in the source its button carries `data-sf-on="click:0"` and its region `data-sf-mode="server"` and the network panel shows no module loaded for it. Click the button: one request to `/_sf/island/…` answers the new state and the markup, the contact options appear and the heading above them is the same DOM node it was. Change `mode="server"` to `mode="browser"` in the order page and rebuild: the same click is now React's, with the module loaded and hydrated and nothing in `OrderHelp` changed.
