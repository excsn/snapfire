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
| `bundle` | always, last |

For each stem the host reads `<stem>.toml` then `<stem>.yaml`, whichever exist, in that order, then lets `C5_`-prefixed environment variables override any key with `__` as the separator. A file that is absent is simply not on the ladder, so a checkout with only `app.toml` runs while a deployment adds `production.toml` and `production-eu.yaml` without touching the base. The report lists every file it read under `config`, in order, so the ladder is never a guess. Secrets follow the same ladder: c5store's encrypted values are written by c5cli into a YAML overlay, which is why a secret lives in a `.yaml` beside the `.toml` that holds the rest.

`bundle.toml` is the last rung and a project does not write one. `fsr bundle` writes it into the deploy tree it produces, naming the paths that moved when the files were laid out; chapter 303 covers what it holds. It loads after every deployment overlay because those describe a deployment while it describes a directory and no deployment has an opinion about where in the tree its own plan file ended up.

The sections are few. `[server]` names the listen address, the plan file and the contracts directory. `[document]` names the title, the shell, the entry script, the import map and the stylesheets. `[session]` holds the signing key, the store, the TTL, the capacity and whether the cookie is secure. `[cache]` turns on the render memo with a capacity and a lifetime; without it nothing is cached. `server.dev` turns the live refresh on or off; absent, it is on whenever `RELEASE_ENV` is unset or `development`. `[locales]` names the locales the host serves, the default that goes unprefixed and whether a chosen prefix is remembered in a cookie. `[clients.<name>]` gives each service its document and base URL. `[[static]]` maps a route to a directory.

## What the host infers

Most of `[document]` and `[[static]]` is never written, because the host infers it from the app directory and says so. The bundle's own facts file names the public path, so the static root for `dist/` and the entry script come from there. A `vendor/` directory is served at its conventional path. A `styles/` directory is served and every stylesheet in it is linked into the head. Each `[clients.<name>]` without a document gets `clients/<name>.openapi.json` or the `.proto` beside it. The boot report has an `inferred` section listing every one of those decisions:

```
inferred  document.entry from dist/.snapfire-build.json
          static /static/css from styles/
          clients.inventory.document from clients/
```

**Nothing the host decided is invisible.** That is the contract the report keeps with the person reading the log at three in the morning.

## The origin a canonical link points at

A crawler reads `rel=canonical` and `rel=alternate` as absolute URLs only. A path on either is not a weaker version of the tag, it is an ignored one, so name the origin this deployment is reached at:

```toml
[document]
origin = "https://example.com"
```

The host then writes every path href on those two rels absolute, whichever of three places it came from. A `canonical()` a loader's `meta` returned. A `[[document.head]]` row. Its own locale canonical, the one pointing `/en_US/about` at `/about` so a prefixed request for the default locale is not a second page. An href that is already absolute is left exactly as written, which is how a cross-domain `alternate` still works. Every other `rel` keeps its path, since a crawler reads those relative to the document.

It is the scheme and the host and nothing else. A trailing slash or a path is refused at boot rather than producing `https://example.com//about` on a live page. A value with no scheme is refused the same way, since the scheme is the part a host name cannot supply on its own.

This is a deployment's one preferred origin, not the host a request arrived on. Those differ on purpose: two host names serving the same pages is exactly what a canonical link exists to collapse, so a self-referential one per host would assert both as originals and create the duplicate it is meant to prevent. An application that genuinely serves a different site per host wants `ctx.host` and a `canonical()` it builds itself, which passes through untouched because it is already absolute.

## The host this deployment answers on

A body reads `ctx.host`. It is null until `[server]` lists the hosts the deployment answers on:

```toml
[server]
hosts = ["example.com", "www.example.com"]
```

The list is an allowlist, not a format. A request's `Host` is lowercased and compared whole, port included, so a deployment on a port lists the port and `localhost:3000` is a separate entry from `localhost`. A header naming anything the list does not hold answers null rather than the value the client sent, so nothing a caller writes reaches a body unless the deployment already named it.

**The server in front must set the header.** `ctx.host` is only as trustworthy as whatever terminates the connection, because a value the list happens to hold is indistinguishable from the same value sent by hand. With nginx that is `proxy_set_header Host $host;` under a `server_name` that matches, plus a default server for the socket so a request naming something else is answered there instead of being passed through. The boot report says so on every start while the key is set:

```
hosts     example.com
          www.example.com
          ctx.host reads the request's Host against these; the server in front must set it; a client otherwise names its own
```

Leave the key out and the header is never read at all, which is the default and the right setting for an application that does not need it.

## The boot report

Boot prints the application's report, every route, source, action and rendered module with its owner, then the services with their transport and base URL, the static roots, the configuration files and the inferences. It is the same table the build printed, with the host's own rows added. A host that cannot bind a name refuses to boot with the name, rather than serving a plan it cannot answer; the failure modes are a source nothing answers, an action nothing answers, a route claimed twice without an override and a service with no transport.

## The lab

Start the storefront and read the `config` rows: one file, `config/app.toml`. Add `config/development.toml` containing only `[document]` with `title = "Deals"` and start again. The report lists both files and the tab reads Deals; nothing else changed, since the ladder merged one key. Now set `APP_ENV=staging` and start once more: `development.toml` is still read, since it is the release environment's file; a `staging.toml` would be next on the ladder if it existed. Remove the file when you are done.
