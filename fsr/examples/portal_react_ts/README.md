# portal_react_ts

The shell of a company site: a header, a team directory and a sign-in, with three sites other teams own mounted under it at `/billing`, `/blog` and `/status`. One document, one session, one navigation across all of them.

Each site reaches the portal a different way, which is the point of having three:

| Site | `[sites.<name>] artifact` | How it gets there |
| --- | --- | --- |
| `blog` | `sites/blog` | A linked working tree. Rebuilt in place, mounted as version `path`, never pinned. The development shape. |
| `billing` | `billing@1.0.0` | A versioned install. Packed into an archive, installed into the cache under `[sites] root` by an operator, then mounted from there. |
| `status` | `status@1.0.0` | Fetched by the portal. The cache lacks the version, so the portal takes it from `[sites] store`, an HTTP server over a directory of archives, verifies it and mounts it. |

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
| A header whose section link stays marked while a page under it is shown, across the mount | `match="prefix"` in `app/src/ui/Header.tsx`, `a[aria-current]` in `app/styles/app.css` |

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
fsr sites install . deploy/archives/billing-1.0.0.tar.gz --no-pin
```

`install` verifies every file against the archive's manifest before renaming it into `deploy/sites/billing/1.0.0`. Without `--no-pin` it writes the hash into `[sites.billing]`, which is what a production table does. It is left off here because the hash changes on every rebuild.

Publish status to the store and serve the store:

```sh
fsr sites pack sites/status --version 1.0.0 --out deploy/store/status-1.0.0.tar.gz
python3 -m http.server 8199 --bind 127.0.0.1 --directory deploy/store
```

Any server that answers `GET /<name>-<version>.tar.gz` is a store: nginx over the directory, a bucket, an Artifactory generic repository. `FSR_SITES_STORE_HEADER="Authorization: Bearer <token>"` adds a header to every fetch.

Then run the portal in another terminal:

```sh
cargo run -p portal_react_ts
```

The first boot logs `fetched status@1.0.0 from http://127.0.0.1:8199` and the report lists all three under `sites`. Open `http://127.0.0.1:8100/`, sign in as `alice` / `wonder` and follow Billing, Blog and Status. `GET /__fsr/sites` shows billing and status at `1.0.0` and blog at `path`.

To ship status 1.1.0, pack it into the store and move `[sites.status] artifact` to `status@1.1.0`. The poll notices the row moved, fetches the version and swaps the tables. A request in flight finishes on the old ones.

`cargo test -p portal_react_ts` does the same install and fetch in process, with the store on a free port.
