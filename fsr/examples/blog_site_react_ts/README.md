# blog_site_react_ts

A blog as a site the portal mounts under `/blog`: an index, a tag list and one page per post, with the posts baked in at build time as module constants under `src/posts.ts`. React comes from the shell contract the site is built against, so the site vendors nothing. There is no Rust project: `fsr build app` writes the artifact the portal reads and `fsr serve app` runs the site alone.

| It shows | Where |
| --- | --- |
| A mounted site's pages written by the shell's `fsr prerender` and spliced under the portal's session-reading layout per request | `renders.json` under the portal's prerender directory, `x-sf-prerendered` on `/blog` there |
| The same site prerendering its own documents when it runs alone, one per post from `paths` | `fsr prerender app`, `dist/prerender/post/<slug>/index.html` |
| A route that names its parameter set | `export const paths` in `routes/post/[slug]/page.loader.ts` |
| A post that does not exist answering 404 | `fail("not_found", ...)` in the same loader |
| Placements written against the portable template module in a React site | `@snapfire/fsr-authoring/template` in every route file |
| A post body that is markup the application produced | `dangerouslySetInnerHTML` in `routes/post/[slug]/page.tsx` |

## Run it alone

```sh
fsr build app
fsr prerender app
fsr serve app
```

`http://127.0.0.1:8102/blog` is the blog with its own layout as the page. `fsr test app` runs the body tests and the specs.

## Under the portal

Build this site, then run the portal, which mounts it from `[sites.blog]`:

```sh
fsr build app
cargo run -p portal_react_ts
```

`http://127.0.0.1:8100/blog` is the blog under the portal's header, sharing its session and navigation. `fsr prerender ../portal_react_ts/app` writes the blog's pages into the portal's prerender directory.
