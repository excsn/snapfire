# Usage Guide: snapfire_fsr_cli

How to lay out an application's routes, clients and schemas, run a build and read the report and the files it writes. What to do when a body is residue.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Laying Out Routes](#laying-out-routes)
* [Writing a Loader](#writing-a-loader)
* [Writing a Layout](#writing-a-layout)
* [Filling a Layout's Slots](#filling-a-layouts-slots)
* [Writing Actions](#writing-actions)
* [Writing a Route Handler](#writing-a-route-handler)
* [Writing Middleware](#writing-middleware)
* [Importing a Service](#importing-a-service)
* [Declaring the Session and Action Inputs](#declaring-the-session-and-action-inputs)
* [Reading the Generated Types](#reading-the-generated-types)
* [Registering the Islands](#registering-the-islands)
* [Typing Pages and Calling Actions](#typing-pages-and-calling-actions)
* [Vendoring a Package](#vendoring-a-package)
* [Fetching Declarations](#fetching-declarations)
* [Reading the Generated tsconfig](#reading-the-generated-tsconfig)
* [Using xwpm Instead](#using-xwpm-instead)
* [Building](#building)
* [Serving Without a Rust Project](#serving-without-a-rust-project)
* [Prerendering](#prerendering)
* [Reading the Report](#reading-the-report)
* [Reading the Plan File](#reading-the-plan-file)
* [Naming the Shell](#naming-the-shell)
* [Answering a Residue Diagnostic](#answering-a-residue-diagnostic)
* [Building From Rust](#building-from-rust)
* [Error Handling](#error-handling)

## Core Concepts

* **App directory** is the directory that holds `routes/`; `fsr` writes `generated/` under it.
* **Route** is a directory under `routes/` that holds `page.tsx`.
* **Pattern** comes from the directory path: `index` is `/`, `[id]` is `{id}`, `[...rest]` is `{*rest}`.
* **Source id** is the route's static segments joined with `.`, `index` for the root; it names the loader.
* **Action id** is `<source id>.<export>`, so `routes/cart/actions.ts` exporting `checkout` is `cart.checkout`.
* **Middleware** is `middleware.ts` at the top of the app, run before every request that is not a static file with the request line as `request`; it continues, redirects, rewrites, responds or adds headers.
* **Handler** is an export of a directory's `route.ts` named `GET`, `POST`, `PUT`, `PATCH` or `DELETE`, answered with JSON rather than a document; its id is `<route id>.<METHOD>`. A directory is a page or a handler, never both.
* **Layout** is `layout.tsx` in a routes directory: an island that wraps every page beneath it and renders the page where it puts `children`; its `layout.loader.ts` is its loader, named for the module it feeds the way `page.loader.ts` is.
* **Slot** is a named region a layout places beside its page, as a prop of that name or `<Slot name>`. A parallel slot is `slots/<name>/` beside the layout, holding the ordinary route files, with the source id `layout.<name>`.
* **Variant** is `page.<slot>.tsx` beside a route's `page.tsx`: the rendering a soft navigation opens in that slot of the nearest layout declaring it, sharing the page's loader. It streams behind `loading.<slot>.tsx` when there is one.
* **Module id** is the page's path with `#default`, `routes/cart/page.tsx#default`; the client registers islands under it.
* **Shell** is the module every route's root node renders through, `shell#document` unless told otherwise.
* **Error module** is a route's own `error.tsx`, falling back to `routes/error.tsx` for every page.
* **Loading module** is a route's `loading.tsx`; its presence marks the node deferred with that fallback.
* **Not-found module** is `routes/not-found.tsx`, rendered with status 404 for a path no route matches; it receives `params.path`.
* **Lowered** is the owner of every source and action the build emits; the host may override any of them in Rust.
* **Client** is a document under `clients/`, `<name>.openapi.json` or `<name>.proto`, imported as the service `<name>`; the host reaches the first over HTTP and the second over gRPC.
* **Schema** is a TypeScript module under `schemas/` whose exported interfaces become contract types; one named `Session` types `ctx.session`.
* **Generated** is `generated/`, rewritten on every build: `plan.json`, `services.d.ts`, `fsr.ts`, `contracts/`, `islands.ts` and `client.ts`, plus `tsconfig.json` and `tsconfig.build.json` beside it. All of it is build output, ignored by git and rebuilt by a `build.rs` that calls the library. The tsconfig maps `@snapfire/fsr` to `generated/fsr`, so a body imports `Ctx`, `action` and `fail` from that bare name and gets the app's own types.
* **Vendor** is `vendor/`, committed: the runtime modules the browser loads, one directory per package, written by `fsr add` from esm.sh and named in `importmap.json`.
* **Types** is `types/`, gitignored: the declarations of every package the import map names, one directory per package, written by `fsr types` and path-mapped by the generated tsconfig. Never served, never load-bearing.
* **Layout** is where those live: `vendor/`, `types/`, `importmap.json` and `/static/js/vendor` by default; an `xwpm.wmf` in the app names its own.

## Quick Start

```
app/
  clients/   shopping.openapi.json
  schemas/   session.ts  cart.ts
  routes/
    error.tsx
    not-found.tsx
    layout.tsx     layout.loader.ts
    slots/promo/   page.tsx  page.loader.ts
    index/         page.tsx  page.loader.ts
    product/[id]/  page.tsx  page.modal.tsx  page.loader.ts
    cart/          page.tsx  page.loader.ts  actions.ts
```

```sh
fsr build app
```

```
routes    /                      routes/index
          /cart                  routes/cart
          /product/{id}          routes/product/[id]
sources   index                  lowered     routes/index/page.loader.ts
          cart                   lowered     routes/cart/page.loader.ts
          product                lowered     routes/product/[id]/page.loader.ts
actions   cart.addToCart         lowered     routes/cart/actions.ts
          cart.checkout          lowered     routes/cart/actions.ts
services  shopping               http        clients/shopping.openapi.json
schemas   AddToCart              schemas/cart.ts
          Session                schemas/session.ts
wrote app/generated/plan.json
wrote app/generated/contracts/shopping.json
wrote app/generated/contracts/schemas.json
wrote app/generated/services.d.ts
wrote app/generated/fsr.ts
```

## Laying Out Routes

A directory is a route when it holds `page.tsx`. Directories without one are only path segments.

```
routes/index/page.tsx            /
routes/product/[id]/page.tsx     /product/{id}
routes/docs/[...rest]/page.tsx   /docs/{*rest}
routes/admin/users/page.tsx      /admin/users     source id admin.users
```

Routes are emitted sorted by pattern, so entry ids are stable across builds regardless of file system order. A directory holding `route.ts` instead of a page is a handler route with the same pattern rules; one holding both fails the build.

A `not-found.tsx` at the top of `routes/` is not a route. It is the page the host renders with status 404 when nothing matches. It receives the path it is answering as `params.path`:

```tsx
export default function NotFound({ params }: { params: { path: string } }) {
  return <h1>No page at {params.path}</h1>;
}
```

## Writing a Loader

`page.loader.ts` exports `load`. It is lowered, never run, so it may import types freely and values not at all.

```ts
import type { Ctx } from "@snapfire/fsr";

export async function load({ params, services }: Ctx<"/product/{id}">) {
  return { product: await services.shopping.getProduct({ id: BigInt(params.id) }) };
}
```

The route pattern as the type argument gives `params` its fields; without it `params` is the union of every route's.

## Writing a Layout

`layout.tsx` wraps every page under its directory and renders the page where it puts `children`. Its props are what `layout.loader.ts` beside it returns, independent of the page's loader, so shared data such as a cart count lives in the layout's loader and no page carries it. A layout is an island: it hydrates, holds state and survives a navigation between the pages it wraps.

```tsx
import type { ReactNode } from "react";
import type { LayoutProps } from "@generated/client";

export default function Layout({ cartCount, children }: LayoutProps & { children: ReactNode }) {
  return (
    <>
      <Header cartCount={cartCount} />
      {children}
    </>
  );
}
```

```ts
export async function load({ session }: Ctx) {
  return { cartCount: Object.values(session.cart).reduce((n, q) => n + q, 0n) };
}
```

Layouts nest: `routes/account/layout.tsx` sits inside `routes/layout.tsx` for every page under `account/`. A layout's loader takes the plain `Ctx`, since it serves many patterns. The page inside a layout cannot read the layout's data and the layout cannot read the page's; they share the session and the actions.

## Filling a Layout's Slots

A layout places named regions beside its page. `slots/<name>/` beside the `layout.tsx` is a parallel segment with the ordinary route files, `page.tsx`, `page.loader.ts`, `loading.tsx` and `error.tsx`, that the layout receives as a prop of that name:

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

```ts
export async function load({ services }: Ctx) {
  return { snacks: await services.shopping.listProducts({ tag: "snack" }) };
}
```

Its props type is `LayoutPromoProps`, its source id `layout.promo`, and it is keyed, cached and kept across navigation like any segment. `<Slot name="modal" />` declares a region nothing fills on a document load; children on it, or `{promo ?? <p>…</p>}` in the prop form, are the fallback the region shows until something does.

`page.modal.tsx` beside a route's `page.tsx` is the rendering a soft navigation opens in that slot of the nearest layout above it declaring one, the page under the layout staying as the browser has it. It shares the route's loader and props type, streams behind a `loading.modal.tsx` of its own when there is one, and is never rendered for a document load: a reload or a shared link of the same URL is the full page. The server applies it when the navigation comes from a route under the same layout; a link forces the document's rendering with `full` or names a slot with `into`, through `Link` from `@snapfire/fsr-client/react` or the `data-sf-full` and `data-sf-into` attributes on any anchor.

The report lists slots by source id and intercepts as `<pattern> into <slot>`. A route may carry one variant per slot. A `slots/` directory anywhere but beside a `layout.tsx`, a slot without a `page.tsx` or with routes beneath it and a variant naming a slot no layout above declares each stop the build.

## Writing Actions

`actions.ts` exports constants built with `action`. The type argument names the input type.

```ts
import { action, fail } from "@snapfire/fsr";
import type { AddToCart } from "../../schemas/cart";

export const addToCart = action<AddToCart>(async ({ input, session }) => {
  const key = String(input.product_id);
  const held = session.cart ?? {};
  session.cart = { ...held, [key]: (held[key] ?? 0n) + input.quantity };
  return { lines: session.cart };
});
```

The input type must be declared under `schemas/`; the host checks a submitted value against it before the body runs.

## Writing a Route Handler

`route.ts` exports one function per HTTP method it answers. A plain function reads the request body as `input` unchecked; an `action<T>` has the body checked against `T` first, the way an action's input is. The value returned is the JSON response; `fail` sets the status by its kind.

```ts
import { action, fail } from "@snapfire/fsr";
import type { ActionCtx, Ctx } from "@snapfire/fsr";
import type { AddToCart } from "../../../schemas/cart";

export async function GET({ session }: Ctx<"/api/cart">) {
  return { lines: session.cart };
}

export const POST = action(async ({ input, session }: ActionCtx<AddToCart>) => {
  if (input.quantity <= 0n) fail("invalid", "quantity must be positive");
  session.cart = { ...session.cart, [String(input.product_id)]: input.quantity };
  return { lines: session.cart };
});
```

The handler runs before any page is matched, sees the same `session`, `identity` and `services` a loader sees and writes the session the same way. A method the file does not export answers 404.

## Writing Middleware

`middleware.ts` exports `middleware`. It runs before the action route, the handlers and the pages, with `request.method` and `request.path` plus the same `query`, `session`, `identity` and `services` a loader gets. What it returns decides the request: nothing continues; `redirect` answers with a `Location` and status 307 unless `status` says otherwise; `status` with an optional `body` answers outright, text for a string and JSON for anything else; `rewrite` serves another path under the same location; `headers` join the response in every case.

```ts
import type { MiddlewareCtx, MiddlewareResult } from "@snapfire/fsr";

export async function middleware({ request, identity }: MiddlewareCtx): Promise<MiddlewareResult> {
  if (request.path.startsWith("/account") && !identity) return { redirect: "/login" };
  if (request.path === "/shop") return { rewrite: "/" };
  return { headers: { "x-frame-options": "DENY" } };
}
```

A body test imports `middleware` from the file and builds its context with `request`: `ctx({ request: { method: "GET", path: "/shop" } })`.

## Importing a Service

Put the document under `clients/` named after the service. Every operation becomes a method on `services.<name>` with typed arguments and return.

```
app/clients/shopping.openapi.json      services.shopping.listProducts, getProduct, placeOrder
```

## Declaring the Session and Action Inputs

Schemas are plain exported interfaces in the subset the contract holds: `string`, `number`, `bigint`, `boolean`, `null`, arrays, `Record<string, T>`, named references, `?` and `| null` for optional, plus `type X = "a" | "b"` for a union of tags.

```ts
// app/schemas/session.ts
export interface Session {
  cart: Record<string, bigint>;
}

export const defaults: Session = {
  cart: {},
};

// app/schemas/cart.ts
export interface AddToCart {
  product_id: bigint;
  quantity: bigint;
}
```

The interface named `Session` types `ctx.session`. A fresh session has none of its keys, so `defaults` says what a body reads until it writes one; the build folds each default into every read of that key. The type stays non-optional without a lie.

## Reading the Generated Types

```ts
// generated/services.d.ts
export interface Product { id: bigint; name: string; price_cents: bigint; stock: bigint; tags: string[] }
export interface Services {
  shopping: {
    listProducts(args?: { tag?: string | null; }): Promise<Product[]>;
    getProduct(args: { id: bigint; }): Promise<Product>;
    placeOrder(args: { lines: OrderLine[]; }): Promise<Order>;
  };
}

// generated/fsr.ts
export interface Routes { "/": {}; "/cart": {}; "/product/{id}": { id: string }; }
export interface Ctx<P extends keyof Routes = keyof Routes> { params: Routes[P]; query: Record<string, string>; session: Session; identity: Identity | null; services: Services; now: bigint }
export function action<Input = void, Out = unknown>(body: (ctx: ActionCtx<Input>) => Promise<Out>): ...
```

Every integer width is `bigint`, because a body runs over the value model where an integer is always an integer. A `number` is a float.

## Registering the Islands

`generated/islands.ts` registers every page, error and loading module discovery named, so the browser mounts exactly what the plan refers to. `main.ts` calls it and registers only what the build cannot know, such as the component of a route added in Rust.

```ts
import { boot, enableNavigation } from "@snapfire/fsr-client";
import { registerIslands } from "../generated/islands.js";

registerIslands();
boot();
enableNavigation();
```

The file must be in the browser build, so `tsconfig.build.json` lists `generated/islands.ts`; the mounter defaults to `reactMounter` from `@snapfire/fsr-client/react` and is set with `Options`.

## Typing Pages and Calling Actions

`generated/client.ts` is what a page imports. It holds the contract's types as the browser sees them, integers as `bigint | number`, one `<Route>Props` type per page inferred from the loader's return and `actions`, one typed callable per action nested by route id.

```tsx
import { actions, type CartProps } from "../../generated/client";

export default function Cart({ lines }: CartProps) {
  return <button onClick={() => actions.cart.addToCart({ product_id: lines[0].id, quantity: -1n })}>remove one</button>;
}
```

```ts
// generated/client.ts
export type CartProps = { lines: (Product & { quantity: bigint | number })[] };
export const actions = {
  cart: {
    addToCart: call("cart.addToCart") as unknown as (input: AddToCart) => Promise<{ lines: Record<string, bigint | number> }>,
    checkout: call("cart.checkout") as unknown as () => Promise<Order>,
  },
};
```

A return the inference cannot settle prints as `unknown`. The file carries runtime code, so `tsconfig.build.json` lists it.

## Vendoring a Package

`fsr add` fetches a package from esm.sh as ES modules with its dependencies bundled in, writes each entry under `vendor/<package>/`, records the version in `vendor/.fsr-vendor.json` and points the import map at the file. Name a version always; name a subpath for an entry other than the package's main. `--external` lists the packages a bundle must import bare rather than carry, which is how two entries share one React.

```sh
fsr add app react@18.3.1 sweetalert2@11.6.15
fsr add app react@18.3.1/jsx-runtime react-dom@18.3.1/client --external react
```

```
added     react                        react/react.bundle.mjs  8715 bytes
added     sweetalert2                  sweetalert2/sweetalert2.bundle.mjs  64908 bytes
added     react/jsx-runtime            react/jsx-runtime.bundle.mjs  2192 bytes
added     react-dom/client             react-dom/client.bundle.mjs  136190 bytes
```

A module that imports a package outside its bundle stops the command naming it; vendor that package and repeat with it in `--external`. The host serves `vendor/` at `/static/js/vendor` by convention, which is the prefix the import map entries carry.

## Fetching Declarations

`fsr types` reads the import map and for every package it names fills `types/<package>/` from the npm registry: the package's own declarations when it publishes `types`, else `@types/<package>` from DefinitelyTyped, plus the dependencies a DefinitelyTyped package declares. A package `fsr add` vendored is fetched at the same major. The fsr packages, `@snapfire/fsr-client` and `@snapfire/fsr-authoring`, come from the binary itself. What was taken is recorded in `types/.fsr-types.json`; a package already present is kept until `--refresh`.

```sh
fsr types app
```

```
types     @snapfire/fsr-authoring      fsr 0.1.0
types     @snapfire/fsr-client         fsr 0.1.0
types     react                        @types/react 18.3.31
types     react-dom                    @types/react-dom 18.3.7
types     sweetalert2                  sweetalert2 11.26.25
types     prop-types                   @types/prop-types 15.7.15
types     csstype                      csstype 3.2.3
```

A package with nothing to fetch is reported `missing` and the build goes on; its imports are `any` in the editor and errors under `strict`. Put `types/` in `.gitignore`: declarations are read by an editor and `tsc --noEmit`, never shipped, so a fresh checkout runs `fsr types` once rather than committing them.

## Reading the Generated tsconfig

`fsr build` writes `tsconfig.json` from the layout: `@snapfire/fsr` to the generated context module, every package under `types/` to its declaration entry, `<package>/*` to its directory so a subpath such as `react/jsx-runtime` resolves; an entry that is a script of `declare module` blocks is included rather than mapped. It is the whole editor configuration, so there is nothing to keep in step by hand.

```json
"paths": {
  "@snapfire/fsr": ["./generated/fsr"],
  "react": ["./types/react/index.d.ts"],
  "react/*": ["./types/react/*"],
  "sweetalert2/*": ["./types/sweetalert2/*"]
},
"include": ["src/**/*", "routes/**/*", "schemas/**/*", "generated/**/*", "types/sweetalert2/sweetalert2.d.ts"]
```

`tsconfig.build.json` is the browser half for snapfirec: `src/`, the pages, the island registry and the client module, so the server-side bodies and their `@snapfire/fsr` import stay out of the bundle.

## Using xwpm Instead

An `xwpm.wmf` in the app directory marks it as an application xwpm manages. `fsr add` then runs `xwpm add <package>@<version>` for each spec, `fsr types` runs `xwpm restore` and `xwpm types` and writes only the fsr packages' declarations; the build reads `vendor`, `base`, `importmap` and `types` from the file's root records for the import map, the report and the tsconfig. xwpm carries dependencies and converts packages itself, so `--external` is not used under it. xwpm must be on `PATH`; a missing binary is an error naming the file that asked for it.

## Building

`build` writes `generated/` and the two tsconfigs; `check` prints the report and writes nothing. Both exit non-zero on residue, a parse error, a missing `load`, a document that does not import, a duplicate type name or an action input no schema declares.

```sh
fsr build app
fsr check app
```

## Serving Without a Rust Project

`serve` builds the stock host over the app and listens until stopped. The configuration is `config/app.toml` beside the app, or an `app.toml` inside it with `[app] dir = "."`; `--listen` overrides `server.listen`. `dev` runs the same host when no `Cargo.toml` wraps the app, watching `config/` in place of `src/`.

```sh
fsr build app
snapfirec --root app --config tsconfig.build.json --source-map --public-path /static/js/app --import-map importmap.json
fsr serve app --listen 127.0.0.1:8080
```

```sh
fsr dev app
```

## Prerendering

`fsr prerender app` renders every route that reads nothing of the request once, into `--out`, else `server.prerender` from the configuration, else `dist/prerender` under the app. It prints what it wrote. The host serves those files from then on; the boot report's `prerender` rows say which routes qualify.

```sh
fsr prerender app
fsr prerender app --out build/static
```

A route qualifies when its pattern has no parameter and every loader on its tree is lowered and reads no `params`, `query`, `session`, `identity`, `input` or `now`. A Rust source disqualifies its route.

## Reading the Report

Six sections, each row naming what was found and where it came from. Every source and action row says `lowered`, because that is the only owner the build produces; the host prints the same report at boot with `rust override` where Rust took a name back. Services name their document, labelled `http` for an OpenAPI document and `grpc` for a `.proto`; schemas name their file. The `types` rows list the fsr packages and every import map package with the directory and source of its declarations or `missing; run fsr types`.

## Reading the Plan File

`plan.json` is format 2 of `snapfire_fsr_plan`: routes with their plan trees, a `sources` table and an `actions` table, each lowered row carrying its body as IR.

```json
{ "id": "cart.checkout", "owner": "lowered", "module": "routes/cart/actions.ts", "export": "checkout",
  "body": [ { "let": { "name": "lines", "expr": { "map": [ ... ] } } }, ... ] }
```

The host reads it with `snapfire_fsr::App::from_manifest`, which binds every lowered row unless Rust overrides the name. It takes the merged `generated/contracts/` through `contract()` so a lowered action's input is checked before its body runs.

## Naming the Shell

The root node of every route is the shell module with the page in one slot. Both are configurable.

```sh
fsr build app --shell shell#document --slot content
```

## Answering a Residue Diagnostic

A body outside the IR stops the build with its position and the construct.

```
routes/cart/page.loader.ts:2:18: `slugify` is not bound here; an import the build cannot follow, or a name from outside the body
```

Two answers: rewrite the body inside the IR or keep the file and bind the name in Rust with `source_override` on the host builder, which makes the row a Rust override in the report. Removing the `page.loader.ts` and binding the id with `source` also works, since a route without a loader declares no source.

## Building From Rust

The library half runs the same build without the binary. A crate that serves the app calls it from `build.rs`, so the generated files never need checking in and `cargo build` on a fresh checkout is enough.

```rust
// build.rs
fn main() {
  let app = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("app");
  for watched in ["routes", "schemas", "clients", "importmap.json", "types"] {
    println!("cargo:rerun-if-changed={}", app.join(watched).display());
  }
  let built = snapfire_fsr_cli::build(&app, &snapfire_fsr_cli::Options::default()).unwrap_or_else(|e| panic!("fsr build app: {e}"));
  snapfire_fsr_cli::write(&app, &built).unwrap_or_else(|e| panic!("fsr build app: {e}"));
}
```

```
# .gitignore
app/generated/
app/tsconfig.json
app/tsconfig.build.json
app/types/
app/dist/
```

```rust
use std::path::Path;
use snapfire_fsr_cli::{build, write, Options};

let built = build(Path::new("app"), &Options::default())?;
print!("{}", built.report);
write(Path::new("app"), &built)?;
```

## Error Handling

`BuildError` is what `build`, `write`, `vendor::add` and `types::fetch` return. `Lower` wraps the recogniser's error, which is the residue case; `Import` names a document that did not import; `DuplicateType` names a type declared twice; `Contract` is a contract that does not hold together; `UnknownInput` is an action naming a type no schema declares; `Segment` names a directory whose name is not a route segment; `Spec` is an `fsr add` argument without a version; `Http` names the URL and what went wrong; `Manifest` a vendor, types or import map file that did not parse; `Dependency` a bundle importing a package it does not carry; `Xwpm` an `xwpm` invocation that failed or could not start; `NoRoutes` and `Io` are what they say.

```rust
use snapfire_fsr_cli::BuildError;
use snapfire_fsr_lower::LowerError;

match build(app, &options) {
  Ok(built) => built,
  Err(BuildError::Lower(LowerError::Residue(r))) => return report_residue(r),
  Err(e) => return fail(e),
}
```
