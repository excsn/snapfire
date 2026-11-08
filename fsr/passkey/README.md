# snapfire_fsr_passkey

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_passkey.svg)](https://crates.io/crates/snapfire_fsr_passkey)
[![Docs.rs](https://docs.rs/snapfire_fsr_passkey/badge.svg)](https://docs.rs/snapfire_fsr_passkey)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Lets users of an FSR application sign in with a passkey instead of a password.

`Passkeys` runs the WebAuthn sign-in and enrolment checks on the server and keeps each user's passkeys in a store you choose. It plugs into [`snapfire_fsr_auth`](../auth/README.md) as a sign-in method. In the browser, `signIn` and `enrol` from `@snapfire/fsr/client` show the browser's passkey prompt.

A passkey can be used in two ways:

* As the sign-in itself. The browser offers every passkey it holds for the site and the one the user picks says who they are.
* As a second factor after another method, such as a password. The browser then offers only that user's passkeys.

Most applications never depend on this crate. With `fsr serve`, add a passkey entry to `[[auth.methods]]` with `provider = "passkey"` and a `store` directory. A passkey always needs a second method beside it, so that a new visitor can make an account and a user who lost their device can get back in. Use the crate directly when you assemble your own host.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_passkey = "0"
snapfire_fsr_auth = "0"
```

The crate has no features.

## Quick start

```rust
use std::sync::Arc;
use snapfire_fsr_passkey::{FilePasskeys, PasskeyConfig, Passkeys};

let passkeys = Passkeys::new(
  PasskeyConfig::new("shop.example", "Shop", "https://shop.example"),
  Arc::new(FilePasskeys::new("data/passkeys/passkeys.json")),
);
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Sign users in with a passkey | `Passkeys::new(PasskeyConfig::new(rp_id, rp_name, origin), store)` |
| Require a passkey after a password | `snapfire_fsr_auth::Sequence` with `Passkeys` second |
| Keep passkeys in memory or in a file | `MemoryPasskeys`, `FilePasskeys` |
| Keep passkeys somewhere else | `PasskeyStore` |
| Check a ceremony's parts yourself | `webauthn::client_data`, `webauthn::auth_data` |
