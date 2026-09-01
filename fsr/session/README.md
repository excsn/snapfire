# snapfire_fsr_session

MPL-2.0. Pre-release, version 0.1.0, not published to crates.io.

The session layer for Snapfire FSR: it opens a request's session from the `Cookie` header, keeps two separate cells for it, saves them when the response starts and issues the CSRF tokens forms carry. The two cells are the point of the crate. `SessionCell` holds application-visible data and the resolved `Identity`, and it flows into `RequestCtx` where loaders, actions and evaluators read it. `TokenCell` holds backend credentials and auth flow state, it lives only on `Opened` and it never enters `RequestCtx`, so the only things that can reach it are the session layer, the auth flow and the service layer's outbound call chain. The cookie itself carries a signed opaque session id, never session data and never a credential. Task-by-task instructions are in [README.USAGE.md](README.USAGE.md); the surface is in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_session = { path = "../session" }
```

The crate has no Cargo features. It depends on `snapfire_fsr_core` for the value model and on `snapfire_fsr_runtime` for `SessionCell` and `Identity`, and it brings in `fibre_cache` for the in-process store, `hmac` and `sha2` for cookie signing, `rand` for id generation and `parking_lot` for the token cell's lock.

## What to reach for

| What you are doing | What to reach for |
| --- | --- |
| Open a request's session before matching begins | `Sessions::open` |
| Read or write data the application may see | `Opened::cell`, a `SessionCell` from `snapfire_fsr_runtime` |
| Hold a backend credential or auth flow state | `Opened::tokens`, a `TokenCell` |
| Save a dirty session and set the cookie a new one needs | `Sessions::persist` |
| Log a user out and expire the cookie | `Sessions::destroy` |
| Give a page a CSRF token to embed | `Sessions::csrf_token` |
| Check the token a form submitted back | `Sessions::verify_csrf` |
| Change the cookie name, lifetime or `Secure` flag | `SessionConfig` |
| Keep sessions in process memory | `MemorySessionStore` |
| Tune the store's shard count or hand it a built cache | `MemorySessionStore::sharded`, `MemorySessionStore::with_cache` |
| Keep sessions in Redis, Postgres or anything else | implement `SessionStore` |
| Sign and verify the cookie value | `HmacCodec` or your own `CookieCodec` |
| Name the thing the cookie carries | `SessionId` |

## Status

Pre-release and unpublished, with no stability guarantee on any signature here. It is exercised end to end by the `advanced_tera_app` example under `fsr/examples/`, which wires it into an actix-web adapter alongside `snapfire_fsr_auth` and `snapfire_fsr_service`. The crate carries nine integration tests in `tests/session.rs` covering the cookie round trip, tamper rejection, the clean-fresh-session case, identity persistence, destroy, CSRF binding, token custody and store tuning.
