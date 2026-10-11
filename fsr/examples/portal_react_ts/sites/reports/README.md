# reports_site_react_ts

A site that runs in its own process. It is built with a `[site]` section like `billing`, so every id it emits is prefixed `reports:` and every route sits under `/reports`, but the portal never holds its artifact: `[sites.reports]` in the portal names the prefix and this process's address. The portal renders each page from the payload rows this process answers with, inside the portal's own document.

| It shows | Where |
| --- | --- |
| A client the portal does not carry, read by this process alone | `app/clients/metrics.openapi.json`, `app/clients/metrics.mock.json` |
| The secret the portal signs its requests with, so the portal's session reaches this process | `secret` in `config/app.toml`, the same value in the portal's `[sites.reports]` |
| A deferred segment that streams from this process through the portal to the browser | `app/routes/[id]/loading.tsx` |
| What this host tells the portal about itself: its prefix, its entry module and its stylesheet | `GET /__fsr/sites` on this process, the `site` object |
| A store key the portal seeds, read by the layout's island in the browser | `ShellStore["portal/who"]` in `app/src/store.ts` |

## Run it alone

```sh
cargo run -p reports_site_react_ts
```

`http://127.0.0.1:8104/reports` is the site with its own layout as the page and React served from the portal's `app/vendor/`. `cargo test -p reports_site_react_ts` drives the standalone host. With this process up, `cargo run -p portal_react_ts` serves the same pages at `http://127.0.0.1:8100/reports` under the portal's header.
