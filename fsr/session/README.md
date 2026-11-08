# snapfire_fsr_session

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_session.svg)](https://crates.io/crates/snapfire_fsr_session)
[![Docs.rs](https://docs.rs/snapfire_fsr_session/badge.svg)](https://docs.rs/snapfire_fsr_session)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Sessions for FSR applications: signed cookies, a session store, server-side token custody and CSRF tokens.

Each request's session is opened from its `Cookie` header and saved when the response starts. A session has two separate parts:

* `SessionCell` holds the data your application may read and write. It also holds the signed-in identity. Loaders, actions and templates see it.
* `TokenCell` holds backend credentials and sign-in state. Application code never sees it. Only the session layer, the sign-in flow in `snapfire_fsr_auth` and the outbound call chain in `snapfire_fsr_service` can reach it.

By default the cookie carries only a signed random session id. Sessions can also be kept entirely in the cookie, encrypted, with no store at all.

Most applications get sessions through `fsr` and its host. Depend on this crate directly when you build your own HTTP edge or store sessions somewhere other than process memory.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_session = "0"
```

## Quick start

```rust
use std::sync::Arc;

use snapfire_fsr_core::Value;
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};

let sessions = Sessions::new(Arc::new(MemorySessionStore::new(10_000)), b"a signing key of at least 32 bytes", SessionConfig::default());

let opened = sessions.open(None).await;
opened.cell.insert("theme", Value::str("dark"));

let set_cookie = sessions.persist(&opened).await?.expect("a new session that was written sets a cookie");

let again = sessions.open(Some(set_cookie.split(';').next().unwrap())).await;
assert_eq!(again.cell.get("theme"), Some(Value::str("dark")));
```

`persist` writes the store only when something changed. It returns a `Set-Cookie` only when the browser needs a new or updated cookie.

## What to reach for

| You want to | Use |
| --- | --- |
| Open a request's session before routing | `Sessions::open` |
| Read or write data the application may see | `Opened::cell`, a `SessionCell` from `snapfire_fsr_runtime` |
| Hold a backend credential or sign-in state | `Opened::tokens`, a `TokenCell` |
| Save a changed session and get the cookie a new one needs | `Sessions::persist` |
| Give a session a new id at sign-in | `Sessions::renew` |
| Log a user out and expire the cookie | `Sessions::destroy` |
| Give a page a CSRF token to embed | `Sessions::csrf_token` |
| Check the token a form sent back | `Sessions::verify_csrf` |
| Choose single-use, per-session or derived CSRF tokens | `SingleUse`, `PerSession`, `Derived`, `Sessions::with_csrf` |
| Change the cookie name, lifetime, size limit or `Secure` flag | `SessionConfig` |
| Keep sessions in process memory | `MemorySessionStore` |
| Tune the store's shard count or hand it a built cache | `MemorySessionStore::sharded`, `MemorySessionStore::with_cache` |
| Keep sessions in Redis, Postgres or anything else | implement `SessionStore` |
| Keep the whole session in an encrypted cookie | `Sessions::sealed` with a `SealedCookie` |
| Sign the cookie your own way | `HmacCodec` or your own `CookieCodec` |
| Rotate the signing key without signing everyone out | `Keyring`: `rotate` a new key in, `retire` the old one after the session lifetime |
| Keep a session alive past its end | `SessionCell::extend`, then `persist` |
