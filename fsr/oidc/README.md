# snapfire_fsr_oidc

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_oidc.svg)](https://crates.io/crates/snapfire_fsr_oidc)
[![Docs.rs](https://docs.rs/snapfire_fsr_oidc/badge.svg)](https://docs.rs/snapfire_fsr_oidc)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

Lets users of an FSR application sign in with Google, Microsoft, Apple or any other OpenID Connect provider.

`OidcProvider` sends the visitor to the provider, checks the ID token that comes back and refreshes the access token when it runs out. It uses the authorization code flow with PKCE and checks token signatures against the provider's published keys. It plugs into [`snapfire_fsr_auth`](../auth/README.md) as a sign-in method.

Google, Microsoft and Apple each have a preset. Apple's preset signs the client secret for you and accepts the posted callback Apple sends.

Most applications never depend on this crate. With `fsr serve`, set `provider = "oidc"` in `[auth]`, with a `preset` or an `issuer`. Use the crate directly when you assemble your own host or run sign-in inside your own auth service.

* The task guide is [README.USAGE.md](README.USAGE.md).
* The call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_oidc = "0"
snapfire_fsr_auth = "0"
```

The crate has no features.

## Quick start

```rust
use snapfire_fsr_oidc::{ClientSecret, OidcConfig, OidcProvider};

let google = OidcProvider::new(
  OidcConfig::google("1234.apps.googleusercontent.com", "https://shop.example/auth/callback")
    .secret(ClientSecret::Post(std::env::var("GOOGLE_CLIENT_SECRET")?)),
);
```

## What to reach for

| Problem | Piece |
| --- | --- |
| Sign users in against an issuer's discovery document | `OidcProvider::new(OidcConfig::new(issuer, client_id, redirect_uri))` |
| Sign in with Google, Microsoft or Apple | `OidcConfig::google`, `OidcConfig::microsoft`, `OidcConfig::apple` |
| Send a client secret | `ClientSecret::Post` |
| Read an issuer's claim under another name | `OidcConfig::rename` |
| Get a new access token from a refresh token | `IdentityProvider::refresh` |
| Check a signed token against a key set yourself | `verify` |
