# API Reference: snapfire_fsr_oidc

OpenID Connect sign-in as a `snapfire_fsr_auth::IdentityProvider`: the authorization code flow with PKCE, ID token verification and refresh.

## Contents

* [1. The Provider](#1-the-provider)
  * [OidcProvider](#oidcprovider)
* [2. Configuration](#2-configuration)
  * [OidcConfig](#oidcconfig)
  * [ClientSecret](#clientsecret)
  * [Constants](#constants)
* [3. Tokens](#3-tokens)
  * [verify](#verify)
  * [Jwk](#jwk)
  * [JwkSet](#jwkset)
  * [sign_es256](#sign_es256)
* [4. Error Handling](#4-error-handling)
  * [Refused](#refused)
  * [AuthError](#autherror)

## 1. The Provider

### OidcProvider

One issuer as an `IdentityProvider`.

* `pub struct OidcProvider { /* private */ }`. Implements `snapfire_fsr_auth::IdentityProvider`.
* `pub fn new(config: OidcConfig) -> Self`
* `pub fn config(&self) -> &OidcConfig`
* The discovery document is fetched on first use and again once it is older than `DISCOVERY_TTL`.
* The key set is fetched on first use. It is fetched again when an ID token names a key the set lacks and the last fetch is older than `OidcConfig::key_refetch`.

`fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>>`

* Returns `Next::Redirect` to the discovery document's `authorization_endpoint`.
* The query holds `response_type=code`, `client_id`, `redirect_uri`, `scope` space separated, `state`, `nonce`, `code_challenge`, `code_challenge_method=S256` and `response_mode` when configured.
* `state`, `nonce` and the PKCE verifier are each 32 random bytes in base64url. `code_challenge` is the base64url SHA-256 of the verifier.
* The step's state holds `state`, `nonce` and `verifier`.
* `Intent::Enrol` is `AuthError::Invalid`.

`fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>>`

* A callback carrying `error` is `AuthError::Denied` with `error_description`, else `error`.
* A missing `state`, a `state` other than the step's (compared in constant time) or a missing `code` is `Invalid`.
* Posts `grant_type=authorization_code`, `code`, `redirect_uri`, `code_verifier`, `client_id` and the client secret to the `token_endpoint`.
* A token answer of `invalid_grant` or `access_denied` is `Denied`. Any other failure is `Invalid`, as is an answer with no `id_token`.
* The ID token must pass `verify` and these checks:
  * `iss` equals the discovery document's `issuer`, with `{tenantid}` replaced by the token's `tid` when it has one.
  * `aud` holds `client_id`. With more than one audience, `azp` equals `client_id`.
  * `exp` is present and not more than `CLOCK_SKEW_SECS` past. Otherwise `Denied`.
  * Neither `nbf` nor `iat` is more than `CLOCK_SKEW_SECS` ahead.
  * `nonce` is the step's nonce.
  * `sub` is present.
* Completes with `Identity { subject: sub, claims }`. The claims are every claim but `sub` and `iss`, `aud`, `exp`, `iat`, `nbf`, `nonce`, `at_hash`, `c_hash`, `auth_time`, `azp`, `jti` and `sid`, plus each renamed claim.
* The tokens are `access_token` when issued, `refresh_token` when issued and `expires_at` in seconds since the epoch when `expires_in` was given.

`fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>>`

* With a `refresh_token` in `tokens`, posts `grant_type=refresh_token`, `refresh_token`, `client_id` and the client secret. Returns the `access_token`, `refresh_token` and `expires_at` that came back.
* `None` without a `refresh_token` or when the issuer answers `invalid_grant` or `access_denied`.

## 2. Configuration

### OidcConfig

One issuer as a client of it. `Debug`, `Clone`, `#[non_exhaustive]`.

* `pub issuer: String`: trailing `/` trimmed. The discovery document is `<issuer>/.well-known/openid-configuration`.
* `pub client_id: String`
* `pub client_secret: ClientSecret`: `ClientSecret::None` by default.
* `pub redirect_uri: String`
* `pub scopes: Vec<String>`: `["openid", "email", "profile"]` by default.
* `pub response_mode: Option<String>`: `None` by default.
* `pub rename: BTreeMap<String, String>`: each application claim name to the issuer's claim it is read from.
* `pub key_refetch: Duration`: `JWKS_REFETCH` by default.
* `pub fn new(issuer: impl Into<String>, client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self`
* `pub fn google(client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self`: issuer `https://accounts.google.com`.
* `pub fn microsoft(tenant: &str, client_id: impl Into<String>, redirect_uri: impl Into<String>) -> Self`: issuer `https://login.microsoftonline.com/<tenant>/v2.0`. `tenant` is a tenant id, a domain, `common`, `organizations` or `consumers`.
* `pub fn apple(client_id: impl Into<String>, redirect_uri: impl Into<String>, team_id: impl Into<String>, key_id: impl Into<String>, pkcs8: Vec<u8>) -> Self`: issuer `https://appleid.apple.com`, scopes `openid name email`, `response_mode` `form_post` and a `ClientSecret::SignedJwt` with audience `https://appleid.apple.com`.
* `pub fn secret(self, secret: ClientSecret) -> Self`
* `pub fn scopes(self, scopes: Vec<String>) -> Self`
* `pub fn response_mode(self, mode: Option<String>) -> Self`
* `pub fn rename(self, rename: BTreeMap<String, String>) -> Self`
* `pub fn key_refetch(self, every: Duration) -> Self`

### ClientSecret

How the client proves itself at the token endpoint. `Clone`, `#[non_exhaustive]`. `Debug` never prints the secret.

* `None`: a public client, PKCE alone.
* `Post(String)`: `client_secret` in the form body.
* `SignedJwt { team_id: String, key_id: String, pkcs8: Vec<u8>, audience: String }`: `client_secret` is a JWT minted per request with `sign_es256`, `kid` `key_id`. Its claims are `iss` `team_id`, `sub` `client_id`, `aud` `audience`, `iat` now and `exp` 300 seconds on.

### Constants

* `pub const CLOCK_SKEW_SECS: u64 = 60`
* `pub const DISCOVERY_TTL: Duration`: one hour.
* `pub const JWKS_REFETCH: Duration`: sixty seconds.

## 3. Tokens

### verify

* `pub fn verify(token: &str, keys: &[Jwk]) -> Result<Map<String, serde_json::Value>, Refused>`: the claims of a compact JWS whose signature over `<header>.<payload>` verifies under one candidate key. Checks no claim.
* Accepted `alg`: `RS256` (RSA PKCS#1 v1.5, 2048 to 8192 bits), `ES256` (P-256, the fixed 64-byte signature) and `EdDSA` (Ed25519). Any other, `none` and the HMAC family included, is `Refused::Algorithm`.
* A candidate key has the algorithm's `kty` (`RSA`, `EC` or `OKP`), a `use` of `sig` or none, an `alg` equal to the header's or none and, when the header names a `kid`, that `kid`.
* An `ES256` key must have `crv` `P-256` and an `EdDSA` key `crv` `Ed25519`.

### Jwk

One JSON Web Key. `Debug`, `Clone`, `Deserialize`.

* `pub struct Jwk { pub kid: Option<String>, pub kty: String, pub alg: Option<String>, pub usage: Option<String>, pub n: Option<String>, pub e: Option<String>, pub crv: Option<String>, pub x: Option<String>, pub y: Option<String> }`
* `usage` is read from `use`.

### JwkSet

A key set document. `Debug`, `Clone`, `Deserialize`.

* `pub struct JwkSet { pub keys: Vec<Jwk> }`

### sign_es256

* `pub fn sign_es256(pkcs8: &[u8], kid: &str, claims: &serde_json::Value) -> Result<String, String>`: a compact JWS of `claims` with header `{ "alg": "ES256", "kid", "typ": "JWT" }`, signed with a PKCS#8 P-256 key. `Err` names the key or the signing failure.

## 4. Error Handling

### Refused

Why `verify` refused a token. `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`.

* `Malformed(String)`: not three parts, a part that is not base64url, a header or claims that are not JSON objects or a key missing a component.
* `UnknownKey(String)`: no candidate key. Holds the header's `kid`, empty when it has none.
* `Signature`: no candidate key verified the signature.
* `Algorithm(String)`: an `alg` that is not accepted.

### AuthError

`OidcProvider` returns `snapfire_fsr_auth::AuthError`.

* `Denied`: an `error` callback, a token answer of `invalid_grant` or `access_denied` and an expired ID token.
* `Invalid`: everything else. That covers a discovery document or key set that does not fetch, a state mismatch, an ID token that fails `verify` or a check and an `Intent::Enrol` start.
