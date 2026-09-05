# Usage Guide: snapfire_fsr_auth

How to run a login flow over an `IdentityProvider`, where the flow state lives while the browser is away, what the HTTP adapter still owns and how identity reaches a template.

## Table of Contents

* [Core Concepts](#core-concepts)
* [Quick Start](#quick-start)
* [Building an Auth Over a Provider](#building-an-auth-over-a-provider)
* [Starting the Login Flow](#starting-the-login-flow)
* [Serving the Login Page](#serving-the-login-page)
* [Finishing the Flow at the Callback](#finishing-the-flow-at-the-callback)
* [Logging Out](#logging-out)
* [Wiring the Flow Endpoints](#wiring-the-flow-endpoints)
  * [The Login Endpoint](#the-login-endpoint)
  * [The Callback Endpoint](#the-callback-endpoint)
  * [The Logout Endpoint](#the-logout-endpoint)
* [Reading Identity in a Template](#reading-identity-in-a-template)
* [Reaching Tokens From the Service Layer](#reaching-tokens-from-the-service-layer)
* [Writing an Identity Provider](#writing-an-identity-provider)
  * [Implementing begin](#implementing-begin)
  * [Implementing callback](#implementing-callback)
* [Configuring the Dev Provider](#configuring-the-dev-provider)
* [Reading the Accounts From a File](#reading-the-accounts-from-a-file)
* [Why the Callback Is Bound to the Session](#why-the-callback-is-bound-to-the-session)
* [Error Handling](#error-handling)

## Core Concepts

* **`IdentityProvider`** is the seam: `begin` produces the redirect that starts the flow, `callback` consumes the provider's response.
* **`Auth`** is the flow over any provider: `login` starts it, `callback` finishes it, `logout` forgets it.
* **`Begin`** is what `begin` returns: the `redirect` the browser is sent to plus the `state` that has to survive the round trip.
* **`AuthOutcome`** is what `callback` returns on success: an `Identity` plus the `tokens` the backend tier will need.
* **`Identity`** is who the request is, a `subject` string plus a `claims` map, defined in `snapfire_fsr_runtime`.
* **Flow state** is the provider's `state` plus the original destination, held under the reserved key `_sf_auth` in the session's token cell.
* **Token custody** is `TokenCell` on `Opened`: server-side storage that never enters `RequestCtx`, so loaders, actions and evaluators cannot read it.
* **`Opened`** is one request's session as the session layer sees it, carrying `id`, `cell`, `tokens` and `fresh`.
* **`SessionCell`** is the part of the session application code does see; identity lives here, tokens never do.
* **`return_to`** is the path the browser lands on after a successful callback, carried through the flow state rather than through the URL.
* **Consuming the flow** means `callback` removes `_sf_auth` before it calls the provider, so one `login` buys exactly one attempt.
* **`AuthError`** has two variants: `Denied` for a rejected identity (403) and `Invalid` for a malformed or absent flow (400).
* **`DevProvider`** checks a name and a password against a fixed table, the second implementation that proves the seam.
* **Auth never renders**: the login page is an ordinary route through the ordinary plan, reached by the redirect `begin` returns.
* **The flow endpoints are the adapter's**: `/auth/login`, `/auth/callback` and `/auth/logout` are HTTP shapes this crate does not define.

## Quick Start

A whole round trip in one process, no HTTP involved.

```rust
use std::sync::Arc;
use std::time::Duration;

use futures::executor::block_on;
use snapfire_fsr_auth::{Auth, DevProvider};
use snapfire_fsr_core::{Value, ValueMap};
use snapfire_fsr_session::{MemorySessionStore, SessionConfig, Sessions};

fn main() {
  let sessions = Sessions::new(
    Arc::new(MemorySessionStore::new(128, Duration::from_secs(60))),
    b"dev-signing-key-32-bytes-long!!!",
    SessionConfig::default(),
  );
  let auth = Auth::new(Arc::new(DevProvider::new("/login").user("alice", "wonder")));

  let opened = block_on(sessions.open(None));

  let redirect = block_on(auth.login(&opened, "/dash/servers"));
  assert_eq!(redirect, "/login?return_to=%2Fdash%2Fservers");

  let mut params = ValueMap::new();
  params.insert("user".to_owned(), Value::Str("alice".to_owned()));
  params.insert("password".to_owned(), Value::Str("wonder".to_owned()));

  let destination = block_on(auth.callback(&opened, params)).unwrap();
  assert_eq!(destination, "/dash/servers");
  assert_eq!(opened.cell.identity().unwrap().subject, "alice");
  assert_eq!(opened.tokens.get("access_token"), Some(Value::Str("dev-token-alice".into())));
}
```

## Building an Auth Over a Provider

`Auth::new` takes the provider behind an `Arc`, so one `Auth` serves every request.

```rust
use std::sync::Arc;
use snapfire_fsr_auth::{Auth, DevProvider};

let auth = Auth::new(Arc::new(
  DevProvider::new("/login").user("alice", "wonder").user("bob", "builder"),
));
```

The same call takes any other implementation of the trait, since the parameter is `Arc<dyn IdentityProvider>`.

```rust
let auth = Auth::new(Arc::new(my_provider));
```

Keep it beside the session layer on your application state; the example holds both on one `AppCore`.

```rust
pub struct AppCore {
  pub(crate) sessions: Sessions,
  pub(crate) auth: Auth,
  // ...
}
```

## Starting the Login Flow

`login` calls the provider's `begin`, stores the returned state together with the destination, then hands back the URL to redirect to. It never writes to the session cell, so an anonymous visitor who starts a login is still anonymous.

```rust
let redirect = auth.login(&opened, "/dash/servers").await;
```

With `DevProvider::new("/login")` that string is `/login?return_to=%2Fdash%2Fservers`. Turn it into a 303 at the adapter.

```rust
HttpResponse::SeeOther()
  .insert_header((header::LOCATION, redirect))
  .finish()
```

The write lands in token custody under `_sf_auth`, which marks the token cell dirty, so `Sessions::persist` will save the record when the response starts.

## Serving the Login Page

`begin` redirects to an application-owned path, so the login page is a route like any other: a matcher entry, a plan node, a template.

```rust
pub const LOGIN: EntryId = EntryId(2);

matcher.insert("/login", LOGIN).expect("route pattern");
resolver.insert(LOGIN, login_plan());

fn login_plan() -> PlanNode {
  layout_over(PlanNode::new(NodeId(3), ModuleId::new("login.tera", "default")))
}
```

The template is an ordinary form whose action is the callback endpoint. Nothing in it comes from this crate.

```html
<form method="post" action="/auth/callback">
  <input name="user" placeholder="user">
  <input type="password" name="password" placeholder="password">
  <button>sign in</button>
</form>
```

## Finishing the Flow at the Callback

`callback` takes the provider's response as a `ValueMap` and returns the destination to redirect to.

```rust
let destination = auth.callback(&opened, params).await?;
```

Four things happen inside, in order: the flow state is removed from token custody, `return_to` is read out of it, the provider is asked to turn `params` plus `state` into an `AuthOutcome`, then the identity is set on `opened.cell` while the tokens are merged into `opened.tokens`.

```rust
assert_eq!(opened.cell.identity().unwrap().subject, "alice");
assert_eq!(opened.tokens.get("access_token"), Some(Value::Str("dev-token-alice".into())));
assert_eq!(opened.cell.get("access_token"), None, "tokens stay in custody");
```

Because the state is removed before the provider runs, a rejected attempt takes the flow with it. The browser has to go back through `login`.

```rust
let err = auth.callback(&opened, creds("alice", "nope")).await.unwrap_err();
assert!(matches!(err, AuthError::Denied(_)));

let err = auth.callback(&opened, creds("alice", "wonder")).await.unwrap_err();
assert!(matches!(err, AuthError::Invalid(_)), "a failed flow cannot be replayed");
```

If the state map holds no `return_to` string, the destination is `/`.

## Logging Out

`logout` is synchronous and does exactly two things: it clears the session cell (data plus identity) and it clears token custody.

```rust
auth.logout(&opened);
assert!(opened.cell.identity().is_none());
assert_eq!(opened.tokens.get("access_token"), None);
assert!(opened.cell.is_dirty(), "logout persists as a write");
```

That is the whole of what this crate does on the way out. Deleting the stored record and expiring the cookie belong to the session layer, so the adapter calls `Sessions::destroy` as well.

```rust
auth.logout(opened);
let expire = app.sessions.destroy(opened).await;
```

## Wiring the Flow Endpoints

`/auth/login`, `/auth/callback` and `/auth/logout` are HTTP shapes, so they live at the adapter rather than in this crate. What follows is the wiring the `advanced_tera_app` example uses; the paths are yours to choose as long as the login page and the form agree with them.

### The Login Endpoint

Work out the destination, then redirect to whatever `login` returns.

```rust
async fn handle_login(req: &HttpRequest, app: &AppCore, opened: &Opened) -> HttpResponse {
  let return_to: String = form_urlencoded::parse(req.query_string().as_bytes())
    .find_map(|(k, v)| (k == "return_to").then(|| v.into_owned()))
    .or_else(|| {
      req.headers().get(header::REFERER).and_then(|v| v.to_str().ok()).map(str::to_owned)
    })
    .unwrap_or_else(|| "/".to_owned());
  let redirect = app.auth.login(opened, &return_to).await;
  see_other(&redirect)
}
```

### The Callback Endpoint

Parse the provider's response into a `ValueMap`, then let `AuthError::http_status` pick the status for a failure.

```rust
async fn handle_callback(app: &AppCore, opened: &Opened, body: Bytes) -> HttpResponse {
  match app.auth.callback(opened, form_params(&body)).await {
    Ok(destination) => see_other(&destination),
    Err(e) => HttpResponse::build(StatusCode::from_u16(e.http_status()).unwrap())
      .body(e.to_string()),
  }
}
```

`form_params` is the adapter's, not this crate's; anything that produces a `ValueMap` of `Value::Str` works, including a query string on a GET callback.

```rust
fn form_params(body: &Bytes) -> ValueMap {
  let mut map = ValueMap::new();
  for (k, v) in form_urlencoded::parse(body) {
    map.insert(k.into_owned(), Value::Str(v.into_owned()));
  }
  map
}
```

### The Logout Endpoint

Logout is a state change, so the adapter verifies CSRF first, then calls `logout` plus `Sessions::destroy` and attaches the expiring cookie.

```rust
async fn handle_logout(app: &AppCore, opened: &Opened, body: Bytes) -> HttpResponse {
  let mut params = form_params(&body);
  let token = match params.shift_remove("_csrf") {
    Some(Value::Str(token)) => token,
    _ => String::new(),
  };
  if !app.sessions.verify_csrf(&opened.id, &token) {
    return HttpResponse::Forbidden().body("csrf verification failed");
  }
  app.auth.logout(opened);
  let expire = app.sessions.destroy(opened).await;
  let mut response = see_other("/");
  if let Ok(value) = header::HeaderValue::from_str(&expire) {
    response.headers_mut().append(header::SET_COOKIE, value);
  }
  response
}
```

Because `destroy` already deleted the record, this route is handled before the normal `persist` path rather than through it.

```rust
if req.path() == "/auth/logout" && req.method() == Method::POST {
  return handle_logout(&app, &opened, body).await;
}
```

## Reading Identity in a Template

Nothing in this crate renders. Identity travels the ordinary route: the adapter puts `opened.cell` into `RequestCtx`, the assembler injects it as the `identity` prop on every node.

```rust
let ctx = RequestCtx {
  params: matched.params,
  session: opened.cell.clone(),
  csrf: Some(app.sessions.csrf_token(&opened.id)),
  services,
};
```

The prop is a map with `subject` plus `claims`, absent entirely when the session is anonymous, so a template branches on whether it is defined.

```html
{% if identity is defined %}
  <span class="who">signed in as {{ identity.subject }}</span>
  <form class="logout" method="post" action="/auth/logout">
    <input type="hidden" name="_csrf" value="{{ csrf_token | default(value="") }}">
    <button>logout</button>
  </form>
{% else %}
  <a class="login-link" href="/auth/login">login</a>
{% endif %}
```

## Reaching Tokens From the Service Layer

The tokens `callback` merged are readable by the service layer alone. `snapfire_fsr_service` implements its `Credentials` trait for `TokenCell`, so the adapter binds custody when it binds identity.

```rust
let services = app.services.bind(opened.cell.identity(), Arc::new(opened.tokens.clone()));
```

What application code receives is a `ServiceHandle`, which can call but cannot read, which is why the assertion in the callback chapter holds: `opened.cell.get("access_token")` is `None` no matter what the provider returned.

## Writing an Identity Provider

Implement the trait for your own type. Both methods return `BoxFuture`, so a synchronous provider wraps its result in `ready`.

```rust
use futures_util::future::{ready, BoxFuture};
use snapfire_fsr_auth::{AuthError, AuthOutcome, Begin, IdentityProvider};
use snapfire_fsr_core::ValueMap;

pub struct MyProvider {
  authorize_url: String,
}

impl IdentityProvider for MyProvider {
  fn begin(&self, return_to: &str) -> BoxFuture<'_, Begin> { /* ... */ }
  fn callback(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>> { /* ... */ }
}
```

### Implementing begin

Return where the browser goes plus whatever has to come back with it. The state stays server-side, in token custody, for the whole round trip.

```rust
fn begin(&self, _return_to: &str) -> BoxFuture<'_, Begin> {
  let nonce = generate_nonce();
  let redirect = format!("{}?state={}", self.authorize_url, nonce);
  let mut state = ValueMap::new();
  state.insert("nonce".to_owned(), Value::Str(nonce));
  Box::pin(ready(Begin { redirect, state }))
}
```

`Auth::login` adds `return_to` to that map after `begin` returns, so a provider must not put its own meaning on that key.

### Implementing callback

`params` is the provider's response, `state` is what `begin` stashed. Compare them here; a mismatch is `Invalid`, a genuine refusal is `Denied`.

```rust
fn callback(&self, params: ValueMap, state: ValueMap) -> BoxFuture<'_, Result<AuthOutcome, AuthError>> {
  let result = (|| {
    let returned = match params.get("state") {
      Some(Value::Str(s)) => s.as_str(),
      _ => return Err(AuthError::Invalid("missing state".to_owned())),
    };
    match state.get("nonce") {
      Some(Value::Str(nonce)) if nonce == returned => {}
      _ => return Err(AuthError::Invalid("state mismatch".to_owned())),
    }

    let mut tokens = ValueMap::new();
    tokens.insert("access_token".to_owned(), Value::Str(exchange(&params)?));
    Ok(AuthOutcome {
      identity: Identity { subject: subject_of(&params), claims: ValueMap::new() },
      tokens,
    })
  })();
  Box::pin(ready(result))
}
```

Everything in `tokens` goes to custody and everything in `identity.claims` becomes readable by templates, so put a credential in the first and never in the second.

## Configuring the Dev Provider

`DevProvider::new` takes the path of your login page; `user` appends to the table and returns `Self`, so the calls chain.

```rust
let provider = DevProvider::new("/login")
  .user("alice", "wonder")
  .user("bob", "builder");
```

For a user whose claims a template or an interceptor needs, use `user_with_claims`.

```rust
let mut claims = ValueMap::new();
claims.insert("role".to_owned(), Value::Str("operator".to_owned()));

let provider = DevProvider::new("/login").user_with_claims("alice", "wonder", claims);
```

It reads `user` plus `password` out of the callback params, ignores the flow state entirely and mints one token per successful login, `access_token` set to `dev-token-<name>`. It is a development convenience: passwords are compared in the clear against an in-memory table.

## Reading the Accounts From a File

`DevProvider::from_toml` builds the same table from a file, which is how the stock host's `file` provider is configured:

```toml
[[users]]
name = "alice"
password = "wonder"
claims = { role = "admin" }

[[users]]
name = "bob"
password = "builder"
```

```rust
let provider = DevProvider::from_toml("/login", "config/auth.toml")?;
```

A file with no row is an error rather than an empty provider: a login page nobody can pass is a misconfiguration.

## Why the Callback Is Bound to the Session

The flow state rides token custody rather than a hidden form field or a signed URL, which makes the binding automatic. The callback can only read `_sf_auth` from the session cookie the browser sent, so a callback replayed into a different session finds nothing there.

```rust
let opened = sessions.open(None).await;
let err = auth.callback(&opened, creds("alice", "wonder")).await.unwrap_err();
assert!(matches!(err, AuthError::Invalid(_)));
assert_eq!(err.http_status(), 400);
```

Removing the state before the provider runs gives the second property: an attempt is spent whether it succeeds or fails, so a captured callback body is worth one try at most. Both are pinned by tests in `tests/auth.rs`.

## Error Handling

`AuthError` is the crate's only error type; only `Auth::callback` returns one. `Denied` means the provider identified the request as not permitted; `Invalid` means the request was malformed or there was no flow to finish.

```rust
match auth.callback(&opened, params).await {
  Ok(destination) => redirect_to(&destination),
  Err(e @ AuthError::Denied(_)) => render_status(e.http_status(), &e.to_string()),
  Err(e @ AuthError::Invalid(_)) => render_status(e.http_status(), &e.to_string()),
}
```

`http_status` gives 403 for `Denied` plus 400 for `Invalid`, so an adapter that does not need to distinguish them can hand the status straight to its response builder.

```rust
Err(e) => HttpResponse::build(StatusCode::from_u16(e.http_status()).unwrap()).body(e.to_string()),
```

`Display` renders `denied: <message>` or `invalid: <message>`. The message is meant for a log or a developer, not for a sign-in page: `DevProvider` returns `unknown user or wrong password` for both a bad name and a bad password, which is the level of detail an end user should see.
