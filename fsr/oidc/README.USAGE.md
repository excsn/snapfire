# Usage Guide: snapfire_fsr_oidc

How to sign users in against an OpenID Connect issuer, from the stock host's configuration or a provider built in Rust.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Configuring a Method on the Stock Host](#configuring-a-method-on-the-stock-host)
  * [Using a Preset](#using-a-preset)
  * [Signing In Before the Secret Exists](#signing-in-before-the-secret-exists)
  * [Offering It Beside Other Methods](#offering-it-beside-other-methods)
* [Building a Provider in Rust](#building-a-provider-in-rust)
* [Renaming Claims](#renaming-claims)
* [Refreshing Tokens](#refreshing-tokens)
* [Verifying a Token Yourself](#verifying-a-token-yourself)
* [Error Handling](#error-handling)

## Core Concepts

* **Issuer** is the identity provider's base URL; its discovery document names the endpoints and the key set.
* **Authorization code flow** sends the browser to the issuer, which comes back to the callback with a `code` the server exchanges for tokens.
* **PKCE** binds that code to this sign-in: the server keeps a verifier and sends only its SHA-256.
* **`state`** ties the callback to the sign-in that began it; **`nonce`** ties the ID token to it.
* **ID token** is a JWS the issuer signs, naming who signed in as `sub` with their claims.
* **Key set (JWKS)** is the issuer's published public keys, fetched again when a token names a key the cached set lacks.
* **Redirect URI** is `<document.origin>/auth/callback`, which the issuer must have registered.
* **Client secret** proves the application at the token endpoint; Apple's is a JWT signed per request.
* **Stand-in** is what an `oidc` method is in development while its secret has no value: a "sign in as" form over the `users` table.
* **Custody** holds `access_token`, `refresh_token` and `expires_at`; the application never sees them.

## Quick Start

On the stock host, one method:

```toml
[document]
origin = "https://shop.example"

[auth]
provider = "oidc"
preset = "google"
client_id = "1234.apps.googleusercontent.com"
client_secret = "google"

[secrets.google]
description = "the Google OAuth client secret"
```

```sh
C5_SECRETS__GOOGLE__VALUE=... fsr serve app
```

A link to `/auth/login?return_to=/account` signs the visitor in with Google and lands on `/account`.

## Configuring a Method on the Stock Host

`provider = "oidc"` with an `issuer` and a `client_id` is the whole configuration for any compliant issuer. `client_secret` names a `[secrets.<name>]` entry, whose value comes from `C5_SECRETS__<NAME>__VALUE` or an encrypted file:

```toml
[auth]
provider = "oidc"
issuer = "https://id.example.com/realms/staff"
client_id = "portal"
client_secret = "staff"
scopes = ["openid", "email", "profile", "groups"]
```

Register `<document.origin>/auth/callback` with the issuer. Outside development a missing `document.origin` or a secret with no value stops the boot with the variable to set.

### Using a Preset

`preset` names the issuer and its quirks. Okta, Auth0 and Keycloak need only their `issuer`.

```toml
[auth]
provider = "oidc"
preset = "microsoft"
tenant = "contoso.onmicrosoft.com"
client_id = "..."
client_secret = "entra"
```

Apple's client secret is a JWT signed with the key Apple issued, so it names the team, the key and the secret holding the key's PEM:

```toml
[auth]
provider = "oidc"
preset = "apple"
client_id = "com.example.web"
team_id = "ABCDE12345"
key_id = "KEY123"
signing_key = "apple_key"
```

### Signing In Before the Secret Exists

In development, an `oidc` method whose secret has no value is a stand-in: `/auth/login` presents a form naming the users of `config/auth.toml` and signs in as the one chosen, with that user's claims. The boot report says so:

```text
auth      oidc google, a stand-in until secrets.google has a value, login page /login, ...
```

Set the secret and restart to sign in against the issuer itself.

### Offering It Beside Other Methods

Several methods are `[[auth.methods]]`; `/auth/login?method=corp` runs that one and `/auth/login` alone presents the choice:

```toml
[auth]
login = "/login"

[[auth.methods]]
name = "password"
provider = "file"

[[auth.methods]]
name = "corp"
provider = "oidc"
issuer = "https://id.example.com"
client_id = "portal"
client_secret = "corp"
```

## Building a Provider in Rust

A Rust host or an auth service builds the provider and hands it to the host or serves it behind its own contract:

```rust
use std::sync::Arc;
use snapfire_fsr_oidc::{ClientSecret, OidcConfig, OidcProvider};

let config = OidcConfig::google(client_id, "https://shop.example/auth/callback").secret(ClientSecret::Post(secret));
let host = Host::from(".")?.identity(Arc::new(OidcProvider::new(config))).build()?;
```

## Renaming Claims

Every claim of the ID token but the protocol's own reaches `Identity::claims` under its own name. `rename` adds an application name for an issuer's claim:

```toml
[auth]
provider = "oidc"
issuer = "https://id.example.com"
client_id = "portal"
claims = { role = "groups" }
```

```rust
let config = config.rename([("role".to_owned(), "groups".to_owned())].into_iter().collect());
```

## Refreshing Tokens

With `offline_access` or the issuer's equivalent among the scopes, the token endpoint issues a refresh token. The host's bearer interceptor renews `access_token` through `refresh` when `expires_at` has passed or a client answers unauthorized; a refresh token the issuer no longer honours renews nothing and the call answers as it was.

```rust
let renewed = provider.refresh(tokens).await?;
```

## Verifying a Token Yourself

`verify` checks a JWS against a key set and answers its claims, checking no claim itself:

```rust
use snapfire_fsr_oidc::{verify, JwkSet};

let set: JwkSet = serde_json::from_str(&jwks_json)?;
let claims = verify(&token, &set.keys)?;
```

## Error Handling

The provider answers `snapfire_fsr_auth::AuthError`. `Denied` is a refusal the visitor should see, back on the login page with `error=denied`; `Invalid` is everything a developer should read.

```rust
match provider.advance(params, state).await {
  Ok(next) => next,
  Err(AuthError::Denied(why)) => return refuse(why),
  Err(AuthError::Invalid(why)) => return fail(why),
  Err(other) => return fail(other.to_string()),
}
```

`verify` answers `Refused`: `UnknownKey` is the one worth matching, since fetching the key set again may resolve it.
