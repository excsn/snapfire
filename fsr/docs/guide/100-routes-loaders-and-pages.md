# 100. Routes, loaders and pages

The question this chapter answers: how does a directory become a route, where does its data come from, what does its page receive and what happens when the user clicks a link?

**For:** app developers.

## A directory is a route

Everything under `app/routes/` is a route when it holds a `page.tsx`. The directory's path is the pattern: `routes/index/` is `/`, `routes/cart/` is `/cart`, `routes/product/[id]/` is `/product/{id}` with `id` a parameter; `[...rest]` at the end catches the remainder. A directory without a page is not a route, so a shared folder can live in the tree without becoming a URL. Names derive from paths and nothing is named twice: the loader in `routes/cart/` is the source `cart`; an action exported from `routes/cart/actions.ts` as `addToCart` is `cart.addToCart`.

Beside the page a route may have a `loader.ts`, an `actions.ts`, an `error.tsx` and a `loading.tsx`. The build discovers all four, so adding a file is the whole of registering it. The report lists the pattern beside the directory:

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

## Navigation keeps what did not change

Links are ordinary anchors. With navigation enabled, a same-origin click fetches the destination's payload rather than a new document. Every region of a page carries a segment key, the module plus the parameters and query that produced it; the client walks the old and new payloads together, replacing only the region whose key differs. A region that did not change keeps its DOM and its island state. The shell survives every click, since its module and inputs never change; a page's own region is replaced when its module or its inputs do, which is what a click from the catalog into a product asks for.

The key includes the query string, which is why `/` and `/?q=filament` are different segments: a search that changed the results must replace the grid; a key that ignored the query would patch nothing.

## The lab

Add a route: `routes/deals/page.tsx` with a component that returns a paragraph. Run `fsr check app` and read the report: `/deals` is listed, with no source since there is no loader. Add a `loader.ts` that returns `{ when: ctx.now }`, check again and `deals` appears under sources as `lowered`. The page can now import `DealsProps` and it has a `when: bigint`.

Then add a `loading.tsx` beside it and start the server. Load `/deals` with the network tab open: the document arrives before the loader finishes and the page fills its slot when it does.
