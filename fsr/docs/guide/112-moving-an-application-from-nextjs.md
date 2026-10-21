# 112. Moving an application from Next.js

The question this chapter answers: you have a Next.js application, App Router or Pages Router. What does `fsr migrate` rewrite, what does it leave for you and what do you run next?

**For:** app developers moving a Next.js application.

## The command

```sh
fsr migrate ./my-next-app ./my-fsr-app
```

The first directory is the one holding the Next application's `package.json`; it is read and never written. The second is a new project, scaffolded the way `fsr new --with react` scaffolds one, with the migrated application written over its `app/`. `node_modules`, `.next` and version control are not read. `--no-fetch` skips vendoring React and fetching declarations, as it does for `fsr new`.

The command prints what it wrote, a note for each line it could not rewrite and the commands to run next:

```
from      Next.js App Router
wrote     14 files under ./my-fsr-app
note      app/routes/layout.tsx: the root layout's `<head>` was taken out; a route's head comes from its loader's `meta`
next      fsr add ./my-fsr-app/app clsx date-fns
next      fsr check ./my-fsr-app/app
```

`fsr add` vendors the packages the application imports. `fsr check` builds and typechecks it, which is where the build names any component it leaves to the browser and why.

## Where the files go

| Next.js | FSR |
| --- | --- |
| `app/` or `src/app/` | `app/routes/`, every directory and route file kept |
| `app/@modal/` | `app/routes/slots/modal/` |
| `pages/about.tsx`, `pages/blog/[slug].tsx` | `app/routes/about/page.tsx`, `app/routes/blog/[slug]/page.tsx` |
| `pages/api/hello.ts` | `app/routes/api/hello/route.ts` |
| `pages/_app.tsx` | `app/routes/layout.tsx` |
| `pages/404.tsx` | `app/routes/not-found.tsx` |
| `public/` | `app/public/` |
| `middleware.ts` | `app/middleware.ts` |
| `components/`, `lib/` and every other source directory | `app/src/` |
| `*.test.tsx` beside a route, `__tests__/`, `e2e/` | `app/tests/` |
| `next.config.js`, `tsconfig.json`, lockfiles, `.env` | not carried over |

Every import follows the file it names. A relative specifier is rewritten for both files' new directories. A `tsconfig.json` alias such as `@/components/Nav` becomes `@src/components/Nav`. A page written as `page.jsx` or `page.js` stays as it is, since FSR reads route files in JavaScript too.

## What is rewritten

| Next.js | FSR |
| --- | --- |
| `export const metadata = { title, description }` | `meta` in the route's loader |
| `<Head><title>…</title></Head>` in a page | `meta` in the route's loader, reading the page's props from `data` |
| an `async` page or layout | its awaits in `load` in the route's loader, the page receiving what its markup reads as props |
| `await params`, `searchParams` | `params`, `query` |
| `notFound()` in what moves to the loader | `fail("not_found", …)` |
| `getStaticProps`, `getServerSideProps` | `load`, returning the props themselves |
| `getStaticPaths` | `paths`, one params object per path |
| `pages/_app`'s `<Component {...pageProps} />` | the layout's `children` |
| `next/link` | `Link` from `@snapfire/fsr/components` |
| `next/image` | `Picture` from `@snapfire/fsr/components` |
| `next/font/google` | a `[fonts.<key>]` table in `config/app.toml` and a `font-<key>` class over `--font-<key>` |
| the root layout's `<html>` and `<body>` | a fragment and a `<div>`, since the host writes the document |

A loader that only titles its route exports `meta` and no `load`:

```ts
export const meta = () => ({ title: "About", description: "Who we are" });
```

An async server component moves the same way whether it is the page or a component the page places. The page's own awaits become its loader:

```tsx
// app/page.tsx, before
import { db } from "../lib/db";

export default async function Home() {
  const products = await db.products();
  return <ProductGrid products={products} />;
}
```

```ts
// app/routes/page.loader.ts, after
import { db } from "../src/lib/db";

export async function load() {
  const products = await db.products();
  return { products };
}
```

```tsx
// app/routes/page.tsx, after
export default function Home({ products }) {
  return <ProductGrid products={products} />;
}
```

A component further down that awaits a read known before it renders, one whose arguments come from no prop or state, has the read moved to the loader of each page that places it. The loader seeds the result in the store and the component reads it with `useStore`, from a key the migration declares in `app/src/store.ts`. A read only a later render makes, a component placed under `tab === "posts" &&`, moves the same way and is noted, since its data now loads with the page.

## Server actions

Each async function a `"use server"` module exports becomes an action in the `actions.ts` of the route it belongs to, the nearest directory at or above the module holding a page. Its one typed parameter becomes its input and the type moves under `app/schemas/`:

```ts
// app/actions.ts, before
"use server";
import { Feature } from "./types";

export async function upvote(feature: Feature) {
  await kv.hset(`item:${feature.id}`, feature);
  revalidatePath("/");
}
```

```ts
// app/routes/actions.ts, after
import { action } from "@snapfire/fsr";
import type { ActionCtx } from "@snapfire/fsr";
import type { Feature } from "@schemas/feature";

export const upvote = action(async ({ input }: ActionCtx<Feature>) => {
  const feature = input;
  await kv.hset(`item:${feature.id}`, feature);
});
```

`revalidatePath` and `revalidateTag` are dropped, since an action revalidates its route. A caller imports `actions` from `@generated/client` and calls `actions.$root.upvote(feature)`.

An action taking a `FormData` it reads only as `formData.get("key")` takes those keys as its input. The pass writes the input type, every field a `string`, since that is what a form posts; the host coerces each field against the type, so changing one to `number` is all a numeric field needs:

```ts
// app/schemas/createuserinput.ts
export interface CreateUserInput {
  name: string;
  email: string;
}
```

Each read becomes `input.name`. An action that reads its `FormData` any other way, takes no type or takes more than one argument is left where it was with a note. When that module was `actions.ts`, the actions left behind move to `server-actions.ts`, since FSR reads every export of `actions.ts` as an action.

## Prisma

An application with a `schema.prisma` gets a Rust service in place of the Prisma client. Each model becomes a record in `src/db.rs` and each distinct call a loader makes becomes a method on `Db`, holding the call in its doc comment and a `todo!()` for its query:

```rust
/// `model Product` from `schema.prisma`.
#[derive(Record, Clone)]
pub struct Product {
  pub id: i32,
  pub name: String,
  /// A `Decimal`: the value model has no decimal, so it travels as its text.
  pub price: String,
  pub category: Option<Category>,
  pub category_id: i32,
}

#[service]
impl Db {
  /// `prisma.product.findMany({ include: { category: true } })`
  pub fn product_find_many_with_category(&self) -> Result<Vec<Product>, ServiceError> {
    todo!("the query the doc comment names")
  }
}
```

A relation is an `Option` or a `Vec`, empty unless the call includes it. A value the call reads at request time, directly under `where` or `data` and keyed by one of the model's fields, becomes a parameter typed from that field: `prisma.post.findUnique({ where: { id: postId } })` is `services.db.postFindUniqueById({ id: postId })` answered by `post_find_unique_by_id(&self, id: i32)`. Any other request-time value, such as `{ title: { contains: q } }`, is a note and the call is left for you. The loader calls the method through `services`:

```ts
import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx<"/">) {
  const products = await services.db.productFindManyWithCategory();
  return { products };
}
```

A type imported from `@prisma/client` is imported from `@generated/client`, which `fsr build` writes from the records. The schema, the client module and `prisma.config.ts` are removed. `src/main.rs` and `Cargo.toml` start the host with the service; write each query with sqlx or the database crate of your choice, then `cargo run`.

## What is left for you

Each of these is a note naming the line:

- a fetch from a loader to a backend, a database client or a `process.env` secret: FSR reaches a backend through a contract (chapter 001)
- `draftMode()`, `cookies()` and `headers()`: a loader reads `session`, `identity` and `query` instead
- `revalidate`: the data cache sets its time to live under `[cache]`
- a redirect from a data function: `middleware.ts` answers `{ redirect }`
- `useRouter`, `usePathname` and `useSearchParams`: a page receives its params and query from its loader; `navigate` from `@snapfire/fsr-client` moves to another page
- Next's API routes, which take `(req, res)`: a `route.ts` exports `GET`, `POST` and the rest, each taking the request context
- a title template: `[document] title_template` in `config/app.toml`
- `next/font/local`: `[fonts.<key>]` with `files`
- a slot's `default`, `template`, `global-error` and an intercepting `(.)` directory

A Google font is linked from Google after migration. `fsr fonts add app google:Inter@400,700` serves its faces from the application instead, with `remote` taken out of its table.
