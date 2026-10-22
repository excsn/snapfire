# 112. Moving an application from Next.js

The question this chapter answers: you have a Next.js application, App Router or Pages Router. What does `fsr migrate` rewrite, what does it leave for you and what do you run next?

**For:** app developers moving a Next.js application.

## The command

```sh
fsr migrate ./my-next-app ./my-fsr-app
```

The first directory is the one holding the Next application's `package.json`; it is read and never written. The second is a new project, scaffolded the way `fsr new --with react` scaffolds one, with the migrated application written over its `app/`. `node_modules`, `.next` and version control are not read. `--no-fetch` skips vendoring React and fetching declarations, as it does for `fsr new`. `--map <file>` names a migration map, below.

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
| `generateMetadata` awaiting what `load` already returns, `params` or `searchParams` | `meta` in the route's loader, reading those values from `data`, `params` and `query` and returning `fromNextMetadata(...)` of the same object |
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
| `next/script`'s `<Script src>` in a page or layout | a `scriptTag` row in the route's `meta` head, `lazyOnload` loading on `idle` |
| `GoogleMapsEmbed`, `YouTubeEmbed` from `@next/third-parties/google` | the `<iframe>` each renders |

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

`fromNextMetadata`, from `@snapfire/fsr/head`, writes a Next `Metadata` object as the head: the title (`{ absolute }` as an absolute title), the description, `alternates` as the canonical and one alternate per language, `openGraph` and `twitter` as their properties and images, `robots` and `authors`. Fields with no head element, `icons` and the title template among them, are left out. A `generateMetadata` that awaits something `load` does not return stays in the page with a note. So does one that awaits inside the object it returns.

## Workspace packages

An application in a pnpm, npm or yarn workspace often imports packages of the same workspace by a `workspace:` range: a shell that wraps Next, a component library, utilities. Those are source, so `fsr migrate` brings them in rather than leaving bare imports the build cannot follow. It finds the workspace root above the application (`pnpm-workspace.yaml` or the `workspaces` field of a `package.json`), follows the application's `workspace:` dependencies and theirs, then follows the application's imports through each package's `exports`: exact subpaths, `*` patterns and condition objects, where the `import`, `default` or `browser` entry is taken, with a note when `node` names another file. Only the files the application reaches are copied, under `app/src/packages/<name>/`. Every import of them is rewritten to follow, so the rest of the migration rewrites them too. An alias in the application's own `tsconfig.json` that points into a package, a shim standing in for one of its modules, wins over that package's `exports` as it does for TypeScript. The packages' own npm dependencies join the `fsr add` line.

## The migration map

A codebase that wraps Next in its own shell calls the wrapper everywhere, `getLocale()` from the shell instead of reading the locale off the route, the shell's `Link` and `notFound`. The rules above match Next's modules, so they would miss every one of those calls. A migration map says what the wrapper stands for. `fsr migrate` then rewrites each call as it rewrites the framework's own. It is a handwritten file, `migrate.toml`, read by `fsr migrate` alone: nothing at build or run time reads it. It can be deleted once the migration is done.

```toml
#:schema ./migrate.schema.json

# The shell's notFound is Next's: the rewrite to `fail` applies.
[map."@shell/navigation/server#notFound"]
to = "next/navigation#notFound"

# A module stands for a module: every export of it is imported from there.
[map."@shell/navigation/client"]
to = "next/navigation"

# Its Link is FSR's.
[map."@shell/navigation#Link"]
to = "@snapfire/fsr/components#Link"

# A locale read off the URL, in the route's loader.
[map."@shell/i18n/server#getLocale"]
call = "params.locale"

# A call with arguments: $0, $1 and onward are what the call passed.
[map."@shell/flags#flag"]
call = "(config[$0] ?? $1)"

# Setup FSR does itself.
[map."@shell/telemetry#initTelemetry"]
drop = true

# Something a person rewrites, named wherever it is imported.
[map."@shell/i18n/server#getTranslations"]
note = "translations move to the locale catalogs"
```

Each key under `map` is a module or one export of it after `#`, with `#default` for a default export; an export's entry wins over its module's. An entry gives exactly one of these. `note` may go with any of them or stand alone:

| Field | What it does |
| --- | --- |
| `to` | the import is rewritten to the module or export named, so whatever applies to that export applies here: a Next export is then rewritten by the rules above, an FSR export is used as it is. A module's entry takes a module |
| `call` | a call to the export or an `await` of it becomes the expression in a route module, where the route's loader runs it with the request in reach. A call anywhere else is left with a note |
| `drop` | each statement that only calls the export is removed with its import. A call inside an expression is left with a note |
| `note` | the text is noted at each import of the module or export |

`fsr migrate` reads `migrate.toml` from the application's root and from the root of each workspace package it brings in, so a shell can ship the map for itself and every application using it migrates the same way; the application's entries win over a package's. `--map <file>` reads that file in place of the application's own. `fsr migrate --schema` prints the file's JSON Schema; save it beside the map and name it on the first line, as above, for completion and checking in an editor. An entry that gives more than one of `to`, `call` and `drop` is noted and not applied. So is a module that stands for an export.

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

An action a component declares in its own body moves to the route's `actions.ts` the same way. What it reads of the component joins its input. Each `<form action={deletePost}>` posts those values as hidden fields beside the CSRF token; `next/form`'s `<Form>` becomes a `<form>` that does the same:

```tsx
// app/posts/[id]/page.tsx, before
export default async function Post({ params }) {
  const { id } = await params;
  const postId = parseInt(id);
  async function deletePost() {
    "use server";
    await db.remove(postId);
  }
  return <form action={deletePost}><button>Delete</button></form>;
}
```

```tsx
// app/routes/posts/[id]/page.tsx, after
export default function Post({ postId, csrf_token }: PostsIdProps) {
  return <form method="post" action="/_sf/action/posts.$id.deletePost">
    <input type="hidden" name="_csrf" value={csrf_token ?? ""} />
    <input type="hidden" name="postId" value={postId} />
    <button>Delete</button></form>;
}
```

A captured value is typed `number` when `parseInt`, `parseFloat`, `Number` or a numeric literal made it and `string` when a string, a template or the route's `params` did. An action that reads anything else of the component is left where it was with a note. So is one used other than as a form's `action`. A `redirect()` or `permanentRedirect()` the action makes as a statement becomes `redirect` from `@snapfire/fsr`, which sends the form post to that path; one inside an expression leaves the action where it was with a note.

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

A relation is an `Option` or a `Vec`, empty unless the call includes it. A value the call reads at request time, directly under `where` or `data` and keyed by one of the model's fields, becomes a parameter typed from that field: `prisma.post.findUnique({ where: { id: postId } })` is `services.db.postFindUniqueById({ id: postId })` answered by `post_find_unique_by_id(&self, id: i32)`. Any other request-time value, such as `{ title: { contains: q } }`, is a note and the call is left for you, with the client module and `schema.prisma` kept until every call is a method. A call in an action becomes a method the same way, the action taking `services` from its context. The loader calls the method through `services`:

```ts
import type { Ctx } from "@snapfire/fsr";

export async function load({ services }: Ctx<"/">) {
  const products = await services.db.productFindManyWithCategory();
  return { products };
}
```

A type imported from `@prisma/client` is imported from `@generated/client`, which `fsr build` writes from the records. The schema, the client module and `prisma.config.ts` are removed. `src/main.rs` and `Cargo.toml` start the host with the service; write each query with sqlx or the database crate of your choice, then `cargo run`.

## Public environment variables

A `process.env.NEXT_PUBLIC_*` read in a page, a layout or a loader becomes a `[public]` key in `config/app.toml`, the prefix dropped and the name lowercased, declared empty for each deployment to set. A page or layout receives it as a prop its loader returns from `config`:

```tsx
// app/layout.tsx, before
export default function RootLayout({ children }: { children: React.ReactNode }) {
  return <html><GoogleAnalytics gaId={process.env.NEXT_PUBLIC_GA_ID as string} /><body>{children}</body></html>;
}
```

```ts
// app/routes/layout.loader.ts, after
import type { Ctx } from "@snapfire/fsr";

export async function load({ config }: Ctx) {
  return { gaId: config.ga_id };
}
```

The layout destructures `gaId` beside `children` and reads it where the variable was. A module constant that held the variable, `const API_KEY = process.env.NEXT_PUBLIC_GOOGLE_API_KEY`, becomes the prop of that name. A read in a component below the route is a note, as is one anywhere else a prop does not reach.

## What is left for you

Each of these is a note naming the line:

- a fetch from a loader to a backend, a database client or a server-only `process.env` variable: FSR reaches a backend through a contract (chapter 001) and a secret only as a client's credential (chapter 202)
- `draftMode()`, `cookies()` and `headers()`: a loader reads `session`, `identity` and `query` instead
- `revalidate`: the data cache sets its time to live under `[cache]`
- a redirect from a data function: `middleware.ts` answers `{ redirect }`
- `useRouter`, `usePathname` and `useSearchParams`: a page receives its params and query from its loader; `navigate` from `@snapfire/fsr-client` moves to another page
- Next's API routes, which take `(req, res)`: a `route.ts` exports `GET`, `POST` and the rest, each taking the request context
- a title template: `[document] title_template` in `config/app.toml`
- a `<Script>` with inline code or an `onLoad`: a module `src/main.ts` imports, calling `loadScript` (chapter 305)
- `next/font/local`: `[fonts.<key>]` with `files`
- a slot's `default`, `template`, `global-error` and an intercepting `(.)` directory

A Google font is linked from Google after migration. `fsr fonts add app google:Inter@400,700` serves its faces from the application instead, with `remote` taken out of its table.
