# portal_react_ts

The shell of a company site: a header, a team directory and a sign-in, with four sites other teams own mounted under it at `/billing`, `/blog`, `/status` and `/reports`. One document, one session, one navigation across all of them.

Each site reaches the portal a different way, which is the point of having four:

| Site | `[sites.<name>] artifact` | How it gets there |
| --- | --- | --- |
| `blog` | `sites/blog` | A linked working tree. Rebuilt in place, mounted as version `path`, never pinned. The development shape. |
| `billing` | `billing@1.0.0` | A versioned install. Packed into an archive, installed into the cache under `[sites] root` by an operator, then mounted from there. |
| `status` | `status@1.0.0` | Fetched by the portal. The cache lacks the version, so the portal takes it from `[sites] store`, an HTTP server over a directory of archives, verifies it and mounts it. |
| `reports` | none, `over = ["http://127.0.0.1:8104"]` | Its own process. The portal holds no artifact for it: it asks `reports_site_react_ts` for each page as payload rows and renders them inside its own document, forwarding the site's statics, actions and handlers as bytes. |

| It shows | Where |
| --- | --- |
| A site mounted from its artifact, its routes nested under the portal's root layout | `[sites.billing]` in `config/app.toml`, the `sites` row of the report |
| A version installed from an archive and one fetched from a store, both verified against their manifest | `fsr sites install`, `[sites] root` and `[sites] store` in `config/app.toml` |
| The portal's middleware running on the site's routes too, with the site named | `request.site` in `app/middleware.ts`, the `x-portal` header on `/billing` |
| A sign-in the site never implements: the portal's identity reaches the site's loaders and middleware | `[auth]` here, the guard in `sites/billing/app/middleware.ts` |
| Store keys the portal seeds for every document, typed for the site by the shell contract | `store` in `app/routes/layout.loader.ts`, `generated/shell.json`, `ShellStore` in the site |
| A navigation from the portal into each site and back that keeps the header's island | `Link` to `/billing`, `/blog` and `/status` in `src/ui/Header.tsx`, the `E` row of the payload |
| A deploy that is a pointer moved: the artifact table reread on `SIGHUP` or the poll, the mounted versions on `/__fsr/sites` | `[sites] poll` in `config/app.toml`, `snapfire_fsr_sites::watch` in `src/main.rs` |
| A second site with no binary, whose fixed pages the portal's prerender renders ahead under its session-reading layout | `[sites.blog]`, `sites/blog`, `renders.json` after `fsr prerender app` |
| A header whose section link stays marked while a page under it is shown, across the mount | `match="prefix"` and the `aria-[current]:` classes in `app/src/ui/Header.tsx` |
| A theme for the whole document, decided per request: the visitor's pick from the account page, a `?theme=` preview, the theme a site's layout sets and one billing forces over all of them, with light or dark held across every site | `app/theme.ts`, `app/styles/themes/`, `export const theme` in `sites/blog/app/routes/layout.tsx` and `sites/billing/app/routes/layout.tsx`, the form in `app/routes/account/page.tsx`, `<html data-sf-theme>` and `data-sf-scheme` |
| Tailwind in the shell and in every kind of site, each build generating the utilities for the classes its own pages render | `@import "tailwindcss"` and `@theme` in `app/styles/app.css`, `tailwindcss/theme` and `tailwindcss/utilities` in each site's stylesheet, the `tailwind` row of each report, `app/generated/tailwind.classes` |
| A site in its own process rendered inside the portal's document, with a client the portal cannot reach and a deferred segment streamed through | `[sites.reports]` in `config/app.toml`, `sites/reports`, the `over` row of the report |
| The portal's session reaching that process under a shared secret | `secret` in `[sites.reports]` and in `sites/reports/config/app.toml` |
| A `robots.txt` and a sitemap index across the mounts, leaving out what the middleware turns a signed-out visitor away from; the reports process answers its own sitemap under its prefix, its detail pages listed from a service | `[crawl]` in `config/app.toml` and `sites/reports/config/app.toml`, `/sitemap.xml`, `paths` in `sites/reports/app/routes/[id]/page.loader.ts` |
| A card image drawn from each page's data, in the shell, in a site in this process and in the site in its own process, named by `og:image` in the portal's document; the shell's and the blog's styled with Tailwind classes, each drawn from its own build's table | `app/routes/opengraph-image.tsx`, `sites/blog/app/routes/post/[slug]/opengraph-image.tsx`, `sites/reports/app/routes/[id]/opengraph-image.tsx`, `/blog/post/why-a-plan-file/opengraph-image.png` |
| A release whose loader fails refused before it swaps in, the running version kept | `probe` in `sites/billing/config/app.toml`, `TeamsListed` in `src/main.rs`, the `409` from `fsr sites reload` |

## Run it

From this directory, build the three sites. Each writes its plan and its bundle under its own prefix, billing's through its `build.rs`:

```sh
cargo build -p billing_site_react_ts
fsr build sites/blog/app
fsr build sites/status/app
```

Install billing the way an operator would: pack a release and install it into the cache.

```sh
fsr sites pack sites/billing --version 1.0.0 --out deploy/archives/billing-1.0.0.tar.gz
fsr sites install . deploy/archives/billing-1.0.0.tar.gz --layer local
```

`install` verifies every file against the archive's manifest before renaming it into `deploy/sites/billing/1.0.0`, then writes the hash into `[sites.billing]`, which is what a production table does. `--layer local` puts that pin in `config/local.toml`, which the host reads under the default `APP_ENV` and this checkout ignores: the hash is of what your `fsr` built, so it belongs with your checkout and not in the committed table.

Publish status to the store and serve the store:

```sh
fsr sites pack sites/status --version 1.0.0 --out deploy/store/status-1.0.0.tar.gz
python3 -m http.server 8199 --bind 127.0.0.1 --directory deploy/store
```

Any server that answers `GET /<name>-<version>.tar.gz` is a store: nginx over the directory, a bucket, an Artifactory generic repository. `FSR_SITES_STORE_HEADER="Authorization: Bearer <token>"` adds a header to every fetch.

Run reports as its own process, which is all a site over an address needs:

```sh
cargo run -p reports_site_react_ts
```

Then run the portal in another terminal:

```sh
cargo run -p portal_react_ts
```

The first boot logs `fetched status@1.0.0 from http://127.0.0.1:8199` and the report lists the three artifacts under `sites` and reports as `at /reports over http://127.0.0.1:8104`. Open `http://127.0.0.1:8100/`, sign in as `alice` / `wonder` and follow Billing, Blog, Status and Reports. Reports is rendered by the other process inside the portal's header; stop that process and `/reports` answers `503` with the portal's layout around an error, start it again and the next request renders. `GET /__fsr/sites` shows billing and status at `1.0.0` and blog at `path`. Status is now in the cache, so pin it the same way and `fsr doctor .` reports nothing:

```sh
fsr sites pin . status --layer local
```

To ship status 1.1.0, pack it into the store and move `[sites.status] artifact` to `status@1.1.0`. The poll notices the row moved, fetches the version and swaps the tables. A request in flight finishes on the old ones.

Before the swap the portal renders the candidate: every route that reads nothing of the request, billing's `probe` list from its own configuration, `/billing/overdue` as a signed-in `probe` among them, then its own `TeamsListed` check from `src/main.rs`. To see a release refused, copy `sites/billing` aside, make `listInvoices` in its `ledger.mock.json` answer `{"$fail": {"kind": "unavailable", "message": "the ledger is down"}}`, pack it as `billing-1.1.0.tar.gz` into the store and move `[sites.billing] artifact` to `billing@1.1.0`. The poll logs `reload refused by a probe: source billing:$root: unavailable the ledger is down`, `/billing` keeps answering from `1.0.0` and `fsr sites list . --host 127.0.0.1:8100` shows the instance lagging behind the table on purpose. Fix the mock, pack `1.1.1` and the next poll swaps it in. `fsr sites reload --host` reads the artifacts the booted table names again, so replacing an installed version's files in place and posting it answers the same reason as a `409`.

`cargo test -p portal_react_ts` does the same install and fetch in process, with the store on a free port. It serves the reports site on another to mount it over.

A site team works the other way round: `fsr dev sites/billing/app` finds the portal through billing's `[site] shell`, serves the portal as it stands and mounts billing from its working tree in place of the installed version, rebuilding billing on every save. `--under <dir>` does the same from a copy of the portal's deploy tree, for a team that holds the site alone.
