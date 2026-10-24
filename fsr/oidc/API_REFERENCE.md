# API Reference: snapfire_fsr_oidc

OpenID Connect sign-in as an `IdentityProvider`: the authorization code flow with PKCE, ID token verification and refresh.

## Contents

* [1. The Provider](#1-the-provider)
  * [OidcProvider](#oidcprovider)
* [2. Configuration](#2-configuration)
  * [OidcConfig](#oidcconfig)
  * [ClientSecret](#clientsecret)
  * [Constants](#constants)
* [3. Tokens](#3-tokens)
  * [verify](#verify)
  * [Jwk and JwkSet](#jwk-and-jwkset)
  * [sign_es256](#sign_es256)
* [4. Error Handling](#4-error-handling)
  * [Refused](#refused)
  * [AuthError](#autherror)

## 1. The Provider

### OidcProvider

An issuer as an `IdentityProvider`. Fetches the issuer's discovery document on first use and again after `DISCOVERY_TTL`; fetches its key set on first use and again when a token names a key the set lacks, at most once per `OidcConfig::key_refetch`.

* `pub fn new(config: OidcConfig) -> Self`
* `pub fn config(&self) -> &OidcConfig`
* `begin(start)`: `Next::Redirect` to the authorization endpoint with `response_type=code`, `client_id`, `redirect_uri`, `scope` (space separated), fresh 32-byte `state` and `nonce`, `code_challenge` (the base64url SHA-256 of a fresh 32-byte verifier), `code_challenge_method=S256` and `response_mode` when configured. The state, nonce and verifier are the step's state. An `Intent::Enrol` start is `AuthError::Invalid`.
* `advance(params, state)`: a callback carrying `error` is `AuthError::Denied` with `error_description`, else `error`. A `state` other than the step's (compared in constant time), a missing `state` or `code` is `Invalid`. Posts `grant_type=authorization_code`, `code`, `redirect_uri`, `code_verifier`, `client_id` and the client secret to the token endpoint. An `invalid_grant` or `access_denied` answer is `Denied`; any other failure is `Invalid`. The ID token must verify and carry the issuer (`{tenantid}` in the issuer replaced by the token's `tid`), an audience holding `client_id` (with `azp` equal to it when there are several), an `exp` not past by `CLOCK_SKEW_SECS` (`Denied` otherwise), no `nbf` or `iat` beyond the skew ahead and the step's `nonce`. Completes with `Identity { subject: sub, claims }`, the claims being every claim but `sub` and the protocol's own (`iss`, `aud`, `exp`, `iat`, `nbf`, `nonce`, `at_hash`, `c_hash`, `auth_time`, `azp`, `jti`, `sid`), plus each renamed claim. The tokens are `access_token`, `refresh_token` when issued and `expires_at` (seconds since the epoch) when `expires_in` was given.
* `refresh(tokens)`: with a `refresh_token` in `tokens`, posts `grant_type=refresh_token` and answers the new `access_token`, `refresh_token` and `expires_at` that came back. `None` without a refresh token or when the issuer answers `invalid_grant`.

## 2. Configuration

### OidcConfig

One issuer as a client of it. `Debug`, `Clone`; `#[non_exhaustive]`, built with the constructors and setters.

* `pub issuer: String`, trailing `/` trimmed; the discovery document is `<issuer>/.well-known/openid-configuration`
* `pub client_id: String`
* `pub client_secret: ClientSecret`, `ClientSecret::None` by default
* `pub redirect_uri: String`
* `pub scopes: Vec<String>`, `["openid", "email", "profile"]` by default
* `pub response_mode: Option<String>`
* `pub rename: BTreeMap<String, String>`: application claim name to the issuer's
* `pub key_refetch: Duration`, `JWKS_REFETCH` by default
* `pub fn new(issuer, client_id, redirect_uri) -> Self`
* `pub fn google(client_id, redirect_uri) -> Self`: issuer `https://accounts.google.com`
* `pub fn microsoft(tenant: &str, client_id, redirect_uri) -> Self`: issuer `https://login.microsoftonline.com/<tenant>/v2.0`
* `pub fn apple(client_id, redirect_uri, team_id, key_id, pkcs8: Vec<u8>) -> Self`: issuer `https://appleid.apple.com`, scopes `openid name email`, `response_mode` `form_post` and a `ClientSecret::SignedJwt` with audience `https://appleid.apple.com`
* `pub fn secret(self, secret: ClientSecret) -> Self`, `pub fn scopes(self, scopes: Vec<String>) -> Self`, `pub fn response_mode(self, mode: Option<String>) -> Self`, `pub fn rename(self, rename: BTreeMap<String, String>) -> Self`, `pub fn key_refetch(self, every: Duration) -> Self`

### ClientSecret

How the client proves itself at the token endpoint. `Clone`; `Debug` never prints the secret. `#[non_exhaustive]`.

* `None`: a public client, PKCE alone.
* `Post(String)`: `client_secret` in the form body.
* `SignedJwt { team_id: String, key_id: String, pkcs8: Vec<u8>, audience: String }`: a JWT minted per request, ES256 with `kid` `key_id`, claims `iss` `team_id`, `sub` `client_id`, `aud` `audience`, `iat` now and `exp` five minutes on.

### Constants

* `pub const CLOCK_SKEW_SECS: u64 = 60`
* `pub const DISCOVERY_TTL: Duration`, one hour
* `pub const JWKS_REFETCH: Duration`, sixty seconds

## 3. Tokens

### verify

* `pub fn verify(token: &str, keys: &[Jwk]) -> Result<Map<String, serde_json::Value>, Refused>`

Verifies a compact JWS of three base64url parts. Accepts `RS256` (RSA PKCS#1 v1.5, 2048 to 8192 bits), `ES256` (P-256, the fixed 64-byte signature) and `EdDSA` (Ed25519); any other `alg`, `none` and the HMAC family included, is `Refused::Algorithm`. Candidate keys have the algorithm's `kty`, a `use` of `sig` or none, an `alg` equal to the header's or none and, when the header names a `kid`, that `kid`; none is `Refused::UnknownKey`. Answers the claims object once one candidate verifies the signature over `<header>.<payload>`. Checks no claim.

### Jwk and JwkSet

* `pub struct Jwk { pub kid: Option<String>, pub kty: String, pub alg: Option<String>, pub usage: Option<String>, pub n: Option<String>, pub e: Option<String>, pub crv: Option<String>, pub x: Option<String>, pub y: Option<String> }`: deserialises a JWK, `usage` from `use`.
* `pub struct JwkSet { pub keys: Vec<Jwk> }`

### sign_es256

* `pub fn sign_es256(pkcs8: &[u8], kid: &str, claims: &serde_json::Value) -> Result<String, String>`: a compact JWS of `claims`, header `{ "alg": "ES256", "kid", "typ": "JWT" }`, signed with a PKCS#8 P-256 key. `Err` naming the key or the signing failure.

## 4. Error Handling

### Refused

Why `verify` refused a token. `Debug`, `Clone`, `PartialEq`, `Eq`, `Display`.

* `Malformed(String)`: not three parts, a part that is not base64url, a header or claims that are not JSON objects, a key missing a component
* `UnknownKey(String)`: no candidate key, with the header's `kid`
* `Signature`: no candidate verified it
* `Algorithm(String)`: an algorithm not accepted

### AuthError

The provider answers `snapfire_fsr_auth::AuthError`: `Denied` for the visitor's or the issuer's refusal (an `error` callback, `invalid_grant` or `access_denied`, an expired ID token), `Invalid` for everything else (a discovery or key set that does not fetch, a state mismatch, an ID token that does not verify or check out, an enrolment).
