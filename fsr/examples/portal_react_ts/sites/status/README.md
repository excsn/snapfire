# status

A status page as a site the portal mounts under `/status`: one table of systems, their owners and their state, built from a module constant in `src/systems.ts`. Like the blog it has no Rust project and vendors nothing, taking React from the shell contract.

What it demonstrates is how it arrives. The portal's table names it as `status@1.0.0` and its cache does not hold that version, so the portal fetches the archive from `[sites] store`, verifies every file against the manifest packed with it and only then mounts it.

| It shows | Where |
| --- | --- |
| A site the shell fetches from a store instead of one an operator installs | `[sites.status] artifact = "status@1.0.0"` and `[sites] store` in the portal's `config/app.toml` |
| A release as an archive with a manifest | `fsr sites pack sites/status --version 1.0.0` |
| Stylesheet rules scoped to the site, so nothing lands on the portal's pages | `.status-*` selectors in `app/styles/status.css` |

## Run it alone

```sh
fsr build app
fsr serve app
```

`http://127.0.0.1:8103/status` is the page with its own layout, React served from the portal's `app/vendor/`.

## Under the portal

The portal's README packs this site into the store, serves the store and runs the portal, which fetches it on the first boot.
