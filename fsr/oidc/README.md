# snapfire_fsr_oidc

[![Crates.io](https://img.shields.io/crates/v/snapfire_fsr_oidc.svg)](https://crates.io/crates/snapfire_fsr_oidc)
[![Docs.rs](https://docs.rs/snapfire_fsr_oidc/badge.svg)](https://docs.rs/snapfire_fsr_oidc)
[![License: MPL-2.0](https://img.shields.io/badge/License-MPL%202.0-brightgreen.svg)](https://opensource.org/licenses/MPL-2.0)

OpenID Connect sign-in for SnapFire FSR: `OidcProvider` is an `IdentityProvider` from `snapfire_fsr_auth` over any issuer, running the authorization code flow with PKCE, verifying the ID token against the issuer's published keys over `ring` and refreshing tokens. Presets carry Google's, Microsoft's and Apple's quirks. The stock host builds one from `[auth] provider = "oidc"` with its `oidc` feature, which `fsr serve` carries; an auth service can run the same provider behind its own contract. The task guide is [README.USAGE.md](README.USAGE.md) and the call surface is [API_REFERENCE.md](API_REFERENCE.md).

## Install

```toml
[dependencies]
snapfire_fsr_oidc = "0"
snapfire_fsr_auth = "0"
```

The crate has no features.

## What to reach for

| Problem | Piece |
| --- | --- |
| Sign users in against an issuer's discovery document | `OidcProvider::new(OidcConfig::new(issuer, client_id, redirect_uri))` |
| Sign in with Google, Microsoft or Apple | `OidcConfig::google`, `OidcConfig::microsoft`, `OidcConfig::apple` |
| Send a client secret | `ClientSecret::Post` |
| Read an issuer's claim under another name | `OidcConfig::rename` |
| Renew an access token from custody | `IdentityProvider::refresh` |
| Check a JWS against a key set yourself | `verify` |
