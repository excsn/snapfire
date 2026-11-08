# snapfire_fsr_auth

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_auth.svg)](https://crates.io/crates/snapfire_fsr_auth)
[![Docs.rs](https://docs.rs/snapfire_fsr_auth/badge.svg)](https://docs.rs/snapfire_fsr_auth)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Signs visitors in to an FSR application and keeps their backend tokens on the server.

A sign-in is a series of steps. Each step either redirects the browser, hands the login page some data to act on or finishes. `Auth` runs those steps over any `IdentityProvider` you plug in. When the flow finishes, the visitor's identity goes on the session and their tokens go into server-side custody, where application code cannot read them.

The crate renders nothing and serves no HTTP. Your login page is an ordinary route. Templates see the visitor through the `identity` prop. The login, callback and logout endpoints belong to whatever HTTP adapter your application uses.

Most applications get this through `fsr` and its host. Depend on it directly when you write your own identity provider or wire the flow into your own host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_auth = "0"
snapfire_fsr_core = "0"
snapfire_fsr_runtime = "0"
snapfire_fsr_session = "0"
```

Every `Auth` call takes an open session from `snapfire_fsr_session`.

## Quick start

A whole sign-in with the development provider, without HTTP:

```rust
use std::sync::Arc;

use futures::executor::block_on;
use snapfire_fsr_auth::{Auth, DevProvider, Start, Step};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};

let sessions = Sessions::new(Arc::new(MemorySessionStore::new(128)), b"dev-signing-key-32-bytes-long!!!", SessionConfig::default());
let auth = Auth::new(Arc::new(DevProvider::new().user("alice", "wonder")));
let opened = block_on(sessions.open(None));

let step = block_on(auth.begin(&opened, Start::sign_in("/dash"))).unwrap();
assert!(matches!(step, Step::Present { ref kind, .. } if kind == "form"));

let mut params = ValueMap::default();
params.insert("user".to_owned(), Value::str("alice"));
params.insert("password".to_owned(), Value::str("wonder"));

let step = block_on(auth.advance(&opened, params)).unwrap();
assert_eq!(step, Step::Complete { to: "/dash".to_owned(), signed_in: true });
assert_eq!(opened.cell.identity().unwrap().subject, "alice");
```

## What to reach for

| You want to | Use |
| --- | --- |
| Start a sign-in and learn what the browser does first | `Auth::begin` with `Start::sign_in` |
| Take the browser's answer and learn the next step | `Auth::advance` |
| Add a passkey or a linked account for whoever is signed in | `Auth::begin` with `Start::enrol` |
| Sign out and drop the backend tokens | `Auth::logout` |
| Plug in a provider of your own | `IdentityProvider` |
| Redirect the browser, hand the page data or finish a step | `Next` |
| Offer several methods on one login page | `Methods` |
| Require a second factor after another method | `Sequence` |
| Sign in against a fixed user table while developing | `DevProvider` |
| Return an identity with the tokens your backends need | `AuthOutcome` |
| Turn a failed step into an HTTP status | `AuthError::http_status` |
