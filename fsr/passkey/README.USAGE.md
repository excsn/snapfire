# Usage Guide: snapfire_fsr_passkey

How to sign users in with passkeys on the stock host or from Rust, beside another method or as a second factor, and where the passkeys are kept.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Offering Passkeys on the Stock Host](#offering-passkeys-on-the-stock-host)
  * [Requiring a Passkey After a Password](#requiring-a-passkey-after-a-password)
  * [Adding a Passkey From a Settings Page](#adding-a-passkey-from-a-settings-page)
* [Building the Provider in Rust](#building-the-provider-in-rust)
* [Keeping Passkeys Elsewhere](#keeping-passkeys-elsewhere)
* [Error Handling](#error-handling)

## Core Concepts

* **Passkey** is a key pair the visitor's device holds for one site; the site keeps only the public half.
* **Relying party ID** is the domain the passkey is bound to, the origin's host. It is never an IP address, so development runs at `localhost`.
* **Ceremony** is the browser's prompt: `navigator.credentials.create` enrols, `get` signs in.
* **Challenge** is a fresh random value each ceremony signs, so an answer cannot be replayed.
* **Discoverable sign-in** offers every passkey the device holds for the site; the one chosen names whose it is.
* **Second factor** is a passkey asked for after another method, limited to that identity's own passkeys.
* **Sign count** is the authenticator's counter; one that does not move is a cloned authenticator.
* **Attestation** is the authenticator vouching for its make; it is not asked for and not checked.
* **`PasskeyStore`** is where credentials live: memory, a JSON file or a store of your own.

## Quick Start

On the stock host, a passkey beside a password, which signs a new visitor up and lets one who lost a device back in:

```toml
[document]
origin = "https://shop.example"

[auth]
login = "/login"

[[auth.methods]]
name = "password"
provider = "file"

[[auth.methods]]
name = "passkey"
provider = "passkey"
store = "data/passkeys"
```

The login page runs the ceremony with the client:

```ts
import { signIn } from "@snapfire/fsr/client";

passkeyButton.onclick = () => signIn({ method: "passkey", returnTo: "/account" });
```

## Offering Passkeys on the Stock Host

A passkey method keeps its passkeys in `passkeys.json` under `store`, a directory relative to the project. The host refuses a method without one and a `store` inside `config/`, since passkeys are data the host writes and `fsr dev` restarts on every change there. It binds them to `document.origin`'s host and names the site in the prompt after `document.title` unless `rp_name` says otherwise. `verification = "required"` insists on a PIN or biometric:

```toml
[[auth.methods]]
name = "passkey"
provider = "passkey"
store = "data/passkeys"
rp_name = "Shop"
verification = "required"
```

A passkey is never the only method. Signing up needs a method that proves who the visitor is and a lost device needs a way back, so the host refuses a configuration where every method is a passkey.

In development without `document.origin`, the relying party is `localhost` on the listening port, so browse to `http://localhost:<port>` rather than `127.0.0.1`.

### Requiring a Passkey After a Password

Name the passkey method as the `second_factor`. A visitor who signs in with a password and holds no passkey yet is asked to create one there:

```toml
[auth]
login = "/login"
second_factor = "passkey"

[[auth.methods]]
name = "password"
provider = "file"

[[auth.methods]]
name = "passkey"
provider = "passkey"
store = "data/passkeys"
```

### Adding a Passkey From a Settings Page

An enrolment is a state change, so it carries the page's CSRF token:

```ts
import { enrol } from "@snapfire/fsr/client";

await enrol(csrfToken, { method: "passkey", returnTo: "/settings" });
```

## Building the Provider in Rust

A passkey goes beside a method that proves who a new visitor is, through `Methods` from `snapfire_fsr_auth`:

```rust
use std::sync::Arc;
use snapfire_fsr_auth::Methods;
use snapfire_fsr_passkey::{FilePasskeys, PasskeyConfig, Passkeys};

let config = PasskeyConfig::new("shop.example", "Shop", "https://shop.example").require_verification(true);
let passkeys = Passkeys::new(config, Arc::new(FilePasskeys::new("data/passkeys/passkeys.json")));
let methods = Methods::new().method("password", Arc::new(accounts)).method("passkey", Arc::new(passkeys));
let host = Host::from(".")?.identity(Arc::new(methods)).build()?;
```

## Keeping Passkeys Elsewhere

`FilePasskeys` suits one host. Several hosts share a store of your own, three methods over anything shared:

```rust
use futures_util::future::BoxFuture;
use snapfire_fsr_passkey::{Credential, PasskeyStore};

struct Shared { pool: deadpool_redis::Pool }

impl PasskeyStore for Shared {
  fn find(&self, id: &str) -> BoxFuture<'_, Result<Option<Credential>, String>> { /* ... */ }
  fn of_subject(&self, subject: &str) -> BoxFuture<'_, Result<Vec<Credential>, String>> { /* ... */ }
  fn save(&self, credential: Credential) -> BoxFuture<'_, Result<(), String>> { /* ... */ }
}
```

`Credential` serialises with serde, so a store keeps it as JSON.

## Error Handling

The provider answers `snapfire_fsr_auth::AuthError`. `Denied` is a ceremony that did not check out, which the visitor sees as a refusal; `Invalid` is a malformed answer or a store that failed.

```rust
match passkeys.advance(params, state).await {
  Ok(next) => next,
  Err(AuthError::Denied(why)) => return refuse(why),
  Err(other) => return fail(other.to_string()),
}
```
