# 004. Server components without React Server Components

The question this chapter answers: SnapFire FSR renders components on the server, streams a tree, mounts client modules into it and calls server functions by id, which is what React Server Components do. So why does FSR not have RSC and what is the "async component" it has instead?

**For:** everyone, in particular a developer arriving from Next who expects `async function Page()`.

## The position

SnapFire FSR strongly rejects React Server Components and will not implement it.

The reason is that RSC bundles two things. One is a set of outcomes: components rendered on the server, a streamed tree the browser fills in, client modules mounted into it, server functions called by id. FSR delivers every one of those; this chapter walks through where each lives. The other is the mechanism RSC uses to get there: a second kind of component that runs JavaScript on the server while it renders, awaiting data as it goes. That mechanism is the problem. It needs a JavaScript engine on the server for every request, it fetches in tree order so each level of nesting is a round trip, it makes a render impossible to cache by what it read and it splits an application into two kinds of component with a `"use client"` line between them. Every one of those costs is paid to get outcomes FSR already has without them.

This is what FSR does instead. A page is composition that never reaches the browser; its data comes from a loader that runs before the render, with every loader in the subtree running together. The build reads each component and renders it in Rust when it can, while the few it cannot read render in the browser alone with the report naming the line that decided it. The components the browser does need arrive with their markup, their props and the values the server already computed, navigation fetches the same tree and diffs it by segment and an action is a typed call by id. An async component in the one sense FSR accepts, a component whose module loads on demand, renders on the server like any other. The rest of this chapter is the detail.

## RSC is five mechanisms

React Server Components arrive as one feature, but they are five mechanisms welded together and four of them are not React.

1. **Module classification.** Every module is tagged at build time as server-only, client-safe or both, with `"use client"` as the boundary. It decides what ships to the browser.
2. **A streamable tree.** The server emits a tree of finished elements, references to client modules with their props and holes to be filled later. That is the Flight format.
3. **Server execution.** Running the server side of a component to produce that tree.
4. **Client reconciliation.** The browser receives the stream, mounts the referenced modules into the holes and hydrates them.
5. **Function references.** A server function gets an id that survives serialization and becomes an endpoint. That is a server action.

Only the third needs a JavaScript engine on the server and only for components that actually contain JavaScript the build cannot read. Only the fourth is React-shaped. Even that is islands with the layout arriving as a tree instead of baked into the HTML.

## What FSR has for each

FSR builds the four that are not React and builds the fifth without an engine.

**Classification** is the build's ownership report. Every component is marked `lowered`, `client` or `static`, which chapter 003 explains. The island registry the build generates is the list of modules the browser needs. There is no `"use client"` to write: the mark follows from what the component holds. A component that reads only its props and the helpers the build can inline is server-rendered without anyone declaring it so.

**The streamable tree** is the payload, the wire form chapter 100 describes. A row is finished markup, an island reference with its props and region key or a pending slot a later row fills. A navigation fetches the same tree a document is built from; a deferred segment streams as a row of its own. The runtime crate's reference calls these `Raw`, `Client` and `Pending` nodes.

**Server execution** is the interpreter. The build lowers a component into the plan file, a loop for each `.map`, a condition for each ternary, a lambda for each helper. The host evaluates it with the props the loader produced. That covers every component the build could read, which in the examples is every component but two, both browser-only by meaning. A component the build cannot read renders in the browser alone and the report names the line that decided it. No engine runs it on the server, by decision: the cost of an engine is paid per component per request and a residue component is the end state rather than a queue.

**Reconciliation** is the client's boot and navigator: each island element carries its module id, its region key and its props, the boot mounts them through the registered mounter and the navigator keeps every region whose digest did not change.

**Function references** are actions. `action(async (ctx) => ...)` in a route's `actions.ts` gets the id `cart.addToCart` in the plan file, the build writes a typed callable for it into `generated/client.ts` and the host refuses to boot if any declared action has nothing answering it. Chapter 101 is that mechanism.

Every outcome RSC exists for is built above. The one piece FSR does not take, the component that runs JavaScript on the server while it renders, is the piece that would undo the rest. It is also where the word "async" gets confusing.

## The async component, taken apart

Two different things are called an async component. FSR has one of them.

**A component loaded on demand** is `React.lazy` or Vue's `defineAsyncComponent`, which is where Vue's name for it comes from. It exists for code splitting: a heavy component's module should not be in the first bundle. In the browser alone that costs a loading flash, which is why both come with a Suspense boundary and a placeholder. FSR renders the component on the server regardless, so the markup is in the document on first paint. The browser fetches the module only when the island mounts:

```tsx
import { lazy } from "react";

const LazyPart = lazy(() => import("@src/ui/LazyPart"));

export default function LazyProbe({ label }: { label: string }) {
  return (
    <section className="lazy-probe">
      <h3 className="label">{label}</h3>
      <LazyPart />
    </section>
  );
}
```

```vue
<script setup lang="ts">
import { defineAsyncComponent } from "vue";

const LazyVuePart = defineAsyncComponent(() => import("@src/ui/LazyVuePart.vue"));
</script>
```

The build follows the `import("literal")` wherever it sits, so the module is in the bundle and reachable. The conformance applications hydrate a lazily loaded child against the values the server rendered it from on both adapters. The user sees the finished markup at once and the module cost is paid only where the component is used.

**A component that awaits while it renders** is RSC's `async function Page()`:

```tsx
export default async function Talk({ id }: { id: string }) {
  const talk = await services.program.talk({ id });
  return <h2>{talk.title}</h2>;
}
```

That is a loader and a component glued together. FSR refuses it with the loader named:

```
routes/talk/[id]/page.tsx:11:16: an `async` component; what it awaits belongs in the route's loader, `page.loader.ts`, which runs before the page renders
```

A page is composition, so the build stops there rather than leaving the page to the browser.

The same code, with the await moved to where it belongs, is a route the host runs today:

```ts
// routes/talk/[id]/page.loader.ts
export async function load({ params, services }: Ctx<"/talk/{id}">) {
  return { talk: await services.program.talk({ id: params.id }) };
}
```

```tsx
// routes/talk/[id]/page.tsx
export default function Talk({ talk }: { talk: Talk }) {
  return <h2>{talk.title}</h2>;
}
```

This is not a limit of the interpreter. A loader body is lowered to the same IR and its awaits run in Rust; an async component's body would lower the same way. The refusal is about where the await sits. Three properties depend on it.

## Why the await stays in the loader

**Loads run together.** Every source in a subtree fires before any render, so a layout's fetch never waits on a page's and a page's never waits on a card's. An await inside a render runs in tree order: the parent renders, then the child fetches, then its child fetches. That is the waterfall RSC is known for. The usual remedies, hoisting fetches to the top of the tree, `Promise.all`, preload calls, are the developer rebuilding by hand what a load phase gives for free.

**The render is a function of the data.** The host memoizes a rendered subtree under the fingerprint of what it read, which is what lets a request for the same page skip evaluating it. A component that fetches mid-render reads something the fingerprint cannot name, so the subtree cannot be cached. Nor can anything above it.

**The browser copy runs the same component.** An island's component runs in the browser too, hydrating over the server's markup and re-rendering when its props or its state change. React has no async client component short of `use` under Suspense; Vue needs a Suspense boundary for a top-level await. A component with an await in it could only ever be a server component, which is the line RSC has to draw between two kinds of component in one tree. FSR has one kind: a component is a function of its props. Data arrives as props from the loader or through the store.

What an awaiting component offers that FSR does not is colocation at the component level: a reusable `<UserBadge id>` that fetches for itself wherever it is placed. Even there, an await per placement is the wrong primitive, since twenty placements would mean twenty sequential fetches. The answer, if an application ever needs it, is one batched source keyed by the set of ids, decided with a number. Nothing in the examples needs it: a slot with its own loader covers a component that owns its data; the store covers a value many islands read.

## What this buys

A page in FSR is composition and never reaches the browser. The components inside it are server-rendered unless they hold something the browser must own. The ones that reach the browser arrive with their markup, their props and the values and subtrees the server already computed, so React computes only what can change there. Navigation is a tree diff over segments. Actions are typed calls by id. All of that is RSC's outcome, with no `"use client"` to place, no second kind of component, no engine on the server and a render the host can cache.

## The lab

Open [`page.tsx`](../../examples/conference_react_ts/app/routes/talk/[id]/page.tsx) under `routes/talk/[id]/` and make the default export `async`, awaiting anything inside it. Run `fsr check app`: the build stops at the page, names the line and says the await belongs in `page.loader.ts`, where the talk is loaded today. Put it back.

Now open the conformance application under `examples/conformance/adapters_react18/app` and run `fsr dev app`. Load `/held`: the lazy probe's markup is in the document source, heading included, before any module loads. In the network log the lazy part's module arrives after the page, when the island mounts; the console reports no hydration mismatch. The module loads late and the markup is in the document from the start, which is what FSR means by an async component.
