# API Reference: snapfire_fsr_auth

Auth for SnapFire FSR: the `IdentityProvider` seam, the login flow over it and the dev provider.

## Contents

* [1. The Flow](#1-the-flow)
  * [Auth](#auth)
* [2. The Provider Seam](#2-the-provider-seam)
  * [IdentityProvider](#identityprovider)
  * [Begin](#begin)
  * [AuthOutcome](#authoutcome)
* [3. The Dev Provider](#3-the-dev-provider)
  * [DevProvider](#devprovider)
* [4. Types From Other Crates](#4-types-from-other-crates)
  * [Identity](#identity)
  * [Opened](#opened)
  * [TokenCell](#tokencell)
  * [Value and ValueMap](#value-and-valuemap)
* [5. Error Handling](#5-error-handling)
  * [AuthError](#autherror)

## 1. The Flow

### Auth

The login flow over any `IdentityProvider`. Holds the provider and no other state; every call takes the request's `Opened` session.

* `pub fn new(provider: Arc<dyn IdentityProvider>) -> Self`
* `pub async fn login(&self, opened: &Opened, return_to: &str) -> String`
* `pub fn pending_return_to(&self, opened: &Opened) -> Option<String>`
* `pub async fn callback(&self, opened: &Opened, params: ValueMap) -> Result<String, AuthError>`
* `pub fn logout(&self, opened: &Opened)`

`login` awaits `provider.begin(return_to)`, inserts `return_to` into the returned `state` as a `Value::Str`, writes the whole map to `opened.tokens` under the reserved key `_sf_auth` and returns `Begin::redirect` unchanged. It writes nothing to `opened.cell`, so the session stays anonymous. The insert overwrites any `return_to` a provider put in its own state. The write marks the token cell dirty, so `Sessions::persist` saves the record.

`pending_return_to` reads `return_to` out of the flow state without consuming it; `None` when no flow is in progress or the state holds no string there.

`callback` removes `_sf_auth` from `opened.tokens` before calling the provider, so the flow is consumed whether the attempt succeeds or fails. It returns `AuthError::Invalid("no login in progress for this session")` when that key is absent or is not a `Value::Map`. The destination comes from the state's `return_to`, defaulting to `/` when it is missing or not a `Value::Str`. On success it calls `opened.cell.set_identity(Some(outcome.identity))` then `opened.tokens.merge(outcome.tokens)`, so identity is readable by application code while tokens are not. The destination is then returned.

`logout` calls `opened.cell.clear()` plus `opened.tokens.clear()`, which drops session data, identity and every token in one pair of dirty writes. It does not delete the stored record and does not produce a cookie; `Sessions::destroy` does both; the caller invokes it separately.

`_sf_auth` is reserved in the token cell. Writing to that key from application code corrupts an in-flight login and is not detected.

## 2. The Provider Seam

### IdentityProvider

The seam an identity source implements. Requires `Send + Sync`; both methods return a `BoxFuture` borrowing `self`.

* `fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin>`
* `fn callback(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>>`

`begin` is infallible: there is no error channel, so a provider that cannot start a flow must redirect to something that can report it. `return_to` is the destination the application wants after login; a provider may encode it into the redirect or ignore it, since `Auth` carries it in the flow state regardless.

`callback` receives the provider's response as `params` plus exactly the map `begin` returned, with `return_to` added. Cross-request checks such as a state or nonce comparison belong here; `Auth` performs none.

`DevProvider` is the only implementation in this crate.

### Begin

What `begin` returns.

* `pub redirect: String`
* `pub state: ValueMap`

`redirect` is used verbatim as the location the browser is sent to; it is not validated, escaped or resolved against a base. `state` is held server-side in token custody for the duration of the round trip and never reaches the browser.

### AuthOutcome

What a successful `callback` returns.

* `pub identity: Identity`
* `pub tokens: ValueMap`

`identity` replaces whatever identity the session cell held. `tokens` is merged into token custody key by key, so a key already present is overwritten while a key absent from the map survives.

## 3. The Dev Provider

### DevProvider

Name and password against a fixed in-memory table. Passwords are compared in the clear, so this is for development only.

* `pub fn new(login_path: impl Into<String>) -> Self`
* `pub fn user(self, name: impl Into<String>, password: impl Into<String>) -> Self`
* `pub fn user_with_claims(self, name: impl Into<String>, password: impl Into<String>, claims: ValueMap) -> Self`
* `pub fn from_toml(login_path: impl Into<String>, path: impl AsRef<Path>) -> Result<Self, String>`

`new` starts with an empty table. `user` appends an entry with empty claims; `user_with_claims` appends one with the claims given. Both consume and return `Self` for chaining. Neither rejects a duplicate name: the first match in insertion order wins. `from_toml` reads a file of `[[users]]` rows, each `name`, `password` and an optional `claims` table whose values become `Value`s (a string, integer, float, boolean, array or table; a datetime becomes its string), and appends them in file order. An unreadable or unparsable file, no row at all or an empty name is `Err` naming the path and the reason.

`begin` returns `Begin { redirect: format!("{login_path}?return_to={encoded}"), state: ValueMap::new() }`, where `encoded` is `return_to` passed through `form_urlencoded::byte_serialize`. The redirect always carries a `return_to` query parameter, so a `login_path` that already has a query string produces a malformed URL.

`callback` ignores `state`. It reads `user` plus `password` from `params`, accepting `Value::Str` only. A missing or non-string `user` is `AuthError::Invalid("missing user")`; the same for `password` is `AuthError::Invalid("missing password")`. No table entry matching both is `AuthError::Denied("unknown user or wrong password")`. On success the outcome is `Identity { subject: <name>, claims: <the entry's claims> }` plus one token, `access_token` set to `dev-token-<name>`.

## 4. Types From Other Crates

These appear in this crate's signatures and are defined elsewhere in the family.

### Identity

`snapfire_fsr_runtime::Identity`. Who the request is. Derives `Debug`, `Clone` and `PartialEq`.

* `pub subject: String`
* `pub claims: ValueMap`

Reaches templates as the injected `identity` prop, a map of `subject` plus `claims`. Anything placed in `claims` is readable by application code, so a credential belongs in `AuthOutcome::tokens` instead.

### Opened

`snapfire_fsr_session::Opened`. One request's session as the session layer sees it.

* `pub id: SessionId`
* `pub cell: SessionCell`
* `pub tokens: TokenCell`
* `pub fresh: bool`

`Auth` touches `cell` and `tokens` only. `id` is what `Sessions::csrf_token` and `Sessions::verify_csrf` are keyed by.

### TokenCell

`snapfire_fsr_session::TokenCell`. Server-side custody for backend credentials plus auth flow state. Cloning shares the same state. It is never placed in `RequestCtx`, so loaders, actions and evaluators cannot reach it; `snapfire_fsr_service` implements its `Credentials` trait for this type, which is how an outbound call attaches a token.

* `pub fn get(&self, key: &str) -> Option<Value>`
* `pub fn set(&self, key: impl Into<String>, value: Value)`
* `pub fn remove(&self, key: &str) -> Option<Value>`
* `pub fn merge(&self, tokens: ValueMap)`
* `pub fn clear(&self)`
* `pub fn is_dirty(&self) -> bool`
* `pub fn snapshot(&self) -> ValueMap`

### Value and ValueMap

`snapfire_fsr_core::Value` is the value model; `snapfire_fsr_core::ValueMap` is `IndexMap<String, Value>`, so insertion order is preserved. Callback params, provider state, claims and tokens are all `ValueMap`. This crate reads only `Value::Str` plus `Value::Map` out of them; any other variant in a key it inspects is treated as absent.

## 5. Error Handling

### AuthError

The crate's only error type, returned by `Auth::callback` alone. Derives `Debug`, `Clone` and `PartialEq`; implements `std::error::Error`.

* `Denied(String)`
* `Invalid(String)`
* `pub fn http_status(&self) -> u16`

`Denied` means the provider identified the request as not permitted. `Invalid` means the request was malformed or there was no flow in progress to finish.

`http_status` returns 403 for `Denied` plus 400 for `Invalid`.

`Display` writes `denied: <message>` for the first variant, `invalid: <message>` for the second. Messages are diagnostic text, not user-facing copy, so an adapter that echoes them into a response body exposes whatever the provider wrote.
