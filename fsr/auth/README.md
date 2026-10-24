# snapfire_fsr_auth

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_auth.svg)](https://crates.io/crates/snapfire_fsr_auth)
[![Docs.rs](https://docs.rs/snapfire_fsr_auth/badge.svg)](https://docs.rs/snapfire_fsr_auth)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Auth for SnapFire FSR: the front door of the session layer, covering how an anonymous session becomes an identified one and where backend tokens live. `IdentityProvider` is the seam, a sign-in as a sequence of steps that redirect, present data to the page or complete; `Auth` is the flow over it (`begin`, `advance`, `logout`) and `DevProvider` is a name-and-password implementation for development. The crate never renders and owns no HTTP endpoints; the login page is an ordinary route through the ordinary plan, identity reaches templates only as the injected `identity` prop and the flow endpoints live at whatever HTTP adapter the application brings. Task-shaped instructions are in [README.USAGE.md](README.USAGE.md); signatures are in [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_auth = "0"
snapfire_fsr_core = "0"
snapfire_fsr_runtime = "0"
snapfire_fsr_session = "0"
```

`snapfire_fsr_session` is not optional: `Auth` takes an `Opened` session on every call.

## What to reach for

| What you are trying to do | What to reach for |
| --- | --- |
| Start a sign-in and learn what the browser does first | `Auth::begin` with `Start::sign_in` |
| Take the browser's answer and learn the next step | `Auth::advance` |
| Add a passkey or a linked account to whoever is signed in | `Auth::begin` with `Start::enrol` |
| Forget identity plus backend tokens on the way out | `Auth::logout` |
| Plug in a provider of your own | `IdentityProvider` |
| Send the browser elsewhere, hand the page data or finish | `Next` |
| Offer several methods on one login page | `Methods` |
| Require a second factor after any other method | `Sequence` |
| Sign in against a fixed user table while developing | `DevProvider` |
| Hand back an identity plus the tokens the backend tier will need | `AuthOutcome` |
| Turn a failed step into an HTTP response | `AuthError::http_status` |
