# 003. Server rendering without a JavaScript engine

The question this chapter answers: how does a React page arrive as HTML from a server that has no JavaScript engine and what does the browser do with it?

**For:** everyone.

## Rendering models

Client-side rendering ships a blank document and a script; the browser builds the page. Server-side rendering runs the components on the server and ships HTML, then the browser downloads the same components and runs them again over the existing DOM to attach handlers, which is hydration. Islands narrow that: only the interactive components ship JavaScript and everything around them is inert HTML. Server components go further, running some components only on the server and sending the browser a tree rather than HTML.

All four are variations on a tree the server produces, where some nodes are finished markup and some name a module the browser must mount, along with the props to mount it with. fsr builds that tree and does not label a route as SSR or islands. A route gets whatever mix of nodes the plan and the evaluators produce and the browser's client reads any mix the same way. One setting moves the whole application to one end of that range, which is the last section of this chapter; everything between the ends is decided per component by what the build could lower.

## Where the markup comes from

A page in the storefront is a React component. React cannot run in Rust, so the usual approach would be a JavaScript engine on the server. fsr avoids needing one for the common case.

The build reads the page the way it reads a loader. JSX is a tree of elements, text and expressions; a `.map` over an array is a loop; a ternary with markup in a branch is a condition and so is an `if (...) return` ahead of the last `return`; a component the page imports is another tree applied to props. The build lowers all of it into a render tree in the plan file; the helper functions the page calls, `money`, `categoryLabel`, `percentOff`, are inlined as lambdas beside it. At request time the host evaluates that tree with the props the loader produced and writes HTML.

The output follows React's own server renderer in the places hydration can tell: adjacent text nodes are separated by an empty comment, empty strings write nothing, a controlled `<select>` marks its option selected. Follow those and React's hydration accepts the markup as its own and attaches its handlers without touching the DOM. Miss one and React throws the markup away and renders from scratch, which the browser console reports as a hydration mismatch. The storefront's three pages hydrate clean.

That fidelity is measured. A bench renders each of those pages twice, once through the interpreter and once through React's own `renderToString` in the QuickJS the test runner already embeds and fails on any byte of difference before it reports a number. The numbers are another reason to keep the interpreter: it is currently 1.8x ahead of React in QuickJS on the catalog and about 3.7x on the two small pages and a fresh QuickJS context costs around 20 ms to bring up before it renders anything, which is what an isolate-per-request design would pay. [docs/benches/render.md](../benches/render.md) has the method, what it does not measure and every run kept.

A component runs on the server when the build could read it. The report says which ones it could:

```
rendered  routes/page.tsx#default            lowered
          src/ui/Header.tsx#Header           lowered
          src/ui/Stars.tsx#Stars             lowered
```

A page it could not read is marked `client`, pointing at the residue that decided it, which may sit in a component the page imports, since the page cannot render without that component. A `client` section below states each such residue once, with the rewrite that does the same thing in the IR and the tags to follow from the page down to it, so when the cause is three files below a page the report gives you the path to it:

```
rendered  routes/page.tsx#default            client      src/ui/Stars.tsx:12:19
client    src/ui/Stars.tsx:12:19             `.slice()`, which is not a builtin
          the builtins are `map`, `filter`, ...; anything else goes in a module-level helper the build can read
          1 page renders in the browser for it
            routes/page.tsx#default          <Header> routes/page.tsx:22:7, <Stars> src/ui/Header.tsx:8:5
```

That page arrives as a module reference with its props. It mounts and works in the browser, but the server sends no markup for it, so there is no first paint.

## What the browser receives

The document is the shell: the head with its stylesheets and import map, then each island as an element carrying the module id, the region key the build gave that placement, the server markup when there is one and a script tag with the props. The client's boot scans for those elements and mounts each with its registered mounter, hydrating when markup is present and rendering when it is not. The island registry is generated by the build from the plan, so a page cannot fail to mount because someone mistyped its id.

The key is what pairs a placement with the markup the server rendered for it. Without one the pairing is document order, which works until the same component is placed more than once: an island inside a `.map` or behind a condition would take whichever region came next, so reordering a list would hand each island the state of its neighbour. The build numbers island placements apart from the hoisted values, so placing a component never renumbers the other.

Navigation between routes fetches the same tree in its wire form rather than as HTML. Each segment of the page carries a key naming which segment it is and a digest of what it rendered; the client walks the old and new trees together, keeping every region whose digest is unchanged and replacing the rest, so a layout's DOM and its island state survive a click and so does a pane the click did not actually change. An action that succeeds re-fetches the current route by default, which is how the storefront's header badge follows the cart without anyone wiring it.

## Static templates

A template with no state and no handlers of its own has nothing for the browser to change. The build marks it `static` in the report, writes no browser twin for it and leaves it out of the island registry; the server writes its markup with no island marker around it. A component with state that it renders inline is placed as an island of its own, so the template stays static whatever it renders. The islands inside it are mounted by the document's own scan. The storefront's catalog page and its layout are both static: each product card is an island and so is the header. An application whose templates are all static loads no framework for them, which is what lets the recipes example ship Vue islands and no React; chapter 104 is that application.

A static segment is still kept across navigation, since a segment's digest is of its own markup and a layout's does not move when the page beneath it changes. When it does move, after an action re-rendered the layout with a new count, the navigator replaces its markup but keeps every island inside it that the new markup also places, by region key, so an open panel in a static layout survives the mutation that changed the number beside it.

## Values the server computes for the browser

Hydration is React running the page again to find out where its handlers go and every helper on the render path runs with it: `money`, `percentOff`, `categoryLabel`, once in Rust for the markup and once in the browser for a tree React then discards. The two have to agree byte for byte, so the browser computes an answer the server already had.

Instead, the server sends the computed values. A call whose inputs are props only, nothing that reaches a `useState`, a store key or the request, is computed once in Rust and delivered beside the props under `$h`, keyed by the module, the call and the loop it sits in. The build writes a copy of the component for the bundle in which that call reads the delivered value and keeps the original as its fallback and snapfirec compiles the copy in place of the source. A whole subtree goes the same way when nothing in it can change in the browser, meaning it has no handler, state or island: the server records its markup and the browser hands it to React as the element's inner HTML, so React renders nothing inside it and hydrates nothing inside it. The catalog's product cards are such subtrees. What is left for React to compute at hydration is what can actually change in the browser and the report counts the rest:

```
hoisted   src/ui/ProductCard.tsx#ProductCard 5 values, 5 subtrees
          src/ui/Stars.tsx#Stars             1 value, 1 subtree
```

You do not write or enable any of this. The build does it to every component it could read and the source, the editor and `fsr check` never see the copy.

An island can go one step further and have no browser half at all: placed in server mode, its events are sent to the server, Rust runs the handler and renders the island again and the browser patches the markup it gets back into place. [Chapter 102](102-components-the-server-renders.md) shows both.

## Rendering everything in the browser

Everything above describes the default, where the build lowered a component and Rust renders it. The opposite end is a setting:

```toml
[server]
render = "islands"
```

With it, no evaluator is registered for the lowered components, so every one of them falls to the null evaluator and becomes a node naming a module with its props. The build does not change: components still lower and the report still says so. The difference is that nothing asks the interpreter to render one.

The server still does most of its work. Loaders and actions run in Rust, because they never went through an evaluator. Metadata, the store seed, the session and routing also stay on the server. What the document carries is the shell, the head, the store seed, the loader data as each island's props and one region per plan child: a bare `<sf-s>` for the page under a layout, a named one for each parallel segment. The browser mounts each module rather than hydrating it, since there is no server markup to hydrate against and a region's markup is copied into the tree the mounted component renders, so a layout that never ran on the server still receives its page.

The gain is that no component has to render twice and agree. Residue does not matter here, because no component is rendered in Rust: a component may reach for anything and there is no Rust rendering of it to disagree with. The loss is the first paint. The document has no page content until the bundle has loaded and run, which is what every client-rendered application costs. Prerendering a route means prerendering a document with no page in it.

That cost is why it is not the default. The setting suits an application behind a login where nothing is indexed and the first paint is a spinner either way. It also suits a component library that will never lower. Otherwise keep the default, where the markup comes from the server and the browser reads what the server already computed.

## Running residue on the server

Components that read state the server cannot see, that suspend, that reach into libraries the build cannot follow, are residue. Today they render in the browser only. An engine that runs residue components on the server has not been built and whether to build one is an open question, because the cost of an engine is paid per component per request and most of a storefront never needs it: the eight modules in the example lower after one helper was rewritten to avoid `new URLSearchParams`. When one arrives, a residue component will render on the server through it and the report will say so.

Whatever is decided about an engine, data resolves before render. A loader owns every await and a component is a function of its props. That is what lets Rust cache a subtree and skip evaluating it. An engine would have to keep the same rule and could not let a component fetch mid-render.

## The lab

Load the catalog and view the source. The product cards are there in the HTML, inside an element that names `routes/page.tsx#default` and is followed by the props as JSON. Open the console: no errors; clicking "Add to cart" moves the badge, so React attached its handlers over the Rust output.

Look at the props script that follows the catalog's island. Beside the products it carries `"$h"`, a table whose keys name `ProductCard` and `Stars` with an id and a loop index and whose values are the prices and the star strings the cards show, plus the inner markup of each card. That is what React read at hydration instead of calling `money` and `Math.round` for every card.

Now put `render = "islands"` under `[server]` in `config/app.toml` and restart. View the source: the shell, the head, the store seed and three elements naming `routes/layout.tsx#default`, `routes/cart/page.tsx#default` and the promo slot, each followed by its props and not one product card. The page still works and the console is still clean: click a card, the modal opens over the catalog; add to cart, the badge moves. The browser drew everything you can see, from data the server sent it. Take the line back out.

Now open [`Stars.tsx`](../../examples/shopping_react_ts/app/src/ui/Stars.tsx) and change `Math.round(rating)` to `new Intl.NumberFormat().format(rating)`. Run `fsr check app`. Every page that imports `Stars` is now marked `client`; the `client` section names that one line in `Stars.tsx` once, with all of those pages under it and the tags to follow to reach each. The pages still load and still work; view the source again and the cards are gone from the HTML, present only as props. Put `Math.round` back.
