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

Imports resolve by relative path or by the aliases [chapter 302](302-imports-and-aliases.md) describes, `@src/ui/Header` or `@generated/client`. A bare specifier the render reaches, a chart library say, is residue, since the build cannot read it.

## What the browser keeps

Three things in a component are the browser's and the build drops them rather than refusing them:

- **Event handlers.** Any `on*` attribute. The server writes the markup; the browser attaches the behaviour when it hydrates.
- **Inner functions.** The `add` and `search` functions the handlers call. Dropped by name; a reference to one outside a handler is residue.
- **`useState`.** `const [quantity, setQuantity] = useState(1)` reads as `const quantity = 1`, which is exactly what a first render sees in the browser too. The setter is a handler.

Everything else outside the vocabulary is residue and the page renders in the browser only: `new`, a hook other than `useState`, `dangerouslySetInnerHTML`, children or a spread passed to a lowered component, a member expression as a tag. The report says `client` and names the line. The page still works, since it always could.

## Writing for the server without thinking about it

The pages in the storefront were written as ordinary React and seven of eight lowered on the first try. The eighth built a query string with `new URLSearchParams`, which the build cannot follow; it became a template with `encodeURIComponent`. That is the whole cost so far: write React as a function of props, keep state and effects in handlers; the server render falls out. A component that needs more is a component the browser renders, which the report says plainly rather than a build that fails.

The one rule that matters is the invariant behind it: **a component is a function of its props**. Data comes from the loader; a component that fetched for itself could not be rendered by anything that did not run it.

## The lab

Open [`Header.tsx`](../../examples/shopping_react_ts/app/src/ui/Header.tsx). It uses `useState` twice, has a `search` function and a form with `onSubmit`. Run `fsr check app` and it is `lowered`; load the catalog and view the source: the header is in the HTML with the search box's initial value and no handlers. Type in the box and submit; the browser owns that and it works.

Now give the header a second hook: `const ref = useRef(null)` on the form. Check again. The report marks every page that renders the header as `client`, with the line in `Header.tsx`. Remove it.
