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
* [Posting a Form to an Action](#posting-a-form-to-an-action)
* [Serving Locales](#serving-locales)
* [Signing In on the Host](#signing-in-on-the-host)
* [Keeping Sessions in a Service](#keeping-sessions-in-a-service)
* [Caching Rendered Segments](#caching-rendered-segments)
* [Caching Service Answers](#caching-service-answers)
* [Reloading the Application in Place](#reloading-the-application-in-place)
* [Serving a Site](#serving-a-site)
* [Mounting Sites](#mounting-sites)
* [Refreshing the Browser in Development](#refreshing-the-browser-in-development)
* [Taking a Name Back](#taking-a-name-back)
* [Replacing the Shell](#replacing-the-shell)
* [Testing Over a Mock Transport](#testing-over-a-mock-transport)
* [Running Without a Backend](#running-without-a-backend)
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
  print!("{}", host.report());
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
csrf = "identified"               # when a CSRF token is minted: once signed in, or always
store = "memory"                  # or "service", with client naming the [clients] entry that holds sessions

[cache]                           # optional: the render memo, nothing is cached without it
capacity = 1000                   # entries, the default

[cache.data]                      # optional: answer cached methods from memory
capacity = 500                    # entries per policy; cache.capacity when absent
ttl = "1m"                        # the default

[locales]                         # optional: without it every request is `en`
supported = ["en_US", "fr_FR"]
default = "en_US"                 # served unprefixed; the first supported one when absent
remember = true                   # write the cookie when a prefix chose the locale

[auth]                            # optional: without it there is no login and no /auth/ route
provider = "file"                 # the accounts in config/auth.toml; "service" asks a client; oidc is planned
login = "/login"                  # the application's login page, the default

[clients.shopping]
base_url = "http://127.0.0.1:8081"
bearer = true                     # send custody's access_token as a bearer; a string names another key

[clients.inventory]
transport = "mock"                # answer from clients/inventory.mock.json and reach nothing
responses = "clients/inventory.mock.json"   # the default; relative to the app directory

[site]                            # optional: this application is a site, see Serving a Site
name = "billing"                  # prefixes every id the build emits, billing:
at = "/billing"                   # every route and link sits under it

[sites]                           # optional: this application mounts sites, see Mounting Sites
root = "/srv/sites"               # where name@version artifacts resolve
poll = "30s"                      # reread the table this often; absent, only on SIGHUP

[sites.billing]
artifact = "billing@1.4.2"        # <root>/billing/1.4.2, or a path against the project root
hash = "3a098783bbb3ebc5"         # optional: refuse an artifact whose content hash differs
allow_engine = false              # refuse an artifact with engine-owned rows, the default
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

A Rust route titles its document the way a loader's `meta` does: `meta` describes the segment whose data source it names once that data has loaded, with the request context in hand, so a title can ask a service. The innermost described segment on the plan wins.

```rust
struct SectionTitle;

impl Metadata for SectionTitle {
  fn describe(&self, ctx: &RequestCtx, data: &Data) -> BoxFuture<'static, Result<Meta, LoadError>> {
    let section = ctx.params.get("section").cloned();
    Box::pin(async move { Ok(Meta { title: section.map(|s| format!("{s} - Fleet")), description: None }) })
  }
}

let host = Host::from(".")?.meta("layout_loader", Arc::new(SectionTitle)).build()?;
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

## Posting a Form to an Action

The action route takes a form as well as a fetch. A `POST` with a form-encoded body carries `_csrf`, which the host verifies against the session before the action runs; the other fields reach the action as strings; a success answers 303 back to the page that posted, by its `Referer`, and a failure answers the JSON error. The token is the `csrf_token` prop a page renders into a hidden input, minted once the session is identified, or for every session with `csrf = "always"`, which a form anonymous visitors post needs; that setting establishes the session on the first response so the token verifies on the next.

```html
<form method="post" action="/_sf/action/add_server">
  <input name="name">
  <input type="hidden" name="_csrf" value="{{ csrf_token }}">
  <button>add</button>
</form>
```

A payload request may name the encoding it wants with `enc`; `json` is the one that exists and anything else is 406.

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

A route with no parameter whose every source is lowered and reads nothing of the request renders the same for everyone. The boot report lists it under `prerender`. `prerender` renders each once per locale, anonymously, into the configured directory: the default locale at the top, every other under its tag, `fr_FR/about/index.html`. From then on the host answers a `GET` for it from the file with `x-sf-prerendered: 1`, session cookie and middleware still applied, the file chosen by the locale the request resolved to. A Rust source keeps its route dynamic, since the host cannot read what a Rust function reads, and so does a page or layout reading its `identity` or `csrf_token` prop, since a render for nobody cannot supply them. A route that reads only the locale still qualifies: that is what the render per locale is for.

```rust
let written = host.prerender(&host.report().prerender.clone().unwrap()).await?;
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

## Signing In on the Host

An `[auth]` section mounts the identity flow from `snapfire_fsr_auth` on three framework-owned routes, the way the action route is owned. `GET /auth/login` starts it, with `return_to` from the query, else the `Referer`'s path, else `/`, and only ever a path on this origin; the provider's `begin` says where the browser goes, which for the `file` provider is the application's login page with `return_to` in the query. The login page is the application's own route, since auth never renders; a `GET` of it seeds the flow when none is in progress, so a typed URL still posts somewhere. `/auth/callback` takes a form or JSON `POST`, or a `GET` carrying the provider's query; a success is a 303 to where the flow began, a refusal a 303 back to the login page with `error=denied` and the `return_to` it had, a callback with no flow in progress a 400. `POST /auth/logout` verifies `_csrf` from the form, or `x-sf-csrf` from a fetch, against the session, clears identity and custody, deletes the record and answers 303 `/` with the cookie expiring. None of the three takes a locale prefix.

The `file` provider is `DevProvider::from_toml` over `config/auth.toml`, read beside `app.toml` and never through the ladder, so an overlay cannot merge two tables of accounts:

```toml
[[users]]
name = "alice"
password = "wonder"
claims = { role = "admin" }
```

The `service` provider keeps no accounts at all. `client` names a `[clients.<name>]` entry whose contract declares `authenticate(user, password)` answering `subject`, `claims` and `access_token`; the callback sends the form's two fields there, a `401` or a `404` from the service is the denial and anything else the service answers is a 400. The token the service issued goes into custody, so the bearer rides the clients that ask for it the same as with `file`:

```toml
[auth]
provider = "service"
client = "identity"
login = "/login"

[clients.identity]
base_url = "http://127.0.0.1:8092"
```

A Rust host hands in any `IdentityProvider` instead, and the login page is `auth.login` when the section is written, `/login` otherwise:

```rust
let host = Host::from(".")?.identity(Arc::new(my_provider)).build()?;
```

Once a session is identified the host mints a CSRF token for it. Every render, middleware, handler and action runs with the session's token custody bound to its services, so a loader's outbound call carries what the callback stored. Bodies see `identity` and the `csrf_token` prop and never the custody. Which client sends the token is written per client, `bearer = true` for `access_token` or a string naming another custody key; a client without it sends nothing, so a third-party API never sees a user's credential. The boot report says which:

```
auth      file, login page /login, routes /auth/login, /auth/callback and /auth/logout
bearer    shopping               access_token
```

A guard is middleware, since the host does not know which routes are private:

```rust
let host = Host::from(".")?
  .middleware_override(|ctx, request| async move {
    let path = match &request {
      Value::Map(line) => line.get("path").cloned(),
      _ => None,
    };
    let mut out = ValueMap::new();
    if path == Some(Value::str("/account")) && ctx.session.identity().is_none() {
      out.insert("redirect".into(), Value::str("/auth/login?return_to=/account"));
    }
    Ok(Value::Map(out))
  })
  .build()?;
```

## Keeping Sessions in a Service

`[session] store = "service"` moves every session record out of the host's memory and behind a client, so a fleet of hosts shares one session and a restart forgets nothing. The client's contract declares three methods, `getSession(id)` answering `{ record }` or a `404`, `putSession(id, record)` and `deleteSession(id)`; the record travels as one string in the payload's JSON encoding, so the service stores an opaque blob and never learns the shape of a session. A client can hold both the sessions and the accounts, as the console's identity service does.

```toml
[session]
key = "a signing key"
store = "service"
client = "identity"
```

```text
services  identity               http        http://127.0.0.1:8092
session   service via identity
auth      service via identity, login page /login, routes /auth/login, /auth/callback and /auth/logout
```

A `getSession` that fails for a reason other than `404` is logged and the request runs anonymous; a `putSession` that fails is logged and the response still goes out. `HostBuilder::session_store` still wins over the section for a Rust host. Under `fsr test` sessions stay in memory whatever the section says, since a spec's mocks cannot hold them.

## Caching Rendered Segments

Every page and layout the build lowers carries its module name as its plan `cache_key`. With a `[cache]` section the host installs a bounded `FibreCache` and the runtime memoizes each rendered subtree under that key, the matched params, the identity subject, the CSRF token when a host sets one and a fingerprint of the subtree's loaded data. Loaders still run on every request, since data resolves before render; what a hit skips is evaluation. A changed answer is a different fingerprint and so a miss, never a stale hit, which is why nothing in the application declares a lifetime: `ttl` only bounds how long an entry nobody asks for again is kept.

```rust
host.render_to_string("/product/1", RenderMode::Html, SessionCell::default()).await?;
host.render_to_string("/product/1", RenderMode::Html, SessionCell::default()).await?;
assert_eq!(host.invalidate("routes/product/[id]/page.tsx#default").await, 1);
```

`invalidate` takes a module name and drops every entry under it, across all params and identities, and says how many went. A subtree with a streamed descendant, a failed source or the head slot is never written, so a page behind `loading.tsx` keeps its layout out of the cache too. Without a `[cache]` section nothing is cached and `invalidate` answers zero.

## Caching Service Answers

`[cache.data]` under the section installs the data cache from `snapfire_fsr_service` over every method whose contract declares `cache`, which an OpenAPI document spells as `x-sf-cache` on the operation and a Rust host as `Method::cached`. The report lists every cached method with its policy and every method that drops tags:

```text
cached    fleet.listAgents       ttl 15s shared [agents]
          fleet.listAlerts       ttl 15s shared [alerts]
writes    fleet.acknowledgeAlert [alerts]
```

A loader's call to a cached method is answered from memory for the policy's `ttl`, one entry per distinct arguments; a render that follows fingerprints the same data and so hits the render memo too. An identified call bypasses a `private` method's cache, shares a `shared` one and gets its own entry under `subject`, as the service guide says. An action that calls a method with `writes` drops the tags it names; `invalidate_tags` does the same from Rust.

```rust
host.invalidate_tags(["catalog"]);
```

Without `[cache.data]` no method is cached whatever the contract says. `fsr test` renders with it off, since a spec's mocks are the calls it counts.

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

## Running Without a Backend

A client whose `transport` is `mock` answers every call from a file and opens no connection, so the whole application runs with nothing else started. The contract is still imported from the document, so a method the document does not declare is refused before the file is consulted. An overlay in the configuration ladder is the natural place for it, so `APP_ENV=mock` switches a deployment over and nothing else changes.

```toml
# config/mock.toml
[clients.fleet]
transport = "mock"
```

The file is an object of method name to response, written in the payload's JSON encoding, so a plain number is an integer or a double by its shape and the tagged forms carry `bigint`, typed arrays and the rest. A `$fail` entry answers with a failure of the named kind, which is how a degraded segment is rehearsed.

```json
{
  "listAgents": [{ "id": 1, "name": "builder-eu-1", "region": "eu", "status": "up", "queue_depth": 3, "cpu": 61.5 }],
  "acknowledgeAlert": { "$fail": { "kind": "unavailable", "message": "the mock records nothing" } }
}
```

The report names the file where a live client would show its base URL:

```text
services  fleet                  mock        clients/fleet.mock.json
```

`base_url` may be left out of a mock client; every other client needs one. A `transport` other than `mock` is a configuration error.

## Reloading the Application in Place

Everything a request reads, the plan, the contracts, the clients, the head, the static roots, the locales and the identity flow, is one set of tables the host swaps whole. `reload` rebuilds them through the reloader the builder was given, checks them the way a boot does and swaps them in; a request already running finishes on the tables it started with, the next one sees the new ones. The sessions are not part of the tables, so every signed-in user stays signed in across a reload, and a reload whose `[session]` settings differ from the running ones is refused and leaves the tables alone.

```rust
let host = Host::from(".")?
  .reloader(|| Host::from(".").map(|b| b.source_override("catalog", catalog)))
  .build()?;
let report = host.reload()?;
print!("{report}");
```

The reloader is a builder for the application as it now stands, with whatever the first builder added in Rust added again. `fsr serve` sets one that rereads the project, and `fsr dev` asks for it after a change to the generated files instead of restarting the process. A Rust host with no reloader is reloaded with a builder it made itself, `reload_with`.

## Serving a Site

An application with a `[site]` section is a site: `fsr build` prefixes every id it emits with `<name>:` and puts every route under `at`, so two sites can carry the same files, and the host serves it alone the same way it serves any application. A site's clients register under the prefix too, `billing:ledger`, since its bodies were lowered to call them by that name; the report's `site` row names the site and its prefix and every other row shows the prefixed ids. Its stylesheets are inferred under `<at>/static/css` and its bundle under the public path the build was given, so both keep working once the site is mounted.

```toml
[site]
name = "billing"
at = "/billing"
shell = "../portal/app/generated/shell.json"   # what the site is built against, see the cli guide
```

```rust
let host = Host::from("../billing")?.build()?;
assert!(host.report().to_string().contains("site      billing                at /billing"));
let html = host.render_to_string("/billing/invoice/1", RenderMode::Html, SessionCell::default()).await?;
assert!(html.contains("data-sf-module=\"billing:routes/invoice/[id]/page.tsx#default\""));
```

Nothing in a site's own code knows it is one: routes are written as `routes/invoice/[id]/`, links as `/billing/invoice/1`, since a link is served exactly as written, and a body calls `services.ledger` while the build spells it `billing:ledger` in the plan.

## Mounting Sites

The host mounts a site from its artifact, the project directory a site's build leaves behind with `config/` beside `app/`, and serves it under the site's prefix with the shell's root layout wrapped around it. One document, one session, one navigation across the shell and every site.

```rust
use snapfire_fsr_host::{Host, Mount};

let billing = Mount::load("billing", "/srv/sites/billing/1.4.2", "1.4.2", "3a098783bbb3ebc5", false)?;
let host = Host::from(".")?.mount(billing).build()?;
```

What a mount does, in order: it reads the artifact's configuration through the shell's ladder and refuses one whose `[site]` names another site; it refuses an artifact with engine-owned rows unless `allow_engine` is set, and one whose bundle carries a server module; it takes the site's middleware aside; it nests every route and intercept of the site under the shell's `routes/layout.tsx#default`, or under the document alone when the shell has no root layout; it adds the site's rows to the shell's tables under their prefixed ids and merges the site's contract; it registers the site's clients under `<name>:<client>` and their bearer keys; it serves the site's static roots that sit under its prefix and skips the rest, since the shell serves `/static/js/fsr` and the vendor tree itself; it adds the site's import map entries the shell lacks; and it ignores the site's `[session]`, `[auth]`, `[locales]` and `[cache]`, which are the shell's. The report prints one `sites` row per mount with its prefix, artifact, version and hash, and one more naming what was ignored.

A request under a site's prefix runs the shell's middleware first, with `request.site` naming the site, and then the site's, on the same path; a site's middleware may redirect, respond, add headers or rewrite within its own prefix, and never sees the shell's. The document adds the site's stylesheets and entry module to the head on the site's routes, and a payload for one carries an `E` row so the navigator loads the site's islands on first arrival. `GET /__fsr/sites` answers with every mounted site's name, prefix, version and hash, for a monitor to compare against the table.

`snapfire_fsr_sites` turns the `[sites]` table into mounts, hashes each artifact, refuses a pinned hash that differs and rereads the table on `SIGHUP` or a poll, so `fsr serve` and a Rust shell built with it need no code beyond `mount_all` and `watch`.

## Refreshing the Browser in Development

In development, which is what `RELEASE_ENV` unset means, every served document carries a small script and the host answers two more paths. The script opens `GET /__fsr/events`, a server-sent event stream, and `POST /__fsr/changed` tells every open document that something changed. `POST /__fsr/reload` calls `reload` and answers with the new report, or the error. `fsr dev` posts `changed` after a rebundle and `reload` after a change to the generated files; a restart drops the stream and the browser reconnects on its own. A Rust host announces the same thing itself:

```rust
host.changed();
```

Every event names the bundle the server sees now, a hash over the modules `dist/.snapfire-build.json` lists. A document rendered against a different bundle reloads, since the modules it hydrated with are stale. The same bundle means only the server side or a stylesheet moved: the script re-links every stylesheet with a fresh query string and asks the client library's `refresh` to fetch the route's payload and patch it in place, so layouts keep their DOM and state; a page without the client library reloads instead. Static files are served with `Cache-Control: no-cache` in development so a reload revalidates them.

`dev = false` under `[server]` turns all of it off, `dev = true` turns it on whatever the environment, and `prerender` never writes the script. The boot report prints one `dev` row while it is on.

## Reading the Report

`Host::report` is the application's report as of the last reload, plus the `site` row when the application is a site, the `sites` rows when it mounts any, and the services reached, the static roots served, the configuration read and what was inferred. Printed at boot it reads:

```
routes    /                      plan file
          /about                 rust
sources   index                  lowered
actions   cart.checkout          lowered
services  shopping               http        http://127.0.0.1:8081
static    /static/js/app         /srv/shop/app/dist
cache     1000 entries, ttl 1m
auth      file, login page /login, routes /auth/login, /auth/callback and /auth/logout
bearer    shopping               access_token
dev       live refresh on /__fsr/events, told by POST /__fsr/changed
config    /srv/shop/config
inferred  static /static/js/app from dist/.snapfire-build.json
          document.entry from dist/.snapfire-build.json
```

## Error Handling

`HostError` is what `Host::from`, `build` and `render` return. `NoConfig` is a path with no `config/` or `app.toml`, `Config` carries the source and the loading or deserialising error, `Value` names a setting that did not parse, `Bind` is the binding rule from `snapfire_fsr`, `Import` a document that did not import, `NotFound` a path no route matches, `Leak` a bundle under `dist/` carrying a loader, an actions module, a handler or the middleware, or importing one, each named with its reason, `Mount` a site that could not be mounted, naming the site and why.

```rust
use snapfire_fsr_host::HostError;

match Host::from(".").and_then(|b| b.build()) {
  Ok(host) => host,
  Err(HostError::Bind(e)) => return refuse(format!("the plan and the Rust disagree: {e}")),
  Err(e) => return refuse(e.to_string()),
}
```
