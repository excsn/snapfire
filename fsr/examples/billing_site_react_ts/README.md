# billing_site_react_ts

A site: an application built with a `[site]` section, so every id it emits is prefixed `billing:` and every route sits under `/billing`. It runs alone with `cargo run -p billing_site_react_ts`, and `portal_react_ts` mounts the same artifact under its own header.

| It shows | Where |
| --- | --- |
| The `[site]` section: a name, a prefix and the shell contract it is built against | `config/app.toml` |
| Routes, loaders, an action and middleware written with literal `/billing` paths, nothing rewritten | `app/routes/`, `app/middleware.ts` |
| A store key the portal seeds, typed by the shell contract | `ShellStore["portal/who"]` in `app/src/store.ts`, read in `app/routes/layout.tsx` |
| A guard that relies on a sign-in the site never implements | `app/middleware.ts` on `/billing/overdue` |
| A client of its own, mocked from a file, cached on the contract's say-so | `app/clients/ledger.openapi.json`, `app/clients/ledger.mock.json` |
| Static roots the portal serves itself, kept only for running alone | the `[[static]]` root and `vendor/`, a link to the portal's, both `ignored` in the portal's report |

## Run it alone

```sh
cargo run -p billing_site_react_ts
```

`http://127.0.0.1:8101/billing` is the site with its own layout as the page and no sign-in, since the guard's redirect has nowhere to go here. `cargo test -p billing_site_react_ts` drives the standalone host.
