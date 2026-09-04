# 200. The stock host and its configuration

The question this chapter answers: what runs an fsr application when nobody has written a server, where does it read its configuration and how do you know what it decided?

**For:** platform developers.

## The host is a library with a stock binary's worth of behaviour

`snapfire_fsr_host` turns a configuration directory, the plan file and the contracts into a service over HTTP types. It matches the route, opens the session from the cookie, runs the loaders, renders the page, serves the statics, answers actions and persists the session into the response. hyper serves it directly; axum can nest it; actix reaches it through a shim behind the `actix` feature, which is what the storefront uses so its two backends and the host can share one binary.

A stock host is two lines; the storefront's `main.rs` adds one Rust route between them and its two backends around them:

```rust
let host = Host::from(env!("CARGO_MANIFEST_DIR"))?.build()?;
snapfire_fsr_host::actix::serve(Arc::new(host), ("127.0.0.1", 8080)).await
```

`Host::from` takes a project root, a `config/` directory or a single file, finds the app directory from it, reads the plan file and the contracts under `generated/` and binds every row. Everything the builder returns before `build` is a seam [chapter 201](201-graduating-to-rust.md) uses; an application that needs none of them is those two lines.

## The configuration ladder

Configuration is TOML or YAML under `config/`, read as a ladder of files where a later one overrides an earlier one; the ladder is chosen by three environment variables rather than by listing files:

| Stem | From |
| --- | --- |
| `app` | always |
| `development` | `RELEASE_ENV`, default `development` |
| `local` | `APP_ENV`, default `local` |
| `<region>` | `APP_REGION`, when set |
| `<env>-<region>` | both, when the region is set |

For each stem the host reads `<stem>.toml` then `<stem>.yaml`, whichever exist, in that order, then lets `C5_`-prefixed environment variables override any key with `__` as the separator. A file that is absent is simply not on the ladder, so a checkout with only `app.toml` runs while a deployment adds `production.toml` and `production-eu.yaml` without touching the base. The report lists every file it read under `config`, in order, so the ladder is never a guess. Secrets follow the same ladder: c5store's encrypted values are written by c5cli into a YAML overlay, which is why a secret lives in a `.yaml` beside the `.toml` that holds the rest.

The sections are few. `[server]` names the listen address, the plan file and the contracts directory. `[document]` names the title, the shell, the entry script, the import map and the stylesheets. `[session]` holds the signing key, the store, the TTL, the capacity and whether the cookie is secure. `[cache]` turns on the render memo with a capacity and a lifetime; without it nothing is cached. `[clients.<name>]` gives each service its document and base URL. `[[static]]` maps a route to a directory.

## What the host infers

Most of `[document]` and `[[static]]` is never written, because the host infers it from the app directory and says so. The bundle's own facts file names the public path, so the static root for `dist/` and the entry script come from there. A `vendor/` directory is served at its conventional path. A `styles/` directory is served and every stylesheet in it is linked into the head. Each `[clients.<name>]` without a document gets `clients/<name>.openapi.json` or the `.proto` beside it. The boot report has an `inferred` section listing every one of those decisions:

```
inferred  document.entry from dist/.snapfire-build.json
          static /static/css from styles/
          clients.inventory.document from clients/
```

**Nothing the host decided is invisible.** That is the contract the report keeps with the person reading the log at three in the morning.

## The boot report

Boot prints the application's report, every route, source, action and rendered module with its owner, then the services with their transport and base URL, the static roots, the configuration files and the inferences. It is the same table the build printed, with the host's own rows added. A host that cannot bind a name refuses to boot with the name, rather than serving a plan it cannot answer; the failure modes are a source nothing answers, an action nothing answers, a route claimed twice without an override and a service with no transport.

## The lab

Start the storefront and read the `config` rows: one file, `config/app.toml`. Add `config/development.toml` containing only `[document]` with `title = "Deals"` and start again. The report lists both files and the tab reads Deals; nothing else changed, since the ladder merged one key. Now set `APP_ENV=staging` and start once more: `development.toml` is still read, since it is the release environment's file; a `staging.toml` would be next on the ladder if it existed. Remove the file when you are done.
