# 100. Routes, loaders and pages

The question this chapter answers: how does a directory become a route, where does its data come from, what does its page receive and what happens when the user clicks a link?

**For:** app developers.

## A directory is a route

Everything under `app/routes/` is a route when it holds a `page.tsx`. The directory's path is the pattern: `routes/index/` is `/`, `routes/cart/` is `/cart`, `routes/product/[id]/` is `/product/{id}` with `id` a parameter; `[...rest]` at the end catches the remainder. A directory without a page is not a route, so a shared folder can live in the tree without becoming a URL. Names derive from paths and nothing is named twice: the loader in `routes/cart/` is the source `cart`; an action exported from `routes/cart/actions.ts` as `addToCart` is `cart.addToCart`.

Beside the page a route may have a `page.loader.ts`, an `actions.ts`, an `error.tsx` and a `loading.tsx`. The build discovers all four, so adding a file is the whole of registering it. At the top of `routes/`, beside the shared `error.tsx`, a `not-found.tsx` is the page for a path nothing matches. The report lists the pattern beside the directory:

```
routes    /                      routes/index
          /cart                  routes/cart
          /product/{id}          routes/product/[id]
```

## The loader is the page's only input

A loader exports `load` and receives the context: `params` from the pattern, `query` from the query string, `session`, `identity` and `services`. What it returns is the page's props, whole. There is no other way for data to reach a page, which is the point: a page is a function of what its loader returned, so the server can render it, cache it and reason about it.

```ts
export async function load({ params, session, services }: Ctx<"/product/{id}">) {
  const id = BigInt(params.id);
  const product = await services.shopping.getProduct({ id });
  const stock = await services.inventory.getStock({ product_id: id });
  return { product, stock, inCart: session.cart[params.id] ?? 0n, cartCount: ... };
}
```

Two things in that body are worth noticing. The two awaits do not depend on each other, so the interpreter runs them together; the loader is written in sequence and executed in parallel because the plan can see the shape. And `params.id` is a string, since every parameter is, so the body converts it before the contract sees it; a wrong conversion is a build-time type error rather than a runtime surprise.

The generated `Ctx<"/product/{id}">` knows the route's parameters, so a loader cannot read a parameter its pattern does not have. `query` is a plain record of strings, one value per key; it is how the catalog's search and category filter reach the loader: `query.q` and `query.category` are read like any other field and the recogniser treats them as request reads.

## The page receives what the loader returned

The build infers each loader's return type and writes it to `generated/client.ts` under the route's name, so the page imports `IndexProps` and receives exactly what `load` produced, typed, with the value model's shapes preserved: a contract `integer` is `bigint`, a `number` is `number`, an optional field is `| null`. There is no separate props declaration to keep in sync, because the props type is a projection of the loader.

A page is an island: it is mounted in the browser and, when the build could lower it, rendered on the server first. [Chapter 102](102-components-the-server-renders.md) is what a page may say for that to work. Either way, the page's job is to render its props; anything that changes them goes through an action and comes back through the loader.

## Errors and loading

An `error.tsx` beside a route (or `routes/error.tsx` for all of them) receives `{ error: string }` when the loader fails: a service that is down, a response the contract rejected, a `fail` the body raised. The document still renders around it, so an error page is a page with a message rather than a blank tab.

A `loading.tsx` marks the route deferred: the document ships with the loading module in the page's slot and the real page streams in when the loader finishes, filling the slot in place. Streaming is a property of the plan, declared by the file's presence, not something the page or the loader has to do.

A `routes/not-found.tsx` answers a path no route matches. The host renders it like any page, inside the shell and hydrated, with status 404 and `params.path` carrying the path asked for; without one the answer is a line of text. It is not a route, so it has no loader and no pattern, and a link to it from a page is a full load rather than a client navigation.

## A handler answers with a value

A directory may hold a `route.ts` instead of a page. Its exports named `GET`, `POST`, `PUT`, `PATCH` or `DELETE` are handlers: the host matches the method and the pattern before any page, runs the body with the same context a loader gets plus the request body as `input` and answers with the returned value as JSON. A `fail` sets the status by its kind. Written as an `action<T>`, a handler has its input checked against `T` before the body runs; written as a plain function, `input` is whatever the request carried.

```ts
export async function GET({ session }: Ctx<"/api/cart">) {
  return { lines: session.cart };
}

export const POST = action(async ({ input, session }: ActionCtx<AddToCart>) => { ... });
```

This is the API route: the same session, identity and services as a page, reached by anything that speaks HTTP. A body test imports a method from the route file the way it imports `load` from a loader. A page spec can `fetch` it. A directory is a page or a handler, never both.

## Middleware runs first

`middleware.ts` at the top of the app exports a function that runs before every request that is not a static file: the action route, the handlers and the pages alike. It reads `request.method` and `request.path` beside the session, the identity and the services. What it returns decides the request. Nothing continues. `redirect` answers with a location. `rewrite` serves another path under the same URL. `status` with a `body` answers outright. `headers` join whatever response follows.

```ts
export async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path.startsWith("/account") && !identity) return { redirect: "/login" };
  return { headers: { "x-frame-options": "DENY" } };
}
```

The storefront's middleware sends the old `/basket` to `/cart`, serves `/shop` as the catalog and stamps every response. Under `fsr test`, a body test runs it with a `request` in its context, a spec's `fetch` goes through it and `load` follows the redirect and reports where it landed.

## Navigation keeps what did not change

Links are ordinary anchors. With navigation enabled, a same-origin click fetches the destination's payload rather than a new document. Every region of a page carries a segment key, the module plus the parameters and query that produced it; the client walks the old and new payloads together, replacing only the region whose key differs. A region that did not change keeps its DOM and its island state. The shell survives every click, since its module and inputs never change; a page's own region is replaced when its module or its inputs do, which is what a click from the catalog into a product asks for.

The key includes the query string, which is why `/` and `/?q=filament` are different segments: a search that changed the results must replace the grid; a key that ignored the query would patch nothing.

The payload is usually already there when the click lands. Moving the pointer over a link, focusing it or touching it fetches its payload and holds it for thirty seconds, so the click applies what is held and the loader ran while the user was still deciding; a back or forward inside that window makes no request either. An action that revalidates drops everything held, so nothing from before the mutation is shown after it. A link whose loader should not run speculatively says `data-sf-prefetch="none"`.

## The lab

Add a route: `routes/deals/page.tsx` with a component that returns a paragraph. Run `fsr check app` and read the report: `/deals` is listed, with no source since there is no loader. Add a `page.loader.ts` that returns `{ when: ctx.now }`, check again and `deals` appears under sources as `lowered`. The page can now import `DealsProps` and it has a `when: bigint`.

Then add a `loading.tsx` beside it and start the server. Load `/deals` with the network tab open: the document arrives before the loader finishes and the page fills its slot when it does.
