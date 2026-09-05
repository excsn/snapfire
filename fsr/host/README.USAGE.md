# Usage Guide: snapfire_fsr_host

How to write `config/app.toml`, what the host infers so the file stays short, how to override a setting per deployment, start the host, serve it with hyper, axum or actix, take a name back in Rust and test an application without its backend.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Laying Out the Project](#laying-out-the-project)
* [Writing config/app.toml](#writing-configapptoml)
* [What the Host Infers](#what-the-host-infers)
* [Overriding per Deployment](#overriding-per-deployment)
* [Serving with hyper](#serving-with-hyper)
* [Mounting in axum](#mounting-in-axum)
* [Serving with actix](#serving-with-actix)
* [Adding a Route in Rust](#adding-a-route-in-rust)
* [Serving Locales](#serving-locales)
* [Caching Rendered Segments](#caching-rendered-segments)
* [Refreshing the Browser in Development](#refreshing-the-browser-in-development)
* [Taking a Name Back](#taking-a-name-back)
* [Replacing the Shell](#replacing-the-shell)
* [Testing Over a Mock Transport](#testing-over-a-mock-transport)
* [Reading the Report](#reading-the-report)
* [Error Handling](#error-handling)

## Core Concepts

* **Project root** is the directory holding `config/`; `Host::from` takes it, a `config/` directory or one file.
* **App directory** is `[app] dir` under the project root, `app` by default; every path in the configuration and everything inferred resolves against it.
* **Config directory** is `config/`. The host loads a fixed ladder out of it through c5store, `app.toml` first and the deployment overlays after it, later files overriding earlier ones, then `C5_` environment variables over all of them. A file the ladder does not name is not read.
* **Deployment** is three environment variables: `RELEASE_ENV` (default `development`), `APP_ENV` (default `local`) and `APP_REGION` (unset by default). Each names an overlay file.
* **Plan file** is `generated/plan.json`, written by `fsr build`, with routes and lowered bodies.
* **Contracts** are `generated/contracts/*.json`, the same build's output, one file per client document plus `schemas.json`; the host merges them at boot, refusing a type or service two files define, then checks a lowered action's input against the result.
* **Client** is a `[clients.<name>]` entry: a document and a base URL, imported into one service registry with a transport per client, HTTP for an OpenAPI document and gRPC for a `.proto`.
* **Shell** is the evaluator for the document module every route's root node names, `shell#document` by default; the stock one emits the doctype, the head and the mount point.
* **Head** is what the stock shell puts in `<head>`: the title, a `<link>` per stylesheet, the inlined import map and the entry module from `[document]`.
* **Static root** is a `[[static]]` entry, a route prefix served from a directory by `tower-http`'s `ServeDir`, checked before any route.
* **Session** is opened from the cookie on every request and persisted into the response when it changed, through `snapfire_fsr_session`.
* **Edge** is `Host::handle`: one `http::Request<Bytes>` in, one streaming `http::Response` out, covering static roots, `/_sf/action/<id>` and pages in either mode.
* **Service** is `Host::service`, the same edge as a `tower::Service`, so any tower stack drives it.

## Quick Start

```rust
use std::sync::Arc;
use snapfire_fsr_host::Host;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let host = Host::from_cwd().and_then(|b| b.build()).map_err(std::io::Error::other)?;
  print!("{}", host.report);
  let listen = host.listen().to_owned();
  Arc::new(host).serve(&listen).await
}
```

## Laying Out the Project

```
shopping/
  config/
    app.toml               # deployment facts
    local.toml             # the APP_ENV overlay, loaded after app.toml
  app/                     # routes, schemas, clients, generated/, dist/, vendor/
  src/main.rs              # optional: the backend, a Rust route, an override
```

## Writing config/app.toml

Only what cannot be inferred: where to listen, how to sign sessions, where each service lives.

```toml
[server]
listen = "127.0.0.1:8080"
prerender = "dist/prerender"      # optional: where rendered-once routes live
dev = false                       # optional: live refresh; absent, on unless RELEASE_ENV is set to something else than development

[document]
title = "Shopping"

[session]
key = "a signing key"             # required
ttl = "8h"                        # 30s, 15m, 8h, 2d or seconds

[cache]                           # optional: the render memo, nothing is cached without it
capacity = 1000                   # entries, the default
ttl = "1m"                        # the default

[locales]                         # optional: without it every request is `en`
supported = ["en_US", "fr_FR"]
default = "en_US"                 # served unprefixed; the first supported one when absent
remember = true                   # write the cookie when a prefix chose the locale

[clients.shopping]
base_url = "http://127.0.0.1:8081"
```

A key the host does not know is an error naming the key. So is a section it does not know, so a typo cannot silently do nothing.

## What the Host Infers

From the app directory, each reported at boot under `inferred`:

| Setting | Inferred from |
| --- | --- |
| the bundle's static route | `dist/.snapfire-build.json`'s `publicPath`, serving `dist` there |
| `document.entry` | the same file's `src/main.js` entry under that path |
| `document.import_map` | `importmap.json` in the app directory |
| a `/static/js/vendor` root | `vendor/` in the app directory |
| a `/static/css` root and `document.styles` | `styles/` in the app directory, every `.css` in it linked from the head in name order |
| `clients.<name>.document` | `clients/<name>.openapi.json`, or `clients/<name>.proto` when only that exists |

Anything written in the file wins over the inference. `[[static]]` entries add roots the conventions do not cover, with `dir` relative to the app directory.

## Overriding per Deployment

The files in `config/` load in this order, each `.toml` then `.yaml`, skipping the ones that do not exist:

| File | Named by |
| --- | --- |
| `app` | always |
| `<RELEASE_ENV>` | `RELEASE_ENV`, default `development` |
| `<APP_ENV>` | `APP_ENV`, default `local` |
| `<APP_REGION>` | `APP_REGION`, no default |
| `<APP_ENV>-<APP_REGION>` | both |

So `config/production.toml` is read when `APP_ENV=production` and ignored otherwise; `config/local.toml` is a developer's machine by default. A file with any other name is never read. Every key is also reachable from the environment with the `C5_` prefix and `__` between levels, so a container sets the listen address without a file:

```sh
APP_ENV=production C5_SERVER__LISTEN=0.0.0.0:80 C5_SESSION__KEY=... ./shop
```

A file the ladder does not name, a path from a flag or a secrets file mounted elsewhere, goes on the end with `Located::extra`; a relative path resolves against the config directory.

```rust
use snapfire_fsr_host::config::locate;

let located = locate(Path::new("."))?.extra("/run/secrets/shop.toml");
let host = Host::from_located(located)?.build()?;
```

The report prints which files were read in order and what was inferred.

## Serving with hyper

`serve` binds the address and runs HTTP/1 connections on it; `serve_listener` takes a listener you bound, which is how a test picks port zero.

```rust
let host = Arc::new(host);
host.clone().serve("127.0.0.1:8080").await?;

let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
host.serve_listener(listener).await?;
```

## Mounting in axum

The host is a `tower::Service`, so an existing router nests it under a prefix and keeps its own middleware.

```rust
use axum::Router;

let app = Router::new().nest_service("/shop", host.service());
```

## Serving with actix

With the `actix` feature, `actix::serve` runs the host on its own. `actix::handle` is a handler for an existing `App`'s default service.

```rust
snapfire_fsr_host::actix::serve(host, ("127.0.0.1", 8080)).await?;
```

```rust
App::new().app_data(Data::new(host.clone())).default_service(web::to(snapfire_fsr_host::actix::handle))
```

## Adding a Route in Rust

A route the file system convention does not describe, beside the ones the plan file carries. A pattern the plan already claims is refused unless `route_override` says so.

```rust
use snapfire_fsr::Plan;

let host = Host::from(".")?
  .route("/about", Plan::of("shell#document").slot("content", Plan::of("src/About.tsx#default")))
  .build()?;
```

`not_found` sets the tree the host renders, with status 404, for a path no route matches; it replaces the one `routes/not-found.tsx` put in the plan file. The page receives the path it is answering as `params.path`.

```rust
let host = Host::from(".")?
  .not_found(Plan::of("shell#document").slot("content", Plan::of("src/Missing.tsx#default")))
  .build()?;
```

## Adding a Handler in Rust

A handler answers a method and a pattern with a value rather than a document, before any page is matched. The plan file carries the ones `route.ts` exports; `handler` adds one in Rust and `handler_override` replaces a lowered one.

```rust
use snapfire_fsr_core::Value;

let host = Host::from(".")?
  .handler("GET", "/api/health", |_ctx, _input| async { Ok(Value::str("ok")) })
  .build()?;

let answer = host.call_handler("GET", "/api/health", session, Value::Null).await?;
```

## Middleware in Rust

The plan's `middleware.ts` runs before every request; `middleware_override` replaces it with a Rust function of the request context and the request line, whose value the host reads as a `Preflight`. `preflight` runs it without a request, for a test.

```rust
use snapfire_fsr_host::{Preflight, PreflightAction};

let host = Host::from(".")?
  .middleware_override(|_ctx, request| async move {
    let mut out = ValueMap::new();
    if let Value::Map(line) = &request {
      if line.get("path") == Some(&Value::str("/old")) {
        out.insert("redirect".into(), Value::str("/new"));
      }
    }
    Ok(Value::Map(out))
  })
  .build()?;

assert_eq!(host.preflight("GET", "/old", session).await?.action, PreflightAction::Redirect { to: "/new".into(), status: 307 });
```

## Prerendering the Routes That Never Change

A route with no parameter whose every source is lowered and reads nothing of the request renders the same for everyone. The boot report lists it under `prerender`. `prerender` renders each once per locale, anonymously, into the configured directory: the default locale at the top, every other under its tag, `fr_FR/about/index.html`. From then on the host answers a `GET` for it from the file with `x-sf-prerendered: 1`, session cookie and middleware still applied, the file chosen by the locale the request resolved to. A Rust source keeps its route dynamic, since the host cannot read what a Rust function reads. A route that reads only the locale still qualifies: that is what the render per locale is for.

```rust
let written = host.prerender(&host.report.prerender.clone().unwrap()).await?;
for (pattern, file) in written {
  println!("{pattern} -> {}", file.display());
}
assert!(host.prerendered("/about", RenderMode::Html).is_some());
```

`fsr prerender <app>` does the same for the stock host. Delete the directory to go back to rendering per request.

## Serving Locales

A `[locales]` section makes the locale a request attribute the host resolves before anything else, the way it resolves the session. The sources are consulted in `order`, `prefix`, `cookie` and `header` by default, and the first that answers wins. A path prefix is a supported tag in any case or separator, `/fr_FR/about`, `/fr-fr/about` or `/FR_FR/about`, stripped before the route matches, so no route carries a locale segment. The cookie is `sf_locale` unless `cookie` says otherwise. `Accept-Language` is matched by weight, exact tag first and then language alone, so `fr-CA` reaches `fr_FR`. Nothing answering, the default serves.

```toml
[locales]
supported = ["en_US", "fr_FR", "de"]
default = "en_US"
order = ["prefix", "cookie", "header"]
remember = true
cookie = "sf_locale"
```

The default locale is served unprefixed and may be prefixed too: `/en_US/about` renders the same page as `/about` with a canonical link pointing at the bare path. With `remember` on, a prefix that chose a locale the cookie does not hold writes the cookie, so an unprefixed link from a French page stays French. A link is served exactly as written; the host never adds a prefix to a URL the application did not write.

What the application sees: `ctx.locale` in every loader, action, handler and middleware, spelled as `supported` spells it; a `locale` prop on every node the assembler renders, which the shell writes as `<html lang="fr-FR" data-sf-locale="fr_FR">`; every segment key marked `@fr_FR` outside the default locale, so a switch swaps every segment and misses the render memo; an `L` row in the payload. Middleware reads the stripped path. An action takes the locale of the document that called it, from the `x-sf-from` header the client sends, then the cookie and the header. Static roots, the action route and the live-refresh endpoints are never prefixed.

```rust
let host = Host::from(".")?.build()?;
assert_eq!(host.locales().default, "en_US");
let html = host.render_to_string("/fr_FR/about", RenderMode::Html, SessionCell::default()).await?;
assert!(html.contains("<html lang=\"fr-FR\" data-sf-locale=\"fr_FR\">"));
let value = host.call_action_in("cart.checkout", session, host.locales().locale("fr_FR"), input).await?;
```

Without the section there is one locale, `en`, no source is consulted and nothing is a prefix.

## Caching Rendered Segments

Every page and layout the build lowers carries its module name as its plan `cache_key`. With a `[cache]` section the host installs a bounded `FibreCache` and the runtime memoizes each rendered subtree under that key, the matched params, the identity subject, the CSRF token when a host sets one and a fingerprint of the subtree's loaded data. Loaders still run on every request, since data resolves before render; what a hit skips is evaluation. A changed answer is a different fingerprint and so a miss, never a stale hit, which is why nothing in the application declares a lifetime: `ttl` only bounds how long an entry nobody asks for again is kept.

```rust
host.render_to_string("/product/1", RenderMode::Html, SessionCell::default()).await?;
host.render_to_string("/product/1", RenderMode::Html, SessionCell::default()).await?;
assert_eq!(host.invalidate("routes/product/[id]/page.tsx#default").await, 1);
```

`invalidate` takes a module name and drops every entry under it, across all params and identities, and says how many went. A subtree with a streamed descendant, a failed source or the head slot is never written, so a page behind `loading.tsx` keeps its layout out of the cache too. Without a `[cache]` section nothing is cached and `invalidate` answers zero.

## Taking a Name Back

The binding rule from `snapfire_fsr` applies unchanged. A lowered source or action is a default; Rust replaces it with an override. The report says `rust override`.

```rust
let host = Host::from(".")?
  .source_override("pricing", |ctx| async move { pricing::load(ctx).await })
  .action_override("cart.checkout", |ctx, input| async move { checkout::run(ctx, input).await })
  .build()?;
```

Binding a lowered name with plain `source` or `action` is refused, since the file already answers it.

## Replacing the Shell

The stock shell emits the doctype, the head slot and `<div id="app">` with the content slot. Give another evaluator for the document module to change the document.

```rust
let host = Host::from(".")?.shell(Arc::new(MyShell)).build()?;
```

## Testing Over a Mock Transport

`services_over` keeps the contract the configuration names and answers every call from the given transport, so a test needs no backend.

```rust
use snapfire_fsr_host::{Host, RenderMode};
use snapfire_fsr_runtime::SessionCell;
use snapfire_fsr_service::MockTransport;

let transport = Arc::new(MockTransport::new().returns("shopping.listProducts", products));
let host = Host::from(env!("CARGO_MANIFEST_DIR"))?.services_over(transport.clone()).build()?;

let html = host.render_to_string("/", RenderMode::Html, SessionCell::default()).await?;
let session = SessionCell::default();
let out = host.call_action("cart.addToCart", session.clone(), input).await?;
assert_eq!(transport.calls().len(), 1);
```

`handle` drives the whole edge, cookies included, without a socket.

## Refreshing the Browser in Development

In development, which is what `RELEASE_ENV` unset means, every served document carries a small script and the host answers two more paths. The script opens `GET /__fsr/events`, a server-sent event stream, and `POST /__fsr/changed` tells every open document that something changed. `fsr dev` posts it after a rebundle that did not restart the server; a restart drops the stream and the browser reconnects on its own. A Rust host announces the same thing itself:

```rust
host.changed();
```

Every event names the bundle the server sees now, a hash over the modules `dist/.snapfire-build.json` lists. A document rendered against a different bundle reloads, since the modules it hydrated with are stale. The same bundle means only the server side or a stylesheet moved: the script re-links every stylesheet with a fresh query string and asks the client library's `refresh` to fetch the route's payload and patch it in place, so layouts keep their DOM and state; a page without the client library reloads instead. Static files are served with `Cache-Control: no-cache` in development so a reload revalidates them.

`dev = false` under `[server]` turns all of it off, `dev = true` turns it on whatever the environment, and `prerender` never writes the script. The boot report prints one `dev` row while it is on.

## Reading the Report

`Host::report` is the application's report plus the services reached, the static roots served, the configuration read and what was inferred. Printed at boot it reads:

```
routes    /                      plan file
          /about                 rust
sources   index                  lowered
actions   cart.checkout          lowered
services  shopping               http        http://127.0.0.1:8081
static    /static/js/app         /srv/shop/app/dist
cache     1000 entries, ttl 1m
dev       live refresh on /__fsr/events, told by POST /__fsr/changed
config    /srv/shop/config
inferred  static /static/js/app from dist/.snapfire-build.json
          document.entry from dist/.snapfire-build.json
```

## Error Handling

`HostError` is what `Host::from`, `build` and `render` return. `NoConfig` is a path with no `config/` or `app.toml`, `Config` carries the source and the loading or deserialising error, `Value` names a setting that did not parse, `Bind` is the binding rule from `snapfire_fsr`, `Import` a document that did not import, `NotFound` a path no route matches.

```rust
use snapfire_fsr_host::HostError;

match Host::from(".").and_then(|b| b.build()) {
  Ok(host) => host,
  Err(HostError::Bind(e)) => return refuse(format!("the plan and the Rust disagree: {e}")),
  Err(e) => return refuse(e.to_string()),
}
```
