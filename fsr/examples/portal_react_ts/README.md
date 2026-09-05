# portal_react_ts

The shell of a company site: a header, a team directory and a sign-in; under `/billing`, a site another team owns, mounted from that team's build output. One document, one session, one navigation across both.

| It shows | Where |
| --- | --- |
| A site mounted under a path from its artifact, its routes nested under the portal's root layout | `[sites.billing]` in `config/app.toml`, the `sites` row of the report |
| The portal's middleware running on the site's routes too, with the site named | `request.site` in `app/middleware.ts`, the `x-portal` header on `/billing` |
| A sign-in the site never implements: the portal's identity reaches the site's loaders and middleware | `[auth]` here, the guard in `../billing_site_react_ts/app/middleware.ts` |
| Store keys the portal seeds for every document, typed for the site by the shell contract | `store` in `app/routes/layout.loader.ts`, `generated/shell.json`, `ShellStore` in the site |
| A navigation from the portal into the site and back that keeps the header's island | `Link` to `/billing` in `src/ui/Header.tsx`, the `E` row of the payload |
| A deploy that is a pointer moved: the artifact table reread on `SIGHUP` or the poll, the mounted versions on `/__fsr/sites` | `[sites] poll` in `config/app.toml`, `snapfire_fsr_sites::watch` in `src/main.rs` |

## Run it

The site's bundle is served under its own prefix, so build both applications first, then run the portal:

```sh
cargo build -p billing_site_react_ts
cargo run -p portal_react_ts
```

Each `build.rs` emits its own plan and bundle, the site's under `/billing/static/js/app` from its `[site] at`.

Then open `http://127.0.0.1:8100/`, sign in as `alice` / `wonder` and follow Billing. `cargo test -p portal_react_ts` drives the same host in process.
