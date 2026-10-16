# 110. MDX and markdown

**For app developers.** How a page or a piece of content is written in MDX or plain markdown, how the build lowers it like any other component, how a page picks a file by its data, the component map that styles markdown's elements, highlighted code and the collections an index reads.

## MDX is a feature of fsr

The parser, the highlighter and frontmatter are Cargo features of the CLI, on by default. An install that wants none of them turns the defaults off and keeps what it needs:

```sh
cargo install snapfire_fsr_cli --no-default-features --features tera
```

| Feature | Reads |
| --- | --- |
| `mdx` | `.mdx` and `.md` routes and imports |
| `mdx-highlight` | `mdx`, plus fenced code highlighted at build |
| `mdx-frontmatter` | `mdx`, plus YAML or TOML frontmatter as `export const meta` |

An fsr built without `mdx` refuses an `.mdx` page or import by name rather than leaving it out. A `build.rs` that builds an application gets the features with the build dependency's defaults.

`fsr use app mdx` writes a starter `mdx-components.tsx` and, with highlighting built in, `styles/code.css`. `fsr new shop --with mdx` starts an application with them.

## A page written in MDX

`page.mdx` beside a route's loader is the route's page. It follows MDX: ESM at the top, markdown with JSX in it and `{expressions}`:

```mdx
export const meta = { title: "Install · The FSR handbook" };

# Install

<ol className="steps">
  {props.steps.map((step) => (
    <li key={step.command}>
      <code>{step.command}</code>
      <span>{step.explains}</span>
    </li>
  ))}
</ol>

This page is {props.steps.length} steps from `src/content.ts`.
```

The loader runs first, as for `page.tsx`. Its data is `props`. `export const meta` is the route's meta, a plain object since an MDX file has no `{ data }` to read; a route whose loader also exports `meta` is refused. `page.md` is markdown alone, MDX's `md` format, with no JSX and no expressions. Raw HTML in it is left out with a warning in the build report.

The build turns the file into TSX and lowers that, so everything chapter 102 says about a component holds. An import lowers, a vendored component becomes an island, an expression lowers where it can and what does not is moved into an island the browser renders, as for a page. A residue names the line and column of the `.mdx`, here under `[build] strict`:

```text
routes/page.mdx:3:6: `window` is not bound here; an import the build cannot follow or a name from outside the body
```

## Importing a file

An `.mdx` or `.md` file under the app is a module whose default export is a component:

```tsx
import Post, { meta, toc } from "../content/first.mdx";

export default function Page() {
  return (
    <article>
      <h1>{meta.title}</h1>
      <nav>{toc.map((entry) => <a key={entry.id} href={`#${entry.id}`}>{entry.text}</a>)}</nav>
      <Post />
    </article>
  );
}
```

`toc` lists the file's headings, each `{ depth, id, text }`. `generated/mdx.d.ts` declares `*.mdx` and `*.md` for the type checker.

## A page that picks a file by its data

The blog's post page is one route for every post. It imports the file its data names, the way a Next page does:

```tsx
export default async function PostPage({ post }: PostSlugProps) {
  const { default: Body, meta } = await import(`../../../content/posts/${post.file}`);
  return <article><h1>{meta.title}</h1><Body /></article>;
}
```

The build reads the template: every file in the directory it names that fits it is a choice, lowered once. `<Body />` renders the one whose name the key spells and `meta` is that file's export. A key that names no file renders nothing. The page may be `async` when everything it awaits is such an import; any other `await` belongs in its loader.

## Collections

An index needs every post's title without a list of its own. `[mdx.collections]` names a directory:

```toml
[mdx.collections]
posts = "content/posts"
```

The build writes `generated/content.ts` with one entry per `.mdx` or `.md` file in it, by file name:

```ts
import { posts } from "@generated/content";

export const titles = posts.map((post) => post.meta.title);
export const paths = () => posts.map((post) => ({ slug: post.slug }));
```

Each entry is `{ slug, file, meta, toc, excerpt }`: `slug` the name without its extension, `file` the name in the directory, which is what a template import takes. `excerpt` is the plain text of the file's first paragraph, or `null` when it has none. A loader sorts and filters the list like any other constant.

## The component map

`mdx-components.tsx` at the app's root maps the elements markdown writes to components, as Next reads it:

```tsx
import type { MDXComponents } from "mdx/types";

function Heading({ id, className, children }: { id?: string; className?: string; children?: any }) {
  return <h2 id={id} className={className ?? "anchored"}><a href={`#${id}`}>#</a> {children}</h2>;
}

export function useMDXComponents(components?: MDXComponents): MDXComponents {
  return {
    h2: Heading,
    wrapper: ({ children }: { children?: any }) => <div className="prose">{children}</div>,
    ...components,
  };
}
```

The build reads the object `useMDXComponents` returns, so its entries are a component's name or a component written in place. A JSX tag written in an `.mdx` file is never mapped, only what markdown writes. `wrapper` takes the whole content as its children; a file's own `export default` is its layout and stands in for the wrapper. Either is given the content and no other props.

A placement can replace elements for one file with a `components` prop, an object of components written out:

```tsx
import { Link } from "@snapfire/fsr-authoring/template";

<Body components={{ a: Link }} />
```

The build lowers the file again with those elements replaced, so every markdown link in the post is a `Link` and marks the current page.

## The `[mdx]` table

| Key | Default | What it does |
| --- | --- | --- |
| `gfm` | `true` | Tables, strikethrough, task lists, footnotes and literal autolinks, as `remark-gfm` writes them |
| `heading_ids` | `true` | An `id` on every heading as `rehype-slug` makes it, plus the `toc` export |
| `frontmatter` | `false` | YAML or TOML frontmatter as `export const meta`, with `mdx-frontmatter` |
| `highlight` | on with `mdx-highlight` | Fenced code highlighted at build |
| `collections` | none | A name for each content directory listed in `generated/content.ts` |

A key whose feature fsr was built without is refused naming the feature. Remark and rehype plugins are JavaScript and are not run; the table above is what they are used for.

## Highlighted code

With highlighting, a fence whose language names a grammar is written as one span per line, each token a span whose classes are its TextMate scopes, `hl-` prefixed. The fence options are `rehype-pretty-code`'s:

~~~mdx
```ts title="routes/post/[slug]/page.loader.ts" {2}
export async function load({ params }: Ctx<"/post/{slug}">) {
  return { slug: params.slug };
}
```
~~~

No colours are written: `styles/code.css` from `fsr use app mdx` colours the classes in GitHub under `data-sf-scheme="light"` and OneHalfDark under `data-sf-scheme="dark"`, each also under the system's preference when the document names no scheme. It also resets an application's own `code` style inside a highlighted block. The classes stay out of the Tailwind class list. A fence with no language is `<pre><code>`. A fence whose language no grammar knows keeps its `title` as a `figcaption` and is `<pre data-language="x"><code className="language-x" data-language="x">` with its text unhighlighted.

## In the browser

A file lowers and renders on the server like any page, so the browser loads nothing for it. When an island imports one, the browser renders it too: the build writes the browser's copy into `.fsr-bundle/` and snapfirec compiles that copy. The copy reads the map through `useMDXComponents` as MDX's own output does:

```tsx
import { useState } from "react";
import TipBody from "../../content/tip.mdx";

export function Tip() {
  const [open, setOpen] = useState(false);
  return <div><button onClick={() => setOpen(!open)}>Show the tip</button>{open ? <TipBody /> : null}</div>;
}
```

## Lab

In the handbook, open `/install`, `/faq` and `/tips`: an MDX page reading its loader's data, a markdown page titled by its frontmatter and a page whose `export default` is its layout. On `/tips`, click "Show the tip" and the island renders `content/tip.mdx` in the browser with the same heading component the server uses. In the portal, open `/blog/post/why-a-plan-file` for a table, a footnote and highlighted code, then `/blog/post/a-site-is-a-mount`: its markdown links are `Link`s, so the one to the post itself carries `aria-current`. Add a file to `sites/blog/app/content/posts/` and run `fsr build sites/blog/app`: the index lists it and the post route prerenders it with no other edit.
