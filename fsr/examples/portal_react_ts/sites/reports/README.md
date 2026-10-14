# reports_site_react_ts

A site the portal mounts from its working tree, as it does `blog`. It is built with a `[site]` section, so every id it emits is prefixed `reports:` and every route sits under `/reports`. It brings a backend the portal does not carry: the portal builds the site's `metrics` client from the site's own configuration when it mounts it.

| It shows | Where |
| --- | --- |
| A client the portal does not carry, declared by the site alone | `[clients.metrics]` in `config/app.toml`, `app/clients/metrics.openapi.json`, `app/clients/metrics.mock.json` |
| A deferred segment that streams under the portal's header | `app/routes/[id]/loading.tsx` |
| A GET route handler under the site's prefix | `app/routes/[id]/rows/route.ts` |
| A store key the portal seeds, read by the layout's island in the browser | `ShellStore["portal/who"]` in `app/src/store.ts` |
| Its pages in the portal's sitemap, one entry per report from the `paths` export calling the metrics client | `paths` in `app/routes/[id]/page.loader.ts` |
| A card image per report at `/reports/<id>/opengraph-image.png` | `app/routes/[id]/opengraph-image.tsx` |

## Run it alone

```sh
cargo run -p reports_site_react_ts
```

`http://127.0.0.1:8104/reports` is the site with its own layout as the page and React served from the portal's `app/vendor/`. `cargo test -p reports_site_react_ts` drives the standalone host. `cargo run -p portal_react_ts` serves the same pages at `http://127.0.0.1:8100/reports` under the portal's header.
