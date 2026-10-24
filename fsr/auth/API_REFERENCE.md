# API Reference: snapfire_fsr_auth

Auth for SnapFire FSR: the `IdentityProvider` seam, the sign-in flow over it as a sequence of steps and the dev provider.

## Contents

* [1. The Flow](#1-the-flow)
  * [Auth](#auth)
  * [Step](#step)
* [2. The Provider Seam](#2-the-provider-seam)
  * [IdentityProvider](#identityprovider)
  * [Start](#start)
  * [Intent](#intent)
  * [Next](#next)
  * [AuthOutcome](#authoutcome)
* [3. The Dev Provider](#3-the-dev-provider)
  * [DevProvider](#devprovider)
  * [password_form](#password_form)
* [4. Types From Other Crates](#4-types-from-other-crates)
  * [Identity](#identity)
  * [Opened](#opened)
  * [TokenCell](#tokencell)
  * [Value and ValueMap](#value-and-valuemap)
* [5. Error Handling](#5-error-handling)
  * [AuthError](#autherror)

## 1. The Flow

### Auth

The sign-in flow over any `IdentityProvider`, as a sequence of steps. Holds the provider and no other state; every call takes the request's `Opened` session.

* `pub fn new(provider: Arc<dyn IdentityProvider>) -> Self`
* `pub fn provider(&self) -> &Arc<dyn IdentityProvider>`
* `pub async fn begin(&self, opened: &Opened, start: Start) -> Result<Step, AuthError>`
* `pub async fn advance(&self, opened: &Opened, params: ValueMap) -> Result<Step, AuthError>`
* `pub async fn ensure_flow(&self, opened: &Opened, return_to: &str)`
* `pub fn pending_return_to(&self, opened: &Opened) -> Option<String>`
* `pub fn logout(&self, opened: &Opened)`

`begin` drops any flow in progress and calls `provider.begin(start)`. An `Intent::Enrol` start on a session with no identity is `AuthError::Denied` before the provider is asked; otherwise its `identity` is replaced by the session's own, so a provider always enrols for whoever is signed in.

`advance` removes the flow from `opened.tokens` before calling `provider.advance(params, state)` with the state the last step kept, so a step is consumed whether its answer succeeds or fails and a refused answer cannot be replayed. No flow in progress is `AuthError::Invalid("no sign-in in progress for this session")`.

Both settle the provider's `Next` the same way. `Redirect` and `Present` keep the flow under the reserved token key `_sf_auth`: `return_to`, the intent and the step's `state`, never shown to the browser. `Complete` ends the flow. For a sign-in it calls `opened.cell.set_identity(Some(outcome.identity))`; for an enrolment it refuses an outcome whose subject is not the session's with `AuthError::Denied` and leaves the identity alone. Either way `outcome.tokens` merges into custody, so identity is readable by application code while tokens are not.

`ensure_flow` begins a sign-in when none is in progress, so a login page reached by a typed URL still has a flow to answer. `pending_return_to` reads the destination of the flow in progress without consuming it.

`logout` calls `opened.cell.clear()` plus `opened.tokens.clear()`. It does not delete the stored record and does not produce a cookie; `Sessions::destroy` does both.

`_sf_auth` is reserved in the token cell. Writing to that key from application code corrupts an in-flight sign-in and is not detected.

### Step

What the host answers the browser with after a step. Derives `Debug`, `Clone` and `PartialEq`.

* `Redirect(String)`: send the browser to this URL.
* `Present { kind: String, data: ValueMap }`: the page acts on `data` as `kind` says and posts its answer to the callback.
* `Complete { to: String, signed_in: bool }`: the flow is over; land on `to`. `signed_in` is `true` when the session's identity changed, which is when a host gives the session a new id.

## 2. The Provider Seam

### IdentityProvider

One way of proving who the visitor is, as a sequence of steps. Requires `Send + Sync`. Only value-model data crosses it, so a provider in another process is the same trait over a transport.

* `fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>>`
* `fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>>`
* `fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>>`, default `Ok(None)`

`advance` receives the browser's answer to the last step, the callback's query, form or JSON body, with exactly the `state` that step returned. Cross-request checks such as a state, nonce or challenge comparison belong here; `Auth` performs none. `refresh` answers new tokens for the ones custody holds, `None` when the provider cannot refresh them.

### Start

What a flow starts from. Derives `Debug` and `Clone`. `#[non_exhaustive]`: build one with `sign_in` or `enrol`.

* `pub intent: Intent`
* `pub return_to: String`: the path on this origin the flow lands on.
* `pub method: Option<String>`: the method the visitor chose, for a provider offering several.
* `pub identity: Option<Identity>`: who is signed in, always present for an enrolment.
* `pub fn sign_in(return_to: impl Into<String>) -> Self`
* `pub fn enrol(return_to: impl Into<String>, identity: Identity) -> Self`
* `pub fn method(self, method: Option<String>) -> Self`

### Intent

* `SignIn`
* `Enrol`: add a way of signing in, such as a passkey or a linked account, to the identity already signed in.

Derives `Debug`, `Clone`, `Copy`, `PartialEq` and `Eq`. `#[non_exhaustive]`.

### Next

What the browser does next. A closed set: a new answer would be a change to the seam.

* `Redirect { to: String, state: ValueMap }`: send the browser to `to`, an identity provider's page, which comes back to the callback.
* `Present { kind: String, data: ValueMap, state: ValueMap }`: hand `data` to the page, which posts its answer to the callback. `kind` names what the page does: `form` for fields to fill, `webauthn.get` and `webauthn.create` for a passkey ceremony's options, `notice` for a message such as a code sent elsewhere.
* `Complete(AuthOutcome)`
* `pub fn redirect(to: impl Into<String>, state: ValueMap) -> Self`
* `pub fn present(kind: impl Into<String>, data: ValueMap, state: ValueMap) -> Self`

`state` never reaches the browser: `Auth` keeps it in token custody and hands it to the next `advance`.

### AuthOutcome

What a completed flow produces. `#[non_exhaustive]`: build one with `new`.

* `pub identity: Identity`
* `pub tokens: ValueMap`
* `pub fn new(identity: Identity, tokens: ValueMap) -> Self`

`tokens` is merged into token custody key by key, so a key already present is overwritten while a key absent from the map survives.

## 3. The Dev Provider

### DevProvider

Name and password against a fixed in-memory table. Passwords are compared in the clear, so this is for development only.

* `pub fn new() -> Self`, also `Default`
* `pub fn user(self, name: impl Into<String>, password: impl Into<String>) -> Self`
* `pub fn user_with_claims(self, name: impl Into<String>, password: impl Into<String>, claims: ValueMap) -> Self`
* `pub fn from_toml(path: impl AsRef<Path>) -> Result<Self, String>`

`new` starts with an empty table. `user` appends an entry with empty claims; `user_with_claims` appends one with the claims given. Neither rejects a duplicate name: the first match in insertion order wins. `from_toml` reads a file of `[[users]]` rows, each `name`, `password` and an optional `claims` table whose values become `Value`s (a datetime becomes its string). An unreadable or unparsable file, no row at all or an empty name is `Err` naming the path and the reason.

`begin` answers `password_form()` for a sign-in and `AuthError::Invalid` for an enrolment. `advance` ignores `state` and reads `user` and `password` from `params`, accepting `Value::Str` only. A missing one is `AuthError::Invalid("missing user")` or `("missing password")`; no entry matching both is `AuthError::Denied("unknown user or wrong password")`. On success it completes with `Identity { subject: <name>, claims: <the entry's claims> }` and one token, `access_token` set to `dev-token-<name>`.

### password_form

* `pub fn password_form() -> Next`: `Next::Present` of kind `form`, `data` `{ fields: ["user", "password"] }` and empty state, the step a name-and-password provider presents.

## 4. Types From Other Crates

These appear in this crate's signatures and are defined elsewhere in the family.

### Identity

`snapfire_fsr_runtime::Identity`. Who the request is. Derives `Debug`, `Clone` and `PartialEq`. `#[non_exhaustive]`: build one with `new`.

* `pub subject: String`
* `pub claims: ValueMap`
* `pub fn new(subject: String, claims: ValueMap) -> Self`

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

The crate's only error type, returned by `Auth::begin`, `Auth::advance` and every provider method. Derives `Debug`, `Clone` and `PartialEq`; implements `std::error::Error`.

* `Denied(String)`
* `Invalid(String)`
* `pub fn http_status(&self) -> u16`

`Denied` means the provider identified the request as not permitted. `Invalid` means the request was malformed or there was no flow in progress to finish.

`http_status` returns 403 for `Denied` plus 400 for `Invalid`.

`Display` writes `denied: <message>` for the first variant, `invalid: <message>` for the second. Messages are diagnostic text, not user-facing copy, so an adapter that echoes them into a response body exposes whatever the provider wrote.
