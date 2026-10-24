# Usage Guide: snapfire_fsr_auth

How to run a sign-in over an `IdentityProvider` as a sequence of steps, where the flow state lives between them, what the HTTP adapter still owns and how identity reaches a template.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building an Auth Over a Provider](#building-an-auth-over-a-provider)
* [Beginning a Sign-In](#beginning-a-sign-in)
* [Serving the Login Page](#serving-the-login-page)
* [Advancing to the Next Step](#advancing-to-the-next-step)
* [Enrolling Another Method](#enrolling-another-method)
* [Logging Out](#logging-out)
* [Wiring the Flow Endpoints](#wiring-the-flow-endpoints)
  * [The Login Endpoint](#the-login-endpoint)
  * [The Callback Endpoint](#the-callback-endpoint)
  * [The Logout Endpoint](#the-logout-endpoint)
* [Reading Identity in a Template](#reading-identity-in-a-template)
* [Reaching Tokens From the Service Layer](#reaching-tokens-from-the-service-layer)
* [Writing an Identity Provider](#writing-an-identity-provider)
  * [Implementing begin](#implementing-begin)
  * [Implementing advance](#implementing-advance)
  * [Implementing refresh](#implementing-refresh)
* [Configuring the Dev Provider](#configuring-the-dev-provider)
* [Reading the Accounts From a File](#reading-the-accounts-from-a-file)
* [Why Each Answer Is Bound to the Session](#why-each-answer-is-bound-to-the-session)
* [Error Handling](#error-handling)

## Core Concepts

* **`IdentityProvider`** is the seam: `begin` answers the first step, `advance` takes the browser's answer to a step and answers the next, `refresh` renews tokens.
* **`Next`** is a provider's answer, one of three: `Redirect` the browser to a URL, `Present` data for the page to act on or `Complete` with an `AuthOutcome`.
* **`Start`** is what a flow begins from: the `Intent`, the destination, the method the visitor chose and, when enrolling, who is signed in.
* **`Intent`** is `SignIn` or `Enrol`, adding a method such as a passkey to the identity already signed in.
* **`Auth`** is the flow over any provider: `begin` starts it, `advance` takes each answer, `logout` forgets it.
* **`Step`** is what `Auth` hands the adapter after each call: `Redirect`, `Present` or `Complete` with the destination.
* **`AuthOutcome`** is what a completed flow produces: an `Identity` plus the `tokens` the backend tier will need.
* **`Identity`** is who the request is, a `subject` string plus a `claims` map, defined in `snapfire_fsr_runtime`.
* **Flow state** is the step's `state` plus the destination and the intent, held under the reserved key `_sf_auth` in the session's token cell and never shown to the browser.
* **Token custody** is `TokenCell` on `Opened`: server-side storage that never enters `RequestCtx`, so loaders, actions and evaluators cannot read it.
* **Consuming a step** means `advance` removes `_sf_auth` before it calls the provider, so one step buys exactly one answer.
* **`AuthError`** has two variants: `Denied` for a refusal (403) and `Invalid` for a malformed answer or no flow (400).
* **`DevProvider`** checks a name and a password against a fixed table, the second implementation that proves the seam.
* **Auth never renders**: the login page is an ordinary route. What a `Present` step holds is data for it.

## Quick Start

A whole sign-in in one process, no HTTP involved.

```rust
use std::sync::Arc;

use futures::executor::block_on;
use snapfire_fsr_auth::{Auth, DevProvider, Start, Step};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};

fn main() {
  let sessions = Sessions::new(Arc::new(MemorySessionStore::new(128)), b"dev-signing-key-32-bytes-long!!!", SessionConfig::default());
  let auth = Auth::new(Arc::new(DevProvider::new().user("alice", "wonder")));

  let opened = block_on(sessions.open(None));

  let step = block_on(auth.begin(&opened, Start::sign_in("/dash/servers"))).unwrap();
  assert!(matches!(step, Step::Present { ref kind, .. } if kind == "form"));

  let mut params = ValueMap::new();
  params.insert("user".to_owned(), Value::str("alice"));
  params.insert("password".to_owned(), Value::str("wonder"));

  let step = block_on(auth.advance(&opened, params)).unwrap();
  assert_eq!(step, Step::Complete { to: "/dash/servers".to_owned(), signed_in: true });
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
  assert_eq!(opened.tokens.get("access_token"), Some(Value::str("dev-token-alice")));
}
```

## Building an Auth Over a Provider

`Auth::new` takes the provider behind an `Arc`, so one `Auth` serves every request.

```rust
use std::sync::Arc;
use snapfire_fsr_auth::{Auth, DevProvider};

let auth = Auth::new(Arc::new(DevProvider::new().user("alice", "wonder").user("bob", "builder")));
```

The same call takes any other implementation of the trait, since the parameter is `Arc<dyn IdentityProvider>`.

```rust
let auth = Auth::new(Arc::new(my_provider));
```

## Beginning a Sign-In

`begin` drops any flow in progress, asks the provider for the first step and keeps that step's state in custody. It never writes to the session cell, so a visitor who starts signing in is still anonymous.

```rust
let step = auth.begin(&opened, Start::sign_in("/dash/servers").method(Some("google".to_owned()))).await?;
```

The adapter turns the step into a response. A `Redirect` is a 303 to the identity provider. A `Present` is either data for a page that asked for JSON or a 303 to the login page, which renders the form. A `Complete` lands on its destination.

```rust
match step {
  Step::Redirect(to) => see_other(&to),
  Step::Present { .. } => see_other(&format!("/login?return_to={}", encode(&return_to))),
  Step::Complete { to, .. } => see_other(&to),
}
```

## Serving the Login Page

The login page is a route like any other. A password provider presents a `form` step whose `data` names its fields, so the page is an ordinary form posting to the callback. Nothing in it comes from this crate.

```html
<form method="post" action="/auth/callback">
  <input name="user" placeholder="user">
  <input type="password" name="password" placeholder="password">
  <button>sign in</button>
</form>
```

`ensure_flow` begins a sign-in when none is in progress, so a login page reached by a typed URL still has a flow to answer:

```rust
auth.ensure_flow(&opened, "/dash/servers").await;
```

## Advancing to the Next Step

`advance` takes the browser's answer (the callback's query, form or JSON body) and asks the provider for the next step. It runs as many steps as the provider asks for: a redirect to an identity provider, then a second-factor form, then completion.

```rust
let step = auth.advance(&opened, params).await?;
```

On `Complete` the identity is set on `opened.cell` while the tokens are merged into `opened.tokens`. `signed_in` tells the adapter to give the session a new id.

```rust
assert_eq!(opened.cell.identity().unwrap().subject, "alice");
assert_eq!(opened.cell.get("access_token"), None, "tokens stay in custody");
```

Because a step is removed before the provider runs, a refused answer takes the flow with it. The browser has to begin again.

```rust
let err = auth.advance(&opened, creds("alice", "nope")).await.unwrap_err();
assert!(matches!(err, AuthError::Denied(_)));

let err = auth.advance(&opened, creds("alice", "wonder")).await.unwrap_err();
assert!(matches!(err, AuthError::Invalid(_)), "a refused step cannot be replayed");
```

## Enrolling Another Method

An enrolment adds a way of signing in, a passkey or a linked account, to whoever is signed in. It runs the same steps; `Auth` refuses it on an anonymous session and refuses an outcome naming another subject.

```rust
let identity = opened.cell.identity().expect("signed in");
let step = auth.begin(&opened, Start::enrol("/settings", identity).method(Some("passkey".to_owned()))).await?;
```

Completion leaves the identity alone and reports `signed_in: false`, so the session keeps its id.

## Logging Out

`logout` is synchronous and does exactly two things: it clears the session cell (data plus identity) and it clears token custody.

```rust
auth.logout(&opened);
assert!(opened.cell.identity().is_none());
assert_eq!(opened.tokens.get("access_token"), None);
```

Deleting the stored record and expiring the cookie belong to the session layer, so the adapter calls `Sessions::destroy` as well.

```rust
auth.logout(opened);
let expire = sessions.destroy(opened).await?;
```

## Wiring the Flow Endpoints

`/auth/login`, `/auth/callback` and `/auth/logout` are HTTP shapes, so they live at the adapter rather than in this crate. The stock host serves them, plus `/auth/enrol`; the paths are yours to choose in your own adapter.

### The Login Endpoint

Work out the destination, begin and answer the step.

```rust
async fn handle_login(req: &Request, auth: &Auth, opened: &Opened) -> Response {
  let return_to = query(req, "return_to").unwrap_or_else(|| "/".to_owned());
  let start = Start::sign_in(return_to).method(query(req, "method"));
  answer(auth.begin(opened, start).await)
}
```

### The Callback Endpoint

Parse the answer into a `ValueMap`, advance and answer the next step. `AuthError::http_status` picks the status for a failure.

```rust
async fn handle_callback(auth: &Auth, opened: &Opened, params: ValueMap) -> Response {
  answer(auth.advance(opened, params).await)
}

fn answer(step: Result<Step, AuthError>) -> Response {
  match step {
    Ok(Step::Redirect(to)) => see_other(&to),
    Ok(Step::Present { kind, data }) => json(&serde_json::json!({ "next": "present", "kind": kind, "data": to_json(data) })),
    Ok(Step::Complete { to, .. }) => see_other(&to),
    Err(e) => status(e.http_status(), e.to_string()),
  }
}
```

### The Logout Endpoint

Logout is a state change, so the adapter verifies CSRF first, then calls `logout` plus `Sessions::destroy` and attaches the expiring cookie.

```rust
async fn handle_logout(sessions: &Sessions, auth: &Auth, opened: &Opened, token: &str) -> Response {
  if !sessions.verify_csrf(opened, token) {
    return status(403, "csrf verification failed".to_owned());
  }
  auth.logout(opened);
  let expire = sessions.destroy(opened).await?;
  see_other_with_cookie("/", &expire)
}
```

## Reading Identity in a Template

Nothing in this crate renders. Identity travels the ordinary route: the adapter puts `opened.cell` into `RequestCtx` and the assembler injects it as the `identity` prop, a map with `subject` plus `claims`, absent when the session is anonymous.

```html
{% if identity is defined %}
  <span class="who">signed in as {{ identity.subject }}</span>
{% else %}
  <a class="login-link" href="/auth/login">sign in</a>
{% endif %}
```

## Reaching Tokens From the Service Layer

The tokens a completed flow merged are readable by the service layer alone. `snapfire_fsr_service` implements its `Credentials` trait for `TokenCell`, so the adapter binds custody when it binds identity.

```rust
let services = services.bind(opened.cell.identity(), Arc::new(opened.tokens.clone()));
```

What application code receives is a `ServiceHandle`, which can call but cannot read.

## Writing an Identity Provider

Implement the trait for your own type. Every method returns `BoxFuture`, so a synchronous provider wraps its result in `ready`.

```rust
use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_auth::{AuthError, AuthOutcome, IdentityProvider, Next, Start};
use snapfire_fsr_core::ValueMap;

pub struct MyProvider {
  authorize_url: String,
}

impl IdentityProvider for MyProvider {
  fn begin(&self, start: Start) -> BoxFuture<'_, Result<Next, AuthError>> { /* ... */ }
  fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> { /* ... */ }
}
```

### Implementing begin

Answer the first step plus whatever has to come back with it. The state stays server-side, in token custody, until the next `advance`.

```rust
fn begin(&self, _start: Start) -> BoxFuture<'_, Result<Next, AuthError>> {
  let nonce = generate_nonce();
  let mut state = ValueMap::new();
  state.insert("nonce".to_owned(), Value::str(nonce.clone()));
  Box::pin(ready(Ok(Next::redirect(format!("{}?state={nonce}", self.authorize_url), state))))
}
```

A passkey provider presents its challenge instead of redirecting:

```rust
Ok(Next::present("webauthn.get", options, state))
```

### Implementing advance

`params` is the browser's answer, `state` is what the last step kept. Compare them here; a mismatch is `Invalid`, a genuine refusal is `Denied`. Another step is just another `Next`.

```rust
fn advance(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<Next, AuthError>> {
  let result = (|| {
    let returned = match params.get("state") {
      Some(Value::Str(s)) => s.to_string(),
      _ => return Err(AuthError::Invalid("missing state".to_owned())),
    };
    if state.get("nonce") != Some(&Value::str(returned)) {
      return Err(AuthError::Invalid("state mismatch".to_owned()));
    }
    let mut tokens = ValueMap::new();
    tokens.insert("access_token".to_owned(), Value::str(exchange(&params)?));
    Ok(Next::Complete(AuthOutcome::new(Identity::new(subject_of(&params), ValueMap::new()), tokens)))
  })();
  Box::pin(ready(result))
}
```

Everything in `tokens` goes to custody and everything in `identity.claims` becomes readable by templates, so put a credential in the first and never in the second.

### Implementing refresh

`refresh` takes the tokens custody holds and answers new ones. It answers `None` when the provider cannot renew them. The default answers `None`.

```rust
fn refresh(&self, tokens: ValueMap) -> BoxFuture<'_, Result<Option<ValueMap>, AuthError>> {
  Box::pin(async move { Ok(Some(self.exchange_refresh_token(&tokens).await?)) })
}
```

## Configuring the Dev Provider

`user` appends to the table and returns `Self`, so the calls chain.

```rust
let provider = DevProvider::new().user("alice", "wonder").user("bob", "builder");
```

For a user whose claims a template or an interceptor needs, use `user_with_claims`.

```rust
let mut claims = ValueMap::new();
claims.insert("role".to_owned(), Value::str("operator"));
let provider = DevProvider::new().user_with_claims("alice", "wonder", claims);
```

It presents `password_form()`, reads `user` plus `password` out of the answer, ignores the state and mints one token per sign-in, `access_token` set to `dev-token-<name>`. It enrols nothing. It is a development convenience: passwords are compared in the clear against an in-memory table.

## Reading the Accounts From a File

`DevProvider::from_toml` builds the same table from a file, which is how the stock host's `file` provider is configured:

```toml
[[users]]
name = "alice"
password = "wonder"
claims = { role = "admin" }
```

```rust
let provider = DevProvider::from_toml("config/auth.toml")?;
```

A file with no row is an error rather than an empty provider: a login page nobody can pass is a misconfiguration.

## Why Each Answer Is Bound to the Session

The flow state rides token custody rather than a hidden form field or a signed URL, which makes the binding automatic. An answer can only read `_sf_auth` from the session cookie the browser sent, so an answer replayed into a different session finds nothing there.

```rust
let opened = sessions.open(None).await;
let err = auth.advance(&opened, creds("alice", "wonder")).await.unwrap_err();
assert!(matches!(err, AuthError::Invalid(_)));
assert_eq!(err.http_status(), 400);
```

Removing the state before the provider runs gives the second property: a step is spent whether its answer succeeds or fails, so a captured answer is worth one try at most. Both are pinned by tests in `tests/auth.rs`.

## Error Handling

`AuthError` is the crate's only error type, returned by `Auth::begin`, `Auth::advance` and every provider method. `Denied` means the visitor is not permitted; `Invalid` means the answer was malformed or there was no flow to answer.

```rust
match auth.advance(&opened, params).await {
  Ok(step) => answer(step),
  Err(e) => status(e.http_status(), e.to_string()),
}
```

`http_status` gives 403 for `Denied` plus 400 for `Invalid`. `Display` renders `denied: <message>` or `invalid: <message>`; the message is meant for a log or a developer, not for a sign-in page. `DevProvider` returns `unknown user or wrong password` for both a bad name and a bad password, which is the level of detail an end user should see.
