# 004. Server components without React Server Components

The question this chapter answers: SnapFire FSR renders components on the server, streams a tree, mounts client modules into it and calls server functions by id. React Server Components do those things too. So why is there no RSC in FSR and what is the "async component" FSR does have?

**For:** everyone, especially if you are coming from Next and expect to write `async function Page()`.

## The position

SnapFire FSR strongly rejects React Server Components and will not implement it.

RSC gives you components that render on the server, a tree the browser fills in as it streams, client modules mounted into that tree and server functions you can call by id. FSR gives you all of those and the sections below show where each one lives. RSC gets there by adding a second kind of component, one that runs JavaScript on the server while it renders and awaits data as it goes. FSR leaves that out for four reasons. It needs a JavaScript engine on the server for every request. It fetches in tree order, so every level of nesting adds a round trip. A render that fetches cannot be cached by what it read. It also splits an application into two kinds of component, with a `"use client"` line between them that you place yourself.

In FSR your data comes from a loader and every loader in the subtree runs before anything renders. A page is composition, so it never goes to the browser. The build reads each component and renders it in Rust when it can. A component that cannot be read renders in the browser and the report names the line that decided it. Whatever the browser needs arrives with its markup, its props and the values the server already worked out. Navigation fetches the same tree and swaps the segments that changed. An action is a typed function you call by id. A component whose module loads on demand, the one kind of async component FSR accepts, renders on the server like any other. The rest of this chapter goes through each of these.

## RSC is five mechanisms

RSC is usually described as one feature. It is five mechanisms combined and only one of them is specific to React.

1. **Module classification.** At build time every module is tagged server-only, client-safe or both. `"use client"` is the marker. This decides what ships to the browser.
2. **A streamable tree.** The server sends a tree of finished elements, references to client modules with their props and holes to fill in later. That is the Flight format.
3. **Server execution.** Running the server side of a component to produce that tree.
4. **Client reconciliation.** The browser takes the stream, mounts the referenced modules into the holes and hydrates them.
5. **Function references.** A server function gets an id that survives serialization and becomes an endpoint. That is a server action.

Only the third needs a JavaScript engine on the server. It needs one only for components the build cannot read itself. The fourth is the only one shaped by React. It amounts to islands, with the layout arriving as a tree instead of being baked into the HTML.

## What FSR has for each one

**Classification** is the build's ownership report. Every component is marked `lowered`, `client` or `static`, which chapter 003 explains. The build also generates the island registry, the list of modules the browser needs. You never write `"use client"` because the mark comes from what the component holds. A component that reads only its props and calls helpers the build can inline renders on the server without any declaration.

**The streamable tree** is the payload, the wire form from chapter 100. A row is finished markup, an island reference with its props and region key or a pending slot that a later row fills in. Navigation fetches the same tree a document is built from. A deferred segment streams as its own row. The runtime crate's reference calls these `Raw`, `Client` and `Pending` nodes.

**Server execution** is the interpreter. The build lowers a component into the plan file: a loop for each `.map`, a condition for each ternary, a lambda for each helper. The host evaluates that with the props the loader produced. This covers every component the build can read. Across the examples that is every component except two, both of which only make sense in a browser. A component the build cannot read renders in the browser alone and the report names the line. No engine runs it on the server. An engine costs time per component per request, so a component the build cannot read stays a browser component permanently.

**Reconciliation** is the client's boot and navigator. Each island element carries its module id, its region key and its props. The boot mounts them through the registered mounter. The navigator keeps every region whose digest did not change.

**Function references** are actions. Write `action(async (ctx) => ...)` in a route's `actions.ts` and it gets an id like `cart.addToCart` in the plan file. The build writes a typed callable for it into `generated/client.ts`. The host refuses to boot if any declared action has nothing answering it. Chapter 101 covers this.

The one piece FSR leaves out is the component that runs JavaScript on the server while it renders. That piece is also where the word "async" gets confusing.

## Two things called an async component

**A component loaded on demand** is `React.lazy` or Vue's `defineAsyncComponent`, which is where Vue's name for it comes from. It exists for code splitting: a heavy component's module should not be in the first bundle. In a browser-only app that causes a loading flash, which is why both come with a Suspense boundary and a placeholder. FSR renders the component on the server anyway, so the markup is in the document on first paint. The browser fetches the module only when the island mounts:

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

The build follows the `import("literal")` wherever it sits, so the module is in the bundle and reachable. The conformance apps hydrate a lazily loaded child against the values the server rendered it from, on both adapters. The finished markup shows up at once and the module is fetched only on pages that use the component. FSR supports this kind.

**A component that awaits while rendering** is RSC's `async function Page()`:

```tsx
export default async function Talk({ id }: { id: string }) {
  const talk = await services.program.talk({ id });
  return <h2>{talk.title}</h2>;
}
```

This combines a loader and a component in one function. FSR refuses it and the error says where the await belongs:

```
routes/talk/[id]/page.tsx:11:16: an `async` component; what it awaits belongs in the route's loader, `page.loader.ts`, which runs before the page renders
```

A page is composition, so the build stops here rather than handing the page to the browser.

With the await moved into the loader, the same route works today:

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

The interpreter could run the async version. A loader body is lowered to the same IR and its awaits run in Rust; an async component's body would lower the same way. FSR refuses it because of where the await sits, since three properties depend on the await staying in the loader.

## Why the await stays in the loader

**Loads run together.** Every source in a subtree fires before any render starts. A layout's fetch never waits on a page's and a page's never waits on a card's. An await inside a render runs in tree order instead: the parent renders, then the child fetches, then the child's child fetches. That is the waterfall RSC applications are known for. The usual fixes are hoisting fetches to the top, `Promise.all` and preload calls, which rebuild by hand what a load phase already does.

**The render depends only on its data.** The host memoizes a rendered subtree under a fingerprint of what it read, which is how a second request for the same page skips evaluating it. A component that fetches mid-render reads something the fingerprint cannot see. That subtree cannot be cached and neither can anything above it.

**The browser runs the same component.** An island's component runs in the browser too. It hydrates over the server's markup and re-renders when its props or state change. React has no async client component unless you use `use` under Suspense. Vue needs a Suspense boundary for a top-level await. A component with an await in it could therefore only ever be a server component, which is why RSC draws a line between two kinds of component in one tree. In FSR every component takes its data as props, from the loader or through the store.

An awaiting component does offer colocation at the component level: a reusable `<UserBadge id>` that fetches its own data wherever you place it. An await per placement is a poor way to do that, since twenty badges would mean twenty fetches in sequence. If an app needs this, the fix would be one batched source keyed by the set of ids, decided after measuring it. None of the examples need it. A slot with its own loader covers a component that owns its data. The store covers a value that many islands read.

## What you get

A page in FSR is composition and never reaches the browser. The components inside it render on the server unless they hold something the browser has to own. The ones that do reach the browser arrive with their markup, their props and the values and subtrees the server already computed, so React only computes what can change there. Navigation is a tree diff over segments. Actions are typed calls by id. You get these without placing `"use client"`, without a second kind of component, without an engine on the server and with a render the host can cache.

## The lab

Open [`page.tsx`](../../examples/conference_react_ts/app/routes/talk/[id]/page.tsx) under `routes/talk/[id]/` and make the default export `async`, awaiting anything inside it. Run `fsr check app`. The build stops at the page, names the line and says the await belongs in `page.loader.ts`, which is where the talk is loaded today. Put it back.

Then go to the conformance app under `examples/conformance/adapters_react18/app` and run `fsr dev app`. Load `/held`. The lazy probe's markup, heading included, is in the document source before any module loads. In the network log the lazy part's module arrives after the page, when the island mounts. The console shows no hydration mismatch.
