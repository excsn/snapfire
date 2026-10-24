# snapfire_fsr_passkey

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_passkey.svg)](https://crates.io/crates/snapfire_fsr_passkey)
[![Docs.rs](https://docs.rs/snapfire_fsr_passkey/badge.svg)](https://docs.rs/snapfire_fsr_passkey)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Passkey sign-in for SnapFire FSR: `Passkeys` is an `IdentityProvider` from `snapfire_fsr_auth` running WebAuthn sign-in and enrolment, with the relying party's checks done over `ring` and the credentials kept in a `PasskeyStore`. A sign-in with no identity is discoverable; as a second factor it asks for the identity's own passkeys. The browser half is `signIn` and `enrol` in `@snapfire/fsr-client`, which run the browser's prompt. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_passkey = "0"
snapfire_fsr_auth = "0"
```

The crate has no features.

## What to reach for

| Problem | Piece |
| --- | --- |
| Sign users in with a passkey | `Passkeys::new(PasskeyConfig::new(rp_id, rp_name, origin), store)` |
| Require a passkey after a password | `snapfire_fsr_auth::Sequence` with `Passkeys` second |
| Keep passkeys in memory or in a file | `MemoryPasskeys`, `FilePasskeys` |
| Keep passkeys somewhere else | `PasskeyStore` |
| Check a ceremony's parts yourself | `webauthn::client_data`, `webauthn::auth_data` |
