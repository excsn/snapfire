# billing_site_react_ts

A site: an application built with a `[site]` section, so every id it emits is prefixed `billing:` and every route sits under `/billing`. It runs alone with `cargo run -p billing_site_react_ts` and `portal_react_ts` mounts the same artifact under its own header.

| It shows | Where |
| --- | --- |
| The `[site]` section: a name, a prefix and the shell contract it is built against | `config/app.toml` |
| Routes, loaders, an action and middleware written with literal `/billing` paths, nothing rewritten | `app/routes/`, `app/middleware.ts` |
| An image imported by a site served under its prefix, alone and mounted, with its variants in the artifact | `app/src/img/ledger.png` placed by `app/routes/page.tsx` |
| A store key the portal seeds, typed by the shell contract | `ShellStore["portal/who"]` in `app/src/store.ts`, read in `app/routes/layout.tsx` |
| A guard that relies on a sign-in the site never implements | `app/middleware.ts` on `/billing/overdue` |
| A nav whose current link is marked by the server and kept right by the navigator, styled by Tailwind's `aria-[current]:` variant | `<Link>` and `tab` in `app/routes/layout.tsx` |
| A row styled from its data through an attribute instead of a class built from the status | `data-status` and `group-data-[status=overdue]:` in `app/routes/page.tsx` |
| A client of its own, mocked from a file, cached on the contract's say-so | `app/clients/ledger.openapi.json`, `app/clients/ledger.mock.json` |
| React from the portal's vendor tree when running alone, since the import map names the shell's URLs | `shell` in `config/app.toml`, the `inferred` row naming the shell's `vendor/` |

## Run it alone

```sh
cargo run -p billing_site_react_ts
```

`http://127.0.0.1:8101/billing` is the site with its own layout as the page and React served from the portal's `app/vendor/`. There is no sign-in, so `/billing/overdue` redirects to a `/auth/login` that only the portal answers. `cargo test -p billing_site_react_ts` drives the standalone host.

## Run it under the portal

```sh
fsr dev app
```

From this directory, that finds the portal through `[site] shell` and serves it on the portal's address with this working tree mounted at `/billing` in place of `billing@1.0.0`, the other sites as the table says. Sign in as `alice` and open `/billing/overdue`: the guard lets you through on the portal's session. Edit a page and the portal reloads it. The portal's files are never written to. A site team outside this repository runs the same with `--under <dir>`, where `<dir>` is the portal's deploy tree from `fsr bundle`.
