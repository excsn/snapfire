# snapfire_fsr_host

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_host.svg)](https://crates.io/crates/snapfire_fsr_host)
[![Docs.rs](https://docs.rs/snapfire_fsr_host/badge.svg)](https://docs.rs/snapfire_fsr_host)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

The stock host for SnapFire FSR. It reads `config/` through c5store, so files layer and `C5_` environment variables win, infers the rest from the app directory, reads the plan file and the contract `fsr build` wrote, binds every lowered loader and action, builds the service clients from the OpenAPI documents and `.proto` files the configuration names, over HTTP or gRPC, opens and persists sessions from the cookie and serves the static roots. All of that is one `tower::Service` over the `http` crate's request and response types with a streaming body, so it is served by the hyper listener the crate carries, nested into an axum router as it is or reached from actix through the shim behind the `actix` feature. A Rust host that wants to take a name back keeps the same builder: `source_override`, `action_override`, `route`, `evaluator`, `shell` and `services_over` are all here. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_host = "0.5"
```

| Feature | Adds |
| --- | --- |
| `actix` | `snapfire_fsr_host::actix::{handle, serve}`, the shim from actix's request and response types to the host's |
| `ws` | `/_sf/socket`, a WebSocket per topic: `HostBuilder::socket` answers what a page sends and the reply goes out as store rows. Adds `tokio-tungstenite` |
| `tls` | `[server.tls]`: the hyper listener terminates TLS with rustls over ring, ALPN chooses the version, and the configured signal re-reads the certificate. Adds `rustls`, `rustls-pki-types` and `tokio-rustls` |

No feature is needed for hyper or axum. The crate depends on `c5store` with `toml` for configuration, `snapfire_fsr` for the binding rule, `snapfire_fsr_service` for clients and contracts, `snapfire_fsr_session` for sessions, `http`, `http-body`, `tower`, `tower-http` with `fs` and hyper.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Start from a project root, its `config/` or one file | `Host::from`, `Host::from_cwd` |
| Override a setting per deployment | `config/<APP_ENV>.toml`, `config/<RELEASE_ENV>.toml`, `config/<APP_REGION>.toml` or `C5_SERVER__LISTEN` |
| Load one more file the ladder does not name | `config::locate` then `Located::extra`, `Host::from_located` |
| Serve with nothing but this crate | `Host::serve` |
| Mount inside an axum or tower stack | `Host::service`, a `tower::Service` |
| Serve with actix | the `actix` feature, `actix::serve` or `actix::handle` |
| Serve TLS without a proxy | the `tls` feature, `[server.tls]`, and SIGHUP after a renewal |
| Tell an open page that something changed | `Host::publish`, read by `/_sf/live` |
| Take what a page sends, keystroke by keystroke | the `ws` feature, `HostBuilder::socket`, over `/_sf/socket` |
| Answer one name in Rust | `HostBuilder::source_override`, `action_override`, `source`, `action` |
| Add a route in Rust | `HostBuilder::route`, `route_override` |
| Replace the document shell | `HostBuilder::shell` |
| Test without a backend | `HostBuilder::services_over` with a `MockTransport`, then `Host::render_to_string` and `Host::call_action` |
| Run without a backend | `[clients.<name>] transport = "mock"` over `clients/<name>.mock.json` |
| Reuse a backend's answers | `[cache.data]` over the contract's `cache` annotations, `Host::invalidate_tags` |
| Keep sessions somewhere else | `[session] store = "service"` behind a client, or `HostBuilder::session_store` |
| Sign users in | `[auth]` over `config/auth.toml`, `provider = "service"` asking a client, or `HostBuilder::identity` with any `IdentityProvider` |
| Send the session's token to one backend | `[clients.<name>] bearer = true` |
| See what was bound and served | `Host::report` |

| Serve a team's application under a path of yours, from its build output, one session and one navigation across both | `HostBuilder::mount` and `Mount`, or `snapfire_fsr_sites` over a `[sites]` table |
