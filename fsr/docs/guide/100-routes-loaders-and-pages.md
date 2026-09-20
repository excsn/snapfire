# 100. Routes, loaders and pages

The question this chapter answers: how does a directory become a route, where does its data come from, what does its page receive and what happens when the user clicks a link?

**For:** app developers.

## A directory is a route

Everything under `app/routes/` is a route when it holds a `page.tsx`. The directory's path is the pattern, with no name treated specially: `routes/` itself is `/`, `routes/cart/` is `/cart`, `routes/index/` is `/index`, `routes/product/[id]/` is `/product/{id}` with `id` a parameter; `[...rest]` at the end catches the remainder. A directory without a page is not a route, so a shared folder can live in the tree without becoming a URL. Names derive from paths and nothing is named twice: the loader in `routes/cart/` is the source `cart` and the one in `routes/` is `$root`, a name no directory can produce; an action exported from `routes/cart/actions.ts` as `addToCart` is `cart.addToCart`.

Beside the page a route may have a `page.loader.ts`, an `actions.ts`, an `error.tsx` and a `loading.tsx`. The build discovers all four, so adding a file is the whole of registering it. A `layout.tsx` in any directory on the way wraps the pages beneath it, with its own `layout.loader.ts`. At the top of `routes/`, beside the shared `error.tsx`, a `not-found.tsx` is the page for a path nothing matches. The report lists the pattern beside the directory:

```
routes    /                      routes
          /cart                  routes/cart
          /product/{id}          routes/product/[id]
```

A page does not have to be TSX. A directory may hold `page.tera` in place of `page.tsx` and a layout directory `layout.tera` in place of `layout.tsx`; one of each, since a directory holding both is refused naming the two files. A template is not lowered and not bundled. The stock host reads it from the file and renders it itself, with the loader's return as its context, so `page.loader.ts` and `actions.ts` sit beside it exactly as they sit beside a `page.tsx` and are lowered the same way. The report lists such a module as `template` where a TSX page is `lowered`. It needs an `fsr` built with the `tera` feature, which the published binary is; without it a `page.tera` is refused naming the feature rather than ignored. Chapter 105 has a whole application written that way.

## What a route is called

Every route has a **source id** and it is the name you see in the boot report, in a trace, in the plan and in the generated types. It is the directory's segments joined with `.`, with two markers.

`$root` is the route at `routes/` itself, since it has no segments to join. A parameter contributes `$<name>` rather than the bracketed directory: `routes/product/[id]/` is `product.$id` and `routes/docs/[...rest]/` is `docs.$rest`.

| Directory | Pattern | Source id | Props type |
| --- | --- | --- | --- |
| `routes/` | `/` | `$root` | `RootProps` |
| `routes/cart/` | `/cart` | `cart` | `CartProps` |
| `routes/product/[id]/` | `/product/{id}` | `product.$id` | `ProductIdProps` |
| `routes/admin/users/` | `/admin/users` | `admin.users` | `AdminUsersProps` |

The `$` is what makes the id injective rather than decorative. A directory name may hold alphanumerics, `_` and `-` and nothing else, so no static segment can ever produce a `$` part, which is why `routes/a/x/` and `routes/a/[x]/` cannot collide on a name. The marker is dropped when the props type is derived, so `product.$id` is `ProductIdProps` and the `$` never reaches your TypeScript.

Actions and slots extend the same id. An `addToCart` exported from `routes/cart/actions.ts` is `cart.addToCart` and a parallel slot beside a layout is `layout.<name>`. Under a site prefix the whole thing carries it, so the billing site's root is `billing:$root`.

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

## The loader can title the document

Metadata is data on the route, not a component mechanism: no framework head component, nothing rendered to find the title. A loader module may export `meta`, a function of the data `load` returned and the document's `<title>` and description meta come from it, over the title in `app.toml`.

```ts
export const meta = ({ data }: MetaCtx<DataOf<typeof load>>) => ({
  title: `${data.product.name} · Shopping`,
  description: `${data.product.name} for $${(Number(data.product.price_cents) / 100).toFixed(2)}`,
});
```

The innermost route with a `meta` wins, so a layout's loader can set a default a page's overrides. A client-side navigation retitles the document from the payload and a page behind `loading.tsx` retitles it the moment its data arrives, since the meta rides with the streamed segment. Reading `params`, `query`, `session` or `now` inside `meta` is allowed and marks the route dynamic the way it would in `load`.

## The loader can seed a store the islands share

Every island on a page is its own React root, so nothing crosses between them: no context, no lifted state. What they do share is a store, one keyed map for the document. A loader fills it the same way it titles the document. Export `store` beside `load`, a function of the data the loader returned:

```ts
export const store = ({ data }: { data: { cartCount: bigint } }) => ({ "cart/count": Number(data.cartCount) });
```

Every segment on the route may export one and they merge outermost first, so a page wins a key its layout also sets. The seed reaches the browser in the document, so a component reading a key is rendered from the same value on the server and hydrates without a flash; a navigation carries it in the payload and a streamed segment carries its own when it resolves. A `store` export names its keys as literal strings, since it runs before any component and has no imports to follow.

The storefront's root layout seeds the cart's total and the header shows it. Nothing passes it down: the header is a component inside the layout's island, the buy button is in the page's and the number they agree on is the key. [Chapter 102](102-components-the-server-renders.md) is how a component reads and writes it.

## The loader names the paths a parameter takes

A route with a parameter renders per request, since nothing says which values the parameter takes. When the set is known at build, the page loader says so with a `paths` export, a function returning one object per path:

```ts
export async function load({ params }: Ctx<"/blog/{slug}">) {
  return { post: POSTS.find((p) => p.slug === params.slug) };
}

export const paths = () => POSTS.map((p) => ({ slug: p.slug }));
```

`fsr prerender` runs it once per locale and writes the route at each path, so every post is a file and a slug outside the set still renders live. The body may read the locale and call a service, since a set can come from a backend as well as a constant; reading the request is refused at build. A `paths` belongs to a page loader on a route with a parameter and nowhere else. [Chapter 300](300-the-build-and-the-dev-loop.md) has what prerendering covers.

## The loader knows the locale

With a `[locales]` section in the configuration, every request has a locale before anything runs: the path prefix, `/fr_FR/about`, then the cookie, then `Accept-Language`, then the default, which serves unprefixed. The prefix is stripped before the route matches, so no route carries a locale segment and the loader reads `locale` beside `params`. A component reads `useLocale()`, which the build lowers, so the server renders what the browser hydrates against. A link is served as written: `/fr_FR/help` is French, `/help` is whatever the request resolves to.

```ts
export async function load({ locale, services }: Ctx) {
  return { copy: await services.content.page({ slug: "help", locale }) };
}
```

The ops console picks its language from the settings drawer, two document loads with a prefix each and the host remembers the choice in a cookie.

## The loader can know the host

`ctx.host` is the host the request named, for a deployment that answers on more than one and has to tell them apart: an absolute URL in a `rel=canonical`, a tenant read off the domain. It is `string | null`. It is null until `[server] hosts` lists the hosts, which chapter 200 covers along with what the server in front has to do for the value to mean anything.

```ts
export const meta = ({ data }: MetaCtx<Data>) => ({
  title: data.title,
  head: [canonical(`https://${data.host}/docs/${data.slug}`)],
});
```

A source reading it is never prerendered, since two configured hosts are two answers.

## The loader can read the deployment's values

`ctx.config` is `[public]` from the configuration: the values a deployment sets and a body needs, an analytics id, a feature switch, a support address. `app.toml` declares each key with the value development runs under and an overlay sets the deployment's own. The declaration types the read: a string is `string`, an integer `bigint`, a float `number`, a boolean `boolean`. A key the section does not declare is not a field of `ctx.config`, so a misspelling is a type error at build rather than a null at runtime. `fsr doctor` reports a plan reading a key the deployment's configuration lacks.

```toml
[public]
analytics_id = ""
```

```ts
export async function load({ config }: Ctx) {
  return { analyticsId: config.analytics_id };
}

export const store = ({ data }: { data: { analyticsId: string } }) => ({ "site/analytics": data.analyticsId });
```

The values are the same on every request, so a source reading only them still prerenders. They are public in the plain sense: a loader returns them, a page renders them, a store seeds the browser with them, so a secret is not one of them. Chapter 200 covers the section and its overlays.

## The page receives what the loader returned

The build infers each loader's return type and writes it to `generated/client.ts` under the route's name, so the page imports `RootProps` and receives exactly what `load` produced, typed, with the value model's shapes preserved: a contract `integer` is `bigint`, a `number` is `number`, an optional field is `| null`. There is no separate props declaration to keep in sync, because the props type is a projection of the loader.

A page is an island: it is mounted in the browser and, when the build could lower it, rendered on the server first. [Chapter 102](102-components-the-server-renders.md) is what a page may say for that to work. Either way, the page's job is to render its props; anything that changes them goes through an action and comes back through the loader.

## Errors and loading

An `error.tsx` beside a route (or `routes/error.tsx` for all of them) receives `{ error: string }` when the loader fails: a service that is down, a response the contract rejected, a `fail` the body raised. The document still renders around it, so an error page is a page with a message rather than a blank tab. The status follows the page: when the route's own page loader fails, the document answers with the kind's status, `404` for `fail("not_found", ...)`, `503` for `unavailable`, since a crawler, a cache and a browser's history read the status and not the markup. A layout's loader or a slot's failing degrades that segment and leaves the status at `200`, because the page is still the page. A client navigation to such a route gets the same status with the payload, which the navigator applies as it would any other. `fsr prerender` writes no file for a path whose loader failed.

A `loading.tsx` marks the route deferred: the document ships with the loading module in the page's slot and the real page streams in when the loader finishes, filling the slot in place. Streaming is a property of the plan, declared by the file's presence, not something the page or the loader has to do.

A `routes/not-found.tsx` answers a path no route matches. The host renders it like any page, inside the shell and hydrated, with status 404 and `params.path` carrying the path asked for; without one the answer is a line of text. It is not a route, so it has no loader and no pattern and a link to it from a page is a full load rather than a client navigation.

## A layout wraps the pages beneath it

`layout.tsx` in a routes directory renders around every page under it, the page appearing where the layout puts `children`. Its props come from `layout.loader.ts` beside it, independent of the page's loader: the storefront's header takes the cart count from the root layout's loader. No page carries it any more. Layouts nest by directory.

A layout is an island like a page. It hydrates in its own root and the page hydrates in a root inside it, so a navigation between two pages swaps the page and leaves the layout's DOM and state alone: text typed in the header's search box survives the click. When an action revalidates, the layout takes its new props and re-renders in place, so the cart count follows the mutation without the box losing its text. A layout is keyed by its module and the route parameters its loader reads, which is why a layout that reads none stays put across every page.

The line between them is firm: the page cannot read the layout's data and the layout cannot read the page's. Nothing but the session and the actions crosses it. That is what keeps a page under a React layout free to be anything.

A React layout can give that up for one thing: context. `export default tree(Layout)`, with `tree` from the React adapter, mounts the layout and its page as one React root, so a provider in the layout reaches the page and a navigation renders the new page from its props inside the live layout rather than in a root of its own. The server's markup is the same either way and the layout's state survives a click the same way. A provider tag lowers as its children, so the layout still renders on the server; a page reading the context with `useContext` renders in the browser only, as it always did. The tree reaches the page directly under the layout: a layout below it is a root of its own. The conference example declares both of its layouts this way.

## A layout has slots and a route can render into one

Three rules cover what Next calls parallel and intercepting routes. A directory is a URL and nothing else. A layout declares its holes in code. A slot that is a route of its own lives under `slots/` beside the layout.

A **parallel slot** is a segment beside the page with its own loader, loading and error boundary, rendered into a region the layout places. It lives under `slots/<name>/` beside the `layout.tsx`, holding the ordinary route files: `page.tsx`, `page.loader.ts`, `loading.tsx`, `error.tsx`. The layout places it as a prop of that name or as `<Slot name>`; either way the region is `<sf-s data-sf-name>` in the markup, which the layout's root adopts and never reconciles. Children of `<Slot>` are the fallback the region shows while nothing fills it and takes back when a navigation empties it. The prop form has no fallback to offer: the `slots/` directory that makes it a slot also puts a page on every plan under the layout, so the prop is always filled and a `{promo ?? …}` right-hand side never runs. Write `{promo}` for placement and use `<Slot name>` with children for a region that can genuinely be empty. Its props type is `Layout<Name>Props` and its source id is `layout.<name>`. It is keyed, cached and kept across navigation like any segment. The storefront's `routes/slots/promo/` shows snacks under the header on every page, loaded once per document by its own loader and stays put when the page under it changes.

The samples from here on are the storefront's, which is a React application: a project started with `fsr new <dir> --with react` or given React later with `fsr use app react`. A plain `fsr new` writes a bare application with no framework. There the same placements come from `@snapfire/fsr-authoring/template` with `Children` where a React layout writes `ReactNode`, as chapter 104 shows.

```tsx
import { Slot } from "@snapfire/fsr-client/react";

export default function Layout({ cartCount, children, promo }: LayoutProps & { children: ReactNode; promo: ReactNode }) {
  return (
    <>
      <Header cartCount={cartCount} />
      {promo}
      {children}
      <Slot name="modal" />
    </>
  );
}
```

An **intercept** is a second rendering of a URL, used when it is reached by a click rather than a document load. It is `page.<slot>.tsx` beside the route's `page.tsx`, sharing its loader. It renders into the slot of that name on the nearest layout above it that declares one, the page under that layout staying exactly as the browser has it. The storefront's `routes/product/[id]/page.modal.tsx` is a quick look: a click on a product from the catalog opens it over the catalog, the URL becomes `/product/1`, back closes it and a document load of the same URL, a shared link or a reload, is the full product page. A `loading.<slot>.tsx` beside it streams the variant behind a fallback of its own; without one it waits for its data.

The server decides. The navigator sends the document's path with every soft request as `x-sf-from`; the host matches the target's intercept and applies it when the origin's route shares the layout declaring the slot. The payload then carries the layouts down to that one with the slot filled and the page marked kept and the browser writes the region without touching anything else. Those layouts belong to the page beneath, so their loaders run under the document's request and their segment keys match what the browser holds: `ctx.query` still holds the filter the list was loaded with while a peek is open over it. A loader that wants the overlay's own request reads `ctx.address`, its `path`, `params` and `query`, which is null outside an intercept. A link opts out with `full` and names a slot outright with `into`, through `Link` from the React adapter or the `data-sf-full` and `data-sf-into` attributes on any anchor:

```tsx
import { Link } from "@snapfire/fsr-client/react";

<Link href={`/product/${id}`} full>Full details</Link>
```

The report lists each slot under `slots` by its source id and each intercept under `intercepts` as the pattern and the slot it opens in. A route may carry a variant per slot, `page.modal.tsx` and `page.drawer.tsx` side by side and the one that opens is whichever comes first in file order with a slot the live layout declares, unless a link's `into` names one. A `slots/` directory anywhere but beside a `layout.tsx`, a slot without a `page.tsx` or with routes beneath it and a variant naming a slot no layout above declares each stop the build.

## A nav marks the page it is showing

A `<Link>` whose `href` is the path the request matched carries `aria-current="page"`, written by the server and so present before any script runs. A section link asks for `match="prefix"` and is marked `aria-current="true"` while a page under it is showing, which is a different value so that a screen reader hears one current page rather than two. `match="none"` opts out and an `aria-current` written by hand stands.

```tsx
<nav>
  <Link href="/billing" match="prefix">Billing</Link>
  <Link href="/billing/overdue">Overdue</Link>
</nav>
```

```css
nav a[aria-current] { background: #eef3fc; }
nav a[aria-current="page"] { font-weight: 600; }
```

Nothing is passed down for this: a nav in a layout needs no prop and no state. A navigation re-renders the page segment and leaves the layout holding the nav exactly as it was, so the navigator re-reads the marked links itself once the page has changed. The portal marks `/billing` while a billing page is showing and the billing site, mounted under it from its own artifact, marks its own nav from the same request path.

An intercept opens a drawer or a modal over the page and puts the target's URL in the address bar. A link is judged by that address, so the modal's own link to its full page is marked while it is open and a nav item for a section the target falls outside goes dark. A nav that describes the page beneath asks to be judged by the document instead:

```tsx
<Link href="/agents" match="prefix" current="document">Agents</Link>
```

The two differ only while an intercept is open. The console's header does this: the settings drawer puts `/settings` in the address bar and `Agents` keeps its mark, while the gear that opened it, judged by the address, takes one.

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

Links are ordinary anchors. With navigation enabled, a same-origin click fetches the destination's payload rather than a new document. Every region of a page carries a segment key, the module plus the parameters and query that produced it and a digest of what it rendered; the client walks the old and new payloads together, keeping every region whose digest held and replacing the rest. A region that did not change keeps its DOM and its island state, whatever its key became, which is what lets a four-pane page change one pane on a click. The shell survives every click, since its module and inputs never change; a page's own region is replaced when its module or its parameters do, which is what a click from the catalog into a product asks for.

The key includes the query string, which is why `/` and `/?q=filament` are different segments: a search that changed the results must render the grid again. A navigation that changes only the query morphs the region instead of replacing it. The new markup is patched in: an element that stands where it stood keeps its DOM and every island it places again keeps its DOM and its state and takes its new props, so a page island keeps what its reader was doing while the results change around it. A change of path replaces the region. For one navigation, `navigate(href, true, { keep })` decides it either way, as does a `Link` with `keep` or an anchor with `data-sf-keep`. `replace: true` puts the target in place of the current history entry and `scroll: false` leaves the window where it is. Together they suit a control that navigates on every change.

The payload is usually already there when the click lands. Moving the pointer over a link, focusing it or touching it fetches its payload and holds it for thirty seconds, so the click applies what is held and the loader ran while the user was still deciding; a back or forward inside that window makes no request either. An action that revalidates drops everything held, so nothing from before the mutation is shown after it. A link whose loader should not run speculatively says `data-sf-prefetch="none"`.

## The lab

Add a route: `routes/deals/page.tsx` with a component that returns a paragraph. Run `fsr check app` and read the report: `/deals` is listed, with no source since there is no loader. Add a `page.loader.ts` that returns `{ when: ctx.now }`, check again and `deals` appears under sources as `lowered`. The page can now import `DealsProps` and it has a `when: bigint`.

Then add a `loading.tsx` beside it and start the server. Load `/deals` with the network tab open: the document arrives before the loader finishes and the page fills its slot when it does.
