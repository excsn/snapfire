# API Reference: snapfire_fsr_auth

The `IdentityProvider` trait, the sign-in flow over it, provider composition and a development provider for SnapFire FSR.

## Contents

* [1. The Flow](#1-the-flow)
  * [`Auth`](#auth)
  * [`Step`](#step)
* [2. Providers](#2-providers)
  * [`IdentityProvider`](#identityprovider)
  * [`Start`](#start)
  * [`Intent`](#intent)
  * [`Next`](#next)
  * [`AuthOutcome`](#authoutcome)
* [3. Composing Methods](#3-composing-methods)
  * [`Methods`](#methods)
  * [`Sequence`](#sequence)
* [4. The Dev Provider](#4-the-dev-provider)
  * [`DevProvider`](#devprovider)
  * [`password_form`](#password_form)
* [5. Types From Other Crates](#5-types-from-other-crates)
  * [`Identity`](#identity)
  * [`Opened` and `TokenCell`](#opened-and-tokencell)
  * [`Value` and `ValueMap`](#value-and-valuemap)
* [6. Error Handling](#6-error-handling)
  * [`AuthError`](#autherror)

## 1. The Flow

### `Auth`

The sign-in flow over one `IdentityProvider`. Holds the provider and no other state.

* `pub fn new(provider: Arc<dyn IdentityProvider>) -> Self`
* `pub fn provider(&self) -> &Arc<dyn IdentityProvider>`
* `pub async fn begin(&self, opened: &Opened, start: Start) -> Result<Step, AuthError>`: drops any flow in progress, then calls `provider.begin(start)`.
  * An `Intent::Enrol` start on a session with no identity is `AuthError::Denied` and the provider is not called.
  * For `Intent::Enrol`, `start.identity` is replaced by the session's identity.
* `pub async fn advance(&self, opened: &Opened, params: ValueMap) -> Result<Step, AuthError>`: calls `provider.advance(params, state)` with the `state` the last step kept.
  * The flow is removed from `opened.tokens` before the provider is called, so the step is consumed whether the answer succeeds or fails.
  * No flow in progress is `AuthError::Invalid("no sign-in in progress for this session")`.
* `pub async fn ensure_flow(&self, opened: &Opened, return_to: &str)`: `begin(opened, Start::sign_in(return_to))` when no flow is in progress. Errors are discarded.
* `pub fn pending_return_to(&self, opened: &Opened) -> Option<String>`: the `return_to` of the flow in progress. Does not consume the flow.
* `pub fn logout(&self, opened: &Opened)`: `opened.cell.clear()` and `opened.tokens.clear()`. Does not delete the stored record and produces no cookie.

How `begin` and `advance` settle the provider's `Next`:

* `Redirect` and `Present` store the flow under the token key `_sf_auth`: `return_to`, the intent and the step's `state`.
* `Complete` on a sign-in calls `opened.cell.set_identity(Some(outcome.identity))`.
* `Complete` on an enrolment whose subject is not the session's is `AuthError::Denied`. The identity is left unchanged.
* `Complete` merges `outcome.tokens` into `opened.tokens` in both cases.
* `_sf_auth` is reserved in the token cell. Writing that key from application code corrupts a flow in progress and is not detected.

### `Step`

What the host answers the browser with after a step. `Debug`, `Clone`, `PartialEq`.

* `Redirect(String)`: send the browser to this URL.
* `Present { kind: String, data: ValueMap }`: the page acts on `data` as `kind` says and posts its answer to the callback.
* `Complete { to: String, signed_in: bool }`: the flow is over and the browser lands on `to`. `signed_in` is `true` when the session's identity changed.

## 2. Providers

### `IdentityProvider`

One way of proving who the visitor is, as a sequence of steps. `Send + Sync`.

* `fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>>`
* `fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>>`: `params` is the callback's query, form or JSON body. `state` is exactly what the last step returned.
* `fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>>`: new tokens for the ones custody holds, `None` when the provider cannot refresh them. Default `Ok(None)`.
* `Auth` performs no state, nonce or challenge comparison between steps.

### `Start`

What a flow starts from. `Debug`, `Clone`. `#[non_exhaustive]`: build one with `sign_in` or `enrol`.

* `pub intent: Intent`
* `pub return_to: String`: the path on this origin the flow lands on.
* `pub method: Option<String>`: the method the visitor chose.
* `pub identity: Option<Identity>`: who is signed in. Always `Some` for an enrolment that `Auth` passes on.
* `pub fn sign_in(return_to: impl Into<String>) -> Self`
* `pub fn enrol(return_to: impl Into<String>, identity: Identity) -> Self`
* `pub fn method(self, method: Option<String>) -> Self`
* `pub fn identity(self, identity: Option<Identity>) -> Self`: who a sign-in is for. `Sequence` sets it for the second provider.

### `Intent`

`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`. `#[non_exhaustive]`.

* `SignIn`
* `Enrol`: add a way of signing in, such as a passkey or a linked account, to the identity already signed in.

### `Next`

What the browser does next.

* `Redirect { to: String, state: ValueMap }`: send the browser to `to`, which comes back to the callback.
* `Present { kind: String, data: ValueMap, state: ValueMap }`: hand `data` to the page, which posts its answer to the callback.
  * `kind` values in use: `form` for fields to fill, `webauthn.get` and `webauthn.create` for a passkey ceremony's options, `notice` for a message, `choose` from `Methods`.
* `Complete(AuthOutcome)`
* `pub fn redirect(to: impl Into<String>, state: ValueMap) -> Self`
* `pub fn present(kind: impl Into<String>, data: ValueMap, state: ValueMap) -> Self`
* `state` is never sent to the browser. `Auth` keeps it in token custody and passes it to the next `advance`.

### `AuthOutcome`

What a completed flow produces. `#[non_exhaustive]`: build one with `new`.

* `pub identity: Identity`
* `pub tokens: ValueMap`: merged into token custody key by key. A key already present is overwritten and a key absent from the map is kept.
* `pub fn new(identity: Identity, tokens: ValueMap) -> Self`

## 3. Composing Methods

Both types implement `IdentityProvider`.

### `Methods`

Several methods on one login page, by name, in the order added. `Default`.

* `pub fn new() -> Self`
* `pub fn method(self, name: impl Into<String>, provider: Arc<dyn IdentityProvider>) -> Self`
* `pub fn names(&self) -> Vec<&str>`
* `begin` runs the method `Start::method` names. With no `Start::method` and one method, it runs that one.
* With no `Start::method` and more than one method, `begin` presents kind `choose` with `data` `{ methods: [names] }`. The answer is `{ method }`.
* With no methods at all, `begin` is `AuthError::Invalid("no sign-in method is configured")`.
* A name it does not hold is `AuthError::Invalid` listing the names.
* An answer to `choose` without `method` is `AuthError::Invalid("choose a method")`.
* Each step's state carries the method's name. `advance` passes the answer to that method only.
* `refresh` asks each method in order and returns the first `Some`.

### `Sequence`

A sign-in that passes one provider and then another for the same subject.

* `pub fn new(first: Arc<dyn IdentityProvider>, second: Arc<dyn IdentityProvider>) -> Self`
* `first` runs to completion, then `second` begins with `Start::identity` set to the identity `first` produced.
* The flow completes when `second` completes for the same subject, with `first`'s identity and both providers' tokens. `second`'s value wins for a key both hold.
* `second` completing for another subject is `AuthError::Denied`.
* No identity is set on the session between the two providers.
* An enrolment runs `second` alone.
* `refresh` is `first.refresh`.

## 4. The Dev Provider

### `DevProvider`

Name and password against a fixed in-memory table. Passwords are compared in the clear. `Default`.

* `pub fn new() -> Self`: an empty table.
* `pub fn user(self, name: impl Into<String>, password: impl Into<String>) -> Self`: appends a user with empty claims.
* `pub fn user_with_claims(self, name: impl Into<String>, password: impl Into<String>, claims: ValueMap) -> Self`: appends a user with `claims`.
* `pub fn from_toml(path: impl AsRef<Path>) -> Result<Self, String>`: reads `[[users]]` rows of `name`, `password` and an optional `claims` table.
  * A TOML datetime in `claims` becomes its string.
  * An unreadable or unparsable file, no row or an empty name is `Err` naming the path and the reason.
* `pub fn stand_in(self) -> Self`: sign-in by name alone, with no password.
* Duplicate names are not rejected. The first match in insertion order wins.
* `begin` answers `password_form()` for a sign-in. With `stand_in` it presents kind `form` with `data` `{ fields: ["user"], users: [names] }`.
* `begin` for an enrolment is `AuthError::Invalid`.
* `advance` ignores `state` and reads `user` and `password` from `params`. Only `Value::Str` is accepted.
* A missing value is `AuthError::Invalid("missing user")` or `AuthError::Invalid("missing password")`.
* No match is `AuthError::Denied("unknown user or wrong password")`. With `stand_in` it is `AuthError::Denied("unknown user")`.
* Success completes with `Identity { subject: <name>, claims: <the user's claims> }` and one token, `access_token` = `dev-token-<name>`.

### `password_form`

* `pub fn password_form() -> Next`: `Next::Present` of kind `form` with `data` `{ fields: ["user", "password"] }` and empty `state`.

## 5. Types From Other Crates

### `Identity`

`snapfire_fsr_runtime::Identity`, documented in [the runtime reference](../runtime/API_REFERENCE.md).

* `claims` is readable by application code and reaches templates in the `identity` prop. Tokens are not.

### `Opened` and `TokenCell`

`snapfire_fsr_session::Opened` and `snapfire_fsr_session::TokenCell`, documented in [the session reference](../session/API_REFERENCE.md).

* `Auth` reads and writes `opened.cell` and `opened.tokens` only.

### `Value` and `ValueMap`

`snapfire_fsr_core::Value` and `snapfire_fsr_core::ValueMap`, documented in [the core reference](../core/API_REFERENCE.md).

* This crate reads only `Value::Str` and `Value::Map` from params, state and tokens. Any other variant in a key it reads is treated as absent.

## 6. Error Handling

### `AuthError`

Returned by `Auth::begin`, `Auth::advance` and every provider method. `Debug`, `Clone`, `PartialEq`, `std::error::Error`. `#[non_exhaustive]`.

* `Denied(String)`: the request is not permitted.
* `Invalid(String)`: the request is malformed or no flow is in progress.
* `pub fn http_status(&self) -> u16`: 403 for `Denied`, 400 for `Invalid`.
* `Display`: `denied: <message>` or `invalid: <message>`.
* Messages are the provider's diagnostic text and are not escaped or filtered.
