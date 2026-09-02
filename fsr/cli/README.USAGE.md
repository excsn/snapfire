# Usage Guide: snapfire_fsr_cli

How to lay out an application's routes, clients and schemas, run a build and read the report and the files it writes. What to do when a body is residue.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Laying Out Routes](#laying-out-routes)
* [Writing a Loader](#writing-a-loader)
* [Writing Actions](#writing-actions)
* [Importing a Service](#importing-a-service)
* [Declaring the Session and Action Inputs](#declaring-the-session-and-action-inputs)
* [Reading the Generated Types](#reading-the-generated-types)
* [Registering the Islands](#registering-the-islands)
* [Typing Pages and Calling Actions](#typing-pages-and-calling-actions)
* [Building](#building)
* [Reading the Report](#reading-the-report)
* [Reading the Plan File](#reading-the-plan-file)
* [Naming the Shell](#naming-the-shell)
* [Answering a Residue Diagnostic](#answering-a-residue-diagnostic)
* [Building From Rust](#building-from-rust)
* [Error Handling](#error-handling)

## Core Concepts

* **App directory** is the directory that holds `routes/`; `fsr` writes `plan.json` beside it.
* **Route** is a directory under `routes/` that holds `page.tsx`.
* **Pattern** comes from the directory path: `index` is `/`, `[id]` is `{id}`, `[...rest]` is `{*rest}`.
* **Source id** is the route's static segments joined with `.`, `index` for the root; it names the loader.
* **Action id** is `<source id>.<export>`, so `routes/cart/actions.ts` exporting `checkout` is `cart.checkout`.
* **Module id** is the page's path with `#default`, `routes/cart/page.tsx#default`; the client registers islands under it.
* **Shell** is the module every route's root node renders through, `shell#document` unless told otherwise.
* **Error module** is a route's own `error.tsx`, falling back to `routes/error.tsx` for every page.
* **Loading module** is a route's `loading.tsx`; its presence marks the node deferred with that fallback.
* **Lowered** is the owner of every source and action the build emits; the host may override any of them in Rust.
* **Client** is an OpenAPI document under `clients/`, `<name>.openapi.json`, imported as the service `<name>`.
* **Schema** is a TypeScript module under `schemas/` whose exported interfaces become contract types; one named `Session` types `ctx.session`.
* **Generated** is `generated/`, rewritten on every build: `services.d.ts`, `fsr.ts`, `contract.json`, `islands.ts` and `client.ts`. The app's `tsconfig.json` maps `@snapfire/fsr` to `generated/fsr`, so a body imports `Ctx`, `action` and `fail` from that bare name and gets the app's own types.

## Quick Start

```
app/
  clients/   shopping.openapi.json
  schemas/   session.ts  cart.ts
  routes/
    error.tsx
    index/         page.tsx  loader.ts
    product/[id]/  page.tsx  loader.ts
    cart/          page.tsx  loader.ts  actions.ts
```

```sh
fsr build app
```

```
routes    /                      routes/index
          /cart                  routes/cart
          /product/{id}          routes/product/[id]
sources   index                  lowered     routes/index/loader.ts
          cart                   lowered     routes/cart/loader.ts
          product                lowered     routes/product/[id]/loader.ts
actions   cart.addToCart         lowered     routes/cart/actions.ts
          cart.checkout          lowered     routes/cart/actions.ts
services  shopping               http        clients/shopping.openapi.json
schemas   AddToCart              schemas/cart.ts
          Session                schemas/session.ts
wrote app/plan.json
wrote app/generated/contract.json
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

Routes are emitted sorted by pattern, so entry ids are stable across builds regardless of file system order.

## Writing a Loader

`loader.ts` exports `load`. It is lowered, never run, so it may import types freely and values not at all.

```ts
import type { Ctx } from "@snapfire/fsr";

export async function load({ params, services }: Ctx<"/product/{id}">) {
  return { product: await services.shopping.getProduct({ id: BigInt(params.id) }) };
}
```

The route pattern as the type argument gives `params` its fields; without it `params` is the union of every route's.

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

## Building

`build` writes `plan.json` and `generated/`; `check` prints the report and writes nothing. Both exit non-zero on residue, a parse error, a missing `load`, a document that does not import, a duplicate type name or an action input no schema declares.

```sh
fsr build app
fsr check app
```

## Reading the Report

Five sections, each row naming what was found and where it came from. Every source and action row says `lowered`, because that is the only owner the build produces; the host prints the same report at boot with `rust override` where Rust took a name back. Services name their document and schemas their file.

## Reading the Plan File

`plan.json` is format 2 of `snapfire_fsr_plan`: routes with their plan trees, a `sources` table and an `actions` table, each lowered row carrying its body as IR.

```json
{ "id": "cart.checkout", "owner": "lowered", "module": "routes/cart/actions.ts", "export": "checkout",
  "body": [ { "let": { "name": "lines", "expr": { "map": [ ... ] } } }, ... ] }
```

The host reads it with `snapfire_fsr::App::from_manifest`, which binds every lowered row unless Rust overrides the name. It takes `generated/contract.json` through `contract()` so a lowered action's input is checked before its body runs.

## Naming the Shell

The root node of every route is the shell module with the page in one slot. Both are configurable.

```sh
fsr build app --shell shell#document --slot content
```

## Answering a Residue Diagnostic

A body outside the IR stops the build with its position and the construct.

```
routes/cart/loader.ts:2:18: `slugify` is not bound here; an import the build cannot follow, or a name from outside the body
```

Two answers: rewrite the body inside the IR or keep the file and bind the name in Rust with `source_override` on the host builder, which makes the row a Rust override in the report. Removing the `loader.ts` and binding the id with `source` also works, since a route without a loader declares no source.

## Building From Rust

The library half runs the same build without the binary.

```rust
use std::path::Path;
use snapfire_fsr_cli::{build, write, Options};

let built = build(Path::new("app"), &Options::default())?;
print!("{}", built.report);
write(Path::new("app"), &built)?;
```

## Error Handling

`BuildError` is what `build` and `write` return. `Lower` wraps the recogniser's error, which is the residue case; `Import` names a document that did not import; `DuplicateType` names a type declared twice; `Contract` is a contract that does not hold together; `UnknownInput` is an action naming a type no schema declares; `Segment` names a directory whose name is not a route segment; `NoRoutes` and `Io` are what they say.

```rust
use snapfire_fsr_cli::BuildError;
use snapfire_fsr_lower::LowerError;

match build(app, &options) {
  Ok(built) => built,
  Err(BuildError::Lower(LowerError::Residue(r))) => return report_residue(r),
  Err(e) => return fail(e),
}
```
