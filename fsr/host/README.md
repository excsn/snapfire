# snapfire_fsr_host

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_host.svg)](https://crates.io/crates/snapfire_fsr_host)
[![Docs.rs](https://docs.rs/snapfire_fsr_host/badge.svg)](https://docs.rs/snapfire_fsr_host)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The server that runs a SnapFire FSR application.

Point it at a project and it serves what `fsr build` produced: pages, actions, route handlers, static files and sessions. Configuration comes from `config/app.toml` and the overlay files for each deployment. Environment variables override any value. Whatever you leave out is inferred from the app directory.

It is a `tower::Service` over the `http` crate's types. You can serve it with the hyper listener it carries, nest it in an axum router or reach it from actix.

`fsr dev` and `fsr serve` run it for you. An application with no Rust code of its own never depends on it. Depend on it when your application has its own server binary. That covers a backend called in process, a route or handler written in Rust, an override of a lowered loader or action and a server stack of your own.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_host = "0"
```

Serving with hyper or axum needs no feature.

| Feature | Default | Adds |
| --- | --- | --- |
| `client_readable` | on | The readable build of the browser client, beside the minified one. Off, the binary is about 215 KiB smaller and `/static/js/fsr` always answers with the minified build |
| `client_source_maps` | off | Source maps for the browser client, with its TypeScript inlined. Devtools then show the client's source. About 1.3 MB |
| `actix` | off | `actix::handle` and `actix::serve`, for serving the host from actix |
| `ws` | off | `/_sf/socket`, a WebSocket per topic. `HostBuilder::socket` answers what a page sends and the reply goes out as store rows |
| `auth` | off | `[auth]` and the `/auth/` routes: the `file` and `service` sign-in methods, several methods on one login page, a second factor and token refresh. Sessions, CSRF and token custody need no feature |
| `oidc` | off | `provider = "oidc"`, sign-in against an OpenID Connect issuer. Turns on `auth` |
| `passkey` | off | `provider = "passkey"`, WebAuthn passkeys. Turns on `auth` |
| `tls` | off | `[server.tls]`: the listener terminates TLS itself and re-reads the certificate on a signal |
| `share_cards` | off | Card images such as `opengraph-image.tsx`, drawn on request and at prerender |
| `tera` | off | `.tera` routes and layouts, rendered by the Tera evaluator |
| `grpc` | off | `clients/<name>.proto` contracts, called over gRPC |
| `graphql` | off | `clients/<name>.graphql` contracts |
| `http` | off | OpenAPI contracts called over HTTP. A mocked client is served without it |
| `sites_reload` | off | `POST /__fsr/sites/reload`, which reloads the mounted sites. The host does not guard this route |

When a configuration needs a feature the binary was built without, the host refuses to boot and names the feature. That covers an `[auth]` section, an `oidc` or `passkey` method, `[server.tls]`, a plan that draws card images and a `.proto`, `.graphql` or HTTP client.

## Quick start

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

`host.report()` prints what the host bound: the routes, the services, the static roots, the configuration files it read and what it inferred.

## What to reach for

The host reads `config/app.toml` first. Then it reads one overlay file each for the values of `RELEASE_ENV`, `APP_ENV`, `APP_REGION` and `<APP_ENV>-<APP_REGION>`, such as `config/production.toml`. Any of them may be `.yaml` instead of `.toml`. A later file overrides an earlier one. `C5_` environment variables override all of them.

| Problem | Piece |
| --- | --- |
| Start from a project root, its `config/` or one file | `Host::from`, `Host::from_cwd` |
| Override a setting per deployment | `config/<RELEASE_ENV>.toml`, `config/<APP_ENV>.toml`, `config/<APP_REGION>.toml` or a variable such as `C5_SERVER__LISTEN` |
| Load a configuration file the overlays do not cover | `Loader::at(root).extra(path)`, then `Host::from(loader)` |
| Keep `session.key` or a bearer key out of the file in the clear | A `.c5encval` value with its key under `config/private_keys` or in `C5_SECRETKEY_<name>`. `Loader::secrets` changes how it decrypts |
| Reload the application after a deploy | `Host::reload`. It rereads through the loader `Host::from` kept or calls the builder's `reloader` |
| Rotate the session key without signing everyone out | `session.previous_keys` and a reload. From Rust, `HostBuilder::keyring` and `Keyring::rotate` |
| Sign or encrypt the session cookie your own way | `HostBuilder::codec` with a `CookieCodec` |
| Keep a session alive past `session.ttl` | `session.extend(seconds)` in an action, a route handler or middleware. From Rust, `ctx.session.extend`. Nothing extends a session on its own |
| Serve with nothing but this crate | `Host::serve` |
| Mount inside an axum or tower stack | `Host::service`, a `tower::Service` |
| Serve with actix | The `actix` feature, then `actix::serve` or `actix::handle` |
| Serve TLS without a proxy | The `tls` feature, `[server.tls]` and SIGHUP after a renewal |
| Tell an open page that something changed | `Host::publish`, which `/_sf/live` streams to the page |
| Take what a page sends, keystroke by keystroke | The `ws` feature and `HostBuilder::socket`, over `/_sf/socket` |
| Answer one data source or action in Rust | `HostBuilder::source_override`, `action_override`, `source`, `action` |
| Add a route in Rust | `HostBuilder::route`, `route_override` |
| Replace the document shell | `HostBuilder::shell` |
| Test without a backend | `HostBuilder::services_over` with a `MockTransport`, then `Host::render_to_string` and `Host::call_action` |
| Run without a backend | `[clients.<name>] transport = "mock"` over `clients/<name>.mock.json` |
| Reuse a backend's answers | `[cache.data]` over the contract's `cache` annotations. `Host::invalidate_tags` drops answers by tag |
| Keep sessions somewhere else | `[session] store = "service"` behind a client. From Rust, `HostBuilder::session_store` |
| Sign users in | The `auth` feature and `[auth]` over `config/auth.toml`. `provider = "service"` asks a client. `oidc` and `passkey` need their features. `HostBuilder::identity` takes any `IdentityProvider` |
| Send the session's token to one backend | `[clients.<name>] bearer = true` |
| Let a browser keep a static file | `server.static_max_age`. Left out, a file with no hash in its name revalidates every time. `0` sends no header |
| Let a browser keep the application's modules across deploys | Nothing to do. Outside development the host names each bundle module by its content and serves that name `immutable` |
| Flatten the module waterfall before the first island mounts | `document.module_preload` |
| Serve the readable client instead of the minified one | `document.client = "readable"`. `auto` serves it while `server.dev` is on |
| Send a Content-Security-Policy | `[document.csp]` as directives and sources. The host adds the import map's hash and a nonce per response itself |
| Try a policy before enforcing it | `[document.csp_report_only]`, the same shape, sent as report-only |
| Serve `robots.txt` and a sitemap of what a signed-out visitor reaches | `[crawl]` with `document.origin` |
| Make the application installable and readable offline | `[pwa]`: a manifest from its fields and a service worker that keeps what a visitor opened |
| See what was bound and served | `Host::report` |
| Serve another team's application under a path of yours, with one session and one navigation across both | `HostBuilder::mount` with a `Mount`. `snapfire_fsr_sites` mounts a whole `[sites]` table |
