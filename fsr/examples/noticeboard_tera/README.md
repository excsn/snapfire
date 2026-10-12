# noticeboard_tera

A building's noticeboard: a list of notices and a page per notice, every page a Tera template under `routes/` and no TypeScript component anywhere. The loaders are TypeScript, lowered as they are beside a `page.tsx`; the templates are what renders, read by the stock host from the files themselves. There is no Rust project and no `main.rs`: `fsr serve app` is the server.

It exists to show that a template is one more thing a route directory can hold. `routes/page.tera` is `/`, `routes/notice/[id]/page.tera` is `/notice/{id}`, `routes/layout.tera` wraps both and places the page with `{{ slot(name="content") }}`. A template's context is what its loader returned, so `notices` in the index and `notice` on a page are the loader's fields. A partial is a template anywhere under `app/`, named by its path: `{% include "templates/nav.tera" %}`.

## Running it

```sh
fsr serve app
```

`fsr prerender app` writes the index and, since the notice page's loader exports `paths`, one file per notice. A production host answers those from the files and renders any other id live. `fsr serve app` in development renders every page live; `/notice/bins?__prerendered` shows the page as a prerender writes it. The same `paths` fills `/sitemap.xml`, which `[crawl]` turns on beside `/robots.txt`. Each notice's `meta` names its posting date with `modified`, which the sitemap writes as its `lastmod`.

## What it holds

| Path | What it is |
| --- | --- |
| `/` | the notices, from a loader reading a module constant |
| `/notice/{id}` | one notice, or a line saying there is none |
| `routes/layout.tera` | the frame, with a footer counting the notices its own loader returned |
| `templates/nav.tera` | a partial the layout includes by path |
| `images/board.png` | the photo the layout places with `fsr_picture`, found by the build under the images directory rather than by an import |
| `routes/opengraph-image.png` | the card a shared link shows, on the template layout's module so every page carries it |
| `fonts/Inter-*.woff2` | the body face, named by `[fonts.sans]` with Arial as its fallback; the host writes the `@font-face` rules, the sized fallback and `--font-sans` into every document, so the templates add nothing |

## What it does not do

No island is placed, so nothing hydrates and `generated/islands.ts` registers nothing. A template places one with `{{ island(module="src/ui/Thing.tsx#default") }}` and the build bundles it from the literal, which `uni` shows on a Rust host.
