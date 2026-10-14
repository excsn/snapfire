# blog_site_react_ts

A blog as a site the portal mounts under `/blog`: an index, a tag list and one page per post, each post a file under `content/posts/`, two in MDX and one in markdown. React comes from the shell contract the site is built against, so the site vendors nothing. There is no Rust project: `fsr build app` writes the artifact the portal reads and `fsr serve app` runs the site alone.

| It shows | Where |
| --- | --- |
| A mounted site's pages written by the shell's `fsr prerender` and spliced under the portal's session-reading layout per request | `renders.json` under the portal's prerender directory, `x-sf-prerendered` on `/blog` there |
| The same site prerendering its own documents when it runs alone, one per post from `paths` | `fsr prerender app`, `dist/prerender/post/<slug>/index.html` |
| A route that names its parameter set | `export const paths` in `routes/post/[slug]/page.loader.ts` |
| A post that does not exist answering 404 | `fail("not_found", ...)` in the same loader |
| Placements written against the portable template module in a React site | `@snapfire/fsr-authoring/template` in every route file |
| Every post listed for the index, the tags and `paths` without a list of its own | `[mdx.collections]` in `config/app.toml`, `@generated/content` in `src/posts.ts` |
| One page for every post, importing the file the post's data names | the template `import()` in `routes/post/[slug]/page.tsx` |
| Markdown links placed as `Link` for that page alone, so a post's link to itself is marked current | `components={{ a: Link }}` on the post body |
| Headings and quotes styled through the component map | `mdx-components.tsx` |
| A table, a footnote, a task list and highlighted code with a title and a marked line | `content/posts/why-a-plan-file.mdx`, `content/posts/a-site-is-a-mount.mdx`, `styles/code.css` |
| A component and an island placed from MDX | `<Aside>` and `<Island><Claps /></Island>` in the posts |
| A markdown post titled by YAML frontmatter | `content/posts/static-under-a-shell.md`, `[mdx] frontmatter = true` |
| A theme the site sets for its own pages that a visitor's pick replaces, with the portal's tokens read through fallbacks | `export const theme = "editorial"` in `routes/layout.tsx`, `styles/themes/editorial.css`, `@theme reference` in `styles/blog.css` |
| A card image per post drawn from the post's data, with its tags as a flex row of pills, at `/blog/post/<slug>/opengraph-image.png`, styled with the same Tailwind classes a page uses | `routes/post/[slug]/opengraph-image.tsx`, `generated/tailwind.cards.json` |

## Run it alone

```sh
fsr build app
fsr prerender app
fsr serve app
```

The build needs an fsr with its MDX features, `cargo install snapfire_fsr_cli --features mdx-highlight,mdx-frontmatter`; one without them refuses `[mdx.collections]` naming the feature.

`http://127.0.0.1:8102/blog` is the blog with its own layout as the page. `fsr test app` runs the body tests and the specs.

## Under the portal

The portal mounts this directory as a linked working tree, `artifact = "sites/blog"`, so `fsr build app` is all a change needs before the portal rereads it. The portal's README runs the three sites together.

`http://127.0.0.1:8100/blog` is the blog under the portal's header, sharing its session and navigation. `fsr prerender ../../app` writes the blog's pages into the portal's prerender directory.
