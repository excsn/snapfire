# 003. Server rendering without a JavaScript engine

The question this chapter answers: how does a React page arrive as HTML from a server that has no JavaScript engine and what does the browser do with it?

**For:** everyone.

## Rendering models

Client-side rendering ships a blank document and a script; the browser builds the page. Server-side rendering runs the components on the server and ships HTML, then the browser downloads the same components and runs them again over the existing DOM to attach handlers, which is hydration. Islands narrow that: only the interactive components ship JavaScript and everything around them is inert HTML. Server components go further, running some components only on the server and sending the browser a tree rather than HTML.

All four are variations on a tree the server produces, where some nodes are finished markup and some name a module the browser must mount, along with the props to mount it with. fsr builds that tree and does not label a route as SSR or islands. A route gets whatever mix of nodes the plan and the evaluators produce and the browser's client reads any mix the same way. Pages and layouts always render on the server; which components inside them reach the browser is decided per component by what they hold and what the build could lower.

## Where the markup comes from

A page in the storefront is a React component. React cannot run in Rust, so the usual approach would be a JavaScript engine on the server. fsr avoids needing one for the common case.

The build reads the page the way it reads a loader. JSX is a tree of elements, text and expressions; a `.map` over an array is a loop; a ternary with markup in a branch is a condition and so is an `if (...) return` ahead of the last `return`; a component the page imports is another tree applied to props. The build lowers all of it into a render tree in the plan file; the helper functions the page calls, `money`, `categoryLabel`, `percentOff`, are inlined as lambdas beside it. At request time the host evaluates that tree with the props the loader produced and writes HTML.

For a React component the output follows React's own server renderer in the places hydration can tell: adjacent text nodes are separated by an empty comment, empty strings write nothing, a controlled `<select>` marks its option selected. Follow those and React's hydration accepts the markup as its own and attaches its handlers without touching the DOM. Miss one and React throws the markup away and renders from scratch, which the browser console reports as a hydration mismatch. Every island on the storefront's three pages hydrates clean.

That fidelity is measured. A bench renders each of those pages twice, once through the interpreter and once through React's own `renderToString` in the QuickJS the test runner already embeds and fails on any byte of difference before it reports a number. The numbers are another reason to keep the interpreter: it is currently 1.8x ahead of React in QuickJS on the catalog and about 3.7x on the two small pages and a fresh QuickJS context costs around 20 ms to bring up before it renders anything, which is what an isolate-per-request design would pay. [docs/benches/render.md](../benches/render.md) has the method, what it does not measure and every run kept.

A component runs on the server when the build could read it. The report says which ones it could:

```
rendered  routes/page.tsx#default            lowered
          src/ui/Header.tsx#Header           lowered
          src/ui/Stars.tsx#Stars             lowered
```

A component it could not read is marked `client`, pointing at the residue that decided it. The page placing it still lowers: the component becomes a React island. In an application without React nothing could run it, so the build stops and says to make it lower or to write it as a Vue or Svelte component or a custom element. A `client` section below states each such residue once, with the rewrite that does the same thing in the IR and the tag that places the component, so when the cause is three files below a page the report gives you the path to it:

```
rendered  routes/page.tsx#default            lowered
          src/ui/Header.tsx#Header           lowered
          src/ui/Stars.tsx#Stars             client      src/ui/Stars.tsx:12:19
client    src/ui/Stars.tsx:12:19             `.normalize()`, which is not a builtin
          the builtins are `map`, `filter`, ...; anything else goes in a module-level helper the build can read
          1 module renders in the browser for it
            src/ui/Stars.tsx#Stars           <Stars> src/ui/Header.tsx:8:5
```

That component arrives as a module reference with its props. It mounts and works in the browser, but the server sends no markup for it, so its part of the page has no first paint. The rest of the page does.

## What the browser receives

The document is the shell: the head with its stylesheets and import map, then each island as an element carrying the module id, the region key the build gave that placement, the server markup when there is one and a script tag with the props. The client's boot scans for those elements and mounts each with its registered mounter, hydrating when markup is present and rendering when it is not. The island registry is generated by the build from the plan, so a page cannot fail to mount because someone mistyped its id.

The key is what pairs a placement with the markup the server rendered for it. Without one the pairing is document order, which works until the same component is placed more than once: an island inside a `.map` or behind a condition would take whichever region came next, so reordering a list would hand each island the state of its neighbour. The build numbers island placements apart from the hoisted values, so placing a component never renumbers the other.

Navigation between routes fetches the same tree in its wire form rather than as HTML. Each segment of the page carries a key naming which segment it is and a digest of what it rendered; the client walks the old and new trees together, keeping every region whose digest is unchanged and replacing the rest, so a layout's DOM and its island state survive a click and so does a pane the click did not actually change. An action that succeeds re-fetches the current route by default, which is how the storefront's header badge follows the cart without anyone wiring it.

## Static templates

A template with no state and no handlers of its own has nothing for the browser to change. The build marks it `static` in the report, writes no browser twin for it and leaves it out of the island registry; the server writes its markup with no island marker around it. A component with state that it renders inline is placed as an island of its own, so the template stays static whatever it renders. The islands inside it are mounted by the document's own scan. The storefront's catalog page and its layout are both static: each product card is an island and so is the header. An application whose templates are all static loads no framework for them, which is what lets the recipes example ship Vue islands and no React; chapter 104 is that application.

A page or a layout ends up static too, because the build moves what only a framework can run out of it. The smallest part of its markup that uses its state, handlers or effects becomes an island module beside it, `page.island0.tsx`, which exists only in the bundle overlay. The page places that island with the values it reads as props. The report lists each split under `extracted`:

```
extracted routes/talk/[id]/page.tsx#default  routes/talk/[id]/page.island0.tsx#default (clashes, setClashes)
```

A layout whose state reaches one of its slots cannot be split, since composition fills a slot and an island cannot take one. The report lists it under `kept` and it stays a React root.

A static segment is still kept across navigation, since a segment's digest is of its own markup and a layout's does not move when the page beneath it changes. When it does move, after an action re-rendered the layout with a new count, the navigator replaces its markup but keeps every island inside it that the new markup also places, by region key, so an open panel in a static layout survives the mutation that changed the number beside it.

## Values the server computes for the browser

Hydration is React running an island's component again to find out where its handlers go and every helper on the render path runs with it: `money`, `percentOff`, `categoryLabel`, once in Rust for the markup and once in the browser for a tree React then discards. The two have to agree byte for byte, so the browser computes an answer the server already had.

Instead, the server sends the computed values. A call whose inputs are props only, nothing that reaches a `useState`, a store key or the request, is computed once in Rust and delivered beside the props under `$h`, keyed by the module, the call and the loop it sits in. The build writes a copy of the component for the bundle in which that call reads the delivered value and keeps the original as its fallback and snapfirec compiles the copy in place of the source. A whole subtree goes the same way when nothing in it can change in the browser, meaning it has no handler, state or island: the server records its markup and the browser hands it to React as the element's inner HTML, so React renders nothing inside it and hydrates nothing inside it. The catalog's product cards are such subtrees. What is left for React to compute at hydration is what can actually change in the browser and the report counts the rest:

```
hoisted   src/ui/ProductCard.tsx#ProductCard 5 values, 5 subtrees
          src/ui/Stars.tsx#Stars             1 value, 1 subtree
```

You do not write or enable any of this. The build does it to every component it could read and the source, the editor and `fsr check` never see the copy.

An island can go one step further and have no browser half at all: placed in server mode, its events are sent to the server, Rust runs the handler and renders the island again and the browser patches the markup it gets back into place. [Chapter 102](102-components-the-server-renders.md) shows both.

## Running residue on the server

Components that read state the server cannot see, that suspend, that reach into libraries the build cannot follow, are residue. Today they render in the browser only. An engine that runs residue components on the server has not been built and whether to build one is an open question, because the cost of an engine is paid per component per request and most of a storefront never needs it: the eight modules in the example lower after one helper was rewritten to avoid `new URLSearchParams`. When one arrives, a residue component will render on the server through it and the report will say so.

Whatever is decided about an engine, data resolves before render. A loader owns every await and a component is a function of its props. That is what lets Rust cache a subtree and skip evaluating it. An engine would have to keep the same rule and could not let a component fetch mid-render.

## The lab

Load the catalog and view the source. The product cards are there in the HTML, each inside an element that names `src/ui/ProductCard.tsx#ProductCard` and is followed by its props as JSON. Nothing names `routes/page.tsx#default`: the page is composition and the browser never renders it. Open the console: no errors; clicking "Add to cart" moves the badge, so React attached its handlers over the Rust output.

Look at the props script that follows a card. Beside the product it carries `"$h"`, a table whose keys name `ProductCard` and `Stars` with an id and whose values are the price and the star string the card shows, plus the card's inner markup. That is what React read at hydration instead of calling `money` and `Math.round`.

Now open [`Stars.tsx`](../../examples/shopping_react_ts/app/src/ui/Stars.tsx) and change `Math.round(rating)` to `new Intl.NumberFormat("de-DE").format(rating)`, which names a locale where the request's should decide. Run `fsr check app`. `Stars` is now marked `client`; the `client` section names that one line in `Stars.tsx` once, with the tag that places it. The product page and its modal still lower around it. Open a product and view the source: the stars are missing from the HTML and present only as props. React draws them once the page loads, since a component that does not lower is a React island in a React application. Put `Math.round` back.
