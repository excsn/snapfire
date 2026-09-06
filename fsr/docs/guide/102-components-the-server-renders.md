# 102. Components the server renders

The question this chapter answers: what may a page or component say so that the server can render it, what happens to the parts only the browser can run and how do you know which is which?

**For:** app developers.

## A component is a function of its props

The build reads a page as an exported function whose parameter is `props` or a destructuring of it, whose body is `const`s, inner functions and one `return` of JSX. That covers most of what a page is. The storefront's catalog, cart and product pages, its error page and the four components under `src/ui/` all read this way; the report lists each under `rendered` as `lowered`.

Inside the JSX, the vocabulary is what JSX already is:

- An element with attributes: strings, expressions or bare booleans. `className` and its relatives become their HTML names; `style` takes an object literal; `key` and `ref` are dropped since the server has no use for them.
- Text and `{expr}`. Text keeps JSX's whitespace rule and decodes entities. An expression prints as React prints it: strings and numbers as text, `null` and booleans as nothing, an array item by item.
- `c ? <a /> : <b />` and `c && <a />`, an `if`, with `null` as an empty branch.
- `xs.map((x) => <li />)`, a loop; the callback may be a block of `const`s ending in `return`.
- `<Card product={p} />`, a component from this file or an import, rendered with the props given.

The expressions in between are the same language a loader speaks, plus the pure functions a page reaches for: `Math.round`, `toFixed`, `repeat`, `join`, `trim`, `includes`, `encodeURIComponent`, `toLocaleString("en-US")` and `Array.from({ length })`. Every one returns what JavaScript returns, so `5 - Math.round(x)` types like JavaScript.

## Helpers and imports

A page calls helpers: `money(cents)`, `categoryLabel(key)`, `percentOff(price, list)`. The build follows the import, reads the helper as a function of `const`s, `if (c) return a;` chains and a `return`, then inlines it as a lambda at the call site. A module `const` such as the category list inlines as its value. Imports are followed on first use, so a helper module that also imports a browser library, the way `feedback.ts` imports SweetAlert2, costs nothing until a render actually reads from it, which a render never does since toasts are handlers.

Imports resolve by relative path or by the aliases [chapter 302](302-imports-and-aliases.md) describes, `@src/ui/Header` or `@generated/client`. A namespace import works as a tag: `<Ui.Card>` reaches `Card` in the file `import * as Ui` names. A rest in a destructuring is the object without the named keys, so `{ className, ...rest }` spread onto an element or a component passes everything else through. A bare specifier the render reaches, a chart library say, is residue, since the build cannot read it.

One bare specifier is not: `@snapfire/fsr-client/std`, the standard library. `intl.number(n)` groups a number for the document's locale, `intl.currency(n, "USD")`, `intl.date(when, "long")` and `intl.plural(n)` do what their names say, `text.slug` and `text.truncate` shape strings, `time.format`, `time.add`, `time.diff` and `time.parse` work on instants in UTC and `crypto.hash` is SHA-256. Each is a pair: a Rust function the server calls and a JavaScript function the browser calls, agreeing byte for byte under the same locale, which is what lets the stars on a product card say `1,834` in English and `1 834` in French from one line of TypeScript. The same import works in a loader, and so does any helper: a body follows imports the way a component does, so `count(n, "item")` from `ext/labels.ts` labels an order on the page and could label it in the loader that fetched it.

`t("help.title")` is the same idea for text: the message under that key in the locale's catalog, `locales/fr_FR.toml` beside the app, with `t("agents.watching", { count })` picking the plural form and filling `{count}`. The server reads the file, the browser reads the same table the document carried, so the console's help page is three `t` calls instead of two copies of the page.

Three members are the server's alone: `time.now`, `crypto.random` and `id.new` cannot run twice and agree, so a component's render path may not call them and the build says so, naming the line. A loader, an action, middleware and an event handler may. `ext/` is where the application's own extensions live, reached as `@ext/labels`: every export there must lower, and a `native("fleet.queueLabel", f)` declaration there pairs a browser function with a Rust one the host registers under that name, which is how the console's agent rows print `3 queued` from Rust and from React alike.

## What the browser keeps

Three things in a component are the browser's and the build drops them rather than refusing them:

- **Event handlers.** Any `on*` attribute. The server writes the markup; the browser attaches the behaviour when it hydrates.
- **Inner functions.** The `add` and `search` functions the handlers call, and a `const` holding an arrow. Dropped by name; a reference to one outside a handler is residue.
- **Hooks.** `const [quantity, setQuantity] = useState(1)` reads as `const quantity = 1`, which is exactly what a first render sees in the browser too, and the setter is a handler. `useMemo(() => e)` reads as `e`, `useRef(x)` as `{ current: x }`, `useCallback` as a handler. `useEffect` and its layout and insertion variants are dropped whole, since the server never runs an effect and neither does React's own server renderer.

Children and spreads are ordinary. A component that takes `children` places them with `{children}`, and the build renders what the caller wrote between the tags in the caller's scope, so a layout can wrap a page without the page knowing. `<Header {...header} />` spreads an object into props and `<h1 {...attrs}>` into attributes, later entries winning the way React merges them, and a spread's `className` and a literal `class` are one attribute.

Everything else outside the vocabulary is residue and the page renders in the browser only: `new`, `useContext` or a custom hook, `dangerouslySetInnerHTML`, a member expression as a tag whose object is not a namespace import. The report says `client` and names the line. The page still works, since it always could.

## What the browser reads instead of computing

A helper call on the render path used to run twice, in Rust for the markup and in React at hydration. Now the build looks at each one: when its inputs are props only, the server computes it and the browser reads the value the server delivered, calling the helper only where the server did not, a branch the server did not take or an input that changed with browser state. A subtree with nothing the browser can change, no handler, no state, no island, no component inside it with state of its own, is delivered whole as markup, and React neither renders nor hydrates inside it.

Nothing about that is written. The rule is the one this chapter already has: a component is a function of its props. What reaches state stays a call in the browser: `money(total * qty)` with `qty` from `useState` is computed where `qty` lives, `money(l.price)` beside it is not. A call inside a lambda, `items.map((i) => money(i)).join(", ")`, stays as written too. The report says what was hoisted, per component, as values and subtrees, and the storefront's cards show the shape: the price and the discount are values, the card's body is a subtree, the "Add to cart" button that carries a handler is not inside it.

## An island the server drives

An island in browser mode has a JavaScript half that React runs. An island in server mode has none: its events go to the server, Rust runs the handler and renders the island again from the new state, and the browser patches the markup that comes back into the DOM, touching only what changed. The placement chooses it:

```tsx
<Island when="visible" mode="server">
  <OrderHelp orderId={order.id} />
</Island>
```

`OrderHelp` is the same component either way, a `useState` and a button whose `onClick` flips it. The build lowers the handler into the plan beside the state the way it lowers a loader: a handler may be `const`s and calls to state setters, `setOpen(!open)`, `setN((prev) => prev - 1)`, `setQty(Number(e.target.value))`, a named function by name or called, with `e.preventDefault()` allowed and dropped. In browser mode anything else in a handler is simply the browser's; in server mode it is refused at build with the line, and so is a component inside the island that has state or handlers of its own, since the round trip carries one state and one set of handlers, the island's. The server marks each bound element, the island's initial state rides in its props, and the client mounts it with no React root at all, so no module is loaded for it.

The trade is a round trip per event, which the island shows as `data-sf-pending` while it is out, and no optimistic guess. A toggle, a quantity, a filter or a sort order fits; a text field the user types into continuously belongs in browser mode. Neither mode is the framework's preference; the report lists what runs each way:

```
islands   src/ui/OrderHelp.tsx#OrderHelp     server      1 handler
```

## A component as its own island

A page hydrates as one React root, so a component inside it shares that root: it re-renders with the page and hydrates when the page does. To give a component a root of its own, with its own timing and state the page never touches, place it with `Island` from the React adapter:

```tsx
import { Island } from "@snapfire/fsr-client/react";

<Island when="visible">
  <OrderHelp orderId={order.id} />
</Island>
```

The build lowers the use: the server renders `OrderHelp` with its props as a nested island in a region of the page's markup, the page's root adopts that region and never reconciles it, and the browser mounts `OrderHelp` in its own root when it scrolls into view. `island(OrderHelp, { when: "visible" })` at module level is the same thing as a component. The storefront's order page does this for its help section, which is why the checklist's island timed on visibility is there.

## State two islands share

Two islands are two roots, so a value both of them show cannot be a prop and cannot be context. It is a store key, and `useStore` reads like `useState`:

```tsx
import { useStore } from "@snapfire/fsr-client/react";
import { cartCount } from "@src/store";

const [items, setItems] = useStore(cartCount, 0);
```

The build lowers the read to the route's seed with the initial value as its fallback, so the server renders the same number the browser will, and the setter is a handler like any other. Writing the key re-renders every component reading it, in whichever root it sits, which is what makes the storefront's badge follow a click in the buy box.

The key has to be one the build can read: a string literal, or a `key()` it can follow through an import, which is why the storefront declares its keys in [`src/store.ts`](../../examples/shopping_react_ts/app/src/store.ts) and imports them. A key computed at runtime is residue naming the line.

A mutation still goes through an action, and the seed the revalidation carries is what the key ends up holding. `optimistic` puts the guess up first so the click lands before the round trip, and restores what was there if the call fails:

```ts
await optimistic(cartCount, (get(cartCount) ?? 0) + quantity, () =>
  actions.cart.addToCart({ product_id: product.id, quantity: BigInt(quantity) }),
);
```

## Writing for the server without thinking about it

The pages in the storefront were written as ordinary React and seven of eight lowered on the first try. The eighth built a query string with `new URLSearchParams`, which the build cannot follow; it became a template with `encodeURIComponent`. That is the whole cost so far: write React as a function of props, keep state and effects in handlers; the server render falls out. A component that needs more is a component the browser renders, which the report says plainly rather than a build that fails.

The one rule that matters is the invariant behind it: **a component is a function of its props**. Data comes from the loader; a component that fetched for itself could not be rendered by anything that did not run it.

## The lab

Open [`Header.tsx`](../../examples/shopping_react_ts/app/src/ui/Header.tsx). It uses `useState` twice, has a `search` function and a form with `onSubmit`. Run `fsr check app` and it is `lowered`; load the catalog and view the source: the header is in the HTML with the search box's initial value and no handlers. Type in the box and submit; the browser owns that and it works.

Now open [`layout.tsx`](../../examples/shopping_react_ts/app/routes/layout.tsx), which wraps every page beneath it. It renders `<Header>` and places the page where its `children` go. In the catalog's source the header closes and `<main class="page catalog">` opens inside the layout's region, and the report lists `routes/layout.tsx#default` as `lowered` beside the pages under it.

The header's badge is a store read. Load the catalog and view the source: the count is in the HTML, and a `script[data-sf-store]` near the end carries the seed the layout's loader produced. Open a product and add it to the cart; the badge moves before the response arrives, because [`page.tsx`](../../examples/shopping_react_ts/app/routes/product/%5Bid%5D/page.tsx) writes the key optimistically from a root the header does not share.

Now give the header a second hook: `const ref = useRef(null)` on the form. Check again. The report marks every page that renders the header as `client`, with the line in `Header.tsx`. Remove it.

Place an order and open it. The help section at the bottom is [`OrderHelp`](../../examples/shopping_react_ts/app/src/ui/OrderHelp.tsx) in server mode; in the source its button carries `data-sf-on="click:0"` and its region `data-sf-mode="server"`, and the network panel shows no module loaded for it. Click the button: one request to `/_sf/island/…` answers the new state and the markup, the contact options appear, and the heading above them is the same DOM node it was. Change `mode="server"` to `mode="browser"` in the order page and rebuild: the same click is now React's, with the module loaded and hydrated, and nothing in `OrderHelp` changed.
